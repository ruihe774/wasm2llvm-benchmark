//! LZ77 + canonical Huffman compressor/decompressor round trip (alloc-heavy).
#![no_std]
extern crate alloc;

#[path = "../../common/common.rs"]
mod common;
#[path = "../../common/heap.rs"]
mod heap_impl;
use common::*;

#[global_allocator]
static A: heap_impl::heap::Bump = heap_impl::heap::Bump::new();

use alloc::vec;
use alloc::vec::Vec;

struct BitWriter { out: Vec<u8>, acc: u64, n: u32 }
impl BitWriter {
    fn new() -> Self { BitWriter { out: Vec::new(), acc: 0, n: 0 } }
    fn put(&mut self, bits: u32, len: u32) {
        self.acc |= (bits as u64) << self.n;
        self.n += len;
        while self.n >= 8 { self.out.push(self.acc as u8); self.acc >>= 8; self.n -= 8; }
    }
    fn finish(mut self) -> Vec<u8> { if self.n > 0 { self.out.push(self.acc as u8); } self.out }
}
struct BitReader<'a> { d: &'a [u8], pos: usize, acc: u64, n: u32 }
impl<'a> BitReader<'a> {
    fn get(&mut self, len: u32) -> u32 {
        while self.n < len {
            let b = if self.pos < self.d.len() { self.d[self.pos] } else { 0 };
            self.pos += 1;
            self.acc |= (b as u64) << self.n;
            self.n += 8;
        }
        let v = (self.acc & ((1u64 << len) - 1)) as u32;
        self.acc >>= len;
        self.n -= len;
        v
    }
}

// symbols: 0..=255 literal, 256 end, 257.. match (length code), distances coded raw 16 bits
const NSYM: usize = 256 + 1 + 64;

fn lz77(data: &[u8]) -> Vec<(u16, u16)> {
    // (sym, dist): sym<256 literal; sym>=257 => length = sym-257+3
    let mut out = Vec::new();
    let mut head = vec![u32::MAX; 1 << 14];
    let mut prev = vec![u32::MAX; data.len()];
    let mut i = 0;
    while i < data.len() {
        let mut best_len = 0;
        let mut best_dist = 0;
        if i + 3 <= data.len() {
            let h = ((data[i] as usize) << 6 ^ (data[i + 1] as usize) << 3 ^ data[i + 2] as usize) & ((1 << 14) - 1);
            let mut cand = head[h];
            let mut chain = 0;
            while cand != u32::MAX && chain < 32 {
                let c = cand as usize;
                if i - c > 65535 { break; }
                let mut l = 0;
                while l < 66 && i + l < data.len() && data[c + l] == data[i + l] { l += 1; }
                if l > best_len { best_len = l; best_dist = i - c; }
                cand = prev[c];
                chain += 1;
            }
            prev[i] = head[h];
            head[h] = i as u32;
        }
        if best_len >= 3 {
            out.push(((257 + best_len - 3) as u16, best_dist as u16));
            for k in 1..best_len {
                let j = i + k;
                if j + 3 <= data.len() {
                    let h = ((data[j] as usize) << 6 ^ (data[j + 1] as usize) << 3 ^ data[j + 2] as usize) & ((1 << 14) - 1);
                    prev[j] = head[h];
                    head[h] = j as u32;
                }
            }
            i += best_len;
        } else {
            out.push((data[i] as u16, 0));
            i += 1;
        }
    }
    out.push((256, 0));
    out
}

fn code_lengths(freq: &[u32]) -> Vec<u8> {
    // Package-merge-free approach: build Huffman tree with a simple O(n^2) min selection, then cap at 15.
    let n = freq.len();
    let mut weight: Vec<u64> = freq.iter().map(|&f| f as u64).collect();
    let mut parent = vec![usize::MAX; 2 * n];
    let mut alive: Vec<usize> = (0..n).filter(|&i| freq[i] > 0).collect();
    let mut lens = vec![0u8; n];
    if alive.len() == 1 { lens[alive[0]] = 1; return lens; }
    let mut next = n;
    weight.resize(2 * n, 0);
    while alive.len() > 1 {
        alive.sort_by(|a, b| weight[*b].cmp(&weight[*a]).then(b.cmp(a)));
        let x = alive.pop().unwrap();
        let y = alive.pop().unwrap();
        weight[next] = weight[x] + weight[y];
        parent[x] = next;
        parent[y] = next;
        alive.push(next);
        next += 1;
    }
    for i in 0..n {
        if freq[i] == 0 { continue; }
        let mut d = 0; let mut p = i;
        while parent[p] != usize::MAX { p = parent[p]; d += 1; }
        lens[i] = d.min(15) as u8;
    }
    // fix Kraft inequality after capping
    loop {
        let k: u64 = lens.iter().filter(|&&l| l > 0).map(|&l| 1u64 << (15 - l)).sum();
        if k <= 1 << 15 { break; }
        let i = (0..n).filter(|&i| lens[i] > 0 && lens[i] < 15).max_by_key(|&i| (lens[i], i)).unwrap();
        lens[i] += 1;
    }
    lens
}

fn canonical(lens: &[u8]) -> Vec<u32> {
    let mut codes = vec![0u32; lens.len()];
    let mut code = 0u32;
    for l in 1..=15u8 {
        for (i, &li) in lens.iter().enumerate() {
            if li == l { codes[i] = code.reverse_bits() >> (32 - l as u32); code += 1; }
        }
        code <<= 1;
    }
    codes
}

fn compress(data: &[u8]) -> Vec<u8> {
    let toks = lz77(data);
    let mut freq = vec![0u32; NSYM];
    for (s, _) in &toks { freq[*s as usize] += 1; }
    let lens = code_lengths(&freq);
    let codes = canonical(&lens);
    let mut w = BitWriter::new();
    for &l in &lens { w.put(l as u32, 4); }
    for (s, d) in &toks {
        w.put(codes[*s as usize], lens[*s as usize] as u32);
        if *s >= 257 { w.put(*d as u32, 16); }
    }
    w.finish()
}

fn decompress(c: &[u8]) -> Vec<u8> {
    let mut r = BitReader { d: c, pos: 0, acc: 0, n: 0 };
    let lens: Vec<u8> = (0..NSYM).map(|_| r.get(4) as u8).collect();
    let codes = canonical(&lens);
    // decode table: (code, len) -> sym, via 15-bit lookup
    let mut table = vec![(0u16, 0u8); 1 << 15];
    for s in 0..NSYM {
        let l = lens[s];
        if l == 0 { continue; }
        let step = 1usize << l;
        let mut k = codes[s] as usize;
        while k < (1 << 15) { table[k] = (s as u16, l); k += step; }
    }
    let mut out = Vec::new();
    loop {
        // peek 15 bits
        while r.n < 15 {
            let b = if r.pos < r.d.len() { r.d[r.pos] } else { 0 };
            r.pos += 1;
            r.acc |= (b as u64) << r.n;
            r.n += 8;
        }
        let (s, l) = table[(r.acc & 0x7fff) as usize];
        r.acc >>= l; r.n -= l as u32;
        if s == 256 { break; }
        if s < 256 { out.push(s as u8); } else {
            let len = s as usize - 257 + 3;
            let d = r.get(16) as usize;
            let start = out.len() - d;
            for k in 0..len { let b = out[start + k]; out.push(b); }
        }
    }
    out
}

fn gen_text(rng: &mut Rng, n: usize) -> Vec<u8> {
    const WORDS: [&str; 24] = ["the", "quick", "brown", "fox", "jumps", "over", "lazy", "dog", "wasm", "llvm",
        "module", "function", "memory", "table", "global", "export", "import", "value", "stack", "lane",
        "vector", "branch", "loop", "block"];
    let mut v = Vec::with_capacity(n + 16);
    while v.len() < n {
        let w = WORDS[rng.below(WORDS.len() as u32) as usize];
        v.extend_from_slice(w.as_bytes());
        v.push(if rng.below(10) == 0 { b'\n' } else { b' ' });
        if rng.below(50) == 0 { for _ in 0..rng.below(20) { v.push(rng.next_u32() as u8); } }
    }
    v.truncate(n);
    v
}

#[no_mangle]
pub extern "C" fn run(n: i32, rounds: i32) -> i64 {
    puts("compress\n");
    let mut rng = Rng::new(n as u64);
    let mut h = FNV0;
    for _ in 0..rounds {
        let data = gen_text(&mut rng, n as usize);
        let c = compress(&data);
        let d = decompress(&c);
        assert!(d == data);
        h = fnv(h, &c);
        pi32(c.len() as i32);
    }
    pi64(h as i64);
    h as i64
}

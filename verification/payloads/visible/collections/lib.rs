//! alloc-based code: Vec, sorting, BTreeMap, String/core::fmt, memory.grow, memory.copy/fill.
#![no_std]
extern crate alloc;

#[path = "../../common/common.rs"]
mod common;
#[path = "../../common/heap.rs"]
mod heap_impl;
use common::*;

#[global_allocator]
static A: heap_impl::heap::Bump = heap_impl::heap::Bump::new();

use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

#[no_mangle]
pub extern "C" fn run(n: i32) -> i64 {
    puts("collections\n");
    let mut rng = Rng::new(n as u64);
    let n = n as usize;

    // sorting
    let mut v: Vec<u32> = (0..n).map(|_| rng.next_u32()).collect();
    let mut w = v.clone();
    v.sort_unstable();
    w.sort();
    assert!(v == w);
    let mut h = FNV0;
    for x in &v { h = fnv(h, &x.to_le_bytes()); }
    pi64(h as i64);

    // sort strings
    let mut words: Vec<String> = Vec::new();
    for i in 0..n / 4 {
        let mut s = String::new();
        let len = 1 + rng.below(12);
        for _ in 0..len { s.push((b'a' + rng.below(26) as u8) as char); }
        let _ = write!(s, "{}", i % 7);
        words.push(s);
    }
    words.sort();
    words.dedup();
    let mut joined = String::new();
    for wd in words.iter().take(20) { joined.push_str(wd); joined.push(' '); }
    outln!("{} words; first: {}", words.len(), joined);

    // BTreeMap word frequencies
    let mut freq: BTreeMap<u32, u32> = BTreeMap::new();
    for _ in 0..n { *freq.entry(rng.below(500)).or_insert(0) += 1; }
    let top = freq.iter().max_by_key(|(k, c)| (**c, **k)).unwrap();
    outln!("distinct={} top={:?}", freq.len(), top);
    let set: BTreeSet<u64> = v.iter().map(|x| (*x as u64) % 1000).collect();
    outln!("set len {} range sum {}", set.len(), set.range(100..200).sum::<u64>());

    // VecDeque BFS on an implicit grid graph
    let side = 64usize;
    let mut dist = alloc::vec![u32::MAX; side * side];
    let mut walls = alloc::vec![false; side * side];
    for c in walls.iter_mut() { *c = rng.below(100) < 28; }
    walls[0] = false;
    let mut q = VecDeque::new();
    dist[0] = 0;
    q.push_back(0usize);
    while let Some(p) = q.pop_front() {
        let (x, y) = (p % side, p / side);
        let nb = [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)];
        for (nx, ny) in nb {
            if nx < side && ny < side {
                let np = ny * side + nx;
                if !walls[np] && dist[np] == u32::MAX {
                    dist[np] = dist[p] + 1;
                    q.push_back(np);
                }
            }
        }
    }
    let reach = dist.iter().filter(|d| **d != u32::MAX).count();
    let far = dist.iter().filter(|d| **d != u32::MAX).max().unwrap();
    outln!("reachable {} farthest {}", reach, far);

    // big buffers: memory.copy / memory.fill paths, growth
    let mut big: Vec<u8> = alloc::vec![0xAB; 300_000];
    big[1000..200_000].copy_within(0..199_000, 0);
    for (i, b) in big.iter_mut().enumerate().step_by(97) { *b = i as u8; }
    let mut big2 = big.clone();
    big2.rotate_left(12345);
    big2[5..100_005].fill(7);
    h = fnv(h, &big2[..]);
    pi64(h as i64);
    pi32(core::arch::wasm32::memory_size(0) as i32);

    // formatting of numbers (core::fmt integer and float paths)
    let mut s = String::new();
    for i in 0..30 {
        let x = rng.next_u64();
        let f = (x >> 11) as f64 / (1u64 << 40) as f64 - 1000.0;
        let _ = write!(s, "{} {:x} {:o} {:+} {:.3} {:e} {:?}|", x as i64, x, x as u16, (x as i32) / 7, f, f as f32, f);
        let _ = i;
    }
    h = fnv(h, s.as_bytes());
    outln!("{}", &s[..200]);
    pi64(h as i64);
    h as i64
}

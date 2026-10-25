//! Hash functions: SHA-256, CRC32 (table), Adler-32, xxhash-like 64-bit mixing, SipHash-1-3.
#![no_std]

#[path = "../../common/common.rs"]
mod common;
use common::*;

const K256: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let bitlen = (data.len() as u64) * 8;
    let mut block = [0u8; 64];
    let nblocks = (data.len() + 9 + 63) / 64;
    for bi in 0..nblocks {
        for i in 0..64 {
            let idx = bi * 64 + i;
            block[i] = if idx < data.len() {
                data[idx]
            } else if idx == data.len() {
                0x80
            } else if idx >= nblocks * 64 - 8 {
                (bitlen >> (8 * (nblocks * 64 - 1 - idx))) as u8
            } else {
                0
            };
        }
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([block[4 * i], block[4 * i + 1], block[4 * i + 2], block[4 * i + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut s = h;
        for i in 0..64 {
            let s1 = s[4].rotate_right(6) ^ s[4].rotate_right(11) ^ s[4].rotate_right(25);
            let ch = (s[4] & s[5]) ^ (!s[4] & s[6]);
            let t1 = s[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K256[i]).wrapping_add(w[i]);
            let s0 = s[0].rotate_right(2) ^ s[0].rotate_right(13) ^ s[0].rotate_right(22);
            let maj = (s[0] & s[1]) ^ (s[0] & s[2]) ^ (s[1] & s[2]);
            let t2 = s0.wrapping_add(maj);
            s = [t1.wrapping_add(t2), s[0], s[1], s[2], s[3].wrapping_add(t1), s[4], s[5], s[6]];
        }
        for i in 0..8 { h[i] = h[i].wrapping_add(s[i]); }
    }
    let mut out = [0u8; 32];
    for i in 0..8 { out[4 * i..4 * i + 4].copy_from_slice(&h[i].to_be_bytes()); }
    out
}

static mut CRC_TABLE: [u32; 256] = [0; 256];

fn crc32(data: &[u8]) -> u32 {
    let t = unsafe { &mut *core::ptr::addr_of_mut!(CRC_TABLE) };
    if t[1] == 0 {
        for i in 0..256u32 {
            let mut c = i;
            for _ in 0..8 { c = if c & 1 != 0 { 0xEDB88320 ^ (c >> 1) } else { c >> 1 }; }
            t[i as usize] = c;
        }
    }
    let mut c = !0u32;
    for &b in data { c = t[((c ^ b as u32) & 0xff) as usize] ^ (c >> 8); }
    !c
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data { a = (a + x as u32) % 65521; b = (b + a) % 65521; }
    (b << 16) | a
}

fn sip13(k0: u64, k1: u64, data: &[u8]) -> u64 {
    let mut v = [k0 ^ 0x736f6d6570736575, k1 ^ 0x646f72616e646f6d, k0 ^ 0x6c7967656e657261, k1 ^ 0x7465646279746573];
    fn round(v: &mut [u64; 4]) {
        v[0] = v[0].wrapping_add(v[1]); v[1] = v[1].rotate_left(13); v[1] ^= v[0]; v[0] = v[0].rotate_left(32);
        v[2] = v[2].wrapping_add(v[3]); v[3] = v[3].rotate_left(16); v[3] ^= v[2];
        v[0] = v[0].wrapping_add(v[3]); v[3] = v[3].rotate_left(21); v[3] ^= v[0];
        v[2] = v[2].wrapping_add(v[1]); v[1] = v[1].rotate_left(17); v[1] ^= v[2]; v[2] = v[2].rotate_left(32);
    }
    let mut chunks = data.chunks_exact(8);
    for c in &mut chunks {
        let m = u64::from_le_bytes(c.try_into().unwrap());
        v[3] ^= m; round(&mut v); v[0] ^= m;
    }
    let mut last = (data.len() as u64) << 56;
    for (i, b) in chunks.remainder().iter().enumerate() { last |= (*b as u64) << (8 * i); }
    v[3] ^= last; round(&mut v); v[0] ^= last;
    v[2] ^= 0xff;
    round(&mut v); round(&mut v); round(&mut v);
    v[0] ^ v[1] ^ v[2] ^ v[3]
}

static mut DATA: [u8; 100_000] = [0; 100_000];

#[no_mangle]
pub extern "C" fn run(len: i32) -> i64 {
    puts("hashes\n");
    let data = unsafe { &mut *core::ptr::addr_of_mut!(DATA) };
    let len = (len as usize).min(data.len());
    let mut rng = Rng::new(len as u64);
    for b in data[..len].iter_mut() { *b = rng.next_u32() as u8; }
    let d = sha256(&data[..len]);
    for b in d { puts(match b >> 4 { x @ 0..=9 => ["0","1","2","3","4","5","6","7","8","9"][x as usize], x => ["a","b","c","d","e","f"][(x - 10) as usize] }); puts(match b & 15 { x @ 0..=9 => ["0","1","2","3","4","5","6","7","8","9"][x as usize], x => ["a","b","c","d","e","f"][(x - 10) as usize] }); }
    puts("\n");
    let abc = sha256(b"abc");
    pi32(u32::from_be_bytes([abc[0], abc[1], abc[2], abc[3]]) as i32); // 0xba7816bf
    let c = crc32(&data[..len]);
    pi32(c as i32);
    pi32(crc32(b"123456789") as i32); // 0xcbf43926
    let a = adler32(&data[..len]);
    pi32(a as i32);
    let mut s = 0u64;
    for i in 0..64 { s ^= sip13(i, !i, &data[..(len * i as usize / 64)]); }
    pi64(s as i64);
    (u64::from_le_bytes(d[..8].try_into().unwrap()) ^ c as u64 ^ (a as u64) << 32 ^ s) as i64
}

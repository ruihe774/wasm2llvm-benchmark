//! ChaCha20 (scalar and SIMD), SHA-512 (u64), Poly1305 (u128 arithmetic).
#![no_std]

#[path = "../../common/common.rs"]
mod common;
use common::*;
use core::arch::wasm32::*;

fn qr(s: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    s[a] = s[a].wrapping_add(s[b]); s[d] ^= s[a]; s[d] = s[d].rotate_left(16);
    s[c] = s[c].wrapping_add(s[d]); s[b] ^= s[c]; s[b] = s[b].rotate_left(12);
    s[a] = s[a].wrapping_add(s[b]); s[d] ^= s[a]; s[d] = s[d].rotate_left(8);
    s[c] = s[c].wrapping_add(s[d]); s[b] ^= s[c]; s[b] = s[b].rotate_left(7);
}

fn chacha_block(key: &[u32; 8], ctr: u32, nonce: &[u32; 3]) -> [u32; 16] {
    let init = [0x61707865, 0x3320646e, 0x79622d32, 0x6b206574, key[0], key[1], key[2], key[3], key[4], key[5], key[6],
        key[7], ctr, nonce[0], nonce[1], nonce[2]];
    let mut s = init;
    for _ in 0..10 {
        qr(&mut s, 0, 4, 8, 12); qr(&mut s, 1, 5, 9, 13); qr(&mut s, 2, 6, 10, 14); qr(&mut s, 3, 7, 11, 15);
        qr(&mut s, 0, 5, 10, 15); qr(&mut s, 1, 6, 11, 12); qr(&mut s, 2, 7, 8, 13); qr(&mut s, 3, 4, 9, 14);
    }
    for i in 0..16 { s[i] = s[i].wrapping_add(init[i]); }
    s
}

#[inline(always)]
fn rotl(v: v128, n: u32) -> v128 { v128_or(i32x4_shl(v, n), u32x4_shr(v, 32 - n)) }

/// 4 blocks in parallel, one block per lane.
fn chacha_block4(key: &[u32; 8], ctr: u32, nonce: &[u32; 3]) -> [[u32; 16]; 4] {
    let mut s = [i32x4_splat(0); 16];
    let consts = [0x61707865u32, 0x3320646e, 0x79622d32, 0x6b206574];
    for i in 0..4 { s[i] = u32x4_splat(consts[i]); }
    for i in 0..8 { s[4 + i] = u32x4_splat(key[i]); }
    s[12] = u32x4(ctr, ctr.wrapping_add(1), ctr.wrapping_add(2), ctr.wrapping_add(3));
    for i in 0..3 { s[13 + i] = u32x4_splat(nonce[i]); }
    let init = s;
    macro_rules! q { ($a:expr, $b:expr, $c:expr, $d:expr) => {
        s[$a] = i32x4_add(s[$a], s[$b]); s[$d] = rotl(v128_xor(s[$d], s[$a]), 16);
        s[$c] = i32x4_add(s[$c], s[$d]); s[$b] = rotl(v128_xor(s[$b], s[$c]), 12);
        s[$a] = i32x4_add(s[$a], s[$b]); s[$d] = i8x16_shuffle::<3, 0, 1, 2, 7, 4, 5, 6, 11, 8, 9, 10, 15, 12, 13, 14>(v128_xor(s[$d], s[$a]), s[$d]);
        s[$c] = i32x4_add(s[$c], s[$d]); s[$b] = rotl(v128_xor(s[$b], s[$c]), 7);
    } }
    for _ in 0..10 {
        q!(0, 4, 8, 12); q!(1, 5, 9, 13); q!(2, 6, 10, 14); q!(3, 7, 11, 15);
        q!(0, 5, 10, 15); q!(1, 6, 11, 12); q!(2, 7, 8, 13); q!(3, 4, 9, 14);
    }
    let mut out = [[0u32; 16]; 4];
    for i in 0..16 {
        let v = i32x4_add(s[i], init[i]);
        out[0][i] = u32x4_extract_lane::<0>(v);
        out[1][i] = u32x4_extract_lane::<1>(v);
        out[2][i] = u32x4_extract_lane::<2>(v);
        out[3][i] = u32x4_extract_lane::<3>(v);
    }
    out
}

const K512: [u64; 80] = [
    0x428a2f98d728ae22, 0x7137449123ef65cd, 0xb5c0fbcfec4d3b2f, 0xe9b5dba58189dbbc, 0x3956c25bf348b538, 0x59f111f1b605d019,
    0x923f82a4af194f9b, 0xab1c5ed5da6d8118, 0xd807aa98a3030242, 0x12835b0145706fbe, 0x243185be4ee4b28c, 0x550c7dc3d5ffb4e2,
    0x72be5d74f27b896f, 0x80deb1fe3b1696b1, 0x9bdc06a725c71235, 0xc19bf174cf692694, 0xe49b69c19ef14ad2, 0xefbe4786384f25e3,
    0x0fc19dc68b8cd5b5, 0x240ca1cc77ac9c65, 0x2de92c6f592b0275, 0x4a7484aa6ea6e483, 0x5cb0a9dcbd41fbd4, 0x76f988da831153b5,
    0x983e5152ee66dfab, 0xa831c66d2db43210, 0xb00327c898fb213f, 0xbf597fc7beef0ee4, 0xc6e00bf33da88fc2, 0xd5a79147930aa725,
    0x06ca6351e003826f, 0x142929670a0e6e70, 0x27b70a8546d22ffc, 0x2e1b21385c26c926, 0x4d2c6dfc5ac42aed, 0x53380d139d95b3df,
    0x650a73548baf63de, 0x766a0abb3c77b2a8, 0x81c2c92e47edaee6, 0x92722c851482353b, 0xa2bfe8a14cf10364, 0xa81a664bbc423001,
    0xc24b8b70d0f89791, 0xc76c51a30654be30, 0xd192e819d6ef5218, 0xd69906245565a910, 0xf40e35855771202a, 0x106aa07032bbd1b8,
    0x19a4c116b8d2d0c8, 0x1e376c085141ab53, 0x2748774cdf8eeb99, 0x34b0bcb5e19b48a8, 0x391c0cb3c5c95a63, 0x4ed8aa4ae3418acb,
    0x5b9cca4f7763e373, 0x682e6ff3d6b2b8a3, 0x748f82ee5defb2fc, 0x78a5636f43172f60, 0x84c87814a1f0ab72, 0x8cc702081a6439ec,
    0x90befffa23631e28, 0xa4506cebde82bde9, 0xbef9a3f7b2c67915, 0xc67178f2e372532b, 0xca273eceea26619c, 0xd186b8c721c0c207,
    0xeada7dd6cde0eb1e, 0xf57d4f7fee6ed178, 0x06f067aa72176fba, 0x0a637dc5a2c898a6, 0x113f9804bef90dae, 0x1b710b35131c471b,
    0x28db77f523047d84, 0x32caab7b40c72493, 0x3c9ebe0a15c9bebc, 0x431d67c49c100d4c, 0x4cc5d4becb3e42b6, 0x597f299cfc657e2a,
    0x5fcb6fab3ad6faec, 0x6c44198c4a475817,
];

fn sha512(data: &[u8]) -> [u64; 8] {
    let mut h: [u64; 8] = [0x6a09e667f3bcc908, 0xbb67ae8584caa73b, 0x3c6ef372fe94f82b, 0xa54ff53a5f1d36f1,
        0x510e527fade682d1, 0x9b05688c2b3e6c1f, 0x1f83d9abfb41bd6b, 0x5be0cd19137e2179];
    let total = data.len() + 17;
    let nblocks = (total + 127) / 128;
    let bitlen = (data.len() as u128) * 8;
    for bi in 0..nblocks {
        let mut blk = [0u8; 128];
        for i in 0..128 {
            let idx = bi * 128 + i;
            blk[i] = if idx < data.len() { data[idx] } else if idx == data.len() { 0x80 }
                else if idx >= nblocks * 128 - 16 { (bitlen >> (8 * (nblocks * 128 - 1 - idx))) as u8 } else { 0 };
        }
        let mut w = [0u64; 80];
        for i in 0..16 { w[i] = u64::from_be_bytes(blk[8 * i..8 * i + 8].try_into().unwrap()); }
        for i in 16..80 {
            let s0 = w[i - 15].rotate_right(1) ^ w[i - 15].rotate_right(8) ^ (w[i - 15] >> 7);
            let s1 = w[i - 2].rotate_right(19) ^ w[i - 2].rotate_right(61) ^ (w[i - 2] >> 6);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut s = h;
        for i in 0..80 {
            let s1 = s[4].rotate_right(14) ^ s[4].rotate_right(18) ^ s[4].rotate_right(41);
            let ch = (s[4] & s[5]) ^ (!s[4] & s[6]);
            let t1 = s[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K512[i]).wrapping_add(w[i]);
            let s0 = s[0].rotate_right(28) ^ s[0].rotate_right(34) ^ s[0].rotate_right(39);
            let maj = (s[0] & s[1]) ^ (s[0] & s[2]) ^ (s[1] & s[2]);
            s = [t1.wrapping_add(s0.wrapping_add(maj)), s[0], s[1], s[2], s[3].wrapping_add(t1), s[4], s[5], s[6]];
        }
        for i in 0..8 { h[i] = h[i].wrapping_add(s[i]); }
    }
    h
}

/// Poly1305 with 130-bit arithmetic on u128 limbs (r clamped).
fn poly1305(key: &[u8; 32], msg: &[u8]) -> u128 {
    let r = u128::from_le_bytes(key[..16].try_into().unwrap()) & 0x0ffffffc0ffffffc0ffffffc0fffffff;
    let s = u128::from_le_bytes(key[16..].try_into().unwrap());
    let p: u128 = (1u128 << 127) - 1; // use 2^127-1 field to stay in u128 (not real Poly1305, same op mix)
    let mut acc: u128 = 0;
    for chunk in msg.chunks(15) {
        let mut b = [0u8; 16];
        b[..chunk.len()].copy_from_slice(chunk);
        b[chunk.len()] = 1;
        let n = u128::from_le_bytes(b);
        acc = (acc + n) % p;
        // mulmod via 64-bit halves
        let (ah, al) = (acc >> 64, acc & 0xffff_ffff_ffff_ffff);
        let (rh, rl) = (r >> 64, r & 0xffff_ffff_ffff_ffff);
        let ll = al * rl;
        let lh = al * rh;
        let hl = ah * rl;
        let hh = ah * rh;
        let mid = (lh % p + hl % p) % p;
        let mut t = (ll % p + ((mid << 32) % p << 32) % p) % p;
        let hh2 = ((hh % p) << 1) % p; // 2^128 = 2 mod p
        t = (t + hh2) % p;
        acc = t;
    }
    acc.wrapping_add(s)
}

static mut BUF: [u8; 1 << 16] = [0; 1 << 16];

#[no_mangle]
pub extern "C" fn run(blocks: i32, seed: i32) -> i64 {
    puts("crypto\n");
    let mut rng = Rng::new(seed as u64);
    let key: [u32; 8] = core::array::from_fn(|_| rng.next_u32());
    let nonce = [rng.next_u32(), rng.next_u32(), rng.next_u32()];
    let mut h = FNV0;
    let mut ctr = 0u32;
    for _ in 0..blocks / 4 {
        let quad = chacha_block4(&key, ctr, &nonce);
        for j in 0..4 {
            let s = chacha_block(&key, ctr.wrapping_add(j as u32), &nonce);
            assert!(s == quad[j]);
            for x in s { h = fnv(h, &x.to_le_bytes()); }
        }
        ctr = ctr.wrapping_add(4);
    }
    pi64(h as i64);
    let buf = unsafe { &mut *core::ptr::addr_of_mut!(BUF) };
    for b in buf.iter_mut() { *b = rng.next_u32() as u8; }
    let d = sha512(&buf[..(blocks as usize * 64).min(buf.len())]);
    for x in d { h = fnv(h, &x.to_le_bytes()); }
    let abc = sha512(b"abc");
    pi64(abc[0] as i64); // 0xddaf35a193617aba
    let k: [u8; 32] = core::array::from_fn(|i| (i as u8).wrapping_mul(7));
    let p = poly1305(&k, &buf[..4096]);
    pi64(p as i64);
    pi64((p >> 64) as i64);
    pi64(h as i64);
    (h ^ p as u64) as i64
}

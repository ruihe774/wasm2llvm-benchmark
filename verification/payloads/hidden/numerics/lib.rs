//! Numeric edge cases: i128 arithmetic, wide multiplies, checked/overflowing/saturating
//! ops, float<->int conversions on boundary values, sign-extension, bit tricks.
#![no_std]
#![feature(wasm_numeric_instr, float_minimum_maximum)]

#[path = "../../common/common.rs"]
mod common;
use common::*;
use core::arch::wasm32 as w;

fn h(acc: u64, x: u64) -> u64 { fnv(acc, &x.to_le_bytes()) }
fn hf32(acc: u64, x: f32) -> u64 { h(acc, if x.is_nan() { 1 } else { x.to_bits() as u64 }) }
fn hf64(acc: u64, x: f64) -> u64 { h(acc, if x.is_nan() { 1 } else { x.to_bits() }) }

#[inline(never)]
fn i128_ops(a: u128, b: u128) -> u64 {
    let mut r = FNV0;
    let (sa, sb) = (a as i128, b as i128);
    r = h(r, a.wrapping_mul(b) as u64);
    r = h(r, (a.wrapping_mul(b) >> 64) as u64);
    if b != 0 { r = h(r, (a / b) as u64); r = h(r, (a % b) as u64); }
    if sb != 0 && !(sa == i128::MIN && sb == -1) { r = h(r, (sa / sb) as u64); r = h(r, (sa % sb) as u64); }
    r = h(r, (a << (b as u32 & 127)) as u64);
    r = h(r, (sa >> (b as u32 & 127)) as u64);
    r = h(r, (a >> (b as u32 & 127) >> 64) as u64);
    r = h(r, a.leading_zeros() as u64 | (b.trailing_zeros() as u64) << 8 | (a.count_ones() as u64) << 16);
    r = h(r, (a as f64).to_bits() ^ (sa as f32).to_bits() as u64);
    r
}

#[inline(never)]
fn int_ops(a: u64, b: u64) -> u64 {
    let mut r = FNV0;
    let (sa, sb) = (a as i64, b as i64);
    r = h(r, sa.checked_mul(sb).map_or(1, |v| v as u64));
    r = h(r, sa.overflowing_add(sb).0 as u64 ^ sa.overflowing_add(sb).1 as u64);
    r = h(r, sa.saturating_sub(sb) as u64);
    r = h(r, (a as u32).saturating_add(b as u32) as u64);
    r = h(r, (sa as i32).saturating_mul(sb as i32) as u64);
    r = h(r, ((a as u128 * b as u128) >> 64) as u64); // i64 mulh via 128-bit
    r = h(r, ((sa as i128 * sb as i128) >> 64) as u64);
    r = h(r, (a as u32 as u64 * b as u32 as u64));
    r = h(r, sa.wrapping_rem_euclid(sb | 1) as u64 ^ sa.wrapping_div_euclid(sb | 1) as u64);
    r = h(r, (a as i8).wrapping_abs() as u64 ^ (a as i16).wrapping_neg() as u64);
    r = h(r, (a as u16).swap_bytes() as u64 ^ (a as u32).reverse_bits() as u64);
    r = h(r, (sa as i32 as i64 as u64) ^ (sa as i16 as i64 as u64) << 1 ^ (sa as i8 as i32 as u32 as u64) << 2);
    r = h(r, a.isqrt() ^ (a as u32).checked_ilog2().unwrap_or(99) as u64);
    r = h(r, (a as u32).wrapping_pow((b & 7) as u32) as u64 ^ sa.wrapping_pow((b & 15) as u32) as u64);
    r
}

#[inline(never)]
fn conv(bits: u64) -> u64 {
    let mut r = FNV0;
    let f = f64::from_bits(bits);
    let g = f32::from_bits(bits as u32);
    r = h(r, f as i8 as u64 ^ (f as u8 as u64) << 8 ^ (f as i16 as u64) << 16 ^ (f as u16 as u64) << 32);
    r = h(r, f as i32 as u64 ^ (f as u32 as u64) << 32);
    r = h(r, f as i64 as u64);
    r = h(r, f as u64);
    r = h(r, (f as i128) as u64 ^ ((f as u128) >> 64) as u64);
    r = h(r, g as i32 as u64 ^ (g as u32 as u64) << 32);
    r = h(r, g as i64 as u64 ^ g as u64);
    r = hf32(r, f as f32);
    r = hf64(r, g as f64);
    r = hf32(r, bits as f32);
    r = hf32(r, bits as i64 as f32);
    r = hf64(r, bits as f64);
    r = hf64(r, bits as i64 as f64);
    r = hf32(r, (bits >> 40) as u32 as f32);
    r = hf32(r, (bits as u32 | 1) as f32);
    r = hf64(r, w::f64_nearest(f));
    r = hf32(r, w::f32_nearest(g));
    r = hf64(r, f.minimum(-0.0));
    r = hf64(r, f.maximum(0.0));
    r = hf32(r, g.minimum(f as f32));
    // the sign of a NaN produced by promote is implementation-defined in wasm, so keep it out of copysign
    r = hf64(r, f.copysign(if g.is_nan() { 1.0 } else { g as f64 }));
    r = h(r, (f.abs() == g.abs() as f64) as u64 | ((f < 0.0) as u64) << 1 | ((g >= -0.0) as u64) << 2);
    r
}

#[no_mangle]
pub extern "C" fn run(n: i32) -> i64 {
    puts("numerics\n");
    let mut rng = Rng::new(n as u64);
    let mut acc = FNV0;
    let edges: [u64; 24] = [0, 1, 2, 3, 0x7f, 0x80, 0xff, 0x7fff, 0x8000, 0xffff, 0x7fff_ffff, 0x8000_0000,
        0xffff_ffff, 0x1_0000_0000, 0x7fff_ffff_ffff_ffff, 0x8000_0000_0000_0000, u64::MAX, u64::MAX - 1,
        0x0010_0000_0000_0000, 0x0020_0000_0000_0001, 0x4330_0000_0000_0000, 0x43e0_0000_0000_0000,
        0xc3e0_0000_0000_0000, 0x7ff0_0000_0000_0000];
    for &a in edges.iter() {
        for &b in edges.iter() {
            acc ^= int_ops(a, b);
            acc = acc.rotate_left(1) ^ i128_ops((a as u128) << 64 | b as u128, b as u128 * 3 + a as u128);
        }
        acc ^= conv(a);
        acc ^= conv(a.rotate_left(32));
    }
    let fedges: [f64; 20] = [2147483647.0, 2147483647.9, 2147483648.0, -2147483648.9, -2147483649.0, 4294967295.9,
        4294967296.0, 9223372036854774784.0, 9223372036854775808.0, -9223372036854775808.0, -9223372036854777856.0,
        18446744073709549568.0, 18446744073709551616.0, 0.5, -0.5, 1.5, -1.5, 2.5, 1e-310, -1e300];
    for &f in fedges.iter() {
        acc ^= conv(f.to_bits());
        acc ^= conv((f as f32).to_bits() as u64);
    }
    pi64(acc as i64);
    for _ in 0..(n as usize) {
        let (a, b) = (rng.next_u64(), rng.next_u64() >> rng.below(64));
        acc ^= int_ops(a, b);
        acc = acc.rotate_left(3) ^ i128_ops((a as u128) << (b & 63) | b as u128, (b as u128) << 17 | 1);
        acc ^= conv(a);
        acc ^= conv(b);
    }
    pi64(acc as i64);
    acc as i64
}

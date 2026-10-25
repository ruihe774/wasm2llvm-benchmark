//! Floating point: f32/f64 arithmetic, min/max, rounding, sqrt, conversions
//! (including saturating float->int casts), copysign/abs/neg, reinterpret.
#![no_std]
#![feature(wasm_numeric_instr, float_minimum_maximum)]

#[path = "../../common/common.rs"]
mod common;
use common::*;

use core::arch::wasm32 as w;

#[inline(never)]
fn round_ops32(x: f32) -> [f32; 6] {
    [w::f32_ceil(x), w::f32_floor(x), w::f32_trunc(x), w::f32_nearest(x), x.abs(), -x]
}


#[inline(never)]
fn round_ops64(x: f64) -> [f64; 5] {
    [w::f64_ceil(x), w::f64_floor(x), w::f64_trunc(x), w::f64_nearest(x), w::f64_sqrt(x.abs())]
}

#[inline(never)]
fn sat_casts(x: f64) -> u64 {
    let a = x as i32 as u32 as u64;
    let b = x as u32 as u64;
    let c = x as i64 as u64;
    let d = x as u64;
    let e = (x as f32) as i32 as u32 as u64;
    let f = (x as f32) as u64;
    let g = (x as f32) as i8 as u8 as u64;
    a ^ b.rotate_left(7) ^ c.rotate_left(13) ^ d.rotate_left(19) ^ e.rotate_left(29) ^ f.rotate_left(37) ^ g
}

#[inline(never)]
fn int_to_float(x: u64) -> u64 {
    let a = (x as f32).to_bits() as u64;
    let b = (x as i64 as f32).to_bits() as u64;
    let c = (x as f64).to_bits();
    let d = (x as i64 as f64).to_bits();
    let e = (x as u32 as f32).to_bits() as u64;
    let f = (x as i32 as f32).to_bits() as u64;
    let g = (x as u32 as f64).to_bits();
    let h = (x as i32 as f64).to_bits();
    a ^ b.rotate_left(3) ^ c.rotate_left(11) ^ d.rotate_left(17) ^ e.rotate_left(23) ^ f.rotate_left(31) ^ g.rotate_left(41) ^ h.rotate_left(53)
}

/// Bit pattern hash that maps every NaN to one value (NaN payloads are nondeterministic).
fn h32(x: f32) -> u64 { if x.is_nan() { 0x7fc0_0000 } else { x.to_bits() as u64 } }
fn h64(x: f64) -> u64 { if x.is_nan() { 0x7ff8_0000_0000_0000 } else { x.to_bits() } }

#[inline(never)]
fn series(n: u32) -> f64 {
    // Leibniz pi, harmonic sums, compensated summation
    let mut pi = 0.0f64;
    let mut sum = 0.0f64;
    let mut c = 0.0f64;
    for k in 0..n {
        let t = if k % 2 == 0 { 1.0 } else { -1.0 } / (2 * k + 1) as f64;
        pi += t;
        let y = 1.0 / ((k + 1) as f64) - c;
        let s = sum + y;
        c = (s - sum) - y;
        sum = s;
    }
    pi * 4.0 + sum
}

#[inline(never)]
fn mandel(w: u32, h: u32) -> u32 {
    let mut total = 0u32;
    for py in 0..h {
        for px in 0..w {
            let x0 = px as f32 / w as f32 * 3.5 - 2.5;
            let y0 = py as f32 / h as f32 * 2.0 - 1.0;
            let (mut x, mut y, mut i) = (0.0f32, 0.0f32, 0u32);
            while x * x + y * y <= 4.0 && i < 200 {
                let xt = x * x - y * y + x0;
                y = 2.0 * x * y + y0;
                x = xt;
                i += 1;
            }
            total = total.wrapping_mul(7).wrapping_add(i);
        }
    }
    total
}

#[no_mangle]
pub extern "C" fn run(seed: i32) -> i64 {
    puts("floats\n");
    let mut rng = Rng::new(seed as u64);
    let mut acc = FNV0;
    let specials64 = [0.0, -0.0, 1.0, -1.0, 0.5, -0.5, 1.5, 2.5, -2.5, 3.5, 1e10, -1e10, 4294967295.5, 4294967296.0,
        -2147483648.5, 2147483647.5, 9.3e18, 1.9e19, -9.3e18, f64::INFINITY, f64::NEG_INFINITY, f64::NAN,
        f64::MIN_POSITIVE, 5e-324, f64::MAX, f64::EPSILON, 0.49999999999999994, 4503599627370497.0];
    for &x in specials64.iter() {
        acc = fnv(acc, &sat_casts(x).to_le_bytes());
        for r in round_ops64(x) { acc = fnv(acc, &h64(r).to_le_bytes()); }
        for r in round_ops32(x as f32) { acc = fnv(acc, &h32(r).to_le_bytes()); }
        for &y in specials64.iter() {
            let v = [x + y, x - y, x * y, x / y, x.min(y), x.max(y), x.minimum(y), x.maximum(y), x.copysign(y)];
            for r in v { acc = fnv(acc, &h64(r).to_le_bytes()); }
            let (a, b) = (x as f32, y as f32);
            let v = [a + b, a - b, a * b, a / b, a.minimum(b), a.maximum(b), a.copysign(b)];
            for r in v { acc = fnv(acc, &h32(r).to_le_bytes()); }
            let cmp = (x < y) as u8 | ((x <= y) as u8) << 1 | ((x == y) as u8) << 2 | ((x != y) as u8) << 3
                | ((a > b) as u8) << 4 | ((a >= b) as u8) << 5 | ((a == b) as u8) << 6 | ((a != b) as u8) << 7;
            acc = fnv(acc, &[cmp]);
        }
    }
    pi64(acc as i64);
    for _ in 0..3000 {
        let bits = rng.next_u64();
        let x = f64::from_bits(bits);
        let y = f32::from_bits(bits as u32);
        acc = fnv(acc, &sat_casts(x).to_le_bytes());
        acc = fnv(acc, &sat_casts(y as f64).to_le_bytes());
        acc = fnv(acc, &int_to_float(bits).to_le_bytes());
        acc = fnv(acc, &int_to_float(bits >> (bits & 63)).to_le_bytes());
        acc = fnv(acc, &h64(x * 1.5 + 0.25).to_le_bytes());
        acc = fnv(acc, &h32(w::f32_sqrt(y.abs())).to_le_bytes());
        acc = fnv(acc, &h64((y as f64) * (x as f32 as f64)).to_le_bytes());
    }
    pi64(acc as i64);
    let s = series(100000);
    pf64(s);
    let m = mandel(120, 60);
    pi32(m as i32);
    pf32(s as f32);
    (acc ^ m as u64 ^ s.to_bits()) as i64
}

//! Integer basics: recursion, loops, i32/i64 arithmetic, bit ops, sign-extension, globals.
#![no_std]

#[path = "../../common/common.rs"]
mod common;
use common::*;

static mut COUNTER: u32 = 0;

#[inline(never)]
fn fib(n: u32) -> u64 {
    unsafe { COUNTER += 1 };
    if n < 2 { n as u64 } else { fib(n - 1) + fib(n - 2) }
}

#[inline(never)]
fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

#[inline(never)]
fn collatz_len(mut n: u64) -> u32 {
    let mut c = 0;
    while n != 1 {
        n = if n % 2 == 0 { n / 2 } else { 3 * n + 1 };
        c += 1;
    }
    c
}

#[inline(never)]
fn bits(x: u64) -> u64 {
    (x.count_ones() as u64)
        ^ ((x.leading_zeros() as u64) << 8)
        ^ ((x.trailing_zeros() as u64) << 16)
        ^ x.rotate_left(13)
        ^ (x as u32).rotate_right(7) as u64
        ^ ((x as u32).leading_zeros() as u64) << 24
        ^ ((x as u32).trailing_zeros() as u64) << 32
        ^ ((x as u32).count_ones() as u64) << 40
}

#[inline(never)]
fn sext(x: u64) -> i64 {
    let a = x as u8 as i8 as i32;
    let b = x as u16 as i16 as i32;
    let c = x as u8 as i8 as i64;
    let d = x as u16 as i16 as i64;
    let e = x as u32 as i32 as i64;
    (a as i64).wrapping_mul(3) ^ (b as i64).wrapping_mul(5) ^ c.wrapping_mul(7) ^ d.wrapping_mul(11) ^ e.wrapping_mul(13)
}

#[inline(never)]
fn divs(a: i64, b: i64) -> i64 {
    if b == 0 || (a == i64::MIN && b == -1) {
        return 0;
    }
    let q32 = (a as i32).wrapping_div(b as i32 | 1);
    let r32 = (a as i32).wrapping_rem(b as i32 | 1);
    let uq = (a as u64) / (b as u64);
    let ur = (a as u64) % (b as u64);
    (a / b) ^ (a % b) ^ (q32 as i64) ^ ((r32 as i64) << 3) ^ (uq as i64) ^ ((ur as i64) << 5)
}

#[no_mangle]
pub extern "C" fn run(n: i32) -> i64 {
    puts("basics\n");
    let f = fib(n as u32);
    pi64(f as i64);
    pi32(unsafe { COUNTER } as i32);
    let mut acc: u64 = FNV0;
    let mut rng = Rng::new(n as u64);
    for i in 0..2000u64 {
        let x = rng.next_u64();
        let y = rng.next_u64() >> (i % 64);
        acc = acc.rotate_left(5) ^ gcd(x >> 20, y >> 20);
        acc = acc.wrapping_add(bits(x));
        acc ^= sext(x) as u64;
        acc = acc.wrapping_mul(31) ^ divs(x as i64, (y as i64) >> (i % 50)) as u64;
        acc ^= (x as u32).wrapping_shl(y as u32) as u64;
        acc ^= ((x as i32) >> (y as u32 & 31)) as u64;
        acc ^= x.wrapping_shr(y as u32);
        acc ^= ((x as i64).wrapping_shr(y as u32)) as u64;
    }
    pi64(acc as i64);
    let mut total = 0u32;
    for k in 1..3000u64 {
        total = total.wrapping_add(collatz_len(k));
    }
    pi32(total as i32);
    // signed / unsigned comparisons
    let mut cmp = 0i32;
    for i in -50i32..50 {
        for j in -50i32..50 {
            cmp = cmp.wrapping_mul(3).wrapping_add((i < j) as i32 + 2 * ((i as u32) < (j as u32)) as i32
                + 4 * ((i as i64) >= (j as i64)) as i32 + 8 * ((i as u64) > (j as u64)) as i32);
        }
    }
    pi32(cmp);
    (acc ^ f ^ total as u64) as i64
}

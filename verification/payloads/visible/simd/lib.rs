//! SIMD128: broad coverage of core::arch::wasm32 intrinsics plus auto-vectorized loops.
#![no_std]

#[path = "../../common/common.rs"]
mod common;
use common::*;
use core::arch::wasm32::*;

fn vbytes(v: v128) -> [u8; 16] { unsafe { core::mem::transmute(v) } }
fn hv(h: u64, v: v128) -> u64 { fnv(h, &vbytes(v)) }

/// Random finite floats in a moderate range (never NaN/inf so results stay deterministic).
fn rf32(r: &mut Rng) -> f32 { (r.f32() - 0.5) * 2000.0 }
fn rf64(r: &mut Rng) -> f64 { (r.f64() - 0.5) * 2.0e6 }
fn rv(r: &mut Rng) -> v128 { u64x2(r.next_u64(), r.next_u64()) }
fn rvf32(r: &mut Rng) -> v128 { f32x4(rf32(r), rf32(r), rf32(r), rf32(r)) }
fn rvf64(r: &mut Rng) -> v128 { f64x2(rf64(r), rf64(r)) }

#[inline(never)]
fn int_ops(a: v128, b: v128, s: u32) -> [v128; 64] {
    [
        i8x16_add(a, b), i8x16_sub(a, b), i8x16_add_sat(a, b), u8x16_add_sat(a, b), i8x16_sub_sat(a, b),
        u8x16_sub_sat(a, b), i8x16_min(a, b), u8x16_min(a, b), i8x16_max(a, b), u8x16_max(a, b), u8x16_avgr(a, b),
        i8x16_abs(a), i8x16_neg(a), i8x16_popcnt(a), i8x16_shl(a, s), i8x16_shr(a, s), u8x16_shr(a, s),
        i8x16_eq(a, b), i8x16_ne(a, b), i8x16_lt(a, b), u8x16_lt(a, b), i8x16_ge(a, b), u8x16_gt(a, b),
        i16x8_add(a, b), i16x8_sub(a, b), i16x8_mul(a, b), i16x8_add_sat(a, b), u16x8_sub_sat(a, b),
        i16x8_q15mulr_sat(a, b), i16x8_shl(a, s), i16x8_shr(a, s), u16x8_shr(a, s), u16x8_avgr(a, b),
        i16x8_min(a, b), u16x8_max(a, b), i16x8_le(a, b), u16x8_ge(a, b),
        i32x4_add(a, b), i32x4_sub(a, b), i32x4_mul(a, b), i32x4_shl(a, s), i32x4_shr(a, s), u32x4_shr(a, s),
        i32x4_min(a, b), u32x4_min(a, b), i32x4_max(a, b), u32x4_max(a, b), i32x4_dot_i16x8(a, b),
        i32x4_lt(a, b), u32x4_gt(a, b), i32x4_abs(a), i32x4_neg(a),
        i64x2_add(a, b), i64x2_sub(a, b), i64x2_mul(a, b), i64x2_shl(a, s), i64x2_shr(a, s), u64x2_shr(a, s),
        i64x2_eq(a, b), i64x2_ne(a, b), i64x2_lt(a, b), i64x2_ge(a, b), i64x2_abs(a), i64x2_neg(a),
    ]
}

#[inline(never)]
fn conv_ops(a: v128, b: v128) -> [v128; 28] {
    [
        i8x16_narrow_i16x8(a, b), u8x16_narrow_i16x8(a, b), i16x8_narrow_i32x4(a, b), u16x8_narrow_i32x4(a, b),
        i16x8_extend_low_i8x16(a), i16x8_extend_high_i8x16(a), i16x8_extend_low_u8x16(a), i16x8_extend_high_u8x16(a),
        i32x4_extend_low_i16x8(a), i32x4_extend_high_u16x8(a), i64x2_extend_low_i32x4(a), i64x2_extend_high_u32x4(a),
        i16x8_extmul_low_i8x16(a, b), i16x8_extmul_high_u8x16(a, b), i32x4_extmul_low_u16x8(a, b),
        i32x4_extmul_high_i16x8(a, b), i64x2_extmul_low_i32x4(a, b), i64x2_extmul_high_u32x4(a, b),
        i16x8_extadd_pairwise_i8x16(a), i16x8_extadd_pairwise_u8x16(a), i32x4_extadd_pairwise_i16x8(a),
        i32x4_extadd_pairwise_u16x8(a),
        v128_and(a, b), v128_or(a, b), v128_xor(a, b), v128_not(a), v128_andnot(a, b),
        v128_bitselect(a, b, u64x2(0xff00ff00f0f0f0f0, 0x0123456789abcdef)),
    ]
}

#[inline(never)]
fn float_ops(a: v128, b: v128, c: v128, d: v128) -> [v128; 44] {
    [
        f32x4_add(a, b), f32x4_sub(a, b), f32x4_mul(a, b), f32x4_div(a, b), f32x4_min(a, b), f32x4_max(a, b),
        f32x4_pmin(a, b), f32x4_pmax(a, b), f32x4_abs(a), f32x4_neg(a), f32x4_sqrt(f32x4_abs(a)), f32x4_ceil(a),
        f32x4_floor(a), f32x4_trunc(a), f32x4_nearest(a), f32x4_eq(a, b), f32x4_ne(a, a), f32x4_lt(a, b),
        f32x4_le(a, b), f32x4_gt(a, b), f32x4_ge(a, b),
        f64x2_add(c, d), f64x2_sub(c, d), f64x2_mul(c, d), f64x2_div(c, d), f64x2_min(c, d), f64x2_max(c, d),
        f64x2_pmin(c, d), f64x2_pmax(c, d), f64x2_sqrt(f64x2_abs(c)), f64x2_nearest(c), f64x2_floor(c), f64x2_lt(c, d),
        i32x4_trunc_sat_f32x4(a), u32x4_trunc_sat_f32x4(a), f32x4_convert_i32x4(b), f32x4_convert_u32x4(b),
        i32x4_trunc_sat_f64x2_zero(c), u32x4_trunc_sat_f64x2_zero(d), f64x2_convert_low_i32x4(b),
        f64x2_convert_low_u32x4(b), f32x4_demote_f64x2_zero(c), f64x2_promote_low_f32x4(a), f64x2_neg(c),
    ]
}

#[inline(never)]
fn lane_ops(a: v128, b: v128, x: u64) -> u64 {
    let mut h = FNV0;
    h = hv(h, i8x16_shuffle::<0, 17, 2, 19, 4, 21, 6, 23, 31, 30, 29, 28, 3, 3, 3, 3>(a, b));
    h = hv(h, i16x8_shuffle::<7, 6, 5, 4, 8, 9, 10, 11>(a, b));
    h = hv(h, i32x4_shuffle::<0, 4, 1, 5>(a, b));
    h = hv(h, i64x2_shuffle::<1, 2>(a, b));
    h = hv(h, i8x16_swizzle(a, u8x16_splat(x as u8 & 31)));
    h = hv(h, i8x16_swizzle(a, b));
    h = hv(h, i8x16_splat(x as i8));
    h = hv(h, i16x8_splat(x as i16));
    h = hv(h, i32x4_splat(x as i32));
    h = hv(h, i64x2_splat(x as i64));
    h = hv(h, f32x4_splat(x as u32 as f32));
    h = hv(h, f64x2_splat(x as f64));
    h = hv(h, i8x16_replace_lane::<5>(a, x as i8));
    h = hv(h, i16x8_replace_lane::<3>(a, x as i16));
    h = hv(h, i32x4_replace_lane::<2>(a, x as i32));
    h = hv(h, i64x2_replace_lane::<1>(a, x as i64));
    h = hv(h, f32x4_replace_lane::<0>(a, 1.25));
    h = hv(h, f64x2_replace_lane::<1>(a, -3.5));
    h ^= i8x16_extract_lane::<9>(a) as i64 as u64;
    h ^= (u8x16_extract_lane::<9>(a) as u64) << 8;
    h ^= (i16x8_extract_lane::<6>(b) as i64 as u64) << 3;
    h ^= (u16x8_extract_lane::<6>(b) as u64) << 13;
    h ^= (i32x4_extract_lane::<3>(a) as u64) << 1;
    h ^= i64x2_extract_lane::<1>(b) as u64;
    h = h.wrapping_mul(31) ^ (i8x16_bitmask(a) as u64) ^ (i16x8_bitmask(b) as u64) << 16
        ^ (i32x4_bitmask(a) as u64) << 24 ^ (i64x2_bitmask(b) as u64) << 28;
    h = h.wrapping_mul(31) ^ (v128_any_true(a) as u64) ^ (i8x16_all_true(a) as u64) << 1
        ^ (i16x8_all_true(b) as u64) << 2 ^ (i32x4_all_true(a) as u64) << 3 ^ (i64x2_all_true(b) as u64) << 4
        ^ (v128_any_true(v128_and(a, v128_not(a))) as u64) << 5;
    h
}

#[inline(never)]
unsafe fn mem_ops(buf: &mut [u8; 64], x: u64) -> u64 {
    let p = buf.as_mut_ptr();
    let mut h = FNV0;
    let off = (x % 40) as usize;
    let v = v128_load(p.add(off) as *const v128);
    h = hv(h, v);
    h = hv(h, i16x8_load_extend_i8x8(p.add(off + 1) as *const i8));
    h = hv(h, u16x8_load_extend_u8x8(p.add(off + 2) as *const u8));
    h = hv(h, i32x4_load_extend_i16x4(p.add(off + 3) as *const i16));
    h = hv(h, u32x4_load_extend_u16x4(p.add(off + 4) as *const u16));
    h = hv(h, i64x2_load_extend_i32x2(p.add(off + 5) as *const i32));
    h = hv(h, u64x2_load_extend_u32x2(p.add(off + 6) as *const u32));
    h = hv(h, v128_load8_splat(p.add(off + 7)));
    h = hv(h, v128_load16_splat(p.add(off + 8) as *const u16));
    h = hv(h, v128_load32_splat(p.add(off + 9) as *const u32));
    h = hv(h, v128_load64_splat(p.add(off + 10) as *const u64));
    h = hv(h, v128_load32_zero(p.add(off + 11) as *const u32));
    h = hv(h, v128_load64_zero(p.add(off + 12) as *const u64));
    h = hv(h, v128_load8_lane::<3>(v, p.add(off + 13)));
    h = hv(h, v128_load16_lane::<5>(v, p.add(off + 14) as *const u16));
    h = hv(h, v128_load32_lane::<1>(v, p.add(off + 15) as *const u32));
    h = hv(h, v128_load64_lane::<0>(v, p.add(off + 16) as *const u64));
    let w = u64x2(x, !x);
    v128_store8_lane::<7>(w, p.add(off + 1));
    v128_store16_lane::<2>(w, p.add(off + 3) as *mut u16);
    v128_store32_lane::<3>(w, p.add(off + 5) as *mut u32);
    v128_store64_lane::<1>(w, p.add(off + 7) as *mut u64);
    v128_store(p.add((x % 48) as usize) as *mut v128, i8x16_add(v, w));
    fnv(h, &buf[..])
}

// Auto-vectorization candidates (LLVM vectorizes these with simd128).
#[inline(never)]
fn saxpy(a: f32, x: &[f32], y: &mut [f32]) {
    for (yi, xi) in y.iter_mut().zip(x.iter()) {
        *yi = a * *xi + *yi;
    }
}

#[inline(never)]
fn sum_u8(x: &[u8]) -> u32 { x.iter().map(|&v| v as u32).sum() }

#[inline(never)]
fn dot_i16(a: &[i16], b: &[i16]) -> i32 {
    a.iter().zip(b).fold(0i32, |s, (&x, &y)| s.wrapping_add(x as i32 * y as i32))
}

#[inline(never)]
fn clamp_u8(x: &[i32], out: &mut [u8]) {
    for (o, &v) in out.iter_mut().zip(x) {
        *o = v.clamp(0, 255) as u8;
    }
}

static mut FX: [f32; 1024] = [0.0; 1024];
static mut FY: [f32; 1024] = [0.0; 1024];
static mut B8: [u8; 4096] = [0; 4096];
static mut I16A: [i16; 2048] = [0; 2048];
static mut I16B: [i16; 2048] = [0; 2048];
static mut I32S: [i32; 1024] = [0; 1024];
static mut OUT8: [u8; 1024] = [0; 1024];

#[no_mangle]
pub extern "C" fn run(seed: i32) -> i64 {
    puts("simd\n");
    let mut r = Rng::new(seed as u64);
    let mut h = FNV0;
    for i in 0..400u32 {
        let (a, b) = (rv(&mut r), rv(&mut r));
        for v in int_ops(a, b, i % 70) { h = hv(h, v); }
        for v in conv_ops(a, b) { h = hv(h, v); }
        let (fa, fb, fc, fd) = (rvf32(&mut r), rvf32(&mut r), rvf64(&mut r), rvf64(&mut r));
        for v in float_ops(fa, fb, fc, fd) { h = hv(h, v); }
        h ^= lane_ops(a, b, r.next_u64());
    }
    pi64(h as i64);
    let mut buf = [0u8; 64];
    for (i, b) in buf.iter_mut().enumerate() { *b = (i as u8).wrapping_mul(37) ^ 0x5a; }
    for _ in 0..500 {
        h ^= unsafe { mem_ops(&mut buf, r.next_u64()) };
        h = h.rotate_left(9);
    }
    pi64(h as i64);
    unsafe {
        let (fx, fy, b8, ia, ib, is, o8) = (&mut *core::ptr::addr_of_mut!(FX), &mut *core::ptr::addr_of_mut!(FY),
            &mut *core::ptr::addr_of_mut!(B8), &mut *core::ptr::addr_of_mut!(I16A), &mut *core::ptr::addr_of_mut!(I16B),
            &mut *core::ptr::addr_of_mut!(I32S), &mut *core::ptr::addr_of_mut!(OUT8));
        for i in 0..1024 { fx[i] = r.f32(); fy[i] = r.f32(); is[i] = r.next_u32() as i32 >> 20; }
        for v in b8.iter_mut() { *v = r.next_u32() as u8; }
        for i in 0..2048 { ia[i] = r.next_u32() as i16; ib[i] = r.next_u32() as i16; }
        for k in 0..20 { saxpy(0.5 + k as f32 * 0.01, &fx[..], &mut fy[..]); }
        let mut fs = 0.0f64;
        for v in fy.iter() { fs += *v as f64; }
        pf64(fs);
        pi32(sum_u8(&b8[..]) as i32);
        pi32(dot_i16(&ia[..], &ib[..]));
        clamp_u8(&is[..], &mut o8[..]);
        h = fnv(h, &o8[..]);
    }
    pi64(h as i64);
    h as i64
}

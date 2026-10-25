//! Image processing kernels with explicit SIMD128 intrinsics (u8/i16 saturating,
//! averaging, widening, narrowing, shuffles) and f32x4 4x4 matrix transforms.
#![no_std]

#[path = "../../common/common.rs"]
mod common;
use common::*;
use core::arch::wasm32::*;

const W: usize = 256;
const H: usize = 128;
static mut IMG: [u8; W * H] = [0; W * H];
static mut IMG2: [u8; W * H] = [0; W * H];
static mut TMP: [u8; W * H] = [0; W * H];
static mut PTS: [f32; 4 * 1024] = [0.0; 4 * 1024];

unsafe fn ld(p: *const u8) -> v128 { v128_load(p as *const v128) }
unsafe fn st(p: *mut u8, v: v128) { v128_store(p as *mut v128, v) }

/// 3-tap horizontal box blur [1 2 1]/4 with u16 accumulation.
unsafe fn blur_h(src: &[u8], dst: &mut [u8]) {
    for y in 0..H {
        let row = y * W;
        dst[row] = src[row];
        dst[row + W - 1] = src[row + W - 1];
        let mut x = 1;
        while x + 16 < W {
            let l = ld(src.as_ptr().add(row + x - 1));
            let c = ld(src.as_ptr().add(row + x));
            let r = ld(src.as_ptr().add(row + x + 1));
            let lo = i16x8_add(i16x8_add(u16x8_extend_low_u8x16(l), u16x8_extend_low_u8x16(r)), i16x8_shl(u16x8_extend_low_u8x16(c), 1));
            let hi = i16x8_add(i16x8_add(u16x8_extend_high_u8x16(l), u16x8_extend_high_u8x16(r)), i16x8_shl(u16x8_extend_high_u8x16(c), 1));
            let lo = u16x8_shr(i16x8_add(lo, i16x8_splat(2)), 2);
            let hi = u16x8_shr(i16x8_add(hi, i16x8_splat(2)), 2);
            st(dst.as_mut_ptr().add(row + x), u8x16_narrow_i16x8(lo, hi));
            x += 16;
        }
        while x < W - 1 {
            dst[row + x] = ((src[row + x - 1] as u16 + 2 * src[row + x] as u16 + src[row + x + 1] as u16 + 2) >> 2) as u8;
            x += 1;
        }
    }
}

/// Vertical Sobel magnitude approximation |gy| with saturating arithmetic.
unsafe fn sobel_v(src: &[u8], dst: &mut [u8]) {
    for v in dst.iter_mut() { *v = 0; }
    for y in 1..H - 1 {
        let mut x = 0;
        while x + 16 <= W {
            let a = ld(src.as_ptr().add((y - 1) * W + x));
            let b = ld(src.as_ptr().add((y + 1) * W + x));
            let d = v128_or(u8x16_sub_sat(a, b), u8x16_sub_sat(b, a));
            let e = u8x16_add_sat(d, d);
            st(dst.as_mut_ptr().add(y * W + x), e);
            x += 16;
        }
    }
}

/// Alpha blend using q15 multiply: out = a*alpha + b*(1-alpha)
unsafe fn blend(a: &[u8], b: &[u8], alpha_q15: i16, dst: &mut [u8]) {
    let al = i16x8_splat(alpha_q15);
    let bl = i16x8_splat(32767 - alpha_q15);
    let mut i = 0;
    while i + 16 <= a.len() {
        let va = ld(a.as_ptr().add(i));
        let vb = ld(b.as_ptr().add(i));
        let lo = i16x8_add_sat(i16x8_q15mulr_sat(i16x8_shl(u16x8_extend_low_u8x16(va), 7), al),
                               i16x8_q15mulr_sat(i16x8_shl(u16x8_extend_low_u8x16(vb), 7), bl));
        let hi = i16x8_add_sat(i16x8_q15mulr_sat(i16x8_shl(u16x8_extend_high_u8x16(va), 7), al),
                               i16x8_q15mulr_sat(i16x8_shl(u16x8_extend_high_u8x16(vb), 7), bl));
        st(dst.as_mut_ptr().add(i), u8x16_narrow_i16x8(i16x8_shr(lo, 7), i16x8_shr(hi, 7)));
        i += 16;
    }
}

/// Histogram + min/max/avg via SIMD reductions.
unsafe fn stats(src: &[u8]) -> (u32, u8, u8, u64) {
    let mut mn = u8x16_splat(255);
    let mut mx = u8x16_splat(0);
    let mut sum = i32x4_splat(0);
    let mut avg = u8x16_splat(0);
    let mut i = 0;
    while i + 16 <= src.len() {
        let v = ld(src.as_ptr().add(i));
        mn = u8x16_min(mn, v);
        mx = u8x16_max(mx, v);
        avg = u8x16_avgr(avg, v);
        sum = i32x4_add(sum, i32x4_extadd_pairwise_u16x8(i16x8_extadd_pairwise_u8x16(v)));
        i += 16;
    }
    let mut m1 = 255u8; let mut m2 = 0u8;
    let mb: [u8; 16] = core::mem::transmute(mn);
    let xb: [u8; 16] = core::mem::transmute(mx);
    for k in 0..16 { m1 = m1.min(mb[k]); m2 = m2.max(xb[k]); }
    let s = i32x4_extract_lane::<0>(sum) as u32 + i32x4_extract_lane::<1>(sum) as u32
        + i32x4_extract_lane::<2>(sum) as u32 + i32x4_extract_lane::<3>(sum) as u32;
    // reverse bytes via swizzle and fold into hash
    let rev = i8x16_swizzle(avg, i8x16(15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0));
    let h = fnv(FNV0, &core::mem::transmute::<v128, [u8; 16]>(rev));
    (s, m1, m2, h ^ i8x16_bitmask(i8x16_gt(avg, u8x16_splat(128))) as u64)
}

/// Transform points by a 4x4 matrix using f32x4 (column-major splat/mul/add).
unsafe fn transform(pts: &mut [f32], m: &[f32; 16]) {
    let c0 = v128_load(m.as_ptr() as *const v128);
    let c1 = v128_load(m.as_ptr().add(4) as *const v128);
    let c2 = v128_load(m.as_ptr().add(8) as *const v128);
    let c3 = v128_load(m.as_ptr().add(12) as *const v128);
    let mut i = 0;
    while i + 4 <= pts.len() {
        let p = v128_load(pts.as_ptr().add(i) as *const v128);
        let x = i32x4_shuffle::<0, 0, 0, 0>(p, p);
        let y = i32x4_shuffle::<1, 1, 1, 1>(p, p);
        let z = i32x4_shuffle::<2, 2, 2, 2>(p, p);
        let w = i32x4_shuffle::<3, 3, 3, 3>(p, p);
        let r = f32x4_add(f32x4_add(f32x4_mul(c0, x), f32x4_mul(c1, y)), f32x4_add(f32x4_mul(c2, z), f32x4_mul(c3, w)));
        // normalize by w' (never zero for our matrices), clamp
        let wn = f32x4_splat(f32x4_extract_lane::<3>(r));
        let n = f32x4_div(r, wn);
        let n = f32x4_pmax(f32x4_pmin(n, f32x4_splat(1e6)), f32x4_splat(-1e6));
        v128_store(pts.as_mut_ptr().add(i) as *mut v128, f32x4_replace_lane::<3>(n, 1.0));
        i += 4;
    }
}

#[no_mangle]
pub extern "C" fn run(iters: i32, seed: i32) -> i64 {
    puts("simd_image\n");
    let mut rng = Rng::new(seed as u64);
    let (img, img2, tmp, pts) = unsafe { (&mut *core::ptr::addr_of_mut!(IMG), &mut *core::ptr::addr_of_mut!(IMG2), &mut *core::ptr::addr_of_mut!(TMP), &mut *core::ptr::addr_of_mut!(PTS)) };
    for y in 0..H {
        for x in 0..W {
            img[y * W + x] = ((x * 3 + y * 5) as u8) ^ (rng.below(32) as u8);
        }
    }
    for p in pts.chunks_mut(4) { p[0] = rng.f32() * 10.0 - 5.0; p[1] = rng.f32() * 10.0 - 5.0; p[2] = rng.f32() * 10.0; p[3] = 1.0; }
    let mut h = FNV0;
    for it in 0..iters {
        unsafe {
            blur_h(&img[..], &mut tmp[..]);
            sobel_v(&tmp[..], &mut img2[..]);
            blend(&img2[..], &tmp[..], (it as i16 * 1234) & 0x7fff, &mut img[..]);
            let (s, mn, mx, sh) = stats(&img[..]);
            h = fnv(h, &s.to_le_bytes());
            h ^= sh ^ ((mn as u64) << 40) ^ ((mx as u64) << 48);
            let a = it as f32 * 0.01;
            let m = [1.0, 0.0, a, 0.0, 0.0, 1.0, 0.0, 0.0, -a, 0.0, 1.0, 0.0, 0.1, 0.2, 0.3, 1.0 + a];
            transform(&mut pts[..], &m);
        }
    }
    h = fnv(h, &img[..]);
    for p in pts.iter() { h = fnv(h, &p.to_bits().to_le_bytes()); }
    pi64(h as i64);
    h as i64
}

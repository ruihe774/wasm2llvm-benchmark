#![no_std]

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

static TABLE: [i32; 8] = [3, 1, 4, 1, 5, 9, 2, 6];

#[no_mangle]
pub extern "C" fn add(a: i32, b: i32) -> i32 {
    a.wrapping_add(b)
}

#[no_mangle]
pub extern "C" fn dot(a: *const f32, b: *const f32, n: usize) -> f32 {
    let mut s = 0.0f32;
    let mut i = 0;
    while i < n {
        unsafe {
            s += *a.add(i) * *b.add(i);
        }
        i += 1;
    }
    s
}

#[no_mangle]
pub extern "C" fn sum_bytes(a: *const u8, n: usize) -> u32 {
    let mut s: u32 = 0;
    let mut i = 0;
    while i < n {
        unsafe {
            s = s.wrapping_add(*a.add(i) as u32 * 31);
        }
        i += 1;
    }
    s
}

#[no_mangle]
pub extern "C" fn fib(n: u32) -> u64 {
    if n < 2 { n as u64 } else { fib(n - 1) + fib(n - 2) }
}

#[no_mangle]
pub extern "C" fn table_at(i: usize) -> i32 {
    TABLE[i % 8]
}

#[no_mangle]
pub extern "C" fn classify(x: f64) -> i32 {
    if x.is_nan() { 0 } else if x < 0.0 { -1 } else if x == 0.0 { 0 } else { 1 }
}

#[no_mangle]
pub extern "C" fn memops(dst: *mut u8, src: *const u8, n: usize) {
    unsafe { core::ptr::copy(src, dst, n) }
}

#[no_mangle]
pub extern "C" fn truncz(x: f64) -> i64 { x as i64 }

#[no_mangle]
pub extern "C" fn mulh(a: u64, b: u64) -> u64 {
    ((a as u128 * b as u128) >> 64) as u64
}

pub trait Op {
    fn apply(&self, x: i32) -> i32;
}
struct AddOp(i32);
struct MulOp(i32);
impl Op for AddOp {
    fn apply(&self, x: i32) -> i32 { x.wrapping_add(self.0) }
}
impl Op for MulOp {
    fn apply(&self, x: i32) -> i32 { x.wrapping_mul(self.0) }
}
#[no_mangle]
pub extern "C" fn run_op(which: u32, x: i32) -> i32 {
    let op: &dyn Op = if which == 0 { &AddOp(7) } else { &MulOp(3) };
    op.apply(x)
}

#[no_mangle]
pub extern "C" fn run_closure(which: u32, x: i32) -> i32 {
    let f: fn(i32) -> i32 = if which == 0 { |v| v.wrapping_sub(9) } else { |v| v ^ 0x55 };
    f(x)
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn simd_add(a: *const u8, b: *const u8, out: *mut u8) {
    use core::arch::wasm32::*;
    unsafe {
        let va = v128_load(a as *const v128);
        let vb = v128_load(b as *const v128);
        let r = i32x4_add(va, vb);
        v128_store(out as *mut v128, r);
    }
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn simd_sum_bytes(a: *const u8, n: usize) -> u32 {
    use core::arch::wasm32::*;
    let mut acc = i32x4_splat(0);
    let mut i = 0;
    unsafe {
        while i + 16 <= n {
            let v = v128_load(a.add(i) as *const v128);
            acc = i32x4_add(acc, i32x4_extend_low_u16x8(v));
            i += 16;
        }
        let mut out = [0i32; 4];
        v128_store(out.as_mut_ptr() as *mut v128, acc);
        (out[0].wrapping_add(out[1]).wrapping_add(out[2]).wrapping_add(out[3])) as u32
    }
}

#[no_mangle]
pub extern "C" fn bench(n: u32) -> u32 {
    let mut acc: u32 = 1;
    let mut i: u32 = 0;
    while i < n {
        acc = acc.wrapping_mul(1664525).wrapping_add(1013904223);
        acc ^= acc >> 13;
        i += 1;
    }
    acc
}

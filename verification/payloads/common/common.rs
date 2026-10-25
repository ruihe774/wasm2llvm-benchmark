// Shared support code for w2l payloads (wasm32v1-none, no_std).
//
// Host imports (provided by both the native harness and the reference runner):
//   env.print_i32/print_i64/print_f32/print_f64/print_v128/putchar
#![allow(dead_code)]

#[link(wasm_import_module = "env")]
extern "C" {
    fn print_i32(x: i32);
    fn print_i64(x: i64);
    fn print_f32(x: f32);
    fn print_f64(x: f64);
    fn putchar(c: i32);
}


pub fn pi32(x: i32) { unsafe { print_i32(x) } }
pub fn pi64(x: i64) { unsafe { print_i64(x) } }
/// Prints the bit pattern; callers must never pass NaN (NaN bits are nondeterministic in wasm).
pub fn pf32(x: f32) { assert!(!x.is_nan()); unsafe { print_f32(x) } }
pub fn pf64(x: f64) { assert!(!x.is_nan()); unsafe { print_f64(x) } }
#[cfg(target_feature = "simd128")]
pub fn pv128(x: core::arch::wasm32::v128) {
    use core::arch::wasm32::*;
    pi64(i64x2_extract_lane::<0>(x));
    pi64(i64x2_extract_lane::<1>(x));
}

pub fn puts(s: &str) {
    for b in s.bytes() {
        unsafe { putchar(b as i32) }
    }
}

pub struct Out;
impl core::fmt::Write for Out {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        puts(s);
        Ok(())
    }
}

#[macro_export]
macro_rules! outln {
    ($($t:tt)*) => {{ use core::fmt::Write; let _ = writeln!($crate::common::Out, $($t)*); }};
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}

/// xorshift64* PRNG
pub struct Rng(pub u64);
impl Rng {
    pub fn new(seed: u64) -> Self { Rng(seed ^ 0x9E37_79B9_7F4A_7C15 | 1) }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    pub fn next_u32(&mut self) -> u32 { (self.next_u64() >> 32) as u32 }
    pub fn below(&mut self, n: u32) -> u32 { ((self.next_u32() as u64 * n as u64) >> 32) as u32 }
    pub fn f64(&mut self) -> f64 { (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64) }
    pub fn f32(&mut self) -> f32 { (self.next_u32() >> 8) as f32 * (1.0 / (1u32 << 24) as f32) }
}

/// FNV-1a over bytes, for checksums.
pub fn fnv(h: u64, bytes: &[u8]) -> u64 {
    let mut h = h;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}
pub const FNV0: u64 = 0xcbf29ce484222325;


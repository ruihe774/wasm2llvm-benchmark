//! Exports that hit every wasm trap kind reachable from Rust, plus memory.grow failure.
#![no_std]
#![feature(core_intrinsics)]
#![allow(internal_features)]

#[path = "../../common/common.rs"]
mod common;
use common::*;
use core::intrinsics;

#[no_mangle]
pub extern "C" fn div_s(a: i32, b: i32) -> i32 { unsafe { intrinsics::unchecked_div(a, b) } }
#[no_mangle]
pub extern "C" fn div_u(a: u32, b: u32) -> u32 { unsafe { intrinsics::unchecked_div(a, b) } }
#[no_mangle]
pub extern "C" fn rem_s64(a: i64, b: i64) -> i64 { unsafe { intrinsics::unchecked_rem(a, b) } }
#[no_mangle]
pub extern "C" fn div_s64(a: i64, b: i64) -> i64 { unsafe { intrinsics::unchecked_div(a, b) } }

#[no_mangle]
pub extern "C" fn f2i(x: f32) -> i32 { unsafe { x.to_int_unchecked::<i32>() } }
#[no_mangle]
pub extern "C" fn d2u64(x: f64) -> u64 { unsafe { x.to_int_unchecked::<u64>() } }

#[inline(never)]
fn add1(x: i32) -> i32 { x.wrapping_add(1) }
#[inline(never)]
fn add2(x: i32) -> i32 { x.wrapping_add(2) }

static TABLE: [fn(i32) -> i32; 2] = [add1, add2];

/// Calls through a function pointer whose static type does not match (sel=1),
/// a null pointer (sel=2), an out-of-range table index (sel=3), or correctly (sel=0).
#[no_mangle]
pub extern "C" fn bad_call(sel: i32, x: i32) -> i64 {
    let f = TABLE[(x & 1) as usize];
    unsafe {
        match sel {
            0 => f(x) as i64,
            1 => {
                let g: fn(i64) -> i64 = core::mem::transmute(f);
                g(x as i64)
            }
            2 => {
                let g: fn(i32) -> i32 = core::mem::transmute(core::hint::black_box(0usize));
                g(x) as i64
            }
            _ => {
                let g: fn(i32) -> i32 = core::mem::transmute(core::hint::black_box(0x10_0000usize));
                g(x) as i64
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn grow(pages: i32) -> i32 {
    core::arch::wasm32::memory_grow(0, pages as usize) as i32
}

#[no_mangle]
pub extern "C" fn size() -> i32 { core::arch::wasm32::memory_size(0) as i32 }

/// Store at an address; traps when out of bounds. Returns the value read back.
#[no_mangle]
pub extern "C" fn poke(addr: u32, v: u64) -> u64 {
    unsafe {
        core::ptr::write_volatile(addr as usize as *mut u64, v);
        core::ptr::read_volatile(addr as usize as *const u64)
    }
}

#[no_mangle]
pub extern "C" fn poke8(addr: u32, v: u32) -> u32 {
    unsafe {
        core::ptr::write_volatile(addr as usize as *mut u8, v as u8);
        core::ptr::read_volatile(addr as usize as *const u8) as u32
    }
}

#[no_mangle]
pub extern "C" fn hello() { puts("traps ready\n"); }

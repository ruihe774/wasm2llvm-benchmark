//! Indirect calls (trait objects, function pointer tables), large match statements
//! (br_table), a small stack VM, and exports that trap deliberately.
#![no_std]
#![feature(wasm_numeric_instr)]

#[path = "../../common/common.rs"]
mod common;
use common::*;

trait Shape {
    fn area(&self) -> f64;
    fn scale(&mut self, k: f64);
    fn id(&self) -> u32;
}
struct Circle(f64);
struct Rect(f64, f64);
struct Tri(f64, f64, f64);
impl Shape for Circle {
    fn area(&self) -> f64 { 3.141592653589793 * self.0 * self.0 }
    fn scale(&mut self, k: f64) { self.0 *= k; }
    fn id(&self) -> u32 { 1 }
}
impl Shape for Rect {
    fn area(&self) -> f64 { self.0 * self.1 }
    fn scale(&mut self, k: f64) { self.0 *= k; self.1 *= k; }
    fn id(&self) -> u32 { 2 }
}
impl Shape for Tri {
    fn area(&self) -> f64 {
        let s = (self.0 + self.1 + self.2) / 2.0;
        let q = s * (s - self.0) * (s - self.1) * (s - self.2);
        if q > 0.0 { core::arch::wasm32::f64_sqrt(q) } else { 0.0 }
    }
    fn scale(&mut self, k: f64) { self.0 *= k; self.1 *= k; self.2 *= k; }
    fn id(&self) -> u32 { 3 }
}

fn op_add(a: i64, b: i64) -> i64 { a.wrapping_add(b) }
fn op_sub(a: i64, b: i64) -> i64 { a.wrapping_sub(b) }
fn op_mul(a: i64, b: i64) -> i64 { a.wrapping_mul(b) }
fn op_xor(a: i64, b: i64) -> i64 { a ^ b }
fn op_shl(a: i64, b: i64) -> i64 { a.wrapping_shl(b as u32) }
fn op_rot(a: i64, b: i64) -> i64 { a.rotate_left(b as u32) }
fn op_div(a: i64, b: i64) -> i64 { if b == 0 { a } else { a.wrapping_div(b) } }
fn op_min(a: i64, b: i64) -> i64 { a.min(b) }

static OPS: [fn(i64, i64) -> i64; 8] = [op_add, op_sub, op_mul, op_xor, op_shl, op_rot, op_div, op_min];

#[inline(never)]
fn classify(x: u32) -> u32 {
    match x % 23 {
        0 => 17, 1 => x.wrapping_mul(3), 2 => x >> 3, 3 => !x, 4 => x.rotate_left(4), 5 => 99,
        6 => x ^ 0xdead, 7 => x.wrapping_add(7), 8 => x.count_ones(), 9 => x.leading_zeros(),
        10 => 1, 11 => x & 0xff, 12 => x | 0x100, 13 => x.wrapping_sub(13), 14 => x / 14, 15 => x % 15,
        16 => x.swap_bytes(), 17 => x.reverse_bits(), 18 => 0, 19 => x.trailing_zeros(),
        20 => x.wrapping_mul(x), 21 => (x as i32 >> 5) as u32, _ => 12345,
    }
}

// A tiny bytecode VM (dense match -> br_table).
#[inline(never)]
fn vm(code: &[u8], input: i64) -> i64 {
    let mut stack = [0i64; 64];
    let mut sp = 0usize;
    let mut pc = 0usize;
    let mut steps = 0;
    stack[0] = input;
    sp += 1;
    while pc < code.len() && steps < 10000 {
        steps += 1;
        let op = code[pc];
        pc += 1;
        match op & 15 {
            0 => { stack[sp] = (op >> 4) as i64; sp += 1; }
            1 => { if sp >= 1 { stack[sp] = stack[sp - 1]; sp += 1; } }
            2 => { if sp >= 2 { sp -= 1; stack[sp - 1] = stack[sp - 1].wrapping_add(stack[sp]); } }
            3 => { if sp >= 2 { sp -= 1; stack[sp - 1] = stack[sp - 1].wrapping_mul(stack[sp]); } }
            4 => { if sp >= 2 { sp -= 1; stack[sp - 1] ^= stack[sp]; } }
            5 => { if sp >= 2 { stack.swap(sp - 1, sp - 2); } }
            6 => { if sp >= 2 { sp -= 1; } }
            7 => { if sp >= 1 && stack[sp - 1] & 1 == 1 { pc = pc.saturating_sub((op >> 4) as usize + 1); } }
            8 => { if sp >= 1 { stack[sp - 1] = stack[sp - 1].wrapping_shr(1); } }
            9 => { if sp >= 2 { sp -= 1; stack[sp - 1] = OPS[(stack[sp] & 7) as usize](stack[sp - 1], stack[sp]); } }
            10 => { if sp >= 1 { stack[sp - 1] = classify(stack[sp - 1] as u32) as i64; } }
            _ => { if sp >= 1 { stack[sp - 1] = stack[sp - 1].wrapping_neg(); } }
        }
        if sp >= 60 { sp = 1; }
    }
    stack[..sp].iter().fold(0i64, |a, b| a.wrapping_mul(31).wrapping_add(*b))
}

#[no_mangle]
pub extern "C" fn run(n: i32) -> i64 {
    puts("dispatch\n");
    let mut rng = Rng::new(n as u64);
    let mut c = [Circle(1.0), Circle(2.5)];
    let mut r = [Rect(2.0, 3.0), Rect(0.5, 8.0)];
    let mut t = [Tri(3.0, 4.0, 5.0), Tri(2.0, 2.0, 3.0)];
    let mut total = 0.0;
    let mut ids = 0u32;
    for i in 0..n as usize {
        let s: &mut dyn Shape = match rng.below(3) {
            0 => &mut c[i % 2],
            1 => &mut r[i % 2],
            _ => &mut t[i % 2],
        };
        s.scale(if i % 2 == 0 { 1.01 } else { 0.995 });
        total += s.area();
        ids = ids.wrapping_mul(3).wrapping_add(s.id());
    }
    pf64(total);
    pi32(ids as i32);
    let mut acc = 1i64;
    for i in 0..10000 {
        let f = OPS[rng.below(8) as usize];
        acc = f(acc, (rng.next_u32() % 63) as i64 + i);
        acc ^= classify(rng.next_u32()) as i64;
    }
    pi64(acc);
    let mut code = [0u8; 48];
    let mut vh = FNV0;
    for _ in 0..300 {
        for b in code.iter_mut() { *b = rng.next_u32() as u8; }
        vh = fnv(vh, &vm(&code, rng.next_u64() as i64).to_le_bytes());
    }
    pi64(vh as i64);
    acc ^ vh as i64
}

/// Reads memory at `addr` (traps if out of bounds).
#[no_mangle]
pub extern "C" fn peek(addr: u32) -> u32 {
    unsafe { core::ptr::read_volatile(addr as usize as *const u32) }
}

/// Panics (-> unreachable trap) when n is odd.
#[no_mangle]
pub extern "C" fn maybe_panic(n: i32) -> i32 {
    if n % 2 != 0 { panic!() }
    n / 2
}

#[inline(never)]
fn deep(x: u32) -> u32 {
    // Effectively never terminates for the inputs used; not tail-recursive and
    // uses no shadow stack.
    if x == 0 { return 0; }
    deep(x.wrapping_mul(2654435761).wrapping_add(1)).wrapping_mul(x | 1) ^ x
}

/// Unbounded recursion -> call stack exhausted.
#[no_mangle]
pub extern "C" fn recurse(x: u32) -> u32 { deep(x) }

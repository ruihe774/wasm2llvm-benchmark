//! A register-machine interpreter running compiled mini-programs (sieve, fib, collatz).
//! Dense `match` dispatch (br_table) and many small functions.
#![no_std]

#[path = "../../common/common.rs"]
mod common;
use common::{pi64, puts, fnv, FNV0};

#[derive(Clone, Copy)]
enum Op {
    Li(u8, i32),          // r = imm
    Add(u8, u8, u8),      // a = b + c
    Sub(u8, u8, u8),
    Mul(u8, u8, u8),
    Addi(u8, u8, i32),
    Lt(u8, u8, u8),       // a = b < c
    Eq(u8, u8, u8),
    Load(u8, u8),         // a = mem[r b]
    Store(u8, u8),        // mem[r a] = b
    Jmp(i32),
    Jz(u8, i32),
    Jnz(u8, i32),
    Out(u8),
    Shr(u8, u8, i32),
    And(u8, u8, u8),
    Halt,
}

struct Vm { r: [i32; 16], mem: [i32; 8192], out: u64, steps: u64 }

impl Vm {
    fn run(&mut self, prog: &[Op]) {
        let mut pc = 0usize;
        loop {
            self.steps += 1;
            match prog[pc] {
                Op::Li(a, v) => self.r[a as usize] = v,
                Op::Add(a, b, c) => self.r[a as usize] = self.r[b as usize].wrapping_add(self.r[c as usize]),
                Op::Sub(a, b, c) => self.r[a as usize] = self.r[b as usize].wrapping_sub(self.r[c as usize]),
                Op::Mul(a, b, c) => self.r[a as usize] = self.r[b as usize].wrapping_mul(self.r[c as usize]),
                Op::Addi(a, b, v) => self.r[a as usize] = self.r[b as usize].wrapping_add(v),
                Op::Lt(a, b, c) => self.r[a as usize] = (self.r[b as usize] < self.r[c as usize]) as i32,
                Op::Eq(a, b, c) => self.r[a as usize] = (self.r[b as usize] == self.r[c as usize]) as i32,
                Op::Load(a, b) => self.r[a as usize] = self.mem[(self.r[b as usize] as usize) & 8191],
                Op::Store(a, b) => self.mem[(self.r[a as usize] as usize) & 8191] = self.r[b as usize],
                Op::Jmp(t) => { pc = t as usize; continue; }
                Op::Jz(a, t) => if self.r[a as usize] == 0 { pc = t as usize; continue; },
                Op::Jnz(a, t) => if self.r[a as usize] != 0 { pc = t as usize; continue; },
                Op::Out(a) => self.out = (self.out ^ self.r[a as usize] as u32 as u64).wrapping_mul(0x100000001b3),
                Op::Shr(a, b, v) => self.r[a as usize] = ((self.r[b as usize] as u32) >> (v & 31)) as i32,
                Op::And(a, b, c) => self.r[a as usize] = self.r[b as usize] & self.r[c as usize],
                Op::Halt => return,
            }
            pc += 1;
        }
    }
}

use Op::*;

// sieve of Eratosthenes up to r1 (<8192); outputs count of primes
fn sieve(n: i32) -> [Op; 20] {
    [
        Li(1, n), Li(2, 2), Li(9, 1), Li(10, 0),
        /*4*/ Lt(3, 2, 1), Jz(3, 18),
        /*6*/ Load(4, 2), Jnz(4, 16),
        /*8*/ Addi(10, 10, 1), Mul(5, 2, 2),
        /*10*/ Lt(3, 5, 1), Jz(3, 16), Store(5, 9), Add(5, 5, 2), Jmp(10), Halt,
        /*16*/ Addi(2, 2, 1), Jmp(4),
        /*18*/ Out(10), Halt,
    ]
}

// iterative fib mod 2^32, n times, output each 1000th
fn fib(n: i32) -> [Op; 14] {
    [
        Li(1, 0), Li(2, 1), Li(3, n), Li(4, 0), Li(7, 1023),
        /*5*/ Lt(5, 4, 3), Jz(5, 13),
        /*7*/ Add(6, 1, 2), Add(1, 2, 0), Add(2, 6, 0), Addi(4, 4, 1),
        /*11*/ Out(2), Jmp(5),
        /*13*/ Halt,
    ]
}

// sum of Collatz trajectory lengths for 1..n
fn collatz(n: i32) -> [Op; 22] {
    [
        Li(1, 1), Li(3, n), Li(9, 1), Li(10, 0), Li(5, 3),
        /*5*/ Lt(4, 1, 3), Jz(4, 20),
        /*7*/ Add(2, 1, 0),
        /*8*/ Eq(4, 2, 9), Jnz(4, 18),
        /*10*/ And(4, 2, 9), Jnz(4, 14),
        /*12*/ Shr(2, 2, 1), Jmp(16),
        /*14*/ Mul(2, 2, 5), Addi(2, 2, 1),
        /*16*/ Addi(10, 10, 1), Jmp(8),
        /*18*/ Addi(1, 1, 1), Jmp(5),
        /*20*/ Out(10), Halt,
    ]
}

#[no_mangle]
pub extern "C" fn run(scale: i32) -> i64 {
    puts("vm\n");
    let mut vm = Vm { r: [0; 16], mem: [0; 8192], out: FNV0, steps: 0 };
    vm.run(&sieve(8000.min(500 * scale)));
    pi64(vm.out as i64);
    vm.r = [0; 16];
    vm.run(&fib(20000 * scale));
    pi64(vm.out as i64);
    vm.r = [0; 16];
    vm.run(&collatz(3000 * scale));
    let mut h = vm.out;
    for x in vm.mem[..300].iter() { h = fnv(h, &x.to_le_bytes()); }
    pi64(h as i64);
    pi64(vm.steps as i64);
    (h ^ vm.steps) as i64
}

//! Text processing: core::fmt (integers, floats via Grisu/Dragon), UTF-8 iteration,
//! case mapping, BTreeMap/BinaryHeap, sorting with closures.
#![no_std]
extern crate alloc;

#[path = "../../common/common.rs"]
mod common;
#[path = "../../common/heap.rs"]
mod heap_impl;
use common::*;

#[global_allocator]
static A: heap_impl::heap::Bump = heap_impl::heap::Bump::new();

use alloc::collections::{BTreeMap, BinaryHeap};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::cmp::Reverse;
use core::fmt::Write;

const TEXT: &str = "Ünïcödé façade naïve coöperate — Grüße aus Köln! Ελληνικά κείμενα, русский текст, 日本語のテキスト、\
中文文本。 Emoji: 🦀🚀✨ ½ ¼ ß ẞ ǅ ﬁ İstanbul ΣΑΣ";

#[no_mangle]
pub extern "C" fn run(n: i32) -> i64 {
    puts("text\n");
    let mut rng = Rng::new(n as u64);
    let mut out = String::new();

    // UTF-8 / char handling
    let upper: String = TEXT.chars().flat_map(|c| c.to_uppercase()).collect();
    let lower: String = TEXT.chars().flat_map(|c| c.to_lowercase()).collect();
    let _ = writeln!(out, "{}", upper);
    let _ = writeln!(out, "{}", lower);
    let _ = writeln!(out, "chars={} bytes={} alpha={} ws={} numeric={}", TEXT.chars().count(), TEXT.len(),
        TEXT.chars().filter(|c| c.is_alphabetic()).count(), TEXT.chars().filter(|c| c.is_whitespace()).count(),
        TEXT.chars().filter(|c| c.is_numeric()).count());
    let rev: String = TEXT.chars().rev().collect();
    let _ = writeln!(out, "{}", &rev[..rev.char_indices().nth(30).unwrap().0]);
    let _ = writeln!(out, "{:?}", &TEXT[..TEXT.char_indices().nth(12).unwrap().0]);

    // float formatting
    for _ in 0..(n as usize) {
        let x = f64::from_bits(rng.next_u64());
        if !x.is_finite() { continue; }
        let y = (rng.next_u32() as f32) / 7.0;
        let _ = write!(out, "{} {:e} {:.5} {} {:?};", x, x, y, y, (x as f32));
    }
    let _ = writeln!(out);
    let mut scale = 1e-10;
    for p in 0..20 {
        let v = scale * 1.2345678901234567;
        scale *= 10.0;
        let _ = write!(out, "{:.*} ", (p % 18) as usize, v);
    }
    let _ = writeln!(out);

    // integer formatting & parsing
    let mut nums: Vec<i64> = (0..200).map(|_| rng.next_u64() as i64 >> rng.below(64)).collect();
    nums.sort_by_key(|v| (v.count_ones(), Reverse(*v)));
    let s: Vec<String> = nums.iter().map(|v| v.to_string()).collect();
    let parsed: i64 = s.iter().map(|t| t.parse::<i64>().unwrap()).fold(0i64, |a, b| a.wrapping_add(b));
    let _ = writeln!(out, "parsed sum {} first {} last {}", parsed, s[0], s[s.len() - 1]);
    let _ = writeln!(out, "{:>12}|{:<8}|{:^9}|{:08.3}|{:#x}|{:#b}", "right", "left", "mid", -3.14159, 255, 5u8);

    // word statistics with BTreeMap + BinaryHeap top-k
    let mut freq: BTreeMap<String, usize> = BTreeMap::new();
    for word in upper.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()) {
        *freq.entry(word.to_string()).or_default() += 1;
    }
    for i in 0..(n as usize * 4) {
        let k = format!("w{}", rng.below(60) + (i % 3) as u32);
        *freq.entry(k).or_default() += 1;
    }
    let mut heap = BinaryHeap::new();
    for (w, c) in freq.iter() { heap.push((*c, Reverse(w.clone()))); }
    for _ in 0..8 {
        if let Some((c, Reverse(w))) = heap.pop() { let _ = write!(out, "{}:{} ", w, c); }
    }
    let _ = writeln!(out);

    puts(&out);
    let h = fnv(FNV0, out.as_bytes());
    pi64(h as i64);
    h as i64
}

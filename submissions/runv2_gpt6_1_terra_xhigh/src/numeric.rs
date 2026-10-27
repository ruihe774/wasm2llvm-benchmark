//! Lower scalar WebAssembly numeric instructions to poison-free LLVM helpers.
use crate::wasm::ValType::{self, F32, F64, I32, I64};
use std::fmt::Write;

pub fn signature(op: u32) -> Option<(Vec<ValType>, ValType)> {
    let (a, b, r) = match op {
        0x45 => (I32, false, I32), 0x46..=0x4f => (I32, true, I32),
        0x50 => (I64, false, I32), 0x51..=0x5a => (I64, true, I32),
        0x5b..=0x60 => (F32, true, I32), 0x61..=0x66 => (F64, true, I32),
        0x67..=0x69 => (I32, false, I32), 0x6a..=0x78 => (I32, true, I32),
        0x79..=0x7b => (I64, false, I64), 0x7c..=0x8a => (I64, true, I64),
        0x8b..=0x91 => (F32, false, F32), 0x92..=0x98 => (F32, true, F32),
        0x99..=0x9f => (F64, false, F64), 0xa0..=0xa6 => (F64, true, F64),
        0xa7 => (I64, false, I32),
        0xa8..=0xa9 => (F32, false, I32), 0xaa..=0xab => (F64, false, I32),
        0xac..=0xad => (I32, false, I64),
        0xae..=0xaf => (F32, false, I64), 0xb0..=0xb1 => (F64, false, I64),
        0xb2..=0xb3 => (I32, false, F32), 0xb4..=0xb5 => (I64, false, F32),
        0xb6 => (F64, false, F32),
        0xb7..=0xb8 => (I32, false, F64), 0xb9..=0xba => (I64, false, F64),
        0xbb => (F32, false, F64), 0xbc => (F32, false, I32), 0xbd => (F64, false, I64),
        0xbe => (I32, false, F32), 0xbf => (I64, false, F64),
        0xc0..=0xc1 => (I32, false, I32), 0xc2..=0xc4 => (I64, false, I64),
        0x100..=0x101 => (F32, false, I32), 0x102..=0x103 => (F64, false, I32),
        0x104..=0x105 => (F32, false, I64), 0x106..=0x107 => (F64, false, I64),
        _ => return None,
    };
    Some((if b { vec![a,a] } else { vec![a] }, r))
}

fn bits(t: ValType) -> &'static str { match t { I32 | F32 => "i32", I64 | F64 => "i64", _ => unreachable!() } }
fn float(t: ValType) -> &'static str { match t { F32 => "float", F64 => "double", _ => unreachable!() } }
fn suffix(t: ValType) -> &'static str { match t { I32 => "i32", I64 => "i64", F32 => "f32", F64 => "f64", _ => unreachable!() } }
fn is_float(t: ValType) -> bool { matches!(t, F32 | F64) }

pub fn declarations() -> String {
    let mut s = String::new();
    for i in ["i32", "i64"] {
        for f in ["ctlz", "cttz"] { writeln!(s, "declare {i} @llvm.{f}.{i}({i}, i1)").unwrap(); }
        writeln!(s, "declare {i} @llvm.ctpop.{i}({i})").unwrap();
        for f in ["fshl", "fshr"] { writeln!(s, "declare {i} @llvm.{f}.{i}({i}, {i}, {i})").unwrap(); }
    }
    for (n, t) in [("f32", "float"), ("f64", "double")] {
        for f in ["fabs", "ceil", "floor", "trunc", "roundeven", "sqrt"] { writeln!(s, "declare {t} @llvm.{f}.{n}({t})").unwrap(); }
        for f in ["minimum", "maximum", "copysign"] { writeln!(s, "declare {t} @llvm.{f}.{n}({t}, {t})").unwrap(); }
        for i in ["i32", "i64"] { for f in ["fptosi", "fptoui"] { writeln!(s, "declare {i} @llvm.{f}.sat.{i}.{n}({t})").unwrap(); } }
    }
    s
}

/// The input and output are bit patterns in the low bytes of 128-bit slots.
pub fn helper(op: u32) -> Result<String, String> {
    let (args, result) = signature(op).ok_or_else(|| format!("unknown scalar opcode {op:x}"))?;
    let a = args[0];
    let bt = bits(a);
    let mut s = format!("define internal i128 @w2l_num_{op:x}(i128 %a, i128 %b) {{\nentry:\n");
    writeln!(s, "  %ax = trunc i128 %a to {bt}").unwrap();
    if is_float(a) { writeln!(s, "  %x = bitcast {bt} %ax to {}", float(a)).unwrap(); }
    else { writeln!(s, "  %x = add {bt} %ax, 0").unwrap(); }
    if args.len() == 2 {
        writeln!(s, "  %by = trunc i128 %b to {bt}").unwrap();
        if is_float(a) { writeln!(s, "  %y = bitcast {bt} %by to {}", float(a)).unwrap(); }
        else { writeln!(s, "  %y = add {bt} %by, 0").unwrap(); }
    }
    match op {
        0x45 | 0x50 => { writeln!(s, "  %cmp = icmp eq {bt} %x, 0\n  %r = zext i1 %cmp to i32").unwrap(); }
        0x46..=0x4f | 0x51..=0x5a => {
            let base = if op <= 0x4f { 0x46 } else { 0x51 };
            let pred = ["eq", "ne", "slt", "ult", "sgt", "ugt", "sle", "ule", "sge", "uge"][(op-base) as usize];
            writeln!(s, "  %cmp = icmp {pred} {bt} %x, %y\n  %r = zext i1 %cmp to i32").unwrap();
        }
        0x5b..=0x66 => {
            let base = if op <= 0x60 { 0x5b } else { 0x61 };
            let pred = ["oeq", "une", "olt", "ogt", "ole", "oge"][(op-base) as usize];
            writeln!(s, "  %cmp = fcmp {pred} {} %x, %y\n  %r = zext i1 %cmp to i32", float(a)).unwrap();
        }
        0x67..=0x78 | 0x79..=0x8a => {
            let n = op - if op <= 0x78 { 0x67 } else { 0x79 };
            match n {
                0 | 1 => { let f = if n==0 {"ctlz"} else {"cttz"}; writeln!(s, "  %r = call {bt} @llvm.{f}.{bt}({bt} %x, i1 false)").unwrap(); }
                2 => writeln!(s, "  %r = call {bt} @llvm.ctpop.{bt}({bt} %x)").unwrap(),
                3..=5 | 10..=12 => { let inst = ["add","sub","mul","","","","","and","or","xor"][(n-3) as usize]; writeln!(s, "  %r = {inst} {bt} %x, %y").unwrap(); }
                6..=9 => {
                    writeln!(s, "  %zero = icmp eq {bt} %y, 0\n  br i1 %zero, label %trap_zero, label %nonzero\ntrap_zero:\n  call void @w2l_trap(i32 1)\n  unreachable\nnonzero:").unwrap();
                    let signed = n==6 || n==8;
                    if signed {
                        let min = if a == I32 {"-2147483648"} else {"-9223372036854775808"};
                        writeln!(s, "  %min = icmp eq {bt} %x, {min}\n  %minusone = icmp eq {bt} %y, -1\n  %overflow = and i1 %min, %minusone").unwrap();
                        if n==6 { writeln!(s, "  br i1 %overflow, label %trap_overflow, label %do_div\ntrap_overflow:\n  call void @w2l_trap(i32 2)\n  unreachable\ndo_div:\n  %r = sdiv {bt} %x, %y").unwrap(); }
                        else { writeln!(s, "  %safe_y = select i1 %overflow, {bt} 1, {bt} %y\n  %r = srem {bt} %x, %safe_y").unwrap(); }
                    } else { let inst = if n==7 {"udiv"} else {"urem"}; writeln!(s, "  %r = {inst} {bt} %x, %y").unwrap(); }
                }
                13..=15 => { let mask = if a==I32 {31} else {63}; let inst = ["shl","ashr","lshr"][(n-13) as usize]; writeln!(s, "  %count = and {bt} %y, {mask}\n  %r = {inst} {bt} %x, %count").unwrap(); }
                16..=17 => { let f = if n==16 {"fshl"} else {"fshr"}; writeln!(s, "  %r = call {bt} @llvm.{f}.{bt}({bt} %x, {bt} %x, {bt} %y)").unwrap(); }
                _ => unreachable!(),
            }
        }
        0x8b..=0xa6 => {
            let base = if op <= 0x98 { 0x8b } else { 0x99 }; let n = op-base;
            let ft=float(a); let suff=suffix(a);
            match n {
                0 => writeln!(s,"  %r = call {ft} @llvm.fabs.{suff}({ft} %x)").unwrap(),
                1 => writeln!(s,"  %r = fneg {ft} %x").unwrap(),
                2..=6 => { let f=["ceil","floor","trunc","roundeven","sqrt"][(n-2) as usize]; writeln!(s,"  %r = call {ft} @llvm.{f}.{suff}({ft} %x)").unwrap(); }
                7..=10 => { let f=["fadd","fsub","fmul","fdiv"][(n-7) as usize]; writeln!(s,"  %r = {f} {ft} %x, %y").unwrap(); }
                11..=13 => { let f=["minimum","maximum","copysign"][(n-11) as usize]; writeln!(s,"  %r = call {ft} @llvm.{f}.{suff}({ft} %x, {ft} %y)").unwrap(); }
                _ => unreachable!(),
            }
        }
        0xa7 => writeln!(s,"  %r = trunc i64 %x to i32").unwrap(),
        0xac | 0xad => { let f=if op==0xac {"sext"} else {"zext"}; writeln!(s,"  %r = {f} i32 %x to i64").unwrap(); }
        0xa8..=0xab | 0xae..=0xb1 | 0x100..=0x107 => {
            let signed = match op { 0xa8|0xaa|0xae|0xb0|0x100|0x102|0x104|0x106=>true, _=>false };
            let f=if signed {"fptosi"} else {"fptoui"}; let out=bits(result); let ft=float(a); let fs=suffix(a);
            if op < 0x100 {
                writeln!(s,"  %isnan = fcmp uno {ft} %x, %x\n  br i1 %isnan, label %trap_nan, label %range\ntrap_nan:\n  call void @w2l_trap(i32 3)\n  unreachable\nrange:").unwrap();
                // truncation first makes the open lower endpoint (-1 for unsigned,
                // MIN-1 for signed) exact even when the source is f64.
                if a==F32 { writeln!(s,"  %wide = fpext float %x to double").unwrap(); } else { writeln!(s,"  %wide = fadd double %x, 0.000000e+00").unwrap(); }
                writeln!(s,"  %truncated = call double @llvm.trunc.f64(double %wide)").unwrap();
                let (lo,hi) = match (result,signed) {
                    (I32,true) => ("-2.147483648000000e+09","2.147483648000000e+09"),
                    (I32,false) => ("0.000000e+00","4.294967296000000e+09"),
                    (I64,true) => ("-9.223372036854775808000000e+18","9.223372036854775808000000e+18"),
                    _ => ("0.000000e+00","1.844674407370955161600000e+19"),
                };
                writeln!(s,"  %low = fcmp olt double %truncated, {lo}\n  %high = fcmp oge double %truncated, {hi}\n  %bad = or i1 %low, %high\n  br i1 %bad, label %trap_range, label %convert\ntrap_range:\n  call void @w2l_trap(i32 2)\n  unreachable\nconvert:").unwrap();
            }
            writeln!(s,"  %r = call {out} @llvm.{f}.sat.{out}.{fs}({ft} %x)").unwrap();
        }
        0xb2..=0xb5 | 0xb7..=0xba => {
            let signed=matches!(op,0xb2|0xb4|0xb7|0xb9); let f=if signed {"sitofp"} else {"uitofp"};
            writeln!(s,"  %r = {f} {bt} %x to {}",float(result)).unwrap();
        }
        0xb6 => writeln!(s,"  %r = fptrunc double %x to float").unwrap(),
        0xbb => writeln!(s,"  %r = fpext float %x to double").unwrap(),
        0xbc..=0xbf => { // Reinterpretation is already represented in raw bits.
            writeln!(s,"  %r = bitcast {} %x to {}", if is_float(a) {float(a)} else {bits(a)}, if is_float(result) {float(result)} else {bits(result)}).unwrap();
        }
        0xc0..=0xc4 => {
            let narrow = match op {0xc0|0xc2=>"i8", 0xc1|0xc3=>"i16", _=>"i32"};
            writeln!(s,"  %narrow = trunc {bt} %x to {narrow}\n  %r = sext {narrow} %narrow to {bt}").unwrap();
        }
        _ => return Err(format!("unimplemented scalar opcode {op:x}")),
    }
    let rt=bits(result);
    if is_float(result) { writeln!(s,"  %rbits = bitcast {} %r to {rt}",float(result)).unwrap(); }
    else { writeln!(s,"  %rbits = add {rt} %r, 0").unwrap(); }
    writeln!(s,"  %wide_result = zext {rt} %rbits to i128\n  ret i128 %wide_result\n}}\n").unwrap();
    Ok(s)
}

use crate::{
    function::FunctionEmitter,
    wasm::ValType::{self, F32, F64, I32, I64},
};

pub fn emit_numeric(f: &mut FunctionEmitter<'_>, op: u8) -> Result<(), String> {
    match op {
        0x45 => {
            let a = f.pop(I32)?;
            let c = f.fresh();
            f.line(&format!("{c} = icmp eq i32 {a}, 0"));
            let r = f.fresh();
            f.line(&format!("{r} = zext i1 {c} to i32"));
            f.push(I32, &r)?;
        }
        0x46..=0x4f => emit_int_compare(f, op, I32, op - 0x46)?,
        0x50 => {
            let a = f.pop(I64)?;
            let c = f.fresh();
            f.line(&format!("{c} = icmp eq i64 {a}, 0"));
            let r = f.fresh();
            f.line(&format!("{r} = zext i1 {c} to i32"));
            f.push(I32, &r)?;
        }
        0x51..=0x5a => emit_int_compare(f, op, I64, op - 0x51)?,
        0x5b..=0x60 => emit_float_compare(f, op, F32, op - 0x5b)?,
        0x61..=0x66 => emit_float_compare(f, op, F64, op - 0x61)?,
        0x67..=0x69 => emit_integer_unary(f, op, I32)?,
        0x6a..=0x78 => emit_integer_binary(f, op, I32)?,
        0x79..=0x7b => emit_integer_unary(f, op, I64)?,
        0x7c..=0x8a => emit_integer_binary(f, op, I64)?,
        0x8b..=0x91 => emit_float_unary(f, op, F32)?,
        0x92..=0x98 => emit_float_binary(f, op, F32)?,
        0x99..=0x9f => emit_float_unary(f, op, F64)?,
        0xa0..=0xa6 => emit_float_binary(f, op, F64)?,
        0xa7..=0xbf => emit_conversion(f, op)?,
        _ => return Err(format!("unsupported numeric opcode 0x{op:02x}")),
    }
    Ok(())
}

pub fn emit_sign_extend(f: &mut FunctionEmitter<'_>, op: u8) -> Result<(), String> {
    let (ty, narrow) = match op {
        0xc0 => (I32, "i8"), 0xc1 => (I32, "i16"), 0xc2 => (I64, "i8"),
        0xc3 => (I64, "i16"), 0xc4 => (I64, "i32"),
        _ => return Err("invalid sign-extension opcode".into()),
    };
    let value = f.pop(ty)?;
    let small = f.fresh();
    f.line(&format!("{small} = trunc {} {value} to {narrow}", ty.llvm()));
    let result = f.fresh();
    f.line(&format!("{result} = sext {narrow} {small} to {}", ty.llvm()));
    f.push(ty, &result)
}

pub fn emit_saturating(f: &mut FunctionEmitter<'_>, op: u32) -> Result<(), String> {
    let (input, output, signed) = match op {
        0 => (F32, I32, true), 1 => (F32, I32, false),
        2 => (F64, I32, true), 3 => (F64, I32, false),
        4 => (F32, I64, true), 5 => (F32, I64, false),
        6 => (F64, I64, true), 7 => (F64, I64, false),
        _ => return Err("invalid saturating conversion opcode".into()),
    };
    let value = f.pop(input)?;
    let name = format!("@llvm.{}{}.sat.{}.{}",
        if signed { "fptosi" } else { "fptoui" },
        "", output.llvm(), if input == F32 { "f32" } else { "f64" });
    let result = f.fresh();
    f.line(&format!("{result} = call {} {name}({} {value})", output.llvm(), input.llvm()));
    f.push(output, &result)
}

fn emit_int_compare(f: &mut FunctionEmitter<'_>, op: u8, ty: ValType, pred: u8) -> Result<(), String> {
    let b = f.pop(ty)?;
    let a = f.pop(ty)?;
    let cond = match pred {
        0 => "eq", 1 => "ne", 2 => "slt", 3 => "ult", 4 => "sgt",
        5 => "ugt", 6 => "sle", 7 => "ule", 8 => "sge", _ => "uge",
    };
    let c = f.fresh();
    f.line(&format!("{c} = icmp {cond} {} {a}, {b}", ty.llvm()));
    let r = f.fresh();
    f.line(&format!("{r} = zext i1 {c} to i32"));
    let _ = op;
    f.push(I32, &r)
}

fn emit_float_compare(f: &mut FunctionEmitter<'_>, op: u8, ty: ValType, pred: u8) -> Result<(), String> {
    let b = f.pop(ty)?;
    let a = f.pop(ty)?;
    let cond = match pred { 0 => "oeq", 1 => "une", 2 => "olt", 3 => "ogt", 4 => "ole", _ => "oge" };
    let c = f.fresh();
    f.line(&format!("{c} = fcmp {cond} {} {a}, {b}", ty.llvm()));
    let r = f.fresh();
    f.line(&format!("{r} = zext i1 {c} to i32"));
    let _ = op;
    f.push(I32, &r)
}

fn emit_integer_unary(f: &mut FunctionEmitter<'_>, op: u8, ty: ValType) -> Result<(), String> {
    let a = f.pop(ty)?;
    let result = f.fresh();
    let intrinsic = match op {
        0x67 | 0x79 => "ctlz",
        0x68 | 0x7a => "cttz",
        0x69 | 0x7b => "ctpop",
        _ => return Err("invalid integer unary opcode".into()),
    };
    f.line(&format!("{result} = call {} @llvm.{intrinsic}.{}({} {a}{})", ty.llvm(), ty.llvm(), ty.llvm(), if intrinsic == "ctpop" { "" } else { ", i1 false" }));
    f.push(ty, &result)
}

fn emit_integer_binary(f: &mut FunctionEmitter<'_>, op: u8, ty: ValType) -> Result<(), String> {
    let b = f.pop(ty)?;
    let a = f.pop(ty)?;
    let bits = if ty == I32 { 32 } else { 64 };
    let word = ty.llvm();
    let result = match op {
        0x6a | 0x7c => binary(f, "add", ty, &a, &b),
        0x6b | 0x7d => binary(f, "sub", ty, &a, &b),
        0x6c | 0x7e => binary(f, "mul", ty, &a, &b),
        0x6d | 0x7f => {
            let zero = f.fresh();
            f.line(&format!("{zero} = icmp eq {word} {b}, 0"));
            f.emit_trap_if(&zero, 1);
            let min = if bits == 32 { i32::MIN.to_string() } else { i64::MIN.to_string() };
            let min_cmp = f.fresh();
            f.line(&format!("{min_cmp} = icmp eq {word} {a}, {min}"));
            let neg_one = f.fresh();
            f.line(&format!("{neg_one} = icmp eq {word} {b}, -1"));
            let overflow = f.fresh();
            f.line(&format!("{overflow} = and i1 {min_cmp}, {neg_one}"));
            f.emit_trap_if(&overflow, 2);
            binary(f, "sdiv", ty, &a, &b)
        }
        0x6e | 0x80 => {
            let zero = f.fresh();
            f.line(&format!("{zero} = icmp eq {word} {b}, 0"));
            f.emit_trap_if(&zero, 1);
            binary(f, "udiv", ty, &a, &b)
        }
        0x6f | 0x81 => emit_signed_remainder(f, ty, &a, &b)?,
        0x70 | 0x82 => {
            let zero = f.fresh();
            f.line(&format!("{zero} = icmp eq {word} {b}, 0"));
            f.emit_trap_if(&zero, 1);
            binary(f, "urem", ty, &a, &b)
        }
        0x71 | 0x83 => binary(f, "and", ty, &a, &b),
        0x72 | 0x84 => binary(f, "or", ty, &a, &b),
        0x73 | 0x85 => binary(f, "xor", ty, &a, &b),
        0x74 | 0x86 => emit_shift(f, ty, &a, &b, "shl", bits - 1),
        0x75 | 0x87 => emit_shift(f, ty, &a, &b, "ashr", bits - 1),
        0x76 | 0x88 => emit_shift(f, ty, &a, &b, "lshr", bits - 1),
        0x77 | 0x89 => emit_rotate(f, ty, &a, &b, bits, true),
        0x78 | 0x8a => emit_rotate(f, ty, &a, &b, bits, false),
        _ => return Err("invalid integer binary opcode".into()),
    };
    f.push(ty, &result)
}

fn binary(f: &mut FunctionEmitter<'_>, op: &str, ty: ValType, a: &str, b: &str) -> String {
    let result = f.fresh();
    f.line(&format!("{result} = {op} {} {a}, {b}", ty.llvm()));
    result
}

fn emit_signed_remainder(f: &mut FunctionEmitter<'_>, ty: ValType, a: &str, b: &str) -> Result<String, String> {
    let word = ty.llvm();
    let zero = f.fresh();
    f.line(&format!("{zero} = icmp eq {word} {b}, 0"));
    f.emit_trap_if(&zero, 1);
    let negative_a = f.fresh();
    f.line(&format!("{negative_a} = icmp slt {word} {a}, 0"));
    let neg_a = f.fresh();
    f.line(&format!("{neg_a} = sub {word} 0, {a}"));
    let abs_a = f.fresh();
    f.line(&format!("{abs_a} = select i1 {negative_a}, {word} {neg_a}, {word} {a}"));
    let negative_b = f.fresh();
    f.line(&format!("{negative_b} = icmp slt {word} {b}, 0"));
    let neg_b = f.fresh();
    f.line(&format!("{neg_b} = sub {word} 0, {b}"));
    let abs_b = f.fresh();
    f.line(&format!("{abs_b} = select i1 {negative_b}, {word} {neg_b}, {word} {b}"));
    let rem = f.fresh();
    f.line(&format!("{rem} = urem {word} {abs_a}, {abs_b}"));
    let neg_rem = f.fresh();
    f.line(&format!("{neg_rem} = sub {word} 0, {rem}"));
    let result = f.fresh();
    f.line(&format!("{result} = select i1 {negative_a}, {word} {neg_rem}, {word} {rem}"));
    Ok(result)
}

fn emit_shift(f: &mut FunctionEmitter<'_>, ty: ValType, a: &str, b: &str, op: &str, mask: u32) -> String {
    let count = f.fresh();
    f.line(&format!("{count} = and {} {b}, {mask}", ty.llvm()));
    let result = f.fresh();
    f.line(&format!("{result} = {op} {} {a}, {count}", ty.llvm()));
    result
}

fn emit_rotate(f: &mut FunctionEmitter<'_>, ty: ValType, a: &str, b: &str, bits: u32, left: bool) -> String {
    let mask = bits - 1;
    let count = f.fresh();
    f.line(&format!("{count} = and {} {b}, {mask}", ty.llvm()));
    let neg = f.fresh();
    f.line(&format!("{neg} = sub {} 0, {count}", ty.llvm()));
    let rev = f.fresh();
    f.line(&format!("{rev} = and {} {neg}, {mask}", ty.llvm()));
    let (op1, op2) = if left { ("shl", "lshr") } else { ("lshr", "shl") };
    let x = f.fresh();
    f.line(&format!("{x} = {op1} {} {a}, {count}", ty.llvm()));
    let y = f.fresh();
    f.line(&format!("{y} = {op2} {} {a}, {rev}", ty.llvm()));
    let result = f.fresh();
    f.line(&format!("{result} = or {} {x}, {y}", ty.llvm()));
    result
}

fn emit_float_unary(f: &mut FunctionEmitter<'_>, op: u8, ty: ValType) -> Result<(), String> {
    let value = f.pop(ty)?;
    let bits = if ty == F32 { 32 } else { 64 };
    let intty = if bits == 32 { "i32" } else { "i64" };
    let sign = if bits == 32 { i32::MIN as i64 } else { i64::MIN };
    let result = match op {
        0x8b | 0x99 => {
            let int_value = f.fresh();
            f.line(&format!("{int_value} = bitcast {} {value} to {intty}", ty.llvm()));
            let abs = f.fresh();
            let mask = if bits == 32 { "2147483647" } else { "9223372036854775807" };
            f.line(&format!("{abs} = and {intty} {int_value}, {mask}"));
            let r = f.fresh();
            f.line(&format!("{r} = bitcast {intty} {abs} to {}", ty.llvm()));
            r
        }
        0x8c | 0x9a => {
            let int_value = f.fresh();
            f.line(&format!("{int_value} = bitcast {} {value} to {intty}", ty.llvm()));
            let neg = f.fresh();
            f.line(&format!("{neg} = xor {intty} {int_value}, {sign}"));
            let r = f.fresh();
            f.line(&format!("{r} = bitcast {intty} {neg} to {}", ty.llvm()));
            r
        }
        _ => {
            let intrinsic = match op {
                0x8d | 0x9b => "ceil", 0x8e | 0x9c => "floor",
                0x8f | 0x9d => "trunc", 0x90 | 0x9e => "roundeven",
                0x91 | 0x9f => "sqrt", _ => return Err("invalid float unary opcode".into()),
            };
            let result = f.fresh();
            f.line(&format!("{result} = call {} @llvm.{intrinsic}.{}({} {value})", ty.llvm(), if ty == F32 { "f32" } else { "f64" }, ty.llvm()));
            result
        }
    };
    f.push(ty, &result)
}

fn emit_float_binary(f: &mut FunctionEmitter<'_>, op: u8, ty: ValType) -> Result<(), String> {
    let b = f.pop(ty)?;
    let a = f.pop(ty)?;
    let result = match op {
        0x92 | 0xa0 => binary(f, "fadd", ty, &a, &b),
        0x93 | 0xa1 => binary(f, "fsub", ty, &a, &b),
        0x94 | 0xa2 => binary(f, "fmul", ty, &a, &b),
        0x95 | 0xa3 => binary(f, "fdiv", ty, &a, &b),
        0x96 | 0xa4 => emit_float_minmax(f, ty, &a, &b, false),
        0x97 | 0xa5 => emit_float_minmax(f, ty, &a, &b, true),
        0x98 | 0xa6 => emit_copysign(f, ty, &a, &b),
        _ => return Err("invalid float binary opcode".into()),
    };
    f.push(ty, &result)
}

fn emit_float_minmax(f: &mut FunctionEmitter<'_>, ty: ValType, a: &str, b: &str, maximum: bool) -> String {
    let (intty, suffix, sign, canonical_nan) = if ty == F32 {
        ("i32", "float", i32::MIN as i64, 0x7fc0_0000u32 as i64)
    } else {
        ("i64", "double", i64::MIN, 0x7ff8_0000_0000_0000u64 as i64)
    };
    let nan_a = f.fresh();
    let nan_b = f.fresh();
    f.line(&format!("{nan_a} = fcmp uno {suffix} {a}, {a}"));
    f.line(&format!("{nan_b} = fcmp uno {suffix} {b}, {b}"));
    let any_nan = f.fresh();
    f.line(&format!("{any_nan} = or i1 {nan_a}, {nan_b}"));
    let cmp = f.fresh();
    f.line(&format!("{cmp} = fcmp {} {suffix} {a}, {b}", if maximum { "ogt" } else { "olt" }));
    let ordered = f.fresh();
    f.line(&format!("{ordered} = select i1 {cmp}, {suffix} {a}, {suffix} {b}"));
    let a_zero = f.fresh();
    let b_zero = f.fresh();
    f.line(&format!("{a_zero} = fcmp oeq {suffix} {a}, 0.0"));
    f.line(&format!("{b_zero} = fcmp oeq {suffix} {b}, 0.0"));
    let both_zero = f.fresh();
    f.line(&format!("{both_zero} = and i1 {a_zero}, {b_zero}"));
    let a_bits = f.fresh();
    let b_bits = f.fresh();
    f.line(&format!("{a_bits} = bitcast {suffix} {a} to {intty}"));
    f.line(&format!("{b_bits} = bitcast {suffix} {b} to {intty}"));
    let a_sign = f.fresh();
    let b_sign = f.fresh();
    f.line(&format!("{a_sign} = and {intty} {a_bits}, {sign}"));
    f.line(&format!("{b_sign} = and {intty} {b_bits}, {sign}"));
    let zero_sign = f.fresh();
    f.line(&format!("{zero_sign} = {} {intty} {a_sign}, {b_sign}", if maximum { "and" } else { "or" }));
    let zero = f.fresh();
    f.line(&format!("{zero} = bitcast {intty} {zero_sign} to {suffix}"));
    let with_zero = f.fresh();
    f.line(&format!("{with_zero} = select i1 {both_zero}, {suffix} {zero}, {suffix} {ordered}"));
    let nan = f.fresh();
    f.line(&format!("{nan} = bitcast {intty} {canonical_nan} to {suffix}"));
    let result = f.fresh();
    f.line(&format!("{result} = select i1 {any_nan}, {suffix} {nan}, {suffix} {with_zero}"));
    result
}

fn emit_copysign(f: &mut FunctionEmitter<'_>, ty: ValType, magnitude: &str, sign_source: &str) -> String {
    let (intty, suffix, sign_mask, abs_mask) = if ty == F32 {
        ("i32", "float", i32::MIN as i64, i32::MAX as i64)
    } else {
        ("i64", "double", i64::MIN, i64::MAX)
    };
    let mag = f.fresh();
    let sign = f.fresh();
    f.line(&format!("{mag} = bitcast {suffix} {magnitude} to {intty}"));
    f.line(&format!("{sign} = bitcast {suffix} {sign_source} to {intty}"));
    let mag2 = f.fresh();
    let sign2 = f.fresh();
    f.line(&format!("{mag2} = and {intty} {mag}, {abs_mask}"));
    f.line(&format!("{sign2} = and {intty} {sign}, {sign_mask}"));
    let bits = f.fresh();
    f.line(&format!("{bits} = or {intty} {mag2}, {sign2}"));
    let result = f.fresh();
    f.line(&format!("{result} = bitcast {intty} {bits} to {suffix}"));
    result
}

fn emit_conversion(f: &mut FunctionEmitter<'_>, op: u8) -> Result<(), String> {
    match op {
        0xa7 => { let value = f.pop(I64)?; let r = f.fresh(); f.line(&format!("{r} = trunc i64 {value} to i32")); f.push(I32, &r) }
        0xa8..=0xab | 0xae..=0xb1 => emit_trapping_float_to_int(f, op),
        0xac | 0xad => {
            let value = f.pop(I32)?; let r = f.fresh();
            f.line(&format!("{r} = {} i32 {value} to i64", if op == 0xac { "sext" } else { "zext" }));
            f.push(I64, &r)
        }
        0xb2..=0xb5 => {
            let (input, signed) = if op == 0xb2 || op == 0xb3 { (I32, op == 0xb2) } else { (I64, op == 0xb4) };
            let value = f.pop(input)?; let r = f.fresh();
            f.line(&format!("{r} = {} {} {value} to float", if signed { "sitofp" } else { "uitofp" }, input.llvm()));
            f.push(F32, &r)
        }
        0xb6 => {
            let value = f.pop(F64)?; let r = f.fresh(); f.line(&format!("{r} = fptrunc double {value} to float")); f.push(F32, &r)
        }
        0xb7..=0xba => {
            let (input, signed) = if op == 0xb7 || op == 0xb8 { (I32, op == 0xb7) } else { (I64, op == 0xb9) };
            let value = f.pop(input)?; let r = f.fresh();
            f.line(&format!("{r} = {} {} {value} to double", if signed { "sitofp" } else { "uitofp" }, input.llvm()));
            f.push(F64, &r)
        }
        0xbb => {
            let value = f.pop(F32)?; let r = f.fresh(); f.line(&format!("{r} = fpext float {value} to double")); f.push(F64, &r)
        }
        0xbc => bitcast(f, F32, I32),
        0xbd => bitcast(f, F64, I64),
        0xbe => bitcast(f, I32, F32),
        0xbf => bitcast(f, I64, F64),
        _ => Err("invalid numeric conversion opcode".into()),
    }
}

fn bitcast(f: &mut FunctionEmitter<'_>, input: ValType, output: ValType) -> Result<(), String> {
    let value = f.pop(input)?;
    let result = f.fresh();
    f.line(&format!("{result} = bitcast {} {value} to {}", input.llvm(), output.llvm()));
    f.push(output, &result)
}

fn emit_trapping_float_to_int(f: &mut FunctionEmitter<'_>, op: u8) -> Result<(), String> {
    let (input, output, signed) = match op {
        0xa8 => (F32, I32, true), 0xa9 => (F32, I32, false),
        0xaa => (F64, I32, true), 0xab => (F64, I32, false),
        0xae => (F32, I64, true), 0xaf => (F32, I64, false),
        0xb0 => (F64, I64, true), 0xb1 => (F64, I64, false),
        _ => return Err("invalid trapping conversion opcode".into()),
    };
    let value = f.pop(input)?;
    let is_nan = f.fresh();
    f.line(&format!("{is_nan} = fcmp uno {} {value}, {value}", input.llvm()));
    f.emit_trap_if(&is_nan, 3);
    let (lower_bits, upper_bits, lower_out) = float_range_bits(input, output, signed);
    let lower = bitcast_float_constant(f, input, lower_bits);
    let upper = bitcast_float_constant(f, input, upper_bits);
    let low_bad = f.fresh();
    let high_bad = f.fresh();
    f.line(&format!("{low_bad} = fcmp {} {} {value}, {lower}", if lower_out { "ole" } else { "olt" }, input.llvm()));
    f.line(&format!("{high_bad} = fcmp oge {} {value}, {upper}", input.llvm()));
    let bad = f.fresh();
    f.line(&format!("{bad} = or i1 {low_bad}, {high_bad}"));
    f.emit_trap_if(&bad, 2);
    let result = f.fresh();
    f.line(&format!("{result} = {} {} {value} to {}", if signed { "fptosi" } else { "fptoui" }, input.llvm(), output.llvm()));
    f.push(output, &result)
}

fn float_range_bits(input: ValType, output: ValType, signed: bool) -> (u64, u64, bool) {
    match (input, output, signed) {
        (F32, I32, true) => (0xcf00_0000, 0x4f00_0000, false),
        (F32, I32, false) => (0xbf80_0000, 0x4f80_0000, true),
        (F32, I64, true) => (0xdf00_0000, 0x5f00_0000, false),
        (F32, I64, false) => (0xbf80_0000, 0x5f80_0000, true),
        (F64, I32, true) => (0xc1e0_0000_0000_0000, 0x41e0_0000_0000_0000, false),
        (F64, I32, false) => (0xbff0_0000_0000_0000, 0x41f0_0000_0000_0000, true),
        (F64, I64, true) => (0xc3e0_0000_0000_0000, 0x43e0_0000_0000_0000, false),
        (F64, I64, false) => (0xbff0_0000_0000_0000, 0x43f0_0000_0000_0000, true),
        _ => unreachable!(),
    }
}

fn bitcast_float_constant(f: &mut FunctionEmitter<'_>, ty: ValType, bits: u64) -> String {
    let result = f.fresh();
    if ty == F32 {
        f.line(&format!("{result} = bitcast i32 {} to float", crate::codegen::i32_bits(bits as u32)));
    } else {
        f.line(&format!("{result} = bitcast i64 {} to double", crate::codegen::i64_bits(bits)));
    }
    result
}

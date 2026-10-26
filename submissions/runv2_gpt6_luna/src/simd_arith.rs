use crate::{
    function::FunctionEmitter,
    simd_codegen::{cast_vector, lane_info, splat_vector, LaneInfo},
    wasm::ValType,
};

pub fn emit_arithmetic(f: &mut FunctionEmitter<'_>, op: u32) -> Result<(), String> {
    match op {
        0x23..=0x4c => unreachable!(),
        0x60 | 0x61 | 0x80 | 0x81 | 0xa0 | 0xa1 | 0xc0 | 0xc1 => {
            let lane = lane_info(lane_kind(op));
            emit_int_abs_neg(f, op, lane)
        }
        0x62 => emit_popcnt(f),
        0x63 | 0x83 | 0xa3 | 0xc3 => emit_all_true(f, op),
        0x64 | 0x84 | 0xa4 | 0xc4 => emit_bitmask(f, op),
        0x65 | 0x66 | 0x85 | 0x86 => emit_narrow(f, op),
        0x67..=0x6a | 0x74..=0x75 | 0x7a | 0x94 => emit_float_round(f, op),
        0x6b..=0x6d | 0x8b..=0x8d | 0xab..=0xad | 0xcb..=0xcd => emit_shift(f, op),
        0x6f | 0x70 | 0x72 | 0x73 | 0x8f | 0x90 | 0x92 | 0x93 => emit_saturating_add_sub(f, op),
        0x82 => emit_q15mulr(f),
        0x7b | 0x9b => emit_average(f, op),
        0x7c..=0x7f => emit_pairwise_add(f, op),
        0x87..=0x8a | 0xa7..=0xaa | 0xc7..=0xca => emit_extend(f, op),
        0x9c..=0x9f | 0xbc..=0xbf | 0xdc..=0xdf => emit_extmul(f, op),
        0xba => emit_dot(f),
        0xd6..=0xdb => emit_i64_compare(f, op),
        0x76..=0x79 | 0x96..=0x99 | 0xb6..=0xb9 => emit_int_minmax(f, op),
        0xe8..=0xeb | 0xf4..=0xf7 => emit_float_minmax(f, op),
        0xe0 | 0xe1 | 0xec | 0xed => emit_float_abs_neg(f, op),
        0xe3 | 0xef => emit_float_round(f, op),
        0xe4..=0xe7 | 0xf0..=0xf3 => emit_float_binary(f, op),
        0x6e | 0x71 | 0x8e | 0x91 | 0x95 | 0xae | 0xb1 | 0xb5 | 0xce | 0xd1 | 0xd5 => emit_int_binary(f, op),
        _ => Err(format!("unsupported SIMD arithmetic opcode 0x{op:x}")),
    }
}

fn lane_kind(op: u32) -> &'static str {
    match op {
        0x67..=0x6a | 0xe0..=0xeb => "f32",
        0x74..=0x75 | 0x7a | 0x94 | 0xec..=0xf7 => "f64",
        0x60..=0x7d => "i8",
        0x7e..=0x9f => "i16",
        0xa0..=0xbf => "i32",
        0xc0..=0xdf => "i64",
        _ => unreachable!(),
    }
}

fn vector_args(f: &mut FunctionEmitter<'_>, count: usize) -> Result<Vec<String>, String> {
    f.pop_values(&vec![ValType::V128; count])
}

fn as_lane(f: &mut FunctionEmitter<'_>, value: &str, lane: LaneInfo) -> String {
    cast_vector(f, value, "<16 x i8>", lane.vector)
}

fn push_lane(f: &mut FunctionEmitter<'_>, value: &str, lane: LaneInfo) -> Result<(), String> {
    let result = cast_vector(f, value, lane.vector, "<16 x i8>");
    f.push(ValType::V128, &result)
}

fn splat_constant(lane: LaneInfo, value: i64) -> String {
    let elements = (0..lane.lanes).map(|_| format!("{} {value}", lane.scalar)).collect::<Vec<_>>().join(", ");
    format!("<{elements}>")
}

fn emit_int_abs_neg(f: &mut FunctionEmitter<'_>, op: u32, lane: LaneInfo) -> Result<(), String> {
    let args = vector_args(f, 1)?;
    let value = as_lane(f, &args[0], lane);
    let result = f.fresh();
    if op & 1 == 1 {
        f.line(&format!("{result} = sub {} zeroinitializer, {value}", lane.vector));
    } else {
        let negative = f.fresh();
        f.line(&format!("{negative} = icmp slt {} {value}, zeroinitializer", lane.vector));
        let negated = f.fresh();
        f.line(&format!("{negated} = sub {} zeroinitializer, {value}", lane.vector));
        f.line(&format!("{result} = select <{} x i1> {negative}, {} {negated}, {} {value}", lane.lanes, lane.vector, lane.vector));
    }
    push_lane(f, &result, lane)
}

fn emit_popcnt(f: &mut FunctionEmitter<'_>) -> Result<(), String> {
    let args = vector_args(f, 1)?;
    let lane = lane_info("i8");
    let value = as_lane(f, &args[0], lane);
    let result = f.fresh();
    f.line(&format!("{result} = call <16 x i8> @llvm.ctpop.v16i8(<16 x i8> {value})"));
    push_lane(f, &result, lane)
}

fn reduction_lane(op: u32) -> LaneInfo {
    lane_info(match op { 0x63 | 0x64 => "i8", 0x83 | 0x84 => "i16", 0xa3 | 0xa4 => "i32", _ => "i64" })
}

fn emit_all_true(f: &mut FunctionEmitter<'_>, op: u32) -> Result<(), String> {
    let args = vector_args(f, 1)?;
    let lane = reduction_lane(op);
    let value = as_lane(f, &args[0], lane);
    let nonzero = f.fresh();
    f.line(&format!("{nonzero} = icmp ne {} {value}, zeroinitializer", lane.vector));
    let mut combined = "true".to_string();
    for index in 0..lane.lanes {
        let part = f.fresh();
        f.line(&format!("{part} = extractelement <{} x i1> {nonzero}, i32 {index}", lane.lanes));
        let next = f.fresh();
        f.line(&format!("{next} = and i1 {combined}, {part}"));
        combined = next;
    }
    let result = f.fresh();
    f.line(&format!("{result} = zext i1 {combined} to i32"));
    f.push(ValType::I32, &result)
}

fn emit_bitmask(f: &mut FunctionEmitter<'_>, op: u32) -> Result<(), String> {
    let args = vector_args(f, 1)?;
    let lane = reduction_lane(op);
    let value = as_lane(f, &args[0], lane);
    let negative = f.fresh();
    f.line(&format!("{negative} = icmp slt {} {value}, zeroinitializer", lane.vector));
    let mut combined = "0".to_string();
    for index in 0..lane.lanes {
        let bit = f.fresh();
        f.line(&format!("{bit} = extractelement <{} x i1> {negative}, i32 {index}", lane.lanes));
        let extended = f.fresh();
        f.line(&format!("{extended} = zext i1 {bit} to i32"));
        let shifted = if index == 0 { extended } else {
            let value = f.fresh();
            f.line(&format!("{value} = shl i32 {extended}, {index}"));
            value
        };
        let next = f.fresh();
        f.line(&format!("{next} = or i32 {combined}, {shifted}"));
        combined = next;
    }
    f.push(ValType::I32, &combined)
}

fn emit_narrow(f: &mut FunctionEmitter<'_>, op: u32) -> Result<(), String> {
    let source = if op == 0x65 || op == 0x66 { lane_info("i16") } else { lane_info("i32") };
    let dest = if source.bits == 16 { lane_info("i8") } else { lane_info("i16") };
    let intermediate = format!("<{} x {}>", source.lanes, dest.scalar);
    let args = vector_args(f, 2)?;
    let mut narrowed = Vec::new();
    for arg in args {
        let value = as_lane(f, &arg, source);
        let is_signed = op == 0x65 || op == 0x85;
        let minimum = if is_signed { -(1i64 << (dest.bits - 1)) } else { 0 };
        let maximum = if is_signed { (1i64 << (dest.bits - 1)) - 1 } else { (1i64 << dest.bits) - 1 };
        let minimum_vector = splat_constant(source, minimum);
        let maximum_vector = splat_constant(source, maximum);
        let low_test = f.fresh();
        f.line(&format!("{low_test} = icmp slt {} {value}, {minimum_vector}", source.vector));
        let low = f.fresh();
        f.line(&format!("{low} = select <{} x i1> {low_test}, {} {minimum_vector}, {} {value}", source.lanes, source.vector, source.vector));
        let high_test = f.fresh();
        f.line(&format!("{high_test} = icmp sgt {} {low}, {maximum_vector}", source.vector));
        let clamped = f.fresh();
        f.line(&format!("{clamped} = select <{} x i1> {high_test}, {} {maximum_vector}, {} {low}", source.lanes, source.vector, source.vector));
        let trunc = f.fresh();
        f.line(&format!("{trunc} = trunc {} {clamped} to {intermediate}", source.vector));
        narrowed.push(trunc);
    }
    let mask = (0..source.lanes).map(|i| format!("i32 {i}"))
        .chain((0..source.lanes).map(|i| format!("i32 {}", source.lanes + i)))
        .collect::<Vec<_>>().join(", ");
    let result = f.fresh();
    f.line(&format!("{result} = shufflevector {intermediate} {}, {intermediate} {}, <{} x i32> <{mask}>", narrowed[0], narrowed[1], dest.lanes));
    push_lane(f, &result, dest)
}

fn emit_shift(f: &mut FunctionEmitter<'_>, op: u32) -> Result<(), String> {
    let kind = lane_kind(op);
    let lane = lane_info(kind);
    let args = f.pop_values(&[ValType::V128, ValType::I32])?;
    let value = as_lane(f, &args[0], lane);
    let count = f.fresh();
    f.line(&format!("{count} = and i32 {}, {}", args[1], lane.bits - 1));
    let splat = crate::simd_codegen::splat_vector(f, lane, &count, "i32");
    let instruction = match op {
        0x6b | 0x8b | 0xab | 0xcb => "shl",
        0x6c | 0x8c | 0xac | 0xcc => "ashr",
        _ => "lshr",
    };
    let result = f.fresh();
    f.line(&format!("{result} = {instruction} {} {value}, {splat}", lane.vector));
    push_lane(f, &result, lane)
}

fn emit_saturating_add_sub(f: &mut FunctionEmitter<'_>, op: u32) -> Result<(), String> {
    let lane = lane_info(lane_kind(op));
    let args = vector_args(f, 2)?;
    let a = as_lane(f, &args[0], lane);
    let b = as_lane(f, &args[1], lane);
    let name = match op {
        0x6f | 0x8f => "sadd.sat", 0x70 | 0x90 => "uadd.sat",
        0x72 | 0x92 => "ssub.sat", _ => "usub.sat",
    };
    let suffix = if lane.bits == 8 { "v16i8" } else { "v8i16" };
    let result = f.fresh();
    f.line(&format!("{result} = call {} @llvm.{name}.{suffix}({} {a}, {} {b})", lane.vector, lane.vector, lane.vector));
    push_lane(f, &result, lane)
}

fn emit_int_binary(f: &mut FunctionEmitter<'_>, op: u32) -> Result<(), String> {
    let lane = lane_info(lane_kind(op));
    let args = vector_args(f, 2)?;
    let a = as_lane(f, &args[0], lane);
    let b = as_lane(f, &args[1], lane);
    let instruction = match op {
        0x6e | 0x8e | 0xae | 0xce => "add",
        0x71 | 0x91 | 0xb1 | 0xd1 => "sub",
        _ => "mul",
    };
    let result = f.fresh();
    f.line(&format!("{result} = {instruction} {} {a}, {b}", lane.vector));
    push_lane(f, &result, lane)
}

fn emit_int_minmax(f: &mut FunctionEmitter<'_>, op: u32) -> Result<(), String> {
    let lane = lane_info(lane_kind(op));
    let args = vector_args(f, 2)?;
    let a = as_lane(f, &args[0], lane);
    let b = as_lane(f, &args[1], lane);
    let (predicate, choose_a) = match op {
        0x76 => ("slt", true), 0x77 => ("ult", true), 0x78 => ("sgt", true), 0x79 => ("ugt", true),
        0x96 => ("slt", true), 0x97 => ("ult", true), 0x98 => ("sgt", true), 0x99 => ("ugt", true),
        0xb6 => ("slt", true), 0xb7 => ("ult", true), 0xb8 => ("sgt", true), _ => ("ugt", true),
    };
    let cmp = f.fresh();
    f.line(&format!("{cmp} = icmp {predicate} {} {a}, {b}", lane.vector));
    let result = f.fresh();
    f.line(&format!("{result} = select <{} x i1> {cmp}, {} {}, {} {}", lane.lanes, lane.vector, if choose_a { &a } else { &b }, lane.vector, if choose_a { &b } else { &a }));
    push_lane(f, &result, lane)
}

fn emit_float_round(f: &mut FunctionEmitter<'_>, op: u32) -> Result<(), String> {
    let kind = lane_kind(op);
    let lane = lane_info(kind);
    let args = vector_args(f, 1)?;
    let value = as_lane(f, &args[0], lane);
    let intrinsic = match op {
        0x67 | 0x74 => "ceil", 0x68 | 0x75 => "floor", 0x69 | 0x7a => "trunc",
        0x6a | 0x94 => "roundeven", 0xe3 | 0xef => "sqrt", _ => return Err("invalid vector float unary opcode".into()),
    };
    let suffix = if kind == "f32" { "v4f32" } else { "v2f64" };
    let result = f.fresh();
    f.line(&format!("{result} = call {} @llvm.{intrinsic}.{suffix}({} {value})", lane.vector, lane.vector));
    push_lane(f, &result, lane)
}

fn emit_float_abs_neg(f: &mut FunctionEmitter<'_>, op: u32) -> Result<(), String> {
    let lane = lane_info(if op < 0xec { "f32" } else { "f64" });
    let args = vector_args(f, 1)?;
    let integer = lane_info(if lane.bits == 32 { "i32" } else { "i64" });
    let bits = as_lane(f, &args[0], integer);
    let result_bits = f.fresh();
    if op == 0xe0 || op == 0xec {
        let mask = if lane.bits == 32 { i32::MAX as i64 } else { i64::MAX };
        f.line(&format!("{result_bits} = and {} {bits}, {}", integer.vector, splat_constant(integer, mask)));
    } else {
        let sign = if lane.bits == 32 { i32::MIN as i64 } else { i64::MIN };
        f.line(&format!("{result_bits} = xor {} {bits}, {}", integer.vector, splat_constant(integer, sign)));
    }
    let result = cast_vector(f, &result_bits, integer.vector, lane.vector);
    push_lane(f, &result, lane)
}

fn emit_float_binary(f: &mut FunctionEmitter<'_>, op: u32) -> Result<(), String> {
    let lane = lane_info(if op < 0xec { "f32" } else { "f64" });
    let args = vector_args(f, 2)?;
    let a = as_lane(f, &args[0], lane);
    let b = as_lane(f, &args[1], lane);
    let instruction = match op {
        0xe4 | 0xf0 => "fadd", 0xe5 | 0xf1 => "fsub", 0xe6 | 0xf2 => "fmul", _ => "fdiv",
    };
    let result = f.fresh();
    f.line(&format!("{result} = {instruction} {} {a}, {b}", lane.vector));
    push_lane(f, &result, lane)
}

fn emit_float_minmax(f: &mut FunctionEmitter<'_>, op: u32) -> Result<(), String> {
    let lane = lane_info(if op < 0xec { "f32" } else { "f64" });
    let args = vector_args(f, 2)?;
    let a = as_lane(f, &args[0], lane);
    let b = as_lane(f, &args[1], lane);
    let pseudo = op == 0xea || op == 0xeb || op == 0xf6 || op == 0xf7;
    let maximum = op == 0xe9 || op == 0xeb || op == 0xf5 || op == 0xf7;
    if pseudo {
        let cmp = f.fresh();
        f.line(&format!("{cmp} = fcmp {} {} {a}, {b}", if maximum { "ogt" } else { "olt" }, lane.vector));
        let result = f.fresh();
        f.line(&format!("{result} = select <{} x i1> {cmp}, {} {a}, {} {b}", lane.lanes, lane.vector, lane.vector));
        return push_lane(f, &result, lane);
    }
    let int_lane = lane_info(if lane.bits == 32 { "i32" } else { "i64" });
    let a_nan = f.fresh();
    let b_nan = f.fresh();
    f.line(&format!("{a_nan} = fcmp uno {} {a}, {a}", lane.vector));
    f.line(&format!("{b_nan} = fcmp uno {} {b}, {b}", lane.vector));
    let any_nan = f.fresh();
    f.line(&format!("{any_nan} = or <{} x i1> {a_nan}, {b_nan}", lane.lanes));
    let cmp = f.fresh();
    f.line(&format!("{cmp} = fcmp {} {} {a}, {b}", if maximum { "ogt" } else { "olt" }, lane.vector));
    let ordered = f.fresh();
    f.line(&format!("{ordered} = select <{} x i1> {cmp}, {} {a}, {} {b}", lane.lanes, lane.vector, lane.vector));
    let a_zero = f.fresh();
    let b_zero = f.fresh();
    f.line(&format!("{a_zero} = fcmp oeq {} {a}, zeroinitializer", lane.vector));
    f.line(&format!("{b_zero} = fcmp oeq {} {b}, zeroinitializer", lane.vector));
    let both_zero = f.fresh();
    f.line(&format!("{both_zero} = and <{} x i1> {a_zero}, {b_zero}", lane.lanes));
    let a_bits = as_lane(f, &args[0], int_lane);
    let b_bits = as_lane(f, &args[1], int_lane);
    let sign_mask = if lane.bits == 32 { i32::MIN as i64 } else { i64::MIN };
    let a_sign = f.fresh();
    let b_sign = f.fresh();
    f.line(&format!("{a_sign} = and {} {a_bits}, {}", int_lane.vector, splat_constant(int_lane, sign_mask)));
    f.line(&format!("{b_sign} = and {} {b_bits}, {}", int_lane.vector, splat_constant(int_lane, sign_mask)));
    let zero_sign = f.fresh();
    f.line(&format!("{zero_sign} = {} {} {a_sign}, {b_sign}", if maximum { "and" } else { "or" }, int_lane.vector));
    let zero = cast_vector(f, &zero_sign, int_lane.vector, lane.vector);
    let with_zero = f.fresh();
    f.line(&format!("{with_zero} = select <{} x i1> {both_zero}, {} {zero}, {} {ordered}", lane.lanes, lane.vector, lane.vector));
    let nan_bits_value = if lane.bits == 32 { 0x7fc0_0000i64 } else { 0x7ff8_0000_0000_0000i64 };
    let nan_bits = splat_vector(f, int_lane, &nan_bits_value.to_string(), int_lane.scalar);
    let nan = cast_vector(f, &nan_bits, int_lane.vector, lane.vector);
    let result = f.fresh();
    f.line(&format!("{result} = select <{} x i1> {any_nan}, {} {nan}, {} {with_zero}", lane.lanes, lane.vector, lane.vector));
    push_lane(f, &result, lane)
}

fn emit_average(f: &mut FunctionEmitter<'_>, op: u32) -> Result<(), String> {
    let lane = lane_info(if op == 0x7b { "i8" } else { "i16" });
    let wide = lane_info(if lane.bits == 8 { "i16" } else { "i32" });
    let args = vector_args(f, 2)?;
    let a = as_lane(f, &args[0], lane);
    let b = as_lane(f, &args[1], lane);
    let aw = f.fresh();
    let bw = f.fresh();
    f.line(&format!("{aw} = zext {} {a} to {}", lane.vector, wide.vector));
    f.line(&format!("{bw} = zext {} {b} to {}", lane.vector, wide.vector));
    let sum = f.fresh();
    f.line(&format!("{sum} = add {} {aw}, {bw}", wide.vector));
    let plus = f.fresh();
    f.line(&format!("{plus} = add {} {sum}, {}", wide.vector, splat_constant(wide, 1)));
    let avg = f.fresh();
    f.line(&format!("{avg} = lshr {} {plus}, {}", wide.vector, splat_constant(wide, 1)));
    let result = f.fresh();
    f.line(&format!("{result} = trunc {} {avg} to {}", wide.vector, lane.vector));
    push_lane(f, &result, lane)
}

fn emit_q15mulr(f: &mut FunctionEmitter<'_>) -> Result<(), String> {
    let lane = lane_info("i16");
    let wide = lane_info("i32");
    let args = vector_args(f, 2)?;
    let a = as_lane(f, &args[0], lane);
    let b = as_lane(f, &args[1], lane);
    let aw = f.fresh();
    let bw = f.fresh();
    f.line(&format!("{aw} = sext <8 x i16> {a} to <8 x i32>"));
    f.line(&format!("{bw} = sext <8 x i16> {b} to <8 x i32>"));
    let product = f.fresh();
    f.line(&format!("{product} = mul <8 x i32> {aw}, {bw}"));
    let rounded = f.fresh();
    f.line(&format!("{rounded} = add <8 x i32> {product}, {}", splat_constant(wide, 16384)));
    let shifted = f.fresh();
    f.line(&format!("{shifted} = ashr <8 x i32> {rounded}, {}", splat_constant(wide, 15)));
    let minimum = splat_constant(wide, -32768);
    let maximum = splat_constant(wide, 32767);
    let below = f.fresh();
    f.line(&format!("{below} = icmp slt <8 x i32> {shifted}, {minimum}"));
    let low = f.fresh();
    f.line(&format!("{low} = select <8 x i1> {below}, <8 x i32> {minimum}, <8 x i32> {shifted}"));
    let above = f.fresh();
    f.line(&format!("{above} = icmp sgt <8 x i32> {low}, {maximum}"));
    let clamped = f.fresh();
    f.line(&format!("{clamped} = select <8 x i1> {above}, <8 x i32> {maximum}, <8 x i32> {low}"));
    let result = f.fresh();
    f.line(&format!("{result} = trunc <8 x i32> {clamped} to <8 x i16>"));
    let _ = wide;
    push_lane(f, &result, lane)
}

fn emit_pairwise_add(f: &mut FunctionEmitter<'_>, op: u32) -> Result<(), String> {
    let (source, dest, signed) = match op {
        0x7c => (lane_info("i8"), lane_info("i16"), true),
        0x7d => (lane_info("i8"), lane_info("i16"), false),
        0x7e => (lane_info("i16"), lane_info("i32"), true),
        _ => (lane_info("i16"), lane_info("i32"), false),
    };
    let args = vector_args(f, 1)?;
    let value = as_lane(f, &args[0], source);
    let even_mask = (0..dest.lanes).map(|i| format!("i32 {}", i * 2)).collect::<Vec<_>>().join(", ");
    let odd_mask = (0..dest.lanes).map(|i| format!("i32 {}", i * 2 + 1)).collect::<Vec<_>>().join(", ");
    let even = f.fresh();
    let odd = f.fresh();
    f.line(&format!("{even} = shufflevector {} {value}, {} zeroinitializer, <{} x i32> <{even_mask}>", source.vector, source.vector, dest.lanes));
    f.line(&format!("{odd} = shufflevector {} {value}, {} zeroinitializer, <{} x i32> <{odd_mask}>", source.vector, source.vector, dest.lanes));
    let wide_even = f.fresh();
    let wide_odd = f.fresh();
    let extension = if signed { "sext" } else { "zext" };
    f.line(&format!("{wide_even} = {extension} {} {even} to {}", source.vector, dest.vector));
    f.line(&format!("{wide_odd} = {extension} {} {odd} to {}", source.vector, dest.vector));
    let sum = f.fresh();
    f.line(&format!("{sum} = add {} {wide_even}, {wide_odd}", dest.vector));
    push_lane(f, &sum, dest)
}

fn emit_extend(f: &mut FunctionEmitter<'_>, op: u32) -> Result<(), String> {
    let (source, dest, high, signed) = match op {
        0x87 => (lane_info("i8"), lane_info("i16"), false, true),
        0x88 => (lane_info("i8"), lane_info("i16"), true, true),
        0x89 => (lane_info("i8"), lane_info("i16"), false, false),
        0x8a => (lane_info("i8"), lane_info("i16"), true, false),
        0xa7 => (lane_info("i16"), lane_info("i32"), false, true),
        0xa8 => (lane_info("i16"), lane_info("i32"), true, true),
        0xa9 => (lane_info("i16"), lane_info("i32"), false, false),
        0xaa => (lane_info("i16"), lane_info("i32"), true, false),
        0xc7 => (lane_info("i32"), lane_info("i64"), false, true),
        0xc8 => (lane_info("i32"), lane_info("i64"), true, true),
        0xc9 => (lane_info("i32"), lane_info("i64"), false, false),
        _ => (lane_info("i32"), lane_info("i64"), true, false),
    };
    let args = vector_args(f, 1)?;
    let value = as_lane(f, &args[0], source);
    let start = if high { dest.lanes } else { 0 };
    let mask = (0..dest.lanes).map(|i| format!("i32 {}", start + i)).collect::<Vec<_>>().join(", ");
    let narrow = f.fresh();
    f.line(&format!("{narrow} = shufflevector {} {value}, {} zeroinitializer, <{} x i32> <{mask}>", source.vector, source.vector, dest.lanes));
    let result = f.fresh();
    f.line(&format!("{result} = {} {} {narrow} to {}", if signed { "sext" } else { "zext" }, narrow_vec_ty(source, dest), dest.vector));
    push_lane(f, &result, dest)
}

fn narrow_vec_ty(source: LaneInfo, dest: LaneInfo) -> String {
    format!("<{} x {}>", dest.lanes, source.scalar)
}

fn emit_extmul(f: &mut FunctionEmitter<'_>, op: u32) -> Result<(), String> {
    let (source, dest, high, signed) = match op {
        0x9c => (lane_info("i8"), lane_info("i16"), false, true),
        0x9d => (lane_info("i8"), lane_info("i16"), true, true),
        0x9e => (lane_info("i8"), lane_info("i16"), false, false),
        0x9f => (lane_info("i8"), lane_info("i16"), true, false),
        0xbc => (lane_info("i16"), lane_info("i32"), false, true),
        0xbd => (lane_info("i16"), lane_info("i32"), true, true),
        0xbe => (lane_info("i16"), lane_info("i32"), false, false),
        0xbf => (lane_info("i16"), lane_info("i32"), true, false),
        0xdc => (lane_info("i32"), lane_info("i64"), false, true),
        0xdd => (lane_info("i32"), lane_info("i64"), true, true),
        0xde => (lane_info("i32"), lane_info("i64"), false, false),
        _ => (lane_info("i32"), lane_info("i64"), true, false),
    };
    let args = vector_args(f, 2)?;
    let start = if high { dest.lanes } else { 0 };
    let mask = (0..dest.lanes).map(|i| format!("i32 {}", start + i)).collect::<Vec<_>>().join(", ");
    let mut wide = Vec::new();
    for arg in args {
        let value = as_lane(f, &arg, source);
        let selected = f.fresh();
        f.line(&format!("{selected} = shufflevector {} {value}, {} zeroinitializer, <{} x i32> <{mask}>", source.vector, source.vector, dest.lanes));
        let extended = f.fresh();
        f.line(&format!("{extended} = {} {} {selected} to {}", if signed { "sext" } else { "zext" }, narrow_vec_ty(source, dest), dest.vector));
        wide.push(extended);
    }
    let result = f.fresh();
    f.line(&format!("{result} = mul {} {}, {}", dest.vector, wide[0], wide[1]));
    push_lane(f, &result, dest)
}

fn emit_dot(f: &mut FunctionEmitter<'_>) -> Result<(), String> {
    let args = vector_args(f, 2)?;
    let source = lane_info("i16");
    let dest = lane_info("i32");
    let a = as_lane(f, &args[0], source);
    let b = as_lane(f, &args[1], source);
    let aw = f.fresh();
    let bw = f.fresh();
    f.line(&format!("{aw} = sext <8 x i16> {a} to <8 x i32>"));
    f.line(&format!("{bw} = sext <8 x i16> {b} to <8 x i32>"));
    let product = f.fresh();
    f.line(&format!("{product} = mul <8 x i32> {aw}, {bw}"));
    let even_mask = "<4 x i32> <i32 0, i32 2, i32 4, i32 6>";
    let odd_mask = "<4 x i32> <i32 1, i32 3, i32 5, i32 7>";
    let even = f.fresh();
    let odd = f.fresh();
    f.line(&format!("{even} = shufflevector <8 x i32> {product}, <8 x i32> zeroinitializer, {even_mask}"));
    f.line(&format!("{odd} = shufflevector <8 x i32> {product}, <8 x i32> zeroinitializer, {odd_mask}"));
    let sum = f.fresh();
    f.line(&format!("{sum} = add <4 x i32> {even}, {odd}"));
    push_lane(f, &sum, dest)
}

fn emit_i64_compare(f: &mut FunctionEmitter<'_>, op: u32) -> Result<(), String> {
    let lane = lane_info("i64");
    let args = vector_args(f, 2)?;
    let a = as_lane(f, &args[0], lane);
    let b = as_lane(f, &args[1], lane);
    let pred = match op { 0xd6 => "eq", 0xd7 => "ne", 0xd8 => "slt", 0xd9 => "sgt", 0xda => "sle", _ => "sge" };
    let cmp = f.fresh();
    f.line(&format!("{cmp} = icmp {pred} <2 x i64> {a}, {b}"));
    let mask = f.fresh();
    f.line(&format!("{mask} = sext <2 x i1> {cmp} to <2 x i64>"));
    push_lane(f, &mask, lane)
}

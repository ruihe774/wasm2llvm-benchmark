use crate::{
    function::FunctionEmitter,
    memory_codegen::emit_mem_pointer,
    simd::{lane_count, memory_access, signature},
    wasm::{SimdImmediate, ValType},
};

#[derive(Clone, Copy)]
pub(crate) struct LaneInfo {
    pub(crate) lanes: usize,
    pub(crate) scalar: &'static str,
    pub(crate) vector: &'static str,
    pub(crate) bits: u32,
    pub(crate) float: bool,
}

pub(crate) fn lane_info(kind: &str) -> LaneInfo {
    match kind {
        "i8" => LaneInfo { lanes: 16, scalar: "i8", vector: "<16 x i8>", bits: 8, float: false },
        "i16" => LaneInfo { lanes: 8, scalar: "i16", vector: "<8 x i16>", bits: 16, float: false },
        "i32" => LaneInfo { lanes: 4, scalar: "i32", vector: "<4 x i32>", bits: 32, float: false },
        "i64" => LaneInfo { lanes: 2, scalar: "i64", vector: "<2 x i64>", bits: 64, float: false },
        "f32" => LaneInfo { lanes: 4, scalar: "float", vector: "<4 x float>", bits: 32, float: true },
        "f64" => LaneInfo { lanes: 2, scalar: "double", vector: "<2 x double>", bits: 64, float: true },
        _ => unreachable!(),
    }
}

pub(crate) fn cast_vector(f: &mut FunctionEmitter<'_>, value: &str, from: &str, to: &str) -> String {
    if from == to { return value.to_string(); }
    let result = f.fresh();
    f.line(&format!("{result} = bitcast {from} {value} to {to}"));
    result
}

pub(crate) fn splat_vector(f: &mut FunctionEmitter<'_>, lane: LaneInfo, value: &str, input_ty: &str) -> String {
    let scalar = if input_ty == lane.scalar {
        value.to_string()
    } else {
        let input_bits = input_ty.strip_prefix('i').and_then(|bits| bits.parse::<u32>().ok()).unwrap_or(lane.bits);
        let instruction = if input_bits < lane.bits { "zext" } else { "trunc" };
        let narrowed = f.fresh();
        f.line(&format!("{narrowed} = {instruction} {input_ty} {value} to {}", lane.scalar));
        narrowed
    };
    let inserted = f.fresh();
    f.line(&format!("{inserted} = insertelement {} zeroinitializer, {} {scalar}, i32 0", lane.vector, lane.scalar));
    let mask = shuffle_mask(lane.lanes, 0);
    let result = f.fresh();
    f.line(&format!("{result} = shufflevector {} {inserted}, {} zeroinitializer, <{} x i32> {mask}", lane.vector, lane.vector, lane.lanes));
    result
}

pub(crate) fn shuffle_mask(count: usize, value: usize) -> String {
    let values = (0..count).map(|_| format!("i32 {value}")).collect::<Vec<_>>().join(", ");
    format!("<{values}>")
}

pub(crate) fn emit_simd(f: &mut FunctionEmitter<'_>, op: u32, immediate: &SimdImmediate) -> Result<(), String> {
    if memory_access(op, immediate).is_some() { return emit_memory(f, op, immediate); }
    match op {
        0x0c => emit_v128_const(f, immediate),
        0x0d => emit_shuffle(f, immediate),
        0x0e => emit_swizzle(f),
        0x0f..=0x14 => emit_splat(f, op),
        0x15..=0x22 => emit_lane(f, op, immediate),
        0x23..=0x4c => emit_compare(f, op),
        0x4d..=0x53 => emit_logic(f, op),
        0x5e | 0x5f | 0xf8..=0xff => emit_conversion(f, op),
        _ => crate::simd_arith::emit_arithmetic(f, op),
    }
}

fn emit_v128_const(f: &mut FunctionEmitter<'_>, immediate: &SimdImmediate) -> Result<(), String> {
    let SimdImmediate::Bytes(bytes) = immediate else { return Err("v128.const is missing its immediate".into()); };
    let literal = bytes.iter().map(|b| format!("i8 {b}")).collect::<Vec<_>>().join(", ");
    let value = format!("<{literal}>");
    f.push(ValType::V128, &value)
}

fn emit_shuffle(f: &mut FunctionEmitter<'_>, immediate: &SimdImmediate) -> Result<(), String> {
    let SimdImmediate::Bytes(mask) = immediate else { return Err("i8x16.shuffle is missing its immediate".into()); };
    let args = f.pop_values(&[ValType::V128, ValType::V128])?;
    let mask = mask.iter().map(|v| format!("i32 {v}")).collect::<Vec<_>>().join(", ");
    let result = f.fresh();
    f.line(&format!("{result} = shufflevector <16 x i8> {}, <16 x i8> {}, <16 x i32> <{mask}>", args[0], args[1]));
    f.push(ValType::V128, &result)
}

fn emit_swizzle(f: &mut FunctionEmitter<'_>) -> Result<(), String> {
    let args = f.pop_values(&[ValType::V128, ValType::V128])?;
    let mut result = "zeroinitializer".to_string();
    for lane in 0..16 {
        let index8 = f.fresh();
        f.line(&format!("{index8} = extractelement <16 x i8> {}, i32 {lane}", args[1]));
        let index = f.fresh();
        f.line(&format!("{index} = zext i8 {index8} to i32"));
        let in_range = f.fresh();
        f.line(&format!("{in_range} = icmp ult i32 {index}, 16"));
        let safe_index = f.fresh();
        f.line(&format!("{safe_index} = and i32 {index}, 15"));
        let byte = f.fresh();
        f.line(&format!("{byte} = extractelement <16 x i8> {}, i32 {safe_index}", args[0]));
        let selected = f.fresh();
        f.line(&format!("{selected} = select i1 {in_range}, i8 {byte}, i8 0"));
        let inserted = f.fresh();
        f.line(&format!("{inserted} = insertelement <16 x i8> {result}, i8 {selected}, i32 {lane}"));
        result = inserted;
    }
    f.push(ValType::V128, &result)
}

fn emit_splat(f: &mut FunctionEmitter<'_>, op: u32) -> Result<(), String> {
    let (input, kind) = match op {
        0x0f => (ValType::I32, "i8"), 0x10 => (ValType::I32, "i16"),
        0x11 => (ValType::I32, "i32"), 0x12 => (ValType::I64, "i64"),
        0x13 => (ValType::F32, "f32"), 0x14 => (ValType::F64, "f64"), _ => unreachable!(),
    };
    let scalar = f.pop(input)?;
    let lane = lane_info(kind);
    let value = splat_vector(f, lane, &scalar, input.llvm());
    let value = cast_vector(f, &value, lane.vector, "<16 x i8>");
    f.push(ValType::V128, &value)
}

fn lane_kind(op: u32) -> &'static str {
    match op {
        0x15..=0x17 => "i8", 0x18..=0x1a => "i16", 0x1b..=0x1c => "i32",
        0x1d..=0x1e => "i64", 0x1f..=0x20 => "f32", 0x21..=0x22 => "f64",
        _ => unreachable!(),
    }
}

fn emit_lane(f: &mut FunctionEmitter<'_>, op: u32, immediate: &SimdImmediate) -> Result<(), String> {
    let SimdImmediate::Lane(lane_index) = immediate else { return Err("SIMD lane operation is missing its lane index".into()); };
    if *lane_index >= lane_count(op) { return Err("SIMD lane index is out of range".into()); }
    let lane = lane_info(lane_kind(op));
    let sig = signature(op).ok_or("invalid lane opcode")?;
    let args = f.pop_values(&sig.0)?;
    let vector = cast_vector(f, &args[0], "<16 x i8>", lane.vector);
    if op == 0x15 || op == 0x16 || op == 0x18 || op == 0x19 || op == 0x1b || op == 0x1d || op == 0x1f || op == 0x21 {
        let value = f.fresh();
        f.line(&format!("{value} = extractelement {} {vector}, i32 {lane_index}", lane.vector));
        if op == 0x15 || op == 0x18 {
            let result = f.fresh();
            f.line(&format!("{result} = sext {} {value} to i32", lane.scalar));
            f.push(ValType::I32, &result)
        } else if op == 0x16 || op == 0x19 {
            let result = f.fresh();
            f.line(&format!("{result} = zext {} {value} to i32", lane.scalar));
            f.push(ValType::I32, &result)
        } else {
            f.push(match lane_kind(op) { "i64" => ValType::I64, "f32" => ValType::F32, "f64" => ValType::F64, _ => ValType::I32 }, &value)
        }
    } else {
        let scalar_ty = match lane_kind(op) { "i8" | "i16" | "i32" => ValType::I32, "i64" => ValType::I64, "f32" => ValType::F32, _ => ValType::F64 };
        let scalar = if lane.scalar == scalar_ty.llvm() { args[1].clone() } else {
            let narrowed = f.fresh();
            f.line(&format!("{narrowed} = trunc {} {} to {}", scalar_ty.llvm(), args[1], lane.scalar));
            narrowed
        };
        let result_vec = f.fresh();
        f.line(&format!("{result_vec} = insertelement {} {vector}, {} {scalar}, i32 {lane_index}", lane.vector, lane.scalar));
        let result = cast_vector(f, &result_vec, lane.vector, "<16 x i8>");
        f.push(ValType::V128, &result)
    }
}

fn emit_compare(f: &mut FunctionEmitter<'_>, op: u32) -> Result<(), String> {
    let (kind, index, is_float) = match op {
        0x23..=0x2c => ("i8", op - 0x23, false),
        0x2d..=0x36 => ("i16", op - 0x2d, false),
        0x37..=0x40 => ("i32", op - 0x37, false),
        0x41..=0x46 => ("f32", op - 0x41, true),
        0x47..=0x4c => ("f64", op - 0x47, true),
        _ => unreachable!(),
    };
    let lane = lane_info(kind);
    let args = f.pop_values(&[ValType::V128, ValType::V128])?;
    let a = cast_vector(f, &args[0], "<16 x i8>", lane.vector);
    let b = cast_vector(f, &args[1], "<16 x i8>", lane.vector);
    let predicate = if is_float {
        ["oeq", "une", "olt", "ogt", "ole", "oge"][index as usize]
    } else {
        ["eq", "ne", "slt", "ult", "sgt", "ugt", "sle", "ule", "sge", "uge"][index as usize]
    };
    let cmp = f.fresh();
    f.line(&format!("{cmp} = {} {predicate} {} {a}, {b}", if is_float { "fcmp" } else { "icmp" }, lane.vector));
    let mask = f.fresh();
    let mask_type = match kind { "f32" => "<4 x i32>", "f64" => "<2 x i64>", _ => lane.vector };
    f.line(&format!("{mask} = sext <{} x i1> {cmp} to {mask_type}", lane.lanes));
    let result = cast_vector(f, &mask, mask_type, "<16 x i8>");
    f.push(ValType::V128, &result)
}

fn emit_logic(f: &mut FunctionEmitter<'_>, op: u32) -> Result<(), String> {
    match op {
        0x4d => {
            let value = f.pop(ValType::V128)?;
            let result = f.fresh();
            f.line(&format!("{result} = xor <16 x i8> {value}, {}", all_ones(lane_info("i8"))));
            f.push(ValType::V128, &result)
        }
        0x4e..=0x51 => {
            let args = f.pop_values(&[ValType::V128, ValType::V128])?;
            let (instruction, lhs, rhs) = match op {
                0x4e => ("and", args[0].as_str(), args[1].as_str()),
                0x4f => ("and", args[0].as_str(), ""),
                0x50 => ("or", args[0].as_str(), args[1].as_str()),
                _ => ("xor", args[0].as_str(), args[1].as_str()),
            };
            let result = f.fresh();
            if op == 0x4f {
                let not = f.fresh();
                f.line(&format!("{not} = xor <16 x i8> {}, {}", args[1], all_ones(lane_info("i8"))));
                f.line(&format!("{result} = and <16 x i8> {lhs}, {not}"));
            } else {
                f.line(&format!("{result} = {instruction} <16 x i8> {lhs}, {rhs}"));
            }
            f.push(ValType::V128, &result)
        }
        0x52 => {
            let args = f.pop_values(&[ValType::V128, ValType::V128, ValType::V128])?;
            let a = f.fresh();
            f.line(&format!("{a} = and <16 x i8> {}, {}", args[0], args[2]));
            let inv = f.fresh();
            f.line(&format!("{inv} = xor <16 x i8> {}, {}", args[2], all_ones(lane_info("i8"))));
            let b = f.fresh();
            f.line(&format!("{b} = and <16 x i8> {}, {inv}", args[1]));
            let result = f.fresh();
            f.line(&format!("{result} = or <16 x i8> {a}, {b}"));
            f.push(ValType::V128, &result)
        }
        0x53 => {
            let value = f.pop(ValType::V128)?;
            let bits = f.fresh();
            f.line(&format!("{bits} = bitcast <16 x i8> {value} to i128"));
            let any = f.fresh();
            f.line(&format!("{any} = icmp ne i128 {bits}, 0"));
            let result = f.fresh();
            f.line(&format!("{result} = zext i1 {any} to i32"));
            f.push(ValType::I32, &result)
        }
        _ => unreachable!(),
    }
}

fn all_ones(lane: LaneInfo) -> String {
    let elements = (0..lane.lanes).map(|_| format!("{} -1", lane.scalar)).collect::<Vec<_>>().join(", ");
    format!("<{elements}>")
}

fn emit_conversion(f: &mut FunctionEmitter<'_>, op: u32) -> Result<(), String> {
    match op {
        0x5e => {
            let value = f.pop(ValType::V128)?;
            let source = cast_vector(f, &value, "<16 x i8>", "<2 x double>");
            let low = f.fresh();
            f.line(&format!("{low} = fptrunc <2 x double> {source} to <2 x float>"));
            let result = f.fresh();
            f.line(&format!("{result} = shufflevector <2 x float> {low}, <2 x float> zeroinitializer, <4 x i32> <i32 0, i32 1, i32 2, i32 3>"));
            let vector = cast_vector(f, &result, "<4 x float>", "<16 x i8>");
            f.push(ValType::V128, &vector)
        }
        0x5f => {
            let value = f.pop(ValType::V128)?;
            let source = cast_vector(f, &value, "<16 x i8>", "<4 x float>");
            let low = f.fresh();
            f.line(&format!("{low} = shufflevector <4 x float> {source}, <4 x float> zeroinitializer, <2 x i32> <i32 0, i32 1>"));
            let result = f.fresh();
            f.line(&format!("{result} = fpext <2 x float> {low} to <2 x double>"));
            let vector = cast_vector(f, &result, "<2 x double>", "<16 x i8>");
            f.push(ValType::V128, &vector)
        }
        0xf8 | 0xf9 => {
            let value = f.pop(ValType::V128)?;
            let source = cast_vector(f, &value, "<16 x i8>", "<4 x float>");
            let result = f.fresh();
            let intrinsic = if op == 0xf8 { "fptosi" } else { "fptoui" };
            f.line(&format!("{result} = call <4 x i32> @llvm.{intrinsic}.sat.v4i32.v4f32(<4 x float> {source})"));
            let vector = cast_vector(f, &result, "<4 x i32>", "<16 x i8>");
            f.push(ValType::V128, &vector)
        }
        0xfa | 0xfb => {
            let value = f.pop(ValType::V128)?;
            let source = cast_vector(f, &value, "<16 x i8>", "<4 x i32>");
            let result = f.fresh();
            f.line(&format!("{result} = {} <4 x i32> {source} to <4 x float>", if op == 0xfa { "sitofp" } else { "uitofp" }));
            let vector = cast_vector(f, &result, "<4 x float>", "<16 x i8>");
            f.push(ValType::V128, &vector)
        }
        0xfc | 0xfd => {
            let value = f.pop(ValType::V128)?;
            let source = cast_vector(f, &value, "<16 x i8>", "<2 x double>");
            let converted = f.fresh();
            let intrinsic = if op == 0xfc { "fptosi" } else { "fptoui" };
            f.line(&format!("{converted} = call <2 x i32> @llvm.{intrinsic}.sat.v2i32.v2f64(<2 x double> {source})"));
            let result = f.fresh();
            f.line(&format!("{result} = shufflevector <2 x i32> {converted}, <2 x i32> zeroinitializer, <4 x i32> <i32 0, i32 1, i32 2, i32 3>"));
            let vector = cast_vector(f, &result, "<4 x i32>", "<16 x i8>");
            f.push(ValType::V128, &vector)
        }
        0xfe | 0xff => {
            let value = f.pop(ValType::V128)?;
            let source = cast_vector(f, &value, "<16 x i8>", "<4 x i32>");
            let low = f.fresh();
            f.line(&format!("{low} = shufflevector <4 x i32> {source}, <4 x i32> zeroinitializer, <2 x i32> <i32 0, i32 1>"));
            let result = f.fresh();
            f.line(&format!("{result} = {} <2 x i32> {low} to <2 x double>", if op == 0xfe { "sitofp" } else { "uitofp" }));
            let vector = cast_vector(f, &result, "<2 x double>", "<16 x i8>");
            f.push(ValType::V128, &vector)
        }
        _ => unreachable!(),
    }
}

fn emit_memory(f: &mut FunctionEmitter<'_>, op: u32, immediate: &SimdImmediate) -> Result<(), String> {
    let (_, width) = memory_access(op, immediate).ok_or("invalid SIMD memory instruction")?;
    let (offset, lane_index) = match immediate {
        SimdImmediate::MemArg { offset, .. } => (*offset, None),
        SimdImmediate::MemLane { offset, lane, .. } => (*offset, Some(*lane)),
        _ => return Err("malformed SIMD memory immediate".into()),
    };
    match op {
        0x0b => {
            let value = f.pop(ValType::V128)?;
            let address = f.pop(ValType::I32)?;
            let ptr = emit_mem_pointer(f, &address, offset, width as u64);
            f.line(&format!("store <16 x i8> {value}, ptr {ptr}, align 1"));
            Ok(())
        }
        0x58..=0x5b => {
            let value = f.pop(ValType::V128)?;
            let address = f.pop(ValType::I32)?;
            let lane = lane_info(match op { 0x58 => "i8", 0x59 => "i16", 0x5a => "i32", _ => "i64" });
            let vector = cast_vector(f, &value, "<16 x i8>", lane.vector);
            let scalar = f.fresh();
            f.line(&format!("{scalar} = extractelement {} {vector}, i32 {}", lane.vector, lane_index.unwrap()));
            let ptr = emit_mem_pointer(f, &address, offset, width as u64);
            f.line(&format!("store {} {scalar}, ptr {ptr}, align 1", lane.scalar));
            Ok(())
        }
        0x54..=0x57 => {
            let value = f.pop(ValType::V128)?;
            let address = f.pop(ValType::I32)?;
            let lane = lane_info(match op { 0x54 => "i8", 0x55 => "i16", 0x56 => "i32", _ => "i64" });
            let vector = cast_vector(f, &value, "<16 x i8>", lane.vector);
            let ptr = emit_mem_pointer(f, &address, offset, width as u64);
            let scalar = f.fresh();
            f.line(&format!("{scalar} = load {}, ptr {ptr}, align 1", lane.scalar));
            let updated = f.fresh();
            f.line(&format!("{updated} = insertelement {} {vector}, {} {scalar}, i32 {}", lane.vector, lane.scalar, lane_index.unwrap()));
            let result = cast_vector(f, &updated, lane.vector, "<16 x i8>");
            f.push(ValType::V128, &result)
        }
        _ => {
            let address = f.pop(ValType::I32)?;
            let ptr = emit_mem_pointer(f, &address, offset, width as u64);
            let result = match op {
                0x00 => {
                    let value = f.fresh(); f.line(&format!("{value} = load <16 x i8>, ptr {ptr}, align 1")); value
                }
                0x01..=0x06 => {
                    let (small, wide, sign) = match op {
                        0x01 | 0x02 => ("<8 x i8>", "<8 x i16>", op == 0x01),
                        0x03 | 0x04 => ("<4 x i16>", "<4 x i32>", op == 0x03),
                        _ => ("<2 x i32>", "<2 x i64>", op == 0x05),
                    };
                    let loaded = f.fresh(); f.line(&format!("{loaded} = load {small}, ptr {ptr}, align 1"));
                    let extended = f.fresh(); f.line(&format!("{extended} = {} {small} {loaded} to {wide}", if sign { "sext" } else { "zext" }));
                    let packed = f.fresh(); f.line(&format!("{packed} = bitcast {wide} {extended} to <16 x i8>")); packed
                }
                0x07..=0x0a => {
                    let scalar_ty = match op { 0x07 => "i8", 0x08 => "i16", 0x09 => "i32", _ => "i64" };
                    let loaded = f.fresh(); f.line(&format!("{loaded} = load {scalar_ty}, ptr {ptr}, align 1"));
                    let lane = lane_info(scalar_ty);
                    let vector = splat_vector(f, lane, &loaded, scalar_ty);
                    cast_vector(f, &vector, lane.vector, "<16 x i8>")
                }
                0x5c => {
                    let loaded = f.fresh(); f.line(&format!("{loaded} = load i32, ptr {ptr}, align 1"));
                    let full = f.fresh(); f.line(&format!("{full} = insertelement <4 x i32> zeroinitializer, i32 {loaded}, i32 0"));
                    cast_vector(f, &full, "<4 x i32>", "<16 x i8>")
                }
                0x5d => {
                    let loaded = f.fresh(); f.line(&format!("{loaded} = load i64, ptr {ptr}, align 1"));
                    let full = f.fresh(); f.line(&format!("{full} = insertelement <2 x i64> zeroinitializer, i64 {loaded}, i32 0"));
                    cast_vector(f, &full, "<2 x i64>", "<16 x i8>")
                }
                _ => return Err(format!("unsupported SIMD memory opcode 0x{op:x}")),
            };
            f.push(ValType::V128, &result)
        }
    }
}

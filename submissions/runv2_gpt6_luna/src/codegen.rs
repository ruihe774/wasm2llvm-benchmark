use std::fmt::Write as _;

use crate::wasm::{ConstExpr, ExportKind, FuncType, Module, SegmentMode, ValType};

pub fn emit(module: &Module) -> Result<String, String> {
    let mut out = String::new();
    out.push_str("source_filename = \"wasm2llvm\"\ntarget triple = \"x86_64-unknown-linux-gnu\"\n\n");
    emit_declarations(&mut out, module);
    emit_globals(&mut out, module);
    emit_runtime(&mut out, module);
    for index in 0..module.functions.len() {
        if module.functions[index].import.is_some() {
            emit_import_wrapper(&mut out, module, index)?;
        } else {
            out.push_str(&crate::function::emit_function(module, index)?);
        }
        out.push('\n');
    }
    emit_init(&mut out, module);
    emit_exports(&mut out, module)?;
    Ok(out)
}

fn emit_declarations(out: &mut String, module: &Module) {
    out.push_str("declare void @w2l_trap(i32) noreturn nounwind\n");
    out.push_str("declare ptr @calloc(i64, i64)\n");
    out.push_str("declare ptr @realloc(ptr, i64)\n");
    out.push_str("declare void @llvm.memcpy.p0.p0.i64(ptr, ptr, i64, i1)\n");
    out.push_str("declare void @llvm.memmove.p0.p0.i64(ptr, ptr, i64, i1)\n");
    out.push_str("declare void @llvm.memset.p0.i64(ptr, i8, i64, i1)\n");
    out.push_str("declare i32 @llvm.ctlz.i32(i32, i1 immarg)\n");
    out.push_str("declare i32 @llvm.cttz.i32(i32, i1 immarg)\n");
    out.push_str("declare i32 @llvm.ctpop.i32(i32)\n");
    out.push_str("declare i64 @llvm.ctlz.i64(i64, i1 immarg)\n");
    out.push_str("declare i64 @llvm.cttz.i64(i64, i1 immarg)\n");
    out.push_str("declare i64 @llvm.ctpop.i64(i64)\n");
    for (suffix, ty) in [("v16i8", "<16 x i8>"), ("v8i16", "<8 x i16>"), ("v4i32", "<4 x i32>"), ("v2i64", "<2 x i64>")] {
        let _ = writeln!(out, "declare {ty} @llvm.ctpop.{suffix}({ty})");
    }
    for ty in ["f32", "f64", "v4f32", "v2f64"] {
        let llty = match ty {
            "f32" => "float", "f64" => "double", "v4f32" => "<4 x float>", _ => "<2 x double>",
        };
        for op in ["ceil", "floor", "trunc", "roundeven", "sqrt"] {
            let _ = writeln!(out, "declare {llty} @llvm.{op}.{ty}({llty})");
        }
    }
    for (int_suffix, int_ty, float_suffix, float_ty) in [
        ("i32", "i32", "f32", "float"), ("i32", "i32", "f64", "double"),
        ("i64", "i64", "f32", "float"), ("i64", "i64", "f64", "double"),
        ("v4i32", "<4 x i32>", "v4f32", "<4 x float>"),
        ("v2i32", "<2 x i32>", "v2f64", "<2 x double>"),
    ] {
        let _ = writeln!(out, "declare {int_ty} @llvm.fptosi.sat.{int_suffix}.{float_suffix}({float_ty})");
        let _ = writeln!(out, "declare {int_ty} @llvm.fptoui.sat.{int_suffix}.{float_suffix}({float_ty})");
    }
    for (suffix, ty) in [("v16i8", "<16 x i8>"), ("v8i16", "<8 x i16>")] {
        for op in ["sadd.sat", "uadd.sat", "ssub.sat", "usub.sat"] {
            let _ = writeln!(out, "declare {ty} @llvm.{op}.{suffix}({ty}, {ty})");
        }
    }
    let mut imports = std::collections::BTreeSet::new();
    for f in &module.functions {
        if let Some((m, n)) = &f.import { imports.insert((hex(m.as_bytes()), hex(n.as_bytes()))); }
    }
    for (m, n) in imports {
        let _ = writeln!(out, "declare void @w2l_import_{m}_{n}(ptr, ptr)");
    }
    out.push('\n');
}

fn emit_globals(out: &mut String, module: &Module) {
    out.push_str("@w2l_mem_base = internal global ptr null\n");
    out.push_str("@w2l_mem_size = internal global i64 0\n");
    let mem_max = module.memories.first().map(|m| m.limits.max.unwrap_or(65536)).unwrap_or(0);
    let _ = writeln!(out, "@w2l_mem_max_pages = internal constant i64 {mem_max}");
    out.push_str("@w2l_table_base = internal global ptr null\n");
    out.push_str("@w2l_table_size = internal global i32 0\n");
    let table_max = module.tables.first().map(|t| t.limits.max.unwrap_or(u32::MAX)).unwrap_or(0);
    let _ = writeln!(out, "@w2l_table_max = internal constant i64 {table_max}");
    out.push_str("@w2l_depth = internal global i32 0\n");
    for (index, g) in module.globals.iter().enumerate() {
        let _ = writeln!(out, "@w2l_g{index} = internal global {} {}", g.ty.llvm(), const_initializer(&g.init));
    }
    for (index, data) in module.data.iter().enumerate() {
        emit_byte_constant(out, &format!("w2l_data_{index}"), &data.bytes);
        let len = if matches!(data.mode, SegmentMode::Passive) { data.bytes.len() } else { 0 };
        let _ = writeln!(out, "@w2l_data_len_{index} = internal global i64 {len}");
    }
    for (index, elem) in module.elements.iter().enumerate() {
        if elem.functions.is_empty() {
            let _ = writeln!(out, "@w2l_elem_{index} = private constant [0 x i32] zeroinitializer");
        } else {
            let values = elem.functions.iter().map(|v| format!("i32 {}", i32_bits(*v))).collect::<Vec<_>>().join(", ");
            let _ = writeln!(out, "@w2l_elem_{index} = private constant [{} x i32] [{}]", elem.functions.len(), values);
        }
        let len = if matches!(elem.mode, SegmentMode::Passive) { elem.functions.len() } else { 0 };
        let _ = writeln!(out, "@w2l_elem_len_{index} = internal global i64 {len}");
    }
    out.push('\n');
}

fn emit_byte_constant(out: &mut String, name: &str, bytes: &[u8]) {
    if bytes.is_empty() {
        let _ = writeln!(out, "@{name} = private constant [0 x i8] zeroinitializer, align 1");
        return;
    }
    let mut literal = String::with_capacity(bytes.len() * 3 + 2);
    literal.push('c');
    literal.push('"');
    for b in bytes { let _ = write!(literal, "\\{b:02X}"); }
    literal.push('"');
    let _ = writeln!(out, "@{name} = private unnamed_addr constant [{} x i8] {literal}, align 1", bytes.len());
}

pub(crate) fn const_initializer(value: &ConstExpr) -> String {
    match value {
        ConstExpr::I32(v) => v.to_string(),
        ConstExpr::I64(v) => v.to_string(),
        ConstExpr::F32(bits) => format!("bitcast (i32 {} to float)", i32_bits(*bits)),
        ConstExpr::F64(bits) => format!("bitcast (i64 {} to double)", i64_bits(*bits)),
    }
}

pub(crate) fn i32_bits(value: u32) -> i32 { value as i32 }
pub(crate) fn i64_bits(value: u64) -> i64 { value as i64 }
pub(crate) fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes { let _ = write!(out, "{b:02x}"); }
    out
}

pub(crate) fn fn_name(index: usize) -> String { format!("wasm_fn_{index}") }
pub(crate) fn return_type(results: &[ValType]) -> String {
    match results.len() {
        0 => "void".into(),
        1 => results[0].llvm().into(),
        _ => format!("{{ {} }}", results.iter().map(|t| t.llvm()).collect::<Vec<_>>().join(", ")),
    }
}

fn emit_runtime(out: &mut String, module: &Module) {
    out.push_str("define internal ptr @w2l_mem_ptr(i32 %addr, i32 %offset, i64 %length) {\nentry:\n");
    out.push_str("  %addr64 = zext i32 %addr to i64\n  %off64 = zext i32 %offset to i64\n  %start = add i64 %addr64, %off64\n  %end = add i64 %start, %length\n  %size = load i64, ptr @w2l_mem_size\n  %oob = icmp ugt i64 %end, %size\n  br i1 %oob, label %trap, label %ok\ntrap:\n  call void @w2l_trap(i32 4)\n  unreachable\nok:\n  %base = load ptr, ptr @w2l_mem_base\n  %ptr = getelementptr i8, ptr %base, i64 %start\n  ret ptr %ptr\n}\n\n");
    out.push_str("define internal i32 @w2l_memory_grow(i32 %delta) {\nentry:\n");
    out.push_str("  %old_size = load i64, ptr @w2l_mem_size\n  %old_pages = lshr i64 %old_size, 16\n  %zero = icmp eq i32 %delta, 0\n  br i1 %zero, label %same, label %grow\nsame:\n  %old_page_result = trunc i64 %old_pages to i32\n  ret i32 %old_page_result\ngrow:\n  %delta64 = zext i32 %delta to i64\n  %new_pages = add i64 %old_pages, %delta64\n  %max_pages = load i64, ptr @w2l_mem_max_pages\n  %too_many = icmp ugt i64 %new_pages, %max_pages\n  br i1 %too_many, label %fail, label %alloc\nalloc:\n  %new_size = shl i64 %new_pages, 16\n  %base = load ptr, ptr @w2l_mem_base\n  %new_base = call ptr @realloc(ptr %base, i64 %new_size)\n  %no_memory = icmp eq ptr %new_base, null\n  br i1 %no_memory, label %fail, label %zero_new\nzero_new:\n  %growth = sub i64 %new_size, %old_size\n  %zero_ptr = getelementptr i8, ptr %new_base, i64 %old_size\n  call void @llvm.memset.p0.i64(ptr %zero_ptr, i8 0, i64 %growth, i1 false)\n  store ptr %new_base, ptr @w2l_mem_base\n  store i64 %new_size, ptr @w2l_mem_size\n  %result = trunc i64 %old_pages to i32\n  ret i32 %result\nfail:\n  ret i32 -1\n}\n\n");
    out.push_str("define internal void @w2l_depth_enter() {\nentry:\n  %depth = load i32, ptr @w2l_depth\n  %next = add i32 %depth, 1\n  store i32 %next, ptr @w2l_depth\n  %too_deep = icmp ugt i32 %next, 16384\n  br i1 %too_deep, label %trap, label %ok\ntrap:\n  call void @w2l_trap(i32 8)\n  unreachable\nok:\n  ret void\n}\n\n");
    out.push_str("define internal void @w2l_depth_leave() {\nentry:\n  %depth = load i32, ptr @w2l_depth\n  %next = sub i32 %depth, 1\n  store i32 %next, ptr @w2l_depth\n  ret void\n}\n\n");
    let _ = module;
}

fn emit_import_wrapper(out: &mut String, module: &Module, index: usize) -> Result<(), String> {
    let function = &module.functions[index];
    let (import_module, import_name) = function.import.as_ref().ok_or("not an import")?;
    let ty = module.types.get(function.type_idx as usize).ok_or("invalid import function type")?;
    let args = ty.params.iter().enumerate().map(|(i,t)| format!("{} %p{i}", t.llvm())).collect::<Vec<_>>().join(", ");
    let ret = return_type(&ty.results);
    let _ = writeln!(out, "define internal {ret} @{}({args}) {{\nentry:", fn_name(index));
    out.push_str("  call void @w2l_depth_enter()\n");
    let arg_slots = ty.params.len().max(1);
    let result_slots = ty.results.len().max(1);
    let _ = writeln!(out, "  %args = alloca [{arg_slots} x <16 x i8>], align 16");
    let _ = writeln!(out, "  %results = alloca [{result_slots} x <16 x i8>], align 16");
    for (i, param) in ty.params.iter().enumerate() {
        let _ = writeln!(out, "  %arg_slot_{i} = getelementptr [{arg_slots} x <16 x i8>], ptr %args, i32 0, i32 {i}");
        let _ = writeln!(out, "  store {} %p{i}, ptr %arg_slot_{i}, align 1", param.llvm());
    }
    for i in 0..ty.results.len() {
        let _ = writeln!(out, "  %result_slot_init_{i} = getelementptr [{result_slots} x <16 x i8>], ptr %results, i32 0, i32 {i}");
        let _ = writeln!(out, "  store <16 x i8> zeroinitializer, ptr %result_slot_init_{i}, align 16");
    }
    let m = hex(import_module.as_bytes());
    let n = hex(import_name.as_bytes());
    let _ = writeln!(out, "  call void @w2l_import_{m}_{n}(ptr %args, ptr %results)");
    let mut values = Vec::new();
    for (i, result) in ty.results.iter().enumerate() {
        let _ = writeln!(out, "  %result_slot_{i} = getelementptr [{result_slots} x <16 x i8>], ptr %results, i32 0, i32 {i}");
        let _ = writeln!(out, "  %result_{i} = load {}, ptr %result_slot_{i}, align 1", result.llvm());
        values.push(format!("%result_{i}"));
    }
    out.push_str("  call void @w2l_depth_leave()\n");
    emit_return(out, &ty.results, &values);
    out.push_str("}\n");
    Ok(())
}

pub(crate) fn emit_return(out: &mut String, results: &[ValType], values: &[String]) {
    match results.len() {
        0 => out.push_str("  ret void\n"),
        1 => { let _ = writeln!(out, "  ret {} {}", results[0].llvm(), values[0]); }
        n => {
            let ty = return_type(results);
            let mut current = "zeroinitializer".to_string();
            for (i, (result, value)) in results.iter().zip(values).enumerate() {
                let name = format!("%ret_agg_{i}");
                let _ = writeln!(out, "  {name} = insertvalue {ty} {current}, {} {value}, {i}", result.llvm());
                current = name;
            }
            let _ = writeln!(out, "  ret {ty} {current}");
        }
    }
}

fn emit_init(out: &mut String, module: &Module) {
    out.push_str("define void @w2l_init() {\nentry:\n  store i32 0, ptr @w2l_depth\n");
    if let Some(memory) = module.memories.first() {
        let bytes = (memory.limits.min as u64) * 65536;
        if bytes != 0 {
            let _ = writeln!(out, "  %mem = call ptr @calloc(i64 1, i64 {bytes})\n  %mem_failed = icmp eq ptr %mem, null\n  br i1 %mem_failed, label %mem_trap, label %mem_ok\nmem_trap:\n  call void @w2l_trap(i32 4)\n  unreachable\nmem_ok:");
            out.push_str("  store ptr %mem, ptr @w2l_mem_base\n");
        }
        let _ = writeln!(out, "  store i64 {bytes}, ptr @w2l_mem_size");
    }
    if let Some(table) = module.tables.first() {
        let entries = table.limits.min as u64;
        let bytes = entries * 4;
        if bytes != 0 {
            let _ = writeln!(out, "  %table = call ptr @calloc(i64 1, i64 {bytes})\n  %table_failed = icmp eq ptr %table, null\n  br i1 %table_failed, label %table_trap, label %table_ok\ntable_trap:\n  call void @w2l_trap(i32 5)\n  unreachable\ntable_ok:");
            out.push_str("  store ptr %table, ptr @w2l_table_base\n");
            let _ = writeln!(out, "  call void @llvm.memset.p0.i64(ptr %table, i8 -1, i64 {bytes}, i1 false)");
        }
        let _ = writeln!(out, "  store i32 {}, ptr @w2l_table_size", i32_bits(table.limits.min));
    }
    for (index, global) in module.globals.iter().enumerate() {
        match global.init {
            ConstExpr::I32(value) => { let _ = writeln!(out, "  store i32 {value}, ptr @w2l_g{index}"); }
            ConstExpr::I64(value) => { let _ = writeln!(out, "  store i64 {value}, ptr @w2l_g{index}"); }
            ConstExpr::F32(bits) => {
                let _ = writeln!(out, "  %global_init_{index} = bitcast i32 {} to float\n  store float %global_init_{index}, ptr @w2l_g{index}", i32_bits(bits));
            }
            ConstExpr::F64(bits) => {
                let _ = writeln!(out, "  %global_init_{index} = bitcast i64 {} to double\n  store double %global_init_{index}, ptr @w2l_g{index}", i64_bits(bits));
            }
        }
    }
    for (index, data) in module.data.iter().enumerate() {
        if let SegmentMode::Active { offset, .. } = &data.mode {
            if let ConstExpr::I32(offset) = offset {
                let _ = writeln!(out, "  %data_dst_{index} = call ptr @w2l_mem_ptr(i32 {offset}, i32 0, i64 {})", data.bytes.len());
                if !data.bytes.is_empty() {
                    let _ = writeln!(out, "  call void @llvm.memcpy.p0.p0.i64(ptr %data_dst_{index}, ptr @w2l_data_{index}, i64 {}, i1 false)", data.bytes.len());
                }
            }
        }
    }
    for (index, elem) in module.elements.iter().enumerate() {
        if let SegmentMode::Active { offset, .. } = &elem.mode {
            if let ConstExpr::I32(offset) = offset {
                emit_active_element_init(out, index, *offset, elem.functions.len());
                for (slot, func) in elem.functions.iter().enumerate() {
                    let _ = writeln!(out, "  %elem_dst_{index}_{slot} = getelementptr i32, ptr %elem_base_{index}, i64 %elem_index_{index}_{slot}\n  store i32 {}, ptr %elem_dst_{index}_{slot}, align 4", i32_bits(*func));
                }
            }
        }
    }
    if let Some(start) = module.start {
        let _ = writeln!(out, "  call void @{}()", fn_name(start as usize));
    }
    out.push_str("  ret void\n}\n\n");
}

fn emit_active_element_init(out: &mut String, index: usize, offset: i32, count: usize) {
    let _ = writeln!(out, "  %elem_offset_{index} = zext i32 {offset} to i64\n  %elem_end_{index} = add i64 %elem_offset_{index}, {count}\n  %elem_size32_{index} = load i32, ptr @w2l_table_size\n  %elem_size_{index} = zext i32 %elem_size32_{index} to i64\n  %elem_oob_{index} = icmp ugt i64 %elem_end_{index}, %elem_size_{index}\n  br i1 %elem_oob_{index}, label %elem_trap_{index}, label %elem_ok_{index}\nelem_trap_{index}:\n  call void @w2l_trap(i32 5)\n  unreachable\nelem_ok_{index}:\n  %elem_base_{index} = load ptr, ptr @w2l_table_base");
    for slot in 0..count {
        let _ = writeln!(out, "  %elem_index_{index}_{slot} = add i64 %elem_offset_{index}, {slot}");
    }
}

fn emit_exports(out: &mut String, module: &Module) -> Result<(), String> {
    for export in &module.exports {
        let name = hex(export.name.as_bytes());
        match export.kind {
            ExportKind::Function(index) => {
                let function = module.functions.get(index as usize).ok_or("invalid exported function")?;
                let ty = module.types.get(function.type_idx as usize).ok_or("invalid exported function type")?;
                let _ = writeln!(out, "define void @w2l_export_{name}(ptr %args, ptr %results) {{\nentry:\n  store i32 0, ptr @w2l_depth");
                let mut args = Vec::new();
                for (i, param) in ty.params.iter().enumerate() {
                    let _ = writeln!(out, "  %arg_ptr_{i} = getelementptr i8, ptr %args, i64 {}\n  %arg_{i} = load {}, ptr %arg_ptr_{i}, align 1", i * 16, param.llvm());
                    args.push(format!("%arg_{i}"));
                }
                let ret = return_type(&ty.results);
                if ty.results.is_empty() {
                    let _ = writeln!(out, "  call void @{}({})", fn_name(index as usize), typed_args(&ty.params, &args));
                } else {
                    let _ = writeln!(out, "  %call_result = call {ret} @{}({})", fn_name(index as usize), typed_args(&ty.params, &args));
                    for (i, result) in ty.results.iter().enumerate() {
                        let value = if ty.results.len() == 1 { "%call_result".to_string() } else {
                            let _ = writeln!(out, "  %result_{i} = extractvalue {ret} %call_result, {i}");
                            format!("%result_{i}")
                        };
                        let _ = writeln!(out, "  %result_ptr_{i} = getelementptr i8, ptr %results, i64 {}\n  store {} {value}, ptr %result_ptr_{i}, align 1", i * 16, result.llvm());
                    }
                }
                out.push_str("  ret void\n}\n\n");
            }
            ExportKind::Global(index) => {
                let global = module.globals.get(index as usize).ok_or("invalid exported global")?;
                let _ = writeln!(out, "define void @w2l_global_{name}(ptr %out) {{\nentry:\n  %value = load {}, ptr @w2l_g{index}\n  store {} %value, ptr %out, align 1\n  ret void\n}}\n", global.ty.llvm(), global.ty.llvm());
            }
            ExportKind::Memory(index) => {
                if index != 0 { return Err("only memory 0 can be exported".into()); }
                out.push_str(&format!("define ptr @w2l_memory_{name}(ptr %size_out) {{\nentry:\n  %size = load i64, ptr @w2l_mem_size\n  store i64 %size, ptr %size_out, align 1\n  %base = load ptr, ptr @w2l_mem_base\n  ret ptr %base\n}}\n\n"));
            }
            ExportKind::Table(_) => {}
        }
    }
    Ok(())
}

pub(crate) fn typed_args(types: &[ValType], args: &[String]) -> String {
    types.iter().zip(args).map(|(ty, value)| format!("{} {value}", ty.llvm())).collect::<Vec<_>>().join(", ")
}

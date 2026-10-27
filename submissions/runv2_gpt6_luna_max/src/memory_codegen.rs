use crate::{
    function::FunctionEmitter,
    wasm::{BulkOp, Instr, ValType},
};

pub fn emit_memory_size_grow(f: &mut FunctionEmitter<'_>, instruction: &Instr) -> Result<(), String> {
    match instruction {
        Instr::MemorySize(_) => {
            let size = f.fresh();
            f.line(&format!("{size} = load i64, ptr @w2l_mem_size"));
            let pages = f.fresh();
            f.line(&format!("{pages} = lshr i64 {size}, 16"));
            let result = f.fresh();
            f.line(&format!("{result} = trunc i64 {pages} to i32"));
            f.push(ValType::I32, &result)
        }
        Instr::MemoryGrow(_) => {
            let delta = f.pop(ValType::I32)?;
            let result = f.fresh();
            f.line(&format!("{result} = call i32 @w2l_memory_grow(i32 {delta})"));
            f.push(ValType::I32, &result)
        }
        _ => Err("expected memory.size or memory.grow".into()),
    }
}

pub fn emit_load(f: &mut FunctionEmitter<'_>, op: u8, _align: u32, offset: u32) -> Result<(), String> {
    let (result_ty, width) = crate::validate::load_type(op).ok_or("invalid load opcode")?;
    let address = f.pop(ValType::I32)?;
    let ptr = emit_mem_pointer(f, &address, offset, width as u64);
    let (small_ty, signed) = match op {
        0x2c => ("i8", Some(true)), 0x2d => ("i8", Some(false)),
        0x2e => ("i16", Some(true)), 0x2f => ("i16", Some(false)),
        0x30 => ("i8", Some(true)), 0x31 => ("i8", Some(false)),
        0x32 => ("i16", Some(true)), 0x33 => ("i16", Some(false)),
        0x34 => ("i32", Some(true)), 0x35 => ("i32", Some(false)),
        _ => (result_ty.llvm(), None),
    };
    let loaded = f.fresh();
    f.line(&format!("{loaded} = load {small_ty}, ptr {ptr}, align 1"));
    let value = if let Some(signed) = signed {
        let extended = f.fresh();
        f.line(&format!("{extended} = {} {small_ty} {loaded} to {}", if signed { "sext" } else { "zext" }, result_ty.llvm()));
        extended
    } else {
        loaded
    };
    f.push(result_ty, &value)
}

pub fn emit_store(f: &mut FunctionEmitter<'_>, op: u8, _align: u32, offset: u32) -> Result<(), String> {
    let (value_ty, width) = crate::validate::store_type(op).ok_or("invalid store opcode")?;
    let value = f.pop(value_ty)?;
    let address = f.pop(ValType::I32)?;
    let ptr = emit_mem_pointer(f, &address, offset, width as u64);
    let stored = match op {
        0x3a | 0x3c => {
            let name = f.fresh(); f.line(&format!("{name} = trunc {} {value} to i8", value_ty.llvm())); name
        }
        0x3b | 0x3d => {
            let name = f.fresh(); f.line(&format!("{name} = trunc {} {value} to i16", value_ty.llvm())); name
        }
        0x3e => {
            let name = f.fresh(); f.line(&format!("{name} = trunc i64 {value} to i32")); name
        }
        _ => value,
    };
    let stored_ty = match op {
        0x3a | 0x3c => "i8", 0x3b | 0x3d => "i16", 0x3e => "i32", _ => value_ty.llvm(),
    };
    f.line(&format!("store {stored_ty} {stored}, ptr {ptr}, align 1"));
    Ok(())
}

pub fn emit_bulk(f: &mut FunctionEmitter<'_>, op: &BulkOp) -> Result<(), String> {
    match op {
        BulkOp::MemoryInit { data, .. } => emit_memory_init(f, *data as usize),
        BulkOp::DataDrop { data } => {
            f.line(&format!("store i64 0, ptr @w2l_data_len_{data}"));
            Ok(())
        }
        BulkOp::MemoryCopy { .. } => {
            let values = f.pop_values(&[ValType::I32, ValType::I32, ValType::I32])?;
            let [dst, src, len] = values.as_slice() else { unreachable!() };
            let length = zext_i32(f, len);
            // The complete ranges are checked before either side is modified.
            check_memory_range(f, src, &length)?;
            check_memory_range(f, dst, &length)?;
            let source = emit_mem_pointer(f, src, 0, 0);
            let dest = emit_mem_pointer(f, dst, 0, 0);
            emit_if_nonzero(f, &length, |f| {
                f.line(&format!("call void @llvm.memmove.p0.p0.i64(ptr {dest}, ptr {source}, i64 {length}, i1 false)"));
                Ok(())
            })
        }
        BulkOp::MemoryFill { .. } => {
            let values = f.pop_values(&[ValType::I32, ValType::I32, ValType::I32])?;
            let [dst, value, len] = values.as_slice() else { unreachable!() };
            let length = zext_i32(f, len);
            check_memory_range(f, dst, &length)?;
            let dest = emit_mem_pointer(f, dst, 0, 0);
            let byte = f.fresh();
            f.line(&format!("{byte} = trunc i32 {value} to i8"));
            emit_if_nonzero(f, &length, |f| {
                f.line(&format!("call void @llvm.memset.p0.i64(ptr {dest}, i8 {byte}, i64 {length}, i1 false)"));
                Ok(())
            })
        }
        BulkOp::TableInit { element, .. } => emit_table_init(f, *element as usize),
        BulkOp::ElementDrop { element } => {
            f.line(&format!("store i64 0, ptr @w2l_elem_len_{element}"));
            Ok(())
        }
        BulkOp::TableCopy { .. } => emit_table_copy(f),
        BulkOp::TableGrow { .. } | BulkOp::TableFill { .. } | BulkOp::TableSize { .. } => {
            Err("table.grow/fill/size require reference types outside the selected feature scope".into())
        }
    }
}

pub(crate) fn emit_mem_pointer(f: &mut FunctionEmitter<'_>, addr: &str, offset: u32, width: u64) -> String {
    let ptr = f.fresh();
    f.line(&format!("{ptr} = call ptr @w2l_mem_ptr(i32 {addr}, i32 {}, i64 {width})", crate::codegen::i32_bits(offset)));
    ptr
}

fn zext_i32(f: &mut FunctionEmitter<'_>, value: &str) -> String {
    let extended = f.fresh();
    f.line(&format!("{extended} = zext i32 {value} to i64"));
    extended
}

fn check_memory_range(f: &mut FunctionEmitter<'_>, addr: &str, len64: &str) -> Result<(), String> {
    let address = zext_i32(f, addr);
    let end = f.fresh();
    f.line(&format!("{end} = add i64 {address}, {len64}"));
    let size = f.fresh();
    f.line(&format!("{size} = load i64, ptr @w2l_mem_size"));
    let oob = f.fresh();
    f.line(&format!("{oob} = icmp ugt i64 {end}, {size}"));
    f.emit_trap_if(&oob, 4);
    Ok(())
}

fn emit_memory_init(f: &mut FunctionEmitter<'_>, data: usize) -> Result<(), String> {
    let values = f.pop_values(&[ValType::I32, ValType::I32, ValType::I32])?;
    let [dst, src, len] = values.as_slice() else { unreachable!() };
    let src64 = zext_i32(f, src);
    let len64 = zext_i32(f, len);
    let src_end = f.fresh();
    f.line(&format!("{src_end} = add i64 {src64}, {len64}"));
    let segment_len = f.fresh();
    f.line(&format!("{segment_len} = load i64, ptr @w2l_data_len_{data}"));
    let src_oob = f.fresh();
    f.line(&format!("{src_oob} = icmp ugt i64 {src_end}, {segment_len}"));
    f.emit_trap_if(&src_oob, 4);
    let dest = emit_mem_pointer(f, dst, 0, 0);
    check_memory_range(f, dst, &len64)?;
    let source = f.fresh();
    f.line(&format!("{source} = getelementptr i8, ptr @w2l_data_{data}, i64 {src64}"));
    emit_if_nonzero(f, &len64, |f| {
        f.line(&format!("call void @llvm.memcpy.p0.p0.i64(ptr {dest}, ptr {source}, i64 {len64}, i1 false)"));
        Ok(())
    })
}

fn emit_table_init(f: &mut FunctionEmitter<'_>, element: usize) -> Result<(), String> {
    let values = f.pop_values(&[ValType::I32, ValType::I32, ValType::I32])?;
    let [dst, src, len] = values.as_slice() else { unreachable!() };
    let src64 = zext_i32(f, src);
    let len64 = zext_i32(f, len);
    let source_end = f.fresh();
    f.line(&format!("{source_end} = add i64 {src64}, {len64}"));
    let elem_len = f.fresh();
    f.line(&format!("{elem_len} = load i64, ptr @w2l_elem_len_{element}"));
    let source_oob = f.fresh();
    f.line(&format!("{source_oob} = icmp ugt i64 {source_end}, {elem_len}"));
    f.emit_trap_if(&source_oob, 5);
    let dst64 = zext_i32(f, dst);
    let dest_end = f.fresh();
    f.line(&format!("{dest_end} = add i64 {dst64}, {len64}"));
    let table_size32 = f.fresh();
    f.line(&format!("{table_size32} = load i32, ptr @w2l_table_size"));
    let table_size = zext_i32(f, &table_size32);
    let dest_oob = f.fresh();
    f.line(&format!("{dest_oob} = icmp ugt i64 {dest_end}, {table_size}"));
    f.emit_trap_if(&dest_oob, 5);
    let table_base = f.fresh();
    f.line(&format!("{table_base} = load ptr, ptr @w2l_table_base"));
    let dest = f.fresh();
    f.line(&format!("{dest} = getelementptr i32, ptr {table_base}, i64 {dst64}"));
    let source = f.fresh();
    f.line(&format!("{source} = getelementptr i32, ptr @w2l_elem_{element}, i64 {src64}"));
    let bytes = f.fresh();
    f.line(&format!("{bytes} = shl i64 {len64}, 2"));
    emit_if_nonzero(f, &bytes, |f| {
        f.line(&format!("call void @llvm.memcpy.p0.p0.i64(ptr {dest}, ptr {source}, i64 {bytes}, i1 false)"));
        Ok(())
    })
}

fn emit_table_copy(f: &mut FunctionEmitter<'_>) -> Result<(), String> {
    let values = f.pop_values(&[ValType::I32, ValType::I32, ValType::I32])?;
    let [dst, src, len] = values.as_slice() else { unreachable!() };
    let dst64 = zext_i32(f, dst);
    let src64 = zext_i32(f, src);
    let len64 = zext_i32(f, len);
    let dst_end = f.fresh();
    let src_end = f.fresh();
    f.line(&format!("{dst_end} = add i64 {dst64}, {len64}"));
    f.line(&format!("{src_end} = add i64 {src64}, {len64}"));
    let size32 = f.fresh();
    f.line(&format!("{size32} = load i32, ptr @w2l_table_size"));
    let size = zext_i32(f, &size32);
    let src_oob = f.fresh();
    f.line(&format!("{src_oob} = icmp ugt i64 {src_end}, {size}"));
    f.emit_trap_if(&src_oob, 5);
    let dst_oob = f.fresh();
    f.line(&format!("{dst_oob} = icmp ugt i64 {dst_end}, {size}"));
    f.emit_trap_if(&dst_oob, 5);
    let base = f.fresh();
    f.line(&format!("{base} = load ptr, ptr @w2l_table_base"));
    let source = f.fresh();
    let dest = f.fresh();
    f.line(&format!("{source} = getelementptr i32, ptr {base}, i64 {src64}"));
    f.line(&format!("{dest} = getelementptr i32, ptr {base}, i64 {dst64}"));
    let bytes = f.fresh();
    f.line(&format!("{bytes} = shl i64 {len64}, 2"));
    emit_if_nonzero(f, &bytes, |f| {
        f.line(&format!("call void @llvm.memmove.p0.p0.i64(ptr {dest}, ptr {source}, i64 {bytes}, i1 false)"));
        Ok(())
    })
}

fn emit_if_nonzero(
    f: &mut FunctionEmitter<'_>,
    length: &str,
    body: impl FnOnce(&mut FunctionEmitter<'_>) -> Result<(), String>,
) -> Result<(), String> {
    let zero = f.fresh();
    f.line(&format!("{zero} = icmp eq i64 {length}, 0"));
    let done = f.fresh_label("bulk_done");
    let copy = f.fresh_label("bulk_nonzero");
    f.cond_branch(&zero, &done, &copy);
    f.start_label(&copy);
    body(f)?;
    f.branch(&done);
    f.start_label(&done);
    Ok(())
}

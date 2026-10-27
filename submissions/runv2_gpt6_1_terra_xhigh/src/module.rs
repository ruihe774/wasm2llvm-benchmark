//! Instance state, ABI entry points, and module-wide LLVM definitions.
use std::collections::BTreeSet;
use std::fmt::Write;
use crate::{codegen, numeric, simd, wasm};
use wasm::{ConstOp, DataMode, ExportDesc, ImportDesc, ElementMode, ElementInit, ValType};

fn hex(s:&str)->String {s.as_bytes().iter().map(|b|format!("{b:02x}")).collect()}
fn data_bytes(b:&[u8])->String {b.iter().map(|x|format!("\\{x:02X}")).collect()}
fn expr(e:&wasm::ConstExpr)->Result<String,String> {Ok(match e.op {
    ConstOp::I32(x)=>(x as u32).to_string(),ConstOp::I64(x)=>(x as u64).to_string(),ConstOp::F32(x)=>x.to_string(),ConstOp::F64(x)=>x.to_string(),
    ConstOp::V128(x)=>u128::from_le_bytes(x).to_string(),ConstOp::RefNull(_)=>"0".into(),ConstOp::RefFunc(x)=>(x as u64+1).to_string(),
    ConstOp::GlobalGet(_)=>return Err("imported globals are not provided by the ABI".into()),
})}
fn offset(e:&wasm::ConstExpr)->Result<String,String> {match e.op { ConstOp::I32(n)=>Ok((n as u32).to_string()),_=>Err("non-i32 segment offset".into())}}

pub fn translate(m:&wasm::Module)->Result<String,String> {
    for i in &m.imports {if !matches!(i.desc,ImportDesc::Func(_)) {return Err("only function imports are supported by the host ABI".into())}}
    // Reference types are not in the payload feature set; the original MVP
    // funcref table and bulk-memory's function-index element segments are.
    if m.tables.len()>1 || m.tables.iter().any(|t| t.element != ValType::FuncRef) {return Err("reference types or multiple tables are outside the supported scope".into())}
    if m.types.iter().any(|t|t.params.iter().chain(&t.results).any(|v|v.is_ref())) || m.globals.iter().any(|g|g.ty.ty.is_ref()) || m.functions.iter().any(|f|f.locals.iter().any(|l|l.ty.is_ref())) || m.elements.iter().any(|e|e.init.iter().any(|x|matches!(x,ElementInit::Expr(_)))) {return Err("reference types are outside the supported scope".into())}
    let memory=m.memory_type(0);
    let max=memory.map(|x|x.limits.max.unwrap_or(65536)).unwrap_or(65536);
    let mut out=String::from("target triple = \"x86_64-unknown-linux-gnu\"\n\n");
    out.push_str(&include_str!("runtime.ll").replace("@W2L_MEMORY_MAX@",&max.to_string()));
    out.push_str(&numeric::declarations());
    out.push_str(&simd::declarations());
    let mut types=Vec::new();for t in &m.types {types.push(codegen::Sig{params:t.params.clone(),results:t.results.clone()});}
    let funcs:Vec<u32>=m.imports.iter().filter_map(|i|match i.desc {ImportDesc::Func(t)=>Some(t),_=>None}).chain(m.functions.iter().map(|f|f.type_index)).collect();
    let env=codegen::Env {types,funcs,globals:m.globals.iter().map(|g|(g.ty.ty,g.ty.mutable)).collect(),memory:memory.is_some(),tables:m.tables.iter().map(|t|t.element).collect(),data_count:m.data.len(),elem_types:m.elements.iter().map(|e|e.ty).collect()};
    for (i,_) in m.globals.iter().enumerate() {writeln!(out,"@w2l_g{i} = internal global i128 0, align 16").unwrap();}
    for (i,d) in m.data.iter().enumerate() {
        writeln!(out,"@w2l_data_{i} = private constant [{} x i8] c\"{}\"",d.bytes.len(),data_bytes(&d.bytes)).unwrap();
        writeln!(out,"@w2l_data_dropped_{i} = internal global i1 false").unwrap();
        writeln!(out,"define internal void @w2l_data_init_{i}(i32 %dst, i32 %src, i32 %n) {{\nentry:\n  %dropped = load i1, ptr @w2l_data_dropped_{i}\n  %size = select i1 %dropped, i64 0, i64 {}\n  %s = zext i32 %src to i64\n  %len = zext i32 %n to i64\n  %end = add i64 %s, %len\n  %bad = icmp ugt i64 %end, %size\n  br i1 %bad, label %trap, label %copy\ntrap:\n  call void @w2l_trap(i32 4)\n  unreachable\ncopy:\n  %p = call ptr @w2l_mem_ptr(i32 %dst, i64 0, i64 %len)\n  %q = getelementptr [{} x i8], ptr @w2l_data_{i}, i64 0, i64 %s\n  call void @llvm.memcpy.p0.p0.i64(ptr %p, ptr %q, i64 %len, i1 false)\n  ret void\n}}",d.bytes.len(),d.bytes.len()).unwrap();
    }
    emit_tables(m,&mut out)?;
    let mut imported=0;
    let mut imported_symbols=BTreeSet::new();
    for i in &m.imports {if let ImportDesc::Func(type_index) = i.desc {
        let symbol=format!("w2l_import_{}_{}",hex(&i.module),hex(&i.name));
        if imported_symbols.insert(symbol.clone()){writeln!(out,"declare void @{symbol}(ptr, ptr)").unwrap();}
        writeln!(out,"define internal void @w2l_f{imported}(ptr %args, ptr %results) {{\nentry:").unwrap();
        let ty=m.types.get(type_index as usize).ok_or("invalid import type")?;
        for j in 0..ty.results.len() {writeln!(out,"  %import_result{j} = getelementptr i128, ptr %results, i64 {j}\n  store i128 0, ptr %import_result{j}, align 1").unwrap();}
        writeln!(out,"  call void @{symbol}(ptr %args, ptr %results)\n  ret void\n}}").unwrap();imported+=1;
    }}
    let mut numeric_ops=BTreeSet::new();
    let mut simd_ops=BTreeSet::new();
    let mut indirect_uses=BTreeSet::new();
    for (i,f) in m.functions.iter().enumerate() {
        let sig=env.types.get(f.type_index as usize).ok_or("invalid function type")?;
        let ir=codegen::compile((imported+i) as u32,&env,sig,&f.locals,&f.body).map_err(|e|format!("function {}: {e}",imported+i))?;
        numeric_ops.extend(ir.numeric);simd_ops.extend(ir.simd);indirect_uses.extend(ir.indirect);out.push_str(&ir.body);
    }
    for op in numeric_ops {out.push_str(&numeric::helper(op)?);}
    for (op,imm) in simd_ops {out.push_str(&simd::helper(op,&imm,&codegen::simd_name(op,&imm))?);}
    emit_indirect(m,&env.funcs,&indirect_uses,&mut out)?;
    if memory.is_some() || !m.tables.is_empty() {
        out.push_str("@llvm.global_dtors = appending global [1 x { i32, ptr, ptr }] [{ i32, ptr, ptr } { i32 65535, ptr @w2l_cleanup, ptr null }]\ndefine internal void @w2l_cleanup() {\nentry:\n");
        if memory.is_some() {
            let reserved=(max as u64*65536).max(65536);
            writeln!(out,"  %memory = load ptr, ptr @w2l_mem_base\n  %mapped = load i1, ptr @w2l_mem_reserved\n  br i1 %mapped, label %unmap, label %free_memory\nunmap:\n  %ignored = call i32 @munmap(ptr %memory, i64 {reserved})\n  br label %free_tables\nfree_memory:\n  call void @free(ptr %memory)\n  br label %free_tables\nfree_tables:").unwrap();
        }
        for i in 0..m.tables.len() {writeln!(out,"  %table{i} = load ptr, ptr @w2l_table_{i}\n  call void @free(ptr %table{i})").unwrap();}
        out.push_str("  ret void\n}\n");
    }
    out.push_str("define void @w2l_init() {\nentry:\n  store i32 0, ptr @w2l_depth\n");
    if let Some(mem)=memory {
        let bytes=mem.limits.min as u64*65536;
        let reserved=(max as u64*65536).max(65536);
        // The mapping is entirely readable/writable; wasm bounds are checked
        // explicitly. Reserving it avoids touching gigabytes of zero pages on grow.
        writeln!(out,"  %mapped = call ptr @mmap(ptr null, i64 {reserved}, i32 3, i32 34, i32 -1, i64 0)\n  %map_failed = icmp eq ptr %mapped, inttoptr (i64 -1 to ptr)\n  br i1 %map_failed, label %memory_fallback, label %memory_mapped\nmemory_mapped:\n  store i1 true, ptr @w2l_mem_reserved\n  br label %memory_ok\nmemory_fallback:\n  %allocated = call ptr @calloc(i64 {}, i64 1)\n  %memory_oom = icmp eq ptr %allocated, null\n  br i1 %memory_oom, label %memory_failure, label %memory_ok\nmemory_failure:\n  call void @w2l_trap(i32 4)\n  unreachable\nmemory_ok:\n  %memory = phi ptr [ %mapped, %memory_mapped ], [ %allocated, %memory_fallback ]\n  store ptr %memory, ptr @w2l_mem_base\n  store i64 {bytes}, ptr @w2l_mem_bytes",bytes.max(1)).unwrap();
    }
    for (i,g) in m.globals.iter().enumerate() {writeln!(out,"  store i128 {}, ptr @w2l_g{i}, align 16",expr(&g.init)?).unwrap();}
    for (i,t) in m.tables.iter().enumerate() {let bytes=(t.limits.min as u64).checked_mul(16).ok_or("table too large")?;writeln!(out,"  %table{i} = call ptr @calloc(i64 {}, i64 1)\n  %table_oom{i} = icmp eq ptr %table{i}, null\n  br i1 %table_oom{i}, label %table_failure{i}, label %table_ok{i}\ntable_failure{i}:\n  call void @w2l_trap(i32 5)\n  unreachable\ntable_ok{i}:\n  store ptr %table{i}, ptr @w2l_table_{i}\n  store i64 {}, ptr @w2l_table_size_{i}",bytes.max(1),t.limits.min).unwrap();}
    for (i,el) in m.elements.iter().enumerate() {
        match &el.mode {ElementMode::Active{table,offset:e}=>{let off=offset(e)?;let len=el.init.len();writeln!(out,"  %elem_dst{i} = call ptr @w2l_table_ptr_{table}(i32 {off}, i32 {len})\n  call void @llvm.memcpy.p0.p0.i64(ptr %elem_dst{i}, ptr @w2l_elem_{i}, i64 {}, i1 false)\n  store i1 true, ptr @w2l_elem_dropped_{i}",len*16).unwrap();},ElementMode::Declarative=>{writeln!(out,"  store i1 true, ptr @w2l_elem_dropped_{i}").unwrap()},ElementMode::Passive=>{}}
    }
    for (i,d) in m.data.iter().enumerate() {if let DataMode::Active{memory:idx,offset:e}=&d.mode {if *idx!=0{return Err("invalid data memory index".into())}writeln!(out,"  call void @w2l_data_init_{i}(i32 {}, i32 0, i32 {})\n  store i1 true, ptr @w2l_data_dropped_{i}",offset(e)?,d.bytes.len()).unwrap();}}
    if let Some(start)=m.start {out.push_str("  %start_args = alloca [1 x i128], align 16\n  %start_results = alloca [1 x i128], align 16\n");writeln!(out,"  call void @w2l_f{start}(ptr %start_args, ptr %start_results)").unwrap();}
    out.push_str("  ret void\n}\n");
    for ex in &m.exports {let h=hex(&ex.name);match ex.desc {
        ExportDesc::Func(i)=>writeln!(out,"define void @w2l_export_{h}(ptr %args, ptr %results) {{\nentry:\n  store i32 0, ptr @w2l_depth\n  call void @w2l_f{i}(ptr %args, ptr %results)\n  ret void\n}}").unwrap(),
        ExportDesc::Global(i)=>writeln!(out,"define void @w2l_global_{h}(ptr %out) {{\nentry:\n  %v = load i128, ptr @w2l_g{i}, align 16\n  store i128 %v, ptr %out, align 1\n  ret void\n}}").unwrap(),
        ExportDesc::Memory(_)=>writeln!(out,"define ptr @w2l_memory_{h}(ptr %out) {{\nentry:\n  %size = load i64, ptr @w2l_mem_bytes\n  store i64 %size, ptr %out, align 1\n  %base = load ptr, ptr @w2l_mem_base\n  ret ptr %base\n}}").unwrap(),
        ExportDesc::Table(_)=>{},
    }}
    Ok(out)
}

fn emit_tables(m:&wasm::Module,out:&mut String)->Result<(),String> {
    for (i,t) in m.tables.iter().enumerate() {
        let max=t.limits.max.unwrap_or(u32::MAX);
        writeln!(out,"@w2l_table_{i} = internal global ptr null\n@w2l_table_size_{i} = internal global i64 0").unwrap();
        writeln!(out,"define internal ptr @w2l_table_ptr_{i}(i32 %index, i32 %count) {{\nentry:\n  %x = zext i32 %index to i64\n  %n = zext i32 %count to i64\n  %end = add i64 %x, %n\n  %size = load i64, ptr @w2l_table_size_{i}\n  %bad = icmp ugt i64 %end, %size\n  br i1 %bad, label %trap, label %good\ntrap:\n  call void @w2l_trap(i32 5)\n  unreachable\ngood:\n  %base = load ptr, ptr @w2l_table_{i}\n  %p = getelementptr i128, ptr %base, i64 %x\n  ret ptr %p\n}}\ndefine internal i128 @w2l_table_get_{i}(i32 %index) {{\nentry:\n  %p = call ptr @w2l_table_ptr_{i}(i32 %index, i32 1)\n  %v = load i128, ptr %p, align 16\n  ret i128 %v\n}}\ndefine internal void @w2l_table_set_{i}(i32 %index, i128 %value) {{\nentry:\n  %p = call ptr @w2l_table_ptr_{i}(i32 %index, i32 1)\n  store i128 %value, ptr %p, align 16\n  ret void\n}}").unwrap();
        writeln!(out,"define internal i32 @w2l_table_grow_{i}(i128 %val, i32 %delta) {{\nentry:\n  %old = load i64, ptr @w2l_table_size_{i}\n  %d = zext i32 %delta to i64\n  %new = add i64 %old, %d\n  %bad = icmp ugt i64 %new, {max}\n  br i1 %bad, label %fail, label %sized\nsized:\n  %zero = icmp eq i32 %delta, 0\n  br i1 %zero, label %done, label %alloc\nalloc:\n  %bytes = shl i64 %new, 4\n  %oldbase = load ptr, ptr @w2l_table_{i}\n  %base = call ptr @realloc(ptr %oldbase, i64 %bytes)\n  %oom = icmp eq ptr %base, null\n  br i1 %oom, label %fail, label %loop\nloop:\n  %idx = phi i64 [ %old, %alloc ], [ %next, %loop ]\n  %p = getelementptr i128, ptr %base, i64 %idx\n  store i128 %val, ptr %p, align 16\n  %next = add i64 %idx, 1\n  %more = icmp ult i64 %next, %new\n  br i1 %more, label %loop, label %commit\ncommit:\n  store ptr %base, ptr @w2l_table_{i}\n  store i64 %new, ptr @w2l_table_size_{i}\n  br label %done\ndone:\n  %result = trunc i64 %old to i32\n  ret i32 %result\nfail:\n  ret i32 -1\n}}").unwrap();
        writeln!(out,"define internal void @w2l_table_fill_{i}(i32 %start, i128 %val, i32 %count) {{\nentry:\n  %p = call ptr @w2l_table_ptr_{i}(i32 %start, i32 %count)\n  %n = zext i32 %count to i64\n  %empty = icmp eq i64 %n, 0\n  br i1 %empty, label %done, label %loop\nloop:\n  %j = phi i64 [ 0, %entry ], [ %next, %loop ]\n  %q = getelementptr i128, ptr %p, i64 %j\n  store i128 %val, ptr %q, align 16\n  %next = add i64 %j, 1\n  %more = icmp ult i64 %next, %n\n  br i1 %more, label %loop, label %done\ndone:\n  ret void\n}}").unwrap();
    }
    for (i,e) in m.elements.iter().enumerate() {
        let mut vals=Vec::new();for v in &e.init {let x=match v {ElementInit::Func(f)=>(*f as u64+1).to_string(),ElementInit::Expr(c)=>expr(c)?};vals.push(format!("i128 {x}"));}
        if vals.is_empty(){writeln!(out,"@w2l_elem_{i} = private constant [0 x i128] zeroinitializer, align 16").unwrap();}
        else {writeln!(out,"@w2l_elem_{i} = private constant [{} x i128] [{}], align 16",vals.len(),vals.join(", ")).unwrap();}
        writeln!(out,"@w2l_elem_dropped_{i} = internal global i1 false\ndefine internal ptr @w2l_elem_ptr_{i}(i32 %index, i32 %count) {{\nentry:\n  %d = load i1, ptr @w2l_elem_dropped_{i}\n  %size = select i1 %d, i64 0, i64 {}\n  %x = zext i32 %index to i64\n  %n = zext i32 %count to i64\n  %end = add i64 %x, %n\n  %bad = icmp ugt i64 %end, %size\n  br i1 %bad, label %trap, label %good\ntrap:\n  call void @w2l_trap(i32 5)\n  unreachable\ngood:\n  %p = getelementptr i128, ptr @w2l_elem_{i}, i64 %x\n  ret ptr %p\n}}",e.init.len()).unwrap();
    }
    Ok(())
}

fn emit_indirect(m:&wasm::Module,funcs:&[u32],uses:&BTreeSet<(u32,u32)>,out:&mut String)->Result<(),String> {
    for &(t,tab) in uses {let ty=m.types.get(t as usize).ok_or("invalid indirect type")?;
        writeln!(out,"define internal void @w2l_indirect_{t}_{tab}(i32 %index, ptr %args, ptr %results) {{\nentry:\n  %ref = call i128 @w2l_table_get_{tab}(i32 %index)\n  %is_null = icmp eq i128 %ref, 0\n  br i1 %is_null, label %null, label %dispatch\nnull:\n  call void @w2l_trap(i32 6)\n  unreachable\ndispatch:\n  switch i128 %ref, label %mismatch [").unwrap();
        let mut compatible=Vec::new();for (f,tid) in funcs.iter().enumerate() {if m.types.get(*tid as usize)==Some(ty){compatible.push(f);writeln!(out,"    i128 {}, label %f{f}",f+1).unwrap();}}
        out.push_str("  ]\nmismatch:\n  call void @w2l_trap(i32 7)\n  unreachable\n");
        for f in compatible {writeln!(out,"f{f}:\n  call void @w2l_f{f}(ptr %args, ptr %results)\n  ret void").unwrap();}
        out.push_str("}\n");
    }Ok(())
}

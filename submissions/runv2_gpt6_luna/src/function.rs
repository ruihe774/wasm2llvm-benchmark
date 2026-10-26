use std::fmt::Write as _;

use crate::wasm::{BlockSig, BulkOp, ConstExpr, FuncType, Instr, Module, SimdImmediate, ValType};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TargetKind { Function, Block, Loop, If }

#[derive(Clone)]
pub(crate) struct ControlTarget {
    kind: TargetKind,
    height: usize,
    params: Vec<ValType>,
    results: Vec<ValType>,
    exit: String,
    loop_header: Option<String>,
    has_break: bool,
}

pub(crate) struct FunctionEmitter<'a> {
    pub(crate) module: &'a Module,
    pub(crate) index: usize,
    pub(crate) ty: FuncType,
    pub(crate) locals: Vec<ValType>,
    pub(crate) out: String,
    pub(crate) stack: Vec<ValType>,
    pub(crate) stack_cap: usize,
    pub(crate) controls: Vec<ControlTarget>,
    pub(crate) temp_id: usize,
    pub(crate) label_id: usize,
    pub(crate) current: bool,
    ret_label: String,
}

pub(crate) fn emit_function(module: &Module, index: usize) -> Result<String, String> {
    let f = module.functions.get(index).ok_or("invalid function index")?;
    if f.import.is_some() { return Err("attempted to emit an import as wasm code".into()); }
    let ty = module.types.get(f.type_idx as usize).ok_or("invalid function type")?.clone();
    let mut locals = ty.params.clone();
    locals.extend_from_slice(&f.locals);
    let stack_cap = f.max_stack.max(1).saturating_add(8);
    let args = ty.params.iter().enumerate().map(|(i, t)| format!("{} %p{i}", t.llvm())).collect::<Vec<_>>().join(", ");
    let mut emitter = FunctionEmitter {
        module, index, ty: ty.clone(), locals, out: String::new(), stack: Vec::new(), stack_cap,
        controls: Vec::new(), temp_id: 0, label_id: 0, current: false, ret_label: "fn_return".into(),
    };
    let ret = crate::codegen::return_type(&ty.results);
    let _ = writeln!(emitter.out, "define internal {ret} @{}({args}) {{", crate::codegen::fn_name(index));
    emitter.start_label("entry");
    emitter.line("call void @w2l_depth_enter()");
    emitter.line(&format!("%stack = alloca [{} x <16 x i8>], align 16", stack_cap));
    let local_types = emitter.locals.clone();
    for (local, local_ty) in local_types.iter().enumerate() {
        emitter.line(&format!("%local{local} = alloca {}, align {}", local_ty.llvm(), local_ty.natural_align()));
        if local >= ty.params.len() {
            emitter.line(&format!("store {} zeroinitializer, ptr %local{local}, align {}", local_ty.llvm(), local_ty.natural_align()));
        } else {
            emitter.line(&format!("store {} %p{local}, ptr %local{local}, align {}", local_ty.llvm(), local_ty.natural_align()));
        }
    }
    emitter.ret_label = emitter.fresh_label("fn_return");
    emitter.controls.push(ControlTarget {
        kind: TargetKind::Function, height: 0, params: Vec::new(), results: ty.results.clone(),
        exit: emitter.ret_label.clone(), loop_header: None, has_break: false,
    });
    emitter.compile_seq(&f.body)?;
    if emitter.current {
        emitter.branch_to_target(0)?;
    }
    if emitter.controls[0].has_break {
        let label = emitter.ret_label.clone();
        emitter.start_label(&label);
        emitter.emit_return_from_stack(&ty.results)?;
    }
    emitter.out.push_str("}\n");
    Ok(emitter.out)
}

impl FunctionEmitter<'_> {
    pub(crate) fn line(&mut self, text: &str) {
        debug_assert!(self.current, "instruction emitted after terminator");
        let _ = writeln!(self.out, "  {text}");
    }

    pub(crate) fn fresh(&mut self) -> String {
        let id = self.temp_id;
        self.temp_id += 1;
        format!("%t{id}")
    }

    pub(crate) fn fresh_label(&mut self, prefix: &str) -> String {
        let id = self.label_id;
        self.label_id += 1;
        format!("{prefix}_{id}")
    }

    pub(crate) fn start_label(&mut self, label: &str) {
        debug_assert!(!self.current, "label inserted before previous block was terminated");
        let _ = writeln!(self.out, "{label}:");
        self.current = true;
    }

    pub(crate) fn branch(&mut self, label: &str) {
        self.line(&format!("br label %{label}"));
        self.current = false;
    }

    pub(crate) fn cond_branch(&mut self, cond: &str, yes: &str, no: &str) {
        self.line(&format!("br i1 {cond}, label %{yes}, label %{no}"));
        self.current = false;
    }

    pub(crate) fn emit_trap_if(&mut self, cond: &str, code: u32) {
        let trap = self.fresh_label("trap");
        let ok = self.fresh_label("checked");
        self.cond_branch(cond, &trap, &ok);
        self.start_label(&trap);
        self.line(&format!("call void @w2l_trap(i32 {code})"));
        self.line("unreachable");
        self.current = false;
        self.start_label(&ok);
    }

    fn terminate_unreachable(&mut self) {
        self.line("unreachable");
        self.current = false;
    }

    pub(crate) fn slot_ptr(&mut self, index: usize) -> String {
        let ptr = self.fresh();
        self.line(&format!("{ptr} = getelementptr [{} x <16 x i8>], ptr %stack, i32 0, i32 {index}", self.stack_cap));
        ptr
    }

    pub(crate) fn load_slot(&mut self, index: usize, ty: ValType) -> String {
        let ptr = self.slot_ptr(index);
        let value = self.fresh();
        self.line(&format!("{value} = load {}, ptr {ptr}, align 1", ty.llvm()));
        value
    }

    pub(crate) fn store_slot(&mut self, index: usize, ty: ValType, value: &str) {
        let ptr = self.slot_ptr(index);
        self.line(&format!("store {} {value}, ptr {ptr}, align 1", ty.llvm()));
    }

    pub(crate) fn pop(&mut self, expected: ValType) -> Result<String, String> {
        let index = self.stack.len().checked_sub(1).ok_or_else(|| self.error("code generation stack underflow"))?;
        let actual = self.stack.pop().unwrap();
        if actual != expected { return Err(self.error(format!("code generation type mismatch: got {actual}, expected {expected}"))); }
        Ok(self.load_slot(index, expected))
    }

    pub(crate) fn pop_any(&mut self) -> Result<(ValType, String), String> {
        let index = self.stack.len().checked_sub(1).ok_or_else(|| self.error("code generation stack underflow"))?;
        let ty = self.stack.pop().unwrap();
        let value = self.load_slot(index, ty);
        Ok((ty, value))
    }

    pub(crate) fn pop_values(&mut self, types: &[ValType]) -> Result<Vec<String>, String> {
        let mut values = Vec::with_capacity(types.len());
        for &ty in types.iter().rev() { values.push(self.pop(ty)?); }
        values.reverse();
        Ok(values)
    }

    pub(crate) fn push(&mut self, ty: ValType, value: &str) -> Result<(), String> {
        if self.stack.len() >= self.stack_cap { return Err(self.error("validated operand stack exceeds allocation")); }
        self.store_slot(self.stack.len(), ty, value);
        self.stack.push(ty);
        Ok(())
    }

    pub(crate) fn push_zero(&mut self, ty: ValType) -> Result<(), String> {
        self.push(ty, "zeroinitializer")
    }

    pub(crate) fn emit_const(&mut self, value: &ConstExpr) -> Result<(), String> {
        match value {
            ConstExpr::I32(v) => self.push(ValType::I32, &v.to_string()),
            ConstExpr::I64(v) => self.push(ValType::I64, &v.to_string()),
            ConstExpr::F32(bits) => {
                let name = self.fresh();
                self.line(&format!("{name} = bitcast i32 {} to float", crate::codegen::i32_bits(*bits)));
                self.push(ValType::F32, &name)
            }
            ConstExpr::F64(bits) => {
                let name = self.fresh();
                self.line(&format!("{name} = bitcast i64 {} to double", crate::codegen::i64_bits(*bits)));
                self.push(ValType::F64, &name)
            }
        }
    }

    fn error(&self, text: impl AsRef<str>) -> String {
        format!("function {}: {}", self.index, text.as_ref())
    }

    fn compile_seq(&mut self, body: &[Instr]) -> Result<(), String> {
        for instruction in body {
            if !self.current { break; }
            self.compile_instr(instruction)?;
        }
        Ok(())
    }

    fn compile_instr(&mut self, instr: &Instr) -> Result<(), String> {
        match instr {
            Instr::Unreachable => {
                self.line("call void @w2l_trap(i32 0)");
                self.terminate_unreachable();
            }
            Instr::Nop => {}
            Instr::Block(sig, body) => self.compile_block(sig, body)?,
            Instr::Loop(sig, body) => self.compile_loop(sig, body)?,
            Instr::If(sig, then_body, else_body) => self.compile_if(sig, then_body, else_body.as_deref())?,
            Instr::Br(depth) => self.branch_to_target(*depth as usize)?,
            Instr::BrIf(depth) => self.branch_if(*depth as usize)?,
            Instr::BrTable(targets, default) => self.branch_table(targets, *default)?,
            Instr::Return => {
                let results = self.ty.results.clone();
                let values = self.pop_values(&results)?;
                self.line("call void @w2l_depth_leave()");
                self.emit_return_values(&results, &values);
                self.current = false;
            }
            Instr::Call(index) => {
                let ty = self.module.types[self.module.functions[*index as usize].type_idx as usize].clone();
                let args = self.pop_values(&ty.params)?;
                let results = self.call_function(*index as usize, &ty, &args)?;
                for (result_ty, value) in ty.results.iter().zip(results) { self.push(*result_ty, &value)?; }
            }
            Instr::CallIndirect { ty, .. } => self.call_indirect(*ty as usize)?,
            Instr::Drop => { let _ = self.pop_any()?; }
            Instr::Select(_) => {
                let condition = self.pop(ValType::I32)?;
                let (ty_b, b) = self.pop_any()?;
                let (ty_a, a) = self.pop_any()?;
                if ty_a != ty_b { return Err(self.error("select operands have different types")); }
                let result = self.fresh();
                let cond = self.truthy(&condition);
                self.line(&format!("{result} = select i1 {cond}, {} {a}, {} {b}", ty_a.llvm(), ty_a.llvm()));
                self.push(ty_a, &result)?;
            }
            Instr::LocalGet(index) => {
                let ty = *self.locals.get(*index as usize).ok_or_else(|| self.error("local index out of range"))?;
                let value = self.fresh();
                self.line(&format!("{value} = load {}, ptr %local{index}, align {}", ty.llvm(), ty.natural_align()));
                self.push(ty, &value)?;
            }
            Instr::LocalSet(index) => {
                let ty = *self.locals.get(*index as usize).ok_or_else(|| self.error("local index out of range"))?;
                let value = self.pop(ty)?;
                self.line(&format!("store {} {value}, ptr %local{index}, align {}", ty.llvm(), ty.natural_align()));
            }
            Instr::LocalTee(index) => {
                let ty = *self.locals.get(*index as usize).ok_or_else(|| self.error("local index out of range"))?;
                let value = self.pop(ty)?;
                self.line(&format!("store {} {value}, ptr %local{index}, align {}", ty.llvm(), ty.natural_align()));
                self.push(ty, &value)?;
            }
            Instr::GlobalGet(index) => {
                let global = self.module.globals.get(*index as usize).ok_or_else(|| self.error("global index out of range"))?;
                let value = self.fresh();
                self.line(&format!("{value} = load {}, ptr @w2l_g{index}", global.ty.llvm()));
                self.push(global.ty, &value)?;
            }
            Instr::GlobalSet(index) => {
                let global = self.module.globals.get(*index as usize).ok_or_else(|| self.error("global index out of range"))?;
                let ty = global.ty;
                let value = self.pop(ty)?;
                self.line(&format!("store {} {value}, ptr @w2l_g{index}", ty.llvm()));
            }
            Instr::Load { op, align, offset } => crate::memory_codegen::emit_load(self, *op, *align, *offset)?,
            Instr::Store { op, align, offset } => crate::memory_codegen::emit_store(self, *op, *align, *offset)?,
            Instr::MemorySize(_) | Instr::MemoryGrow(_) => crate::memory_codegen::emit_memory_size_grow(self, instr)?,
            Instr::Const(value) => self.emit_const(value)?,
            Instr::Numeric(op) => crate::numeric::emit_numeric(self, *op)?,
            Instr::SignExtend(op) => crate::numeric::emit_sign_extend(self, *op)?,
            Instr::Saturating(op) => crate::numeric::emit_saturating(self, *op)?,
            Instr::Bulk(op) => crate::memory_codegen::emit_bulk(self, op)?,
            Instr::Simd { op, immediate } => crate::simd_codegen::emit_simd(self, *op, immediate)?,
        }
        Ok(())
    }

    fn truthy(&mut self, value: &str) -> String {
        let result = self.fresh();
        self.line(&format!("{result} = icmp ne i32 {value}, 0"));
        result
    }

    fn compile_block(&mut self, sig: &BlockSig, body: &[Instr]) -> Result<(), String> {
        let height = self.stack.len().checked_sub(sig.params.len()).ok_or_else(|| self.error("block parameters missing"))?;
        let exit = self.fresh_label("block_exit");
        self.controls.push(ControlTarget {
            kind: TargetKind::Block, height, params: sig.params.clone(), results: sig.results.clone(),
            exit: exit.clone(), loop_header: None, has_break: false,
        });
        self.compile_seq(body)?;
        let normal = self.current;
        if normal {
            self.copy_to_slot_base(height, &sig.results)?;
            self.branch(&exit);
        }
        let target = self.controls.pop().unwrap();
        self.stack.truncate(height);
        self.stack.extend_from_slice(&sig.results);
        if normal || target.has_break {
            self.start_label(&exit);
        }
        self.current = normal || target.has_break;
        Ok(())
    }

    fn compile_loop(&mut self, sig: &BlockSig, body: &[Instr]) -> Result<(), String> {
        let height = self.stack.len().checked_sub(sig.params.len()).ok_or_else(|| self.error("loop parameters missing"))?;
        let header = self.fresh_label("loop_head");
        let exit = self.fresh_label("loop_exit");
        self.branch(&header);
        self.start_label(&header);
        self.controls.push(ControlTarget {
            kind: TargetKind::Loop, height, params: sig.params.clone(), results: sig.results.clone(),
            exit: exit.clone(), loop_header: Some(header), has_break: false,
        });
        self.compile_seq(body)?;
        let normal = self.current;
        if normal {
            self.copy_to_slot_base(height, &sig.results)?;
            self.branch(&exit);
        }
        let target = self.controls.pop().unwrap();
        self.stack.truncate(height);
        self.stack.extend_from_slice(&sig.results);
        if normal || target.has_break { self.start_label(&exit); }
        self.current = normal || target.has_break;
        Ok(())
    }

    fn compile_if(&mut self, sig: &BlockSig, then_body: &[Instr], else_body: Option<&[Instr]>) -> Result<(), String> {
        let condition = self.pop(ValType::I32)?;
        let cond = self.truthy(&condition);
        let height = self.stack.len().checked_sub(sig.params.len()).ok_or_else(|| self.error("if parameters missing"))?;
        let starting_stack = self.stack.clone();
        let then_label = self.fresh_label("if_then");
        let else_label = self.fresh_label("if_else");
        let exit = self.fresh_label("if_exit");
        self.cond_branch(&cond, &then_label, &else_label);
        self.controls.push(ControlTarget {
            kind: TargetKind::If, height, params: sig.params.clone(), results: sig.results.clone(),
            exit: exit.clone(), loop_header: None, has_break: false,
        });

        self.start_label(&then_label);
        self.compile_seq(then_body)?;
        let mut normal = self.current;
        if normal {
            self.copy_to_slot_base(height, &sig.results)?;
            self.branch(&exit);
        }

        self.stack = starting_stack.clone();
        self.start_label(&else_label);
        if let Some(body) = else_body {
            self.compile_seq(body)?;
        }
        if self.current {
            self.copy_to_slot_base(height, &sig.results)?;
            self.branch(&exit);
            normal = true;
        }
        let target = self.controls.pop().unwrap();
        self.stack.truncate(height);
        self.stack.extend_from_slice(&sig.results);
        if normal || target.has_break { self.start_label(&exit); }
        self.current = normal || target.has_break;
        Ok(())
    }

    fn target_info(&mut self, depth: usize) -> Result<(String, usize, Vec<ValType>, bool), String> {
        let index = self.controls.len().checked_sub(depth + 1).ok_or_else(|| self.error("branch depth out of bounds"))?;
        let target = &mut self.controls[index];
        let loop_target = target.kind == TargetKind::Loop;
        let label = if loop_target { target.loop_header.as_ref().unwrap().clone() } else { target.exit.clone() };
        let types = if loop_target { target.params.clone() } else { target.results.clone() };
        if !loop_target { target.has_break = true; }
        Ok((label, target.height, types, loop_target))
    }

    fn copy_to_slot_base(&mut self, height: usize, types: &[ValType]) -> Result<(), String> {
        if self.stack.len() < types.len() { return Err(self.error("branch result values are missing")); }
        let source = self.stack.len() - types.len();
        for (i, &ty) in types.iter().enumerate() {
            let value = self.load_slot(source + i, ty);
            self.store_slot(height + i, ty, &value);
        }
        Ok(())
    }

    fn branch_to_target(&mut self, depth: usize) -> Result<(), String> {
        let (label, height, types, _) = self.target_info(depth)?;
        self.copy_to_slot_base(height, &types)?;
        self.branch(&label);
        Ok(())
    }

    fn branch_if(&mut self, depth: usize) -> Result<(), String> {
        let condition = self.pop(ValType::I32)?;
        let cond = self.truthy(&condition);
        let (target, height, types, _) = self.target_info(depth)?;
        let take = self.fresh_label("br_if_take");
        let fallthrough = self.fresh_label("br_if_next");
        self.cond_branch(&cond, &take, &fallthrough);
        self.start_label(&take);
        self.copy_to_slot_base(height, &types)?;
        self.branch(&target);
        self.start_label(&fallthrough);
        Ok(())
    }

    fn branch_table(&mut self, targets: &[u32], default: u32) -> Result<(), String> {
        let selector = self.pop(ValType::I32)?;
        let mut branches = Vec::with_capacity(targets.len() + 1);
        for &depth in targets.iter().chain(std::iter::once(&default)) {
            let (target, height, types, _) = self.target_info(depth as usize)?;
            branches.push((target, height, types, self.fresh_label("br_table_case")));
        }
        let default_label = branches.last().unwrap().3.clone();
        let cases = branches.iter().enumerate().take(targets.len()).map(|(i, b)| format!("i32 {i}, label %{}", b.3)).collect::<Vec<_>>().join(" ");
        self.line(&format!("switch i32 {selector}, label %{default_label} [ {cases} ]"));
        self.current = false;
        for (target, height, types, case_label) in branches {
            self.start_label(&case_label);
            self.copy_to_slot_base(height, &types)?;
            self.branch(&target);
        }
        Ok(())
    }

    fn call_indirect(&mut self, type_index: usize) -> Result<(), String> {
        let ty = self.module.types.get(type_index).ok_or_else(|| self.error("call_indirect type index out of range"))?.clone();
        let index = self.pop(ValType::I32)?;
        let args = self.pop_values(&ty.params)?;
        let base = self.stack.len();
        let size = self.fresh();
        self.line(&format!("{size} = load i32, ptr @w2l_table_size"));
        let oob = self.fresh();
        self.line(&format!("{oob} = icmp uge i32 {index}, {size}"));
        self.emit_trap_if(&oob, 5);
        let table = self.fresh();
        self.line(&format!("{table} = load ptr, ptr @w2l_table_base"));
        let index64 = self.fresh();
        self.line(&format!("{index64} = zext i32 {index} to i64"));
        let slot = self.fresh();
        self.line(&format!("{slot} = getelementptr i32, ptr {table}, i64 {index64}"));
        let function_index = self.fresh();
        self.line(&format!("{function_index} = load i32, ptr {slot}, align 4"));
        let is_null = self.fresh();
        self.line(&format!("{is_null} = icmp eq i32 {function_index}, -1"));
        self.emit_trap_if(&is_null, 6);

        let mut cases = Vec::new();
        for (function_index, function) in self.module.functions.iter().enumerate() {
            let actual = &self.module.types[function.type_idx as usize];
            if actual == &ty { cases.push((function_index, self.fresh_label("indirect_case"))); }
        }
        let mismatch = self.fresh_label("indirect_mismatch");
        let merge = self.fresh_label("indirect_merge");
        let case_text = cases.iter().map(|(index, label)| format!("i32 {}, label %{label}", crate::codegen::i32_bits(*index as u32))).collect::<Vec<_>>().join(" ");
        self.line(&format!("switch i32 {function_index}, label %{mismatch} [ {case_text} ]"));
        self.current = false;
        for (function_index, label) in &cases {
            self.start_label(label);
            let values = self.call_function(*function_index, &ty, &args)?;
            for (slot, (result_ty, value)) in ty.results.iter().zip(values).enumerate() {
                self.store_slot(base + slot, *result_ty, &value);
            }
            self.branch(&merge);
        }
        self.start_label(&mismatch);
        self.line("call void @w2l_trap(i32 7)");
        self.terminate_unreachable();
        if cases.is_empty() {
            self.current = false;
            return Ok(());
        }
        self.start_label(&merge);
        for (slot, result_ty) in ty.results.iter().enumerate() {
            let value = self.load_slot(base + slot, *result_ty);
            self.push(*result_ty, &value)?;
        }
        Ok(())
    }

    pub(crate) fn call_function(&mut self, index: usize, ty: &FuncType, args: &[String]) -> Result<Vec<String>, String> {
        if args.len() != ty.params.len() { return Err(self.error("internal call argument count mismatch")); }
        let ret = crate::codegen::return_type(&ty.results);
        let target = crate::codegen::fn_name(index);
        if ty.results.is_empty() {
            self.line(&format!("call void @{target}({})", crate::codegen::typed_args(&ty.params, args)));
            return Ok(Vec::new());
        }
        let call = self.fresh();
        self.line(&format!("{call} = call {ret} @{target}({})", crate::codegen::typed_args(&ty.params, args)));
        if ty.results.len() == 1 { return Ok(vec![call]); }
        let mut values = Vec::with_capacity(ty.results.len());
        for (i, ty) in ty.results.iter().enumerate() {
            let value = self.fresh();
            self.line(&format!("{value} = extractvalue {ret} {call}, {i}"));
            let _ = ty;
            values.push(value);
        }
        Ok(values)
    }

    fn emit_return_from_stack(&mut self, results: &[ValType]) -> Result<(), String> {
        let values = results.iter().enumerate().map(|(i, ty)| self.load_slot(i, *ty)).collect::<Vec<_>>();
        self.line("call void @w2l_depth_leave()");
        self.emit_return_values(results, &values);
        self.current = false;
        Ok(())
    }

    fn emit_return_values(&mut self, results: &[ValType], values: &[String]) {
        match results.len() {
            0 => self.line("ret void"),
            1 => self.line(&format!("ret {} {}", results[0].llvm(), values[0])),
            n => {
                let ty = crate::codegen::return_type(results);
                let mut current = "zeroinitializer".to_string();
                for (i, (result, value)) in results.iter().zip(values).enumerate() {
                    let agg = self.fresh();
                    self.line(&format!("{agg} = insertvalue {ty} {current}, {} {value}, {i}", result.llvm()));
                    current = agg;
                }
                let _ = n;
                self.line(&format!("ret {ty} {current}"));
            }
        }
    }
}

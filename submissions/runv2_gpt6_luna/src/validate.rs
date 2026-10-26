use crate::wasm::{BulkOp, BlockSig, ConstExpr, ExportKind, FuncType, Instr, Module, SegmentMode, ValType};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ControlKind {
    Function,
    Block,
    Loop,
    If,
}

struct ControlFrame {
    kind: ControlKind,
    height: usize,
    params: Vec<ValType>,
    results: Vec<ValType>,
    unreachable: bool,
}

struct Validator<'a> {
    module: &'a Module,
    function_index: usize,
    locals: Vec<ValType>,
    stack: Vec<Option<ValType>>,
    controls: Vec<ControlFrame>,
    max_stack: usize,
}

pub fn validate_module(module: &mut Module) -> Result<(), String> {
    for index in 0..module.functions.len() {
        let function = &module.functions[index];
        if function.import.is_some() {
            continue;
        }
        let type_idx = function.type_idx as usize;
        let function_body = function.body.clone();
        let mut locals = module.types.get(type_idx)
            .ok_or_else(|| format!("function {index} has an invalid type index"))?
            .params.clone();
        locals.extend_from_slice(&function.locals);
        let max_stack = {
            let ty = &module.types[type_idx];
            let mut validator = Validator {
                module,
                function_index: index,
                locals,
                stack: Vec::new(),
                controls: vec![ControlFrame {
                    kind: ControlKind::Function,
                    height: 0,
                    params: Vec::new(),
                    results: ty.results.clone(),
                    unreachable: false,
                }],
                max_stack: 0,
            };
            validator.validate_seq(&function_body)?;
            validator.finish_current()?;
            if validator.controls.len() != 1 || !validator.stack.is_empty() {
                return Err(validator.error("function body has an invalid final stack"));
            }
            validator.max_stack.max(1)
        };
        module.functions[index].max_stack = max_stack;
    }

    if uses_memory_init_or_drop(module.functions.iter().filter(|f| f.import.is_none()).map(|f| &f.body))
        && module.declared_data_count.is_none()
    {
        return Err("memory.init/data.drop requires a data-count section".into());
    }
    if let Some(count) = module.declared_data_count {
        if count as usize != module.data.len() {
            return Err("data count does not match data section".into());
        }
    }
    validate_segments(module)?;
    Ok(())
}

fn validate_segments(module: &Module) -> Result<(), String> {
    for (index, segment) in module.elements.iter().enumerate() {
        if let SegmentMode::Active { index: table, offset } = &segment.mode {
            if *table != 0 || module.tables.is_empty() {
                return Err(format!("element segment {index} uses an invalid table"));
            }
            if offset.ty() != ValType::I32 {
                return Err(format!("element segment {index} offset is not i32"));
            }
        }
        for &func in &segment.functions {
            if func as usize >= module.functions.len() {
                return Err(format!("element segment {index} function index is out of bounds"));
            }
        }
    }
    for (index, segment) in module.data.iter().enumerate() {
        if let SegmentMode::Active { index: memory, offset } = &segment.mode {
            if *memory != 0 || module.memories.is_empty() {
                return Err(format!("data segment {index} uses an invalid memory"));
            }
            if offset.ty() != ValType::I32 {
                return Err(format!("data segment {index} offset is not i32"));
            }
        }
    }
    Ok(())
}

fn uses_memory_init_or_drop<'a>(mut bodies: impl Iterator<Item = &'a Vec<Instr>>) -> bool {
    fn in_seq(seq: &[Instr]) -> bool {
        seq.iter().any(|instr| match instr {
            Instr::Bulk(BulkOp::MemoryInit { .. } | BulkOp::DataDrop { .. }) => true,
            Instr::Block(_, body) | Instr::Loop(_, body) => in_seq(body),
            Instr::If(_, then_body, else_body) => in_seq(then_body) || else_body.as_deref().is_some_and(in_seq),
            _ => false,
        })
    }
    bodies.any(|body| in_seq(body))
}

impl Validator<'_> {
    fn error(&self, message: impl AsRef<str>) -> String {
        format!("function {}: {}", self.function_index, message.as_ref())
    }

    fn current(&self) -> &ControlFrame {
        self.controls.last().expect("function control frame")
    }

    fn current_mut(&mut self) -> &mut ControlFrame {
        self.controls.last_mut().expect("function control frame")
    }

    fn push(&mut self, ty: ValType) {
        self.stack.push(Some(ty));
        self.max_stack = self.max_stack.max(self.stack.len());
    }

    fn pop_any(&mut self) -> Result<Option<ValType>, String> {
        let frame = self.current();
        if self.stack.len() == frame.height && frame.unreachable {
            return Ok(None);
        }
        self.stack.pop().ok_or_else(|| self.error("operand stack underflow"))
    }

    fn pop_expect(&mut self, expected: ValType) -> Result<(), String> {
        if self.pop_any()?.is_some_and(|actual| actual != expected) {
            return Err(self.error(format!("type mismatch, expected {expected}")));
        }
        Ok(())
    }

    fn pop_types(&mut self, types: &[ValType]) -> Result<(), String> {
        for &ty in types.iter().rev() {
            self.pop_expect(ty)?;
        }
        Ok(())
    }

    fn make_unreachable(&mut self) {
        let height = self.current().height;
        self.stack.truncate(height);
        self.current_mut().unreachable = true;
    }

    fn validate_seq(&mut self, seq: &[Instr]) -> Result<(), String> {
        for (index, instruction) in seq.iter().enumerate() {
            self.validate_instr(instruction).map_err(|error| {
                format!("{error} (at instruction {index}: {})", instruction_context(instruction))
            })?;
        }
        Ok(())
    }

    fn validate_instr(&mut self, instr: &Instr) -> Result<(), String> {
        match instr {
            Instr::Unreachable => self.make_unreachable(),
            Instr::Nop => {}
            Instr::Block(sig, body) => self.validate_control(sig, ControlKind::Block, body, None)?,
            Instr::Loop(sig, body) => self.validate_control(sig, ControlKind::Loop, body, None)?,
            Instr::If(sig, then_body, else_body) => {
                self.pop_expect(ValType::I32)?;
                if else_body.is_none() && sig.params != sig.results {
                    return Err(self.error("if without else must have identical parameter and result types"));
                }
                self.validate_control(sig, ControlKind::If, then_body, else_body.as_deref())?;
            }
            Instr::Br(depth) => {
                let labels = self.label_types(*depth)?;
                self.pop_types(&labels)?;
                self.make_unreachable();
            }
            Instr::BrIf(depth) => {
                self.pop_expect(ValType::I32)?;
                let labels = self.label_types(*depth)?;
                self.pop_types(&labels)?;
                for ty in labels {
                    self.push(ty);
                }
            }
            Instr::BrTable(targets, default) => {
                self.pop_expect(ValType::I32)?;
                let expected = self.label_types(*default)?;
                for &depth in targets {
                    if self.label_types(depth)? != expected {
                        return Err(self.error("br_table target label types do not match"));
                    }
                }
                self.pop_types(&expected)?;
                self.make_unreachable();
            }
            Instr::Return => {
                let results = self.controls[0].results.clone();
                self.pop_types(&results)?;
                self.make_unreachable();
            }
            Instr::Call(index) => {
                let ty = self.function_type(*index)?.clone();
                self.pop_types(&ty.params)?;
                for result in ty.results {
                    self.push(result);
                }
            }
            Instr::CallIndirect { ty, table } => {
                if *table != 0 || self.module.tables.is_empty() {
                    return Err(self.error("call_indirect requires table 0"));
                }
                let ty = self.module.types.get(*ty as usize)
                    .ok_or_else(|| self.error("call_indirect type index is out of bounds"))?.clone();
                self.pop_expect(ValType::I32)?;
                self.pop_types(&ty.params)?;
                for result in ty.results {
                    self.push(result);
                }
            }
            Instr::Drop => { let _ = self.pop_any()?; }
            Instr::Select(ty) => {
                self.pop_expect(ValType::I32)?;
                let b = self.pop_any()?;
                let a = self.pop_any()?;
                if let (Some(a), Some(b)) = (a, b) {
                    if a != b {
                        return Err(self.error("select operands have different types"));
                    }
                }
                self.push(a.or(b).unwrap_or(ValType::I32));
                if ty.is_some() {
                    return Err(self.error("typed select is unsupported"));
                }
            }
            Instr::LocalGet(index) => self.push(self.local_type(*index)?),
            Instr::LocalSet(index) => {
                let ty = self.local_type(*index)?;
                self.pop_expect(ty)?;
            }
            Instr::LocalTee(index) => {
                let ty = self.local_type(*index)?;
                self.pop_expect(ty)?;
                self.push(ty);
            }
            Instr::GlobalGet(index) => self.push(self.global(*index)?.ty),
            Instr::GlobalSet(index) => {
                let global = self.global(*index)?;
                if !global.mutable {
                    return Err(self.error("global.set targets an immutable global"));
                }
                self.pop_expect(global.ty)?;
            }
            Instr::Load { op, align, .. } => {
                let (ty, width) = load_type(*op).ok_or_else(|| self.error("invalid load opcode"))?;
                check_align(*align, width, self)?;
                self.require_memory()?;
                self.pop_expect(ValType::I32)?;
                self.push(ty);
            }
            Instr::Store { op, align, .. } => {
                let (ty, width) = store_type(*op).ok_or_else(|| self.error("invalid store opcode"))?;
                check_align(*align, width, self)?;
                self.require_memory()?;
                self.pop_expect(ty)?;
                self.pop_expect(ValType::I32)?;
            }
            Instr::MemorySize(index) | Instr::MemoryGrow(index) => {
                if *index != 0 || self.module.memories.is_empty() {
                    return Err(self.error("memory instruction requires memory 0"));
                }
                if matches!(instr, Instr::MemoryGrow(_)) {
                    self.pop_expect(ValType::I32)?;
                    self.push(ValType::I32);
                } else {
                    self.push(ValType::I32);
                }
            }
            Instr::Const(value) => self.push(value.ty()),
            Instr::Numeric(op) => {
                let (inputs, output) = numeric_signature(*op).ok_or_else(|| self.error("invalid numeric opcode"))?;
                self.pop_types(&inputs)?;
                if let Some(output) = output { self.push(output); }
            }
            Instr::SignExtend(op) => {
                let ty = match op {
                    0xc0 | 0xc1 => ValType::I32,
                    0xc2..=0xc4 => ValType::I64,
                    _ => return Err(self.error("invalid sign-extension opcode")),
                };
                self.pop_expect(ty)?;
                self.push(ty);
            }
            Instr::Saturating(op) => {
                let (input, output) = saturating_signature(*op).ok_or_else(|| self.error("invalid saturating conversion opcode"))?;
                self.pop_expect(input)?;
                self.push(output);
            }
            Instr::Bulk(op) => self.validate_bulk(op)?,
            Instr::Simd { op, immediate } => self.validate_simd(*op, immediate)?,
        }
        Ok(())
    }

    fn validate_control(
        &mut self,
        sig: &BlockSig,
        kind: ControlKind,
        body: &[Instr],
        else_body: Option<&[Instr]>,
    ) -> Result<(), String> {
        self.pop_types(&sig.params)?;
        let height = self.stack.len();
        for &ty in &sig.params { self.push(ty); }
        self.controls.push(ControlFrame {
            kind,
            height,
            params: sig.params.clone(),
            results: sig.results.clone(),
            unreachable: false,
        });
        self.validate_seq(body)?;
        self.finish_current()?;
        if let Some(else_body) = else_body {
            let frame = self.controls.last_mut().expect("control frame");
            self.stack.truncate(frame.height);
            for &ty in &frame.params { self.stack.push(Some(ty)); }
            frame.unreachable = false;
            self.validate_seq(else_body)?;
            self.finish_current()?;
        }
        let frame = self.controls.pop().expect("control frame");
        for &ty in &frame.results { self.push(ty); }
        Ok(())
    }

    fn finish_current(&mut self) -> Result<(), String> {
        let (height, results, kind) = {
            let frame = self.current();
            (frame.height, frame.results.clone(), frame.kind)
        };
        self.pop_types(&results)?;
        if self.stack.len() != height {
            return Err(self.error(match kind {
                ControlKind::Function => "function result stack has extra values",
                _ => "block result stack has extra values",
            }));
        }
        Ok(())
    }

    fn label_types(&self, depth: u32) -> Result<Vec<ValType>, String> {
        let index = self.controls.len().checked_sub(depth as usize + 1)
            .ok_or_else(|| self.error("branch depth is out of bounds"))?;
        let frame = &self.controls[index];
        Ok(if frame.kind == ControlKind::Loop { frame.params.clone() } else { frame.results.clone() })
    }

    fn function_type(&self, index: u32) -> Result<&FuncType, String> {
        let function = self.module.functions.get(index as usize)
            .ok_or_else(|| self.error("call function index is out of bounds"))?;
        self.module.types.get(function.type_idx as usize)
            .ok_or_else(|| self.error("function type index is out of bounds"))
    }

    fn local_type(&self, index: u32) -> Result<ValType, String> {
        self.locals.get(index as usize).copied()
            .ok_or_else(|| self.error("local index is out of bounds"))
    }

    fn global(&self, index: u32) -> Result<&crate::wasm::Global, String> {
        self.module.globals.get(index as usize)
            .ok_or_else(|| self.error("global index is out of bounds"))
    }

    fn require_memory(&self) -> Result<(), String> {
        if self.module.memories.is_empty() { Err(self.error("memory instruction requires a memory")) } else { Ok(()) }
    }

    fn validate_bulk(&mut self, op: &BulkOp) -> Result<(), String> {
        match op {
            BulkOp::MemoryInit { data, memory } => {
                if *memory != 0 || self.module.memories.is_empty() || *data as usize >= self.module.data.len() {
                    return Err(self.error("memory.init index is out of bounds"));
                }
                self.pop_types(&[ValType::I32, ValType::I32, ValType::I32])?;
            }
            BulkOp::DataDrop { data } => {
                if *data as usize >= self.module.data.len() { return Err(self.error("data.drop index is out of bounds")); }
            }
            BulkOp::MemoryCopy { dst, src } => {
                if *dst != 0 || *src != 0 || self.module.memories.is_empty() { return Err(self.error("memory.copy index is out of bounds")); }
                self.pop_types(&[ValType::I32, ValType::I32, ValType::I32])?;
            }
            BulkOp::MemoryFill { memory } => {
                if *memory != 0 || self.module.memories.is_empty() { return Err(self.error("memory.fill index is out of bounds")); }
                self.pop_types(&[ValType::I32, ValType::I32, ValType::I32])?;
            }
            BulkOp::TableInit { element, table } => {
                if *table != 0 || self.module.tables.is_empty() || *element as usize >= self.module.elements.len() {
                    return Err(self.error("table.init index is out of bounds"));
                }
                self.pop_types(&[ValType::I32, ValType::I32, ValType::I32])?;
            }
            BulkOp::ElementDrop { element } => {
                if *element as usize >= self.module.elements.len() { return Err(self.error("elem.drop index is out of bounds")); }
            }
            BulkOp::TableCopy { dst, src } => {
                if *dst != 0 || *src != 0 || self.module.tables.is_empty() { return Err(self.error("table.copy index is out of bounds")); }
                self.pop_types(&[ValType::I32, ValType::I32, ValType::I32])?;
            }
            BulkOp::TableGrow { .. } | BulkOp::TableFill { .. } => {
                return Err(self.error("table.grow/fill require reference types, which are outside the supported scope"));
            }
            BulkOp::TableSize { .. } => {
                return Err(self.error("table.size requires reference types, which are outside the supported scope"));
            }
        }
        Ok(())
    }

    fn validate_simd(&mut self, op: u32, immediate: &crate::wasm::SimdImmediate) -> Result<(), String> {
        if let Some((align, width)) = crate::simd::memory_access(op, immediate) {
            if let crate::wasm::SimdImmediate::MemLane { lane, .. } = immediate {
                if *lane >= crate::simd::lane_count(op) {
                    return Err(self.error("SIMD memory lane index is out of bounds"));
                }
            }
            check_align(align, width, self)?;
            self.require_memory()?;
            match op {
                0x0b | 0x54..=0x5b => self.pop_expect(ValType::V128)?,
                _ => {}
            }
            self.pop_expect(ValType::I32)?;
            if matches!(op, 0x00..=0x0a | 0x5c..=0x5d) { self.push(ValType::V128); }
            else if matches!(op, 0x54..=0x57) { self.push(ValType::V128); }
            return Ok(());
        }
        let (inputs, output) = crate::simd::signature(op)
            .ok_or_else(|| self.error(format!("unsupported SIMD opcode 0x{op:x}")))?;
        if let crate::wasm::SimdImmediate::Lane(lane) = immediate {
            if *lane >= crate::simd::lane_count(op) {
                return Err(self.error("SIMD lane index is out of bounds"));
            }
        }
        if let crate::wasm::SimdImmediate::Bytes(bytes) = immediate {
            if op == 0x0d && bytes.iter().any(|&lane| lane >= 32) {
                return Err(self.error("i8x16.shuffle lane index is out of bounds"));
            }
        }
        self.pop_types(&inputs)?;
        if let Some(output) = output { self.push(output); }
        Ok(())
    }
}

fn instruction_context(instruction: &Instr) -> String {
    match instruction {
        Instr::Block(..) => "block".into(),
        Instr::Loop(..) => "loop".into(),
        Instr::If(..) => "if".into(),
        Instr::Numeric(op) => format!("numeric opcode 0x{op:02x}"),
        Instr::Simd { op, .. } => format!("SIMD opcode 0x{op:x}"),
        Instr::Bulk(op) => format!("bulk operation {op:?}"),
        _ => format!("{instruction:?}"),
    }
}

fn check_align(align: u32, width: u32, validator: &Validator<'_>) -> Result<(), String> {
    let maximum = width.trailing_zeros();
    if align > maximum {
        Err(validator.error("memory alignment is larger than the natural alignment"))
    } else {
        Ok(())
    }
}

pub fn load_type(op: u8) -> Option<(ValType, u32)> {
    Some(match op {
        0x28 => (ValType::I32, 4),
        0x29 => (ValType::I64, 8),
        0x2a => (ValType::F32, 4),
        0x2b => (ValType::F64, 8),
        0x2c | 0x2d => (ValType::I32, 1),
        0x2e | 0x2f => (ValType::I32, 2),
        0x30 | 0x31 => (ValType::I64, 1),
        0x32 | 0x33 => (ValType::I64, 2),
        0x34 | 0x35 => (ValType::I64, 4),
        _ => return None,
    })
}

pub fn store_type(op: u8) -> Option<(ValType, u32)> {
    Some(match op {
        0x36 => (ValType::I32, 4),
        0x37 => (ValType::I64, 8),
        0x38 => (ValType::F32, 4),
        0x39 => (ValType::F64, 8),
        0x3a => (ValType::I32, 1),
        0x3b => (ValType::I32, 2),
        0x3c => (ValType::I64, 1),
        0x3d => (ValType::I64, 2),
        0x3e => (ValType::I64, 4),
        _ => return None,
    })
}

fn numeric_signature(op: u8) -> Option<(Vec<ValType>, Option<ValType>)> {
    use ValType::*;
    let unary = |input, output| Some((vec![input], Some(output)));
    let binary = |input, output| Some((vec![input, input], Some(output)));
    match op {
        0x45 => unary(I32, I32),
        0x46..=0x4f => Some((vec![I32, I32], Some(I32))),
        0x50 => unary(I64, I32),
        0x51..=0x5a => Some((vec![I64, I64], Some(I32))),
        0x5b..=0x60 => Some((vec![F32, F32], Some(I32))),
        0x61..=0x66 => Some((vec![F64, F64], Some(I32))),
        0x67..=0x69 => unary(I32, I32),
        0x6a..=0x78 => binary(I32, I32),
        0x79..=0x7b => unary(I64, I64),
        0x7c..=0x8a => binary(I64, I64),
        0x8b..=0x91 => unary(F32, F32),
        0x92..=0x98 => binary(F32, F32),
        0x99..=0x9f => unary(F64, F64),
        0xa0..=0xa6 => binary(F64, F64),
        0xa7 => unary(I64, I32),
        0xa8..=0xab => Some((vec![if op < 0xaa { F32 } else { F64 }], Some(I32))),
        0xac..=0xad => unary(I32, I64),
        0xae..=0xb1 => Some((vec![if op < 0xb0 { F32 } else { F64 }], Some(I64))),
        0xb2..=0xb5 => Some((vec![if op == 0xb2 || op == 0xb3 { I32 } else { I64 }], Some(F32))),
        0xb6 => unary(F64, F32),
        0xb7..=0xba => Some((vec![if op == 0xb7 || op == 0xb8 { I32 } else { I64 }], Some(F64))),
        0xbb => unary(F32, F64),
        0xbc => unary(F32, I32),
        0xbd => unary(F64, I64),
        0xbe => unary(I32, F32),
        0xbf => unary(I64, F64),
        _ => None,
    }
}

fn saturating_signature(op: u32) -> Option<(ValType, ValType)> {
    use ValType::*;
    Some(match op {
        0 => (F32, I32), 1 => (F32, I32),
        2 => (F64, I32), 3 => (F64, I32),
        4 => (F32, I64), 5 => (F32, I64),
        6 => (F64, I64), 7 => (F64, I64),
        _ => return None,
    })
}

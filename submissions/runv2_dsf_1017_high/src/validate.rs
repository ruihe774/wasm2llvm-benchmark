// Validation for the supported WebAssembly subset (2.0 core without reference
// types, plus the SIMD proposal).

use crate::parse::*;

type R<T> = Result<T, String>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Opd {
    T(ValType),
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FrameKind {
    Func,
    Block,
    Loop,
    If,
    Else,
}

struct Ctrl {
    kind: FrameKind,
    start: Vec<ValType>,
    end: Vec<ValType>,
    height: usize,
    unreachable: bool,
}

pub fn validate(m: &Module) -> R<()> {
    // ---- types ----
    for (i, _t) in m.types.iter().enumerate() {
        let _ = i;
    }

    // ---- imports ----
    for imp in &m.imports {
        if (imp.ty as usize) >= m.types.len() {
            return Err(format!("unknown type {} in import", imp.ty));
        }
    }
    for ty in &m.func_types {
        if (*ty as usize) >= m.types.len() {
            return Err(format!("unknown type {} in function section", ty));
        }
    }

    // ---- tables / memories limits ----
    if m.tables.len() > 1 {
        return Err("multiple tables".to_string());
    }
    for t in &m.tables {
        if let Some(mx) = t.max {
            if t.min > mx {
                return Err("size minimum must not be greater than maximum".to_string());
            }
            if mx > u32::MAX as u64 {
                return Err("table size out of range".to_string());
            }
        }
    }
    if m.memories.len() > 1 {
        return Err("multiple memories".to_string());
    }
    for mm in &m.memories {
        if let Some(mx) = mm.max {
            if mm.min > mx {
                return Err("size minimum must not be greater than maximum".to_string());
            }
            if mx > 65536 {
                return Err("memory size must be at most 65536 pages (4GiB)".to_string());
            }
        }
        if mm.min > 65536 {
            return Err("memory size must be at most 65536 pages (4GiB)".to_string());
        }
    }

    // ---- globals ----
    for (i, g) in m.globals.iter().enumerate() {
        const_expr(m, &g.init, g.ty)
            .map_err(|e| format!("global {}: {}", i, e))?;
    }

    // ---- exports ----
    let mut names = std::collections::HashSet::new();
    for e in &m.exports {
        if !names.insert(e.name.clone()) {
            return Err(format!("duplicate export name {:?}", e.name));
        }
        match e.kind {
            0 => {
                if e.idx as usize >= m.num_funcs() {
                    return Err(format!("unknown function {} in export", e.idx));
                }
            }
            1 => {
                if e.idx as usize >= m.tables.len() {
                    return Err(format!("unknown table {} in export", e.idx));
                }
            }
            2 => {
                if e.idx as usize >= m.memories.len() {
                    return Err(format!("unknown memory {} in export", e.idx));
                }
            }
            3 => {
                if e.idx as usize >= m.globals.len() {
                    return Err(format!("unknown global {} in export", e.idx));
                }
            }
            _ => return Err("invalid export kind".to_string()),
        }
    }

    // ---- start ----
    if let Some(s) = m.start {
        let ft = m
            .func_type(s)
            .ok_or_else(|| format!("unknown function {} in start", s))?;
        if !ft.params.is_empty() || !ft.results.is_empty() {
            return Err("start function must have type [] -> []".to_string());
        }
    }

    // ---- element segments ----
    for (i, e) in m.elems.iter().enumerate() {
        if e.mode == ElemMode::Active {
            if e.table as usize >= m.tables.len() {
                return Err(format!("element segment {}: unknown table {}", i, e.table));
            }
            const_expr(m, &e.offset, ValType::I32)
                .map_err(|er| format!("element segment {}: {}", i, er))?;
        }
        for f in &e.funcs {
            if *f as usize >= m.num_funcs() {
                return Err(format!("element segment {}: unknown function {}", i, f));
            }
        }
    }

    // ---- data segments ----
    for (i, d) in m.datas.iter().enumerate() {
        if d.mode == DataMode::Active {
            if d.mem as usize >= m.memories.len() {
                return Err(format!("data segment {}: unknown memory {}", i, d.mem));
            }
            const_expr(m, &d.offset, ValType::I32)
                .map_err(|er| format!("data segment {}: {}", i, er))?;
        }
    }

    if let Some(dc) = m.data_count {
        if dc as usize != m.datas.len() {
            return Err("data count and data section have inconsistent lengths".to_string());
        }
    }

    if m.codes.len() != m.func_types.len() {
        return Err("function and code section have inconsistent lengths".to_string());
    }

    // ---- function bodies ----
    for (i, code) in m.codes.iter().enumerate() {
        let fidx = m.imports.len() + i;
        let tyidx = m.func_types[i];
        let ft = &m.types[tyidx as usize];
        validate_body(m, ft, code, fidx).map_err(|e| format!("func {}: {}", i, e))?;
    }

    Ok(())
}

fn const_expr(m: &Module, instrs: &[Instr], expected: ValType) -> R<()> {
    let mut stack: Vec<ValType> = Vec::new();
    let mut ended = false;
    for ins in instrs {
        if ended {
            return Err("constant expression: instructions after end".to_string());
        }
        match ins {
            Instr::I32Const(_) => stack.push(ValType::I32),
            Instr::I64Const(_) => stack.push(ValType::I64),
            Instr::F32Const(_) => stack.push(ValType::F32),
            Instr::F64Const(_) => stack.push(ValType::F64),
            Instr::Simd(s) => match s.kind {
                SimdKind::Const(_) => stack.push(ValType::V128),
                _ => return Err("constant expression required".to_string()),
            },
            Instr::GlobalGet(_) => {
                return Err(
                    "constant expression required: global.get is not supported (no imported globals)"
                        .to_string(),
                );
            }
            Instr::End => {
                ended = true;
            }
            _ => return Err("constant expression required".to_string()),
        }
        let _ = m;
    }
    if !ended {
        return Err("constant expression: missing end".to_string());
    }
    if stack.len() != 1 || stack[0] != expected {
        return Err("type mismatch in constant expression".to_string());
    }
    Ok(())
}

struct BodyValidator<'a> {
    m: &'a Module,
    locals: Vec<ValType>,
    vals: Vec<Opd>,
    ctrls: Vec<Ctrl>,
    uses_data_ops: bool,
    returns: Vec<ValType>,
}

fn validate_body(m: &Module, ft: &FuncType, code: &FuncBody, _fidx: usize) -> R<()> {
    let mut locals = ft.params.clone();
    locals.extend_from_slice(&code.locals);
    let mut v = BodyValidator {
        m,
        locals,
        vals: Vec::new(),
        ctrls: Vec::new(),
        uses_data_ops: false,
        returns: ft.results.clone(),
    };
    v.push_ctrl(FrameKind::Func, vec![], ft.results.clone());
    let mut hist: Vec<String> = Vec::new();
    for (k, ins) in code.body.iter().enumerate() {
        if let Err(e) = v.validate(ins) {
            let mut ctx = String::new();
            for h in hist.iter() {
                ctx.push_str(&format!("\n   {}", h));
            }
            ctx.push_str(&format!(
                "\n   stack(top): {:?}",
                v.vals.iter().rev().take(10).collect::<Vec<_>>()
            ));
            return Err(format!("instr {} {:?}: {}{}", k, ins, e, ctx));
        }
        if hist.len() >= 8 {
            hist.remove(0);
        }
        hist.push(format!(
            "[{}] {:?} | len={} top={:?}",
            k,
            ins,
            v.vals.len(),
            v.vals.iter().rev().take(6).collect::<Vec<_>>()
        ));
    }
    // After the final end, the control stack must be empty and the operand
    // stack must hold exactly the function results.
    if !v.ctrls.is_empty() {
        return Err("unexpected end of function".to_string());
    }
    if v.vals.len() != v.returns.len() {
        return Err("type mismatch: function results".to_string());
    }
    for (i, t) in v.returns.iter().enumerate() {
        match v.vals[i] {
            Opd::T(a) if a == *t => {}
            _ => return Err("type mismatch: function results".to_string()),
        }
    }
    if v.uses_data_ops && m.data_count.is_none() {
        return Err(
            "data count section required for memory.init/data.drop".to_string(),
        );
    }
    Ok(())
}

impl<'a> BodyValidator<'a> {
    fn cur(&self) -> &Ctrl {
        self.ctrls.last().expect("control stack empty")
    }
    fn cur_mut(&mut self) -> &mut Ctrl {
        self.ctrls.last_mut().expect("control stack empty")
    }

    fn push_val(&mut self, t: Opd) {
        self.vals.push(t);
    }
    fn push_vals(&mut self, ts: &[ValType]) {
        for t in ts {
            self.vals.push(Opd::T(*t));
        }
    }
    fn pop_val_any(&mut self) -> R<Opd> {
        let (height, unreachable) = {
            let c = self.cur();
            (c.height, c.unreachable)
        };
        if self.vals.len() == height && unreachable {
            return Ok(Opd::Unknown);
        }
        if self.vals.len() <= height {
            return Err("type mismatch: expected value but nothing on stack".to_string());
        }
        Ok(self.vals.pop().unwrap())
    }
    fn pop_val(&mut self, expect: ValType) -> R<Opd> {
        let actual = self.pop_val_any()?;
        match actual {
            Opd::Unknown => Ok(Opd::Unknown),
            Opd::T(t) => {
                if t != expect {
                    return Err(format!(
                        "type mismatch: expected {}, found {}",
                        expect.name(),
                        t.name()
                    ));
                }
                Ok(actual)
            }
        }
    }
    fn pop_vals(&mut self, ts: &[ValType]) -> R<Vec<Opd>> {
        let mut out = vec![Opd::Unknown; ts.len()];
        for i in (0..ts.len()).rev() {
            out[i] = self.pop_val(ts[i])?;
        }
        Ok(out)
    }
    fn push_ctrl(&mut self, kind: FrameKind, start: Vec<ValType>, end: Vec<ValType>) {
        let height = self.vals.len();
        self.ctrls.push(Ctrl { kind, start: start.clone(), end, height, unreachable: false });
        self.push_vals(&start);
    }
    fn pop_ctrl(&mut self) -> R<Ctrl> {
        if self.ctrls.is_empty() {
            return Err("unexpected end".to_string());
        }
        let (end, height) = {
            let c = self.cur();
            (c.end.clone(), c.height)
        };
        self.pop_vals(&end)?;
        if self.vals.len() != height {
            return Err("type mismatch: values remaining on stack at end of block".to_string());
        }
        Ok(self.ctrls.pop().unwrap())
    }
    fn mark_unreachable(&mut self) {
        let h = self.cur().height;
        self.vals.truncate(h);
        self.cur_mut().unreachable = true;
    }
    fn label_types(&self, depth: u32) -> R<Vec<ValType>> {
        let n = self.ctrls.len();
        let d = depth as usize;
        if d >= n {
            return Err(format!("unknown label {}", depth));
        }
        let f = &self.ctrls[n - 1 - d];
        Ok(if f.kind == FrameKind::Loop {
            f.start.clone()
        } else {
            f.end.clone()
        })
    }

    fn block_sig(&self, bt: BlockType) -> R<(Vec<ValType>, Vec<ValType>)> {
        self.m
            .block_sig(bt)
            .ok_or_else(|| "unknown block type".to_string())
    }

    fn validate(&mut self, ins: &Instr) -> R<()> {
        match ins {
            Instr::Unreachable => {
                self.mark_unreachable();
            }
            Instr::Nop => {}
            Instr::Block(bt) => {
                let (params, results) = self.block_sig(*bt)?;
                self.pop_vals(&params)?;
                self.push_ctrl(FrameKind::Block, params, results);
            }
            Instr::Loop(bt) => {
                let (params, results) = self.block_sig(*bt)?;
                self.pop_vals(&params)?;
                self.push_ctrl(FrameKind::Loop, params, results);
            }
            Instr::If(bt) => {
                let (params, results) = self.block_sig(*bt)?;
                self.pop_val(ValType::I32)?;
                self.pop_vals(&params)?;
                self.push_ctrl(FrameKind::If, params, results);
            }
            Instr::Else => {
                let frame = self.pop_ctrl()?;
                if frame.kind != FrameKind::If {
                    return Err("else without matching if".to_string());
                }
                self.push_ctrl(FrameKind::Else, frame.start, frame.end);
            }
            Instr::End => {
                let frame = self.pop_ctrl()?;
                if frame.kind == FrameKind::If && frame.start != frame.end {
                    return Err(
                        "type mismatch: if without else must have matching parameter and result types"
                            .to_string(),
                    );
                }
                self.push_vals(&frame.end);
            }
            Instr::Br(d) => {
                let ts = self.label_types(*d)?;
                self.pop_vals(&ts)?;
                self.mark_unreachable();
            }
            Instr::BrIf(d) => {
                let ts = self.label_types(*d)?;
                self.pop_val(ValType::I32)?;
                let popped = self.pop_vals(&ts)?;
                for v in popped {
                    self.push_val(v);
                }
            }
            Instr::BrTable(targets, default) => {
                self.pop_val(ValType::I32)?;
                let dts = self.label_types(*default)?;
                for t in targets {
                    let ts = self.label_types(*t)?;
                    if ts != dts {
                        return Err("type mismatch: br_table target types".to_string());
                    }
                    let popped = self.pop_vals(&ts)?;
                    for v in popped {
                        self.push_val(v);
                    }
                }
                self.pop_vals(&dts)?;
                self.mark_unreachable();
            }
            Instr::Return => {
                let ts = self.returns.clone();
                self.pop_vals(&ts)?;
                self.mark_unreachable();
            }
            Instr::Call(idx) => {
                let ft = self
                    .m
                    .func_type(*idx)
                    .ok_or_else(|| format!("unknown function {}", idx))?
                    .clone();
                self.pop_vals(&ft.params)?;
                self.push_vals(&ft.results);
            }
            Instr::CallIndirect { ty, table } => {
                if (*table as usize) >= self.m.tables.len() {
                    return Err(format!("unknown table {}", table));
                }
                let ft = self
                    .m
                    .types
                    .get(*ty as usize)
                    .ok_or_else(|| format!("unknown type {}", ty))?
                    .clone();
                self.pop_val(ValType::I32)?;
                self.pop_vals(&ft.params)?;
                self.push_vals(&ft.results);
            }
            Instr::Drop => {
                self.pop_val_any()?;
            }
            Instr::Select => {
                self.pop_val(ValType::I32)?;
                let t1 = self.pop_val_any()?;
                let t2 = self.pop_val_any()?;
                match (t1, t2) {
                    (Opd::Unknown, Opd::Unknown) => self.push_val(Opd::Unknown),
                    (Opd::Unknown, Opd::T(t)) => self.push_val(Opd::T(t)),
                    (Opd::T(t), Opd::Unknown) => self.push_val(Opd::T(t)),
                    (Opd::T(a), Opd::T(b)) => {
                        if a != b {
                            return Err("type mismatch: select operands differ".to_string());
                        }
                        self.push_val(Opd::T(a));
                    }
                }
            }
            Instr::LocalGet(i) => {
                let t = *self
                    .locals
                    .get(*i as usize)
                    .ok_or_else(|| format!("unknown local {}", i))?;
                self.push_val(Opd::T(t));
            }
            Instr::LocalSet(i) => {
                let t = *self
                    .locals
                    .get(*i as usize)
                    .ok_or_else(|| format!("unknown local {}", i))?;
                self.pop_val(t)?;
            }
            Instr::LocalTee(i) => {
                let t = *self
                    .locals
                    .get(*i as usize)
                    .ok_or_else(|| format!("unknown local {}", i))?;
                self.pop_val(t)?;
                self.push_val(Opd::T(t));
            }
            Instr::GlobalGet(i) => {
                let g = self
                    .m
                    .globals
                    .get(*i as usize)
                    .ok_or_else(|| format!("unknown global {}", i))?;
                self.push_val(Opd::T(g.ty));
            }
            Instr::GlobalSet(i) => {
                let g = self
                    .m
                    .globals
                    .get(*i as usize)
                    .ok_or_else(|| format!("unknown global {}", i))?
                    .clone();
                if !g.mutable {
                    return Err(format!("global {} is immutable", i));
                }
                self.pop_val(g.ty)?;
            }
            Instr::Load { op, align, offset: _ } => {
                self.require_memory()?;
                let (at, size) = load_sig(*op).ok_or("unknown load opcode")?;
                check_align(*align, size)?;
                self.pop_val(ValType::I32)?;
                self.push_val(Opd::T(at));
            }
            Instr::Store { op, align, offset: _ } => {
                self.require_memory()?;
                let (vt, size) = store_sig(*op).ok_or("unknown store opcode")?;
                check_align(*align, size)?;
                self.pop_val(vt)?;
                self.pop_val(ValType::I32)?;
            }
            Instr::MemorySize => {
                self.require_memory()?;
                self.push_val(Opd::T(ValType::I32));
            }
            Instr::MemoryGrow => {
                self.require_memory()?;
                self.pop_val(ValType::I32)?;
                self.push_val(Opd::T(ValType::I32));
            }
            Instr::I32Const(_) => self.push_val(Opd::T(ValType::I32)),
            Instr::I64Const(_) => self.push_val(Opd::T(ValType::I64)),
            Instr::F32Const(_) => self.push_val(Opd::T(ValType::F32)),
            Instr::F64Const(_) => self.push_val(Opd::T(ValType::F64)),
            Instr::Num(op) => {
                let (params, results) =
                    num_sig(*op).ok_or_else(|| format!("unknown opcode 0x{:x}", op))?;
                self.pop_vals(&params)?;
                self.push_vals(&results);
            }
            Instr::TruncSat(sub) => {
                let (params, results) = trunc_sat_sig(*sub).ok_or("unknown trunc_sat opcode")?;
                self.pop_vals(&params)?;
                self.push_vals(&results);
            }
            Instr::MemoryInit(d) => {
                self.require_memory()?;
                self.uses_data_ops = true;
                self.check_data_idx(*d)?;
                self.pop_vals(&[ValType::I32, ValType::I32, ValType::I32])?;
            }
            Instr::DataDrop(d) => {
                self.uses_data_ops = true;
                self.check_data_idx(*d)?;
            }
            Instr::MemoryCopy => {
                self.require_memory()?;
                self.pop_vals(&[ValType::I32, ValType::I32, ValType::I32])?;
            }
            Instr::MemoryFill => {
                self.require_memory()?;
                self.pop_vals(&[ValType::I32, ValType::I32, ValType::I32])?;
            }
            Instr::TableInit { elem, table } => {
                if (*table as usize) >= self.m.tables.len() {
                    return Err(format!("unknown table {}", table));
                }
                if (*elem as usize) >= self.m.elems.len() {
                    return Err(format!("unknown element segment {}", elem));
                }
                self.pop_vals(&[ValType::I32, ValType::I32, ValType::I32])?;
            }
            Instr::ElemDrop(elem) => {
                if (*elem as usize) >= self.m.elems.len() {
                    return Err(format!("unknown element segment {}", elem));
                }
            }
            Instr::TableCopy { dst, src } => {
                if (*dst as usize) >= self.m.tables.len()
                    || (*src as usize) >= self.m.tables.len()
                {
                    return Err("unknown table".to_string());
                }
                self.pop_vals(&[ValType::I32, ValType::I32, ValType::I32])?;
            }
            Instr::Simd(s) => {
                self.validate_simd(s)?;
            }
        }
        Ok(())
    }

    fn require_memory(&self) -> R<()> {
        if self.m.memories.is_empty() {
            return Err("unknown memory 0".to_string());
        }
        Ok(())
    }

    fn check_data_idx(&self, d: u32) -> R<()> {
        match self.m.data_count {
            Some(dc) if d < dc => Ok(()),
            _ => Err(format!("unknown data segment {}", d)),
        }
    }

    fn validate_simd(&mut self, s: &SimdOp) -> R<()> {
        match &s.kind {
            SimdKind::Const(_) => {
                self.push_val(Opd::T(ValType::V128));
            }
            SimdKind::Shuffle(lanes) => {
                for l in lanes {
                    if *l >= 32 {
                        return Err("invalid shuffle lane index".to_string());
                    }
                }
                self.pop_val(ValType::V128)?;
                self.pop_val(ValType::V128)?;
                self.push_val(Opd::T(ValType::V128));
            }
            SimdKind::Lane { lane } => {
                let (params, results) =
                    simd_sig(s.sub).ok_or_else(|| format!("unknown SIMD opcode {}", s.sub))?;
                let lanes = lane_count(s.sub).ok_or("not a lane opcode")?;
                if *lane as u32 >= lanes {
                    return Err("invalid lane index".to_string());
                }
                self.pop_vals(&params)?;
                self.push_vals(&results);
            }
            SimdKind::Mem { align, offset: _ } => {
                self.require_memory()?;
                let (params, results, size) =
                    simd_mem_sig(s.sub).ok_or_else(|| format!("unknown SIMD opcode {}", s.sub))?;
                check_align(*align, size)?;
                self.pop_vals(&params)?;
                self.push_vals(&results);
            }
            SimdKind::MemLane { align, offset: _, lane } => {
                self.require_memory()?;
                let (params, results, size) = simd_mem_sig(s.sub)
                    .ok_or_else(|| format!("unknown SIMD opcode {}", s.sub))?;
                check_align(*align, size)?;
                let lanes = lane_count(s.sub).ok_or("not a lane opcode")?;
                if *lane as u32 >= lanes {
                    return Err("invalid lane index".to_string());
                }
                self.pop_vals(&params)?;
                self.push_vals(&results);
            }
            SimdKind::Plain => {
                let (params, results) =
                    simd_sig(s.sub).ok_or_else(|| format!("unknown SIMD opcode {}", s.sub))?;
                self.pop_vals(&params)?;
                self.push_vals(&results);
            }
        }
        Ok(())
    }
}

fn check_align(align: u32, size: u64) -> R<()> {
    if align >= 64 {
        return Err("malformed memop alignment".to_string());
    }
    let a = 1u64 << align;
    if a > size {
        return Err("alignment must not be larger than natural".to_string());
    }
    Ok(())
}

pub fn load_sig(op: u8) -> Option<(ValType, u64)> {
    Some(match op {
        0x28 => (ValType::I32, 4),
        0x29 => (ValType::I64, 8),
        0x2a => (ValType::F32, 4),
        0x2b => (ValType::F64, 8),
        0x2c => (ValType::I32, 1),
        0x2d => (ValType::I32, 1),
        0x2e => (ValType::I32, 2),
        0x2f => (ValType::I32, 2),
        0x30 => (ValType::I64, 1),
        0x31 => (ValType::I64, 1),
        0x32 => (ValType::I64, 2),
        0x33 => (ValType::I64, 2),
        0x34 => (ValType::I64, 4),
        0x35 => (ValType::I64, 4),
        _ => return None,
    })
}

pub fn store_sig(op: u8) -> Option<(ValType, u64)> {
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

fn num_sig(op: u16) -> Option<(Vec<ValType>, Vec<ValType>)> {
    use ValType::*;
    let (p, r): (&[ValType], &[ValType]) = match op {
        0x45 => (&[I32], &[I32]),
        0x46..=0x4f => (&[I32, I32], &[I32]),
        0x50 => (&[I64], &[I32]),
        0x51..=0x5a => (&[I64, I64], &[I32]),
        0x5b..=0x60 => (&[F32, F32], &[I32]),
        0x61..=0x66 => (&[F64, F64], &[I32]),
        0x67..=0x69 => (&[I32], &[I32]),
        0x6a..=0x78 => (&[I32, I32], &[I32]),
        0x79..=0x7b => (&[I64], &[I64]),
        0x7c..=0x8a => (&[I64, I64], &[I64]),
        0x8b..=0x91 => (&[F32], &[F32]),
        0x92..=0x98 => (&[F32, F32], &[F32]),
        0x99..=0x9f => (&[F64], &[F64]),
        0xa0..=0xa6 => (&[F64, F64], &[F64]),
        0xa7 => (&[I64], &[I32]),
        0xa8..=0xa9 => (&[F32], &[I32]),
        0xaa..=0xab => (&[F64], &[I32]),
        0xac..=0xad => (&[I32], &[I64]),
        0xae..=0xaf => (&[F32], &[I64]),
        0xb0..=0xb1 => (&[F64], &[I64]),
        0xb2..=0xb3 => (&[I32], &[F32]),
        0xb4..=0xb5 => (&[I64], &[F32]),
        0xb6 => (&[F64], &[F32]),
        0xb7..=0xb8 => (&[I32], &[F64]),
        0xb9..=0xba => (&[I64], &[F64]),
        0xbb => (&[F32], &[F64]),
        0xbc => (&[F32], &[I32]),
        0xbd => (&[F64], &[I64]),
        0xbe => (&[I32], &[F32]),
        0xbf => (&[I64], &[F64]),
        0xc0..=0xc1 => (&[I32], &[I32]),
        0xc2..=0xc4 => (&[I64], &[I64]),
        _ => return None,
    };
    Some((p.to_vec(), r.to_vec()))
}

fn trunc_sat_sig(sub: u32) -> Option<(Vec<ValType>, Vec<ValType>)> {
    use ValType::*;
    Some(match sub {
        0 => (vec![F32], vec![I32]),
        1 => (vec![F32], vec![I32]),
        2 => (vec![F64], vec![I32]),
        3 => (vec![F64], vec![I32]),
        4 => (vec![F32], vec![I64]),
        5 => (vec![F32], vec![I64]),
        6 => (vec![F64], vec![I64]),
        7 => (vec![F64], vec![I64]),
        _ => return None,
    })
}

pub fn lane_count(sub: u32) -> Option<u32> {
    Some(match sub {
        21..=23 => 16,
        24..=26 => 8,
        27..=28 => 4,
        29..=30 => 2,
        31..=32 => 4,
        33..=34 => 2,
        84 | 88 => 16,
        85 | 89 => 8,
        86 | 90 => 4,
        87 | 91 => 2,
        _ => return None,
    })
}

/// Returns (params, results, access size in bytes).
pub fn simd_mem_sig(sub: u32) -> Option<(Vec<ValType>, Vec<ValType>, u64)> {
    use ValType::*;
    Some(match sub {
        0 => (vec![I32], vec![V128], 16),
        1 => (vec![I32], vec![V128], 8),
        2 => (vec![I32], vec![V128], 8),
        3 => (vec![I32], vec![V128], 8),
        4 => (vec![I32], vec![V128], 8),
        5 => (vec![I32], vec![V128], 8),
        6 => (vec![I32], vec![V128], 8),
        7 => (vec![I32], vec![V128], 1),
        8 => (vec![I32], vec![V128], 2),
        9 => (vec![I32], vec![V128], 4),
        10 => (vec![I32], vec![V128], 8),
        11 => (vec![I32, V128], vec![], 16),
        84 => (vec![I32, V128], vec![V128], 1),
        85 => (vec![I32, V128], vec![V128], 2),
        86 => (vec![I32, V128], vec![V128], 4),
        87 => (vec![I32, V128], vec![V128], 8),
        88 => (vec![I32, V128], vec![], 1),
        89 => (vec![I32, V128], vec![], 2),
        90 => (vec![I32, V128], vec![], 4),
        91 => (vec![I32, V128], vec![], 8),
        92 => (vec![I32], vec![V128], 4),
        93 => (vec![I32], vec![V128], 8),
        _ => return None,
    })
}

pub fn simd_sig(sub: u32) -> Option<(Vec<ValType>, Vec<ValType>)> {
    use ValType::*;
    let (p, r): (&[ValType], &[ValType]) = match sub {
        14 => (&[V128, V128], &[V128]),
        15 => (&[I32], &[V128]),
        16 => (&[I32], &[V128]),
        17 => (&[I32], &[V128]),
        18 => (&[I64], &[V128]),
        19 => (&[F32], &[V128]),
        20 => (&[F64], &[V128]),
        21 | 22 => (&[V128], &[I32]),
        23 => (&[V128, I32], &[V128]),
        24 | 25 => (&[V128], &[I32]),
        26 => (&[V128, I32], &[V128]),
        27 => (&[V128], &[I32]),
        28 => (&[V128, I32], &[V128]),
        29 => (&[V128], &[I64]),
        30 => (&[V128, I64], &[V128]),
        31 => (&[V128], &[F32]),
        32 => (&[V128, F32], &[V128]),
        33 => (&[V128], &[F64]),
        34 => (&[V128, F64], &[V128]),
        35..=76 => (&[V128, V128], &[V128]),
        77 => (&[V128], &[V128]),
        78..=81 => (&[V128, V128], &[V128]),
        82 => (&[V128, V128, V128], &[V128]),
        83 => (&[V128], &[I32]),
        94 | 95 => (&[V128], &[V128]),
        96..=98 => (&[V128], &[V128]),
        99 | 100 => (&[V128], &[I32]),
        101 | 102 => (&[V128, V128], &[V128]),
        103..=106 => (&[V128], &[V128]),
        107..=109 => (&[V128, I32], &[V128]),
        110..=115 => (&[V128, V128], &[V128]),
        116 | 117 => (&[V128], &[V128]),
        118..=121 => (&[V128, V128], &[V128]),
        122 => (&[V128], &[V128]),
        123 => (&[V128, V128], &[V128]),
        124..=127 => (&[V128], &[V128]),
        128 | 129 => (&[V128], &[V128]),
        130 => (&[V128, V128], &[V128]),
        131 | 132 => (&[V128], &[I32]),
        133 | 134 => (&[V128, V128], &[V128]),
        135..=138 => (&[V128], &[V128]),
        139..=141 => (&[V128, I32], &[V128]),
        142..=147 => (&[V128, V128], &[V128]),
        148 => (&[V128], &[V128]),
        149 => (&[V128, V128], &[V128]),
        150..=153 => (&[V128, V128], &[V128]),
        155 => (&[V128, V128], &[V128]),
        156..=159 => (&[V128, V128], &[V128]),
        160 | 161 => (&[V128], &[V128]),
        163 | 164 => (&[V128], &[I32]),
        167..=170 => (&[V128], &[V128]),
        171..=173 => (&[V128, I32], &[V128]),
        174 => (&[V128, V128], &[V128]),
        177 => (&[V128, V128], &[V128]),
        181..=185 => (&[V128, V128], &[V128]),
        186 => (&[V128, V128], &[V128]),
        188..=191 => (&[V128, V128], &[V128]),
        192 | 193 => (&[V128], &[V128]),
        195 | 196 => (&[V128], &[I32]),
        199..=202 => (&[V128], &[V128]),
        203..=205 => (&[V128, I32], &[V128]),
        206 => (&[V128, V128], &[V128]),
        209 => (&[V128, V128], &[V128]),
        213 => (&[V128, V128], &[V128]),
        214..=219 => (&[V128, V128], &[V128]),
        220..=223 => (&[V128, V128], &[V128]),
        224 | 225 => (&[V128], &[V128]),
        227 => (&[V128], &[V128]),
        228..=235 => (&[V128, V128], &[V128]),
        236 | 237 => (&[V128], &[V128]),
        239 => (&[V128], &[V128]),
        240..=247 => (&[V128, V128], &[V128]),
        248..=255 => (&[V128], &[V128]),
        _ => return None,
    };
    Some((p.to_vec(), r.to_vec()))
}

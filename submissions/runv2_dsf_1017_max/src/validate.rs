// Validation of modules and function bodies, following the WebAssembly 2.0
// validation algorithm.

use std::collections::HashSet;

use crate::instr::{BlockType, Instr, SimdLaneOp, SimdOp};
use crate::module::{DataMode, ElemMode, ExportKind, Module, ValType};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Opd {
    Known(ValType),
    Unknown,
}

impl Opd {
    fn is_num(self) -> bool {
        match self {
            Opd::Unknown => true,
            Opd::Known(t) => t.is_num(),
        }
    }
    fn is_vec(self) -> bool {
        match self {
            Opd::Unknown => true,
            Opd::Known(t) => t.is_vec(),
        }
    }
    fn is_ref(self) -> bool {
        match self {
            Opd::Unknown => true,
            Opd::Known(t) => t.is_ref(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FrameKind {
    Function,
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

struct Validator<'a> {
    m: &'a Module,
    locals: Vec<ValType>,
    return_types: Vec<ValType>,
    refs: &'a HashSet<u32>,
    vals: Vec<Opd>,
    ctrls: Vec<Ctrl>,
}

fn err<T>(msg: impl Into<String>) -> Result<T, String> {
    Err(msg.into())
}

impl<'a> Validator<'a> {
    fn push(&mut self, t: ValType) {
        self.vals.push(Opd::Known(t));
    }
    fn push_opd(&mut self, t: Opd) {
        self.vals.push(t);
    }
    fn pop_any(&mut self) -> Result<Opd, String> {
        let (h, ur) = {
            let f = self
                .ctrls
                .last()
                .ok_or_else(|| "control stack underflow".to_string())?;
            (f.height, f.unreachable)
        };
        if self.vals.len() == h {
            if ur {
                return Ok(Opd::Unknown);
            }
            return err("operand stack underflow");
        }
        Ok(self.vals.pop().unwrap())
    }
    fn pop(&mut self, expect: ValType) -> Result<Opd, String> {
        let a = self.pop_any()?;
        match a {
            Opd::Unknown => Ok(a),
            Opd::Known(t) => {
                if t != expect {
                    err(format!(
                        "type mismatch: expected {}, found {}",
                        expect.name(),
                        t.name()
                    ))
                } else {
                    Ok(a)
                }
            }
        }
    }
    fn pop_vals(&mut self, types: &[ValType]) -> Result<Vec<Opd>, String> {
        let mut popped = Vec::with_capacity(types.len());
        for t in types.iter().rev() {
            popped.push(self.pop(*t)?);
        }
        popped.reverse();
        Ok(popped)
    }
    fn push_vals(&mut self, types: &[ValType]) {
        for t in types {
            self.push(*t);
        }
    }
    fn push_opds(&mut self, vals: &[Opd]) {
        for v in vals {
            self.push_opd(*v);
        }
    }
    fn push_ctrl(&mut self, kind: FrameKind, start: Vec<ValType>, end: Vec<ValType>) -> Result<(), String> {
        let height = self.vals.len();
        self.ctrls.push(Ctrl {
            kind,
            start: start.clone(),
            end,
            height,
            unreachable: false,
        });
        self.push_vals(&start);
        Ok(())
    }
    fn pop_ctrl(&mut self) -> Result<Ctrl, String> {
        let (end, height) = {
            let f = self
                .ctrls
                .last()
                .ok_or_else(|| "control stack underflow".to_string())?;
            (f.end.clone(), f.height)
        };
        self.pop_vals(&end)?;
        if self.vals.len() != height {
            return err("type mismatch: values remaining on stack at end of block");
        }
        Ok(self.ctrls.pop().unwrap())
    }
    fn set_unreachable(&mut self) {
        if let Some(f) = self.ctrls.last_mut() {
            self.vals.truncate(f.height);
            f.unreachable = true;
        }
    }
    fn label_types(&self, depth_from_top: usize) -> Result<Vec<ValType>, String> {
        let n = self.ctrls.len();
        if depth_from_top >= n {
            return err("unknown label");
        }
        let f = &self.ctrls[n - 1 - depth_from_top];
        Ok(if f.kind == FrameKind::Loop {
            f.start.clone()
        } else {
            f.end.clone()
        })
    }
    fn blocktype(&self, bt: &BlockType) -> Result<(Vec<ValType>, Vec<ValType>), String> {
        match bt {
            BlockType::Empty => Ok((vec![], vec![])),
            BlockType::Val(t) => Ok((vec![], vec![*t])),
            BlockType::TypeIdx(i) => {
                let ft = self
                    .m
                    .types
                    .get(*i as usize)
                    .ok_or_else(|| "unknown type index in block type".to_string())?;
                Ok((ft.params.clone(), ft.results.clone()))
            }
        }
    }
    fn func_type(&self, func_idx: u32) -> Result<&'a crate::module::FuncType, String> {
        let ti = *self
            .m
            .func_types
            .get(func_idx as usize)
            .ok_or_else(|| format!("unknown function {}", func_idx))?;
        self.m
            .types
            .get(ti as usize)
            .ok_or_else(|| format!("unknown type {}", ti))
    }
    fn table(&self, idx: u32) -> Result<&'a crate::module::TableType, String> {
        self.m
            .table_types
            .get(idx as usize)
            .ok_or_else(|| format!("unknown table {}", idx))
    }
    fn mem_exists(&self) -> Result<(), String> {
        if self.m.mem_types.is_empty() {
            return err("unknown memory 0");
        }
        Ok(())
    }

    fn check_align(align: u32, natural: u32) -> Result<(), String> {
        if align >= 32 || (1u64 << align) > natural as u64 {
            return err(format!(
                "alignment 2^{} exceeds natural alignment {}",
                align, natural
            ));
        }
        Ok(())
    }

    fn simd_sig(&self, op: SimdOp) -> (&'static [ValType], &'static [ValType]) {
        use ValType::*;
        const V1: &[ValType] = &[V128];
        const V2: &[ValType] = &[V128, V128];
        const V3: &[ValType] = &[V128, V128, V128];
        const VI: &[ValType] = &[V128, I32];
        const I1: &[ValType] = &[I32];
        const I64_1: &[ValType] = &[I64];
        const F32_1: &[ValType] = &[F32];
        const F64_1: &[ValType] = &[F64];
        use SimdOp::*;
        match op {
            Not | I8x16Abs | I8x16Neg | I8x16Popcnt | I16x8Abs | I16x8Neg | I32x4Abs
            | I32x4Neg | I64x2Abs | I64x2Neg => (V1, V1),
            And | AndNot | Or | Xor | I8x16Swizzle => (V2, V1),
            Bitselect => (V3, V1),
            AnyTrue | I8x16AllTrue | I16x8AllTrue | I32x4AllTrue | I64x2AllTrue
            | I8x16Bitmask | I16x8Bitmask | I32x4Bitmask | I64x2Bitmask => (V1, I1),
            I8x16Splat | I16x8Splat | I32x4Splat => (I1, V1),
            I64x2Splat => (I64_1, V1),
            F32x4Splat => (F32_1, V1),
            F64x2Splat => (F64_1, V1),
            I8x16Shl | I8x16ShrS | I8x16ShrU | I16x8Shl | I16x8ShrS | I16x8ShrU | I32x4Shl
            | I32x4ShrS | I32x4ShrU | I64x2Shl | I64x2ShrS | I64x2ShrU => (VI, V1),
            // Unary v128 -> v128 operations.
            F32x4Ceil | F32x4Floor | F32x4Trunc | F32x4Nearest | F32x4Abs | F32x4Neg
            | F32x4Sqrt | F64x2Ceil | F64x2Floor | F64x2Trunc | F64x2Nearest | F64x2Abs
            | F64x2Neg | F64x2Sqrt | I16x8ExtaddPairwiseI8x16S
            | I16x8ExtaddPairwiseI8x16U | I32x4ExtaddPairwiseI16x8S
            | I32x4ExtaddPairwiseI16x8U | I16x8ExtendLowI8x16S | I16x8ExtendHighI8x16S
            | I16x8ExtendLowI8x16U | I16x8ExtendHighI8x16U | I32x4ExtendLowI16x8S
            | I32x4ExtendHighI16x8S | I32x4ExtendLowI16x8U | I32x4ExtendHighI16x8U
            | I64x2ExtendLowI32x4S | I64x2ExtendHighI32x4S | I64x2ExtendLowI32x4U
            | I64x2ExtendHighI32x4U | I32x4TruncSatF32x4S | I32x4TruncSatF32x4U
            | F32x4ConvertI32x4S | F32x4ConvertI32x4U | I32x4TruncSatF64x2SZero
            | I32x4TruncSatF64x2UZero | F64x2ConvertLowI32x4S | F64x2ConvertLowI32x4U
            | F32x4DemoteF64x2Zero | F64x2PromoteLowF32x4 => (V1, V1),
            // Everything else in the table is a v128,v128 -> v128 operation.
            _ => (V2, V1),
        }
    }

    fn step(&mut self, instr: &Instr) -> Result<(), String> {
        use ValType::*;
        match instr {
            Instr::Unreachable => self.set_unreachable(),
            Instr::Nop => {}
            Instr::Block(bt) => {
                let (ins, outs) = self.blocktype(bt)?;
                self.pop_vals(&ins)?;
                self.push_ctrl(FrameKind::Block, ins, outs)?;
            }
            Instr::Loop(bt) => {
                let (ins, outs) = self.blocktype(bt)?;
                self.pop_vals(&ins)?;
                self.push_ctrl(FrameKind::Loop, ins, outs)?;
            }
            Instr::If(bt) => {
                let (ins, outs) = self.blocktype(bt)?;
                self.pop(I32)?;
                self.pop_vals(&ins)?;
                self.push_ctrl(FrameKind::If, ins, outs)?;
            }
            Instr::Else => {
                let f = self.pop_ctrl()?;
                if f.kind != FrameKind::If {
                    return err("else without matching if");
                }
                self.push_ctrl(FrameKind::Else, f.start, f.end)?;
            }
            Instr::End => {
                let f = self.pop_ctrl()?;
                if f.kind == FrameKind::If && f.start != f.end {
                    // An if without else has an implicit empty else branch,
                    // which requires its parameter and result types to match.
                    return err("if without else requires matching parameter and result types");
                }
                self.push_vals(&f.end);
                if f.kind == FrameKind::Function && !self.ctrls.is_empty() {
                    return err("unexpected end of function");
                }
            }
            Instr::Br(l) => {
                let types = self.label_types(*l as usize)?;
                self.pop_vals(&types)?;
                self.set_unreachable();
            }
            Instr::BrIf(l) => {
                let types = self.label_types(*l as usize)?;
                self.pop(I32)?;
                self.pop_vals(&types)?;
                self.push_vals(&types);
            }
            Instr::BrTable(targets, default) => {
                self.pop(I32)?;
                let dtypes = self.label_types(*default as usize)?;
                let arity = dtypes.len();
                for t in targets {
                    let types = self.label_types(*t as usize)?;
                    if types.len() != arity {
                        return err("br_table targets have inconsistent arity");
                    }
                    let popped = self.pop_vals(&types)?;
                    self.push_opds(&popped);
                }
                self.pop_vals(&dtypes)?;
                self.set_unreachable();
            }
            Instr::Return => {
                let rt = self.return_types.clone();
                self.pop_vals(&rt)?;
                self.set_unreachable();
            }
            Instr::Call(x) => {
                let ft = self.func_type(*x)?;
                let (params, results) = (ft.params.clone(), ft.results.clone());
                self.pop_vals(&params)?;
                self.push_vals(&results);
            }
            Instr::CallIndirect(ty, table) => {
                let ft = self
                    .m
                    .types
                    .get(*ty as usize)
                    .ok_or_else(|| format!("unknown type {}", ty))?;
                let tt = self.table(*table)?;
                if tt.elem != FuncRef {
                    return err("call_indirect requires a funcref table");
                }
                let (params, results) = (ft.params.clone(), ft.results.clone());
                self.pop(I32)?;
                self.pop_vals(&params)?;
                self.push_vals(&results);
            }
            Instr::Drop => {
                self.pop_any()?;
            }
            Instr::Select => {
                self.pop(I32)?;
                let t1 = self.pop_any()?;
                let t2 = self.pop_any()?;
                let ok = (t1.is_num() && t2.is_num()) || (t1.is_vec() && t2.is_vec());
                if !ok {
                    return err("select operands must both be numeric or both be vectors");
                }
                match (t1, t2) {
                    (Opd::Known(a), Opd::Known(b)) if a != b => {
                        return err("select operands have different types")
                    }
                    _ => {}
                }
                let t = match t1 {
                    Opd::Unknown => t2,
                    _ => t1,
                };
                self.push_opd(t);
            }
            Instr::SelectT(t) => {
                self.pop(I32)?;
                self.pop(*t)?;
                self.pop(*t)?;
                self.push(*t);
            }
            Instr::LocalGet(x) => {
                let t = *self
                    .locals
                    .get(*x as usize)
                    .ok_or_else(|| format!("unknown local {}", x))?;
                self.push(t);
            }
            Instr::LocalSet(x) => {
                let t = *self
                    .locals
                    .get(*x as usize)
                    .ok_or_else(|| format!("unknown local {}", x))?;
                self.pop(t)?;
            }
            Instr::LocalTee(x) => {
                let t = *self
                    .locals
                    .get(*x as usize)
                    .ok_or_else(|| format!("unknown local {}", x))?;
                self.pop(t)?;
                self.push(t);
            }
            Instr::GlobalGet(x) => {
                let gt = *self
                    .m
                    .global_types
                    .get(*x as usize)
                    .ok_or_else(|| format!("unknown global {}", x))?;
                self.push(gt.val);
            }
            Instr::GlobalSet(x) => {
                let gt = *self
                    .m
                    .global_types
                    .get(*x as usize)
                    .ok_or_else(|| format!("unknown global {}", x))?;
                if !gt.mutable {
                    return err("global.set on immutable global");
                }
                self.pop(gt.val)?;
            }
            Instr::TableGet(t) => {
                let tt = self.table(*t)?;
                self.pop(I32)?;
                self.push(tt.elem);
            }
            Instr::TableSet(t) => {
                let tt = self.table(*t)?;
                self.pop(tt.elem)?;
                self.pop(I32)?;
            }
            Instr::TableInit(e, t) => {
                let et = *self
                    .m
                    .elems
                    .get(*e as usize)
                    .map(|x| &x.ty)
                    .ok_or_else(|| format!("unknown element segment {}", e))?;
                let tt = self.table(*t)?;
                if tt.elem != et {
                    return err("table.init element type mismatch");
                }
                self.pop(I32)?;
                self.pop(I32)?;
                self.pop(I32)?;
            }
            Instr::ElemDrop(e) => {
                if *e as usize >= self.m.elems.len() {
                    return err(format!("unknown element segment {}", e));
                }
            }
            Instr::TableCopy(d, s) => {
                let dt = self.table(*d)?.elem;
                let st = self.table(*s)?.elem;
                if dt != st {
                    return err("table.copy element type mismatch");
                }
                self.pop(I32)?;
                self.pop(I32)?;
                self.pop(I32)?;
            }
            Instr::TableGrow(t) => {
                let tt = self.table(*t)?;
                self.pop(I32)?;
                self.pop(tt.elem)?;
                self.push(I32);
            }
            Instr::TableSize(t) => {
                self.table(*t)?;
                self.push(I32);
            }
            Instr::TableFill(t) => {
                let tt = self.table(*t)?;
                self.pop(I32)?;
                self.pop(tt.elem)?;
                self.pop(I32)?;
            }
            Instr::MemorySize => {
                self.mem_exists()?;
                self.push(I32);
            }
            Instr::MemoryGrow => {
                self.mem_exists()?;
                self.pop(I32)?;
                self.push(I32);
            }
            Instr::MemoryInit(d) => {
                self.mem_exists()?;
                if *d as usize >= self.m.datas.len() {
                    return err(format!("unknown data segment {}", d));
                }
                self.pop(I32)?;
                self.pop(I32)?;
                self.pop(I32)?;
            }
            Instr::DataDrop(d) => {
                if *d as usize >= self.m.datas.len() {
                    return err(format!("unknown data segment {}", d));
                }
            }
            Instr::MemoryCopy | Instr::MemoryFill => {
                self.mem_exists()?;
                self.pop(I32)?;
                self.pop(I32)?;
                self.pop(I32)?;
            }
            Instr::Load(k, ma) => {
                self.mem_exists()?;
                Self::check_align(ma.align, k.bytes())?;
                self.pop(I32)?;
                self.push(k.result());
            }
            Instr::Store(k, ma) => {
                self.mem_exists()?;
                Self::check_align(ma.align, k.bytes())?;
                self.pop(k.value_type())?;
                self.pop(I32)?;
            }
            Instr::I32Const(_) => self.push(I32),
            Instr::I64Const(_) => self.push(I64),
            Instr::F32Const(_) => self.push(F32),
            Instr::F64Const(_) => self.push(F64),
            Instr::RefNull(t) => self.push(*t),
            Instr::RefIsNull => {
                let t = self.pop_any()?;
                if !t.is_ref() {
                    return err("ref.is_null requires a reference operand");
                }
                self.push(I32);
            }
            Instr::RefFunc(x) => {
                if *x as usize >= self.m.func_types.len() {
                    return err(format!("unknown function {}", x));
                }
                if !self.refs.contains(x) {
                    return err(format!("undeclared function reference {}", x));
                }
                self.push(FuncRef);
            }
            Instr::Un(op) => {
                let t = op.ty();
                self.pop(t)?;
                self.push(t);
            }
            Instr::Bin(op) => {
                let t = op.ty();
                self.pop(t)?;
                self.pop(t)?;
                self.push(t);
            }
            Instr::Test(op) => {
                self.pop(op.ty())?;
                self.push(I32);
            }
            Instr::Rel(op) => {
                let t = op.ty();
                self.pop(t)?;
                self.pop(t)?;
                self.push(I32);
            }
            Instr::Cvt(op) => {
                let (from, to) = op.sig();
                self.pop(from)?;
                self.push(to);
            }
            Instr::V128Const(_) => self.push(V128),
            Instr::V128Load(ma) => {
                self.mem_exists()?;
                Self::check_align(ma.align, 16)?;
                self.pop(I32)?;
                self.push(V128);
            }
            Instr::V128Store(ma) => {
                self.mem_exists()?;
                Self::check_align(ma.align, 16)?;
                self.pop(V128)?;
                self.pop(I32)?;
            }
            Instr::V128LoadLane(op, ma, _lane) => {
                self.mem_exists()?;
                Self::check_align(ma.align, op.bytes())?;
                if op.is_store() {
                    self.pop(V128)?;
                    self.pop(I32)?;
                } else {
                    self.pop(V128)?;
                    self.pop(I32)?;
                    self.push(V128);
                }
            }
            Instr::V128LoadExt(op, ma) => {
                self.mem_exists()?;
                Self::check_align(ma.align, op.bytes())?;
                self.pop(I32)?;
                self.push(V128);
            }
            Instr::I8x16Shuffle(_) => {
                self.pop(V128)?;
                self.pop(V128)?;
                self.push(V128);
            }
            Instr::SimdLane(op, _lane) => match op {
                SimdLaneOp::I8x16ExtractS
                | SimdLaneOp::I8x16ExtractU
                | SimdLaneOp::I16x8ExtractS
                | SimdLaneOp::I16x8ExtractU
                | SimdLaneOp::I32x4Extract => {
                    self.pop(V128)?;
                    self.push(I32);
                }
                SimdLaneOp::I64x2Extract => {
                    self.pop(V128)?;
                    self.push(I64);
                }
                SimdLaneOp::F32x4Extract => {
                    self.pop(V128)?;
                    self.push(F32);
                }
                SimdLaneOp::F64x2Extract => {
                    self.pop(V128)?;
                    self.push(F64);
                }
                SimdLaneOp::I8x16Replace
                | SimdLaneOp::I16x8Replace
                | SimdLaneOp::I32x4Replace => {
                    self.pop(I32)?;
                    self.pop(V128)?;
                    self.push(V128);
                }
                SimdLaneOp::I64x2Replace => {
                    self.pop(I64)?;
                    self.pop(V128)?;
                    self.push(V128);
                }
                SimdLaneOp::F32x4Replace => {
                    self.pop(F32)?;
                    self.pop(V128)?;
                    self.push(V128);
                }
                SimdLaneOp::F64x2Replace => {
                    self.pop(F64)?;
                    self.pop(V128)?;
                    self.push(V128);
                }
            },
            Instr::Simd(op) => {
                let (ins, outs) = self.simd_sig(*op);
                self.pop_vals(ins)?;
                self.push_vals(outs);
            }
        }
        Ok(())
    }
}

/// Validate a constant expression producing `expected`.
pub fn validate_const_expr(
    m: &Module,
    expr: &[Instr],
    expected: ValType,
    refs: &HashSet<u32>,
    what: &str,
) -> Result<(), String> {
    if expr.is_empty() {
        return Err(format!("empty constant expression for {}", what));
    }
    let mut stack: Vec<ValType> = Vec::new();
    let mut saw_end = false;
    for (i, ins) in expr.iter().enumerate() {
        match ins {
            Instr::I32Const(_) => stack.push(ValType::I32),
            Instr::I64Const(_) => stack.push(ValType::I64),
            Instr::F32Const(_) => stack.push(ValType::F32),
            Instr::F64Const(_) => stack.push(ValType::F64),
            Instr::V128Const(_) => stack.push(ValType::V128),
            Instr::RefNull(t) => stack.push(*t),
            Instr::RefFunc(x) => {
                if *x as usize >= m.func_types.len() {
                    return Err(format!("unknown function {} in constant expression", x));
                }
                if !refs.contains(x) {
                    return Err(format!(
                        "undeclared function reference {} in constant expression",
                        x
                    ));
                }
                stack.push(ValType::FuncRef);
            }
            Instr::GlobalGet(x) => {
                if *x as usize >= m.num_imported_globals {
                    return Err(format!(
                        "constant expression may only reference imported globals (global {})",
                        x
                    ));
                }
                let gt = m.global_types[*x as usize];
                if gt.mutable {
                    return Err("constant expression references a mutable global".to_string());
                }
                stack.push(gt.val);
            }
            Instr::End => {
                if i + 1 != expr.len() {
                    return Err(format!("trailing instructions after end in {}", what));
                }
                saw_end = true;
            }
            _ => {
                return Err(format!("non-constant instruction in {}", what));
            }
        }
    }
    if !saw_end {
        return Err(format!("constant expression missing end in {}", what));
    }
    if stack.len() != 1 || stack[0] != expected {
        return Err(format!(
            "constant expression in {} has wrong type (expected {})",
            what,
            expected.name()
        ));
    }
    Ok(())
}

pub fn validate_module(m: &Module) -> Result<(), String> {
    use ValType::*;

    if m.mem_types.len() > 1 {
        return Err("multiple memories are not supported (multi-memory is out of scope)".to_string());
    }

    // Type indices used by imports/functions.
    for (i, f) in m.imported_funcs.iter().enumerate() {
        if f.2 as usize >= m.types.len() {
            return Err(format!("import {} uses unknown type {}", i, f.2));
        }
    }
    for (i, f) in m.funcs.iter().enumerate() {
        if f.type_idx as usize >= m.types.len() {
            return Err(format!("function {} uses unknown type {}", i, f.type_idx));
        }
    }

    // Compute the set of declared function references. It consists of function
    // indices occurring anywhere except in function bodies and the start
    // function: exports, global initializers and element segments.
    let mut refs: HashSet<u32> = HashSet::new();
    for e in &m.exports {
        if e.kind == ExportKind::Func && (e.idx as usize) < m.func_types.len() {
            refs.insert(e.idx);
        }
    }
    for g in &m.globals {
        for ins in &g.init {
            if let Instr::RefFunc(x) = ins {
                refs.insert(*x);
            }
        }
    }
    for el in &m.elems {
        for e in &el.init {
            for ins in e {
                if let Instr::RefFunc(x) = ins {
                    refs.insert(*x);
                }
            }
        }
    }

    // Global initializers.
    for (i, g) in m.globals.iter().enumerate() {
        validate_const_expr(m, &g.init, g.ty.val, &refs, &format!("global {}", i))?;
    }

    // Element segments.
    for (i, el) in m.elems.iter().enumerate() {
        if let ElemMode::Active { table, offset } = &el.mode {
            let tt = m
                .table_types
                .get(*table as usize)
                .ok_or_else(|| format!("element segment {} references unknown table {}", i, table))?;
            if tt.elem != el.ty {
                return Err(format!(
                    "element segment {} type does not match table {}",
                    i, table
                ));
            }
            validate_const_expr(m, offset, I32, &refs, &format!("element segment {}", i))?;
        }
        for e in &el.init {
            validate_const_expr(m, e, el.ty, &refs, &format!("element segment {}", i))?;
        }
    }

    // Data segments.
    for (i, d) in m.datas.iter().enumerate() {
        if let DataMode::Active { mem, offset } = &d.mode {
            if *mem != 0 {
                return Err(format!("data segment {} references memory {}", i, mem));
            }
            if m.mem_types.is_empty() {
                return Err(format!("data segment {} requires a memory", i));
            }
            validate_const_expr(m, offset, I32, &refs, &format!("data segment {}", i))?;
        }
    }

    // Exports.
    let mut names: HashSet<&[u8]> = HashSet::new();
    for e in &m.exports {
        if !names.insert(e.name.as_slice()) {
            return Err("duplicate export name".to_string());
        }
        let (limit, what) = match e.kind {
            ExportKind::Func => (m.func_types.len(), "function"),
            ExportKind::Table => (m.table_types.len(), "table"),
            ExportKind::Mem => (m.mem_types.len(), "memory"),
            ExportKind::Global => (m.global_types.len(), "global"),
        };
        if e.idx as usize >= limit {
            return Err(format!("export references unknown {}", what));
        }
    }

    // Start function.
    if let Some(s) = m.start {
        let ti = *m
            .func_types
            .get(s as usize)
            .ok_or_else(|| "start function index out of range".to_string())?;
        let ft = m
            .types
            .get(ti as usize)
            .ok_or_else(|| "start function type out of range".to_string())?;
        if !ft.params.is_empty() || !ft.results.is_empty() {
            return Err("start function must have type [] -> []".to_string());
        }
    }

    // Function bodies.
    for (i, f) in m.funcs.iter().enumerate() {
        let ft = &m.types[f.type_idx as usize];
        let mut locals = ft.params.clone();
        locals.extend_from_slice(&f.locals);
        let mut v = Validator {
            m,
            locals,
            return_types: ft.results.clone(),
            refs: &refs,
            vals: Vec::new(),
            ctrls: vec![Ctrl {
                kind: FrameKind::Function,
                start: vec![],
                end: ft.results.clone(),
                height: 0,
                unreachable: false,
            }],
        };
        for (pc, ins) in f.body.iter().enumerate() {
            v.step(ins)
                .map_err(|e| format!("function {}: instruction {} ({:?}): {}", i, pc, ins, e))?;
        }
        if !v.ctrls.is_empty() {
            return Err(format!("function {}: unbalanced control structure", i));
        }
        if v.vals.len() != ft.results.len() {
            return Err(format!("function {}: result arity mismatch", i));
        }
        for (a, b) in v.vals.iter().zip(ft.results.iter()) {
            if let Opd::Known(t) = a {
                if t != b {
                    return Err(format!("function {}: result type mismatch", i));
                }
            }
        }
    }

    Ok(())
}

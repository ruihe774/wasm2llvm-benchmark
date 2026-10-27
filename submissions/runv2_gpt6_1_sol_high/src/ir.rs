//! Textual LLVM lowering. Operand values use SSA; entry-block allocas carry
//! locals and structured-control merges (LLVM promotes them at optimization).
//! Every potentially trapping operation is guarded before any native operation
//! that could be undefined. Only the public wrappers use the slot ABI.
use crate::model::*;
use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;

#[derive(Clone, Debug)]
pub struct Val {
    pub ty: Ty,
    pub v: String,
}
#[derive(Clone)]
struct Frame {
    kind: u32,
    height: usize,
    params: Vec<Ty>,
    results: Vec<Ty>,
    saved: Vec<Val>,
    slots: Vec<String>,
    label: String,
    end: String,
    other: String,
    outer: bool,
    used: bool,
    had_else: bool,
}
pub struct Emitter<'a> {
    pub m: &'a Module,
    type_ids: &'a [usize],
    pub body: String,
    allocas: String,
    pub decls: BTreeSet<String>,
    next: usize,
    stack: Vec<Val>,
    frames: Vec<Frame>,
    live: bool,
    locals: Vec<Ty>,
}
pub fn hex(s: &str) -> String {
    s.as_bytes().iter().map(|b| format!("{b:02x}")).collect()
}
fn ret_type(t: &FuncType) -> String {
    match t.results.len() {
        0 => "void".into(),
        1 => t.results[0].llvm().into(),
        _ => format!(
            "{{ {} }}",
            t.results
                .iter()
                .map(|t| t.llvm())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}
fn signature(t: &FuncType) -> String {
    t.params
        .iter()
        .enumerate()
        .map(|(j, t)| format!("{} %a{j}", t.llvm()))
        .collect::<Vec<_>>()
        .join(", ")
}
impl<'a> Emitter<'a> {
    fn new(m: &'a Module, type_ids: &'a [usize]) -> Self {
        Self {
            m,
            type_ids,
            body: String::new(),
            allocas: String::new(),
            decls: BTreeSet::new(),
            next: 0,
            stack: vec![],
            frames: vec![],
            live: true,
            locals: vec![],
        }
    }
    fn name(&mut self) -> String {
        let n = self.next;
        self.next += 1;
        format!("%v{n}")
    }
    fn label(&mut self) -> String {
        let n = self.next;
        self.next += 1;
        format!("b{n}")
    }
    pub fn line(&mut self, s: impl Into<String>) {
        writeln!(self.body, "  {}", s.into()).unwrap();
    }
    fn mark(&mut self, l: &str) {
        writeln!(self.body, "{l}:").unwrap();
    }
    pub fn raw(&mut self, s: impl Into<String>) -> String {
        let v = self.name();
        self.line(format!("{v} = {}", s.into()));
        v
    }
    pub fn value(&mut self, ty: Ty, s: impl Into<String>) -> Val {
        Val { ty, v: self.raw(s) }
    }
    pub fn declare(&mut self, s: impl Into<String>) {
        self.decls.insert(s.into());
    }
    pub fn pop(&mut self, t: Ty) -> Result<Val, String> {
        let v = self.stack.pop().ok_or("lowering stack underflow")?;
        if v.ty != t {
            return Err("lowering type mismatch".into());
        }
        Ok(v)
    }
    fn any(&mut self) -> Result<Val, String> {
        self.stack.pop().ok_or("lowering stack underflow".into())
    }
    pub fn push(&mut self, v: Val) {
        self.stack.push(v)
    }
    fn alloca(&mut self, t: &str) -> String {
        let v = self.name();
        writeln!(self.allocas, "  {v} = alloca {t}, align 16").unwrap();
        v
    }
    pub fn check(&mut self, cond: &str, code: u32) {
        let bad = self.label();
        let ok = self.label();
        self.line(format!("br i1 {cond}, label %{bad}, label %{ok}"));
        self.mark(&bad);
        self.line(format!("call void @trap(i32 {code})"));
        self.line("unreachable");
        self.mark(&ok);
    }
    pub fn mem_ptr(&mut self, addr: &Val, offset: u64, size: u64) -> Result<String, String> {
        if self.m.memory.is_none() {
            return Err("memory instruction without memory".into());
        }
        let a = self.raw(format!("zext i32 {} to i64", addr.v));
        let a = if offset == 0 {
            a
        } else {
            self.raw(format!("add i64 {a}, {offset}"))
        };
        let end = self.raw(format!("add i64 {a}, {size}"));
        let len = self.raw("load i64, ptr @mem.size");
        let bad = self.raw(format!("icmp ugt i64 {end}, {len}"));
        self.check(&bad, 4);
        let base = self.raw("load ptr, ptr @mem.base");
        Ok(self.raw(format!("getelementptr i8, ptr {base}, i64 {a}")))
    }
    fn range(&mut self, addr: &Val, n: &Val, len: &str, code: u32) -> String {
        let a = self.raw(format!("zext i32 {} to i64", addr.v));
        let n = self.raw(format!("zext i32 {} to i64", n.v));
        let end = self.raw(format!("add i64 {a}, {n}"));
        let bad = self.raw(format!("icmp ugt i64 {end}, {len}"));
        self.check(&bad, code);
        a
    }
    fn take(&mut self, ts: &[Ty]) -> Result<Vec<Val>, String> {
        let mut v = Vec::new();
        for t in ts.iter().rev() {
            v.push(self.pop(*t)?)
        }
        v.reverse();
        Ok(v)
    }
    fn peek(&self, ts: &[Ty]) -> Vec<Val> {
        self.stack[self.stack.len() - ts.len()..].to_vec()
    }
    fn stores(&mut self, vs: &[Val], slots: &[String]) {
        for (v, p) in vs.iter().zip(slots) {
            self.line(format!("store {} {}, ptr {p}, align 1", v.ty.llvm(), v.v));
        }
    }
    fn loads(&mut self, ts: &[Ty], slots: &[String]) -> Vec<Val> {
        ts.iter()
            .zip(slots)
            .map(|(t, p)| self.value(*t, format!("load {}, ptr {p}, align 1", t.llvm())))
            .collect()
    }
    fn block_type(&self, i: &Instr) -> (Vec<Ty>, Vec<Ty>) {
        match i.imm {
            -64 => (vec![], vec![]),
            -1 => (vec![], vec![Ty::I32]),
            -2 => (vec![], vec![Ty::I64]),
            -3 => (vec![], vec![Ty::F32]),
            -4 => (vec![], vec![Ty::F64]),
            -5 => (vec![], vec![Ty::V128]),
            n => {
                let t = &self.m.types[n as usize];
                (t.params.clone(), t.results.clone())
            }
        }
    }
    fn begin(&mut self, i: &Instr) -> Result<(), String> {
        let (params, results) = self.block_type(i);
        let outer = self.live;
        let cond = if outer && i.op == 4 {
            Some(self.pop(Ty::I32)?)
        } else {
            None
        };
        let saved = if outer { self.take(&params)? } else { vec![] };
        let height = self.stack.len();
        let label = self.label();
        let end = self.label();
        let other = self.label();
        let slot_types = if i.op == 3 { &params } else { &results };
        let slots = slot_types
            .iter()
            .map(|t| self.alloca(t.llvm()))
            .collect::<Vec<_>>();
        if outer {
            if i.op == 3 {
                self.stores(&saved, &slots);
                self.line(format!("br label %{label}"));
                self.mark(&label);
                let vals = self.loads(&params, &slots);
                self.stack.extend(vals);
            } else {
                self.stack.extend(saved.clone());
                if let Some(c) = cond {
                    let c = self.raw(format!("icmp ne i32 {}, 0", c.v));
                    self.line(format!("br i1 {c}, label %{label}, label %{other}"));
                    self.mark(&label);
                }
            }
        }
        self.frames.push(Frame {
            kind: i.op,
            height,
            params,
            results,
            saved,
            slots,
            label,
            end,
            other,
            outer,
            used: false,
            had_else: false,
        });
        Ok(())
    }
    fn end(&mut self, is_else: bool) -> Result<(), String> {
        let mut f = self.frames.pop().ok_or("bad control frame")?;
        if self.live {
            let vs = self.take(&f.results)?;
            if f.kind == 3 {
                // Loops branch to parameters, but fall through with results.
                let slots = f
                    .results
                    .iter()
                    .map(|t| self.alloca(t.llvm()))
                    .collect::<Vec<_>>();
                self.stores(&vs, &slots);
                f.slots = slots;
            } else {
                self.stores(&vs, &f.slots)
            }
            self.line(format!("br label %{}", f.end));
            f.used = true;
        }
        self.stack.truncate(f.height);
        if is_else {
            self.mark(&f.other);
            self.live = f.outer;
            if !self.live {
                self.line("unreachable");
            }
            self.stack.extend(f.saved.clone());
            f.had_else = true;
            self.frames.push(f);
            return Ok(());
        }
        if f.kind == 4 && !f.had_else {
            self.mark(&f.other);
            if f.outer {
                self.stores(&f.saved, &f.slots);
                self.line(format!("br label %{}", f.end));
                f.used = true;
            } else {
                self.line("unreachable");
            }
        }
        self.live = f.used;
        // A loop's backedges do not make its end reachable.
        self.mark(&f.end);
        if self.live {
            let vals = self.loads(&f.results, &f.slots);
            self.stack.extend(vals);
        } else {
            self.line("unreachable");
        }
        Ok(())
    }
    fn branch(&mut self, depth: u32, conditional: Option<Val>) -> Result<(), String> {
        let idx = self.frames.len() - 1 - depth as usize;
        let f = self.frames[idx].clone();
        let ts = if f.kind == 3 { &f.params } else { &f.results };
        let vs = self.peek(ts);
        self.stores(&vs, &f.slots);
        let target = if f.kind == 3 {
            f.label
        } else {
            self.frames[idx].used = true;
            f.end
        };
        if let Some(c) = conditional {
            let c = self.raw(format!("icmp ne i32 {}, 0", c.v));
            let cont = self.label();
            self.line(format!("br i1 {c}, label %{target}, label %{cont}"));
            self.mark(&cont);
        } else {
            self.line(format!("br label %{target}"));
            self.live = false;
        }
        Ok(())
    }
    fn call(&mut self, t: &FuncType, callee: &str) -> Result<(), String> {
        let vs = self.take(&t.params)?;
        let args = vs
            .iter()
            .map(|v| format!("{} {}", v.ty.llvm(), v.v))
            .collect::<Vec<_>>()
            .join(", ");
        let rt = ret_type(t);
        if t.results.is_empty() {
            self.line(format!("call {rt} {callee}({args})"));
        } else {
            let v = self.raw(format!("call {rt} {callee}({args})"));
            if t.results.len() == 1 {
                self.push(Val {
                    ty: t.results[0],
                    v,
                })
            } else {
                for (j, ty) in t.results.iter().enumerate() {
                    let r = self.value(*ty, format!("extractvalue {rt} {v}, {j}"));
                    self.push(r)
                }
            }
        }
        Ok(())
    }
    fn instr(&mut self, i: &Instr) -> Result<(), String> {
        match i.op {
            2 | 3 | 4 => return self.begin(i),
            5 => return self.end(true),
            11 => return self.end(false),
            _ => {}
        }
        if !self.live {
            return Ok(());
        }
        match i.op {
            0 => {
                self.line("call void @trap(i32 0)");
                self.line("unreachable");
                self.live = false;
            }
            1 => {}
            12 => self.branch(i.a, None)?,
            13 => {
                let c = self.pop(Ty::I32)?;
                self.branch(i.a, Some(c))?;
            }
            14 => {
                let c = self.pop(Ty::I32)?;
                let default = self.label();
                let labels = (0..i.list.len()).map(|_| self.label()).collect::<Vec<_>>();
                let cases = labels
                    .iter()
                    .enumerate()
                    .map(|(j, l)| format!("i32 {j}, label %{l}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                self.line(format!("switch i32 {}, label %{default} [ {cases} ]", c.v));
                for (&d, l) in i.list.iter().zip(&labels) {
                    self.mark(l);
                    self.branch(d, None)?;
                }
                self.mark(&default);
                self.branch(i.a, None)?;
            }
            15 => self.branch((self.frames.len() - 1) as u32, None)?,
            16 => {
                let t = self.m.types[self.m.funcs[i.a as usize].type_idx as usize].clone();
                self.call(&t, &format!("@f{}", i.a))?;
            }
            17 => {
                let idx = self.pop(Ty::I32)?;
                let x = self.raw(format!("zext i32 {} to i64", idx.v));
                let lim = self.m.table.as_ref().unwrap().min;
                let bad = self.raw(format!("icmp uge i64 {x}, {lim}"));
                self.check(&bad, 5);
                let base = self.raw("load ptr, ptr @table.base");
                let p = self.raw(format!("getelementptr i32, ptr {base}, i64 {x}"));
                let id = self.raw(format!("load i32, ptr {p}, align 4"));
                let null = self.raw(format!("icmp eq i32 {id}, 0"));
                self.check(&null, 6);
                let fi = self.raw(format!("sub i32 {id}, 1"));
                let tp = self.raw(format!(
                    "getelementptr [{} x i32], ptr @func.types, i32 0, i32 {fi}",
                    self.m.funcs.len()
                ));
                let ty = self.raw(format!("load i32, ptr {tp}"));
                let mismatch =
                    self.raw(format!("icmp ne i32 {ty}, {}", self.type_ids[i.a as usize]));
                self.check(&mismatch, 7);
                let pp = self.raw(format!(
                    "getelementptr [{} x ptr], ptr @func.ptrs, i32 0, i32 {fi}",
                    self.m.funcs.len()
                ));
                let fun = self.raw(format!("load ptr, ptr {pp}"));
                let t = self.m.types[i.a as usize].clone();
                self.call(&t, &fun)?;
            }
            26 => {
                self.any()?;
            }
            27 => {
                let c = self.pop(Ty::I32)?;
                let b = self.any()?;
                let a = self.pop(b.ty)?;
                let c = self.raw(format!("icmp ne i32 {}, 0", c.v));
                let v = self.value(
                    a.ty,
                    format!(
                        "select i1 {c}, {} {}, {} {}",
                        a.ty.llvm(),
                        a.v,
                        b.ty.llvm(),
                        b.v
                    ),
                );
                self.push(v);
            }
            32 => {
                let ty = self.local_type(i.a);
                let v = self.value(ty, format!("load {}, ptr %l{}, align 1", ty.llvm(), i.a));
                self.push(v);
            }
            33 | 34 => {
                let ty = self.local_type(i.a);
                let v = self.pop(ty)?;
                self.line(format!(
                    "store {} {}, ptr %l{}, align 1",
                    ty.llvm(),
                    v.v,
                    i.a
                ));
                if i.op == 34 {
                    self.push(v)
                }
            }
            35 => {
                let ty = self.m.globals[i.a as usize].ty;
                let v = self.value(ty, format!("load {}, ptr @g{}", ty.llvm(), i.a));
                self.push(v);
            }
            36 => {
                let ty = self.m.globals[i.a as usize].ty;
                let v = self.pop(ty)?;
                self.line(format!("store {} {}, ptr @g{}", ty.llvm(), v.v, i.a));
            }
            40..=62 => self.memory(i)?,
            63 => {
                let n = self.raw("load i64, ptr @mem.size");
                let n = self.raw(format!("lshr i64 {n}, 16"));
                let v = self.value(Ty::I32, format!("trunc i64 {n} to i32"));
                self.push(v);
            }
            64 => {
                let n = self.pop(Ty::I32)?;
                let v = self.value(Ty::I32, format!("call i32 @memory.grow(i32 {})", n.v));
                self.push(v);
            }
            65 => self.push(Val {
                ty: Ty::I32,
                v: (i.imm as i32).to_string(),
            }),
            66 => self.push(Val {
                ty: Ty::I64,
                v: i.imm.to_string(),
            }),
            67 => {
                let bits = u32::from_le_bytes(i.bytes[..4].try_into().unwrap());
                let v = self.value(Ty::F32, format!("bitcast i32 {bits} to float"));
                self.push(v);
            }
            68 => {
                let bits = u64::from_le_bytes(i.bytes[..8].try_into().unwrap());
                let v = self.value(Ty::F64, format!("bitcast i64 {bits} to double"));
                self.push(v);
            }
            69..=196 => self.numeric(i.op)?,
            0xfc00..=0xfc07 => self.convert_sat(i.op - 0xfc00)?,
            0xfc08..=0xfc0e => self.bulk(i)?,
            0xfd00..=0xfdff => crate::simd::emit(self, i)?,
            _ => return Err(format!("unsupported opcode {:x}", i.op)),
        }
        Ok(())
    }
    fn local_type(&self, i: u32) -> Ty {
        self.locals[i as usize]
    }
    fn memory(&mut self, i: &Instr) -> Result<(), String> {
        let (t, bits, signed, store) = match i.op {
            40 => (Ty::I32, 32, false, false),
            41 => (Ty::I64, 64, false, false),
            42 => (Ty::F32, 32, false, false),
            43 => (Ty::F64, 64, false, false),
            44 => (Ty::I32, 8, true, false),
            45 => (Ty::I32, 8, false, false),
            46 => (Ty::I32, 16, true, false),
            47 => (Ty::I32, 16, false, false),
            48 => (Ty::I64, 8, true, false),
            49 => (Ty::I64, 8, false, false),
            50 => (Ty::I64, 16, true, false),
            51 => (Ty::I64, 16, false, false),
            52 => (Ty::I64, 32, true, false),
            53 => (Ty::I64, 32, false, false),
            54 => (Ty::I32, 32, false, true),
            55 => (Ty::I64, 64, false, true),
            56 => (Ty::F32, 32, false, true),
            57 => (Ty::F64, 64, false, true),
            58 => (Ty::I32, 8, false, true),
            59 => (Ty::I32, 16, false, true),
            60 => (Ty::I64, 8, false, true),
            61 => (Ty::I64, 16, false, true),
            62 => (Ty::I64, 32, false, true),
            _ => unreachable!(),
        };
        let v = if store { Some(self.pop(t)?) } else { None };
        let addr = self.pop(Ty::I32)?;
        let p = self.mem_ptr(&addr, i.b as u64, bits / 8)?;
        let mt = if matches!(t, Ty::F32 | Ty::F64) {
            t.llvm().to_string()
        } else {
            format!("i{bits}")
        };
        if let Some(v) = v {
            let x = if mt != t.llvm() {
                self.raw(format!("trunc {} {} to {mt}", t.llvm(), v.v))
            } else {
                v.v
            };
            self.line(format!("store {mt} {x}, ptr {p}, align 1"));
        } else {
            let x = self.raw(format!("load {mt}, ptr {p}, align 1"));
            let x = if mt != t.llvm() {
                self.raw(format!(
                    "{} {mt} {x} to {}",
                    if signed { "sext" } else { "zext" },
                    t.llvm()
                ))
            } else {
                x
            };
            self.push(Val { ty: t, v: x });
        }
        Ok(())
    }
    fn numeric(&mut self, op: u32) -> Result<(), String> {
        if (69..=90).contains(&op) {
            let ty = if op <= 79 { Ty::I32 } else { Ty::I64 };
            if op == 69 || op == 80 {
                let a = self.pop(ty)?;
                let c = self.raw(format!("icmp eq {} {}, 0", ty.llvm(), a.v));
                let v = self.value(Ty::I32, format!("zext i1 {c} to i32"));
                self.push(v);
                return Ok(());
            }
            let base = if ty == Ty::I32 { 70 } else { 81 };
            let pred = [
                "eq", "ne", "slt", "ult", "sgt", "ugt", "sle", "ule", "sge", "uge",
            ][(op - base) as usize];
            let b = self.pop(ty)?;
            let a = self.pop(ty)?;
            let c = self.raw(format!("icmp {pred} {} {}, {}", ty.llvm(), a.v, b.v));
            let v = self.value(Ty::I32, format!("zext i1 {c} to i32"));
            self.push(v);
            return Ok(());
        }
        if (91..=102).contains(&op) {
            let ty = if op < 97 { Ty::F32 } else { Ty::F64 };
            let k = (op - if ty == Ty::F32 { 91 } else { 97 }) as usize;
            let pred = ["oeq", "une", "olt", "ogt", "ole", "oge"][k];
            let b = self.pop(ty)?;
            let a = self.pop(ty)?;
            let c = self.raw(format!("fcmp {pred} {} {}, {}", ty.llvm(), a.v, b.v));
            let v = self.value(Ty::I32, format!("zext i1 {c} to i32"));
            self.push(v);
            return Ok(());
        }
        if (103..=138).contains(&op) {
            let ty = if op < 121 { Ty::I32 } else { Ty::I64 };
            let base = if ty == Ty::I32 { 103 } else { 121 };
            let k = op - base;
            let lt = ty.llvm();
            let width = if ty == Ty::I32 { 32 } else { 64 };
            if k < 3 {
                let a = self.pop(ty)?;
                let name = ["ctlz", "cttz", "ctpop"][k as usize];
                let intr = format!("@llvm.{name}.{lt}");
                let arg = if k == 2 { "" } else { ", i1 false" };
                self.declare(format!(
                    "declare {lt} {intr}({lt}{})",
                    if k == 2 { "" } else { ", i1" }
                ));
                let v = self.value(ty, format!("call {lt} {intr}({lt} {}{arg})", a.v));
                self.push(v);
                return Ok(());
            }
            let b = self.pop(ty)?;
            let a = self.pop(ty)?;
            let v = match k {
                3..=5 => self.value(
                    ty,
                    format!(
                        "{} {lt} {}, {}",
                        ["add", "sub", "mul"][(k - 3) as usize],
                        a.v,
                        b.v
                    ),
                ),
                6..=9 => {
                    let zero = self.raw(format!("icmp eq {lt} {}, 0", b.v));
                    self.check(&zero, 1);
                    let signed = k == 6 || k == 8;
                    let bval = if signed {
                        let min = if width == 32 {
                            "-2147483648"
                        } else {
                            "-9223372036854775808"
                        };
                        let amin = self.raw(format!("icmp eq {lt} {}, {min}", a.v));
                        let bm = self.raw(format!("icmp eq {lt} {}, -1", b.v));
                        let ov = self.raw(format!("and i1 {amin}, {bm}"));
                        if k == 6 {
                            self.check(&ov, 2);
                            b.v.clone()
                        } else {
                            self.raw(format!("select i1 {ov}, {lt} 1, {lt} {}", b.v))
                        }
                    } else {
                        b.v.clone()
                    };
                    self.value(
                        ty,
                        format!(
                            "{} {lt} {}, {bval}",
                            match k {
                                6 => "sdiv",
                                7 => "udiv",
                                8 => "srem",
                                _ => "urem",
                            },
                            a.v
                        ),
                    )
                }
                10..=12 => self.value(
                    ty,
                    format!(
                        "{} {lt} {}, {}",
                        ["and", "or", "xor"][(k - 10) as usize],
                        a.v,
                        b.v
                    ),
                ),
                13..=15 => {
                    let n = self.raw(format!("and {lt} {}, {}", b.v, width - 1));
                    self.value(
                        ty,
                        format!(
                            "{} {lt} {}, {n}",
                            ["shl", "ashr", "lshr"][(k - 13) as usize],
                            a.v
                        ),
                    )
                }
                16 | 17 => {
                    let name = if k == 16 { "fshl" } else { "fshr" };
                    self.declare(format!("declare {lt} @llvm.{name}.{lt}({lt}, {lt}, {lt})"));
                    self.value(
                        ty,
                        format!(
                            "call {lt} @llvm.{name}.{lt}({lt} {}, {lt} {}, {lt} {})",
                            a.v, a.v, b.v
                        ),
                    )
                }
                _ => unreachable!(),
            };
            self.push(v);
            return Ok(());
        }
        if (139..=166).contains(&op) {
            let ty = if op < 153 { Ty::F32 } else { Ty::F64 };
            let k = op - if ty == Ty::F32 { 139 } else { 153 };
            let lt = ty.llvm();
            let suf = if ty == Ty::F32 { "f32" } else { "f64" };
            if k < 7 {
                let a = self.pop(ty)?;
                let v = match k {
                    1 => self.value(ty, format!("fneg {lt} {}", a.v)),
                    _ => {
                        let name =
                            ["fabs", "", "ceil", "floor", "trunc", "roundeven", "sqrt"][k as usize];
                        self.declare(format!("declare {lt} @llvm.{name}.{suf}({lt})"));
                        self.value(ty, format!("call {lt} @llvm.{name}.{suf}({lt} {})", a.v))
                    }
                };
                self.push(v);
            } else {
                let b = self.pop(ty)?;
                let a = self.pop(ty)?;
                let v = match k {
                    7..=10 => self.value(
                        ty,
                        format!(
                            "{} {lt} {}, {}",
                            ["fadd", "fsub", "fmul", "fdiv"][(k - 7) as usize],
                            a.v,
                            b.v
                        ),
                    ),
                    11 | 12 => {
                        let name = if k == 11 { "minimum" } else { "maximum" };
                        self.declare(format!("declare {lt} @llvm.{name}.{suf}({lt}, {lt})"));
                        self.value(
                            ty,
                            format!("call {lt} @llvm.{name}.{suf}({lt} {}, {lt} {})", a.v, b.v),
                        )
                    }
                    13 => {
                        self.declare(format!("declare {lt} @llvm.copysign.{suf}({lt}, {lt})"));
                        self.value(
                            ty,
                            format!("call {lt} @llvm.copysign.{suf}({lt} {}, {lt} {})", a.v, b.v),
                        )
                    }
                    _ => unreachable!(),
                };
                self.push(v)
            }
            return Ok(());
        }
        match op {
            167 => {
                let a = self.pop(Ty::I64)?;
                let v = self.value(Ty::I32, format!("trunc i64 {} to i32", a.v));
                self.push(v)
            }
            168..=171 | 174..=177 => {
                let to = if op < 174 { Ty::I32 } else { Ty::I64 };
                let k = if op < 174 { op - 168 } else { op - 174 };
                let from = if k < 2 { Ty::F32 } else { Ty::F64 };
                let a = self.pop(from)?;
                let v = self.float_to_int(a, to, k % 2 == 0, false);
                self.push(v)
            }
            172 | 173 => {
                let a = self.pop(Ty::I32)?;
                let v = self.value(
                    Ty::I64,
                    format!(
                        "{} i32 {} to i64",
                        if op == 172 { "sext" } else { "zext" },
                        a.v
                    ),
                );
                self.push(v)
            }
            178..=181 | 183..=186 => {
                let to = if op < 183 { Ty::F32 } else { Ty::F64 };
                let k = if op < 183 { op - 178 } else { op - 183 };
                let from = if k < 2 { Ty::I32 } else { Ty::I64 };
                let a = self.pop(from)?;
                let v = self.value(
                    to,
                    format!(
                        "{} {} {} to {}",
                        if k % 2 == 0 { "sitofp" } else { "uitofp" },
                        from.llvm(),
                        a.v,
                        to.llvm()
                    ),
                );
                self.push(v)
            }
            182 | 187 => {
                let to = if op == 182 { Ty::F32 } else { Ty::F64 };
                let from = if op == 182 { Ty::F64 } else { Ty::F32 };
                let a = self.pop(from)?;
                let v = self.value(
                    to,
                    format!(
                        "{} {} {} to {}",
                        if op == 182 { "fptrunc" } else { "fpext" },
                        from.llvm(),
                        a.v,
                        to.llvm()
                    ),
                );
                self.push(v)
            }
            188..=191 => {
                let (from, to) = match op {
                    188 => (Ty::F32, Ty::I32),
                    189 => (Ty::F64, Ty::I64),
                    190 => (Ty::I32, Ty::F32),
                    _ => (Ty::I64, Ty::F64),
                };
                let a = self.pop(from)?;
                let v = self.value(
                    to,
                    format!("bitcast {} {} to {}", from.llvm(), a.v, to.llvm()),
                );
                self.push(v)
            }
            192..=196 => {
                let to = if op <= 193 { Ty::I32 } else { Ty::I64 };
                let n = match op {
                    192 | 194 => 8,
                    193 | 195 => 16,
                    _ => 32,
                };
                let a = self.pop(to)?;
                let x = self.raw(format!("trunc {} {} to i{n}", to.llvm(), a.v));
                let v = self.value(to, format!("sext i{n} {x} to {}", to.llvm()));
                self.push(v)
            }
            _ => return Err(format!("unimplemented numeric {op}")),
        }
        Ok(())
    }
    fn float_to_int(&mut self, a: Val, to: Ty, signed: bool, sat: bool) -> Val {
        let from = a.ty;
        let ft = from.llvm();
        let it = to.llvm();
        let fs = if from == Ty::F32 { "f32" } else { "f64" };
        let name = if signed { "fptosi" } else { "fptoui" };
        if !sat {
            let nan = self.raw(format!("fcmp uno {ft} {}, {}", a.v, a.v));
            self.check(&nan, 3);
            // Range check the truncated floating value, not the original input.
            self.declare(format!("declare {ft} @llvm.trunc.{fs}({ft})"));
            let x = self.raw(format!("call {ft} @llvm.trunc.{fs}({ft} {})", a.v));
            let n = if to == Ty::I32 { 32 } else { 64 };
            let lower = if signed { -(2f64).powi(n - 1) } else { 0.0 };
            let upper = (2f64).powi(if signed { n - 1 } else { n });
            let lo = self.raw(format!("fcmp olt {ft} {x}, 0x{:016X}", lower.to_bits()));
            let hi = self.raw(format!("fcmp oge {ft} {x}, 0x{:016X}", upper.to_bits()));
            let bad = self.raw(format!("or i1 {lo}, {hi}"));
            self.check(&bad, 2);
            self.value(to, format!("{name} {ft} {x} to {it}"))
        } else {
            self.declare(format!("declare {it} @llvm.{name}.sat.{it}.{fs}({ft})"));
            self.value(
                to,
                format!("call {it} @llvm.{name}.sat.{it}.{fs}({ft} {})", a.v),
            )
        }
    }
    fn convert_sat(&mut self, k: u32) -> Result<(), String> {
        let to = if k < 4 { Ty::I32 } else { Ty::I64 };
        let from = if k % 4 < 2 { Ty::F32 } else { Ty::F64 };
        let a = self.pop(from)?;
        let v = self.float_to_int(a, to, k % 2 == 0, true);
        self.push(v);
        Ok(())
    }
    fn bulk(&mut self, i: &Instr) -> Result<(), String> {
        match i.op - 0xfc00 {
            9 => self.line(format!("store i64 0, ptr @data.len{}", i.a)),
            13 => self.line(format!("store i64 0, ptr @elem.len{}", i.a)),
            8 | 10 | 11 | 12 | 14 => {
                let n = self.pop(Ty::I32)?;
                let src = self.pop(Ty::I32)?;
                let dst = self.pop(Ty::I32)?;
                let table = i.op == 0xfc0c || i.op == 0xfc0e;
                let code = if table { 5 } else { 4 };
                let dl = if table {
                    self.m.table.as_ref().unwrap().min.to_string()
                } else {
                    self.raw("load i64, ptr @mem.size")
                };
                let d = self.range(&dst, &n, &dl, code);
                let base = self.raw(if table {
                    "load ptr, ptr @table.base"
                } else {
                    "load ptr, ptr @mem.base"
                });
                let dp = self.raw(format!(
                    "getelementptr {}, ptr {base}, i64 {d}",
                    if table { "i32" } else { "i8" }
                ));
                let len = self.raw(format!("zext i32 {} to i64", n.v));
                if i.op == 0xfc0b {
                    let b = self.raw(format!("trunc i32 {} to i8", src.v));
                    self.line(format!(
                        "call void @llvm.memset.p0.i64(ptr {dp}, i8 {b}, i64 {len}, i1 false)"
                    ));
                } else {
                    let (sl, sbase) = if i.op == 0xfc08 {
                        (
                            self.raw(format!("load i64, ptr @data.len{}", i.a)),
                            format!("@data{}", i.a),
                        )
                    } else if i.op == 0xfc0c {
                        (
                            self.raw(format!("load i64, ptr @elem.len{}", i.a)),
                            format!("@elem{}", i.a),
                        )
                    } else {
                        (dl, base)
                    };
                    let s = self.range(&src, &n, &sl, code);
                    let sp = self.raw(format!(
                        "getelementptr {}, ptr {sbase}, i64 {s}",
                        if table { "i32" } else { "i8" }
                    ));
                    let len = if table {
                        self.raw(format!("mul i64 {len}, 4"))
                    } else {
                        len
                    };
                    self.line(format!("call void @llvm.memmove.p0.p0.i64(ptr {dp}, ptr {sp}, i64 {len}, i1 false)"));
                }
            }
            _ => return Err("unsupported bulk memory opcode".into()),
        }
        Ok(())
    }
}

fn native(
    m: &Module,
    index: usize,
    type_ids: &[usize],
) -> Result<(String, BTreeSet<String>), String> {
    let f = &m.funcs[index];
    let t = &m.types[f.type_idx as usize];
    let rt = ret_type(t);
    let mut e = Emitter::new(m, type_ids);
    if let Some((module, name)) = &f.import {
        let args = e.alloca(&format!("[{} x i128]", t.params.len().max(1)));
        let res = e.alloca(&format!("[{} x i128]", t.results.len().max(1)));
        for (j, ty) in t.params.iter().enumerate() {
            let p = e.raw(format!("getelementptr i8, ptr {args}, i64 {}", j * 16));
            e.line(format!("store {} %a{j}, ptr {p}, align 1", ty.llvm()));
        }
        let sym = format!("@w2l_import_{}_{}", hex(module), hex(name));
        e.declare(format!("declare void {sym}(ptr, ptr)"));
        e.line(format!("call void {sym}(ptr {args}, ptr {res})"));
        let mut vals = vec![];
        for (j, ty) in t.results.iter().enumerate() {
            let p = e.raw(format!("getelementptr i8, ptr {res}, i64 {}", j * 16));
            vals.push(e.value(*ty, format!("load {}, ptr {p}, align 1", ty.llvm())))
        }
        return_values(&mut e, t, &vals);
    } else {
        let locals = t
            .params
            .iter()
            .chain(&f.locals)
            .copied()
            .collect::<Vec<_>>();
        e.locals = locals.clone();
        for (j, ty) in locals.iter().enumerate() {
            writeln!(e.allocas, "  %l{j} = alloca {}, align 16", ty.llvm()).unwrap();
            e.line(format!(
                "store {} {}, ptr %l{j}, align 1",
                ty.llvm(),
                if j < t.params.len() {
                    format!("%a{j}")
                } else {
                    ty.zero().into()
                }
            ));
        }
        let d = e.raw("load i32, ptr @depth");
        let d1 = e.raw(format!("add i32 {d}, 1"));
        e.line(format!("store i32 {d1}, ptr @depth"));
        let bad = e.raw(format!("icmp ugt i32 {d1}, 30000"));
        e.check(&bad, 8);
        let end = e.label();
        let slots = t.results.iter().map(|ty| e.alloca(ty.llvm())).collect();
        e.frames.push(Frame {
            kind: 0,
            height: 0,
            params: vec![],
            results: t.results.clone(),
            saved: locals
                .iter()
                .map(|ty| Val {
                    ty: *ty,
                    v: String::new(),
                })
                .collect(),
            slots,
            label: String::new(),
            end,
            other: String::new(),
            outer: true,
            used: false,
            had_else: false,
        });
        for i in &f.code {
            e.instr(i)?;
        }
        if !e.frames.is_empty() {
            return Err("unterminated function".into());
        }
        if e.live {
            let vals = e.take(&t.results)?;
            let d = e.raw("load i32, ptr @depth");
            let d1 = e.raw(format!("sub i32 {d}, 1"));
            e.line(format!("store i32 {d1}, ptr @depth"));
            return_values(&mut e, t, &vals);
        }
    }
    Ok((
        format!(
            "define internal {rt} @f{index}({}) {{\nentry:\n{}{} }}\n",
            signature(t),
            e.allocas,
            e.body
        ),
        e.decls,
    ))
}
fn return_values(e: &mut Emitter, t: &FuncType, vals: &[Val]) {
    let rt = ret_type(t);
    if vals.is_empty() {
        e.line("ret void");
    } else if vals.len() == 1 {
        e.line(format!("ret {rt} {}", vals[0].v));
    } else {
        let mut a = "zeroinitializer".to_string();
        for (j, v) in vals.iter().enumerate() {
            a = e.raw(format!(
                "insertvalue {rt} {a}, {} {}, {j}",
                v.ty.llvm(),
                v.v
            ));
        }
        e.line(format!("ret {rt} {a}"));
    }
}
fn const_val(expr: &[Instr]) -> Val {
    let i = &expr[0];
    match i.op {
        65 => Val {
            ty: Ty::I32,
            v: (i.imm as i32).to_string(),
        },
        66 => Val {
            ty: Ty::I64,
            v: i.imm.to_string(),
        },
        67 => Val {
            ty: Ty::F32,
            v: format!(
                "bitcast (i32 {} to float)",
                u32::from_le_bytes(i.bytes[..4].try_into().unwrap())
            ),
        },
        68 => Val {
            ty: Ty::F64,
            v: format!(
                "bitcast (i64 {} to double)",
                u64::from_le_bytes(i.bytes[..8].try_into().unwrap())
            ),
        },
        0xfd0c => Val {
            ty: Ty::V128,
            v: format!(
                "<{}>",
                i.bytes
                    .iter()
                    .map(|b| format!("i8 {b}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        },
        _ => unreachable!(),
    }
}
pub fn translate(m: &Module) -> Result<String, String> {
    // Structural equivalence, not a raw type index, determines indirect-call compatibility.
    let mut canonical = HashMap::new();
    let ids: Vec<usize> = m
        .types
        .iter()
        .enumerate()
        .map(|(j, t)| *canonical.entry(t).or_insert(j))
        .collect();
    let type_ids = ids.as_slice();
    let mut out = String::from("target triple = \"x86_64-unknown-linux-gnu\"\n");
    let mut decls = BTreeSet::new();
    out.push_str("@depth = internal global i32 0\n@mem.base = internal global ptr null\n@mem.size = internal global i64 0\n@table.base = internal global ptr null\n");
    for (j, g) in m.globals.iter().enumerate() {
        writeln!(
            out,
            "@g{j} = internal global {} {}",
            g.ty.llvm(),
            g.ty.zero()
        )
        .unwrap();
    }
    for (j, d) in m.data.iter().enumerate() {
        writeln!(
            out,
            "@data{j} = private constant [{} x i8] c\"{}\"\n@data.len{j} = internal global i64 {}",
            d.bytes.len(),
            d.bytes
                .iter()
                .map(|b| format!("\\{b:02X}"))
                .collect::<String>(),
            if d.passive { d.bytes.len() } else { 0 }
        )
        .unwrap();
    }
    for (j, d) in m.elems.iter().enumerate() {
        writeln!(
            out,
            "@elem{j} = private constant [{} x i32] [{}]\n@elem.len{j} = internal global i64 {}",
            d.funcs.len(),
            d.funcs
                .iter()
                .map(|f| format!("i32 {}", f + 1))
                .collect::<Vec<_>>()
                .join(", "),
            if d.passive && !d.declarative {
                d.funcs.len()
            } else {
                0
            }
        )
        .unwrap();
    }
    writeln!(
        out,
        "@func.ptrs = private constant [{} x ptr] [{}]",
        m.funcs.len(),
        (0..m.funcs.len())
            .map(|j| format!("ptr @f{j}"))
            .collect::<Vec<_>>()
            .join(", ")
    )
    .unwrap();
    writeln!(
        out,
        "@func.types = private constant [{} x i32] [{}]",
        m.funcs.len(),
        m.funcs
            .iter()
            .map(|f| format!("i32 {}", type_ids[f.type_idx as usize]))
            .collect::<Vec<_>>()
            .join(", ")
    )
    .unwrap();
    out.push_str("declare void @w2l_trap(i32) noreturn\ndeclare ptr @mmap(ptr, i64, i32, i32, i32, i64)\ndeclare ptr @calloc(i64, i64)\ndeclare void @llvm.memmove.p0.p0.i64(ptr, ptr, i64, i1)\ndeclare void @llvm.memset.p0.i64(ptr, i8, i64, i1)\n");
    out.push_str("define internal void @trap(i32 %code) noreturn {\n store i32 0, ptr @depth\n call void @w2l_trap(i32 %code)\n unreachable\n}\n");
    let max = m
        .memory
        .as_ref()
        .map(|l| l.max.unwrap_or(65536))
        .unwrap_or(0) as u64;
    writeln!(out,"define internal i32 @memory.grow(i32 %n) {{\n %old = load i64, ptr @mem.size\n %pages = lshr i64 %old, 16\n %n64 = zext i32 %n to i64\n %new = add i64 %pages, %n64\n %bad = icmp ugt i64 %new, {max}\n br i1 %bad, label %fail, label %ok\nfail:\n ret i32 -1\nok:\n %bytes = shl i64 %new, 16\n store i64 %bytes, ptr @mem.size\n %r = trunc i64 %pages to i32\n ret i32 %r\n}}").unwrap();
    for j in 0..m.funcs.len() {
        let (f, d) = native(m, j, type_ids)?;
        out.push_str(&f);
        decls.extend(d);
    }
    let mut init = Emitter::new(m, type_ids);
    init.line("store i32 0, ptr @depth");
    if let Some(mem) = &m.memory {
        let base =
            init.raw("call ptr @mmap(ptr null, i64 4294967296, i32 3, i32 16418, i32 -1, i64 0)");
        let fail = init.raw(format!("icmp eq ptr {base}, inttoptr (i64 -1 to ptr)"));
        init.check(&fail, 8);
        init.line(format!("store ptr {base}, ptr @mem.base"));
        init.line(format!(
            "store i64 {}, ptr @mem.size",
            (mem.min as u64) * 65536
        ));
    }
    if let Some(tab) = &m.table {
        let base = init.raw(format!("call ptr @calloc(i64 {}, i64 4)", tab.min.max(1)));
        let fail = init.raw(format!("icmp eq ptr {base}, null"));
        init.check(&fail, 8);
        init.line(format!("store ptr {base}, ptr @table.base"));
    }
    for (j, g) in m.globals.iter().enumerate() {
        let v = const_val(&g.init);
        init.line(format!("store {} {}, ptr @g{j}", g.ty.llvm(), v.v));
    }
    for (j, el) in m.elems.iter().enumerate() {
        if !el.passive && !el.declarative {
            let offset = const_val(&el.offset);
            let n = Val {
                ty: Ty::I32,
                v: el.funcs.len().to_string(),
            };
            let a = init.range(&offset, &n, &m.table.as_ref().unwrap().min.to_string(), 5);
            let b = init.raw("load ptr, ptr @table.base");
            let p = init.raw(format!("getelementptr i32, ptr {b}, i64 {a}"));
            init.line(format!(
                "call void @llvm.memmove.p0.p0.i64(ptr {p}, ptr @elem{j}, i64 {}, i1 false)",
                el.funcs.len() * 4
            ));
        }
    }
    for (j, d) in m.data.iter().enumerate() {
        if !d.passive {
            let offset = const_val(&d.offset);
            let p = init.mem_ptr(&offset, 0, d.bytes.len() as u64)?;
            init.line(format!(
                "call void @llvm.memmove.p0.p0.i64(ptr {p}, ptr @data{j}, i64 {}, i1 false)",
                d.bytes.len()
            ));
        }
    }
    if let Some(s) = m.start {
        init.line(format!("call void @f{s}()"));
    }
    init.line("ret void");
    writeln!(
        out,
        "define void @w2l_init() {{\nentry:\n{}{} }}",
        init.allocas, init.body
    )
    .unwrap();
    for x in &m.exports {
        let name = hex(&x.name);
        match x.kind {
            0 => {
                let t = &m.types[m.funcs[x.index as usize].type_idx as usize];
                let mut e = Emitter::new(m, type_ids);
                e.line("store i32 0, ptr @depth");
                for (j, ty) in t.params.iter().enumerate() {
                    let p = e.raw(format!("getelementptr i8, ptr %args, i64 {}", j * 16));
                    let v = e.value(*ty, format!("load {}, ptr {p}, align 1", ty.llvm()));
                    e.push(v);
                }
                e.call(t, &format!("@f{}", x.index))?;
                let vals = e.take(&t.results)?;
                for (j, v) in vals.iter().enumerate() {
                    let p = e.raw(format!("getelementptr i8, ptr %results, i64 {}", j * 16));
                    e.line(format!("store {} {}, ptr {p}, align 1", v.ty.llvm(), v.v));
                }
                e.line("ret void");
                writeln!(
                    out,
                    "define void @w2l_export_{name}(ptr %args, ptr %results) {{\nentry:\n{}{} }}",
                    e.allocas, e.body
                )
                .unwrap();
            }
            2 => {
                writeln!(out,"define ptr @w2l_memory_{name}(ptr %out) {{\n %n = load i64, ptr @mem.size\n store i64 %n, ptr %out\n %p = load ptr, ptr @mem.base\n ret ptr %p\n}}").unwrap();
            }
            3 => {
                let t = m.globals[x.index as usize].ty.llvm();
                writeln!(out,"define void @w2l_global_{name}(ptr %out) {{\n %v = load {t}, ptr @g{}\n store {t} %v, ptr %out, align 1\n ret void\n}}",x.index).unwrap();
            }
            _ => {}
        }
    }
    for d in decls {
        out.push_str(&d);
        out.push('\n');
    }
    Ok(out)
}

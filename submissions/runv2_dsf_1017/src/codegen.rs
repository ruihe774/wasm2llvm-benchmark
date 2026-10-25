// LLVM IR generation.
//
// Each wasm function becomes an internal LLVM function with a native
// signature.  The wasm operand stack is translated to SSA values; structured
// control flow uses basic blocks and phi nodes.  ABI entry points are thin
// wrappers over the internal functions.

use std::collections::BTreeMap;

use crate::instr::*;
use crate::module::*;
use crate::reader::hex_encode;

pub fn llvm_ty(t: ValType) -> &'static str {
    match t {
        ValType::I32 => "i32",
        ValType::I64 => "i64",
        ValType::F32 => "float",
        ValType::F64 => "double",
        ValType::V128 => "<16 x i8>",
        ValType::FuncRef | ValType::ExternRef => "ptr",
    }
}

fn slot_align(t: ValType) -> u32 {
    match t {
        ValType::I32 | ValType::F32 => 4,
        ValType::I64 | ValType::F64 => 8,
        ValType::V128 => 16,
        ValType::FuncRef | ValType::ExternRef => 8,
    }
}

fn ret_ty(results: &[ValType]) -> String {
    match results.len() {
        0 => "void".to_string(),
        1 => llvm_ty(results[0]).to_string(),
        _ => struct_ty(results),
    }
}

fn struct_ty(results: &[ValType]) -> String {
    let parts: Vec<&str> = results.iter().map(|t| llvm_ty(*t)).collect();
    format!("{{ {} }}", parts.join(", "))
}

fn param_list(params: &[ValType]) -> String {
    params
        .iter()
        .enumerate()
        .map(|(i, t)| format!("{} %p{}", llvm_ty(*t), i))
        .collect::<Vec<_>>()
        .join(", ")
}

pub struct Decls {
    map: BTreeMap<String, String>,
}

impl Decls {
    fn new() -> Self {
        Decls {
            map: BTreeMap::new(),
        }
    }
    pub fn reg(&mut self, key: &str, line: &str) {
        self.map.entry(key.to_string()).or_insert_with(|| line.to_string());
    }
    fn text(&self) -> String {
        let mut s = String::new();
        for line in self.map.values() {
            s.push_str(line);
            s.push('\n');
        }
        s
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ConstVal {
    I32(i32),
    I64(i64),
    F32(u32),
    F64(u64),
    V128([u8; 16]),
    Ref(Option<u32>),
}

impl ConstVal {
    pub fn llvm_text(&self) -> String {
        match self {
            ConstVal::I32(v) => format!("i32 {}", v),
            ConstVal::I64(v) => format!("i64 {}", v),
            ConstVal::F32(bits) => format!("float f0x{:08X}", bits),
            ConstVal::F64(bits) => format!("double f0x{:016X}", bits),
            ConstVal::V128(bytes) => format!("<16 x i8> {}", v128_const_text(bytes)),
            ConstVal::Ref(Some(f)) => format!("ptr @w2l_desc_{}", f),
            ConstVal::Ref(None) => "ptr null".to_string(),
        }
    }
    pub fn to_u32(&self) -> u32 {
        match self {
            ConstVal::I32(v) => *v as u32,
            _ => 0,
        }
    }
}

pub fn v128_const_text(bytes: &[u8; 16]) -> String {
    let parts: Vec<String> = bytes.iter().map(|b| format!("i8 {}", *b as i8)).collect();
    format!("<{}>", parts.join(", "))
}

/// Evaluate a validated constant expression.
pub fn eval_const(m: &Module, expr: &[Instr]) -> ConstVal {
    match expr.first() {
        Some(Instr::I32Const(v)) => ConstVal::I32(*v),
        Some(Instr::I64Const(v)) => ConstVal::I64(*v),
        Some(Instr::F32Const(v)) => ConstVal::F32(*v),
        Some(Instr::F64Const(v)) => ConstVal::F64(*v),
        Some(Instr::V128Const(v)) => ConstVal::V128(*v),
        Some(Instr::RefNull(_)) => ConstVal::Ref(None),
        Some(Instr::RefFunc(x)) => ConstVal::Ref(Some(*x)),
        Some(Instr::GlobalGet(x)) => {
            let gt = m.global_types[*x as usize];
            match gt.val {
                ValType::I32 => ConstVal::I32(0),
                ValType::I64 => ConstVal::I64(0),
                ValType::F32 => ConstVal::F32(0),
                ValType::F64 => ConstVal::F64(0),
                ValType::V128 => ConstVal::V128([0; 16]),
                ValType::FuncRef => ConstVal::Ref(None),
                ValType::ExternRef => ConstVal::Ref(None),
            }
        }
        _ => ConstVal::I32(0),
    }
}

const STACK_BUDGET: i64 = 6 * 1024 * 1024;
const DEPTH_LIMIT: i32 = 1_000_000;

// ---------------------------------------------------------------------------
// Static data emission
// ---------------------------------------------------------------------------

fn llvm_byte_string(bytes: &[u8]) -> String {
    let mut s = String::from("c\"");
    for &b in bytes {
        s.push_str(&format!("\\{:02X}", b));
    }
    s.push('"');
    s
}

pub fn canonical_types(m: &Module) -> Vec<u32> {
    let mut out: Vec<u32> = Vec::with_capacity(m.types.len());
    for i in 0..m.types.len() {
        let mut canon = i as u32;
        for j in 0..i {
            if m.types[j] == m.types[i] {
                canon = j as u32;
                break;
            }
        }
        out.push(canon);
    }
    out
}

fn emit_static(m: &Module) -> String {
    let mut s = String::new();
    s.push_str("@w2l_mem_base = internal global ptr null\n");
    s.push_str("@w2l_mem_size = internal global i64 0\n");
    s.push_str("@w2l_mem_pages = internal global i32 0\n");
    s.push_str("@w2l_depth = internal global i32 0\n");
    s.push_str("@w2l_stack_limit = internal global i64 0\n");

    for (i, _t) in m.table_types.iter().enumerate() {
        s.push_str(&format!("@w2l_tab_base_{} = internal global ptr null\n", i));
        s.push_str(&format!("@w2l_tab_size_{} = internal global i32 0\n", i));
    }

    for (i, gt) in m.global_types.iter().enumerate() {
        let init = if i < m.num_imported_globals {
            zero_const(gt.val)
        } else {
            let g = &m.globals[i - m.num_imported_globals];
            eval_const(m, &g.init)
        };
        s.push_str(&format!(
            "@w2l_glob_{} = internal global {}\n",
            i,
            init.llvm_text()
        ));
    }

    for (i, d) in m.datas.iter().enumerate() {
        let len = match &d.mode {
            DataMode::Passive => d.init.len() as u32,
            DataMode::Active { .. } => 0,
        };
        s.push_str(&format!(
            "@w2l_data_len_{} = internal global i32 {}\n",
            i, len
        ));
        if d.init.is_empty() {
            s.push_str(&format!(
                "@w2l_data_{} = private constant [0 x i8] zeroinitializer\n",
                i
            ));
        } else {
            s.push_str(&format!(
                "@w2l_data_{} = private constant [{} x i8] {}\n",
                i,
                d.init.len(),
                llvm_byte_string(&d.init)
            ));
        }
    }

    for (i, e) in m.elems.iter().enumerate() {
        let len = match &e.mode {
            ElemMode::Passive => e.init.len() as u32,
            _ => 0,
        };
        s.push_str(&format!(
            "@w2l_elem_len_{} = internal global i32 {}\n",
            i, len
        ));
        if e.init.is_empty() {
            s.push_str(&format!(
                "@w2l_elem_{} = private constant [0 x ptr] zeroinitializer\n",
                i
            ));
        } else {
            let mut vals = Vec::new();
            for ex in &e.init {
                let cv = eval_const(m, ex);
                vals.push(cv.llvm_text());
            }
            s.push_str(&format!(
                "@w2l_elem_{} = private constant [{} x ptr] [{}]\n",
                i,
                e.init.len(),
                vals.join(", ")
            ));
        }
    }

    let canon = canonical_types(m);
    for (idx, ti) in m.func_types.iter().enumerate() {
        s.push_str(&format!(
            "@w2l_desc_{} = private constant {{ i32, ptr }} {{ i32 {}, ptr @w2l_func_{} }}\n",
            idx,
            canon[*ti as usize],
            idx
        ));
    }

    s
}

fn zero_const(t: ValType) -> ConstVal {
    match t {
        ValType::I32 => ConstVal::I32(0),
        ValType::I64 => ConstVal::I64(0),
        ValType::F32 => ConstVal::F32(0),
        ValType::F64 => ConstVal::F64(0),
        ValType::V128 => ConstVal::V128([0; 16]),
        ValType::FuncRef | ValType::ExternRef => ConstVal::Ref(None),
    }
}

// ---------------------------------------------------------------------------
// Function generator
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct Val {
    pub ty: ValType,
    pub lty: String,
    pub s: String,
}

pub struct Block {
    pub label: String,
    pub phis: Vec<usize>,
    pub body: Vec<String>,
}

pub struct Phi {
    pub name: String,
    pub ty: String,
    pub incoming: Vec<(String, String)>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FKind {
    Function,
    Block,
    Loop,
    If,
    Else,
}

pub(crate) struct Frame {
    kind: FKind,
    end_label: String,
    header_label: Option<String>,
    header_phi_idx: Vec<usize>,
    param_types: Vec<ValType>,
    result_types: Vec<ValType>,
    base: usize,
    end_incoming: Vec<(Vec<Val>, String)>,
    else_label: Option<String>,
    else_block_idx: Option<usize>,
    params_vals: Vec<Val>,
    had_else: bool,
    reachable_at_entry: bool,
}

pub struct FuncGen<'a> {
    pub m: &'a Module,
    pub decls: &'a mut Decls,
    pub blocks: Vec<Block>,
    pub phis: Vec<Phi>,
    pub cur: usize,
    pub entry_allocas: Vec<String>,
    pub nvals: u32,
    pub nblocks: u32,
    pub reachable: bool,
    pub stack: Vec<Val>,
    pub(crate) ctrls: Vec<Frame>,
    pub locals: Vec<String>,
    pub local_types: Vec<ValType>,
    pub ret_types: Vec<ValType>,
    pub needs_ssse3: bool,
    pub type_canon: &'a [u32],
}

impl<'a> FuncGen<'a> {
    fn new(
        m: &'a Module,
        decls: &'a mut Decls,
        ret_types: Vec<ValType>,
        type_canon: &'a [u32],
    ) -> Self {
        let mut blocks = Vec::new();
        blocks.push(Block {
            label: "entry".to_string(),
            phis: vec![],
            body: vec![],
        });
        FuncGen {
            m,
            decls,
            blocks,
            phis: Vec::new(),
            cur: 0,
            entry_allocas: Vec::new(),
            nvals: 0,
            nblocks: 0,
            reachable: true,
            stack: Vec::new(),
            ctrls: Vec::new(),
            locals: Vec::new(),
            local_types: Vec::new(),
            ret_types,
            needs_ssse3: false,
            type_canon,
        }
    }

    pub fn fresh_reg(&mut self) -> String {
        let s = format!("%t{}", self.nvals);
        self.nvals += 1;
        s
    }

    pub fn fresh_label(&mut self) -> String {
        self.nblocks += 1;
        format!("B{}", self.nblocks)
    }

    pub fn emit(&mut self, line: String) {
        self.blocks[self.cur].body.push(line);
    }

    pub fn cur_label(&self) -> String {
        self.blocks[self.cur].label.clone()
    }

    pub fn new_reg(&mut self, expr: String) -> String {
        let name = self.fresh_reg();
        self.emit(format!("{} = {}", name, expr));
        name
    }

    pub fn val(&mut self, ty: ValType, expr: String) -> Val {
        let lty = llvm_ty(ty).to_string();
        let s = self.new_reg(expr);
        Val { ty, lty, s }
    }

    pub fn val_lty(&mut self, ty: ValType, lty: &str, expr: String) -> Val {
        let s = self.new_reg(expr);
        Val {
            ty,
            lty: lty.to_string(),
            s,
        }
    }

    pub fn bc(&mut self, v: &Val, lty: &str) -> Val {
        if v.lty == lty {
            return v.clone();
        }
        let s = self.new_reg(format!("bitcast {} {} to {}", v.lty, v.s, lty));
        Val {
            ty: v.ty,
            lty: lty.to_string(),
            s,
        }
    }

    pub fn opnd(v: &Val) -> String {
        format!("{} {}", v.lty, v.s)
    }

    pub fn iconst(&self, v: i32) -> Val {
        Val {
            ty: ValType::I32,
            lty: "i32".into(),
            s: format!("{}", v),
        }
    }
    pub fn lconst(&self, v: i64) -> Val {
        Val {
            ty: ValType::I64,
            lty: "i64".into(),
            s: format!("{}", v),
        }
    }
    pub fn fconst(&self, bits: u32) -> Val {
        Val {
            ty: ValType::F32,
            lty: "float".into(),
            s: format!("f0x{:08X}", bits),
        }
    }
    pub fn dconst(&self, bits: u64) -> Val {
        Val {
            ty: ValType::F64,
            lty: "double".into(),
            s: format!("f0x{:016X}", bits),
        }
    }

    pub fn push(&mut self, v: Val) {
        self.stack.push(v);
    }

    pub fn pop(&mut self) -> Val {
        self.stack.pop().expect("operand stack underflow in codegen")
    }

    pub fn pop_expect(&mut self, t: ValType) -> Val {
        let v = self.pop();
        debug_assert_eq!(v.ty, t);
        v
    }

    pub fn pop_n(&mut self, n: usize) -> Vec<Val> {
        let start = self.stack.len() - n;
        self.stack.split_off(start)
    }

    pub fn start_block(&mut self, label: &str) -> usize {
        let idx = self.blocks.len();
        self.blocks.push(Block {
            label: label.to_string(),
            phis: vec![],
            body: vec![],
        });
        self.cur = idx;
        idx
    }

    /// Get (creating on demand) a block that calls w2l_trap with `code`.
    pub fn trap_block(&mut self, code: i32) -> String {
        // We keep a small side table in the frame by scanning block labels.
        let label = format!("trap{}", code);
        if let Some(b) = self.blocks.iter().find(|b| b.label == label) {
            return b.label.clone();
        }
        let idx = self.blocks.len();
        self.blocks.push(Block {
            label: label.clone(),
            phis: vec![],
            body: vec![
                format!("call void @w2l_trap(i32 {})", code),
                "unreachable".to_string(),
            ],
        });
        let _ = idx;
        label
    }

    /// Branch to a trap block if `cond` (an i1 value name) is true,
    /// continuing in a fresh block.
    pub fn branch_trap(&mut self, cond: &str, code: i32) {
        let trap = self.trap_block(code);
        let cont = self.fresh_label();
        self.emit(format!(
            "br i1 {}, label %{}, label %{}",
            cond, trap, cont
        ));
        self.start_block(&cont);
    }

    pub fn trap_now(&mut self, code: i32) {
        let trap = self.trap_block(code);
        self.emit(format!("br label %{}", trap));
        self.reachable = false;
    }

    pub fn reg_decl(&mut self, key: &str, line: &str) -> String {
        self.decls.reg(key, line);
        format!("@{}", key)
    }

    fn emit_depth_dec(&mut self) {
        let d = self.new_reg("load i32, ptr @w2l_depth, align 4".to_string());
        let d1 = self.new_reg(format!("sub i32 {}, 1", d));
        self.emit(format!("store i32 {}, ptr @w2l_depth, align 4", d1));
    }

    pub fn emit_ret(&mut self, vals: &[Val]) {
        self.emit_depth_dec();
        match vals.len() {
            0 => self.emit("ret void".to_string()),
            1 => self.emit(format!("ret {} {}", vals[0].lty, vals[0].s)),
            _ => {
                let sty = struct_ty(&self.ret_types);
                let mut cur = self.new_reg(format!(
                    "insertvalue {} poison, {} {}, 0",
                    sty, vals[0].lty, vals[0].s
                ));
                for (i, v) in vals.iter().enumerate().skip(1) {
                    cur = self.new_reg(format!(
                        "insertvalue {} {}, {} {}, {}",
                        sty, cur, v.lty, v.s, i
                    ));
                }
                self.emit(format!("ret {} {}", sty, cur));
            }
        }
    }

    fn create_phis(&mut self, types: &[ValType], incoming: &[(Vec<Val>, String)]) -> Vec<Val> {
        let mut result = Vec::new();
        let block_idx = self.cur;
        for (i, t) in types.iter().enumerate() {
            let name = self.fresh_reg();
            let ty = llvm_ty(*t).to_string();
            let inc: Vec<(String, String)> = incoming
                .iter()
                .map(|(vals, pred)| (vals[i].s.clone(), pred.clone()))
                .collect();
            let pi = self.phis.len();
            self.phis.push(Phi {
                name: name.clone(),
                ty: ty.clone(),
                incoming: inc,
            });
            self.blocks[block_idx].phis.push(pi);
            result.push(Val {
                ty: *t,
                lty: ty,
                s: name,
            });
        }
        result
    }

    fn blocktype(&self, bt: &BlockType) -> (Vec<ValType>, Vec<ValType>) {
        match bt {
            BlockType::Empty => (vec![], vec![]),
            BlockType::Val(t) => (vec![], vec![*t]),
            BlockType::TypeIdx(i) => {
                let ft = &self.m.types[*i as usize];
                (ft.params.clone(), ft.results.clone())
            }
        }
    }

    fn branch_to_frame(&mut self, depth: u32, vals: Vec<Val>, pred: &str) {
        let n = self.ctrls.len();
        let fi = n - 1 - depth as usize;
        let kind = self.ctrls[fi].kind;
        match kind {
            FKind::Loop => {
                let target = self.ctrls[fi].header_label.clone().unwrap();
                let idxs = self.ctrls[fi].header_phi_idx.clone();
                for (i, v) in vals.iter().enumerate() {
                    self.phis[idxs[i]].incoming.push((v.s.clone(), pred.to_string()));
                }
                self.emit(format!("br label %{}", target));
            }
            _ => {
                let target = self.ctrls[fi].end_label.clone();
                self.ctrls[fi].end_incoming.push((vals, pred.to_string()));
                self.emit(format!("br label %{}", target));
            }
        }
    }

    fn frame_target_label(&self, depth: u32) -> String {
        let n = self.ctrls.len();
        let fi = n - 1 - depth as usize;
        if self.ctrls[fi].kind == FKind::Loop {
            self.ctrls[fi].header_label.clone().unwrap()
        } else {
            self.ctrls[fi].end_label.clone()
        }
    }

    fn gen_control(&mut self, instr: &Instr) {
        match instr {
            Instr::Unreachable => {
                self.trap_now(0);
            }
            Instr::Nop => {}
            Instr::Block(bt) => {
                let (params, results) = self.blocktype(bt);
                if self.reachable {
                    let pv = self.pop_n(params.len());
                    let base = self.stack.len();
                    let end_label = self.fresh_label();
                    let entry_label = self.cur_label();
                    for v in &pv {
                        self.stack.push(v.clone());
                    }
                    self.ctrls.push(Frame {
                        kind: FKind::Block,
                        end_label,
                        header_label: None,
                        header_phi_idx: vec![],
                        param_types: params,
                        result_types: results,
                        base,
                        end_incoming: vec![],
                        else_label: None,
                        else_block_idx: None,
                        params_vals: pv,
                        had_else: false,
                        reachable_at_entry: true,
                    });
                    let _ = entry_label;
                } else {
                    let base = self.stack.len();
                    let end_label_var = self.fresh_label();
                    self.ctrls.push(Frame {
                        kind: FKind::Block,
                        end_label: end_label_var,
                        header_label: None,
                        header_phi_idx: vec![],
                        param_types: params,
                        result_types: results,
                        base,
                        end_incoming: vec![],
                        else_label: None,
                        else_block_idx: None,
                        params_vals: vec![],
                        had_else: false,
                        reachable_at_entry: false,
                    });
                }
            }
            Instr::Loop(bt) => {
                let (params, results) = self.blocktype(bt);
                if self.reachable {
                    let pv = self.pop_n(params.len());
                    let base = self.stack.len();
                    let header = self.fresh_label();
                    let pred = self.cur_label();
                    self.emit(format!("br label %{}", header));
                    let hidx = self.start_block(&header);
                    // create header phis
                    let mut phi_vals = Vec::new();
                    let mut phi_idx = Vec::new();
                    for (i, t) in params.iter().enumerate() {
                        let name = self.fresh_reg();
                        let pi = self.phis.len();
                        self.phis.push(Phi {
                            name: name.clone(),
                            ty: llvm_ty(*t).to_string(),
                            incoming: vec![(pv[i].s.clone(), pred.clone())],
                        });
                        self.blocks[hidx].phis.push(pi);
                        phi_idx.push(pi);
                        phi_vals.push(Val {
                            ty: *t,
                            lty: llvm_ty(*t).to_string(),
                            s: name,
                        });
                    }
                    for v in &phi_vals {
                        self.stack.push(v.clone());
                    }
                    let end_label_var = self.fresh_label();
                    self.ctrls.push(Frame {
                        kind: FKind::Loop,
                        end_label: end_label_var,
                        header_label: Some(header),
                        header_phi_idx: phi_idx,
                        param_types: params,
                        result_types: results,
                        base,
                        end_incoming: vec![],
                        else_label: None,
                        else_block_idx: None,
                        params_vals: pv,
                        had_else: false,
                        reachable_at_entry: true,
                    });
                } else {
                    let base = self.stack.len();
                    let end_label_var = self.fresh_label();
                    self.ctrls.push(Frame {
                        kind: FKind::Loop,
                        end_label: end_label_var,
                        header_label: None,
                        header_phi_idx: vec![],
                        param_types: params,
                        result_types: results,
                        base,
                        end_incoming: vec![],
                        else_label: None,
                        else_block_idx: None,
                        params_vals: vec![],
                        had_else: false,
                        reachable_at_entry: false,
                    });
                }
            }
            Instr::If(bt) => {
                let (params, results) = self.blocktype(bt);
                if self.reachable {
                    let cond = self.pop_expect(ValType::I32);
                    let pv = self.pop_n(params.len());
                    let base = self.stack.len();
                    let then_label = self.fresh_label();
                    let else_label = self.fresh_label();
                    let end_label = self.fresh_label();
                    let entry_label = self.cur_label();
                    let cbool = self.new_reg(format!("icmp ne i32 {}, 0", cond.s));
                    self.emit(format!(
                        "br i1 {}, label %{}, label %{}",
                        cbool, then_label, else_label
                    ));
                    self.start_block(&then_label);
                    let else_idx = self.blocks.len();
                    self.blocks.push(Block {
                        label: else_label.clone(),
                        phis: vec![],
                        body: vec![],
                    });
                    for v in &pv {
                        self.stack.push(v.clone());
                    }
                    self.ctrls.push(Frame {
                        kind: FKind::If,
                        end_label,
                        header_label: None,
                        header_phi_idx: vec![],
                        param_types: params,
                        result_types: results,
                        base,
                        end_incoming: vec![],
                        else_label: Some(else_label),
                        else_block_idx: Some(else_idx),
                        params_vals: pv,
                        had_else: false,
                        reachable_at_entry: true,
                    });
                    let _ = entry_label;
                } else {
                    // dead if
                    let base = self.stack.len();
                    let end_label_var = self.fresh_label();
                    self.ctrls.push(Frame {
                        kind: FKind::If,
                        end_label: end_label_var,
                        header_label: None,
                        header_phi_idx: vec![],
                        param_types: params,
                        result_types: results,
                        base,
                        end_incoming: vec![],
                        else_label: None,
                        else_block_idx: None,
                        params_vals: vec![],
                        had_else: false,
                        reachable_at_entry: false,
                    });
                }
            }
            Instr::Else => {
                let f = self.ctrls.pop().expect("else without frame");
                debug_assert_eq!(f.kind, FKind::If);
                let mut incoming = f.end_incoming;
                if self.reachable {
                    let vals = self.pop_n(f.result_types.len());
                    incoming.push((vals, self.cur_label()));
                    self.emit(format!("br label %{}", f.end_label));
                }
                if f.reachable_at_entry {
                    self.cur = f.else_block_idx.unwrap();
                    self.stack.truncate(f.base);
                    for v in &f.params_vals {
                        self.stack.push(v.clone());
                    }
                    self.reachable = true;
                } else {
                    self.reachable = false;
                    self.stack.truncate(f.base);
                }
                self.ctrls.push(Frame {
                    kind: FKind::Else,
                    end_label: f.end_label,
                    header_label: None,
                    header_phi_idx: vec![],
                    param_types: f.param_types,
                    result_types: f.result_types,
                    base: f.base,
                    end_incoming: incoming,
                    else_label: f.else_label,
                    else_block_idx: f.else_block_idx,
                    params_vals: f.params_vals,
                    had_else: true,
                    reachable_at_entry: f.reachable_at_entry,
                });
            }
            Instr::End => {
                let f = self.ctrls.pop().expect("end without frame");
                match f.kind {
                    FKind::Function => {
                        let mut incoming = f.end_incoming;
                        if self.reachable {
                            let vals = self.pop_n(f.result_types.len());
                            incoming.push((vals, self.cur_label()));
                            self.emit(format!("br label %{}", f.end_label));
                            self.reachable = false;
                        }
                        if !incoming.is_empty() {
                            self.start_block(&f.end_label);
                            let vals = if f.result_types.is_empty() {
                                vec![]
                            } else {
                                self.create_phis(&f.result_types, &incoming)
                            };
                            self.emit_ret(&vals);
                            self.reachable = false;
                        }
                    }
                    FKind::Block => {
                        let mut incoming = f.end_incoming;
                        let mut keep = false;
                        if self.reachable {
                            if incoming.is_empty() {
                                keep = true;
                            } else {
                                let vals = self.pop_n(f.result_types.len());
                                incoming.push((vals, self.cur_label()));
                                self.emit(format!("br label %{}", f.end_label));
                                self.reachable = false;
                            }
                        }
                        if !keep {
                            if !incoming.is_empty() {
                                self.start_block(&f.end_label);
                                let vals = if f.result_types.is_empty() {
                                    vec![]
                                } else {
                                    self.create_phis(&f.result_types, &incoming)
                                };
                                self.reachable = true;
                                self.stack.truncate(f.base);
                                for v in vals {
                                    self.stack.push(v);
                                }
                            } else {
                                self.reachable = false;
                                self.stack.truncate(f.base);
                            }
                        }
                    }
                    FKind::If | FKind::Else => {
                        let mut incoming = f.end_incoming;
                        if self.reachable {
                            let vals = self.pop_n(f.result_types.len());
                            incoming.push((vals, self.cur_label()));
                            self.emit(format!("br label %{}", f.end_label));
                            self.reachable = false;
                        }
                        if f.kind == FKind::If && !f.had_else && f.reachable_at_entry {
                            // Terminate the implicit else block.
                            let ei = f.else_block_idx.unwrap();
                            self.blocks[ei]
                                .body
                                .push(format!("br label %{}", f.end_label));
                            incoming.push((f.params_vals.clone(), f.else_label.clone().unwrap()));
                        }
                        if !incoming.is_empty() {
                            self.start_block(&f.end_label);
                            let vals = if f.result_types.is_empty() {
                                vec![]
                            } else {
                                self.create_phis(&f.result_types, &incoming)
                            };
                            self.reachable = true;
                            self.stack.truncate(f.base);
                            for v in vals {
                                self.stack.push(v);
                            }
                        } else {
                            self.reachable = false;
                            self.stack.truncate(f.base);
                        }
                    }
                    FKind::Loop => {
                        if !self.reachable {
                            self.stack.truncate(f.base);
                        }
                    }
                }
            }
            Instr::Br(d) => {
                if self.reachable {
                    let types = self.branch_types(*d);
                    let vals = self.pop_n(types.len());
                    let pred = self.cur_label();
                    self.branch_to_frame(*d, vals, &pred);
                    self.reachable = false;
                }
            }
            Instr::BrIf(d) => {
                if self.reachable {
                    let cond = self.pop_expect(ValType::I32);
                    let types = self.branch_types(*d);
                    let vals: Vec<Val> = self.stack[self.stack.len() - types.len()..].to_vec();
                    let pred = self.cur_label();
                    let target = self.frame_target_label(*d);
                    let cont = self.fresh_label();
                    let cbool = self.new_reg(format!("icmp ne i32 {}, 0", cond.s));
                    self.emit(format!(
                        "br i1 {}, label %{}, label %{}",
                        cbool, target, cont
                    ));
                    // record phi incoming for the taken edge
                    let n = self.ctrls.len();
                    let fi = n - 1 - *d as usize;
                    if self.ctrls[fi].kind == FKind::Loop {
                        let idxs = self.ctrls[fi].header_phi_idx.clone();
                        for (i, v) in vals.iter().enumerate() {
                            self.phis[idxs[i]].incoming.push((v.s.clone(), pred.clone()));
                        }
                    } else {
                        self.ctrls[fi].end_incoming.push((vals, pred.clone()));
                    }
                    self.start_block(&cont);
                }
            }
            Instr::BrTable(targets, default) => {
                if self.reachable {
                    let idx = self.pop_expect(ValType::I32);
                    // All targets have the same arity/types (validated).
                    let dtypes = self.branch_types(*default);
                    let vals: Vec<Val> = self.stack[self.stack.len() - dtypes.len()..].to_vec();
                    // Create a trampoline per target edge.
                    let mut case_labels: Vec<String> = Vec::new();
                    let all: Vec<u32> = targets.iter().cloned().chain(std::iter::once(*default)).collect();
                    for (k, t) in all.iter().enumerate() {
                        let tl = self.fresh_label();
                        let target = self.frame_target_label(*t);
                        let n = self.ctrls.len();
                        let fi = n - 1 - *t as usize;
                        if self.ctrls[fi].kind == FKind::Loop {
                            let idxs = self.ctrls[fi].header_phi_idx.clone();
                            for (i, v) in vals.iter().enumerate() {
                                self.phis[idxs[i]].incoming.push((v.s.clone(), tl.clone()));
                            }
                        } else {
                            self.ctrls[fi].end_incoming.push((vals.clone(), tl.clone()));
                        }
                        // The trampoline jumps to the target label.
                        self.blocks.push(Block {
                            label: tl.clone(),
                            phis: vec![],
                            body: vec![format!("br label %{}", target)],
                        });
                        if k < targets.len() {
                            case_labels.push(tl);
                        } else {
                            case_labels.push(tl); // last is default
                        }
                    }
                    let default_label = case_labels.last().unwrap().clone();
                    let mut cases = String::new();
                    for (i, tl) in case_labels.iter().take(targets.len()).enumerate() {
                        cases.push_str(&format!("i32 {}, label %{} ", i, tl));
                    }
                    self.emit(format!(
                        "switch i32 {}, label %{} [{}]",
                        idx.s, default_label, cases
                    ));
                    self.reachable = false;
                    self.stack.truncate(self.stack.len() - dtypes.len());
                }
            }
            Instr::Return => {
                if self.reachable {
                    let rt = self.ret_types.clone();
                    let vals = self.pop_n(rt.len());
                    self.emit_ret(&vals);
                    self.reachable = false;
                }
            }
            _ => unreachable!(),
        }
    }

    fn branch_types(&self, depth: u32) -> Vec<ValType> {
        let n = self.ctrls.len();
        let f = &self.ctrls[n - 1 - depth as usize];
        if f.kind == FKind::Loop {
            f.param_types.clone()
        } else {
            f.result_types.clone()
        }
    }

    // ------------------- non-control instruction codegen -------------------

    fn gen_simple(&mut self, instr: &Instr) {
        if !self.reachable {
            return;
        }
        use ValType::*;
        match instr {
            Instr::Drop => {
                self.pop();
            }
            Instr::Select | Instr::SelectT(_) => {
                let c = self.pop_expect(I32);
                let cbool = self.new_reg(format!("icmp ne i32 {}, 0", c.s));
                let b = self.pop();
                let a = self.pop();
                let r = self.val_lty(
                    a.ty,
                    &a.lty.clone(),
                    format!(
                        "select i1 {}, {} {}, {} {}",
                        cbool, a.lty, a.s, b.lty, b.s
                    ),
                );
                self.push(r);
            }
            Instr::LocalGet(x) => {
                let t = self.local_types[*x as usize];
                let r = self.val(
                    t,
                    format!(
                        "load {}, ptr {}, align {}",
                        llvm_ty(t),
                        self.locals[*x as usize],
                        slot_align(t)
                    ),
                );
                self.push(r);
            }
            Instr::LocalSet(x) => {
                let v = self.pop();
                let t = self.local_types[*x as usize];
                self.emit(format!(
                    "store {} {}, ptr {}, align {}",
                    llvm_ty(t),
                    v.s,
                    self.locals[*x as usize],
                    slot_align(t)
                ));
            }
            Instr::LocalTee(x) => {
                let v = self.pop();
                let t = self.local_types[*x as usize];
                self.emit(format!(
                    "store {} {}, ptr {}, align {}",
                    llvm_ty(t),
                    v.s,
                    self.locals[*x as usize],
                    slot_align(t)
                ));
                self.push(v);
            }
            Instr::GlobalGet(x) => {
                let t = self.m.global_types[*x as usize].val;
                let r = self.val(
                    t,
                    format!("load {}, ptr @w2l_glob_{}, align {}", llvm_ty(t), x, slot_align(t)),
                );
                self.push(r);
            }
            Instr::GlobalSet(x) => {
                let v = self.pop();
                let t = self.m.global_types[*x as usize].val;
                self.emit(format!(
                    "store {} {}, ptr @w2l_glob_{}, align {}",
                    llvm_ty(t),
                    v.s,
                    x,
                    slot_align(t)
                ));
            }
            Instr::I32Const(v) => self.push(self.iconst(*v)),
            Instr::I64Const(v) => self.push(self.lconst(*v)),
            Instr::F32Const(v) => self.push(self.fconst(*v)),
            Instr::F64Const(v) => self.push(self.dconst(*v)),
            Instr::V128Const(bytes) => {
                let v = Val {
                    ty: ValType::V128,
                    lty: "<16 x i8>".into(),
                    s: v128_const_text(bytes),
                };
                self.push(v);
            }
            Instr::RefNull(t) => {
                let v = Val {
                    ty: *t,
                    lty: "ptr".into(),
                    s: "null".into(),
                };
                self.push(v);
            }
            Instr::RefIsNull => {
                let v = self.pop();
                let c = self.new_reg(format!("icmp eq ptr {}, null", v.s));
                let r = self.val(I32, format!("zext i1 {} to i32", c));
                self.push(r);
            }
            Instr::RefFunc(f) => {
                let v = Val {
                    ty: FuncRef,
                    lty: "ptr".into(),
                    s: format!("@w2l_desc_{}", f),
                };
                self.push(v);
            }
            Instr::Call(f) => {
                let ti = self.m.func_types[*f as usize] as usize;
                let ft = self.m.types[ti].clone();
                self.gen_call(&format!("@w2l_func_{}", f), &ft);
            }
            Instr::CallIndirect(ty, table) => {
                let ft = self.m.types[*ty as usize].clone();
                let idx = self.pop_expect(I32);
                let tsize = self.new_reg(format!(
                    "load i32, ptr @w2l_tab_size_{}, align 4",
                    table
                ));
                let oob = self.new_reg(format!("icmp uge i32 {}, {}", idx.s, tsize));
                self.branch_trap(&oob, 5);
                let tbase = self.new_reg(format!(
                    "load ptr, ptr @w2l_tab_base_{}, align 8",
                    table
                ));
                let idxz = self.new_reg(format!("zext i32 {} to i64", idx.s));
                let slot = self.new_reg(format!(
                    "getelementptr ptr, ptr {}, i64 {}",
                    tbase, idxz
                ));
                let rf = self.new_reg(format!("load ptr, ptr {}, align 8", slot));
                let isnull = self.new_reg(format!("icmp eq ptr {}, null", rf));
                self.branch_trap(&isnull, 6);
                let tid = self.new_reg(format!("load i32, ptr {}, align 4", rf));
                let want = self.type_canon[*ty as usize];
                let mismatch = self.new_reg(format!("icmp ne i32 {}, {}", tid, want));
                self.branch_trap(&mismatch, 7);
                let fptr = self.new_reg(format!(
                    "getelementptr {{ i32, ptr }}, ptr {}, i32 0, i32 1",
                    rf
                ));
                let fnv = self.new_reg(format!("load ptr, ptr {}, align 8", fptr));
                self.gen_call_through(&fnv, &ft);
            }
            Instr::TableGet(t) => {
                let idx = self.pop_expect(I32);
                let tsize = self.new_reg(format!("load i32, ptr @w2l_tab_size_{}, align 4", t));
                let oob = self.new_reg(format!("icmp uge i32 {}, {}", idx.s, tsize));
                self.branch_trap(&oob, 5);
                let tbase = self.new_reg(format!("load ptr, ptr @w2l_tab_base_{}, align 8", t));
                let idxz = self.new_reg(format!("zext i32 {} to i64", idx.s));
                let slot = self.new_reg(format!("getelementptr ptr, ptr {}, i64 {}", tbase, idxz));
                let v = self.val(
                    self.m.table_types[*t as usize].elem,
                    format!("load ptr, ptr {}, align 8", slot),
                );
                self.push(v);
            }
            Instr::TableSet(t) => {
                let val = self.pop();
                let idx = self.pop_expect(I32);
                let tsize = self.new_reg(format!("load i32, ptr @w2l_tab_size_{}, align 4", t));
                let oob = self.new_reg(format!("icmp uge i32 {}, {}", idx.s, tsize));
                self.branch_trap(&oob, 5);
                let tbase = self.new_reg(format!("load ptr, ptr @w2l_tab_base_{}, align 8", t));
                let idxz = self.new_reg(format!("zext i32 {} to i64", idx.s));
                let slot = self.new_reg(format!("getelementptr ptr, ptr {}, i64 {}", tbase, idxz));
                self.emit(format!("store ptr {}, ptr {}, align 8", val.s, slot));
            }
            Instr::TableSize(t) => {
                let r = self.val(
                    I32,
                    format!("load i32, ptr @w2l_tab_size_{}, align 4", t),
                );
                self.push(r);
            }
            Instr::TableGrow(t) => {
                let delta = self.pop_expect(I32);
                let init = self.pop();
                let r = self.val(
                    I32,
                    format!("call i32 @w2l_tab_grow_{}(ptr {}, i32 {})", t, init.s, delta.s),
                );
                self.push(r);
            }
            Instr::TableFill(t) => {
                let n = self.pop_expect(I32);
                let v = self.pop();
                let dst = self.pop_expect(I32);
                let tsize = self.new_reg(format!("load i32, ptr @w2l_tab_size_{}, align 4", t));
                let d64 = self.new_reg(format!("zext i32 {} to i64", dst.s));
                let n64 = self.new_reg(format!("zext i32 {} to i64", n.s));
                let end = self.new_reg(format!("add i64 {}, {}", d64, n64));
                let sz64 = self.new_reg(format!("zext i32 {} to i64", tsize));
                let oob = self.new_reg(format!("icmp ugt i64 {}, {}", end, sz64));
                self.branch_trap(&oob, 5);
                self.emit(format!(
                    "call void @w2l_tab_fill_{}(i32 {}, ptr {}, i32 {})",
                    t, dst.s, v.s, n.s
                ));
            }
            Instr::TableInit(e, t) => {
                let n = self.pop_expect(I32);
                let src = self.pop_expect(I32);
                let dst = self.pop_expect(I32);
                let elen = self.new_reg(format!(
                    "load i32, ptr @w2l_elem_len_{}, align 4",
                    e
                ));
                let s64 = self.new_reg(format!("zext i32 {} to i64", src.s));
                let n64 = self.new_reg(format!("zext i32 {} to i64", n.s));
                let send = self.new_reg(format!("add i64 {}, {}", s64, n64));
                let el64 = self.new_reg(format!("zext i32 {} to i64", elen));
                let s_oob = self.new_reg(format!("icmp ugt i64 {}, {}", send, el64));
                let tsize = self.new_reg(format!("load i32, ptr @w2l_tab_size_{}, align 4", t));
                let d64 = self.new_reg(format!("zext i32 {} to i64", dst.s));
                let dend = self.new_reg(format!("add i64 {}, {}", d64, n64));
                let tsz64 = self.new_reg(format!("zext i32 {} to i64", tsize));
                let d_oob = self.new_reg(format!("icmp ugt i64 {}, {}", dend, tsz64));
                let oob = self.new_reg(format!("or i1 {}, {}", s_oob, d_oob));
                self.branch_trap(&oob, 5);
                let tbase = self.new_reg(format!("load ptr, ptr @w2l_tab_base_{}, align 8", t));
                let dp = self.new_reg(format!(
                    "getelementptr ptr, ptr {}, i64 {}",
                    tbase, d64
                ));
                let sp = self.new_reg(format!(
                    "getelementptr ptr, ptr @w2l_elem_{}, i64 {}",
                    e, s64
                ));
                let bytes = self.new_reg(format!("mul i64 {}, 8", n64));
                self.reg_decl("memcpy", "declare ptr @memcpy(ptr, ptr, i64)");
                self.emit(format!(
                    "call ptr @memcpy(ptr {}, ptr {}, i64 {})",
                    dp, sp, bytes
                ));
            }
            Instr::ElemDrop(e) => {
                self.emit(format!(
                    "store i32 0, ptr @w2l_elem_len_{}, align 4",
                    e
                ));
            }
            Instr::TableCopy(d, s) => {
                let n = self.pop_expect(I32);
                let src = self.pop_expect(I32);
                let dst = self.pop_expect(I32);
                let dsize = self.new_reg(format!("load i32, ptr @w2l_tab_size_{}, align 4", d));
                let ssize = self.new_reg(format!("load i32, ptr @w2l_tab_size_{}, align 4", s));
                let d64 = self.new_reg(format!("zext i32 {} to i64", dst.s));
                let s64 = self.new_reg(format!("zext i32 {} to i64", src.s));
                let n64 = self.new_reg(format!("zext i32 {} to i64", n.s));
                let dend = self.new_reg(format!("add i64 {}, {}", d64, n64));
                let send = self.new_reg(format!("add i64 {}, {}", s64, n64));
                let dsz = self.new_reg(format!("zext i32 {} to i64", dsize));
                let ssz = self.new_reg(format!("zext i32 {} to i64", ssize));
                let o1 = self.new_reg(format!("icmp ugt i64 {}, {}", dend, dsz));
                let o2 = self.new_reg(format!("icmp ugt i64 {}, {}", send, ssz));
                let oob = self.new_reg(format!("or i1 {}, {}", o1, o2));
                self.branch_trap(&oob, 5);
                let db = self.new_reg(format!("load ptr, ptr @w2l_tab_base_{}, align 8", d));
                let sb = self.new_reg(format!("load ptr, ptr @w2l_tab_base_{}, align 8", s));
                let dp = self.new_reg(format!("getelementptr ptr, ptr {}, i64 {}", db, d64));
                let sp = self.new_reg(format!("getelementptr ptr, ptr {}, i64 {}", sb, s64));
                let bytes = self.new_reg(format!("mul i64 {}, 8", n64));
                self.reg_decl("memmove", "declare ptr @memmove(ptr, ptr, i64)");
                self.emit(format!(
                    "call ptr @memmove(ptr {}, ptr {}, i64 {})",
                    dp, sp, bytes
                ));
            }
            Instr::MemorySize => {
                let r = self.val(
                    I32,
                    "load i32, ptr @w2l_mem_pages, align 4".to_string(),
                );
                self.push(r);
            }
            Instr::MemoryGrow => {
                let d = self.pop_expect(I32);
                let r = self.val(I32, format!("call i32 @w2l_mem_grow(i32 {})", d.s));
                self.push(r);
            }
            Instr::Load(kind, ma) => {
                let addr = self.pop_expect(I32);
                let p = self.mem_ptr(&addr, ma.offset, kind.bytes());
                let raw_lty = llvm_ty(kind.result());
                let raw = match kind {
                    LoadKind::I32 | LoadKind::I64 | LoadKind::F32 | LoadKind::F64 => {
                        let t = kind.result();
                        self.val(t, format!("load {}, ptr {}, align 1", llvm_ty(t), p.s))
                    }
                    LoadKind::I32_8S | LoadKind::I32_8U | LoadKind::I64_8S | LoadKind::I64_8U => {
                        let b = self.new_reg(format!("load i8, ptr {}, align 1", p.s));
                        let op = match kind {
                            LoadKind::I32_8S | LoadKind::I64_8S => "sext",
                            _ => "zext",
                        };
                        self.val(
                            kind.result(),
                            format!("{} i8 {} to {}", op, b, raw_lty),
                        )
                    }
                    LoadKind::I32_16S | LoadKind::I32_16U | LoadKind::I64_16S | LoadKind::I64_16U => {
                        let b = self.new_reg(format!("load i16, ptr {}, align 1", p.s));
                        let op = match kind {
                            LoadKind::I32_16S | LoadKind::I64_16S => "sext",
                            _ => "zext",
                        };
                        self.val(
                            kind.result(),
                            format!("{} i16 {} to {}", op, b, raw_lty),
                        )
                    }
                    LoadKind::I64_32S | LoadKind::I64_32U => {
                        let b = self.new_reg(format!("load i32, ptr {}, align 1", p.s));
                        let op = if matches!(kind, LoadKind::I64_32S) {
                            "sext"
                        } else {
                            "zext"
                        };
                        self.val(kind.result(), format!("{} i32 {} to i64", op, b))
                    }
                };
                self.push(raw);
            }
            Instr::Store(kind, ma) => {
                let v = self.pop();
                let addr = self.pop_expect(I32);
                let p = self.mem_ptr(&addr, ma.offset, kind.bytes());
                let vt = kind.value_type();
                match kind {
                    StoreKind::I32 | StoreKind::I64 | StoreKind::F32 | StoreKind::F64 => {
                        self.emit(format!(
                            "store {} {}, ptr {}, align 1",
                            llvm_ty(vt),
                            v.s,
                            p.s
                        ));
                    }
                    StoreKind::I32_8 | StoreKind::I64_8 => {
                        let t = self.new_reg(format!("trunc {} {} to i8", llvm_ty(vt), v.s));
                        self.emit(format!("store i8 {}, ptr {}, align 1", t, p.s));
                    }
                    StoreKind::I32_16 | StoreKind::I64_16 => {
                        let t = self.new_reg(format!("trunc {} {} to i16", llvm_ty(vt), v.s));
                        self.emit(format!("store i16 {}, ptr {}, align 1", t, p.s));
                    }
                    StoreKind::I64_32 => {
                        let t = self.new_reg(format!("trunc i64 {} to i32", v.s));
                        self.emit(format!("store i32 {}, ptr {}, align 1", t, p.s));
                    }
                }
            }
            Instr::MemoryInit(d) => {
                let n = self.pop_expect(I32);
                let src = self.pop_expect(I32);
                let dst = self.pop_expect(I32);
                let dlen = self.new_reg(format!(
                    "load i32, ptr @w2l_data_len_{}, align 4",
                    d
                ));
                let msize = self.new_reg("load i64, ptr @w2l_mem_size, align 8".to_string());
                let s64 = self.new_reg(format!("zext i32 {} to i64", src.s));
                let n64 = self.new_reg(format!("zext i32 {} to i64", n.s));
                let d64 = self.new_reg(format!("zext i32 {} to i64", dst.s));
                let send = self.new_reg(format!("add i64 {}, {}", s64, n64));
                let dend = self.new_reg(format!("add i64 {}, {}", d64, n64));
                let dl = self.new_reg(format!("zext i32 {} to i64", dlen));
                let o1 = self.new_reg(format!("icmp ugt i64 {}, {}", send, dl));
                let o2 = self.new_reg(format!("icmp ugt i64 {}, {}", dend, msize));
                let oob = self.new_reg(format!("or i1 {}, {}", o1, o2));
                self.branch_trap(&oob, 4);
                let mbase = self.new_reg(
                    "load ptr, ptr @w2l_mem_base, align 8".to_string(),
                );
                let dp = self.new_reg(format!(
                    "getelementptr i8, ptr {}, i64 {}",
                    mbase, d64
                ));
                let sp = self.new_reg(format!(
                    "getelementptr i8, ptr @w2l_data_{}, i64 {}",
                    d, s64
                ));
                self.reg_decl("memcpy", "declare ptr @memcpy(ptr, ptr, i64)");
                self.emit(format!(
                    "call ptr @memcpy(ptr {}, ptr {}, i64 {})",
                    dp, sp, n64
                ));
            }
            Instr::DataDrop(d) => {
                self.emit(format!(
                    "store i32 0, ptr @w2l_data_len_{}, align 4",
                    d
                ));
            }
            Instr::MemoryCopy => {
                let n = self.pop_expect(I32);
                let src = self.pop_expect(I32);
                let dst = self.pop_expect(I32);
                let msize = self.new_reg("load i64, ptr @w2l_mem_size, align 8".to_string());
                let s64 = self.new_reg(format!("zext i32 {} to i64", src.s));
                let d64 = self.new_reg(format!("zext i32 {} to i64", dst.s));
                let n64 = self.new_reg(format!("zext i32 {} to i64", n.s));
                let send = self.new_reg(format!("add i64 {}, {}", s64, n64));
                let dend = self.new_reg(format!("add i64 {}, {}", d64, n64));
                let o1 = self.new_reg(format!("icmp ugt i64 {}, {}", send, msize));
                let o2 = self.new_reg(format!("icmp ugt i64 {}, {}", dend, msize));
                let oob = self.new_reg(format!("or i1 {}, {}", o1, o2));
                self.branch_trap(&oob, 4);
                let mbase = self.new_reg("load ptr, ptr @w2l_mem_base, align 8".to_string());
                let dp = self.new_reg(format!("getelementptr i8, ptr {}, i64 {}", mbase, d64));
                let sp = self.new_reg(format!("getelementptr i8, ptr {}, i64 {}", mbase, s64));
                self.reg_decl("memmove", "declare ptr @memmove(ptr, ptr, i64)");
                self.emit(format!(
                    "call ptr @memmove(ptr {}, ptr {}, i64 {})",
                    dp, sp, n64
                ));
            }
            Instr::MemoryFill => {
                let n = self.pop_expect(I32);
                let val = self.pop_expect(I32);
                let dst = self.pop_expect(I32);
                let msize = self.new_reg("load i64, ptr @w2l_mem_size, align 8".to_string());
                let d64 = self.new_reg(format!("zext i32 {} to i64", dst.s));
                let n64 = self.new_reg(format!("zext i32 {} to i64", n.s));
                let dend = self.new_reg(format!("add i64 {}, {}", d64, n64));
                let oob = self.new_reg(format!("icmp ugt i64 {}, {}", dend, msize));
                self.branch_trap(&oob, 4);
                let mbase = self.new_reg("load ptr, ptr @w2l_mem_base, align 8".to_string());
                let dp = self.new_reg(format!("getelementptr i8, ptr {}, i64 {}", mbase, d64));
                self.reg_decl("memset", "declare ptr @memset(ptr, i32, i64)");
                self.emit(format!(
                    "call ptr @memset(ptr {}, i32 {}, i64 {})",
                    dp, val.s, n64
                ));
            }
            Instr::Un(_) | Instr::Bin(_) | Instr::Test(_) | Instr::Rel(_) | Instr::Cvt(_) => {
                self.gen_numeric(instr);
            }
            Instr::V128Load(_)
            | Instr::V128Store(_)
            | Instr::V128LoadLane(..)
            | Instr::V128LoadExt(..)
            | Instr::I8x16Shuffle(_)
            | Instr::SimdLane(..)
            | Instr::Simd(_) => {
                self.gen_simd(instr);
            }
            _ => unreachable!("control instruction in gen_simple"),
        }
    }

    fn gen_call(&mut self, sym: &str, ft: &FuncType) {
        let nparams = ft.params.len();
        let args = self.pop_n(nparams);
        let argstr = args
            .iter()
            .map(|v| Self::opnd(v))
            .collect::<Vec<_>>()
            .join(", ");
        let rt = ret_ty(&ft.results);
        match ft.results.len() {
            0 => {
                self.emit(format!("call void {}({})", sym, argstr));
            }
            1 => {
                let r = self.val(ft.results[0], format!("call {} {}({})", rt, sym, argstr));
                self.push(r);
            }
            _ => {
                let sty = struct_ty(&ft.results);
                let s = self.new_reg(format!("call {} {}({})", rt, sym, argstr));
                for (i, t) in ft.results.iter().enumerate() {
                    let e = self.val(
                        *t,
                        format!("extractvalue {} {}, {}", sty, s, i),
                    );
                    self.push(e);
                }
            }
        }
    }

    fn gen_call_through(&mut self, fptr: &str, ft: &FuncType) {
        let nparams = ft.params.len();
        let args = self.pop_n(nparams);
        let argstr = args
            .iter()
            .map(|v| Self::opnd(v))
            .collect::<Vec<_>>()
            .join(", ");
        let rt = ret_ty(&ft.results);
        match ft.results.len() {
            0 => {
                self.emit(format!("call void {}({})", fptr, argstr));
            }
            1 => {
                let r = self.val(
                    ft.results[0],
                    format!("call {} {}({})", rt, fptr, argstr),
                );
                self.push(r);
            }
            _ => {
                let sty = struct_ty(&ft.results);
                let s = self.new_reg(format!("call {} {}({})", rt, fptr, argstr));
                for (i, t) in ft.results.iter().enumerate() {
                    let e = self.val(*t, format!("extractvalue {} {}, {}", sty, s, i));
                    self.push(e);
                }
            }
        }
    }

    pub fn mem_ptr(&mut self, addr: &Val, offset: u32, bytes: u32) -> Val {
        let a64 = self.new_reg(format!("zext i32 {} to i64", addr.s));
        let ea = if offset == 0 {
            a64
        } else {
            self.new_reg(format!("add i64 {}, {}", a64, offset))
        };
        let end = self.new_reg(format!("add i64 {}, {}", ea, bytes));
        let size = self.new_reg("load i64, ptr @w2l_mem_size, align 8".to_string());
        let oob = self.new_reg(format!("icmp ugt i64 {}, {}", end, size));
        self.branch_trap(&oob, 4);
        let base = self.new_reg("load ptr, ptr @w2l_mem_base, align 8".to_string());
        let p = self.new_reg(format!("getelementptr i8, ptr {}, i64 {}", base, ea));
        Val {
            ty: ValType::I32,
            lty: "ptr".into(),
            s: p,
        }
    }

    pub fn gen_body(&mut self, params: &[ValType], locals: &[ValType], body: &[Instr]) {
        // Locals: params then declared locals.
        let mut all_types: Vec<ValType> = params.to_vec();
        all_types.extend_from_slice(locals);
        self.local_types = all_types.clone();
        for (i, t) in all_types.iter().enumerate() {
            let a = match t {
                ValType::V128 => 16,
                ValType::I64 | ValType::F64 => 8,
                _ => 4,
            };
            self.entry_allocas.push(format!(
                "%loc_{} = alloca {}, align {}",
                i,
                llvm_ty(*t),
                a
            ));
            self.locals.push(format!("%loc_{}", i));
        }

        // Prologue: depth and stack checks, then parameter stores.
        let d0 = self.new_reg("load i32, ptr @w2l_depth, align 4".to_string());
        let d1 = self.new_reg(format!("add i32 {}, 1", d0));
        self.emit(format!("store i32 {}, ptr @w2l_depth, align 4", d1));
        let deep = self.new_reg(format!("icmp ugt i32 {}, {}", d1, DEPTH_LIMIT));
        self.reg_decl(
            "llvm.frameaddress.p0",
            "declare ptr @llvm.frameaddress.p0(i32)",
        );
        let fp = self.new_reg("call ptr @llvm.frameaddress.p0(i32 0)".to_string());
        let sp = self.new_reg(format!("ptrtoint ptr {} to i64", fp));
        let lim = self.new_reg("load i64, ptr @w2l_stack_limit, align 8".to_string());
        let so = self.new_reg(format!("icmp ult i64 {}, {}", sp, lim));
        let tr = self.new_reg(format!("or i1 {}, {}", deep, so));
        let body_label = self.fresh_label();
        let trap = self.trap_block(8);
        self.emit(format!(
            "br i1 {}, label %{}, label %{}",
            tr, trap, body_label
        ));
        self.start_block(&body_label);
        for (i, _t) in params.iter().enumerate() {
            let t = params[i];
            self.emit(format!(
                "store {} %p{}, ptr {}, align {}",
                llvm_ty(t),
                i,
                self.locals[i],
                slot_align(t)
            ));
        }
        // Non-parameter locals are zero-initialized by wasm semantics.
        for i in params.len()..all_types.len() {
            let t = all_types[i];
            let zero = match t {
                ValType::V128 => "<16 x i8> zeroinitializer".to_string(),
                ValType::FuncRef | ValType::ExternRef => "ptr null".to_string(),
                ValType::F32 | ValType::F64 => format!("{} 0.0", llvm_ty(t)),
                _ => format!("{} 0", llvm_ty(t)),
            };
            self.emit(format!(
                "store {}, ptr {}, align {}",
                zero,
                self.locals[i],
                slot_align(t)
            ));
        }

        // Outer function frame.
        let end_label = self.fresh_label();
        self.ctrls.push(Frame {
            kind: FKind::Function,
            end_label,
            header_label: None,
            header_phi_idx: vec![],
            param_types: vec![],
            result_types: self.ret_types.clone(),
            base: 0,
            end_incoming: vec![],
            else_label: None,
            else_block_idx: None,
            params_vals: vec![],
            had_else: false,
            reachable_at_entry: true,
        });

        for ins in body {
            match ins {
                Instr::Block(_)
                | Instr::Loop(_)
                | Instr::If(_)
                | Instr::Else
                | Instr::End
                | Instr::Br(_)
                | Instr::BrIf(_)
                | Instr::BrTable(..)
                | Instr::Return
                | Instr::Unreachable
                | Instr::Nop => self.gen_control(ins),
                _ => self.gen_simple(ins),
            }
        }
    }

    fn render(&self, name: &str, params: &[ValType], ret: &str) -> String {
        let mut s = String::new();
        let attr = if self.needs_ssse3 { " #0" } else { "" };
        s.push_str(&format!(
            "define internal {} @{}({}){} {{\n",
            ret,
            name,
            param_list(params),
            attr
        ));
        for (i, b) in self.blocks.iter().enumerate() {
            s.push_str(&format!("{}:\n", b.label));
            if i == 0 {
                for a in &self.entry_allocas {
                    s.push_str(&format!("  {}\n", a));
                }
            }
            for &pi in &b.phis {
                let p = &self.phis[pi];
                let inc: Vec<String> = p
                    .incoming
                    .iter()
                    .map(|(v, pred)| format!("[ {}, %{} ]", v, pred))
                    .collect();
                s.push_str(&format!(
                    "  {} = phi {} {}\n",
                    p.name,
                    p.ty,
                    inc.join(", ")
                ));
            }
            for line in &b.body {
                s.push_str(&format!("  {}\n", line));
            }
        }
        s.push_str("}\n\n");
        s
    }
}

// ---------------------------------------------------------------------------
// Module-level generation
// ---------------------------------------------------------------------------

pub fn generate(m: &Module) -> Result<String, String> {
    let mut decls = Decls::new();
    decls.reg("w2l_trap", "declare void @w2l_trap(i32) noreturn");

    let type_canon = canonical_types(m);
    let static_text = emit_static(m);
    let mut funcs = String::new();

    // Imported function trampolines.
    for (i, (module, name, tidx)) in m.imported_funcs.iter().enumerate() {
        let ft = &m.types[*tidx as usize];
        funcs.push_str(&gen_import_trampoline(i, module, name, ft, &mut decls));
    }

    // Defined functions.
    let mut any_ssse3 = false;
    for k in 0..m.funcs.len() {
        let idx = m.num_imported_funcs + k;
        let f = &m.funcs[k];
        let ft = &m.types[f.type_idx as usize];
        let mut g = FuncGen::new(m, &mut decls, ft.results.clone(), &type_canon);
        g.gen_body(&ft.params, &f.locals, &f.body);
        if g.needs_ssse3 {
            any_ssse3 = true;
        }
        let ret = ret_ty(&ft.results);
        funcs.push_str(&g.render(&format!("w2l_func_{}", idx), &ft.params, &ret));
    }

    // Runtime helpers and ABI entry points.
    funcs.push_str(&gen_runtime_helpers(m, &mut decls));
    funcs.push_str(&gen_export_wrappers(m, &mut decls));
    funcs.push_str(&gen_init(m, &mut decls));

    let mut out = String::new();
    out.push_str("target triple = \"x86_64-unknown-linux-gnu\"\n\n");
    out.push_str(&decls.text());
    out.push('\n');
    out.push_str(&static_text);
    out.push('\n');
    out.push_str(&funcs);
    if any_ssse3 {
        out.push_str("\nattributes #0 = { \"target-features\"=\"+ssse3\" }\n");
    }
    Ok(out)
}

fn gen_import_trampoline(
    i: usize,
    module: &[u8],
    name: &[u8],
    ft: &FuncType,
    decls: &mut Decls,
) -> String {
    let sym = format!("w2l_import_{}_{}", hex_encode(module), hex_encode(name));
    decls.reg(&sym, &format!("declare void @{}(ptr, ptr)", sym));
    let ret = ret_ty(&ft.results);
    let np = ft.params.len();
    let nr = ft.results.len();
    let mut s = String::new();
    s.push_str(&format!(
        "define internal {} @w2l_func_{}({}) {{\n",
        ret,
        i,
        param_list(&ft.params)
    ));
    s.push_str("entry:\n");
    s.push_str(&format!(
        "  %args = alloca [{} x <16 x i8>], align 16\n",
        np.max(1)
    ));
    s.push_str(&format!(
        "  %res = alloca [{} x <16 x i8>], align 16\n",
        nr.max(1)
    ));
    for j in 0..np {
        let t = ft.params[j];
        s.push_str(&format!(
            "  %ap{} = getelementptr i8, ptr %args, i64 {}\n",
            j,
            j * 16
        ));
        s.push_str(&format!(
            "  store {} %p{}, ptr %ap{}, align {}\n",
            llvm_ty(t),
            j,
            j,
            slot_align(t)
        ));
    }
    s.push_str(&format!("  call void @{}(ptr %args, ptr %res)\n", sym));
    match nr {
        0 => s.push_str("  ret void\n"),
        1 => {
            let t = ft.results[0];
            s.push_str(&format!(
                "  %r0 = load {}, ptr %res, align {}\n",
                llvm_ty(t),
                slot_align(t)
            ));
            s.push_str(&format!("  ret {} %r0\n", llvm_ty(t)));
        }
        _ => {
            let sty = struct_ty(&ft.results);
            let mut cur = String::from("poison");
            for j in 0..nr {
                let t = ft.results[j];
                s.push_str(&format!(
                    "  %rp{} = getelementptr i8, ptr %res, i64 {}\n",
                    j,
                    j * 16
                ));
                s.push_str(&format!(
                    "  %rv{} = load {}, ptr %rp{}, align {}\n",
                    j,
                    llvm_ty(t),
                    j,
                    slot_align(t)
                ));
                s.push_str(&format!(
                    "  %sv{} = insertvalue {} {}, {} %rv{}, {}\n",
                    j,
                    sty,
                    cur,
                    llvm_ty(t),
                    j,
                    j
                ));
                cur = format!("%sv{}", j);
            }
            s.push_str(&format!("  ret {} {}\n", sty, cur));
        }
    }
    s.push_str("}\n\n");
    s
}

fn gen_export_wrappers(m: &Module, decls: &mut Decls) -> String {
    let mut s = String::new();
    for e in &m.exports {
        match e.kind {
            ExportKind::Func => {
                let idx = e.idx as usize;
                let ti = m.func_types[idx] as usize;
                let ft = &m.types[ti];
                let sym = format!("w2l_export_{}", hex_encode(&e.name));
                let ret = ret_ty(&ft.results);
                s.push_str(&format!(
                    "define void @{}(ptr %args, ptr %results) {{\n",
                    sym
                ));
                s.push_str("entry:\n");
                decls.reg(
                    "llvm.frameaddress.p0",
                    "declare ptr @llvm.frameaddress.p0(i32)",
                );
                s.push_str("  store i32 0, ptr @w2l_depth, align 4\n");
                s.push_str("  %fp = call ptr @llvm.frameaddress.p0(i32 0)\n");
                s.push_str("  %sp = ptrtoint ptr %fp to i64\n");
                s.push_str(&format!("  %lim = sub i64 %sp, {}\n", STACK_BUDGET));
                s.push_str("  store i64 %lim, ptr @w2l_stack_limit, align 8\n");
                let mut args = Vec::new();
                for (j, t) in ft.params.iter().enumerate() {
                    s.push_str(&format!(
                        "  %ap{} = getelementptr i8, ptr %args, i64 {}\n",
                        j,
                        j * 16
                    ));
                    s.push_str(&format!(
                        "  %a{} = load {}, ptr %ap{}, align {}\n",
                        j,
                        llvm_ty(*t),
                        j,
                        slot_align(*t)
                    ));
                    args.push(format!("{} %a{}", llvm_ty(*t), j));
                }
                let call = format!(
                    "call {} @w2l_func_{}({})",
                    ret,
                    idx,
                    args.join(", ")
                );
                match ft.results.len() {
                    0 => s.push_str(&format!("  {}\n", call)),
                    1 => {
                        let t = ft.results[0];
                        s.push_str(&format!("  %r0 = {}\n", call));
                        s.push_str(&format!(
                            "  %rp0 = getelementptr i8, ptr %results, i64 0\n"
                        ));
                        s.push_str(&format!(
                            "  store {} %r0, ptr %rp0, align {}\n",
                            llvm_ty(t),
                            slot_align(t)
                        ));
                    }
                    _ => {
                        let sty = struct_ty(&ft.results);
                        s.push_str(&format!("  %rv = {}\n", call));
                        for (j, t) in ft.results.iter().enumerate() {
                            s.push_str(&format!(
                                "  %rr{} = extractvalue {} %rv, {}\n",
                                j, sty, j
                            ));
                            s.push_str(&format!(
                                "  %rp{} = getelementptr i8, ptr %results, i64 {}\n",
                                j,
                                j * 16
                            ));
                            s.push_str(&format!(
                                "  store {} %rr{}, ptr %rp{}, align {}\n",
                                llvm_ty(*t),
                                j,
                                j,
                                slot_align(*t)
                            ));
                        }
                    }
                }
                s.push_str("  ret void\n}\n\n");
            }
            ExportKind::Global => {
                let idx = e.idx as usize;
                let t = m.global_types[idx].val;
                let sym = format!("w2l_global_{}", hex_encode(&e.name));
                s.push_str(&format!("define void @{}(ptr %out) {{\n", sym));
                s.push_str("entry:\n");
                s.push_str(&format!(
                    "  %v = load {}, ptr @w2l_glob_{}, align {}\n",
                    llvm_ty(t),
                    idx,
                    slot_align(t)
                ));
                s.push_str(&format!(
                    "  store {} %v, ptr %out, align {}\n",
                    llvm_ty(t),
                    slot_align(t)
                ));
                s.push_str("  ret void\n}\n\n");
            }
            ExportKind::Mem => {
                let sym = format!("w2l_memory_{}", hex_encode(&e.name));
                s.push_str(&format!("define ptr @{}(ptr %size_out) {{\n", sym));
                s.push_str("entry:\n");
                s.push_str("  %sz = load i64, ptr @w2l_mem_size, align 8\n");
                s.push_str("  store i64 %sz, ptr %size_out, align 8\n");
                s.push_str("  %base = load ptr, ptr @w2l_mem_base, align 8\n");
                s.push_str("  ret ptr %base\n}\n\n");
            }
            ExportKind::Table => {}
        }
    }
    s
}

fn gen_runtime_helpers(m: &Module, decls: &mut Decls) -> String {
    let mut s = String::new();
    let mem_max: u64 = m
        .mem_types
        .first()
        .map(|mt| mt.limits.max.unwrap_or(65536) as u64)
        .unwrap_or(65536);
    if !m.mem_types.is_empty() {
        decls.reg("realloc", "declare ptr @realloc(ptr, i64)");
        decls.reg("memset", "declare ptr @memset(ptr, i32, i64)");
        s.push_str(&format!(
            r#"define internal i32 @w2l_mem_grow(i32 %delta) {{
entry:
  %old = load i32, ptr @w2l_mem_pages, align 4
  %oldz = zext i32 %old to i64
  %dz = zext i32 %delta to i64
  %new = add i64 %oldz, %dz
  %over = icmp ugt i64 %new, {max}
  br i1 %over, label %fail, label %c1
c1:
  %same = icmp eq i64 %new, %oldz
  br i1 %same, label %done, label %c2
c2:
  %newbytes = shl i64 %new, 16
  %base = load ptr, ptr @w2l_mem_base, align 8
  %nb = call ptr @realloc(ptr %base, i64 %newbytes)
  %isnull = icmp eq ptr %nb, null
  br i1 %isnull, label %fail, label %c3
c3:
  store ptr %nb, ptr @w2l_mem_base, align 8
  %oldbytes = shl i64 %oldz, 16
  %p = getelementptr i8, ptr %nb, i64 %oldbytes
  %len = sub i64 %newbytes, %oldbytes
  call ptr @memset(ptr %p, i32 0, i64 %len)
  store i64 %newbytes, ptr @w2l_mem_size, align 8
  %newtr = trunc i64 %new to i32
  store i32 %newtr, ptr @w2l_mem_pages, align 4
  br label %done
done:
  ret i32 %old
fail:
  ret i32 -1
}}

"#,
            max = mem_max
        ));
    }

    for (ti, tt) in m.table_types.iter().enumerate() {
        let max = tt.limits.max.unwrap_or(0xffff_ffff) as u64;
        s.push_str(&format!(
            r#"define internal i32 @w2l_tab_grow_{ti}(ptr %init, i32 %delta) {{
entry:
  %old = load i32, ptr @w2l_tab_size_{ti}, align 4
  %oldz = zext i32 %old to i64
  %dz = zext i32 %delta to i64
  %new = add i64 %oldz, %dz
  %over = icmp ugt i64 %new, {max}
  br i1 %over, label %fail, label %c1
c1:
  %same = icmp eq i64 %new, %oldz
  br i1 %same, label %done, label %c2
c2:
  %newbytes = mul i64 %new, 8
  %base = load ptr, ptr @w2l_tab_base_{ti}, align 8
  %nb = call ptr @realloc(ptr %base, i64 %newbytes)
  %isnull = icmp eq ptr %nb, null
  br i1 %isnull, label %fail, label %c3
c3:
  store ptr %nb, ptr @w2l_tab_base_{ti}, align 8
  %p0 = getelementptr ptr, ptr %nb, i64 %oldz
  br label %loop
loop:
  %p = phi ptr [ %p0, %c3 ], [ %pn, %body ]
  %cnt = phi i64 [ %oldz, %c3 ], [ %cnt1, %body ]
  %fin = icmp eq i64 %cnt, %new
  br i1 %fin, label %fdone, label %body
body:
  store ptr %init, ptr %p, align 8
  %pn = getelementptr ptr, ptr %p, i64 1
  %cnt1 = add i64 %cnt, 1
  br label %loop
fdone:
  %newtr = trunc i64 %new to i32
  store i32 %newtr, ptr @w2l_tab_size_{ti}, align 4
  br label %done
done:
  ret i32 %old
fail:
  ret i32 -1
}}

"#,
            ti = ti,
            max = max
        ));
        s.push_str(&format!(
            r#"define internal void @w2l_tab_fill_{ti}(i32 %dst, ptr %val, i32 %n) {{
entry:
  %d = zext i32 %dst to i64
  %nn = zext i32 %n to i64
  %base = load ptr, ptr @w2l_tab_base_{ti}, align 8
  %p0 = getelementptr ptr, ptr %base, i64 %d
  br label %loop
loop:
  %p = phi ptr [ %p0, %entry ], [ %pn, %body ]
  %cnt = phi i64 [ 0, %entry ], [ %cnt1, %body ]
  %fin = icmp eq i64 %cnt, %nn
  br i1 %fin, label %exit, label %body
body:
  store ptr %val, ptr %p, align 8
  %pn = getelementptr ptr, ptr %p, i64 1
  %cnt1 = add i64 %cnt, 1
  br label %loop
exit:
  ret void
}}

"#,
            ti = ti
        ));
    }
    decls.reg("realloc", "declare ptr @realloc(ptr, i64)");
    s
}

fn gen_init(m: &Module, decls: &mut Decls) -> String {
    let mut s = String::new();
    s.push_str("define void @w2l_init() {\nentry:\n");
    s.push_str("  store i32 0, ptr @w2l_depth, align 4\n");
    decls.reg(
        "llvm.frameaddress.p0",
        "declare ptr @llvm.frameaddress.p0(i32)",
    );
    s.push_str("  %fp = call ptr @llvm.frameaddress.p0(i32 0)\n");
    s.push_str("  %sp = ptrtoint ptr %fp to i64\n");
    s.push_str(&format!("  %lim = sub i64 %sp, {}\n", STACK_BUDGET));
    s.push_str("  store i64 %lim, ptr @w2l_stack_limit, align 8\n");

    decls.reg("calloc", "declare ptr @calloc(i64, i64)");
    decls.reg("memcpy", "declare ptr @memcpy(ptr, ptr, i64)");

    // Memory.
    if let Some(mt) = m.mem_types.first() {
        let pages = mt.limits.min as u64;
        let bytes = pages << 16;
        s.push_str(&format!(
            "  %m = call ptr @calloc(i64 {}, i64 1)\n",
            bytes.max(1)
        ));
        if bytes > 0 {
            s.push_str("  %mnull = icmp eq ptr %m, null\n");
            s.push_str("  br i1 %mnull, label %failm, label %mok\n");
            s.push_str("failm:\n  call void @w2l_trap(i32 8)\n  unreachable\n");
            s.push_str("mok:\n");
        }
        s.push_str("  store ptr %m, ptr @w2l_mem_base, align 8\n");
        s.push_str(&format!("  store i64 {}, ptr @w2l_mem_size, align 8\n", bytes));
        s.push_str(&format!(
            "  store i32 {}, ptr @w2l_mem_pages, align 4\n",
            pages
        ));
    }

    // Tables.
    for (i, tt) in m.table_types.iter().enumerate() {
        let min = tt.limits.min as u64;
        s.push_str(&format!(
            "  %t{i} = call ptr @calloc(i64 {}, i64 8)\n",
            min.max(1)
        ));
        if min > 0 {
            s.push_str(&format!("  %t{i}n = icmp eq ptr %t{i}, null\n"));
            s.push_str(&format!(
                "  br i1 %t{i}n, label %failt{i}, label %t{i}ok\n"
            ));
            s.push_str(&format!(
                "failt{i}:\n  call void @w2l_trap(i32 8)\n  unreachable\n"
            ));
            s.push_str(&format!("t{}ok:\n", i));
        }
        s.push_str(&format!(
            "  store ptr %t{i}, ptr @w2l_tab_base_{i}, align 8\n"
        ));
        s.push_str(&format!(
            "  store i32 {}, ptr @w2l_tab_size_{i}, align 4\n",
            min
        ));
    }

    // Static bounds checks for active segments; if any is out of bounds we
    // trap during instantiation at that point.
    let mut oob: Option<i32> = None;

    if let Some(mt) = m.mem_types.first() {
        let mem_size = (mt.limits.min as u64) << 16;
        for d in &m.datas {
            if let DataMode::Active { offset, .. } = &d.mode {
                let cv = eval_const(m, offset);
                let off = cv.to_u32() as u64;
                let len = d.init.len() as u64;
                if off + len > mem_size {
                    oob = Some(4);
                    break;
                }
            }
        }
    }

    if oob.is_none() {
        for e in &m.elems {
            if let ElemMode::Active { table, offset } = &e.mode {
                let tsize = m.table_types[*table as usize].limits.min as u64;
                let cv = eval_const(m, offset);
                let off = cv.to_u32() as u64;
                let len = e.init.len() as u64;
                if off + len > tsize {
                    oob = Some(5);
                    break;
                }
            }
        }
    }

    if let Some(code) = oob {
        s.push_str(&format!("  call void @w2l_trap(i32 {})\n  unreachable\n}}\n\n", code));
        return s;
    }

    // Active element segments.
    for (i, e) in m.elems.iter().enumerate() {
        if let ElemMode::Active { table, offset } = &e.mode {
            let cv = eval_const(m, offset);
            let off = cv.to_u32() as u64;
            let len = e.init.len() as u64;
            s.push_str(&format!(
                "  %eb{i} = load ptr, ptr @w2l_tab_base_{}, align 8\n",
                table
            ));
            s.push_str(&format!(
                "  %ep{i} = getelementptr ptr, ptr %eb{i}, i64 {}\n",
                off
            ));
            s.push_str(&format!(
                "  call ptr @memcpy(ptr %ep{i}, ptr @w2l_elem_{i}, i64 {})\n",
                len * 8
            ));
        }
    }

    // Active data segments.
    for (i, d) in m.datas.iter().enumerate() {
        if let DataMode::Active { offset, .. } = &d.mode {
            let cv = eval_const(m, offset);
            let off = cv.to_u32() as u64;
            let len = d.init.len() as u64;
            s.push_str(&format!(
                "  %mb{i} = load ptr, ptr @w2l_mem_base, align 8\n"
            ));
            s.push_str(&format!(
                "  %dp{i} = getelementptr i8, ptr %mb{i}, i64 {}\n",
                off
            ));
            s.push_str(&format!(
                "  call ptr @memcpy(ptr %dp{i}, ptr @w2l_data_{i}, i64 {})\n",
                len
            ));
        }
    }

    if let Some(st) = m.start {
        s.push_str(&format!("  call void @w2l_func_{}()\n", st));
    }

    s.push_str("  ret void\n}\n\n");
    s
}

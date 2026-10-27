// LLVM IR generation for the supported WebAssembly subset.

use crate::parse::*;
use std::collections::BTreeSet;

const DEPTH_LIMIT: i32 = 100000;

const V: &str = "<16 x i8>";
const I8X16: &str = "<16 x i8>";
const I16X8: &str = "<8 x i16>";
const I32X4: &str = "<4 x i32>";
const I64X2: &str = "<2 x i64>";
const F32X4: &str = "<4 x float>";
const F64X2: &str = "<2 x double>";

pub fn generate(m: &Module) -> Result<String, String> {
    let mut g = Gen::new(m);
    g.gen_module()?;
    Ok(g.finish())
}

#[derive(Clone, Debug)]
struct Val {
    name: String,
    ty: ValType,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FrameKind {
    Func,
    Block,
    Loop,
    If,
    Else,
}

struct Frame {
    kind: FrameKind,
    params: Vec<ValType>,
    results: Vec<ValType>,
    param_vals: Vec<Val>,
    below: Vec<Val>,
    end_label: String,
    header_label: String,
    else_label: String,
    slots: Vec<String>,
    has_pred: bool,
    entry_reachable: bool,
    has_else: bool,
}

pub fn ll_ty(t: ValType) -> &'static str {
    match t {
        ValType::I32 => "i32",
        ValType::I64 => "i64",
        ValType::F32 => "float",
        ValType::F64 => "double",
        ValType::V128 => V,
    }
}

fn align_of(t: ValType) -> u32 {
    match t {
        ValType::I32 | ValType::F32 => 4,
        ValType::I64 | ValType::F64 => 8,
        ValType::V128 => 16,
    }
}

fn ret_ty(results: &[ValType]) -> String {
    match results.len() {
        0 => "void".to_string(),
        1 => ll_ty(results[0]).to_string(),
        _ => {
            let parts: Vec<&str> = results.iter().map(|t| ll_ty(*t)).collect();
            format!("{{ {} }}", parts.join(", "))
        }
    }
}

fn hex_name(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        out.push_str(&format!("{:02x}", b));
    }
    out
}

fn f32c(v: f32) -> String {
    format!("f0x{:08x}", v.to_bits())
}
fn f64c(v: f64) -> String {
    format!("f0x{:016x}", v.to_bits())
}

fn lanes_of(ty: &str) -> usize {
    match ty {
        I8X16 => 16,
        I16X8 => 8,
        I32X4 => 4,
        I64X2 => 2,
        F32X4 => 4,
        F64X2 => 2,
        "<16 x i16>" => 16,
        "<8 x i32>" => 8,
        _ => 1,
    }
}
fn suffix_of(ty: &str) -> &'static str {
    match ty {
        I8X16 => "v16i8",
        I16X8 => "v8i16",
        I32X4 => "v4i32",
        I64X2 => "v2i64",
        F32X4 => "v4f32",
        F64X2 => "v2f64",
        _ => "i32",
    }
}
fn mask_ty(ty: &str) -> String {
    format!("<{} x i1>", lanes_of(ty))
}

fn const_vec(ty: &str, ety: &str, val: &str) -> String {
    let n = lanes_of(ty);
    let items: Vec<String> = (0..n).map(|_| format!("{} {}", ety, val)).collect();
    format!("<{}>", items.join(", "))
}

fn idx_mask(indices: &[i64]) -> String {
    format!(
        "<{} x i32> <{}>",
        indices.len(),
        indices
            .iter()
            .map(|i| format!("i32 {}", i))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

struct Gen<'a> {
    m: &'a Module,
    decls: BTreeSet<String>,
    consts: Vec<String>,
    globals: Vec<String>,
    funcs: String,
    helpers: Vec<String>,
    canon: Vec<u32>,
    // per-function state
    out: Vec<String>,
    allocas: Vec<String>,
    tmp_n: usize,
    label_n: usize,
    slot_n: usize,
    stack: Vec<Val>,
    frames: Vec<Frame>,
    reachable: bool,
    terminated: bool,
    locals: Vec<ValType>,
    local_ptr: Vec<String>,
    fn_name: String,
}

impl<'a> Gen<'a> {
    fn new(m: &'a Module) -> Self {
        let mut canon = vec![0u32; m.types.len()];
        for i in 0..m.types.len() {
            canon[i] = i as u32;
            for j in 0..i {
                if m.types[j] == m.types[i] {
                    canon[i] = canon[j];
                    break;
                }
            }
        }
        Gen {
            m,
            decls: BTreeSet::new(),
            consts: Vec::new(),
            globals: Vec::new(),
            funcs: String::new(),
            helpers: Vec::new(),
            canon,
            out: Vec::new(),
            allocas: Vec::new(),
            tmp_n: 0,
            label_n: 0,
            slot_n: 0,
            stack: Vec::new(),
            frames: Vec::new(),
            reachable: true,
            terminated: false,
            locals: Vec::new(),
            local_ptr: Vec::new(),
            fn_name: String::new(),
        }
    }

    // ------------------------------------------------------------------
    // low level emission
    // ------------------------------------------------------------------

    fn emit(&mut self, s: String) {
        self.out.push(s);
    }
    fn tmp(&mut self) -> String {
        self.tmp_n += 1;
        format!("%t{}", self.tmp_n)
    }
    fn new_label(&mut self) -> String {
        self.label_n += 1;
        format!("L{}", self.label_n)
    }
    fn start_block(&mut self, label: &str) {
        if !self.terminated {
            self.out.push("  unreachable".to_string());
        }
        self.out.push(format!("{}:", label));
        self.terminated = false;
    }
    fn term(&mut self, line: String) {
        self.out.push(line);
        self.terminated = true;
    }
    fn need_decl(&mut self, d: &str) {
        self.decls.insert(d.to_string());
    }
    fn call_intr(&mut self, ret: &str, name: &str, argtys: &str, args: &str) -> String {
        self.need_decl(&format!("declare {} @{}({})", ret, name, argtys));
        let t = self.tmp();
        self.emit(format!("  {} = call {} @{}({})", t, ret, name, args));
        t
    }
    fn call_void_intr(&mut self, name: &str, argtys: &str, args: &str) {
        self.need_decl(&format!("declare void @{}({})", name, argtys));
        self.emit(format!("  call void @{}({})", name, args));
    }
    fn call_memcpy(&mut self, dst: &str, src: &str, len: &str) {
        self.call_void_intr(
            "llvm.memcpy.p0.p0.i64",
            "ptr, ptr, i64, i1",
            &format!("ptr {}, ptr {}, i64 {}, i1 false", dst, src, len),
        );
    }
    fn call_memmove(&mut self, dst: &str, src: &str, len: &str) {
        self.call_void_intr(
            "llvm.memmove.p0.p0.i64",
            "ptr, ptr, i64, i1",
            &format!("ptr {}, ptr {}, i64 {}, i1 false", dst, src, len),
        );
    }
    fn call_memset(&mut self, dst: &str, val: &str, len: &str) {
        self.call_void_intr(
            "llvm.memset.p0.i64",
            "ptr, i8, i64, i1",
            &format!("ptr {}, i8 {}, i64 {}, i1 false", dst, val, len),
        );
    }

    fn trap_if(&mut self, cond: &str, code: i32) {
        let tb = self.new_label();
        let ok = self.new_label();
        self.term(format!("  br i1 {}, label %{}, label %{}", cond, tb, ok));
        self.start_block(&tb);
        self.emit(format!("  call void @w2l_trap(i32 {})", code));
        self.emit("  unreachable".to_string());
        self.terminated = true;
        self.start_block(&ok);
    }

    fn push(&mut self, v: Val) {
        self.stack.push(v);
    }
    fn pop(&mut self) -> Val {
        self.stack.pop().expect("value stack underflow")
    }
    fn pop_vals(&mut self, types: &[ValType]) -> Vec<Val> {
        let mut v: Vec<Val> = types
            .iter()
            .map(|t| Val { name: String::new(), ty: *t })
            .collect();
        for i in (0..types.len()).rev() {
            v[i] = self.pop();
        }
        v
    }

    fn alloca(&mut self, ty: &str, align: u32, prefix: &str) -> String {
        self.slot_n += 1;
        let name = format!("%{}{}", prefix, self.slot_n);
        self.allocas.push(format!("  {} = alloca {}, align {}", name, ty, align));
        name
    }
    fn new_slots(&mut self, types: &[ValType], prefix: &str) -> Vec<String> {
        let mut v = Vec::new();
        for t in types {
            v.push(self.alloca(ll_ty(*t), align_of(*t), prefix));
        }
        v
    }
    fn store_slot(&mut self, slot: &str, v: &Val, ty: ValType) {
        self.emit(format!(
            "  store {} {}, ptr {}, align {}",
            ll_ty(ty),
            v.name,
            slot,
            align_of(ty)
        ));
    }
    fn load_slot(&mut self, slot: &str, ty: ValType) -> Val {
        let t = self.tmp();
        self.emit(format!(
            "  {} = load {}, ptr {}, align {}",
            t,
            ll_ty(ty),
            slot,
            align_of(ty)
        ));
        Val { name: t, ty }
    }

    // ------------------------------------------------------------------
    // module level
    // ------------------------------------------------------------------

    fn gen_module(&mut self) -> Result<(), String> {
        self.need_decl("declare void @w2l_trap(i32) noreturn");
        self.need_decl("declare ptr @mmap(ptr, i64, i32, i32, i32, i64)");
        self.need_decl("declare ptr @mremap(ptr, i64, i64, i32)");
        self.need_decl("declare ptr @calloc(i64, i64)");

        self.emit_module_globals();
        self.emit_segments();
        self.gen_mem_grow_helper();

        let n_imports = self.m.imports.len();
        for i in 0..n_imports {
            let imp = &self.m.imports[i];
            let sym = format!(
                "w2l_import_{}_{}",
                hex_name(&imp.module),
                hex_name(&imp.name)
            );
            self.need_decl(&format!("declare void @{}(ptr, ptr)", sym));
            self.gen_import_wrapper(i as u32, &sym)?;
        }
        for i in 0..self.m.func_types.len() {
            let idx = (n_imports + i) as u32;
            self.gen_wasm_function(idx)?;
        }

        self.gen_init()?;
        self.gen_exports()?;
        Ok(())
    }

    fn emit_module_globals(&mut self) {
        self.globals.push("@mem.base = internal global ptr null".to_string());
        self.globals.push("@mem.size = internal global i64 0".to_string());
        self.globals.push("@tbl.base = internal global ptr null".to_string());
        self.globals.push("@tbl.size = internal global i64 0".to_string());
        self.globals.push("@tbl.types = internal global ptr null".to_string());
        self.globals.push("@w2l.depth = internal global i32 0".to_string());
        for (i, g) in self.m.globals.iter().enumerate() {
            self.globals.push(format!(
                "@g{} = internal global {} zeroinitializer",
                i,
                ll_ty(g.ty)
            ));
        }
        for (i, d) in self.m.datas.iter().enumerate() {
            let init = if d.mode == DataMode::Passive { d.data.len() } else { 0 };
            self.globals
                .push(format!("@dlen{} = internal global i32 {}", i, init));
        }
        for (i, e) in self.m.elems.iter().enumerate() {
            let init = if e.mode == ElemMode::Passive { e.funcs.len() } else { 0 };
            self.globals
                .push(format!("@elen{} = internal global i32 {}", i, init));
        }
    }

    fn c_string(data: &[u8]) -> String {
        let mut s = String::from("c\"");
        for &b in data {
            if (0x20..0x7f).contains(&b) && b != b'"' && b != b'\\' {
                s.push(b as char);
            } else {
                s.push_str(&format!("\\{:02X}", b));
            }
        }
        s.push('"');
        s
    }

    fn emit_segments(&mut self) {
        for (i, d) in self.m.datas.iter().enumerate() {
            let decl = if d.data.is_empty() {
                format!("@d{} = private constant [0 x i8] zeroinitializer", i)
            } else {
                format!(
                    "@d{} = private constant [{} x i8] {}",
                    i,
                    d.data.len(),
                    Self::c_string(&d.data)
                )
            };
            self.consts.push(decl);
        }
        for (i, e) in self.m.elems.iter().enumerate() {
            if e.funcs.is_empty() {
                self.consts
                    .push(format!("@e{}_p = private constant [0 x ptr] zeroinitializer", i));
                self.consts
                    .push(format!("@e{}_t = private constant [0 x i32] zeroinitializer", i));
            } else {
                let ps: Vec<String> = e.funcs.iter().map(|f| format!("ptr @f{}", f)).collect();
                self.consts.push(format!(
                    "@e{}_p = private constant [{} x ptr] [{}]",
                    i,
                    e.funcs.len(),
                    ps.join(", ")
                ));
                let ts: Vec<String> = e
                    .funcs
                    .iter()
                    .map(|f| {
                        let ti = self.m.func_type_idx(*f).unwrap_or(0);
                        format!("i32 {}", self.canon[ti as usize])
                    })
                    .collect();
                self.consts.push(format!(
                    "@e{}_t = private constant [{} x i32] [{}]",
                    i,
                    e.funcs.len(),
                    ts.join(", ")
                ));
            }
        }
    }

    fn gen_mem_grow_helper(&mut self) {
        let mut s = String::new();
        s.push_str("define internal i32 @w2l.mem_grow(i64 %newbytes) {\n");
        s.push_str("entry:\n");
        s.push_str("  %old = load i64, ptr @mem.size, align 8\n");
        s.push_str("  %iszero = icmp eq i64 %old, 0\n");
        s.push_str("  %base = load ptr, ptr @mem.base, align 8\n");
        s.push_str("  br i1 %iszero, label %fresh, label %remap\n");
        s.push_str("fresh:\n");
        s.push_str("  %p0 = call ptr @mmap(ptr null, i64 %newbytes, i32 3, i32 34, i32 -1, i64 0)\n");
        s.push_str("  %bad0 = icmp eq ptr %p0, inttoptr (i64 -1 to ptr)\n");
        s.push_str("  br i1 %bad0, label %fail, label %ok0\n");
        s.push_str("ok0:\n");
        s.push_str("  store ptr %p0, ptr @mem.base, align 8\n");
        s.push_str("  store i64 %newbytes, ptr @mem.size, align 8\n");
        s.push_str("  ret i32 1\n");
        s.push_str("remap:\n");
        s.push_str("  %p1 = call ptr @mremap(ptr %base, i64 %old, i64 %newbytes, i32 1)\n");
        s.push_str("  %bad1 = icmp eq ptr %p1, inttoptr (i64 -1 to ptr)\n");
        s.push_str("  br i1 %bad1, label %fail, label %ok1\n");
        s.push_str("ok1:\n");
        s.push_str("  store ptr %p1, ptr @mem.base, align 8\n");
        s.push_str("  store i64 %newbytes, ptr @mem.size, align 8\n");
        s.push_str("  ret i32 1\n");
        s.push_str("fail:\n");
        s.push_str("  ret i32 0\n");
        s.push_str("}\n");
        self.helpers.push(s);
    }

    // ------------------------------------------------------------------
    // per function scaffolding
    // ------------------------------------------------------------------

    fn begin_function(&mut self, name: &str) {
        self.out.clear();
        self.allocas.clear();
        self.tmp_n = 0;
        self.label_n = 0;
        self.slot_n = 0;
        self.stack.clear();
        self.frames.clear();
        self.reachable = true;
        self.terminated = false;
        self.locals.clear();
        self.local_ptr.clear();
        self.fn_name = name.to_string();
        self.out.push("entry:".to_string());
    }

    fn assemble_function(&mut self, params: &[ValType], results: &[ValType]) -> String {
        let plist: Vec<String> = params
            .iter()
            .enumerate()
            .map(|(i, t)| format!("{} %p{}", ll_ty(*t), i))
            .collect();
        let mut body = String::new();
        body.push_str(&format!(
            "define internal {} @{}({}) {{\n",
            ret_ty(results),
            self.fn_name,
            plist.join(", ")
        ));
        body.push_str("entry:\n");
        for a in &self.allocas {
            body.push_str(a);
            body.push('\n');
        }
        for line in &self.out[1..] {
            body.push_str(line);
            body.push('\n');
        }
        if !self.terminated {
            body.push_str("  unreachable\n");
        }
        body.push_str("}\n");
        body
    }

    fn gen_import_wrapper(&mut self, idx: u32, sym: &str) -> Result<(), String> {
        let ft = self.m.func_type(idx).unwrap().clone();
        self.begin_function(&format!("f{}", idx));
        let n = ft.params.len().max(1);
        let ia = self.alloca(&format!("[{} x <16 x i8>]", n), 16, "ia");
        let mut argnames = Vec::new();
        for (i, t) in ft.params.iter().enumerate() {
            let p = self.tmp();
            self.emit(format!(
                "  {} = getelementptr i8, ptr {}, i64 {}",
                p,
                ia,
                i * 16
            ));
            let pv = Val { name: format!("%p{}", i), ty: *t };
            self.store_slot(&p, &pv, *t);
            argnames.push(format!("{} {}", ll_ty(*t), pv.name));
        }
        let m = ft.results.len().max(1);
        let ir = self.alloca(&format!("[{} x <16 x i8>]", m), 16, "ir");
        self.emit(format!(
            "  call void @{}(ptr {}, ptr {})",
            sym, ia, ir
        ));
        if ft.results.is_empty() {
            self.term("  ret void".to_string());
        } else if ft.results.len() == 1 {
            let rp = self.tmp();
            self.emit(format!("  {} = getelementptr i8, ptr {}, i64 0", rp, ir));
            let v = self.load_slot(&rp, ft.results[0]);
            self.term(format!("  ret {} {}", ll_ty(ft.results[0]), v.name));
        } else {
            let st = ret_ty(&ft.results);
            let mut acc = String::from("zeroinitializer");
            for (i, t) in ft.results.iter().enumerate() {
                let rp = self.tmp();
                self.emit(format!(
                    "  {} = getelementptr i8, ptr {}, i64 {}",
                    rp,
                    ir,
                    i * 16
                ));
                let v = self.load_slot(&rp, *t);
                let nt = self.tmp();
                self.emit(format!(
                    "  {} = insertvalue {} {}, {} {}, {}",
                    nt,
                    st,
                    acc,
                    ll_ty(*t),
                    v.name,
                    i
                ));
                acc = nt;
            }
            self.term(format!("  ret {} {}", st, acc));
        }
        let body = self.assemble_function(&ft.params, &ft.results);
        self.funcs.push_str(&body);
        Ok(())
    }

    fn gen_wasm_function(&mut self, idx: u32) -> Result<(), String> {
        let ft = self.m.func_type(idx).unwrap().clone();
        let code_idx = (idx as usize) - self.m.imports.len();
        let code = &self.m.codes[code_idx];
        self.begin_function(&format!("f{}", idx));

        // locals: params followed by declared locals
        let mut local_types = ft.params.clone();
        local_types.extend_from_slice(&code.locals);
        for (i, t) in local_types.iter().enumerate() {
            let p = self.alloca(ll_ty(*t), align_of(*t), "l");
            self.local_ptr.push(p);
            if i < ft.params.len() {
                let v = Val { name: format!("%p{}", i), ty: *t };
                self.store_slot(&self.local_ptr[i].clone(), &v, *t);
            } else {
                self.emit(format!(
                    "  store {} zeroinitializer, ptr {}, align {}",
                    ll_ty(*t),
                    self.local_ptr[i],
                    align_of(*t)
                ));
            }
        }
        self.locals = local_types;

        // call depth accounting
        let d = self.tmp();
        self.emit(format!("  {} = load i32, ptr @w2l.depth, align 4", d));
        let d1 = self.tmp();
        self.emit(format!("  {} = add i32 {}, 1", d1, d));
        self.emit(format!("  store i32 {}, ptr @w2l.depth, align 4", d1));
        let deep = self.tmp();
        self.emit(format!(
            "  {} = icmp ugt i32 {}, {}",
            deep, d1, DEPTH_LIMIT
        ));
        self.trap_if(&deep, 8);

        // function-level control frame
        let ret_label = self.new_label();
        let slots = self.new_slots(&ft.results, "s");
        self.frames.push(Frame {
            kind: FrameKind::Func,
            params: vec![],
            results: ft.results.clone(),
            param_vals: vec![],
            below: vec![],
            end_label: ret_label,
            header_label: String::new(),
            else_label: String::new(),
            slots,
            has_pred: false,
            entry_reachable: true,
            has_else: false,
        });

        for ins in &code.body {
            self.gen_instr(ins)?;
        }

        let body = self.assemble_function(&ft.params, &ft.results);
        self.funcs.push_str(&body);
        Ok(())
    }

    // ------------------------------------------------------------------
    // control flow
    // ------------------------------------------------------------------

    fn gen_instr(&mut self, ins: &Instr) -> Result<(), String> {
        match ins {
            Instr::Block(bt) => self.gen_block(*bt, false),
            Instr::Loop(bt) => self.gen_block(*bt, true),
            Instr::If(bt) => self.gen_if(*bt),
            Instr::Else => self.gen_else(),
            Instr::End => self.gen_end(),
            _ => {
                if !self.reachable {
                    return Ok(());
                }
                self.gen_simple(ins)
            }
        }
    }

    /// Allocate a label's result slots the first time they are needed.
    /// Blocks/ifs that are never targeted keep their results on the SSA stack
    /// and get no merge block, which keeps frames small and IR compact.
    /// Loop frames store their *parameters* in `slots` and always allocate
    /// eagerly, so they are left alone here.
    fn ensure_result_slots(&mut self, fi: usize) {
        if self.frames[fi].kind != FrameKind::Loop
            && self.frames[fi].slots.is_empty()
            && !self.frames[fi].results.is_empty()
        {
            let types = self.frames[fi].results.clone();
            let slots = self.new_slots(&types, "s");
            self.frames[fi].slots = slots;
        }
    }

    fn gen_block(&mut self, bt: BlockType, is_loop: bool) -> Result<(), String> {
        let (params, results) = self
            .m
            .block_sig(bt)
            .ok_or_else(|| "invalid block type".to_string())?;
        if self.reachable {
            let pv = self.pop_vals(&params);
            let below = self.stack.clone();
            if is_loop {
                let slots = self.new_slots(&params, "s");
                for (s, v) in slots.iter().zip(pv.iter()) {
                    self.store_slot(&s.clone(), v, v.ty);
                }
                let header = self.new_label();
                self.term(format!("  br label %{}", header));
                self.start_block(&header);
                let mut loaded = Vec::new();
                for (s, t) in slots.iter().zip(params.iter()) {
                    loaded.push(self.load_slot(&s.clone(), *t));
                }
                self.stack = below.clone();
                self.stack.extend(loaded);
                self.frames.push(Frame {
                    kind: FrameKind::Loop,
                    params,
                    results,
                    param_vals: pv,
                    below,
                    end_label: String::new(),
                    header_label: header,
                    else_label: String::new(),
                    slots,
                    has_pred: true,
                    entry_reachable: true,
                    has_else: false,
                });
            } else {
                let end = self.new_label();
                self.stack = below.clone();
                self.stack.extend(pv.iter().cloned());
                self.frames.push(Frame {
                    kind: FrameKind::Block,
                    params,
                    results,
                    param_vals: pv,
                    below,
                    end_label: end,
                    header_label: String::new(),
                    else_label: String::new(),
                    slots: Vec::new(),
                    has_pred: false,
                    entry_reachable: true,
                    has_else: false,
                });
            }
        } else {
            let end_label = self.new_label();
            let header_label = self.new_label();
            self.frames.push(Frame {
                kind: if is_loop { FrameKind::Loop } else { FrameKind::Block },
                params,
                results,
                param_vals: vec![],
                below: vec![],
                end_label,
                header_label,
                else_label: String::new(),
                slots: Vec::new(),
                has_pred: false,
                entry_reachable: false,
                has_else: false,
            });
        }
        Ok(())
    }

    fn gen_if(&mut self, bt: BlockType) -> Result<(), String> {
        let (params, results) = self
            .m
            .block_sig(bt)
            .ok_or_else(|| "invalid block type".to_string())?;
        if self.reachable {
            let cond = self.pop();
            let pv = self.pop_vals(&params);
            let below = self.stack.clone();
            let then_l = self.new_label();
            let else_l = self.new_label();
            let end_l = self.new_label();
            let c = self.tmp();
            self.emit(format!("  {} = icmp ne i32 {}, 0", c, cond.name));
            self.term(format!(
                "  br i1 {}, label %{}, label %{}",
                c, then_l, else_l
            ));
            self.start_block(&then_l);
            self.stack = below.clone();
            self.stack.extend(pv.iter().cloned());
            self.frames.push(Frame {
                kind: FrameKind::If,
                params,
                results,
                param_vals: pv,
                below,
                end_label: end_l,
                header_label: String::new(),
                else_label: else_l,
                slots: Vec::new(),
                has_pred: false,
                entry_reachable: true,
                has_else: false,
            });
        } else {
            let end_label = self.new_label();
            let else_label = self.new_label();
            self.frames.push(Frame {
                kind: FrameKind::If,
                params,
                results,
                param_vals: vec![],
                below: vec![],
                end_label,
                header_label: String::new(),
                else_label,
                slots: Vec::new(),
                has_pred: false,
                entry_reachable: false,
                has_else: false,
            });
        }
        Ok(())
    }

    fn gen_else(&mut self) -> Result<(), String> {
        let idx = self.frames.len() - 1;
        if self.reachable {
            self.ensure_result_slots(idx);
            let results = self.frames[idx].results.clone();
            let slots = self.frames[idx].slots.clone();
            let end = self.frames[idx].end_label.clone();
            let rv = self.pop_vals(&results);
            for (s, v) in slots.iter().zip(rv.iter()) {
                self.store_slot(&s.clone(), v, v.ty);
            }
            self.term(format!("  br label %{}", end));
            self.frames[idx].has_pred = true;
            self.reachable = false;
        }
        self.frames[idx].kind = FrameKind::Else;
        self.frames[idx].has_else = true;
        if self.frames[idx].entry_reachable {
            let else_l = self.frames[idx].else_label.clone();
            let below = self.frames[idx].below.clone();
            let pv = self.frames[idx].param_vals.clone();
            self.start_block(&else_l);
            self.stack = below;
            self.stack.extend(pv);
            self.reachable = true;
        }
        Ok(())
    }

    fn gen_end(&mut self) -> Result<(), String> {
        let mut frame = self.frames.pop().expect("control stack underflow");
        match frame.kind {
            FrameKind::Func => self.finish_func_end(frame),
            FrameKind::Loop => {
                // fallthrough carries the results; nothing to do
            }
            FrameKind::Block => {
                if !self.reachable {
                    if frame.has_pred {
                        self.start_merge_block(&frame);
                    }
                    return Ok(());
                }
                if !frame.has_pred {
                    // Only the fallthrough reaches this label: its results are
                    // already on the SSA stack and no merge block is needed.
                    return Ok(());
                }
                self.emit_merge_fallthrough(&mut frame);
                self.start_merge_block(&frame);
            }
            FrameKind::If | FrameKind::Else => {
                if frame.kind == FrameKind::If && !frame.has_else {
                    if !frame.entry_reachable {
                        self.reachable = false;
                        return Ok(());
                    }
                    let then_reaches_end = self.reachable;
                    if then_reaches_end {
                        self.emit_merge_fallthrough(&mut frame);
                    }
                    if !then_reaches_end && !frame.has_pred {
                        // The implicit empty else is the only predecessor:
                        // just continue there, keeping the values on the stack.
                        self.start_block(&frame.else_label);
                        self.stack = frame.below.clone();
                        self.stack.extend(frame.param_vals.iter().cloned());
                        self.reachable = true;
                        return Ok(());
                    }
                    // Emit the implicit empty else branch, then the merge.
                    self.start_block(&frame.else_label);
                    self.stack = frame.below.clone();
                    self.stack.extend(frame.param_vals.iter().cloned());
                    self.reachable = true;
                    self.emit_merge_fallthrough(&mut frame);
                    self.start_merge_block(&frame);
                } else if self.reachable {
                    if !frame.has_pred {
                        // Only this fallthrough reaches the end of the if.
                        return Ok(());
                    }
                    self.emit_merge_fallthrough(&mut frame);
                    self.start_merge_block(&frame);
                } else if frame.has_pred {
                    self.start_merge_block(&frame);
                } else {
                    self.reachable = false;
                }
            }
        }
        Ok(())
    }

    /// Store the falling-through results into the frame's slots and jump to
    /// its merge block.  Requires `self.reachable`.
    fn emit_merge_fallthrough(&mut self, frame: &mut Frame) {
        if frame.slots.is_empty() && !frame.results.is_empty() {
            let types = frame.results.clone();
            frame.slots = self.new_slots(&types, "s");
        }
        let rv = self.pop_vals(&frame.results);
        for (s, v) in frame.slots.iter().zip(rv.iter()) {
            self.store_slot(&s.clone(), v, v.ty);
        }
        self.term(format!("  br label %{}", frame.end_label));
        frame.has_pred = true;
        self.reachable = false;
    }

    fn start_merge_block(&mut self, frame: &Frame) {
        self.start_block(&frame.end_label);
        self.stack = frame.below.clone();
        for (s, t) in frame.slots.iter().zip(frame.results.iter()) {
            let v = self.load_slot(&s.clone(), *t);
            self.stack.push(v);
        }
        self.reachable = true;
    }

    fn finish_func_end(&mut self, frame: Frame) {
        let mut has_pred = frame.has_pred;
        if self.reachable {
            let rv = self.pop_vals(&frame.results);
            for (s, v) in frame.slots.iter().zip(rv.iter()) {
                self.store_slot(&s.clone(), v, v.ty);
            }
            self.term(format!("  br label %{}", frame.end_label));
            has_pred = true;
            self.reachable = false;
        }
        if !has_pred {
            return;
        }
        self.start_block(&frame.end_label);
        let mut vals = Vec::new();
        for (s, t) in frame.slots.iter().zip(frame.results.iter()) {
            vals.push(self.load_slot(&s.clone(), *t));
        }
        let d = self.tmp();
        self.emit(format!("  {} = load i32, ptr @w2l.depth, align 4", d));
        let d1 = self.tmp();
        self.emit(format!("  {} = sub i32 {}, 1", d1, d));
        self.emit(format!("  store i32 {}, ptr @w2l.depth, align 4", d1));
        match frame.results.len() {
            0 => self.term("  ret void".to_string()),
            1 => self.term(format!(
                "  ret {} {}",
                ll_ty(frame.results[0]),
                vals[0].name
            )),
            _ => {
                let st = ret_ty(&frame.results);
                let mut acc = String::from("zeroinitializer");
                for (i, t) in frame.results.iter().enumerate() {
                    let nt = self.tmp();
                    self.emit(format!(
                        "  {} = insertvalue {} {}, {} {}, {}",
                        nt,
                        st,
                        acc,
                        ll_ty(*t),
                        vals[i].name,
                        i
                    ));
                    acc = nt;
                }
                self.term(format!("  ret {} {}", st, acc));
            }
        }
        self.reachable = false;
    }

    fn gen_br(&mut self, depth: u32) -> Result<(), String> {
        if !self.reachable {
            return Ok(());
        }
        let n = self.frames.len();
        let d = depth as usize;
        if d >= n {
            return Err(format!("unknown label {}", depth));
        }
        let fi = n - 1 - d;
        let (target, types, is_loop) = {
            let f = &self.frames[fi];
            if f.kind == FrameKind::Loop {
                (f.header_label.clone(), f.params.clone(), true)
            } else {
                (f.end_label.clone(), f.results.clone(), false)
            }
        };
        if !is_loop {
            self.ensure_result_slots(fi);
        }
        let slots = self.frames[fi].slots.clone();
        let args = self.pop_vals(&types);
        for (s, v) in slots.iter().zip(args.iter()) {
            self.store_slot(&s.clone(), v, v.ty);
        }
        self.term(format!("  br label %{}", target));
        if !is_loop {
            self.frames[fi].has_pred = true;
        }
        self.reachable = false;
        Ok(())
    }

    fn gen_br_if(&mut self, depth: u32) -> Result<(), String> {
        let n = self.frames.len();
        let d = depth as usize;
        if d >= n {
            return Err(format!("unknown label {}", depth));
        }
        let fi = n - 1 - d;
        let (target, types, is_loop) = {
            let f = &self.frames[fi];
            if f.kind == FrameKind::Loop {
                (f.header_label.clone(), f.params.clone(), true)
            } else {
                (f.end_label.clone(), f.results.clone(), false)
            }
        };
        if !is_loop {
            self.ensure_result_slots(fi);
        }
        let slots = self.frames[fi].slots.clone();
        let cond = self.pop();
        let args = self.pop_vals(&types);
        let c = self.tmp();
        self.emit(format!("  {} = icmp ne i32 {}, 0", c, cond.name));
        let taken = self.new_label();
        let cont = self.new_label();
        self.term(format!(
            "  br i1 {}, label %{}, label %{}",
            c, taken, cont
        ));
        self.start_block(&taken);
        for (s, v) in slots.iter().zip(args.iter()) {
            self.store_slot(&s.clone(), v, v.ty);
        }
        self.term(format!("  br label %{}", target));
        if !is_loop {
            self.frames[fi].has_pred = true;
        }
        self.start_block(&cont);
        self.reachable = true;
        for a in args {
            self.stack.push(a);
        }
        Ok(())
    }

    fn gen_br_table(&mut self, targets: &[u32], default: u32) -> Result<(), String> {
        let n = self.frames.len();
        let di = default as usize;
        if di >= n {
            return Err(format!("unknown label {}", default));
        }
        let (dtypes, dtarget, dis_loop) = {
            let f = &self.frames[n - 1 - di];
            if f.kind == FrameKind::Loop {
                (f.params.clone(), f.header_label.clone(), true)
            } else {
                (f.results.clone(), f.end_label.clone(), false)
            }
        };
        if !dis_loop {
            self.ensure_result_slots(n - 1 - di);
        }
        let dslots = self.frames[n - 1 - di].slots.clone();
        let idx = self.pop();
        let args = self.pop_vals(&dtypes);

        let dtramp = self.new_label();
        let mut tramps = Vec::new();
        let mut cases = String::new();
        for (i, t) in targets.iter().enumerate() {
            let ti = *t as usize;
            if ti >= n {
                return Err(format!("unknown label {}", t));
            }
            let lab = self.new_label();
            tramps.push((*t, lab.clone()));
            cases.push_str(&format!(" i32 {}, label %{}", i, lab));
        }
        self.term(format!(
            "  switch i32 {}, label %{} [{} ]",
            idx.name, dtramp, cases
        ));
        for (t, lab) in tramps {
            self.start_block(&lab);
            let fi = n - 1 - t as usize;
            let (target, is_loop) = {
                let f = &self.frames[fi];
                if f.kind == FrameKind::Loop {
                    (f.header_label.clone(), true)
                } else {
                    (f.end_label.clone(), false)
                }
            };
            if !is_loop {
                self.ensure_result_slots(fi);
            }
            let slots = self.frames[fi].slots.clone();
            for (s, v) in slots.iter().zip(args.iter()) {
                self.store_slot(&s.clone(), v, v.ty);
            }
            self.term(format!("  br label %{}", target));
            if !is_loop {
                self.frames[fi].has_pred = true;
            }
        }
        self.start_block(&dtramp);
        for (s, v) in dslots.iter().zip(args.iter()) {
            self.store_slot(&s.clone(), v, v.ty);
        }
        self.term(format!("  br label %{}", dtarget));
        if self.frames[n - 1 - di].kind != FrameKind::Loop {
            self.frames[n - 1 - di].has_pred = true;
        }
        self.reachable = false;
        Ok(())
    }

    // ------------------------------------------------------------------
    // simple instructions
    // ------------------------------------------------------------------

    fn gen_simple(&mut self, ins: &Instr) -> Result<(), String> {
        match ins {
            Instr::Unreachable => {
                self.emit("  call void @w2l_trap(i32 0)".to_string());
                self.term("  unreachable".to_string());
                self.reachable = false;
            }
            Instr::Nop => {}
            Instr::Br(d) => self.gen_br(*d)?,
            Instr::BrIf(d) => self.gen_br_if(*d)?,
            Instr::BrTable(t, d) => self.gen_br_table(t, *d)?,
            Instr::Return => {
                let d = (self.frames.len() - 1) as u32;
                self.gen_br(d)?;
            }
            Instr::Call(idx) => self.gen_call(*idx)?,
            Instr::CallIndirect { ty, table } => self.gen_call_indirect(*ty, *table)?,
            Instr::Drop => {
                self.pop();
            }
            Instr::Select => {
                let c = self.pop();
                let b = self.pop();
                let a = self.pop();
                let cc = self.tmp();
                self.emit(format!("  {} = icmp ne i32 {}, 0", cc, c.name));
                let t = self.tmp();
                self.emit(format!(
                    "  {} = select i1 {}, {} {}, {} {}",
                    t,
                    cc,
                    ll_ty(a.ty),
                    a.name,
                    ll_ty(b.ty),
                    b.name
                ));
                self.push(Val { name: t, ty: a.ty });
            }
            Instr::LocalGet(i) => {
                let t = self.locals[*i as usize];
                let p = self.local_ptr[*i as usize].clone();
                let v = self.load_local(&p, t);
                self.push(v);
            }
            Instr::LocalSet(i) => {
                let t = self.locals[*i as usize];
                let p = self.local_ptr[*i as usize].clone();
                let v = self.pop();
                self.store_slot(&p, &v, t);
            }
            Instr::LocalTee(i) => {
                let t = self.locals[*i as usize];
                let p = self.local_ptr[*i as usize].clone();
                let v = self.pop();
                self.store_slot(&p, &v, t);
                self.push(v);
            }
            Instr::GlobalGet(i) => {
                let t = self.m.globals[*i as usize].ty;
                let g = format!("@g{}", i);
                let tmp = self.tmp();
                self.emit(format!(
                    "  {} = load {}, ptr {}, align {}",
                    tmp,
                    ll_ty(t),
                    g,
                    align_of(t)
                ));
                self.push(Val { name: tmp, ty: t });
            }
            Instr::GlobalSet(i) => {
                let t = self.m.globals[*i as usize].ty;
                let v = self.pop();
                self.emit(format!(
                    "  store {} {}, ptr @g{}, align {}",
                    ll_ty(t),
                    v.name,
                    i,
                    align_of(t)
                ));
            }
            Instr::Load { op, offset, .. } => self.gen_load(*op, *offset),
            Instr::Store { op, offset, .. } => self.gen_store(*op, *offset),
            Instr::MemorySize => {
                let sz = self.tmp();
                self.emit(format!("  {} = load i64, ptr @mem.size, align 8", sz));
                let p = self.tmp();
                self.emit(format!("  {} = lshr i64 {}, 16", p, sz));
                let r = self.tmp();
                self.emit(format!("  {} = trunc i64 {} to i32", r, p));
                self.push(Val { name: r, ty: ValType::I32 });
            }
            Instr::MemoryGrow => self.gen_memory_grow(),
            Instr::I32Const(v) => self.push(Val {
                name: format!("{}", v),
                ty: ValType::I32,
            }),
            Instr::I64Const(v) => self.push(Val {
                name: format!("{}", v),
                ty: ValType::I64,
            }),
            Instr::F32Const(bits) => self.push(Val {
                name: format!("f0x{:08x}", bits),
                ty: ValType::F32,
            }),
            Instr::F64Const(bits) => self.push(Val {
                name: format!("f0x{:016x}", bits),
                ty: ValType::F64,
            }),
            Instr::Num(op) => self.gen_num(*op)?,
            Instr::TruncSat(sub) => self.gen_trunc_sat(*sub)?,
            Instr::MemoryInit(d) => self.gen_memory_init(*d)?,
            Instr::DataDrop(d) => {
                self.emit(format!("  store i32 0, ptr @dlen{}, align 4", d));
            }
            Instr::MemoryCopy => self.gen_memory_copy()?,
            Instr::MemoryFill => self.gen_memory_fill()?,
            Instr::TableInit { elem, .. } => self.gen_table_init(*elem)?,
            Instr::ElemDrop(e) => {
                self.emit(format!("  store i32 0, ptr @elen{}, align 4", e));
            }
            Instr::TableCopy { .. } => self.gen_table_copy()?,
            Instr::Simd(s) => self.gen_simd(s)?,
            Instr::Block(_) | Instr::Loop(_) | Instr::If(_) | Instr::Else | Instr::End => {
                unreachable!()
            }
        }
        Ok(())
    }

    fn load_local(&mut self, p: &str, t: ValType) -> Val {
        let tmp = self.tmp();
        self.emit(format!(
            "  {} = load {}, ptr {}, align {}",
            tmp,
            ll_ty(t),
            p,
            align_of(t)
        ));
        Val { name: tmp, ty: t }
    }

    // ------------------------------------------------------------------
    // calls
    // ------------------------------------------------------------------

    fn gen_call(&mut self, idx: u32) -> Result<(), String> {
        let ft = self
            .m
            .func_type(idx)
            .ok_or_else(|| format!("unknown function {}", idx))?
            .clone();
        let args = self.pop_vals(&ft.params);
        self.emit_direct_call(idx, &ft, &args);
        Ok(())
    }

    fn emit_direct_call(&mut self, idx: u32, ft: &FuncType, args: &[Val]) {
        let argstr: Vec<String> = args
            .iter()
            .map(|v| format!("{} {}", ll_ty(v.ty), v.name))
            .collect();
        let argstr = argstr.join(", ");
        match ft.results.len() {
            0 => {
                self.emit(format!("  call void @f{}({})", idx, argstr));
            }
            1 => {
                let t = self.tmp();
                self.emit(format!(
                    "  {} = call {} @f{}({})",
                    t,
                    ll_ty(ft.results[0]),
                    idx,
                    argstr
                ));
                self.push(Val { name: t, ty: ft.results[0] });
            }
            _ => {
                let st = ret_ty(&ft.results);
                let r = self.tmp();
                self.emit(format!("  {} = call {} @f{}({})", r, st, idx, argstr));
                for (i, t) in ft.results.iter().enumerate() {
                    let e = self.tmp();
                    self.emit(format!("  {} = extractvalue {} {}, {}", e, st, r, i));
                    self.push(Val { name: e, ty: *t });
                }
            }
        }
    }

    fn gen_call_indirect(&mut self, ty: u32, table: u32) -> Result<(), String> {
        let ft = self
            .m
            .types
            .get(ty as usize)
            .ok_or_else(|| format!("unknown type {}", ty))?
            .clone();
        let _ = table;
        let idxv = self.pop();
        let args = self.pop_vals(&ft.params);
        let i64v = self.tmp();
        self.emit(format!("  {} = zext i32 {} to i64", i64v, idxv.name));
        let sz = self.tmp();
        self.emit(format!("  {} = load i64, ptr @tbl.size, align 8", sz));
        let oob = self.tmp();
        self.emit(format!("  {} = icmp uge i64 {}, {}", oob, i64v, sz));
        self.trap_if(&oob, 5);
        let base = self.tmp();
        self.emit(format!("  {} = load ptr, ptr @tbl.base, align 8", base));
        let ep = self.tmp();
        self.emit(format!(
            "  {} = getelementptr ptr, ptr {}, i64 {}",
            ep, base, i64v
        ));
        let fp = self.tmp();
        self.emit(format!("  {} = load ptr, ptr {}, align 8", fp, ep));
        let isnull = self.tmp();
        self.emit(format!("  {} = icmp eq ptr {}, null", isnull, fp));
        self.trap_if(&isnull, 6);
        let tb = self.tmp();
        self.emit(format!("  {} = load ptr, ptr @tbl.types, align 8", tb));
        let tep = self.tmp();
        self.emit(format!(
            "  {} = getelementptr i32, ptr {}, i64 {}",
            tep, tb, i64v
        ));
        let tid = self.tmp();
        self.emit(format!("  {} = load i32, ptr {}, align 4", tid, tep));
        let want = self.canon[ty as usize];
        let mismatch = self.tmp();
        self.emit(format!(
            "  {} = icmp ne i32 {}, {}",
            mismatch, tid, want
        ));
        self.trap_if(&mismatch, 7);

        let argstr: Vec<String> = args
            .iter()
            .map(|v| format!("{} {}", ll_ty(v.ty), v.name))
            .collect();
        let argstr = argstr.join(", ");
        match ft.results.len() {
            0 => {
                self.emit(format!("  call void {}({})", fp, argstr));
            }
            1 => {
                let t = self.tmp();
                self.emit(format!(
                    "  {} = call {} {}({})",
                    t,
                    ll_ty(ft.results[0]),
                    fp,
                    argstr
                ));
                self.push(Val { name: t, ty: ft.results[0] });
            }
            _ => {
                let st = ret_ty(&ft.results);
                let r = self.tmp();
                self.emit(format!("  {} = call {} {}({})", r, st, fp, argstr));
                for (i, t) in ft.results.iter().enumerate() {
                    let e = self.tmp();
                    self.emit(format!("  {} = extractvalue {} {}, {}", e, st, r, i));
                    self.push(Val { name: e, ty: *t });
                }
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // memory
    // ------------------------------------------------------------------

    fn mem_ptr(&mut self, offset: u64, size: u64) -> String {
        let addr = self.pop();
        let a64 = self.tmp();
        self.emit(format!("  {} = zext i32 {} to i64", a64, addr.name));
        let ea = self.tmp();
        self.emit(format!("  {} = add i64 {}, {}", ea, a64, offset));
        let end = self.tmp();
        self.emit(format!("  {} = add i64 {}, {}", end, ea, size));
        let sz = self.tmp();
        self.emit(format!("  {} = load i64, ptr @mem.size, align 8", sz));
        let bad = self.tmp();
        self.emit(format!("  {} = icmp ugt i64 {}, {}", bad, end, sz));
        self.trap_if(&bad, 4);
        let base = self.tmp();
        self.emit(format!("  {} = load ptr, ptr @mem.base, align 8", base));
        let p = self.tmp();
        self.emit(format!(
            "  {} = getelementptr i8, ptr {}, i64 {}",
            p, base, ea
        ));
        p
    }

    fn gen_load(&mut self, op: u8, offset: u64) {
        let (ty, size) = crate::validate::load_sig(op).unwrap();
        let p = self.mem_ptr(offset, size);
        let t = self.tmp();
        match op {
            0x28 => self.emit(format!("  {} = load i32, ptr {}, align 1", t, p)),
            0x29 => self.emit(format!("  {} = load i64, ptr {}, align 1", t, p)),
            0x2a => self.emit(format!("  {} = load float, ptr {}, align 1", t, p)),
            0x2b => self.emit(format!("  {} = load double, ptr {}, align 1", t, p)),
            0x2c => self.emit(format!("  {} = load i8, ptr {}, align 1", t, p)),
            0x2d => self.emit(format!("  {} = load i8, ptr {}, align 1", t, p)),
            0x2e => self.emit(format!("  {} = load i16, ptr {}, align 1", t, p)),
            0x2f => self.emit(format!("  {} = load i16, ptr {}, align 1", t, p)),
            0x30 => self.emit(format!("  {} = load i8, ptr {}, align 1", t, p)),
            0x31 => self.emit(format!("  {} = load i8, ptr {}, align 1", t, p)),
            0x32 => self.emit(format!("  {} = load i16, ptr {}, align 1", t, p)),
            0x33 => self.emit(format!("  {} = load i16, ptr {}, align 1", t, p)),
            0x34 => self.emit(format!("  {} = load i32, ptr {}, align 1", t, p)),
            0x35 => self.emit(format!("  {} = load i32, ptr {}, align 1", t, p)),
            _ => unreachable!(),
        }
        let out = match op {
            0x28 | 0x29 | 0x2a | 0x2b => Val { name: t, ty },
            0x2c | 0x2e => {
                let e = self.tmp();
                let src = if op == 0x2c { "i8" } else { "i16" };
                self.emit(format!("  {} = sext {} {} to i32", e, src, t));
                Val { name: e, ty: ValType::I32 }
            }
            0x2d | 0x2f => {
                let e = self.tmp();
                let src = if op == 0x2d { "i8" } else { "i16" };
                self.emit(format!("  {} = zext {} {} to i32", e, src, t));
                Val { name: e, ty: ValType::I32 }
            }
            0x30 | 0x32 => {
                let e = self.tmp();
                let src = if op == 0x30 { "i8" } else { "i16" };
                self.emit(format!("  {} = sext {} {} to i64", e, src, t));
                Val { name: e, ty: ValType::I64 }
            }
            0x31 | 0x33 => {
                let e = self.tmp();
                let src = if op == 0x31 { "i8" } else { "i16" };
                self.emit(format!("  {} = zext {} {} to i64", e, src, t));
                Val { name: e, ty: ValType::I64 }
            }
            0x34 => {
                let e = self.tmp();
                self.emit(format!("  {} = sext i32 {} to i64", e, t));
                Val { name: e, ty: ValType::I64 }
            }
            0x35 => {
                let e = self.tmp();
                self.emit(format!("  {} = zext i32 {} to i64", e, t));
                Val { name: e, ty: ValType::I64 }
            }
            _ => unreachable!(),
        };
        self.push(out);
    }

    fn gen_store(&mut self, op: u8, offset: u64) {
        let (_ty, size) = crate::validate::store_sig(op).unwrap();
        let v = self.pop();
        let p = self.mem_ptr(offset, size);
        match op {
            0x36 => self.emit(format!("  store i32 {}, ptr {}, align 1", v.name, p)),
            0x37 => self.emit(format!("  store i64 {}, ptr {}, align 1", v.name, p)),
            0x38 => self.emit(format!("  store float {}, ptr {}, align 1", v.name, p)),
            0x39 => self.emit(format!("  store double {}, ptr {}, align 1", v.name, p)),
            0x3a => {
                let t = self.tmp();
                self.emit(format!("  {} = trunc i32 {} to i8", t, v.name));
                self.emit(format!("  store i8 {}, ptr {}, align 1", t, p));
            }
            0x3b => {
                let t = self.tmp();
                self.emit(format!("  {} = trunc i32 {} to i16", t, v.name));
                self.emit(format!("  store i16 {}, ptr {}, align 1", t, p));
            }
            0x3c => {
                let t = self.tmp();
                self.emit(format!("  {} = trunc i64 {} to i8", t, v.name));
                self.emit(format!("  store i8 {}, ptr {}, align 1", t, p));
            }
            0x3d => {
                let t = self.tmp();
                self.emit(format!("  {} = trunc i64 {} to i16", t, v.name));
                self.emit(format!("  store i16 {}, ptr {}, align 1", t, p));
            }
            0x3e => {
                let t = self.tmp();
                self.emit(format!("  {} = trunc i64 {} to i32", t, v.name));
                self.emit(format!("  store i32 {}, ptr {}, align 1", t, p));
            }
            _ => unreachable!(),
        }
    }

    fn gen_memory_grow(&mut self) {
        let delta = self.pop();
        let dp = self.tmp();
        self.emit(format!("  {} = zext i32 {} to i64", dp, delta.name));
        let cur = self.tmp();
        self.emit(format!("  {} = load i64, ptr @mem.size, align 8", cur));
        let curp = self.tmp();
        self.emit(format!("  {} = lshr i64 {}, 16", curp, cur));
        let newp = self.tmp();
        self.emit(format!("  {} = add i64 {}, {}", newp, curp, dp));
        let maxp: u64 = match self.m.memories.first() {
            Some(mm) => mm.max.unwrap_or(65536),
            None => 65536,
        };
        let over = self.tmp();
        self.emit(format!("  {} = icmp ugt i64 {}, {}", over, newp, maxp));
        let slot = self.alloca("i32", 4, "gr");
        self.emit(format!("  store i32 -1, ptr {}, align 4", slot));
        let try_l = self.new_label();
        let skip_l = self.new_label();
        let zero_l = self.new_label();
        let iszero = self.tmp();
        self.emit(format!("  {} = icmp eq i64 {}, 0", iszero, dp));
        self.term(format!(
            "  br i1 {}, label %{}, label %{}",
            iszero, zero_l, try_l
        ));
        self.start_block(&zero_l);
        let cur32 = self.tmp();
        self.emit(format!("  {} = trunc i64 {} to i32", cur32, curp));
        self.emit(format!("  store i32 {}, ptr {}, align 4", cur32, slot));
        self.term(format!("  br label %{}", skip_l));
        self.start_block(&try_l);
        let over_l = self.new_label();
        self.term(format!(
            "  br i1 {}, label %{}, label %{}",
            over, skip_l, over_l
        ));
        self.start_block(&over_l);
        let nb = self.tmp();
        self.emit(format!("  {} = shl i64 {}, 16", nb, newp));
        let ok = self.tmp();
        self.emit(format!(
            "  {} = call i32 @w2l.mem_grow(i64 {})",
            ok, nb
        ));
        let okb = self.tmp();
        self.emit(format!("  {} = icmp ne i32 {}, 0", okb, ok));
        let old32 = self.tmp();
        self.emit(format!("  {} = trunc i64 {} to i32", old32, curp));
        let res = self.tmp();
        self.emit(format!(
            "  {} = select i1 {}, i32 {}, i32 -1",
            res, okb, old32
        ));
        self.emit(format!("  store i32 {}, ptr {}, align 4", res, slot));
        self.term(format!("  br label %{}", skip_l));
        self.start_block(&skip_l);
        let r = self.tmp();
        self.emit(format!("  {} = load i32, ptr {}, align 4", r, slot));
        self.push(Val { name: r, ty: ValType::I32 });
    }

    // ------------------------------------------------------------------
    // bulk memory / tables
    // ------------------------------------------------------------------

    fn gen_memory_init(&mut self, d: u32) -> Result<(), String> {
        let n = self.pop();
        let s = self.pop();
        let dst = self.pop();
        let dlen = self.tmp();
        self.emit(format!("  {} = load i32, ptr @dlen{}, align 4", dlen, d));
        let de = self.tmp();
        self.emit(format!("  {} = zext i32 {} to i64", de, dlen));
        let sz = self.tmp();
        self.emit(format!("  {} = load i64, ptr @mem.size, align 8", sz));
        let dn = self.tmp();
        self.emit(format!("  {} = zext i32 {} to i64", dn, dst.name));
        let sn = self.tmp();
        self.emit(format!("  {} = zext i32 {} to i64", sn, s.name));
        let nn = self.tmp();
        self.emit(format!("  {} = zext i32 {} to i64", nn, n.name));
        let dend = self.tmp();
        self.emit(format!("  {} = add i64 {}, {}", dend, dn, nn));
        let send = self.tmp();
        self.emit(format!("  {} = add i64 {}, {}", send, sn, nn));
        let bad1 = self.tmp();
        self.emit(format!("  {} = icmp ugt i64 {}, {}", bad1, dend, sz));
        let bad2 = self.tmp();
        self.emit(format!("  {} = icmp ugt i64 {}, {}", bad2, send, de));
        let bad = self.tmp();
        self.emit(format!("  {} = or i1 {}, {}", bad, bad1, bad2));
        self.trap_if(&bad, 4);
        let base = self.tmp();
        self.emit(format!("  {} = load ptr, ptr @mem.base, align 8", base));
        let dp = self.tmp();
        self.emit(format!(
            "  {} = getelementptr i8, ptr {}, i64 {}",
            dp, base, dn
        ));
        let sp = self.tmp();
        self.emit(format!(
            "  {} = getelementptr i8, ptr @d{}, i64 {}",
            sp, d, sn
        ));
        self.call_memcpy(&dp, &sp, &nn);
        Ok(())
    }

    fn gen_memory_copy(&mut self) -> Result<(), String> {
        let n = self.pop();
        let s = self.pop();
        let dst = self.pop();
        let sz = self.tmp();
        self.emit(format!("  {} = load i64, ptr @mem.size, align 8", sz));
        let dn = self.tmp();
        self.emit(format!("  {} = zext i32 {} to i64", dn, dst.name));
        let sn = self.tmp();
        self.emit(format!("  {} = zext i32 {} to i64", sn, s.name));
        let nn = self.tmp();
        self.emit(format!("  {} = zext i32 {} to i64", nn, n.name));
        let dend = self.tmp();
        self.emit(format!("  {} = add i64 {}, {}", dend, dn, nn));
        let send = self.tmp();
        self.emit(format!("  {} = add i64 {}, {}", send, sn, nn));
        let bad1 = self.tmp();
        self.emit(format!("  {} = icmp ugt i64 {}, {}", bad1, dend, sz));
        let bad2 = self.tmp();
        self.emit(format!("  {} = icmp ugt i64 {}, {}", bad2, send, sz));
        let bad = self.tmp();
        self.emit(format!("  {} = or i1 {}, {}", bad, bad1, bad2));
        self.trap_if(&bad, 4);
        let base = self.tmp();
        self.emit(format!("  {} = load ptr, ptr @mem.base, align 8", base));
        let dp = self.tmp();
        self.emit(format!(
            "  {} = getelementptr i8, ptr {}, i64 {}",
            dp, base, dn
        ));
        let sp = self.tmp();
        self.emit(format!(
            "  {} = getelementptr i8, ptr {}, i64 {}",
            sp, base, sn
        ));
        self.call_memmove(&dp, &sp, &nn);
        Ok(())
    }

    fn gen_memory_fill(&mut self) -> Result<(), String> {
        let n = self.pop();
        let val = self.pop();
        let dst = self.pop();
        let sz = self.tmp();
        self.emit(format!("  {} = load i64, ptr @mem.size, align 8", sz));
        let dn = self.tmp();
        self.emit(format!("  {} = zext i32 {} to i64", dn, dst.name));
        let nn = self.tmp();
        self.emit(format!("  {} = zext i32 {} to i64", nn, n.name));
        let dend = self.tmp();
        self.emit(format!("  {} = add i64 {}, {}", dend, dn, nn));
        let bad = self.tmp();
        self.emit(format!("  {} = icmp ugt i64 {}, {}", bad, dend, sz));
        self.trap_if(&bad, 4);
        let base = self.tmp();
        self.emit(format!("  {} = load ptr, ptr @mem.base, align 8", base));
        let dp = self.tmp();
        self.emit(format!(
            "  {} = getelementptr i8, ptr {}, i64 {}",
            dp, base, dn
        ));
        let b = self.tmp();
        self.emit(format!("  {} = trunc i32 {} to i8", b, val.name));
        self.call_memset(&dp, &b, &nn);
        Ok(())
    }

    fn gen_table_init(&mut self, e: u32) -> Result<(), String> {
        let n = self.pop();
        let s = self.pop();
        let dst = self.pop();
        let elen = self.tmp();
        self.emit(format!("  {} = load i32, ptr @elen{}, align 4", elen, e));
        let el = self.tmp();
        self.emit(format!("  {} = zext i32 {} to i64", el, elen));
        let sz = self.tmp();
        self.emit(format!("  {} = load i64, ptr @tbl.size, align 8", sz));
        let dn = self.tmp();
        self.emit(format!("  {} = zext i32 {} to i64", dn, dst.name));
        let sn = self.tmp();
        self.emit(format!("  {} = zext i32 {} to i64", sn, s.name));
        let nn = self.tmp();
        self.emit(format!("  {} = zext i32 {} to i64", nn, n.name));
        let dend = self.tmp();
        self.emit(format!("  {} = add i64 {}, {}", dend, dn, nn));
        let send = self.tmp();
        self.emit(format!("  {} = add i64 {}, {}", send, sn, nn));
        let bad1 = self.tmp();
        self.emit(format!("  {} = icmp ugt i64 {}, {}", bad1, dend, sz));
        let bad2 = self.tmp();
        self.emit(format!("  {} = icmp ugt i64 {}, {}", bad2, send, el));
        let bad = self.tmp();
        self.emit(format!("  {} = or i1 {}, {}", bad, bad1, bad2));
        self.trap_if(&bad, 5);
        // ptr bytes: 8 per element
        let tb = self.tmp();
        self.emit(format!("  {} = load ptr, ptr @tbl.base, align 8", tb));
        let doff = self.tmp();
        self.emit(format!("  {} = shl i64 {}, 3", doff, dn));
        let soff = self.tmp();
        self.emit(format!("  {} = shl i64 {}, 3", soff, sn));
        let nb = self.tmp();
        self.emit(format!("  {} = shl i64 {}, 3", nb, nn));
        let dp = self.tmp();
        self.emit(format!(
            "  {} = getelementptr i8, ptr {}, i64 {}",
            dp, tb, doff
        ));
        let sp = self.tmp();
        self.emit(format!(
            "  {} = getelementptr i8, ptr @e{}_p, i64 {}",
            sp, e, soff
        ));
        self.call_memcpy(&dp, &sp, &nb);
        // type ids: 4 per element
        let tt = self.tmp();
        self.emit(format!("  {} = load ptr, ptr @tbl.types, align 8", tt));
        let doff2 = self.tmp();
        self.emit(format!("  {} = shl i64 {}, 2", doff2, dn));
        let soff2 = self.tmp();
        self.emit(format!("  {} = shl i64 {}, 2", soff2, sn));
        let nb2 = self.tmp();
        self.emit(format!("  {} = shl i64 {}, 2", nb2, nn));
        let dp2 = self.tmp();
        self.emit(format!(
            "  {} = getelementptr i8, ptr {}, i64 {}",
            dp2, tt, doff2
        ));
        let sp2 = self.tmp();
        self.emit(format!(
            "  {} = getelementptr i8, ptr @e{}_t, i64 {}",
            sp2, e, soff2
        ));
        self.call_memcpy(&dp2, &sp2, &nb2);
        Ok(())
    }

    fn gen_table_copy(&mut self) -> Result<(), String> {
        let n = self.pop();
        let s = self.pop();
        let dst = self.pop();
        let sz = self.tmp();
        self.emit(format!("  {} = load i64, ptr @tbl.size, align 8", sz));
        let dn = self.tmp();
        self.emit(format!("  {} = zext i32 {} to i64", dn, dst.name));
        let sn = self.tmp();
        self.emit(format!("  {} = zext i32 {} to i64", sn, s.name));
        let nn = self.tmp();
        self.emit(format!("  {} = zext i32 {} to i64", nn, n.name));
        let dend = self.tmp();
        self.emit(format!("  {} = add i64 {}, {}", dend, dn, nn));
        let send = self.tmp();
        self.emit(format!("  {} = add i64 {}, {}", send, sn, nn));
        let bad1 = self.tmp();
        self.emit(format!("  {} = icmp ugt i64 {}, {}", bad1, dend, sz));
        let bad2 = self.tmp();
        self.emit(format!("  {} = icmp ugt i64 {}, {}", bad2, send, sz));
        let bad = self.tmp();
        self.emit(format!("  {} = or i1 {}, {}", bad, bad1, bad2));
        self.trap_if(&bad, 5);
        let tb = self.tmp();
        self.emit(format!("  {} = load ptr, ptr @tbl.base, align 8", tb));
        let doff = self.tmp();
        self.emit(format!("  {} = shl i64 {}, 3", doff, dn));
        let soff = self.tmp();
        self.emit(format!("  {} = shl i64 {}, 3", soff, sn));
        let nb = self.tmp();
        self.emit(format!("  {} = shl i64 {}, 3", nb, nn));
        let dp = self.tmp();
        self.emit(format!(
            "  {} = getelementptr i8, ptr {}, i64 {}",
            dp, tb, doff
        ));
        let sp = self.tmp();
        self.emit(format!(
            "  {} = getelementptr i8, ptr {}, i64 {}",
            sp, tb, soff
        ));
        self.call_memmove(&dp, &sp, &nb);
        let tt = self.tmp();
        self.emit(format!("  {} = load ptr, ptr @tbl.types, align 8", tt));
        let doff2 = self.tmp();
        self.emit(format!("  {} = shl i64 {}, 2", doff2, dn));
        let soff2 = self.tmp();
        self.emit(format!("  {} = shl i64 {}, 2", soff2, sn));
        let nb2 = self.tmp();
        self.emit(format!("  {} = shl i64 {}, 2", nb2, nn));
        let dp2 = self.tmp();
        self.emit(format!(
            "  {} = getelementptr i8, ptr {}, i64 {}",
            dp2, tt, doff2
        ));
        let sp2 = self.tmp();
        self.emit(format!(
            "  {} = getelementptr i8, ptr {}, i64 {}",
            sp2, tt, soff2
        ));
        self.call_memmove(&dp2, &sp2, &nb2);
        Ok(())
    }

    // ------------------------------------------------------------------
    // numeric operations
    // ------------------------------------------------------------------

    fn gen_num(&mut self, op: u16) -> Result<(), String> {
        use ValType::*;
        match op {
            0x45 => {
                let a = self.pop();
                let c = self.tmp();
                self.emit(format!("  {} = icmp eq i32 {}, 0", c, a.name));
                self.zext_i1(c);
            }
            0x46..=0x4f => {
                let pred = match op {
                    0x46 => "eq",
                    0x47 => "ne",
                    0x48 => "slt",
                    0x49 => "ult",
                    0x4a => "sgt",
                    0x4b => "ugt",
                    0x4c => "sle",
                    0x4d => "ule",
                    0x4e => "sge",
                    _ => "uge",
                };
                let b = self.pop();
                let a = self.pop();
                let c = self.tmp();
                self.emit(format!("  {} = icmp {} i32 {}, {}", c, pred, a.name, b.name));
                self.zext_i1(c);
            }
            0x50 => {
                let a = self.pop();
                let c = self.tmp();
                self.emit(format!("  {} = icmp eq i64 {}, 0", c, a.name));
                self.zext_i1(c);
            }
            0x51..=0x5a => {
                let pred = match op {
                    0x51 => "eq",
                    0x52 => "ne",
                    0x53 => "slt",
                    0x54 => "ult",
                    0x55 => "sgt",
                    0x56 => "ugt",
                    0x57 => "sle",
                    0x58 => "ule",
                    0x59 => "sge",
                    _ => "uge",
                };
                let b = self.pop();
                let a = self.pop();
                let c = self.tmp();
                self.emit(format!("  {} = icmp {} i64 {}, {}", c, pred, a.name, b.name));
                self.zext_i1(c);
            }
            0x5b..=0x60 => self.fcmp_group(F32, op - 0x5b),
            0x61..=0x66 => self.fcmp_group(F64, op - 0x61),
            0x67..=0x69 => {
                let base = match op {
                    0x67 => "ctlz",
                    0x68 => "cttz",
                    _ => "ctpop",
                };
                let a = self.pop();
                let name = format!("llvm.{}.i32", base);
                if base == "ctpop" {
                    let r = self.call_intr("i32", &name, "i32", &format!("i32 {}", a.name));
                    self.push(Val { name: r, ty: I32 });
                } else {
                    let r = self.call_intr(
                        "i32",
                        &name,
                        "i32, i1",
                        &format!("i32 {}, i1 false", a.name),
                    );
                    self.push(Val { name: r, ty: I32 });
                }
            }
            0x6a..=0x78 => self.i32_bin(op),
            0x79..=0x7b => {
                let base = match op {
                    0x79 => "ctlz",
                    0x7a => "cttz",
                    _ => "ctpop",
                };
                let a = self.pop();
                let name = format!("llvm.{}.i64", base);
                if base == "ctpop" {
                    let r = self.call_intr("i64", &name, "i64", &format!("i64 {}", a.name));
                    self.push(Val { name: r, ty: I64 });
                } else {
                    let r = self.call_intr(
                        "i64",
                        &name,
                        "i64, i1",
                        &format!("i64 {}, i1 false", a.name),
                    );
                    self.push(Val { name: r, ty: I64 });
                }
            }
            0x7c..=0x8a => self.i64_bin(op),
            0x8b..=0x98 => self.fbin(F32, op - 0x8b)?,
            0x99..=0xa6 => self.fbin(F64, op - 0x99)?,
            0xa7 => {
                let a = self.pop();
                let t = self.tmp();
                self.emit(format!("  {} = trunc i64 {} to i32", t, a.name));
                self.push(Val { name: t, ty: I32 });
            }
            0xa8 => self.trunc_int(F32, I32, true)?,
            0xa9 => self.trunc_int(F32, I32, false)?,
            0xaa => self.trunc_int(F64, I32, true)?,
            0xab => self.trunc_int(F64, I32, false)?,
            0xac => {
                let a = self.pop();
                let t = self.tmp();
                self.emit(format!("  {} = sext i32 {} to i64", t, a.name));
                self.push(Val { name: t, ty: I64 });
            }
            0xad => {
                let a = self.pop();
                let t = self.tmp();
                self.emit(format!("  {} = zext i32 {} to i64", t, a.name));
                self.push(Val { name: t, ty: I64 });
            }
            0xae => self.trunc_int(F32, I64, true)?,
            0xaf => self.trunc_int(F32, I64, false)?,
            0xb0 => self.trunc_int(F64, I64, true)?,
            0xb1 => self.trunc_int(F64, I64, false)?,
            0xb2 => self.int_to_float(I32, F32, true),
            0xb3 => self.int_to_float(I32, F32, false),
            0xb4 => self.int_to_float(I64, F32, true),
            0xb5 => self.int_to_float(I64, F32, false),
            0xb6 => {
                let a = self.pop();
                let t = self.tmp();
                self.emit(format!("  {} = fptrunc double {} to float", t, a.name));
                self.push(Val { name: t, ty: F32 });
            }
            0xb7 => self.int_to_float(I32, F64, true),
            0xb8 => self.int_to_float(I32, F64, false),
            0xb9 => self.int_to_float(I64, F64, true),
            0xba => self.int_to_float(I64, F64, false),
            0xbb => {
                let a = self.pop();
                let t = self.tmp();
                self.emit(format!("  {} = fpext float {} to double", t, a.name));
                self.push(Val { name: t, ty: F64 });
            }
            0xbc => {
                let a = self.pop();
                let t = self.tmp();
                self.emit(format!("  {} = bitcast float {} to i32", t, a.name));
                self.push(Val { name: t, ty: I32 });
            }
            0xbd => {
                let a = self.pop();
                let t = self.tmp();
                self.emit(format!("  {} = bitcast double {} to i64", t, a.name));
                self.push(Val { name: t, ty: I64 });
            }
            0xbe => {
                let a = self.pop();
                let t = self.tmp();
                self.emit(format!("  {} = bitcast i32 {} to float", t, a.name));
                self.push(Val { name: t, ty: F32 });
            }
            0xbf => {
                let a = self.pop();
                let t = self.tmp();
                self.emit(format!("  {} = bitcast i64 {} to double", t, a.name));
                self.push(Val { name: t, ty: F64 });
            }
            0xc0 => self.extend_s(I32, 8),
            0xc1 => self.extend_s(I32, 16),
            0xc2 => self.extend_s(I64, 8),
            0xc3 => self.extend_s(I64, 16),
            0xc4 => self.extend_s(I64, 32),
            _ => return Err(format!("unimplemented numeric opcode 0x{:x}", op)),
        }
        Ok(())
    }

    fn zext_i1(&mut self, c: String) {
        let t = self.tmp();
        self.emit(format!("  {} = zext i1 {} to i32", t, c));
        self.push(Val { name: t, ty: ValType::I32 });
    }

    fn fcmp_group(&mut self, ty: ValType, k: u16) {
        let pred = match k {
            0 => "oeq",
            1 => "une",
            2 => "olt",
            3 => "ogt",
            4 => "ole",
            _ => "oge",
        };
        let b = self.pop();
        let a = self.pop();
        let c = self.tmp();
        self.emit(format!(
            "  {} = fcmp {} {} {}, {}",
            c,
            pred,
            ll_ty(ty),
            a.name,
            b.name
        ));
        self.zext_i1(c);
    }

    fn i32_bin(&mut self, op: u16) {
        match op {
            0x6a => self.binop("i32", "add"),
            0x6b => self.binop("i32", "sub"),
            0x6c => self.binop("i32", "mul"),
            0x6d => self.div_s("i32", 32),
            0x6e => self.div_u("i32", 32),
            0x6f => self.rem_s("i32", 32),
            0x70 => self.rem_u("i32", 32),
            0x71 => self.binop("i32", "and"),
            0x72 => self.binop("i32", "or"),
            0x73 => self.binop("i32", "xor"),
            0x74 => self.shift("i32", 32, "shl"),
            0x75 => self.shift("i32", 32, "ashr"),
            0x76 => self.shift("i32", 32, "lshr"),
            0x77 => self.rotate("i32", 32, true),
            0x78 => self.rotate("i32", 32, false),
            _ => unreachable!(),
        }
    }

    fn i64_bin(&mut self, op: u16) {
        match op {
            0x7c => self.binop("i64", "add"),
            0x7d => self.binop("i64", "sub"),
            0x7e => self.binop("i64", "mul"),
            0x7f => self.div_s("i64", 64),
            0x80 => self.div_u("i64", 64),
            0x81 => self.rem_s("i64", 64),
            0x82 => self.rem_u("i64", 64),
            0x83 => self.binop("i64", "and"),
            0x84 => self.binop("i64", "or"),
            0x85 => self.binop("i64", "xor"),
            0x86 => self.shift("i64", 64, "shl"),
            0x87 => self.shift("i64", 64, "ashr"),
            0x88 => self.shift("i64", 64, "lshr"),
            0x89 => self.rotate("i64", 64, true),
            0x8a => self.rotate("i64", 64, false),
            _ => unreachable!(),
        }
    }

    fn binop(&mut self, ty: &str, op: &str) {
        let b = self.pop();
        let a = self.pop();
        let t = self.tmp();
        self.emit(format!("  {} = {} {} {}, {}", t, op, ty, a.name, b.name));
        self.push(Val {
            name: t,
            ty: val_ty_from_ll(ty),
        });
    }

    fn div_s(&mut self, ty: &str, bits: u32) {
        let b = self.pop();
        let a = self.pop();
        let z = self.tmp();
        self.emit(format!("  {} = icmp eq {} {}, 0", z, ty, b.name));
        self.trap_if(&z, 1);
        let (minv, neg1) = if bits == 32 {
            ("-2147483648", "-1")
        } else {
            ("-9223372036854775808", "-1")
        };
        let ismin = self.tmp();
        self.emit(format!("  {} = icmp eq {} {}, {}", ismin, ty, a.name, minv));
        let isneg = self.tmp();
        self.emit(format!("  {} = icmp eq {} {}, {}", isneg, ty, b.name, neg1));
        let ovf = self.tmp();
        self.emit(format!("  {} = and i1 {}, {}", ovf, ismin, isneg));
        self.trap_if(&ovf, 2);
        let r = self.tmp();
        self.emit(format!("  {} = sdiv {} {}, {}", r, ty, a.name, b.name));
        self.push(Val {
            name: r,
            ty: val_ty_from_ll(ty),
        });
    }

    fn div_u(&mut self, ty: &str, _bits: u32) {
        let b = self.pop();
        let a = self.pop();
        let z = self.tmp();
        self.emit(format!("  {} = icmp eq {} {}, 0", z, ty, b.name));
        self.trap_if(&z, 1);
        let r = self.tmp();
        self.emit(format!("  {} = udiv {} {}, {}", r, ty, a.name, b.name));
        self.push(Val {
            name: r,
            ty: val_ty_from_ll(ty),
        });
    }

    fn rem_s(&mut self, ty: &str, bits: u32) {
        let b = self.pop();
        let a = self.pop();
        let z = self.tmp();
        self.emit(format!("  {} = icmp eq {} {}, 0", z, ty, b.name));
        self.trap_if(&z, 1);
        let isneg = self.tmp();
        self.emit(format!("  {} = icmp eq {} {}, -1", isneg, ty, b.name));
        let safe = self.tmp();
        let one = if bits == 32 { "1" } else { "1" };
        self.emit(format!(
            "  {} = select i1 {}, {} {}, {} {}",
            safe, isneg, ty, one, ty, b.name
        ));
        let r = self.tmp();
        self.emit(format!("  {} = srem {} {}, {}", r, ty, a.name, safe));
        let res = self.tmp();
        self.emit(format!(
            "  {} = select i1 {}, {} zeroinitializer, {} {}",
            res, isneg, ty, ty, r
        ));
        self.push(Val {
            name: res,
            ty: val_ty_from_ll(ty),
        });
    }

    fn rem_u(&mut self, ty: &str, _bits: u32) {
        let b = self.pop();
        let a = self.pop();
        let z = self.tmp();
        self.emit(format!("  {} = icmp eq {} {}, 0", z, ty, b.name));
        self.trap_if(&z, 1);
        let r = self.tmp();
        self.emit(format!("  {} = urem {} {}, {}", r, ty, a.name, b.name));
        self.push(Val {
            name: r,
            ty: val_ty_from_ll(ty),
        });
    }

    fn shift(&mut self, ty: &str, bits: u32, op: &str) {
        let b = self.pop();
        let a = self.pop();
        let m = self.tmp();
        self.emit(format!("  {} = and {} {}, {}", m, ty, b.name, bits - 1));
        let r = self.tmp();
        self.emit(format!("  {} = {} {} {}, {}", r, op, ty, a.name, m));
        self.push(Val {
            name: r,
            ty: val_ty_from_ll(ty),
        });
    }

    fn rotate(&mut self, ty: &str, bits: u32, left: bool) {
        let b = self.pop();
        let a = self.pop();
        let m = self.tmp();
        self.emit(format!("  {} = and {} {}, {}", m, ty, b.name, bits - 1));
        let r = self.tmp();
        let name = if left {
            format!("llvm.fshl.{}", ty)
        } else {
            format!("llvm.fshr.{}", ty)
        };
        let rt = self.call_intr(
            ty,
            &name,
            &format!("{}, {}, {}", ty, ty, ty),
            &format!("{} {}, {} {}, {} {}", ty, a.name, ty, a.name, ty, m),
        );
        let _ = r;
        self.push(Val {
            name: rt,
            ty: val_ty_from_ll(ty),
        });
    }

    fn fbin(&mut self, ty: ValType, k: u16) -> Result<(), String> {
        let t = ll_ty(ty);
        match k {
            0 => {
                let a = self.pop();
                let r = self.call_intr(t, &format!("llvm.fabs.{}", tsfx(ty)), t, &format!("{} {}", t, a.name));
                self.push(Val { name: r, ty });
            }
            1 => {
                let a = self.pop();
                let r = self.tmp();
                self.emit(format!("  {} = fneg {} {}", r, t, a.name));
                self.push(Val { name: r, ty });
            }
            2 | 3 | 4 | 5 => {
                let base = match k {
                    2 => "ceil",
                    3 => "floor",
                    4 => "trunc",
                    _ => "roundeven",
                };
                let a = self.pop();
                let r = self.call_intr(
                    t,
                    &format!("llvm.{}.{}", base, tsfx(ty)),
                    t,
                    &format!("{} {}", t, a.name),
                );
                self.push(Val { name: r, ty });
            }
            6 => {
                let a = self.pop();
                let r = self.call_intr(
                    t,
                    &format!("llvm.sqrt.{}", tsfx(ty)),
                    t,
                    &format!("{} {}", t, a.name),
                );
                self.push(Val { name: r, ty });
            }
            7 => self.fbinop(ty, "fadd"),
            8 => self.fbinop(ty, "fsub"),
            9 => self.fbinop(ty, "fmul"),
            10 => self.fbinop(ty, "fdiv"),
            11 | 12 => {
                self.fminmax(ty, k == 12);
            }
            13 => {
                let b = self.pop();
                let a = self.pop();
                let r = self.call_intr(
                    t,
                    &format!("llvm.copysign.{}", tsfx(ty)),
                    &format!("{}, {}", t, t),
                    &format!("{} {}, {} {}", t, a.name, t, b.name),
                );
                self.push(Val { name: r, ty });
            }
            _ => return Err("bad float op".to_string()),
        }
        Ok(())
    }

    fn fbinop(&mut self, ty: ValType, op: &str) {
        let t = ll_ty(ty);
        let b = self.pop();
        let a = self.pop();
        let r = self.tmp();
        self.emit(format!("  {} = {} {} {}, {}", r, op, t, a.name, b.name));
        self.push(Val { name: r, ty });
    }

    fn fminmax(&mut self, ty: ValType, max: bool) {
        let t = ll_ty(ty);
        let (ity, qbit) = if ty == ValType::F32 {
            ("i32", "4194304")
        } else {
            ("i64", "2251799813685248")
        };
        let b = self.pop();
        let a = self.pop();
        let base = if max { "maximum" } else { "minimum" };
        let r = self.call_intr(
            t,
            &format!("llvm.{}.{}", base, tsfx(ty)),
            &format!("{}, {}", t, t),
            &format!("{} {}, {} {}", t, a.name, t, b.name),
        );
        let isn = self.tmp();
        self.emit(format!("  {} = fcmp uno {} {}, {}", isn, t, r, r));
        let bits = self.tmp();
        self.emit(format!("  {} = bitcast {} {} to {}", bits, t, r, ity));
        let q = self.tmp();
        self.emit(format!("  {} = or {} {}, {}", q, ity, bits, qbit));
        let sel = self.tmp();
        self.emit(format!(
            "  {} = select i1 {}, {} {}, {} {}",
            sel, isn, ity, q, ity, bits
        ));
        let res = self.tmp();
        self.emit(format!("  {} = bitcast {} {} to {}", res, ity, sel, t));
        self.push(Val { name: res, ty });
    }

    fn int_to_float(&mut self, from: ValType, to: ValType, signed: bool) {
        let a = self.pop();
        let r = self.tmp();
        self.emit(format!(
            "  {} = {} {} {} to {}",
            r,
            if signed { "sitofp" } else { "uitofp" },
            ll_ty(from),
            a.name,
            ll_ty(to)
        ));
        self.push(Val { name: r, ty: to });
    }

    fn extend_s(&mut self, ty: ValType, bits: u32) {
        let a = self.pop();
        let small = match bits {
            8 => "i8",
            16 => "i16",
            _ => "i32",
        };
        let t1 = self.tmp();
        self.emit(format!("  {} = trunc {} {} to {}", t1, ll_ty(ty), a.name, small));
        let t2 = self.tmp();
        self.emit(format!("  {} = sext {} {} to {}", t2, small, t1, ll_ty(ty)));
        self.push(Val { name: t2, ty });
    }

    fn trunc_int(&mut self, src: ValType, dst: ValType, signed: bool) -> Result<(), String> {
        use ValType::*;
        let x = self.pop();
        let fty = ll_ty(src);
        let nan = self.tmp();
        self.emit(format!("  {} = fcmp uno {} {}, {}", nan, fty, x.name, x.name));
        self.trap_if(&nan, 3);
        let (hi, lo, lo_strict): (String, String, bool) = match (src, dst, signed) {
            (F32, I32, true) => (f32c(2147483648.0), f32c(-2147483648.0), true),
            (F32, I32, false) => (f32c(4294967296.0), f32c(-1.0), false),
            (F64, I32, true) => (f64c(2147483648.0), f64c(-2147483649.0), false),
            (F64, I32, false) => (f64c(4294967296.0), f64c(-1.0), false),
            (F32, I64, true) => (
                f32c(9223372036854775808.0),
                f32c(-9223372036854775808.0),
                true,
            ),
            (F32, I64, false) => (f32c(18446744073709551616.0), f32c(-1.0), false),
            (F64, I64, true) => (
                f64c(9223372036854775808.0),
                f64c(-9223372036854775808.0),
                true,
            ),
            (F64, I64, false) => (f64c(18446744073709551616.0), f64c(-1.0), false),
            _ => return Err("bad trunc conversion".to_string()),
        };
        let hic = self.tmp();
        self.emit(format!("  {} = fcmp oge {} {}, {}", hic, fty, x.name, hi));
        let loc = self.tmp();
        self.emit(format!(
            "  {} = fcmp {} {} {}, {}",
            loc,
            if lo_strict { "olt" } else { "ole" },
            fty,
            x.name,
            lo
        ));
        let bad = self.tmp();
        self.emit(format!("  {} = or i1 {}, {}", bad, hic, loc));
        self.trap_if(&bad, 2);
        let name = if signed {
            format!("llvm.fptosi.sat.{}.{}", ll_ty(dst), tsfx(src))
        } else {
            format!("llvm.fptoui.sat.{}.{}", ll_ty(dst), tsfx(src))
        };
        let r = self.call_intr(ll_ty(dst), &name, fty, &format!("{} {}", fty, x.name));
        self.push(Val { name: r, ty: dst });
        Ok(())
    }

    fn gen_trunc_sat(&mut self, sub: u32) -> Result<(), String> {
        use ValType::*;
        let (src, dst, signed) = match sub {
            0 => (F32, I32, true),
            1 => (F32, I32, false),
            2 => (F64, I32, true),
            3 => (F64, I32, false),
            4 => (F32, I64, true),
            5 => (F32, I64, false),
            6 => (F64, I64, true),
            _ => (F64, I64, false),
        };
        let a = self.pop();
        let name = if signed {
            format!("llvm.fptosi.sat.{}.{}", ll_ty(dst), tsfx(src))
        } else {
            format!("llvm.fptoui.sat.{}.{}", ll_ty(dst), tsfx(src))
        };
        let ft = ll_ty(src);
        let r = self.call_intr(ll_ty(dst), &name, ft, &format!("{} {}", ft, a.name));
        self.push(Val { name: r, ty: dst });
        Ok(())
    }

    // ------------------------------------------------------------------
    // SIMD
    // ------------------------------------------------------------------

    fn bc(&mut self, val: &str, from: &str, to: &str) -> String {
        if from == to {
            return val.to_string();
        }
        let t = self.tmp();
        self.emit(format!(
            "  {} = bitcast {} {} to {}",
            t, from, val, to
        ));
        t
    }
    fn vcast(&mut self, v: &Val, to: &str) -> String {
        self.bc(&v.name, V, to)
    }
    fn back(&mut self, name: &str, from: &str) -> Val {
        Val { name: self.bc(name, from, V), ty: ValType::V128 }
    }
    fn push_back(&mut self, name: &str, from: &str) {
        let v = self.back(name, from);
        self.stack.push(v);
    }
    fn pop_v(&mut self) -> Val {
        self.pop()
    }

    fn splat_scalar(&mut self, scalar_ty: &str, vec_ty: &str, val: &str) -> String {
        let n = lanes_of(vec_ty);
        let t0 = self.tmp();
        self.emit(format!(
            "  {} = insertelement {} zeroinitializer, {} {}, i32 0",
            t0, vec_ty, scalar_ty, val
        ));
        let t1 = self.tmp();
        self.emit(format!(
            "  {} = shufflevector {} {}, {} zeroinitializer, <{} x i32> zeroinitializer",
            t1, vec_ty, t0, vec_ty, n
        ));
        t1
    }

    fn vbin(&mut self, lane_ty: &str, op: &str) {
        let b = self.pop_v();
        let a = self.pop_v();
        let av = self.vcast(&a, lane_ty);
        let bv = self.vcast(&b, lane_ty);
        let t = self.tmp();
        self.emit(format!("  {} = {} {} {}, {}", t, op, lane_ty, av, bv));
        let v = self.back(&t, lane_ty);
        self.push(v);
    }

    fn vbin_intr(&mut self, lane_ty: &str, base: &str) {
        let b = self.pop_v();
        let a = self.pop_v();
        let av = self.vcast(&a, lane_ty);
        let bv = self.vcast(&b, lane_ty);
        let name = format!("llvm.{}.{}", base, suffix_of(lane_ty));
        let t = self.call_intr(
            lane_ty,
            &name,
            &format!("{}, {}", lane_ty, lane_ty),
            &format!("{} {}, {} {}", lane_ty, av, lane_ty, bv),
        );
        let v = self.back(&t, lane_ty);
        self.push(v);
    }

    fn vun_intr(&mut self, lane_ty: &str, base: &str, extra: &str) {
        let a = self.pop_v();
        let av = self.vcast(&a, lane_ty);
        let name = format!("llvm.{}.{}", base, suffix_of(lane_ty));
        let t = self.call_intr(
            lane_ty,
            &name,
            &format!("{}, i1", lane_ty),
            &format!("{} {}, i1 {}", lane_ty, av, extra),
        );
        let v = self.back(&t, lane_ty);
        self.push(v);
    }

    fn vcmp(&mut self, in_ty: &str, out_ty: &str, pred: &str, float: bool) {
        let b = self.pop_v();
        let a = self.pop_v();
        let av = self.vcast(&a, in_ty);
        let bv = self.vcast(&b, in_ty);
        let c = self.tmp();
        self.emit(format!(
            "  {} = {} {} {} {}, {}",
            c,
            if float { "fcmp" } else { "icmp" },
            pred,
            in_ty,
            av,
            bv
        ));
        let m = self.tmp();
        self.emit(format!(
            "  {} = sext {} {} to {}",
            m,
            mask_ty(in_ty),
            c,
            out_ty
        ));
        let v = self.back(&m, out_ty);
        self.push(v);
    }

    fn vcmp_group(&mut self, in_ty: &str, out_ty: &str, k: u32, float: bool) {
        let pred = if float {
            match k {
                0 => "oeq",
                1 => "une",
                2 => "olt",
                3 => "ogt",
                4 => "ole",
                _ => "oge",
            }
        } else {
            match k {
                0 => "eq",
                1 => "ne",
                2 => "slt",
                3 => "ult",
                4 => "sgt",
                5 => "ugt",
                6 => "sle",
                7 => "ule",
                8 => "sge",
                _ => "uge",
            }
        };
        self.vcmp(in_ty, out_ty, pred, float);
    }

    fn vshift(&mut self, lane_ty: &str, op: &str) {
        let amount = self.pop();
        let a = self.pop_v();
        let width = (128 / lanes_of(lane_ty)) as u32;
        let m = self.tmp();
        self.emit(format!("  {} = and i32 {}, {}", m, amount.name, width - 1));
        let ety = ety_of(lane_ty);
        let tr = self.tmp();
        if ety == "i32" {
            self.emit(format!("  {} = add i32 {}, 0", tr, m));
        } else if ety == "i64" {
            self.emit(format!("  {} = zext i32 {} to i64", tr, m));
        } else {
            self.emit(format!("  {} = trunc i32 {} to {}", tr, m, ety));
        }
        let sv = self.splat_scalar(ety, lane_ty, &tr);
        let av = self.vcast(&a, lane_ty);
        let r = self.tmp();
        self.emit(format!("  {} = {} {} {}, {}", r, op, lane_ty, av, sv));
        let v = self.back(&r, lane_ty);
        self.push(v);
    }

    fn vshuffle(&mut self, a: &Val, b: &Val, mask: &[i64]) -> Val {
        let av = self.vcast(a, I8X16);
        let bv = self.vcast(b, I8X16);
        let t = self.tmp();
        self.emit(format!(
            "  {} = shufflevector <16 x i8> {}, <16 x i8> {}, {}",
            t,
            av,
            bv,
            idx_mask(mask)
        ));
        Val { name: t, ty: ValType::V128 }
    }

    fn vshuffle_in(&mut self, v: &Val, in_ty: &str, mask: &[i64]) -> String {
        let av = self.vcast(v, in_ty);
        let t = self.tmp();
        self.emit(format!(
            "  {} = shufflevector {} {}, {} zeroinitializer, {}",
            t,
            in_ty,
            av,
            in_ty,
            idx_mask(mask)
        ));
        t
    }

    fn vextend_half(
        &mut self,
        src_ty: &str,
        dst_ty: &str,
        high: bool,
        signed: bool,
    ) -> Result<(), String> {
        let a = self.pop_v();
        let n = lanes_of(src_ty);
        let half = n / 2;
        let start = if high { half } else { 0 };
        let mask: Vec<i64> = (0..half).map(|i| (start + i) as i64).collect();
        let h = self.vshuffle_in(&a, src_ty, &mask);
        let hty = format!("<{} x {}>", half, ety_of(src_ty));
        let e = self.tmp();
        self.emit(format!(
            "  {} = {} {} {} to {}",
            e,
            if signed { "sext" } else { "zext" },
            hty,
            h,
            dst_ty
        ));
        let v = self.back(&e, dst_ty);
        self.push(v);
        Ok(())
    }

    fn vextadd_pairwise(&mut self, src_ty: &str, dst_ty: &str, signed: bool) -> Result<(), String> {
        let a = self.pop_v();
        let av = self.vcast(&a, src_ty);
        let wty = format!("<{} x {}>", lanes_of(src_ty), ety_of(dst_ty));
        let wide = self.tmp();
        self.emit(format!(
            "  {} = {} {} {} to {}",
            wide,
            if signed { "sext" } else { "zext" },
            src_ty,
            av,
            wty
        ));
        let n = lanes_of(dst_ty) * 2;
        let even: Vec<i64> = (0..n / 2).map(|i| (2 * i) as i64).collect();
        let odd: Vec<i64> = (0..n / 2).map(|i| (2 * i + 1) as i64).collect();
        let ev = self.tmp();
        self.emit(format!(
            "  {} = shufflevector {} {}, {} zeroinitializer, {}",
            ev,
            wty,
            wide,
            wty,
            idx_mask(&even)
        ));
        let od = self.tmp();
        self.emit(format!(
            "  {} = shufflevector {} {}, {} zeroinitializer, {}",
            od,
            wty,
            wide,
            wty,
            idx_mask(&odd)
        ));
        let r = self.tmp();
        self.emit(format!("  {} = add {} {}, {}", r, dst_ty, ev, od));
        let v = self.back(&r, dst_ty);
        self.push(v);
        Ok(())
    }

    fn vextmul(&mut self, src_ty: &str, dst_ty: &str, high: bool, signed: bool) {
        let b = self.pop_v();
        let a = self.pop_v();
        let n = lanes_of(src_ty);
        let half = n / 2;
        let start = if high { half } else { 0 };
        let mask: Vec<i64> = (0..half).map(|i| (start + i) as i64).collect();
        let ah = self.vshuffle_in(&a, src_ty, &mask);
        let bh = self.vshuffle_in(&b, src_ty, &mask);
        let hty = format!("<{} x {}>", half, ety_of(src_ty));
        let ae = self.tmp();
        self.emit(format!(
            "  {} = {} {} {} to {}",
            ae,
            if signed { "sext" } else { "zext" },
            hty,
            ah,
            dst_ty
        ));
        let be = self.tmp();
        self.emit(format!(
            "  {} = {} {} {} to {}",
            be,
            if signed { "sext" } else { "zext" },
            hty,
            bh,
            dst_ty
        ));
        let r = self.tmp();
        self.emit(format!("  {} = mul {} {}, {}", r, dst_ty, ae, be));
        let v = self.back(&r, dst_ty);
        self.push(v);
    }

    fn vnarrow(&mut self, src_ty: &str, dst_ty: &str, signed: bool) {
        let b = self.pop_v();
        let a = self.pop_v();
        let av = self.vcast(&a, src_ty);
        let bv = self.vcast(&b, src_ty);
        let (lo, hi) = match (lanes_of(dst_ty), signed) {
            (16, true) => (-128i64, 127i64),
            (16, false) => (0i64, 255i64),
            (8, true) => (-32768i64, 32767i64),
            _ => (0i64, 65535i64),
        };
        let clamp = |g: &mut Gen, v: &str| -> String {
            let et = ety_of(src_ty);
            let lov = const_vec(src_ty, et, &format!("{}", lo));
            let hiv = const_vec(src_ty, et, &format!("{}", hi));
            let t1 = g.tmp();
            g.emit(format!(
                "  {} = call {} @llvm.smax.{}({} {}, {} {})",
                t1,
                src_ty,
                suffix_of(src_ty),
                src_ty,
                v,
                src_ty,
                lov
            ));
            g.need_decl(&format!(
                "declare {} @llvm.smax.{}({}, {})",
                src_ty,
                suffix_of(src_ty),
                src_ty,
                src_ty
            ));
            let t2 = g.tmp();
            if signed {
                g.emit(format!(
                    "  {} = call {} @llvm.smin.{}({} {}, {} {})",
                    t2,
                    src_ty,
                    suffix_of(src_ty),
                    src_ty,
                    t1,
                    src_ty,
                    hiv
                ));
                g.need_decl(&format!(
                    "declare {} @llvm.smin.{}({}, {})",
                    src_ty,
                    suffix_of(src_ty),
                    src_ty,
                    src_ty
                ));
            } else {
                g.emit(format!(
                    "  {} = call {} @llvm.umin.{}({} {}, {} {})",
                    t2,
                    src_ty,
                    suffix_of(src_ty),
                    src_ty,
                    t1,
                    src_ty,
                    hiv
                ));
                g.need_decl(&format!(
                    "declare {} @llvm.umin.{}({}, {})",
                    src_ty,
                    suffix_of(src_ty),
                    src_ty,
                    src_ty
                ));
            }
            t2
        };
        let ca = clamp(self, &av);
        let cb = clamp(self, &bv);
        let half_ty = format!("<{} x {}>", lanes_of(src_ty), ety_of(dst_ty));
        let da = self.tmp();
        self.emit(format!(
            "  {} = trunc {} {} to {}",
            da, src_ty, ca, half_ty
        ));
        let db = self.tmp();
        self.emit(format!(
            "  {} = trunc {} {} to {}",
            db, src_ty, cb, half_ty
        ));
        let n = lanes_of(src_ty) as i64;
        let mask: Vec<i64> = (0..n).chain(n..2 * n).collect();
        let r = self.tmp();
        self.emit(format!(
            "  {} = shufflevector {} {}, {} {}, {}",
            r,
            half_ty,
            da,
            half_ty,
            db,
            idx_mask(&mask)
        ));
        let v = self.back(&r, dst_ty);
        self.push(v);
    }

    fn vall_true(&mut self, lane_ty: &str) {
        let a = self.pop_v();
        let av = self.vcast(&a, lane_ty);
        let nz = self.tmp();
        self.emit(format!(
            "  {} = icmp ne {} {}, zeroinitializer",
            nz, lane_ty, av
        ));
        let mt = mask_ty(lane_ty);
        let name = format!("llvm.vector.reduce.and.{}", mask_suffix(lane_ty));
        let r = self.call_intr("i1", &name, &mt, &format!("{} {}", mt, nz));
        self.zext_i1(r);
    }

    fn vbitmask(&mut self, lane_ty: &str) {
        let a = self.pop_v();
        let av = self.vcast(&a, lane_ty);
        let nz = self.tmp();
        self.emit(format!(
            "  {} = icmp slt {} {}, zeroinitializer",
            nz, lane_ty, av
        ));
        let n = lanes_of(lane_ty);
        let ity = format!("i{}", n);
        let bits = self.tmp();
        self.emit(format!(
            "  {} = bitcast {} {} to {}",
            bits,
            mask_ty(lane_ty),
            nz,
            ity
        ));
        let r = self.tmp();
        self.emit(format!("  {} = zext {} {} to i32", r, ity, bits));
        self.push(Val { name: r, ty: ValType::I32 });
    }

    fn vswizzle(&mut self) {
        let idx = self.pop_v();
        let a = self.pop_v();
        let iv = self.vcast(&idx, I8X16);
        let av = self.vcast(&a, I8X16);
        let mut acc = String::from("zeroinitializer");
        for j in 0..16i32 {
            let splat_idx = self.splat_scalar("i8", I8X16, &format!("{}", j));
            let cond = self.tmp();
            self.emit(format!(
                "  {} = icmp eq <16 x i8> {}, {}",
                cond, iv, splat_idx
            ));
            let el = self.tmp();
            self.emit(format!(
                "  {} = extractelement <16 x i8> {}, i32 {}",
                el, av, j
            ));
            let splat = self.splat_scalar("i8", I8X16, &el);
            let sel = self.tmp();
            self.emit(format!(
                "  {} = select <16 x i1> {}, <16 x i8> {}, <16 x i8> {}",
                sel, cond, splat, acc
            ));
            acc = sel;
        }
        self.push(Val { name: acc, ty: ValType::V128 });
    }

    fn gen_simd(&mut self, s: &SimdOp) -> Result<(), String> {
        match &s.kind {
            SimdKind::Const(bytes) => {
                let items: Vec<String> = bytes.iter().map(|b| format!("i8 {}", b)).collect();
                self.push(Val {
                    name: format!("<{}>", items.join(", ")),
                    ty: ValType::V128,
                });
            }
            SimdKind::Shuffle(mask) => {
                let b = self.pop_v();
                let a = self.pop_v();
                let m: Vec<i64> = mask.iter().map(|x| *x as i64).collect();
                let v = self.vshuffle(&a, &b, &m);
                self.push(v);
            }
            SimdKind::Lane { lane } => self.gen_simd_lane(s.sub, *lane)?,
            SimdKind::Mem { offset, .. } => self.gen_simd_mem(s.sub, *offset)?,
            SimdKind::MemLane { offset, lane, .. } => {
                self.gen_simd_memlane(s.sub, *offset, *lane)?
            }
            SimdKind::Plain => self.gen_simd_plain(s.sub)?,
        }
        Ok(())
    }

    fn gen_simd_lane(&mut self, sub: u32, lane: u8) -> Result<(), String> {
        match sub {
            21 => {
                let a = self.pop_v();
                let av = self.vcast(&a, I8X16);
                let e = self.tmp();
                self.emit(format!(
                    "  {} = extractelement <16 x i8> {}, i32 {}",
                    e, av, lane
                ));
                let s = self.tmp();
                self.emit(format!("  {} = sext i8 {} to i32", s, e));
                self.push(Val { name: s, ty: ValType::I32 });
            }
            22 => {
                let a = self.pop_v();
                let av = self.vcast(&a, I8X16);
                let e = self.tmp();
                self.emit(format!(
                    "  {} = extractelement <16 x i8> {}, i32 {}",
                    e, av, lane
                ));
                let s = self.tmp();
                self.emit(format!("  {} = zext i8 {} to i32", s, e));
                self.push(Val { name: s, ty: ValType::I32 });
            }
            23 => {
                let v = self.pop();
                let a = self.pop_v();
                let av = self.vcast(&a, I8X16);
                let tr = self.tmp();
                self.emit(format!("  {} = trunc i32 {} to i8", tr, v.name));
                let e = self.tmp();
                self.emit(format!(
                    "  {} = insertelement <16 x i8> {}, i8 {}, i32 {}",
                    e, av, tr, lane
                ));
                self.push_back(&e, I8X16);
            }
            24 | 25 => {
                let a = self.pop_v();
                let av = self.vcast(&a, I16X8);
                let e = self.tmp();
                self.emit(format!(
                    "  {} = extractelement <8 x i16> {}, i32 {}",
                    e, av, lane
                ));
                let s = self.tmp();
                self.emit(format!(
                    "  {} = {} i16 {} to i32",
                    s,
                    if sub == 24 { "sext" } else { "zext" },
                    e
                ));
                self.push(Val { name: s, ty: ValType::I32 });
            }
            26 => {
                let v = self.pop();
                let a = self.pop_v();
                let av = self.vcast(&a, I16X8);
                let tr = self.tmp();
                self.emit(format!("  {} = trunc i32 {} to i16", tr, v.name));
                let e = self.tmp();
                self.emit(format!(
                    "  {} = insertelement <8 x i16> {}, i16 {}, i32 {}",
                    e, av, tr, lane
                ));
                self.push_back(&e, I16X8);
            }
            27 => {
                let a = self.pop_v();
                let av = self.vcast(&a, I32X4);
                let e = self.tmp();
                self.emit(format!(
                    "  {} = extractelement <4 x i32> {}, i32 {}",
                    e, av, lane
                ));
                self.push(Val { name: e, ty: ValType::I32 });
            }
            28 => {
                let v = self.pop();
                let a = self.pop_v();
                let av = self.vcast(&a, I32X4);
                let e = self.tmp();
                self.emit(format!(
                    "  {} = insertelement <4 x i32> {}, i32 {}, i32 {}",
                    e, av, v.name, lane
                ));
                self.push_back(&e, I32X4);
            }
            29 => {
                let a = self.pop_v();
                let av = self.vcast(&a, I64X2);
                let e = self.tmp();
                self.emit(format!(
                    "  {} = extractelement <2 x i64> {}, i32 {}",
                    e, av, lane
                ));
                self.push(Val { name: e, ty: ValType::I64 });
            }
            30 => {
                let v = self.pop();
                let a = self.pop_v();
                let av = self.vcast(&a, I64X2);
                let e = self.tmp();
                self.emit(format!(
                    "  {} = insertelement <2 x i64> {}, i64 {}, i32 {}",
                    e, av, v.name, lane
                ));
                self.push_back(&e, I64X2);
            }
            31 => {
                let a = self.pop_v();
                let av = self.vcast(&a, F32X4);
                let e = self.tmp();
                self.emit(format!(
                    "  {} = extractelement <4 x float> {}, i32 {}",
                    e, av, lane
                ));
                self.push(Val { name: e, ty: ValType::F32 });
            }
            32 => {
                let v = self.pop();
                let a = self.pop_v();
                let av = self.vcast(&a, F32X4);
                let e = self.tmp();
                self.emit(format!(
                    "  {} = insertelement <4 x float> {}, float {}, i32 {}",
                    e, av, v.name, lane
                ));
                self.push_back(&e, F32X4);
            }
            33 => {
                let a = self.pop_v();
                let av = self.vcast(&a, F64X2);
                let e = self.tmp();
                self.emit(format!(
                    "  {} = extractelement <2 x double> {}, i32 {}",
                    e, av, lane
                ));
                self.push(Val { name: e, ty: ValType::F64 });
            }
            34 => {
                let v = self.pop();
                let a = self.pop_v();
                let av = self.vcast(&a, F64X2);
                let e = self.tmp();
                self.emit(format!(
                    "  {} = insertelement <2 x double> {}, double {}, i32 {}",
                    e, av, v.name, lane
                ));
                self.push_back(&e, F64X2);
            }
            _ => return Err(format!("bad lane opcode {}", sub)),
        }
        Ok(())
    }

    fn gen_simd_mem(&mut self, sub: u32, offset: u64) -> Result<(), String> {
        use ValType::*;
        let (_p, _r, size) = crate::validate::simd_mem_sig(sub).ok_or("bad simd mem op")?;
        match sub {
            11 => {
                // v128.store: stack [addr, v128]
                let v = self.pop_v();
                let p = self.mem_ptr(offset, size);
                self.emit(format!("  store <16 x i8> {}, ptr {}, align 1", v.name, p));
            }
            88..=91 => unreachable!(),
            _ => {
                let p = self.mem_ptr(offset, size);
                match sub {
                    0 => {
                        let t = self.tmp();
                        self.emit(format!("  {} = load <16 x i8>, ptr {}, align 1", t, p));
                        self.push(Val { name: t, ty: V128 });
                    }
                    1 | 2 => {
                        let t = self.tmp();
                        self.emit(format!("  {} = load <8 x i8>, ptr {}, align 1", t, p));
                        let e = self.tmp();
                        self.emit(format!(
                            "  {} = {} <8 x i8> {} to <8 x i16>",
                            e,
                            if sub == 1 { "sext" } else { "zext" },
                            t
                        ));
                        self.push_back(&e, I16X8);
                    }
                    3 | 4 => {
                        let t = self.tmp();
                        self.emit(format!("  {} = load <4 x i16>, ptr {}, align 1", t, p));
                        let e = self.tmp();
                        self.emit(format!(
                            "  {} = {} <4 x i16> {} to <4 x i32>",
                            e,
                            if sub == 3 { "sext" } else { "zext" },
                            t
                        ));
                        self.push_back(&e, I32X4);
                    }
                    5 | 6 => {
                        let t = self.tmp();
                        self.emit(format!("  {} = load <2 x i32>, ptr {}, align 1", t, p));
                        let e = self.tmp();
                        self.emit(format!(
                            "  {} = {} <2 x i32> {} to <2 x i64>",
                            e,
                            if sub == 5 { "sext" } else { "zext" },
                            t
                        ));
                        self.push_back(&e, I64X2);
                    }
                    7..=10 => {
                        let (st, vt) = match sub {
                            7 => ("i8", I8X16),
                            8 => ("i16", I16X8),
                            9 => ("i32", I32X4),
                            _ => ("i64", I64X2),
                        };
                        let t = self.tmp();
                        self.emit(format!("  {} = load {}, ptr {}, align 1", t, st, p));
                        let sp = self.splat_scalar(st, vt, &t);
                        self.push_back(&sp, vt);
                    }
                    92 => {
                        let t = self.tmp();
                        self.emit(format!("  {} = load i32, ptr {}, align 1", t, p));
                        let e = self.tmp();
                        self.emit(format!(
                            "  {} = insertelement <4 x i32> zeroinitializer, i32 {}, i32 0",
                            e, t
                        ));
                        self.push_back(&e, I32X4);
                    }
                    93 => {
                        let t = self.tmp();
                        self.emit(format!("  {} = load i64, ptr {}, align 1", t, p));
                        let e = self.tmp();
                        self.emit(format!(
                            "  {} = insertelement <2 x i64> zeroinitializer, i64 {}, i32 0",
                            e, t
                        ));
                        self.push_back(&e, I64X2);
                    }
                    _ => return Err(format!("bad simd mem opcode {}", sub)),
                }
            }
        }
        Ok(())
    }

    fn gen_simd_memlane(&mut self, sub: u32, offset: u64, lane: u8) -> Result<(), String> {
        let (_p, _r, size) = crate::validate::simd_mem_sig(sub).ok_or("bad simd mem op")?;
        if (84..=87).contains(&sub) {
            let v = self.pop_v();
            let p = self.mem_ptr(offset, size);
            let (st, vt) = match sub {
                84 => ("i8", I8X16),
                85 => ("i16", I16X8),
                86 => ("i32", I32X4),
                _ => ("i64", I64X2),
            };
            let t = self.tmp();
            self.emit(format!("  {} = load {}, ptr {}, align 1", t, st, p));
            let av = self.vcast(&v, vt);
            let e = self.tmp();
            self.emit(format!(
                "  {} = insertelement {} {}, {} {}, i32 {}",
                e, vt, av, st, t, lane
            ));
            self.push_back(&e, vt);
        } else {
            let v = self.pop_v();
            let p = self.mem_ptr(offset, size);
            let (st, vt) = match sub {
                88 => ("i8", I8X16),
                89 => ("i16", I16X8),
                90 => ("i32", I32X4),
                _ => ("i64", I64X2),
            };
            let av = self.vcast(&v, vt);
            let e = self.tmp();
            self.emit(format!(
                "  {} = extractelement {} {}, i32 {}",
                e, vt, av, lane
            ));
            self.emit(format!("  store {} {}, ptr {}, align 1", st, e, p));
        }
        Ok(())
    }

    fn gen_simd_plain(&mut self, sub: u32) -> Result<(), String> {
        use ValType::*;
        match sub {
            14 => self.vswizzle(),
            15 => {
                let a = self.pop();
                let t = self.tmp();
                self.emit(format!("  {} = trunc i32 {} to i8", t, a.name));
                let s = self.splat_scalar("i8", I8X16, &t);
                self.push(Val { name: s, ty: V128 });
            }
            16 => {
                let a = self.pop();
                let t = self.tmp();
                self.emit(format!("  {} = trunc i32 {} to i16", t, a.name));
                let s = self.splat_scalar("i16", I16X8, &t);
                self.push_back(&s, I16X8);
            }
            17 => {
                let a = self.pop();
                let s = self.splat_scalar("i32", I32X4, &a.name);
                self.push_back(&s, I32X4);
            }
            18 => {
                let a = self.pop();
                let s = self.splat_scalar("i64", I64X2, &a.name);
                self.push_back(&s, I64X2);
            }
            19 => {
                let a = self.pop();
                let s = self.splat_scalar("float", F32X4, &a.name);
                self.push_back(&s, F32X4);
            }
            20 => {
                let a = self.pop();
                let s = self.splat_scalar("double", F64X2, &a.name);
                self.push_back(&s, F64X2);
            }
            35..=44 => self.vcmp_group(I8X16, I8X16, sub - 35, false),
            45..=54 => self.vcmp_group(I16X8, I16X8, sub - 45, false),
            55..=64 => self.vcmp_group(I32X4, I32X4, sub - 55, false),
            65..=70 => self.vcmp_group(F32X4, I32X4, sub - 65, true),
            71..=76 => self.vcmp_group(F64X2, I64X2, sub - 71, true),
            214..=219 => {
                let k = match sub {
                    214 => 0,
                    215 => 1,
                    216 => 2,
                    217 => 4,
                    218 => 6,
                    _ => 8,
                };
                self.vcmp_group(I64X2, I64X2, k, false);
            }
            77 => {
                let a = self.pop_v();
                let ones = const_vec(I8X16, "i8", "-1");
                let t = self.tmp();
                self.emit(format!("  {} = xor <16 x i8> {}, {}", t, a.name, ones));
                self.push(Val { name: t, ty: V128 });
            }
            78 => self.vbin(I8X16, "and"),
            79 => {
                let b = self.pop_v();
                let a = self.pop_v();
                let ones = const_vec(I8X16, "i8", "-1");
                let nb = self.tmp();
                self.emit(format!("  {} = xor <16 x i8> {}, {}", nb, b.name, ones));
                let t = self.tmp();
                self.emit(format!("  {} = and <16 x i8> {}, {}", t, a.name, nb));
                self.push(Val { name: t, ty: V128 });
            }
            80 => self.vbin(I8X16, "or"),
            81 => self.vbin(I8X16, "xor"),
            82 => {
                let c = self.pop_v();
                let b = self.pop_v();
                let a = self.pop_v();
                let ones = const_vec(I8X16, "i8", "-1");
                let nc = self.tmp();
                self.emit(format!("  {} = xor <16 x i8> {}, {}", nc, c.name, ones));
                let x = self.tmp();
                self.emit(format!("  {} = and <16 x i8> {}, {}", x, a.name, c.name));
                let y = self.tmp();
                self.emit(format!("  {} = and <16 x i8> {}, {}", y, b.name, nc));
                let t = self.tmp();
                self.emit(format!("  {} = or <16 x i8> {}, {}", t, x, y));
                self.push(Val { name: t, ty: V128 });
            }
            83 => {
                let a = self.pop_v();
                let i = self.bc(&a.name, V, "i128");
                let c = self.tmp();
                self.emit(format!("  {} = icmp ne i128 {}, 0", c, i));
                self.zext_i1(c);
            }
            94 => {
                let a = self.pop_v();
                let av = self.vcast(&a, F64X2);
                let f = self.tmp();
                self.emit(format!("  {} = fptrunc <2 x double> {} to <2 x float>", f, av));
                let sh = self.tmp();
                self.emit(format!(
                    "  {} = shufflevector <2 x float> {}, <2 x float> zeroinitializer, <4 x i32> <i32 0, i32 1, i32 2, i32 3>",
                    sh, f
                ));
                self.push_back(&sh, F32X4);
            }
            95 => {
                let a = self.pop_v();
                let av = self.vcast(&a, F32X4);
                let sh = self.tmp();
                self.emit(format!(
                    "  {} = shufflevector <4 x float> {}, <4 x float> zeroinitializer, <2 x i32> <i32 0, i32 1>",
                    sh, av
                ));
                let f = self.tmp();
                self.emit(format!("  {} = fpext <2 x float> {} to <2 x double>", f, sh));
                self.push_back(&f, F64X2);
            }
            96 => self.vun_intr(I8X16, "abs", "false"),
            97 => {
                let a = self.pop_v();
                let t = self.tmp();
                self.emit(format!(
                    "  {} = sub <16 x i8> zeroinitializer, {}",
                    t, a.name
                ));
                self.push(Val { name: t, ty: V128 });
            }
            98 => {
                let a = self.pop_v();
                let av = self.vcast(&a, I8X16);
                let t = self.call_intr(
                    I8X16,
                    "llvm.ctpop.v16i8",
                    I8X16,
                    &format!("<16 x i8> {}", av),
                );
                self.push_back(&t, I8X16);
            }
            99 => self.vall_true(I8X16),
            100 => self.vbitmask(I8X16),
            101 | 102 => self.vnarrow(I16X8, I8X16, sub == 101),
            103..=106 => self.vfloat_un(F32X4, sub - 103, true),
            107 => self.vshift(I8X16, "shl"),
            108 => self.vshift(I8X16, "ashr"),
            109 => self.vshift(I8X16, "lshr"),
            110 => self.vbin(I8X16, "add"),
            111 => self.vbin_intr(I8X16, "sadd.sat"),
            112 => self.vbin_intr(I8X16, "uadd.sat"),
            113 => self.vbin(I8X16, "sub"),
            114 => self.vbin_intr(I8X16, "ssub.sat"),
            115 => self.vbin_intr(I8X16, "usub.sat"),
            116 | 117 => self.vfloat_un(F64X2, sub - 116, true),
            118 => self.vbin_intr(I8X16, "smin"),
            119 => self.vbin_intr(I8X16, "umin"),
            120 => self.vbin_intr(I8X16, "smax"),
            121 => self.vbin_intr(I8X16, "umax"),
            122 => self.vfloat_un(F64X2, 2, true),
            123 => self.vavgr(I8X16),
            124 => self.vextadd_pairwise(I8X16, I16X8, true)?,
            125 => self.vextadd_pairwise(I8X16, I16X8, false)?,
            126 => self.vextadd_pairwise(I16X8, I32X4, true)?,
            127 => self.vextadd_pairwise(I16X8, I32X4, false)?,
            128 => self.vun_intr(I16X8, "abs", "false"),
            129 => {
                let a = self.pop_v();
                let av = self.vcast(&a, I16X8);
                let t = self.tmp();
                self.emit(format!(
                    "  {} = sub <8 x i16> zeroinitializer, {}",
                    t, av
                ));
                self.push_back(&t, I16X8);
            }
            130 => self.vq15mulr(),
            131 => self.vall_true(I16X8),
            132 => self.vbitmask(I16X8),
            133 | 134 => self.vnarrow(I32X4, I16X8, sub == 133),
            135 => self.vextend_half(I8X16, I16X8, false, true)?,
            136 => self.vextend_half(I8X16, I16X8, true, true)?,
            137 => self.vextend_half(I8X16, I16X8, false, false)?,
            138 => self.vextend_half(I8X16, I16X8, true, false)?,
            139 => self.vshift(I16X8, "shl"),
            140 => self.vshift(I16X8, "ashr"),
            141 => self.vshift(I16X8, "lshr"),
            142 => self.vbin(I16X8, "add"),
            143 => self.vbin_intr(I16X8, "sadd.sat"),
            144 => self.vbin_intr(I16X8, "uadd.sat"),
            145 => self.vbin(I16X8, "sub"),
            146 => self.vbin_intr(I16X8, "ssub.sat"),
            147 => self.vbin_intr(I16X8, "usub.sat"),
            148 => self.vfloat_un(F64X2, 3, true),
            149 => self.vbin(I16X8, "mul"),
            150 => self.vbin_intr(I16X8, "smin"),
            151 => self.vbin_intr(I16X8, "umin"),
            152 => self.vbin_intr(I16X8, "smax"),
            153 => self.vbin_intr(I16X8, "umax"),
            155 => self.vavgr(I16X8),
            156 => self.vextmul(I8X16, I16X8, false, true),
            157 => self.vextmul(I8X16, I16X8, true, true),
            158 => self.vextmul(I8X16, I16X8, false, false),
            159 => self.vextmul(I8X16, I16X8, true, false),
            160 => self.vun_intr(I32X4, "abs", "false"),
            161 => {
                let a = self.pop_v();
                let av = self.vcast(&a, I32X4);
                let t = self.tmp();
                self.emit(format!(
                    "  {} = sub <4 x i32> zeroinitializer, {}",
                    t, av
                ));
                self.push_back(&t, I32X4);
            }
            163 => self.vall_true(I32X4),
            164 => self.vbitmask(I32X4),
            167 => self.vextend_half(I16X8, I32X4, false, true)?,
            168 => self.vextend_half(I16X8, I32X4, true, true)?,
            169 => self.vextend_half(I16X8, I32X4, false, false)?,
            170 => self.vextend_half(I16X8, I32X4, true, false)?,
            171 => self.vshift(I32X4, "shl"),
            172 => self.vshift(I32X4, "ashr"),
            173 => self.vshift(I32X4, "lshr"),
            174 => self.vbin(I32X4, "add"),
            177 => self.vbin(I32X4, "sub"),
            181 => self.vbin(I32X4, "mul"),
            182 => self.vbin_intr(I32X4, "smin"),
            183 => self.vbin_intr(I32X4, "umin"),
            184 => self.vbin_intr(I32X4, "smax"),
            185 => self.vbin_intr(I32X4, "umax"),
            186 => self.vdot(),
            188 => self.vextmul(I16X8, I32X4, false, true),
            189 => self.vextmul(I16X8, I32X4, true, true),
            190 => self.vextmul(I16X8, I32X4, false, false),
            191 => self.vextmul(I16X8, I32X4, true, false),
            192 => self.vun_intr(I64X2, "abs", "false"),
            193 => {
                let a = self.pop_v();
                let av = self.vcast(&a, I64X2);
                let t = self.tmp();
                self.emit(format!(
                    "  {} = sub <2 x i64> zeroinitializer, {}",
                    t, av
                ));
                self.push_back(&t, I64X2);
            }
            195 => self.vall_true(I64X2),
            196 => self.vbitmask(I64X2),
            199 => self.vextend_half(I32X4, I64X2, false, true)?,
            200 => self.vextend_half(I32X4, I64X2, true, true)?,
            201 => self.vextend_half(I32X4, I64X2, false, false)?,
            202 => self.vextend_half(I32X4, I64X2, true, false)?,
            203 => self.vshift(I64X2, "shl"),
            204 => self.vshift(I64X2, "ashr"),
            205 => self.vshift(I64X2, "lshr"),
            206 => self.vbin(I64X2, "add"),
            209 => self.vbin(I64X2, "sub"),
            213 => self.vbin(I64X2, "mul"),
            220 => self.vextmul(I32X4, I64X2, false, true),
            221 => self.vextmul(I32X4, I64X2, true, true),
            222 => self.vextmul(I32X4, I64X2, false, false),
            223 => self.vextmul(I32X4, I64X2, true, false),
            224 => self.vfloat_un(F32X4, 4, false),
            225 => self.vfloat_un(F32X4, 5, false),
            227 => self.vfloat_un(F32X4, 6, false),
            228 => self.vbin(F32X4, "fadd"),
            229 => self.vbin(F32X4, "fsub"),
            230 => self.vbin(F32X4, "fmul"),
            231 => self.vbin(F32X4, "fdiv"),
            232 => self.vminmax(F32X4, false),
            233 => self.vminmax(F32X4, true),
            234 => self.vpminmax(F32X4, false),
            235 => self.vpminmax(F32X4, true),
            236 => self.vfloat_un(F64X2, 4, false),
            237 => self.vfloat_un(F64X2, 5, false),
            239 => self.vfloat_un(F64X2, 6, false),
            240 => self.vbin(F64X2, "fadd"),
            241 => self.vbin(F64X2, "fsub"),
            242 => self.vbin(F64X2, "fmul"),
            243 => self.vbin(F64X2, "fdiv"),
            244 => self.vminmax(F64X2, false),
            245 => self.vminmax(F64X2, true),
            246 => self.vpminmax(F64X2, false),
            247 => self.vpminmax(F64X2, true),
            248 => {
                let a = self.pop_v();
                let av = self.vcast(&a, F32X4);
                let t = self.call_intr(
                    I32X4,
                    "llvm.fptosi.sat.v4i32.v4f32",
                    F32X4,
                    &format!("<4 x float> {}", av),
                );
                self.push_back(&t, I32X4);
            }
            249 => {
                let a = self.pop_v();
                let av = self.vcast(&a, F32X4);
                let t = self.call_intr(
                    I32X4,
                    "llvm.fptoui.sat.v4i32.v4f32",
                    F32X4,
                    &format!("<4 x float> {}", av),
                );
                self.push_back(&t, I32X4);
            }
            250 => {
                let a = self.pop_v();
                let av = self.vcast(&a, I32X4);
                let t = self.tmp();
                self.emit(format!("  {} = sitofp <4 x i32> {} to <4 x float>", t, av));
                self.push_back(&t, F32X4);
            }
            251 => {
                let a = self.pop_v();
                let av = self.vcast(&a, I32X4);
                let t = self.tmp();
                self.emit(format!("  {} = uitofp <4 x i32> {} to <4 x float>", t, av));
                self.push_back(&t, F32X4);
            }
            252 | 253 => {
                let a = self.pop_v();
                let av = self.vcast(&a, F64X2);
                let name = if sub == 252 {
                    "llvm.fptosi.sat.v2i32.v2f64"
                } else {
                    "llvm.fptoui.sat.v2i32.v2f64"
                };
                let t = self.call_intr(
                    "<2 x i32>",
                    name,
                    F64X2,
                    &format!("<2 x double> {}", av),
                );
                let sh = self.tmp();
                self.emit(format!(
                    "  {} = shufflevector <2 x i32> {}, <2 x i32> zeroinitializer, <4 x i32> <i32 0, i32 1, i32 2, i32 3>",
                    sh, t
                ));
                self.push_back(&sh, I32X4);
            }
            254 => {
                let a = self.pop_v();
                let av = self.vcast(&a, I32X4);
                let sh = self.tmp();
                self.emit(format!(
                    "  {} = shufflevector <4 x i32> {}, <4 x i32> zeroinitializer, <2 x i32> <i32 0, i32 1>",
                    sh, av
                ));
                let t = self.tmp();
                self.emit(format!("  {} = sitofp <2 x i32> {} to <2 x double>", t, sh));
                self.push_back(&t, F64X2);
            }
            255 => {
                let a = self.pop_v();
                let av = self.vcast(&a, I32X4);
                let sh = self.tmp();
                self.emit(format!(
                    "  {} = shufflevector <4 x i32> {}, <4 x i32> zeroinitializer, <2 x i32> <i32 0, i32 1>",
                    sh, av
                ));
                let t = self.tmp();
                self.emit(format!("  {} = uitofp <2 x i32> {} to <2 x double>", t, sh));
                self.push_back(&t, F64X2);
            }
            _ => return Err(format!("unimplemented SIMD opcode {}", sub)),
        }
        let _ = I8X16;
        let _ = I16X8;
        let _ = I32X4;
        let _ = I64X2;
        let _ = F32X4;
        let _ = F64X2;
        let _ = V;
        let _: ValType = ValType::I32;
        let _: ValType = ValType::I64;
        let _: ValType = ValType::F32;
        let _: ValType = ValType::F64;
        Ok(())
    }

    fn vfloat_un(&mut self, lane_ty: &str, k: u32, _is32: bool) {
        let a = self.pop_v();
        let av = self.vcast(&a, lane_ty);
        match k {
            0 => {
                let t = self.call_intr(
                    lane_ty,
                    &format!("llvm.ceil.{}", suffix_of(lane_ty)),
                    lane_ty,
                    &format!("{} {}", lane_ty, av),
                );
                self.push_back(&t, lane_ty);
            }
            1 => {
                let t = self.call_intr(
                    lane_ty,
                    &format!("llvm.floor.{}", suffix_of(lane_ty)),
                    lane_ty,
                    &format!("{} {}", lane_ty, av),
                );
                self.push_back(&t, lane_ty);
            }
            2 => {
                let t = self.call_intr(
                    lane_ty,
                    &format!("llvm.trunc.{}", suffix_of(lane_ty)),
                    lane_ty,
                    &format!("{} {}", lane_ty, av),
                );
                self.push_back(&t, lane_ty);
            }
            3 => {
                let t = self.call_intr(
                    lane_ty,
                    &format!("llvm.roundeven.{}", suffix_of(lane_ty)),
                    lane_ty,
                    &format!("{} {}", lane_ty, av),
                );
                self.push_back(&t, lane_ty);
            }
            4 => {
                let t = self.call_intr(
                    lane_ty,
                    &format!("llvm.fabs.{}", suffix_of(lane_ty)),
                    lane_ty,
                    &format!("{} {}", lane_ty, av),
                );
                self.push_back(&t, lane_ty);
            }
            5 => {
                let t = self.tmp();
                self.emit(format!("  {} = fneg {} {}", t, lane_ty, av));
                self.push_back(&t, lane_ty);
            }
            _ => {
                let t = self.call_intr(
                    lane_ty,
                    &format!("llvm.sqrt.{}", suffix_of(lane_ty)),
                    lane_ty,
                    &format!("{} {}", lane_ty, av),
                );
                self.push_back(&t, lane_ty);
            }
        }
    }

    fn vpminmax(&mut self, lane_ty: &str, max: bool) {
        let b = self.pop_v();
        let a = self.pop_v();
        let av = self.vcast(&a, lane_ty);
        let bv = self.vcast(&b, lane_ty);
        let c = self.tmp();
        if max {
            self.emit(format!(
                "  {} = fcmp olt {} {}, {}",
                c, lane_ty, av, bv
            ));
        } else {
            self.emit(format!(
                "  {} = fcmp olt {} {}, {}",
                c, lane_ty, bv, av
            ));
        }
        let t = self.tmp();
        self.emit(format!(
            "  {} = select {} {}, {} {}, {} {}",
            t,
            mask_ty(lane_ty),
            c,
            lane_ty,
            bv,
            lane_ty,
            av
        ));
        self.push_back(&t, lane_ty);
    }

    fn vminmax(&mut self, lane_ty: &str, max: bool) {
        let b = self.pop_v();
        let a = self.pop_v();
        let av = self.vcast(&a, lane_ty);
        let bv = self.vcast(&b, lane_ty);
        let base = if max { "maximum" } else { "minimum" };
        let r = self.call_intr(
            lane_ty,
            &format!("llvm.{}.{}", base, suffix_of(lane_ty)),
            &format!("{}, {}", lane_ty, lane_ty),
            &format!("{} {}, {} {}", lane_ty, av, lane_ty, bv),
        );
        let (bits_ty, eb, qbit) = if lane_ty == F32X4 {
            ("<4 x i32>", "i32", "4194304")
        } else {
            ("<2 x i64>", "i64", "2251799813685248")
        };
        let isn = self.tmp();
        self.emit(format!(
            "  {} = fcmp uno {} {}, {}",
            isn, lane_ty, r, r
        ));
        let bits = self.tmp();
        self.emit(format!(
            "  {} = bitcast {} {} to {}",
            bits, lane_ty, r, bits_ty
        ));
        let qv = const_vec(bits_ty, eb, qbit);
        let q = self.tmp();
        self.emit(format!(
            "  {} = or {} {}, {}",
            q, bits_ty, bits, qv
        ));
        let sel = self.tmp();
        self.emit(format!(
            "  {} = select {} {}, {} {}, {} {}",
            sel,
            mask_ty(lane_ty),
            isn,
            bits_ty,
            q,
            bits_ty,
            bits
        ));
        let res = self.tmp();
        self.emit(format!(
            "  {} = bitcast {} {} to {}",
            res, bits_ty, sel, lane_ty
        ));
        self.push_back(&res, lane_ty);
    }

    fn vavgr(&mut self, lane_ty: &str) {
        let wide = if lane_ty == I8X16 { "<16 x i16>" } else { "<8 x i32>" };
        let b = self.pop_v();
        let a = self.pop_v();
        let av = self.vcast(&a, lane_ty);
        let bv = self.vcast(&b, lane_ty);
        let aw = self.tmp();
        self.emit(format!("  {} = zext {} {} to {}", aw, lane_ty, av, wide));
        let bw = self.tmp();
        self.emit(format!("  {} = zext {} {} to {}", bw, lane_ty, bv, wide));
        let sum = self.tmp();
        self.emit(format!("  {} = add {} {}, {}", sum, wide, aw, bw));
        let one = const_vec(wide, ety_of(wide), "1");
        let s1 = self.tmp();
        self.emit(format!("  {} = add {} {}, {}", s1, wide, sum, one));
        let sh = self.tmp();
        self.emit(format!("  {} = lshr {} {}, {}", sh, wide, s1, one));
        let tr = self.tmp();
        self.emit(format!("  {} = trunc {} {} to {}", tr, wide, sh, lane_ty));
        self.push_back(&tr, lane_ty);
    }

    fn vq15mulr(&mut self) {
        let b = self.pop_v();
        let a = self.pop_v();
        let av = self.vcast(&a, I16X8);
        let bv = self.vcast(&b, I16X8);
        let aw = self.tmp();
        self.emit(format!("  {} = sext <8 x i16> {} to <8 x i32>", aw, av));
        let bw = self.tmp();
        self.emit(format!("  {} = sext <8 x i16> {} to <8 x i32>", bw, bv));
        let m = self.tmp();
        self.emit(format!("  {} = mul <8 x i32> {}, {}", m, aw, bw));
        let round = const_vec(I32X4, "i32", "16384");
        let _ = round;
        let rv = const_vec("<8 x i32>", "i32", "16384");
        let r = self.tmp();
        self.emit(format!("  {} = add <8 x i32> {}, {}", r, m, rv));
        let sh = self.tmp();
        let fifteen = const_vec("<8 x i32>", "i32", "15");
        self.emit(format!("  {} = ashr <8 x i32> {}, {}", sh, r, fifteen));
        let lo = const_vec("<8 x i32>", "i32", "-32768");
        let hi = const_vec("<8 x i32>", "i32", "32767");
        let c1 = self.call_intr(
            "<8 x i32>",
            "llvm.smax.v8i32",
            "<8 x i32>, <8 x i32>",
            &format!("<8 x i32> {}, <8 x i32> {}", sh, lo),
        );
        let c2 = self.call_intr(
            "<8 x i32>",
            "llvm.smin.v8i32",
            "<8 x i32>, <8 x i32>",
            &format!("<8 x i32> {}, <8 x i32> {}", c1, hi),
        );
        let tr = self.tmp();
        self.emit(format!("  {} = trunc <8 x i32> {} to <8 x i16>", tr, c2));
        self.push_back(&tr, I16X8);
    }

    fn vdot(&mut self) {
        let b = self.pop_v();
        let a = self.pop_v();
        let av = self.vcast(&a, I16X8);
        let bv = self.vcast(&b, I16X8);
        let even = idx_mask(&[0, 2, 4, 6]);
        let odd = idx_mask(&[1, 3, 5, 7]);
        let ae = self.tmp();
        self.emit(format!(
            "  {} = shufflevector <8 x i16> {}, <8 x i16> zeroinitializer, {}",
            ae, av, even
        ));
        let ao = self.tmp();
        self.emit(format!(
            "  {} = shufflevector <8 x i16> {}, <8 x i16> zeroinitializer, {}",
            ao, av, odd
        ));
        let be = self.tmp();
        self.emit(format!(
            "  {} = shufflevector <8 x i16> {}, <8 x i16> zeroinitializer, {}",
            be, bv, even
        ));
        let bo = self.tmp();
        self.emit(format!(
            "  {} = shufflevector <8 x i16> {}, <8 x i16> zeroinitializer, {}",
            bo, bv, odd
        ));
        let aew = self.tmp();
        self.emit(format!("  {} = sext <4 x i16> {} to <4 x i32>", aew, ae));
        let aow = self.tmp();
        self.emit(format!("  {} = sext <4 x i16> {} to <4 x i32>", aow, ao));
        let bew = self.tmp();
        self.emit(format!("  {} = sext <4 x i16> {} to <4 x i32>", bew, be));
        let bow = self.tmp();
        self.emit(format!("  {} = sext <4 x i16> {} to <4 x i32>", bow, bo));
        let p1 = self.tmp();
        self.emit(format!("  {} = mul <4 x i32> {}, {}", p1, aew, bew));
        let p2 = self.tmp();
        self.emit(format!("  {} = mul <4 x i32> {}, {}", p2, aow, bow));
        let r = self.tmp();
        self.emit(format!("  {} = add <4 x i32> {}, {}", r, p1, p2));
        self.push_back(&r, I32X4);
    }

    // ------------------------------------------------------------------
    // init and exports
    // ------------------------------------------------------------------

    fn eval_const(instrs: &[Instr]) -> ConstVal {
        for ins in instrs {
            match ins {
                Instr::I32Const(v) => return ConstVal::I32(*v),
                Instr::I64Const(v) => return ConstVal::I64(*v),
                Instr::F32Const(v) => return ConstVal::F32(*v),
                Instr::F64Const(v) => return ConstVal::F64(*v),
                Instr::Simd(s) => {
                    if let SimdKind::Const(b) = s.kind {
                        return ConstVal::V128(b);
                    }
                }
                Instr::End => break,
                _ => {}
            }
        }
        ConstVal::I32(0)
    }

    fn gen_init(&mut self) -> Result<(), String> {
        let mut s = String::new();
        s.push_str("define void @w2l_init() {\n");
        s.push_str("entry:\n");
        s.push_str("  store i32 0, ptr @w2l.depth, align 4\n");

        if !self.m.memories.is_empty() {
            let min_pages = self.m.memories[0].min;
            let min_bytes = min_pages * 65536;
            let alloc = min_bytes.max(65536);
            s.push_str(&format!(
                "  %mp = call ptr @mmap(ptr null, i64 {}, i32 3, i32 34, i32 -1, i64 0)\n",
                alloc
            ));
            s.push_str("  store ptr %mp, ptr @mem.base, align 8\n");
            s.push_str(&format!("  store i64 {}, ptr @mem.size, align 8\n", min_bytes));
        }
        if !self.m.tables.is_empty() {
            let n = self.m.tables[0].min;
            s.push_str(&format!(
                "  %tp = call ptr @calloc(i64 {}, i64 8)\n",
                n
            ));
            s.push_str("  store ptr %tp, ptr @tbl.base, align 8\n");
            s.push_str(&format!(
                "  %tt = call ptr @calloc(i64 {}, i64 4)\n",
                n
            ));
            s.push_str("  store ptr %tt, ptr @tbl.types, align 8\n");
            s.push_str(&format!("  store i64 {}, ptr @tbl.size, align 8\n", n));
        }

        // globals
        for (i, g) in self.m.globals.iter().enumerate() {
            let v = Self::eval_const(&g.init);
            let repr = match v {
                ConstVal::I32(x) => format!("i32 {}", x),
                ConstVal::I64(x) => format!("i64 {}", x),
                ConstVal::F32(b) => format!("float f0x{:08x}", b),
                ConstVal::F64(b) => format!("double f0x{:016x}", b),
                ConstVal::V128(b) => {
                    let items: Vec<String> = b.iter().map(|x| format!("i8 {}", x)).collect();
                    format!("<16 x i8> <{}>", items.join(", "))
                }
            };
            s.push_str(&format!(
                "  store {}, ptr @g{}, align {}\n",
                repr,
                i,
                align_of(g.ty)
            ));
        }

        // active element segments
        for (i, e) in self.m.elems.iter().enumerate() {
            if e.mode != ElemMode::Active {
                continue;
            }
            let off = match Self::eval_const(&e.offset) {
                ConstVal::I32(v) => v as u32 as u64,
                _ => 0,
            };
            let n = e.funcs.len() as u64;
            let tb = self.new_label();
            let ok = self.new_label();
            s.push_str(&format!("  %tsz{i} = load i64, ptr @tbl.size, align 8\n"));
            s.push_str(&format!("  %tend{i} = add i64 {}, {}\n", off, n));
            s.push_str(&format!(
                "  %tbad{i} = icmp ugt i64 %tend{i}, %tsz{i}\n"
            ));
            s.push_str(&format!(
                "  br i1 %tbad{i}, label %{}, label %{}\n",
                tb, ok
            ));
            s.push_str(&format!("{}:\n", tb));
            s.push_str("  call void @w2l_trap(i32 5)\n  unreachable\n");
            s.push_str(&format!("{}:\n", ok));
            if n > 0 {
                s.push_str(&format!("  %tbase{i} = load ptr, ptr @tbl.base, align 8\n"));
                s.push_str(&format!(
                    "  %tdst{i} = getelementptr i8, ptr %tbase{i}, i64 {}\n",
                    off * 8
                ));
                s.push_str(&format!(
                    "  call void @llvm.memcpy.p0.p0.i64(ptr %tdst{i}, ptr @e{}_p, i64 {}, i1 false)\n",
                    i,
                    n * 8
                ));
                s.push_str(&format!("  %tty{i} = load ptr, ptr @tbl.types, align 8\n"));
                s.push_str(&format!(
                    "  %tdst{i}2{i} = getelementptr i8, ptr %tty{i}, i64 {}\n",
                    off * 4
                ));
                s.push_str(&format!(
                    "  call void @llvm.memcpy.p0.p0.i64(ptr %tdst{i}2{i}, ptr @e{}_t, i64 {}, i1 false)\n",
                    i,
                    n * 4
                ));
            }
            self.need_decl("declare void @llvm.memcpy.p0.p0.i64(ptr, ptr, i64, i1)");
            self.need_decl("declare void @w2l_trap(i32) noreturn");
        }

        // active data segments
        for (i, d) in self.m.datas.iter().enumerate() {
            if d.mode != DataMode::Active {
                continue;
            }
            let off = match Self::eval_const(&d.offset) {
                ConstVal::I32(v) => v as u32 as u64,
                _ => 0,
            };
            let n = d.data.len() as u64;
            let tb = self.new_label();
            let ok = self.new_label();
            s.push_str(&format!("  %msz{i} = load i64, ptr @mem.size, align 8\n"));
            s.push_str(&format!("  %mend{i} = add i64 {}, {}\n", off, n));
            s.push_str(&format!("  %mbad{i} = icmp ugt i64 %mend{i}, %msz{i}\n"));
            s.push_str(&format!(
                "  br i1 %mbad{i}, label %{}, label %{}\n",
                tb, ok
            ));
            s.push_str(&format!("{}:\n", tb));
            s.push_str("  call void @w2l_trap(i32 4)\n  unreachable\n");
            s.push_str(&format!("{}:\n", ok));
            if n > 0 {
                s.push_str(&format!("  %mbase{i} = load ptr, ptr @mem.base, align 8\n"));
                s.push_str(&format!(
                    "  %mdst{i} = getelementptr i8, ptr %mbase{i}, i64 {}\n",
                    off
                ));
                s.push_str(&format!(
                    "  call void @llvm.memcpy.p0.p0.i64(ptr %mdst{i}, ptr @d{}, i64 {}, i1 false)\n",
                    i, n
                ));
            }
            self.need_decl("declare void @llvm.memcpy.p0.p0.i64(ptr, ptr, i64, i1)");
            self.need_decl("declare void @w2l_trap(i32) noreturn");
        }

        if let Some(st) = self.m.start {
            s.push_str(&format!("  call void @f{}()\n", st));
        }
        s.push_str("  ret void\n}\n");
        self.funcs.push_str(&s);
        Ok(())
    }

    fn gen_exports(&mut self) -> Result<(), String> {
        let mut out = String::new();
        for e in &self.m.exports {
            match e.kind {
                0 => {
                    let sym = format!("w2l_export_{}", hex_name(&e.name));
                    out.push_str(&self.gen_func_export(&sym, e.idx)?);
                }
                2 => {
                    let sym = format!("w2l_memory_{}", hex_name(&e.name));
                    out.push_str(&format!(
                        "define ptr @{}(ptr %size_out) {{\nentry:\n  %s = load i64, ptr @mem.size, align 8\n  store i64 %s, ptr %size_out, align 8\n  %b = load ptr, ptr @mem.base, align 8\n  ret ptr %b\n}}\n",
                        sym
                    ));
                }
                3 => {
                    let sym = format!("w2l_global_{}", hex_name(&e.name));
                    let g = &self.m.globals[e.idx as usize];
                    out.push_str(&format!(
                        "define void @{}(ptr %out) {{\nentry:\n  %v = load {}, ptr @g{}, align {}\n  store {} %v, ptr %out, align {}\n  ret void\n}}\n",
                        sym,
                        ll_ty(g.ty),
                        e.idx,
                        align_of(g.ty),
                        ll_ty(g.ty),
                        align_of(g.ty)
                    ));
                }
                1 => { /* exported tables need no symbol */ }
                _ => {}
            }
        }
        self.funcs.push_str(&out);
        Ok(())
    }

    fn gen_func_export(&mut self, sym: &str, idx: u32) -> Result<String, String> {
        let ft = self
            .m
            .func_type(idx)
            .ok_or_else(|| format!("unknown function {}", idx))?
            .clone();
        let mut s = String::new();
        s.push_str(&format!(
            "define void @{}(ptr %args, ptr %results) {{\nentry:\n",
            sym
        ));
        s.push_str("  store i32 0, ptr @w2l.depth, align 4\n");
        let n_imports = self.m.imports.len();
        let is_import = (idx as usize) < n_imports;
        let mut argnames = Vec::new();
        let load_tmp = |s: &mut String, ct: &mut usize, t: ValType, base: &str, off: usize| {
            let p = if off == 0 {
                if *ct == 0 {
                    format!("%a{}", ct)
                } else {
                    format!("%a{}", ct)
                }
            } else {
                format!("%a{}", ct)
            };
            *ct += 1;
            if off == 0 {
                s.push_str(&format!(
                    "  {} = load {}, ptr %{}, align {}\n",
                    p,
                    ll_ty(t),
                    base,
                    align_of(t)
                ));
            } else {
                let gp = format!("%ag{}", off);
                s.push_str(&format!(
                    "  {} = getelementptr i8, ptr %{}, i64 {}\n",
                    gp, base, off
                ));
                s.push_str(&format!(
                    "  {} = load {}, ptr {}, align {}\n",
                    p,
                    ll_ty(t),
                    gp,
                    align_of(t)
                ));
            }
            p
        };
        let mut ct = 0usize;
        for (i, t) in ft.params.iter().enumerate() {
            let p = load_tmp(&mut s, &mut ct, *t, "args", i * 16);
            argnames.push(format!("{} {}", ll_ty(*t), p));
        }
        if is_import {
            let imp = &self.m.imports[idx as usize];
            let isym = format!(
                "w2l_import_{}_{}",
                hex_name(&imp.module),
                hex_name(&imp.name)
            );
            s.push_str(&format!(
                "  call void @{}(ptr %args, ptr %results)\n  ret void\n}}\n",
                isym
            ));
            return Ok(s);
        }
        let argstr = argnames.join(", ");
        if ft.results.is_empty() {
            s.push_str(&format!("  call void @f{}({})\n", idx, argstr));
        } else if ft.results.len() == 1 {
            let t = ft.results[0];
            s.push_str(&format!(
                "  %rv = call {} @f{}({})\n",
                ll_ty(t),
                idx,
                argstr
            ));
            s.push_str(&format!(
                "  store {} %rv, ptr %results, align {}\n",
                ll_ty(t),
                align_of(t)
            ));
        } else {
            let st = ret_ty(&ft.results);
            s.push_str(&format!("  %rv = call {} @f{}({})\n", st, idx, argstr));
            for (i, t) in ft.results.iter().enumerate() {
                let e = format!("%e{}", i);
                s.push_str(&format!("  {} = extractvalue {} %rv, {}\n", e, st, i));
                if i == 0 {
                    s.push_str(&format!(
                        "  store {} {}, ptr %results, align {}\n",
                        ll_ty(*t),
                        e,
                        align_of(*t)
                    ));
                } else {
                    s.push_str(&format!(
                        "  %ro{} = getelementptr i8, ptr %results, i64 {}\n",
                        i,
                        i * 16
                    ));
                    s.push_str(&format!(
                        "  store {} {}, ptr %ro{}, align {}\n",
                        ll_ty(*t),
                        e,
                        i,
                        align_of(*t)
                    ));
                }
            }
        }
        s.push_str("  ret void\n}\n");
        Ok(s)
    }

    // ------------------------------------------------------------------
    // final assembly
    // ------------------------------------------------------------------

    fn finish(&mut self) -> String {
        let mut s = String::new();
        s.push_str("; generated by wasm2llvm\n");
        s.push_str("target triple = \"x86_64-unknown-linux-gnu\"\n\n");
        for c in &self.consts {
            s.push_str(c);
            s.push('\n');
        }
        s.push('\n');
        for g in &self.globals {
            s.push_str(g);
            s.push('\n');
        }
        s.push('\n');
        for d in &self.decls {
            s.push_str(d);
            s.push('\n');
        }
        s.push('\n');
        for h in &self.helpers {
            s.push_str(h);
            s.push('\n');
        }
        s.push_str(&self.funcs);
        s
    }
}

#[derive(Clone, Copy)]
enum ConstVal {
    I32(i32),
    I64(i64),
    F32(u32),
    F64(u64),
    V128([u8; 16]),
}

fn val_ty_from_ll(ty: &str) -> ValType {
    match ty {
        "i32" => ValType::I32,
        "i64" => ValType::I64,
        "float" => ValType::F32,
        "double" => ValType::F64,
        _ => ValType::V128,
    }
}

fn tsfx(t: ValType) -> &'static str {
    match t {
        ValType::I32 => "i32",
        ValType::I64 => "i64",
        ValType::F32 => "f32",
        ValType::F64 => "f64",
        ValType::V128 => "v16i8",
    }
}

fn ety_of(ty: &str) -> &'static str {
    match ty {
        "<16 x i8>" => "i8",
        "<8 x i16>" => "i16",
        "<4 x i32>" => "i32",
        "<2 x i64>" => "i64",
        "<4 x float>" => "float",
        "<2 x double>" => "double",
        "<16 x i16>" => "i16",
        "<8 x i32>" => "i32",
        _ => "i32",
    }
}

fn mask_suffix(ty: &str) -> &'static str {
    match ty {
        "<16 x i8>" => "v16i1",
        "<8 x i16>" => "v8i1",
        "<4 x i32>" => "v4i1",
        "<2 x i64>" => "v2i1",
        "<4 x float>" => "v4i1",
        "<2 x double>" => "v2i1",
        _ => "v4i1",
    }
}

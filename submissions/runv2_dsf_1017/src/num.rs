// Numeric and SIMD instruction lowering.

use crate::codegen::{llvm_ty, FuncGen, Val};
use crate::instr::*;
use crate::module::ValType;

impl<'a> FuncGen<'a> {
    // -----------------------------------------------------------------
    // Scalar numeric instructions
    // -----------------------------------------------------------------

    pub fn gen_numeric(&mut self, instr: &Instr) {
        match instr {
            Instr::Un(op) => self.gen_un(*op),
            Instr::Bin(op) => self.gen_bin(*op),
            Instr::Test(op) => self.gen_test(*op),
            Instr::Rel(op) => self.gen_rel(*op),
            Instr::Cvt(op) => self.gen_cvt(*op),
            _ => unreachable!(),
        }
    }

    fn gen_un(&mut self, op: UnOp) {
        use UnOp::*;
        match op {
            I32Clz | I64Clz | I32Ctz | I64Ctz | I32Popcnt | I64Popcnt => {
                let (ty, name, decl) = match op {
                    I32Clz => ("i32", "llvm.ctlz.i32", "declare i32 @llvm.ctlz.i32(i32, i1)"),
                    I64Clz => ("i64", "llvm.ctlz.i64", "declare i64 @llvm.ctlz.i64(i64, i1)"),
                    I32Ctz => ("i32", "llvm.cttz.i32", "declare i32 @llvm.cttz.i32(i32, i1)"),
                    I64Ctz => ("i64", "llvm.cttz.i64", "declare i64 @llvm.cttz.i64(i64, i1)"),
                    I32Popcnt => ("i32", "llvm.ctpop.i32", "declare i32 @llvm.ctpop.i32(i32)"),
                    _ => ("i64", "llvm.ctpop.i64", "declare i64 @llvm.ctpop.i64(i64)"),
                };
                let a = self.pop();
                let sym = self.reg_decl(name, decl);
                let r = if name.contains("ctpop") {
                    self.val(
                        if ty == "i32" { ValType::I32 } else { ValType::I64 },
                        format!("call {} {}({} {})", ty, sym, ty, a.s),
                    )
                } else {
                    self.val(
                        if ty == "i32" { ValType::I32 } else { ValType::I64 },
                        format!("call {} {}({} {}, i1 false)", ty, sym, ty, a.s),
                    )
                };
                self.push(r);
            }
            I32Extend8S | I32Extend16S | I64Extend8S | I64Extend16S | I64Extend32S => {
                let a = self.pop();
                let (src_bits, dst) = match op {
                    I32Extend8S => (8, ValType::I32),
                    I32Extend16S => (16, ValType::I32),
                    I64Extend8S => (8, ValType::I64),
                    I64Extend16S => (16, ValType::I64),
                    _ => (32, ValType::I64),
                };
                let small = format!("i{}", src_bits);
                let big = llvm_ty(dst);
                let t = self.new_reg(format!("trunc {} {} to {}", llvm_ty(op.ty()), a.s, small));
                let r = self.val(dst, format!("sext {} {} to {}", small, t, big));
                self.push(r);
            }
            F32Abs | F64Abs => {
                let (ty, name, decl) = if op == F32Abs {
                    (
                        "float",
                        "llvm.fabs.f32",
                        "declare float @llvm.fabs.f32(float)",
                    )
                } else {
                    (
                        "double",
                        "llvm.fabs.f64",
                        "declare double @llvm.fabs.f64(double)",
                    )
                };
                let a = self.pop();
                let sym = self.reg_decl(name, decl);
                let r = self.val(
                    if op == F32Abs { ValType::F32 } else { ValType::F64 },
                    format!("call {} {}({} {})", ty, sym, ty, a.s),
                );
                self.push(r);
            }
            F32Neg | F64Neg => {
                let a = self.pop();
                let r = self.val(op.ty(), format!("fneg {} {}", a.lty, a.s));
                self.push(r);
            }
            F32Ceil | F32Floor | F32Trunc | F32Nearest | F32Sqrt | F64Ceil | F64Floor
            | F64Trunc | F64Nearest | F64Sqrt => {
                let (ty, base) = if matches!(
                    op,
                    F32Ceil | F32Floor | F32Trunc | F32Nearest | F32Sqrt
                ) {
                    ("float", "f32")
                } else {
                    ("double", "f64")
                };
                let fname = match op {
                    F32Ceil | F64Ceil => "ceil",
                    F32Floor | F64Floor => "floor",
                    F32Trunc | F64Trunc => "trunc",
                    F32Nearest | F64Nearest => "roundeven",
                    _ => "sqrt",
                };
                let name = format!("llvm.{}.{}", fname, base);
                let decl = format!("declare {} @{}({})", ty, name, ty);
                let a = self.pop();
                let sym = self.reg_decl(&name, &decl);
                let r = self.val(op.ty(), format!("call {} {}({} {})", ty, sym, ty, a.s));
                self.push(r);
            }
        }
    }

    fn bin_ty(op: BinOp) -> ValType {
        op.ty()
    }

    fn gen_bin(&mut self, op: BinOp) {
        use BinOp::*;
        let ty = Self::bin_ty(op);
        match op {
            I32Add | I64Add => self.bin_simple(op, "add"),
            I32Sub | I64Sub => self.bin_simple(op, "sub"),
            I32Mul | I64Mul => self.bin_simple(op, "mul"),
            I32And | I64And => self.bin_simple(op, "and"),
            I32Or | I64Or => self.bin_simple(op, "or"),
            I32Xor | I64Xor => self.bin_simple(op, "xor"),
            I32DivU | I64DivU => {
                let b = self.pop();
                let a = self.pop();
                let zero = self.new_reg(format!("icmp eq {} {}, 0", llvm_ty(ty), b.s));
                self.branch_trap(&zero, 1);
                let r = self.val(ty, format!("udiv {} {}, {}", llvm_ty(ty), a.s, b.s));
                self.push(r);
            }
            I32DivS | I64DivS => {
                let b = self.pop();
                let a = self.pop();
                let ity = llvm_ty(ty);
                let zero = self.new_reg(format!("icmp eq {} {}, 0", ity, b.s));
                self.branch_trap(&zero, 1);
                let minv = if ty == ValType::I32 {
                    i64::from(i32::MIN) as i32 as i64
                } else {
                    i64::MIN
                };
                let a_min = self.new_reg(format!("icmp eq {} {}, {}", ity, a.s, minv));
                let b_m1 = self.new_reg(format!("icmp eq {} {}, -1", ity, b.s));
                let ovf = self.new_reg(format!("and i1 {}, {}", a_min, b_m1));
                self.branch_trap(&ovf, 2);
                let r = self.val(ty, format!("sdiv {} {}, {}", ity, a.s, b.s));
                self.push(r);
            }
            I32RemU | I64RemU => {
                let b = self.pop();
                let a = self.pop();
                let zero = self.new_reg(format!("icmp eq {} {}, 0", llvm_ty(ty), b.s));
                self.branch_trap(&zero, 1);
                let r = self.val(ty, format!("urem {} {}, {}", llvm_ty(ty), a.s, b.s));
                self.push(r);
            }
            I32RemS | I64RemS => {
                let b = self.pop();
                let a = self.pop();
                let ity = llvm_ty(ty);
                let zero = self.new_reg(format!("icmp eq {} {}, 0", ity, b.s));
                self.branch_trap(&zero, 1);
                let ism1 = self.new_reg(format!("icmp eq {} {}, -1", ity, b.s));
                let safe = self.new_reg(format!("select i1 {}, {} 1, {} {}", ism1, ity, ity, b.s));
                let r0 = self.new_reg(format!("srem {} {}, {}", ity, a.s, safe));
                let r = self.val(ty, format!("select i1 {}, {} 0, {} {}", ism1, ity, ity, r0));
                self.push(r);
            }
            I32Shl | I64Shl | I32ShrS | I64ShrS | I32ShrU | I64ShrU => {
                let bits = if ty == ValType::I32 { 31 } else { 63 };
                let b = self.pop();
                let a = self.pop();
                let c = self.new_reg(format!("and {} {}, {}", llvm_ty(ty), b.s, bits));
                let name = match op {
                    I32Shl | I64Shl => "shl",
                    I32ShrS | I64ShrS => "ashr",
                    _ => "lshr",
                };
                let r = self.val(
                    ty,
                    format!("{} {} {}, {}", name, llvm_ty(ty), a.s, c),
                );
                self.push(r);
            }
            I32Rotl | I64Rotl | I32Rotr | I64Rotr => {
                let bits = if ty == ValType::I32 { 31 } else { 63 };
                let b = self.pop();
                let a = self.pop();
                let c = self.new_reg(format!("and {} {}, {}", llvm_ty(ty), b.s, bits));
                let base = if ty == ValType::I32 { "i32" } else { "i64" };
                let fname = if matches!(op, I32Rotl | I64Rotl) {
                    "llvm.fshl"
                } else {
                    "llvm.fshr"
                };
                let iname = format!("{}.{}", fname, base);
                let decl = format!("declare {} @{}({}, {}, {})", base, iname, base, base, base);
                let sym = self.reg_decl(&iname, &decl);
                let r = self.val(
                    ty,
                    format!("call {} {}({} {}, {} {}, {} {})", base, sym, base, a.s, base, a.s, base, c),
                );
                self.push(r);
            }
            F32Add | F64Add => self.bin_simple(op, "fadd"),
            F32Sub | F64Sub => self.bin_simple(op, "fsub"),
            F32Mul | F64Mul => self.bin_simple(op, "fmul"),
            F32Div | F64Div => self.bin_simple(op, "fdiv"),
            F32Min | F64Min | F32Max | F64Max => {
                let (fname, base) = if ty == ValType::F32 {
                    ("f32", "float")
                } else {
                    ("f64", "double")
                };
                let which = if matches!(op, F32Min | F64Min) {
                    "minimum"
                } else {
                    "maximum"
                };
                let name = format!("llvm.{}.{}", which, fname);
                let decl = format!("declare {} @{}({}, {})", base, name, base, base);
                let b = self.pop();
                let a = self.pop();
                let sym = self.reg_decl(&name, &decl);
                let num = self.new_reg(format!(
                    "call {} {}({} {}, {} {})",
                    base, sym, base, a.s, base, b.s
                ));
                // The wasm NaN rules require an arithmetic (quiet) NaN if any
                // input is NaN; a canonical NaN is always permitted.
                let isnan = self.new_reg(format!("fcmp uno {} {}, {}", base, a.s, b.s));
                let nanlit = if ty == ValType::F32 {
                    "f0x7FC00000"
                } else {
                    "f0x7FF8000000000000"
                };
                let r = self.val(
                    ty,
                    format!(
                        "select i1 {}, {} {}, {} {}",
                        isnan, base, nanlit, base, num
                    ),
                );
                self.push(r);
            }
            F32Copysign | F64Copysign => {
                let (fname, base) = if ty == ValType::F32 {
                    ("f32", "float")
                } else {
                    ("f64", "double")
                };
                let name = format!("llvm.copysign.{}", fname);
                let decl = format!("declare {} @{}({}, {})", base, name, base, base);
                let b = self.pop();
                let a = self.pop();
                let sym = self.reg_decl(&name, &decl);
                let r = self.val(
                    ty,
                    format!("call {} {}({} {}, {} {})", base, sym, base, a.s, base, b.s),
                );
                self.push(r);
            }
        }
    }

    fn bin_simple(&mut self, op: BinOp, opcode: &str) {
        let ty = op.ty();
        let b = self.pop();
        let a = self.pop();
        let r = self.val(
            ty,
            format!("{} {} {}, {}", opcode, llvm_ty(ty), a.s, b.s),
        );
        self.push(r);
    }

    fn gen_test(&mut self, op: TestOp) {
        let a = self.pop();
        let c = self.new_reg(format!("icmp eq {} {}, 0", llvm_ty(op.ty()), a.s));
        let r = self.val(ValType::I32, format!("zext i1 {} to i32", c));
        self.push(r);
    }

    fn gen_rel(&mut self, op: RelOp) {
        use RelOp::*;
        let ty = op.ty();
        let is_float = matches!(
            op,
            F32Eq | F32Ne | F32Lt | F32Gt | F32Le | F32Ge | F64Eq | F64Ne | F64Lt | F64Gt
                | F64Le | F64Ge
        );
        let pred = match op {
            I32Eq | I64Eq | F32Eq | F64Eq => "eq",
            I32Ne | I64Ne | F32Ne | F64Ne => "ne",
            I32LtS | I64LtS => "slt",
            I32LtU | I64LtU => "ult",
            I32GtS | I64GtS => "sgt",
            I32GtU | I64GtU => "ugt",
            I32LeS | I64LeS => "sle",
            I32LeU | I64LeU => "ule",
            I32GeS | I64GeS => "sge",
            I32GeU | I64GeU => "uge",
            F32Lt | F64Lt => "olt",
            F32Gt | F64Gt => "ogt",
            F32Le | F64Le => "ole",
            F32Ge | F64Ge => "oge",
        };
        let pred = if is_float {
            match op {
                F32Eq | F64Eq => "oeq",
                F32Ne | F64Ne => "une",
                _ => pred,
            }
        } else {
            pred
        };
        let b = self.pop();
        let a = self.pop();
        let c = self.new_reg(format!(
            "{} {} {} {}, {}",
            if is_float { "fcmp" } else { "icmp" },
            pred,
            llvm_ty(ty),
            a.s,
            b.s
        ));
        let r = self.val(ValType::I32, format!("zext i1 {} to i32", c));
        self.push(r);
    }

    /// Bounds for trapping float->int conversions, as f0x bit patterns.
    fn trunc_bounds(_from: ValType, to: ValType, signed: bool, is_f32: bool) -> (&'static str, &'static str, bool, bool) {
        // returns (lo, hi, lo_strict, hi_strict)
        if signed {
            match (to, is_f32) {
                (ValType::I32, true) => ("f0xCF000000", "f0x4F000000", false, true), // >= -2^31, < 2^31
                (ValType::I32, false) => (
                    "f0xC1E0000000200000",
                    "f0x41E0000000000000",
                    true,
                    true,
                ), // > -(2^31+1), < 2^31
                (ValType::I64, true) => ("f0xDF000000", "f0x5F000000", false, true), // >= -2^63, < 2^63
                _ => (
                    "f0xC3E0000000000000",
                    "f0x43E0000000000000",
                    false,
                    true,
                ),
            }
        } else {
            match (to, is_f32) {
                (ValType::I32, true) => ("f0xBF800000", "f0x4F800000", true, true), // > -1, < 2^32
                (ValType::I32, false) => ("f0xBFF0000000000000", "f0x41F0000000000000", true, true),
                (ValType::I64, true) => ("f0xBF800000", "f0x5F800000", true, true), // > -1, < 2^64
                _ => ("f0xBFF0000000000000", "f0x43F0000000000000", true, true),
            }
        }
        .into()
    }

    fn gen_cvt(&mut self, op: CvtOp) {
        use CvtOp::*;
        let (from, to) = op.sig();
        match op {
            I32WrapI64 => {
                let a = self.pop();
                let r = self.val(to, format!("trunc i64 {} to i32", a.s));
                self.push(r);
            }
            I64ExtendI32S => {
                let a = self.pop();
                let r = self.val(to, format!("sext i32 {} to i64", a.s));
                self.push(r);
            }
            I64ExtendI32U => {
                let a = self.pop();
                let r = self.val(to, format!("zext i32 {} to i64", a.s));
                self.push(r);
            }
            I32ReinterpretF32 | I64ReinterpretF64 | F32ReinterpretI32 | F64ReinterpretI64 => {
                let a = self.pop();
                let r = self.val(to, format!("bitcast {} {} to {}", llvm_ty(from), a.s, llvm_ty(to)));
                self.push(r);
            }
            I32TruncF32S | I32TruncF64S | I64TruncF32S | I64TruncF64S => {
                self.gen_trunc(from, to, true);
            }
            I32TruncF32U | I32TruncF64U | I64TruncF32U | I64TruncF64U => {
                self.gen_trunc(from, to, false);
            }
            F32ConvertI32S | F32ConvertI64S | F64ConvertI32S | F64ConvertI64S => {
                let a = self.pop();
                let r = self.val(to, format!("sitofp {} {} to {}", llvm_ty(from), a.s, llvm_ty(to)));
                self.push(r);
            }
            F32ConvertI32U | F32ConvertI64U | F64ConvertI32U | F64ConvertI64U => {
                let a = self.pop();
                let r = self.val(to, format!("uitofp {} {} to {}", llvm_ty(from), a.s, llvm_ty(to)));
                self.push(r);
            }
            F32DemoteF64 => {
                let a = self.pop();
                let r = self.val(to, format!("fptrunc double {} to float", a.s));
                self.push(r);
            }
            F64PromoteF32 => {
                let a = self.pop();
                let r = self.val(to, format!("fpext float {} to double", a.s));
                self.push(r);
            }
            I32TruncSatF32S | I32TruncSatF64S | I64TruncSatF32S | I64TruncSatF64S => {
                let a = self.pop();
                let name = format!("llvm.fptosi.sat.{}.{}", llvm_ty(to), llvm_ty(from));
                let decl = format!(
                    "declare {} @{}({})",
                    llvm_ty(to),
                    name,
                    llvm_ty(from)
                );
                let sym = self.reg_decl(&name, &decl);
                let r = self.val(
                    to,
                    format!("call {} {}({} {})", llvm_ty(to), sym, llvm_ty(from), a.s),
                );
                self.push(r);
            }
            I32TruncSatF32U | I32TruncSatF64U | I64TruncSatF32U | I64TruncSatF64U => {
                let a = self.pop();
                let name = format!("llvm.fptoui.sat.{}.{}", llvm_ty(to), llvm_ty(from));
                let decl = format!(
                    "declare {} @{}({})",
                    llvm_ty(to),
                    name,
                    llvm_ty(from)
                );
                let sym = self.reg_decl(&name, &decl);
                let r = self.val(
                    to,
                    format!("call {} {}({} {})", llvm_ty(to), sym, llvm_ty(from), a.s),
                );
                self.push(r);
            }
        }
    }

    fn gen_trunc(&mut self, from: ValType, to: ValType, signed: bool) {
        let is_f32 = from == ValType::F32;
        let (lo, hi, lo_strict, hi_strict) = Self::trunc_bounds(from, to, signed, is_f32);
        let ft = llvm_ty(from);
        let a = self.pop();
        let isnan = self.new_reg(format!("fcmp uno {} {}, {}", ft, a.s, a.s));
        self.branch_trap(&isnan, 3);
        let gecmp = if lo_strict { "ogt" } else { "oge" };
        let loc = self.new_reg(format!("fcmp {} {} {}, {}", gecmp, ft, a.s, lo));
        let hicmp = if hi_strict { "olt" } else { "ole" };
        let hic = self.new_reg(format!("fcmp {} {} {}, {}", hicmp, ft, a.s, hi));
        let ok = self.new_reg(format!("and i1 {}, {}", loc, hic));
        let bad = self.new_reg(format!("xor i1 {}, true", ok));
        self.branch_trap(&bad, 2);
        let inst = if signed { "fptosi" } else { "fptoui" };
        let r = self.val(to, format!("{} {} {} to {}", inst, ft, a.s, llvm_ty(to)));
        self.push(r);
    }

    // -----------------------------------------------------------------
    // SIMD
    // -----------------------------------------------------------------

    fn simd_binop(&mut self, lty: &str, op: &str) {
        let b = self.pop();
        let a = self.pop();
        let a = self.bc(&a, lty);
        let b = self.bc(&b, lty);
        let r = self.val_lty(
            ValType::V128,
            lty,
            format!("{} {} {}, {}", op, lty, a.s, b.s),
        );
        let r = self.bc(&r, "<16 x i8>");
        self.push(r);
    }

    fn simd_intrinsic_bin(&mut self, lty: &str, iname: &str) {
        let decl = format!("declare {} @{}({}, {})", lty, iname, lty, lty);
        let b = self.pop();
        let a = self.pop();
        let a = self.bc(&a, lty);
        let b = self.bc(&b, lty);
        let sym = self.reg_decl(iname, &decl);
        let r = self.val_lty(
            ValType::V128,
            lty,
            format!("call {} {}({} {}, {} {})", lty, sym, lty, a.s, lty, b.s),
        );
        let r = self.bc(&r, "<16 x i8>");
        self.push(r);
    }

    fn simd_intrinsic_un(&mut self, lty: &str, iname: &str) {
        let decl = format!("declare {} @{}({})", lty, iname, lty);
        let a = self.pop();
        let a = self.bc(&a, lty);
        let sym = self.reg_decl(iname, &decl);
        let r = self.val_lty(
            ValType::V128,
            lty,
            format!("call {} {}({} {})", lty, sym, lty, a.s),
        );
        let r = self.bc(&r, "<16 x i8>");
        self.push(r);
    }

    fn simd_cmp(&mut self, lty: &str, elem: &str, pred: &str, float: bool) {
        let b = self.pop();
        let a = self.pop();
        let a = self.bc(&a, lty);
        let b = self.bc(&b, lty);
        let pred = if float {
            match pred {
                "eq" => "oeq",
                "ne" => "une",
                "lt" => "olt",
                "gt" => "ogt",
                "le" => "ole",
                "ge" => "oge",
                other => other,
            }
        } else {
            pred
        };
        let c = self.new_reg(format!(
            "{} {} {} {}, {}",
            if float { "fcmp" } else { "icmp" },
            pred,
            lty,
            a.s,
            b.s
        ));
        let _ = elem;
        let res_lty = if float {
            if lty.contains("float") {
                format!("<{} x i32>", lanes_of(lty))
            } else {
                format!("<{} x i64>", lanes_of(lty))
            }
        } else {
            lty.to_string()
        };
        let r = self.val_lty(
            ValType::V128,
            &res_lty,
            format!(
                "sext <{} x i1> {} to {}",
                lanes_of(lty),
                c,
                res_lty
            ),
        );
        let r = self.bc(&r, "<16 x i8>");
        self.push(r);
    }

    fn splat_scalar(&mut self, scalar_s: &str, scalar_lty: &str, vec_lty: &str, elem: &str, lanes: u32) -> Val {
        let x = if scalar_lty == elem {
            scalar_s.to_string()
        } else if bits_of_ty(elem) < bits_of_ty(scalar_lty) {
            self.new_reg(format!("trunc {} {} to {}", scalar_lty, scalar_s, elem))
        } else {
            self.new_reg(format!("zext {} {} to {}", scalar_lty, scalar_s, elem))
        };
        let ins = self.new_reg(format!(
            "insertelement {} zeroinitializer, {} {}, i32 0",
            vec_lty, elem, x
        ));
        let s = self.new_reg(format!(
            "shufflevector {} {}, {} zeroinitializer, <{} x i32> zeroinitializer",
            vec_lty, ins, vec_lty, lanes
        ));
        Val {
            ty: ValType::V128,
            lty: vec_lty.to_string(),
            s,
        }
    }

    fn pop_v(&mut self) -> Val {
        let v = self.pop();
        self.bc(&v, "<16 x i8>")
    }

    fn push_v(&mut self, v: Val) {
        let v = self.bc(&v, "<16 x i8>");
        self.push(v);
    }

    fn bool_to_i32(&mut self, cond_s: &str) {
        let r = self.val(ValType::I32, format!("zext i1 {} to i32", cond_s));
        self.push(r);
    }

    pub fn gen_simd(&mut self, instr: &Instr) {
        match instr {
            Instr::V128Load(ma) => self.simd_load(ma),
            Instr::V128Store(ma) => self.simd_store(ma),
            Instr::V128LoadExt(op, ma) => self.simd_load_ext(*op, ma),
            Instr::V128LoadLane(op, ma, lane) => self.simd_lane_mem(*op, ma, *lane),
            Instr::I8x16Shuffle(mask) => {
                let b = self.pop_v();
                let a = self.pop_v();
                let m: Vec<String> = mask.iter().map(|x| format!("i32 {}", x)).collect();
                let r = self.val_lty(
                    ValType::V128,
                    "<16 x i8>",
                    format!(
                        "shufflevector <16 x i8> {}, <16 x i8> {}, <16 x i32> <{}>",
                        a.s,
                        b.s,
                        m.join(", ")
                    ),
                );
                self.push(r);
            }
            Instr::SimdLane(op, lane) => self.simd_lane(*op, *lane),
            Instr::Simd(op) => self.simd_op(*op),
            _ => unreachable!(),
        }
    }

    fn simd_load(&mut self, ma: &MemArg) {
        let addr = self.pop();
        let p = self.mem_ptr(&addr, ma.offset, 16);
        let r = self.val_lty(
            ValType::V128,
            "<16 x i8>",
            format!("load <16 x i8>, ptr {}, align 1", p.s),
        );
        self.push(r);
    }

    fn simd_store(&mut self, ma: &MemArg) {
        let v = self.pop_v();
        let addr = self.pop();
        let p = self.mem_ptr(&addr, ma.offset, 16);
        self.emit(format!("store <16 x i8> {}, ptr {}, align 1", v.s, p.s));
    }

    fn simd_load_ext(&mut self, op: SimdLoadOp, ma: &MemArg) {
        let addr = self.pop();
        let p = self.mem_ptr(&addr, ma.offset, op.bytes());
        let r = match op {
            SimdLoadOp::Load8x8S | SimdLoadOp::Load8x8U => {
                let v = self.new_reg(format!("load <8 x i8>, ptr {}, align 1", p.s));
                let opc = if op == SimdLoadOp::Load8x8S { "sext" } else { "zext" };
                self.val_lty(
                    ValType::V128,
                    "<8 x i16>",
                    format!("{} <8 x i8> {} to <8 x i16>", opc, v),
                )
            }
            SimdLoadOp::Load16x4S | SimdLoadOp::Load16x4U => {
                let v = self.new_reg(format!("load <4 x i16>, ptr {}, align 1", p.s));
                let opc = if op == SimdLoadOp::Load16x4S { "sext" } else { "zext" };
                self.val_lty(
                    ValType::V128,
                    "<4 x i32>",
                    format!("{} <4 x i16> {} to <4 x i32>", opc, v),
                )
            }
            SimdLoadOp::Load32x2S | SimdLoadOp::Load32x2U => {
                let v = self.new_reg(format!("load <2 x i32>, ptr {}, align 1", p.s));
                let opc = if op == SimdLoadOp::Load32x2S { "sext" } else { "zext" };
                self.val_lty(
                    ValType::V128,
                    "<2 x i64>",
                    format!("{} <2 x i32> {} to <2 x i64>", opc, v),
                )
            }
            SimdLoadOp::Load8Splat => {
                let v = self.new_reg(format!("load i8, ptr {}, align 1", p.s));
                self.splat_scalar(&v, "i8", "<16 x i8>", "i8", 16)
            }
            SimdLoadOp::Load16Splat => {
                let v = self.new_reg(format!("load i16, ptr {}, align 1", p.s));
                self.splat_scalar(&v, "i16", "<8 x i16>", "i16", 8)
            }
            SimdLoadOp::Load32Splat => {
                let v = self.new_reg(format!("load i32, ptr {}, align 1", p.s));
                self.splat_scalar(&v, "i32", "<4 x i32>", "i32", 4)
            }
            SimdLoadOp::Load64Splat => {
                let v = self.new_reg(format!("load i64, ptr {}, align 1", p.s));
                self.splat_scalar(&v, "i64", "<2 x i64>", "i64", 2)
            }
            SimdLoadOp::Load32Zero => {
                let v = self.new_reg(format!("load i32, ptr {}, align 1", p.s));
                self.val_lty(
                    ValType::V128,
                    "<4 x i32>",
                    format!("insertelement <4 x i32> zeroinitializer, i32 {}, i32 0", v),
                )
            }
            SimdLoadOp::Load64Zero => {
                let v = self.new_reg(format!("load i64, ptr {}, align 1", p.s));
                self.val_lty(
                    ValType::V128,
                    "<2 x i64>",
                    format!("insertelement <2 x i64> zeroinitializer, i64 {}, i32 0", v),
                )
            }
        };
        self.push_v(r);
    }

    fn simd_lane_mem(&mut self, op: SimdMemLaneOp, ma: &MemArg, lane: u8) {
        let is_store = op.is_store();
        if is_store {
            let v = self.pop_v();
            let addr = self.pop();
            let p = self.mem_ptr(&addr, ma.offset, op.bytes());
            let (lty, elem) = match op {
                SimdMemLaneOp::Store8 => ("<16 x i8>", "i8"),
                SimdMemLaneOp::Store16 => ("<8 x i16>", "i16"),
                SimdMemLaneOp::Store32 => ("<4 x i32>", "i32"),
                _ => ("<2 x i64>", "i64"),
            };
            let vv = self.bc(&v, lty);
            let x = self.new_reg(format!(
                "extractelement {} {}, i32 {}",
                lty, vv.s, lane
            ));
            self.emit(format!("store {} {}, ptr {}, align 1", elem, x, p.s));
        } else {
            let v = self.pop_v();
            let addr = self.pop();
            let p = self.mem_ptr(&addr, ma.offset, op.bytes());
            let (lty, elem) = match op {
                SimdMemLaneOp::Load8 => ("<16 x i8>", "i8"),
                SimdMemLaneOp::Load16 => ("<8 x i16>", "i16"),
                SimdMemLaneOp::Load32 => ("<4 x i32>", "i32"),
                _ => ("<2 x i64>", "i64"),
            };
            let x = self.new_reg(format!("load {}, ptr {}, align 1", elem, p.s));
            let vv = self.bc(&v, lty);
            let r = self.val_lty(
                ValType::V128,
                lty,
                format!(
                    "insertelement {} {}, {} {}, i32 {}",
                    lty, vv.s, elem, x, lane
                ),
            );
            self.push_v(r);
        }
    }

    fn simd_lane(&mut self, op: SimdLaneOp, lane: u8) {
        use SimdLaneOp::*;
        let (lty, elem, scalar_ty, is_float) = match op {
            I8x16ExtractS | I8x16ExtractU | I8x16Replace => ("<16 x i8>", "i8", "i32", false),
            I16x8ExtractS | I16x8ExtractU | I16x8Replace => ("<8 x i16>", "i16", "i32", false),
            I32x4Extract | I32x4Replace => ("<4 x i32>", "i32", "i32", false),
            I64x2Extract | I64x2Replace => ("<2 x i64>", "i64", "i64", false),
            F32x4Extract | F32x4Replace => ("<4 x float>", "float", "float", true),
            F64x2Extract | F64x2Replace => ("<2 x double>", "double", "double", true),
        };
        let _ = is_float;
        let is_extract = matches!(
            op,
            I8x16ExtractS | I8x16ExtractU | I16x8ExtractS | I16x8ExtractU | I32x4Extract
                | I64x2Extract | F32x4Extract | F64x2Extract
        );
        if is_extract {
            let v = self.pop_v();
            let vv = self.bc(&v, lty);
            let x = self.new_reg(format!(
                "extractelement {} {}, i32 {}",
                lty, vv.s, lane
            ));
            match scalar_ty {
                "i32" => {
                    if lty == "<16 x i8>" || lty == "<8 x i16>" {
                        let signed = matches!(op, I8x16ExtractS | I16x8ExtractS);
                        let opc = if signed { "sext" } else { "zext" };
                        let r = self.val(
                            ValType::I32,
                            format!("{} {} {} to i32", opc, elem, x),
                        );
                        self.push(r);
                    } else {
                        let r = Val {
                            ty: ValType::I32,
                            lty: "i32".into(),
                            s: x,
                        };
                        self.push(r);
                    }
                }
                "i64" => {
                    self.push(Val {
                        ty: ValType::I64,
                        lty: "i64".into(),
                        s: x,
                    });
                }
                _ => {
                    let ty = if scalar_ty == "float" {
                        ValType::F32
                    } else {
                        ValType::F64
                    };
                    self.push(Val {
                        ty,
                        lty: scalar_ty.to_string(),
                        s: x,
                    });
                }
            }
        } else {
            let sv = self.pop();
            let v = self.pop_v();
            let vv = self.bc(&v, lty);
            let x = if sv.lty == elem {
                sv.s.clone()
            } else if elem == "i8" || elem == "i16" {
                self.new_reg(format!("trunc {} {} to {}", sv.lty, sv.s, elem))
            } else {
                sv.s.clone()
            };
            let r = self.val_lty(
                ValType::V128,
                lty,
                format!(
                    "insertelement {} {}, {} {}, i32 {}",
                    lty, vv.s, elem, x, lane
                ),
            );
            self.push_v(r);
        }
    }

    fn alltrue(&mut self, lty: &str, bits: u32) {
        let a = self.pop_v();
        let a = self.bc(&a, lty);
        let m = self.new_reg(format!(
            "icmp ne {} {}, zeroinitializer",
            lty, a.s
        ));
        let b = self.new_reg(format!(
            "bitcast <{} x i1> {} to i{}",
            lanes_of(lty),
            m,
            bits
        ));
        let all = self.new_reg(format!("icmp eq i{} {}, -1", bits, b));
        self.bool_to_i32(&all);
    }

    fn bitmask(&mut self, lty: &str, bits: u32) {
        let a = self.pop_v();
        let a = self.bc(&a, lty);
        let m = self.new_reg(format!(
            "icmp slt {} {}, zeroinitializer",
            lty, a.s
        ));
        let b = self.new_reg(format!(
            "bitcast <{} x i1> {} to i{}",
            lanes_of(lty),
            m,
            bits
        ));
        let r = self.val(ValType::I32, format!("zext i{} {} to i32", bits, b));
        self.push(r);
    }

    fn vshift(&mut self, lty: &str, elem: &str, lanes: u32, bits: u32, op: &str) {
        let c = self.pop();
        let a = self.pop_v();
        let a = self.bc(&a, lty);
        let masked = self.new_reg(format!("and i32 {}, {}", c.s, bits - 1));
        let cv = self.splat_scalar(&masked, "i32", lty, elem, lanes);
        let r = self.val_lty(
            ValType::V128,
            lty,
            format!("{} {} {}, {}", op, lty, a.s, cv.s),
        );
        self.push_v(r);
    }

    fn narrow(
        &mut self,
        in_lty: &str,
        out_lty: &str,
        in_elem: &str,
        out_elem: &str,
        in_lanes: u32,
        out_lanes: u32,
        signed: bool,
        dst_bits: u32,
    ) {
        let b = self.pop_v();
        let a = self.pop_v();
        let a = self.bc(&a, in_lty);
        let b = self.bc(&b, in_lty);
        let (lo, hi) = if signed {
            (
                -(1i64 << (dst_bits - 1)),
                (1i64 << (dst_bits - 1)) - 1,
            )
        } else {
            (0, (1i64 << dst_bits) - 1)
        };
        // Clamp to [lo, hi] using signed min/max regardless of signedness:
        // for unsigned narrowing, the input is signed and clamped to [0, 2^n-1].
        let lo_c = format!("splat ({} {})", in_elem, lo);
        let hi_c = format!("splat ({} {})", in_elem, hi);
        let decl_min = format!("declare {} @llvm.smax.{}({}, {})", in_lty, vec_name(in_lty), in_lty, in_lty);
        let name_min = format!("llvm.smax.{}", vec_name(in_lty));
        let sym_min = self.reg_decl(&name_min, &decl_min);
        let decl_max = format!("declare {} @llvm.smin.{}({}, {})", in_lty, vec_name(in_lty), in_lty, in_lty);
        let name_max = format!("llvm.smin.{}", vec_name(in_lty));
        let sym_max = self.reg_decl(&name_max, &decl_max);
        let a1 = self.new_reg(format!("call {} {}({} {}, {} {})", in_lty, sym_min, in_lty, a.s, in_lty, lo_c));
        let a2 = self.new_reg(format!("call {} {}({} {}, {} {})", in_lty, sym_max, in_lty, a1, in_lty, hi_c));
        let b1 = self.new_reg(format!("call {} {}({} {}, {} {})", in_lty, sym_min, in_lty, b.s, in_lty, lo_c));
        let b2 = self.new_reg(format!("call {} {}({} {}, {} {})", in_lty, sym_max, in_lty, b1, in_lty, hi_c));
        let half_lty = format!("<{} x {}>", in_lanes, out_elem);
        let at = self.new_reg(format!("trunc {} {} to {}", in_lty, a2, half_lty));
        let bt = self.new_reg(format!("trunc {} {} to {}", in_lty, b2, half_lty));
        let mut mask = Vec::new();
        for i in 0..in_lanes {
            mask.push(format!("i32 {}", i));
        }
        for i in 0..in_lanes {
            mask.push(format!("i32 {}", in_lanes + i));
        }
        let r = self.val_lty(
            ValType::V128,
            out_lty,
            format!(
                "shufflevector {} {}, {} {}, <{} x i32> <{}>",
                half_lty, at, half_lty, bt, out_lanes, mask.join(", ")
            ),
        );
        let _ = out_elem;
        self.push_v(r);
    }

    fn extadd_pairwise(&mut self, in_lty: &str, wide_lty: &str, signed: bool, lanes: u32) {
        let a = self.pop_v();
        let a = self.bc(&a, in_lty);
        let opc = if signed { "sext" } else { "zext" };
        let w = self.new_reg(format!("{} {} {} to {}", opc, in_lty, a.s, wide_lty));
        let mut even = Vec::new();
        let mut odd = Vec::new();
        for i in 0..lanes / 2 {
            even.push(format!("i32 {}", i * 2));
            odd.push(format!("i32 {}", i * 2 + 1));
        }
        let half_lty = format!("<{} x {}>", lanes / 2, elem_of(wide_lty));
        let e = self.new_reg(format!(
            "shufflevector {} {}, {} poison, <{} x i32> <{}>",
            wide_lty, w, wide_lty, lanes / 2, even.join(", ")
        ));
        let o = self.new_reg(format!(
            "shufflevector {} {}, {} poison, <{} x i32> <{}>",
            wide_lty, w, wide_lty, lanes / 2, odd.join(", ")
        ));
        let r = self.val_lty(ValType::V128, &half_lty, format!("add {} {}, {}", half_lty, e, o));
        self.push_v(r);
    }

    fn extmul(&mut self, in_lty: &str, wide_lty: &str, signed: bool, high: bool) {
        let b = self.pop_v();
        let a = self.pop_v();
        let a = self.bc(&a, in_lty);
        let b = self.bc(&b, in_lty);
        let in_lanes = lanes_of(in_lty);
        let off = if high { in_lanes / 2 } else { 0 };
        let mut mask = Vec::new();
        for i in 0..in_lanes / 2 {
            mask.push(format!("i32 {}", off + i));
        }
        let half_lty = format!("<{} x {}>", in_lanes / 2, elem_of(in_lty));
        let ah = self.new_reg(format!(
            "shufflevector {} {}, {} poison, <{} x i32> <{}>",
            in_lty, a.s, in_lty, in_lanes / 2, mask.join(", ")
        ));
        let bh = self.new_reg(format!(
            "shufflevector {} {}, {} poison, <{} x i32> <{}>",
            in_lty, b.s, in_lty, in_lanes / 2, mask.join(", ")
        ));
        let _ = half_lty;
        let opc = if signed { "sext" } else { "zext" };
        let aw = self.new_reg(format!("{} {} {} to {}", opc, half_lty, ah, wide_lty));
        let bw = self.new_reg(format!("{} {} {} to {}", opc, half_lty, bh, wide_lty));
        let r = self.val_lty(ValType::V128, wide_lty, format!("mul {} {}, {}", wide_lty, aw, bw));
        self.push_v(r);
    }

    fn extend_half(&mut self, in_lty: &str, wide_lty: &str, signed: bool, high: bool) {
        let a = self.pop_v();
        let a = self.bc(&a, in_lty);
        let in_lanes = lanes_of(in_lty);
        let off = if high { in_lanes / 2 } else { 0 };
        let mut mask = Vec::new();
        for i in 0..in_lanes / 2 {
            mask.push(format!("i32 {}", off + i));
        }
        let half_lty = format!("<{} x {}>", in_lanes / 2, elem_of(in_lty));
        let h = self.new_reg(format!(
            "shufflevector {} {}, {} poison, <{} x i32> <{}>",
            in_lty, a.s, in_lty, in_lanes / 2, mask.join(", ")
        ));
        let opc = if signed { "sext" } else { "zext" };
        let r = self.val_lty(
            ValType::V128,
            wide_lty,
            format!("{} {} {} to {}", opc, half_lty, h, wide_lty),
        );
        self.push_v(r);
    }

    fn bin_int_vv(&mut self, lty: &str, name: &str) {
        match name {
            "add" | "sub" | "mul" | "and" | "or" | "xor" => self.simd_binop(lty, name),
            _ => unreachable!(),
        }
    }

    fn simd_op(&mut self, op: SimdOp) {
        use SimdOp::*;
        match op {
            // bitwise
            Not => {
                let a = self.pop_v();
                let r = self.val_lty(
                    ValType::V128,
                    "<16 x i8>",
                    format!("xor <16 x i8> {}, <i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1>", a.s),
                );
                self.push(r);
            }
            And => self.simd_binop("<16 x i8>", "and"),
            Or => self.simd_binop("<16 x i8>", "or"),
            Xor => self.simd_binop("<16 x i8>", "xor"),
            AndNot => {
                let second = self.pop_v();
                let first = self.pop_v();
                let nb = self.new_reg(format!(
                    "xor <16 x i8> {}, <i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1>",
                    second.s
                ));
                let r = self.val_lty(
                    ValType::V128,
                    "<16 x i8>",
                    format!("and <16 x i8> {}, {}", first.s, nb),
                );
                self.push(r);
            }
            Bitselect => {
                let c = self.pop_v();
                let b = self.pop_v();
                let a = self.pop_v();
                let t1 = self.new_reg(format!("and <16 x i8> {}, {}", a.s, c.s));
                let nc = self.new_reg(format!("xor <16 x i8> {}, <i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1, i8 -1>", c.s));
                let t2 = self.new_reg(format!("and <16 x i8> {}, {}", b.s, nc));
                let r = self.val_lty(
                    ValType::V128,
                    "<16 x i8>",
                    format!("or <16 x i8> {}, {}", t1, t2),
                );
                self.push(r);
            }
            AnyTrue => {
                let a = self.pop_v();
                let i = self.new_reg(format!("bitcast <16 x i8> {} to i128", a.s));
                let c = self.new_reg(format!("icmp ne i128 {}, 0", i));
                self.bool_to_i32(&c);
            }
            // splats
            I8x16Splat => {
                let a = self.pop();
                let v = self.splat_scalar(&a.s, &a.lty, "<16 x i8>", "i8", 16);
                self.push_v(v);
            }
            I16x8Splat => {
                let a = self.pop();
                let v = self.splat_scalar(&a.s, &a.lty, "<8 x i16>", "i16", 8);
                self.push_v(v);
            }
            I32x4Splat => {
                let a = self.pop();
                let v = self.splat_scalar(&a.s, &a.lty, "<4 x i32>", "i32", 4);
                self.push_v(v);
            }
            I64x2Splat => {
                let a = self.pop();
                let v = self.splat_scalar(&a.s, &a.lty, "<2 x i64>", "i64", 2);
                self.push_v(v);
            }
            F32x4Splat => {
                let a = self.pop();
                let v = self.splat_scalar(&a.s, &a.lty, "<4 x float>", "float", 4);
                self.push_v(v);
            }
            F64x2Splat => {
                let a = self.pop();
                let v = self.splat_scalar(&a.s, &a.lty, "<2 x double>", "double", 2);
                self.push_v(v);
            }
            I8x16Swizzle => {
                self.needs_ssse3 = true;
                let idx = self.pop_v();
                let a = self.pop_v();
                let big = self.new_reg(format!(
                    "icmp ugt <16 x i8> {}, <i8 15, i8 15, i8 15, i8 15, i8 15, i8 15, i8 15, i8 15, i8 15, i8 15, i8 15, i8 15, i8 15, i8 15, i8 15, i8 15>",
                    idx.s
                ));
                let m = self.new_reg(format!(
                    "select <16 x i1> {}, <16 x i8> <i8 -128, i8 -128, i8 -128, i8 -128, i8 -128, i8 -128, i8 -128, i8 -128, i8 -128, i8 -128, i8 -128, i8 -128, i8 -128, i8 -128, i8 -128, i8 -128>, <16 x i8> {}",
                    big, idx.s
                ));
                let sym = self.reg_decl(
                    "llvm.x86.ssse3.pshuf.b.128",
                    "declare <16 x i8> @llvm.x86.ssse3.pshuf.b.128(<16 x i8>, <16 x i8>)",
                );
                let r = self.val_lty(
                    ValType::V128,
                    "<16 x i8>",
                    format!(
                        "call <16 x i8> {}(<16 x i8> {}, <16 x i8> {})",
                        sym, a.s, m
                    ),
                );
                self.push(r);
            }
            // comparisons
            I8x16Eq | I8x16Ne | I8x16LtS | I8x16LtU | I8x16GtS | I8x16GtU | I8x16LeS
            | I8x16LeU | I8x16GeS | I8x16GeU => {
                self.simd_cmp("<16 x i8>", "i8", cmp_pred(op), false);
            }
            I16x8Eq | I16x8Ne | I16x8LtS | I16x8LtU | I16x8GtS | I16x8GtU | I16x8LeS
            | I16x8LeU | I16x8GeS | I16x8GeU => {
                self.simd_cmp("<8 x i16>", "i16", cmp_pred(op), false);
            }
            I32x4Eq | I32x4Ne | I32x4LtS | I32x4LtU | I32x4GtS | I32x4GtU | I32x4LeS
            | I32x4LeU | I32x4GeS | I32x4GeU => {
                self.simd_cmp("<4 x i32>", "i32", cmp_pred(op), false);
            }
            I64x2Eq | I64x2Ne | I64x2LtS | I64x2GtS | I64x2LeS | I64x2GeS => {
                self.simd_cmp("<2 x i64>", "i64", cmp_pred(op), false);
            }
            F32x4Eq | F32x4Ne | F32x4Lt | F32x4Gt | F32x4Le | F32x4Ge => {
                self.simd_cmp("<4 x float>", "float", cmp_pred(op), true);
            }
            F64x2Eq | F64x2Ne | F64x2Lt | F64x2Gt | F64x2Le | F64x2Ge => {
                self.simd_cmp("<2 x double>", "double", cmp_pred(op), true);
            }
            // i8x16
            I8x16Abs => self.simd_abs("<16 x i8>"),
            I8x16Neg => {
                let a = self.pop_v();
                let r = self.val_lty(
                    ValType::V128,
                    "<16 x i8>",
                    format!("sub <16 x i8> zeroinitializer, {}", a.s),
                );
                self.push(r);
            }
            I8x16Popcnt => self.simd_intrinsic_un("<16 x i8>", "llvm.ctpop.v16i8"),
            I8x16AllTrue => self.alltrue("<16 x i8>", 16),
            I8x16Bitmask => self.bitmask("<16 x i8>", 16),
            I8x16NarrowI16x8S => {
                self.narrow("<8 x i16>", "<16 x i8>", "i16", "i8", 8, 16, true, 8)
            }
            I8x16NarrowI16x8U => {
                self.narrow("<8 x i16>", "<16 x i8>", "i16", "i8", 8, 16, false, 8)
            }
            I8x16Shl => self.vshift("<16 x i8>", "i8", 16, 8, "shl"),
            I8x16ShrS => self.vshift("<16 x i8>", "i8", 16, 8, "ashr"),
            I8x16ShrU => self.vshift("<16 x i8>", "i8", 16, 8, "lshr"),
            I8x16Add => self.bin_int_vv("<16 x i8>", "add"),
            I8x16Sub => self.bin_int_vv("<16 x i8>", "sub"),
            I8x16AddSatS => self.simd_intrinsic_bin("<16 x i8>", "llvm.sadd.sat.v16i8"),
            I8x16AddSatU => self.simd_intrinsic_bin("<16 x i8>", "llvm.uadd.sat.v16i8"),
            I8x16SubSatS => self.simd_intrinsic_bin("<16 x i8>", "llvm.ssub.sat.v16i8"),
            I8x16SubSatU => self.simd_intrinsic_bin("<16 x i8>", "llvm.usub.sat.v16i8"),
            I8x16MinS => self.simd_intrinsic_bin("<16 x i8>", "llvm.smin.v16i8"),
            I8x16MinU => self.simd_intrinsic_bin("<16 x i8>", "llvm.umin.v16i8"),
            I8x16MaxS => self.simd_intrinsic_bin("<16 x i8>", "llvm.smax.v16i8"),
            I8x16MaxU => self.simd_intrinsic_bin("<16 x i8>", "llvm.umax.v16i8"),
            I8x16AvgrU => self.avgr("<16 x i8>", "<16 x i16>", "i8", "i16", 16),
            // i16x8
            I16x8ExtaddPairwiseI8x16S => {
                self.extadd_pairwise("<16 x i8>", "<16 x i16>", true, 16)
            }
            I16x8ExtaddPairwiseI8x16U => {
                self.extadd_pairwise("<16 x i8>", "<16 x i16>", false, 16)
            }
            I32x4ExtaddPairwiseI16x8S => {
                self.extadd_pairwise("<8 x i16>", "<8 x i32>", true, 8)
            }
            I32x4ExtaddPairwiseI16x8U => {
                self.extadd_pairwise("<8 x i16>", "<8 x i32>", false, 8)
            }
            I16x8Abs => self.simd_abs("<8 x i16>"),
            I16x8Neg => {
                let a = self.pop_v();
                let av = self.bc(&a, "<8 x i16>");
                let r = self.val_lty(
                    ValType::V128,
                    "<8 x i16>",
                    format!("sub <8 x i16> zeroinitializer, {}", av.s),
                );
                self.push_v(r);
            }
            I16x8Q15mulrSatS => self.q15mulr(),
            I16x8AllTrue => self.alltrue("<8 x i16>", 8),
            I16x8Bitmask => self.bitmask("<8 x i16>", 8),
            I16x8NarrowI32x4S => {
                self.narrow("<4 x i32>", "<8 x i16>", "i32", "i16", 4, 8, true, 16)
            }
            I16x8NarrowI32x4U => {
                self.narrow("<4 x i32>", "<8 x i16>", "i32", "i16", 4, 8, false, 16)
            }
            I16x8ExtendLowI8x16S => self.extend_half("<16 x i8>", "<8 x i16>", true, false),
            I16x8ExtendHighI8x16S => self.extend_half("<16 x i8>", "<8 x i16>", true, true),
            I16x8ExtendLowI8x16U => self.extend_half("<16 x i8>", "<8 x i16>", false, false),
            I16x8ExtendHighI8x16U => self.extend_half("<16 x i8>", "<8 x i16>", false, true),
            I16x8Shl => self.vshift("<8 x i16>", "i16", 8, 16, "shl"),
            I16x8ShrS => self.vshift("<8 x i16>", "i16", 8, 16, "ashr"),
            I16x8ShrU => self.vshift("<8 x i16>", "i16", 8, 16, "lshr"),
            I16x8Add => self.bin_int_vv("<8 x i16>", "add"),
            I16x8Sub => self.bin_int_vv("<8 x i16>", "sub"),
            I16x8Mul => self.bin_int_vv("<8 x i16>", "mul"),
            I16x8AddSatS => self.simd_intrinsic_bin("<8 x i16>", "llvm.sadd.sat.v8i16"),
            I16x8AddSatU => self.simd_intrinsic_bin("<8 x i16>", "llvm.uadd.sat.v8i16"),
            I16x8SubSatS => self.simd_intrinsic_bin("<8 x i16>", "llvm.ssub.sat.v8i16"),
            I16x8SubSatU => self.simd_intrinsic_bin("<8 x i16>", "llvm.usub.sat.v8i16"),
            I16x8MinS => self.simd_intrinsic_bin("<8 x i16>", "llvm.smin.v8i16"),
            I16x8MinU => self.simd_intrinsic_bin("<8 x i16>", "llvm.umin.v8i16"),
            I16x8MaxS => self.simd_intrinsic_bin("<8 x i16>", "llvm.smax.v8i16"),
            I16x8MaxU => self.simd_intrinsic_bin("<8 x i16>", "llvm.umax.v8i16"),
            I16x8AvgrU => self.avgr("<8 x i16>", "<8 x i32>", "i16", "i32", 8),
            I16x8ExtmulLowI8x16S => self.extmul("<16 x i8>", "<8 x i16>", true, false),
            I16x8ExtmulHighI8x16S => self.extmul("<16 x i8>", "<8 x i16>", true, true),
            I16x8ExtmulLowI8x16U => self.extmul("<16 x i8>", "<8 x i16>", false, false),
            I16x8ExtmulHighI8x16U => self.extmul("<16 x i8>", "<8 x i16>", false, true),
            // i32x4
            I32x4Abs => self.simd_abs("<4 x i32>"),
            I32x4Neg => {
                let a = self.pop_v();
                let av = self.bc(&a, "<4 x i32>");
                let r = self.val_lty(
                    ValType::V128,
                    "<4 x i32>",
                    format!("sub <4 x i32> zeroinitializer, {}", av.s),
                );
                self.push_v(r);
            }
            I32x4AllTrue => self.alltrue("<4 x i32>", 4),
            I32x4Bitmask => self.bitmask("<4 x i32>", 4),
            I32x4ExtendLowI16x8S => self.extend_half("<8 x i16>", "<4 x i32>", true, false),
            I32x4ExtendHighI16x8S => self.extend_half("<8 x i16>", "<4 x i32>", true, true),
            I32x4ExtendLowI16x8U => self.extend_half("<8 x i16>", "<4 x i32>", false, false),
            I32x4ExtendHighI16x8U => self.extend_half("<8 x i16>", "<4 x i32>", false, true),
            I32x4Shl => self.vshift("<4 x i32>", "i32", 4, 32, "shl"),
            I32x4ShrS => self.vshift("<4 x i32>", "i32", 4, 32, "ashr"),
            I32x4ShrU => self.vshift("<4 x i32>", "i32", 4, 32, "lshr"),
            I32x4Add => self.bin_int_vv("<4 x i32>", "add"),
            I32x4Sub => self.bin_int_vv("<4 x i32>", "sub"),
            I32x4Mul => self.bin_int_vv("<4 x i32>", "mul"),
            I32x4MinS => self.simd_intrinsic_bin("<4 x i32>", "llvm.smin.v4i32"),
            I32x4MinU => self.simd_intrinsic_bin("<4 x i32>", "llvm.umin.v4i32"),
            I32x4MaxS => self.simd_intrinsic_bin("<4 x i32>", "llvm.smax.v4i32"),
            I32x4MaxU => self.simd_intrinsic_bin("<4 x i32>", "llvm.umax.v4i32"),
            I32x4DotI16x8S => self.dot_i16x8s(),
            I32x4ExtmulLowI16x8S => self.extmul("<8 x i16>", "<4 x i32>", true, false),
            I32x4ExtmulHighI16x8S => self.extmul("<8 x i16>", "<4 x i32>", true, true),
            I32x4ExtmulLowI16x8U => self.extmul("<8 x i16>", "<4 x i32>", false, false),
            I32x4ExtmulHighI16x8U => self.extmul("<8 x i16>", "<4 x i32>", false, true),
            // i64x2
            I64x2Abs => self.simd_abs("<2 x i64>"),
            I64x2Neg => {
                let a = self.pop_v();
                let av = self.bc(&a, "<2 x i64>");
                let r = self.val_lty(
                    ValType::V128,
                    "<2 x i64>",
                    format!("sub <2 x i64> zeroinitializer, {}", av.s),
                );
                self.push_v(r);
            }
            I64x2AllTrue => self.alltrue("<2 x i64>", 2),
            I64x2Bitmask => self.bitmask("<2 x i64>", 2),
            I64x2ExtendLowI32x4S => self.extend_half("<4 x i32>", "<2 x i64>", true, false),
            I64x2ExtendHighI32x4S => self.extend_half("<4 x i32>", "<2 x i64>", true, true),
            I64x2ExtendLowI32x4U => self.extend_half("<4 x i32>", "<2 x i64>", false, false),
            I64x2ExtendHighI32x4U => self.extend_half("<4 x i32>", "<2 x i64>", false, true),
            I64x2Shl => self.vshift("<2 x i64>", "i64", 2, 64, "shl"),
            I64x2ShrS => self.vshift("<2 x i64>", "i64", 2, 64, "ashr"),
            I64x2ShrU => self.vshift("<2 x i64>", "i64", 2, 64, "lshr"),
            I64x2Add => self.bin_int_vv("<2 x i64>", "add"),
            I64x2Sub => self.bin_int_vv("<2 x i64>", "sub"),
            I64x2Mul => self.bin_int_vv("<2 x i64>", "mul"),
            I64x2ExtmulLowI32x4S => self.extmul("<4 x i32>", "<2 x i64>", true, false),
            I64x2ExtmulHighI32x4S => self.extmul("<4 x i32>", "<2 x i64>", true, true),
            I64x2ExtmulLowI32x4U => self.extmul("<4 x i32>", "<2 x i64>", false, false),
            I64x2ExtmulHighI32x4U => self.extmul("<4 x i32>", "<2 x i64>", false, true),
            // f32x4
            F32x4Ceil => self.simd_fun("<4 x float>", "llvm.ceil.v4f32"),
            F32x4Floor => self.simd_fun("<4 x float>", "llvm.floor.v4f32"),
            F32x4Trunc => self.simd_fun("<4 x float>", "llvm.trunc.v4f32"),
            F32x4Nearest => self.simd_fun("<4 x float>", "llvm.roundeven.v4f32"),
            F32x4Abs => self.simd_fun("<4 x float>", "llvm.fabs.v4f32"),
            F32x4Sqrt => self.simd_fun("<4 x float>", "llvm.sqrt.v4f32"),
            F32x4Neg => {
                let a = self.pop_v();
                let av = self.bc(&a, "<4 x float>");
                let r = self.val_lty(
                    ValType::V128,
                    "<4 x float>",
                    format!("fneg <4 x float> {}", av.s),
                );
                self.push_v(r);
            }
            F32x4Add => self.simd_fbin("<4 x float>", "fadd"),
            F32x4Sub => self.simd_fbin("<4 x float>", "fsub"),
            F32x4Mul => self.simd_fbin("<4 x float>", "fmul"),
            F32x4Div => self.simd_fbin("<4 x float>", "fdiv"),
            F32x4Min => self.simd_fminmax("<4 x float>", "float", "v4f32", "minimum"),
            F32x4Max => self.simd_fminmax("<4 x float>", "float", "v4f32", "maximum"),
            F32x4Pmin => self.pminmax("<4 x float>", true),
            F32x4Pmax => self.pminmax("<4 x float>", false),
            // f64x2
            F64x2Ceil => self.simd_fun("<2 x double>", "llvm.ceil.v2f64"),
            F64x2Floor => self.simd_fun("<2 x double>", "llvm.floor.v2f64"),
            F64x2Trunc => self.simd_fun("<2 x double>", "llvm.trunc.v2f64"),
            F64x2Nearest => self.simd_fun("<2 x double>", "llvm.roundeven.v2f64"),
            F64x2Abs => self.simd_fun("<2 x double>", "llvm.fabs.v2f64"),
            F64x2Sqrt => self.simd_fun("<2 x double>", "llvm.sqrt.v2f64"),
            F64x2Neg => {
                let a = self.pop_v();
                let av = self.bc(&a, "<2 x double>");
                let r = self.val_lty(
                    ValType::V128,
                    "<2 x double>",
                    format!("fneg <2 x double> {}", av.s),
                );
                self.push_v(r);
            }
            F64x2Add => self.simd_fbin("<2 x double>", "fadd"),
            F64x2Sub => self.simd_fbin("<2 x double>", "fsub"),
            F64x2Mul => self.simd_fbin("<2 x double>", "fmul"),
            F64x2Div => self.simd_fbin("<2 x double>", "fdiv"),
            F64x2Min => self.simd_fminmax("<2 x double>", "double", "v2f64", "minimum"),
            F64x2Max => self.simd_fminmax("<2 x double>", "double", "v2f64", "maximum"),
            F64x2Pmin => self.pminmax("<2 x double>", true),
            F64x2Pmax => self.pminmax("<2 x double>", false),
            // conversions
            I32x4TruncSatF32x4S => {
                self.simd_sat_cvt("<4 x i32>", "llvm.fptosi.sat.v4i32.v4f32", "<4 x float>")
            }
            I32x4TruncSatF32x4U => {
                self.simd_sat_cvt("<4 x i32>", "llvm.fptoui.sat.v4i32.v4f32", "<4 x float>")
            }
            I32x4TruncSatF64x2SZero => {
                self.simd_sat_cvt_zero("<2 x i32>", "llvm.fptosi.sat.v2i32.v2f64", "<2 x double>")
            }
            I32x4TruncSatF64x2UZero => {
                self.simd_sat_cvt_zero("<2 x i32>", "llvm.fptoui.sat.v2i32.v2f64", "<2 x double>")
            }
            F32x4ConvertI32x4S => {
                let a = self.pop_v();
                let av = self.bc(&a, "<4 x i32>");
                let r = self.val_lty(
                    ValType::V128,
                    "<4 x float>",
                    format!("sitofp <4 x i32> {} to <4 x float>", av.s),
                );
                self.push_v(r);
            }
            F32x4ConvertI32x4U => {
                let a = self.pop_v();
                let av = self.bc(&a, "<4 x i32>");
                let r = self.val_lty(
                    ValType::V128,
                    "<4 x float>",
                    format!("uitofp <4 x i32> {} to <4 x float>", av.s),
                );
                self.push_v(r);
            }
            F64x2ConvertLowI32x4S => {
                let a = self.pop_v();
                let av = self.bc(&a, "<4 x i32>");
                let lo = self.new_reg(format!(
                    "shufflevector <4 x i32> {}, <4 x i32> poison, <2 x i32> <i32 0, i32 1>",
                    av.s
                ));
                let r = self.val_lty(
                    ValType::V128,
                    "<2 x double>",
                    format!("sitofp <2 x i32> {} to <2 x double>", lo),
                );
                self.push_v(r);
            }
            F64x2ConvertLowI32x4U => {
                let a = self.pop_v();
                let av = self.bc(&a, "<4 x i32>");
                let lo = self.new_reg(format!(
                    "shufflevector <4 x i32> {}, <4 x i32> poison, <2 x i32> <i32 0, i32 1>",
                    av.s
                ));
                let r = self.val_lty(
                    ValType::V128,
                    "<2 x double>",
                    format!("uitofp <2 x i32> {} to <2 x double>", lo),
                );
                self.push_v(r);
            }
            F32x4DemoteF64x2Zero => {
                let a = self.pop_v();
                let av = self.bc(&a, "<2 x double>");
                let d = self.new_reg(format!(
                    "fptrunc <2 x double> {} to <2 x float>",
                    av.s
                ));
                let r = self.val_lty(
                    ValType::V128,
                    "<4 x float>",
                    format!(
                        "shufflevector <2 x float> {}, <2 x float> zeroinitializer, <4 x i32> <i32 0, i32 1, i32 2, i32 3>",
                        d
                    ),
                );
                self.push_v(r);
            }
            F64x2PromoteLowF32x4 => {
                let a = self.pop_v();
                let av = self.bc(&a, "<4 x float>");
                let lo = self.new_reg(format!(
                    "shufflevector <4 x float> {}, <4 x float> poison, <2 x i32> <i32 0, i32 1>",
                    av.s
                ));
                let r = self.val_lty(
                    ValType::V128,
                    "<2 x double>",
                    format!("fpext <2 x float> {} to <2 x double>", lo),
                );
                self.push_v(r);
            }
        }
    }

    fn simd_abs(&mut self, lty: &str) {
        let name = format!("llvm.abs.{}", vec_name(lty));
        let decl = format!("declare {} @{}({}, i1)", lty, name, lty);
        let a = self.pop_v();
        let a = self.bc(&a, lty);
        let sym = self.reg_decl(&name, &decl);
        let r = self.val_lty(
            ValType::V128,
            lty,
            format!("call {} {}({} {}, i1 false)", lty, sym, lty, a.s),
        );
        self.push_v(r);
    }

    fn simd_fminmax(&mut self, lty: &str, elem: &str, fname: &str, which: &str) {
        let name = format!("llvm.{}.{}", which, fname);
        let decl = format!("declare {} @{}({}, {})", lty, name, lty, lty);
        let b = self.pop_v();
        let a = self.pop_v();
        let a = self.bc(&a, lty);
        let b = self.bc(&b, lty);
        let sym = self.reg_decl(&name, &decl);
        let num = self.new_reg(format!(
            "call {} {}({} {}, {} {})",
            lty, sym, lty, a.s, lty, b.s
        ));
        let isnan = self.new_reg(format!("fcmp uno {} {}, {}", lty, a.s, b.s));
        let lanes = lanes_of(lty);
        let bits = if elem == "float" { "7FC00000" } else { "7FF8000000000000" };
        let r = self.val_lty(
            ValType::V128,
            lty,
            format!(
                "select <{} x i1> {}, {} splat ({} f0x{}), {} {}",
                lanes, isnan, lty, elem, bits, lty, num
            ),
        );
        self.push_v(r);
    }

    fn simd_fun(&mut self, lty: &str, name: &str) {
        let decl = format!("declare {} @{}({})", lty, name, lty);
        let a = self.pop_v();
        let a = self.bc(&a, lty);
        let sym = self.reg_decl(name, &decl);
        let r = self.val_lty(
            ValType::V128,
            lty,
            format!("call {} {}({} {})", lty, sym, lty, a.s),
        );
        self.push_v(r);
    }

    fn simd_fbin(&mut self, lty: &str, op: &str) {
        let b = self.pop_v();
        let a = self.pop_v();
        let a = self.bc(&a, lty);
        let b = self.bc(&b, lty);
        let r = self.val_lty(ValType::V128, lty, format!("{} {} {}, {}", op, lty, a.s, b.s));
        self.push_v(r);
    }

    fn pminmax(&mut self, lty: &str, is_min: bool) {
        let b = self.pop_v();
        let a = self.pop_v();
        let a = self.bc(&a, lty);
        let b = self.bc(&b, lty);
        let c = if is_min {
            self.new_reg(format!("fcmp olt {} {}, {}", lty, b.s, a.s))
        } else {
            self.new_reg(format!("fcmp olt {} {}, {}", lty, a.s, b.s))
        };
        let r = self.val_lty(
            ValType::V128,
            lty,
            format!("select <{} x i1> {}, {} {}, {} {}", lanes_of(lty), c, lty, b.s, lty, a.s),
        );
        self.push_v(r);
    }

    fn avgr(&mut self, in_lty: &str, wide_lty: &str, in_elem: &str, wide_elem: &str, lanes: u32) {
        let b = self.pop_v();
        let a = self.pop_v();
        let a = self.bc(&a, in_lty);
        let b = self.bc(&b, in_lty);
        let aw = self.new_reg(format!("zext {} {} to {}", in_lty, a.s, wide_lty));
        let bw = self.new_reg(format!("zext {} {} to {}", in_lty, b.s, wide_lty));
        let s = self.new_reg(format!("add {} {}, {}", wide_lty, aw, bw));
        let s1 = self.new_reg(format!("add {} {}, splat ({} 1)", wide_lty, s, wide_elem));
        let sh = self.new_reg(format!("lshr {} {}, splat ({} 1)", wide_lty, s1, wide_elem));
        let r = self.val_lty(
            ValType::V128,
            in_lty,
            format!("trunc {} {} to {}", wide_lty, sh, in_lty),
        );
        let _ = (in_elem, wide_elem, lanes);
        self.push_v(r);
    }

    fn q15mulr(&mut self) {
        let b = self.pop_v();
        let a = self.pop_v();
        let a = self.bc(&a, "<8 x i16>");
        let b = self.bc(&b, "<8 x i16>");
        let aw = self.new_reg(format!("sext <8 x i16> {} to <8 x i32>", a.s));
        let bw = self.new_reg(format!("sext <8 x i16> {} to <8 x i32>", b.s));
        let m = self.new_reg(format!("mul <8 x i32> {}, {}", aw, bw));
        let m1 = self.new_reg(format!("add <8 x i32> {}, splat (i32 16384)", m));
        let sh = self.new_reg(format!("ashr <8 x i32> {}, splat (i32 15)", m1));
        let sym_max = self.reg_decl(
            "llvm.smin.v8i32",
            "declare <8 x i32> @llvm.smin.v8i32(<8 x i32>, <8 x i32>)",
        );
        let sym_min = self.reg_decl(
            "llvm.smax.v8i32",
            "declare <8 x i32> @llvm.smax.v8i32(<8 x i32>, <8 x i32>)",
        );
        let c1 = self.new_reg(format!(
            "call <8 x i32> {}(<8 x i32> {}, <8 x i32> <i32 32767, i32 32767, i32 32767, i32 32767, i32 32767, i32 32767, i32 32767, i32 32767>)",
            sym_max, sh
        ));
        let c2 = self.new_reg(format!(
            "call <8 x i32> {}(<8 x i32> {}, <8 x i32> <i32 -32768, i32 -32768, i32 -32768, i32 -32768, i32 -32768, i32 -32768, i32 -32768, i32 -32768>)",
            sym_min, c1
        ));
        let r = self.val_lty(
            ValType::V128,
            "<8 x i16>",
            format!("trunc <8 x i32> {} to <8 x i16>", c2),
        );
        self.push_v(r);
    }

    fn dot_i16x8s(&mut self) {
        let b = self.pop_v();
        let a = self.pop_v();
        let a = self.bc(&a, "<8 x i16>");
        let b = self.bc(&b, "<8 x i16>");
        let aw = self.new_reg(format!("sext <8 x i16> {} to <8 x i32>", a.s));
        let bw = self.new_reg(format!("sext <8 x i16> {} to <8 x i32>", b.s));
        let m = self.new_reg(format!("mul <8 x i32> {}, {}", aw, bw));
        let e = self.new_reg(format!(
            "shufflevector <8 x i32> {}, <8 x i32> poison, <4 x i32> <i32 0, i32 2, i32 4, i32 6>",
            m
        ));
        let o = self.new_reg(format!(
            "shufflevector <8 x i32> {}, <8 x i32> poison, <4 x i32> <i32 1, i32 3, i32 5, i32 7>",
            m
        ));
        let r = self.val_lty(
            ValType::V128,
            "<4 x i32>",
            format!("add <4 x i32> {}, {}", e, o),
        );
        self.push_v(r);
    }

    fn simd_sat_cvt(&mut self, out_lty: &str, name: &str, in_lty: &str) {
        let decl = format!("declare {} @{}({})", out_lty, name, in_lty);
        let a = self.pop_v();
        let a = self.bc(&a, in_lty);
        let sym = self.reg_decl(name, &decl);
        let r = self.val_lty(
            ValType::V128,
            out_lty,
            format!("call {} {}({} {})", out_lty, sym, in_lty, a.s),
        );
        self.push_v(r);
    }

    fn simd_sat_cvt_zero(&mut self, out_lty: &str, name: &str, in_lty: &str) {
        // Converts two lanes and zero-extends with zeros to a 4-lane vector.
        let decl = format!("declare {} @{}({})", out_lty, name, in_lty);
        let a = self.pop_v();
        let a = self.bc(&a, in_lty);
        let sym = self.reg_decl(name, &decl);
        let two = self.new_reg(format!("call {} {}({} {})", out_lty, sym, in_lty, a.s));
        let full = "<4 x i32>";
        let r = self.val_lty(
            ValType::V128,
            full,
            format!(
                "shufflevector {} {}, {} zeroinitializer, <4 x i32> <i32 0, i32 1, i32 2, i32 3>",
                out_lty, two, out_lty
            ),
        );
        self.push_v(r);
    }
}

fn bits_of_ty(ty: &str) -> u32 {
    match ty {
        "i1" => 1,
        "i8" => 8,
        "i16" => 16,
        "i32" => 32,
        "i64" => 64,
        _ => 64,
    }
}

fn lanes_of(lty: &str) -> u32 {
    // "<16 x i8>" -> 16
    let inner = lty.trim_start_matches('<');
    let n: String = inner.chars().take_while(|c| c.is_ascii_digit()).collect();
    n.parse().unwrap_or(1)
}

fn elem_of(lty: &str) -> &'static str {
    if lty.ends_with("i8>") {
        "i8"
    } else if lty.ends_with("i16>") {
        "i16"
    } else if lty.ends_with("i32>") {
        "i32"
    } else if lty.ends_with("i64>") {
        "i64"
    } else if lty.ends_with("float>") {
        "float"
    } else {
        "double"
    }
}

fn vec_name(lty: &str) -> String {
    // "<8 x i16>" -> "v8i16"
    let lanes = lanes_of(lty);
    let e = elem_of(lty);
    format!("v{}{}", lanes, e)
}

fn cmp_pred(op: SimdOp) -> &'static str {
    use SimdOp::*;
    match op {
        I8x16Eq | I16x8Eq | I32x4Eq | I64x2Eq | F32x4Eq | F64x2Eq => "eq",
        I8x16Ne | I16x8Ne | I32x4Ne | I64x2Ne | F32x4Ne | F64x2Ne => "ne",
        I8x16LtS | I16x8LtS | I32x4LtS | I64x2LtS => "slt",
        I8x16LtU | I16x8LtU | I32x4LtU => "ult",
        I8x16GtS | I16x8GtS | I32x4GtS | I64x2GtS => "sgt",
        I8x16GtU | I16x8GtU | I32x4GtU => "ugt",
        I8x16LeS | I16x8LeS | I32x4LeS | I64x2LeS => "sle",
        I8x16LeU | I16x8LeU | I32x4LeU => "ule",
        I8x16GeS | I16x8GeS | I32x4GeS | I64x2GeS => "sge",
        I8x16GeU | I16x8GeU | I32x4GeU => "uge",
        F32x4Lt | F64x2Lt => "olt",
        F32x4Gt | F64x2Gt => "ogt",
        F32x4Le | F64x2Le => "ole",
        F32x4Ge | F64x2Ge => "oge",
        _ => "eq",
    }
}

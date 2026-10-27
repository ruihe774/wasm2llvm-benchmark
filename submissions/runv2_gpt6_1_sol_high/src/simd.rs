use crate::ir::{Emitter, Val};
use crate::model::{Instr, Ty};
const V: &str = "<16 x i8>";
fn vt(n: u32, t: &str) -> String {
    format!("<{n} x {t}>")
}
fn it(w: u32) -> String {
    vt(128 / w, &format!("i{w}"))
}
fn splat_const(n: u32, t: &str, x: impl ToString) -> String {
    let x = x.to_string();
    format!(
        "<{}>",
        (0..n)
            .map(|_| format!("{t} {x}"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}
fn cast(e: &mut Emitter, v: Val, t: &str) -> String {
    if t == V {
        v.v
    } else {
        e.raw(format!("bitcast {V} {} to {t}", v.v))
    }
}
fn pop(e: &mut Emitter, t: &str) -> Result<String, String> {
    let v = e.pop(Ty::V128)?;
    Ok(cast(e, v, t))
}
fn put(e: &mut Emitter, t: &str, v: String) {
    let v = if t == V {
        v
    } else {
        e.raw(format!("bitcast {t} {v} to {V}"))
    };
    e.push(Val { ty: Ty::V128, v });
}
fn mask(xs: impl Iterator<Item = u32>) -> String {
    format!(
        "<{}>",
        xs.map(|i| format!("i32 {i}"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}
fn shuffle(e: &mut Emitter, t: &str, a: &str, b: &str, indices: Vec<u32>) -> String {
    e.raw(format!(
        "shufflevector {t} {a}, {t} {b}, <{} x i32> {}",
        indices.len(),
        mask(indices.into_iter())
    ))
}
fn splat(e: &mut Emitter, n: u32, t: &str, x: &str) -> String {
    let ty = vt(n, t);
    let a = e.raw(format!(
        "insertelement {ty} zeroinitializer, {t} {x}, i32 0"
    ));
    e.raw(format!(
        "shufflevector {ty} {a}, {ty} zeroinitializer, <{n} x i32> zeroinitializer"
    ))
}
fn intrinsic(e: &mut Emitter, name: &str, suf: &str, t: &str, args: &[String]) -> String {
    let f = format!("@llvm.{name}.{suf}");
    e.declare(format!(
        "declare {t} {f}({})",
        vec![t; args.len()].join(", ")
    ));
    e.raw(format!(
        "call {t} {f}({})",
        args.iter()
            .map(|a| format!("{t} {a}"))
            .collect::<Vec<_>>()
            .join(", ")
    ))
}
fn int_bin(e: &mut Emitter, w: u32, name: &str, intr: bool) -> Result<(), String> {
    let t = it(w);
    let b = pop(e, &t)?;
    let a = pop(e, &t)?;
    let v = if intr {
        intrinsic(e, name, &format!("v{}i{w}", 128 / w), &t, &[a, b])
    } else {
        e.raw(format!("{name} {t} {a}, {b}"))
    };
    put(e, &t, v);
    Ok(())
}
fn extend_half(e: &mut Emitter, a: &str, w: u32, high: bool, signed: bool) -> String {
    let small = it(w);
    let n = 64 / w;
    let start = if high { n } else { 0 };
    let a = shuffle(
        e,
        &small,
        a,
        "zeroinitializer",
        (start..start + n).collect(),
    );
    e.raw(format!(
        "{} {} {a} to {}",
        if signed { "sext" } else { "zext" },
        vt(n, &format!("i{w}")),
        it(w * 2)
    ))
}

pub fn emit(e: &mut Emitter, i: &Instr) -> Result<(), String> {
    let op = i.op - 0xfd00;
    match op {
        0..=11 | 84..=93 => {
            if op == 11 {
                let v = e.pop(Ty::V128)?;
                let a = e.pop(Ty::I32)?;
                let p = e.mem_ptr(&a, i.b as u64, 16)?;
                e.line(format!("store {V} {}, ptr {p}, align 1", v.v));
                return Ok(());
            }
            if (84..=91).contains(&op) {
                let v = e.pop(Ty::V128)?;
                let a = e.pop(Ty::I32)?;
                let w = 8u32 << (op % 4);
                let t = it(w);
                let x = cast(e, v, &t);
                let p = e.mem_ptr(&a, i.b as u64, (w / 8) as u64)?;
                if op < 88 {
                    let x0 = e.raw(format!("load i{w}, ptr {p}, align 1"));
                    let x = e.raw(format!("insertelement {t} {x}, i{w} {x0}, i32 {}", i.imm));
                    put(e, &t, x)
                } else {
                    let x = e.raw(format!("extractelement {t} {x}, i32 {}", i.imm));
                    e.line(format!("store i{w} {x}, ptr {p}, align 1"));
                }
                return Ok(());
            }
            let a = e.pop(Ty::I32)?;
            if op == 0 {
                let p = e.mem_ptr(&a, i.b as u64, 16)?;
                let x = e.raw(format!("load {V}, ptr {p}, align 1"));
                put(e, V, x);
            } else if op <= 6 {
                let w = 8u32 << ((op - 1) / 2);
                let n = 64 / w;
                let src = vt(n, &format!("i{w}"));
                let dst = it(w * 2);
                let p = e.mem_ptr(&a, i.b as u64, 8)?;
                let x = e.raw(format!("load {src}, ptr {p}, align 1"));
                let x = e.raw(format!(
                    "{} {src} {x} to {dst}",
                    if op % 2 == 1 { "sext" } else { "zext" }
                ));
                put(e, &dst, x);
            } else if op <= 10 {
                let w = 8u32 << (op - 7);
                let p = e.mem_ptr(&a, i.b as u64, (w / 8) as u64)?;
                let x = e.raw(format!("load i{w}, ptr {p}, align 1"));
                let x = splat(e, 128 / w, &format!("i{w}"), &x);
                put(e, &it(w), x);
            } else {
                let w = if op == 92 { 32 } else { 64 };
                let p = e.mem_ptr(&a, i.b as u64, (w / 8) as u64)?;
                let x = e.raw(format!("load i{w}, ptr {p}, align 1"));
                let t = it(w);
                let x = e.raw(format!(
                    "insertelement {t} zeroinitializer, i{w} {x}, i32 0"
                ));
                put(e, &t, x);
            }
            return Ok(());
        }
        12 => {
            e.push(Val {
                ty: Ty::V128,
                v: format!(
                    "<{}>",
                    i.bytes
                        .iter()
                        .map(|b| format!("i8 {b}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            });
            return Ok(());
        }
        13 => {
            let b = pop(e, V)?;
            let a = pop(e, V)?;
            let x = shuffle(e, V, &a, &b, i.bytes.iter().map(|x| *x as u32).collect());
            put(e, V, x);
            return Ok(());
        }
        14 => {
            let ix = pop(e, V)?;
            let a = pop(e, V)?;
            // A portable vector lookup network. Every shuffle index is constant;
            // unlike dynamic extractelement, this remains defined for indices >= 16.
            let mut choices = (0..16)
                .map(|j| shuffle(e, V, &a, "zeroinitializer", vec![j; 16]))
                .collect::<Vec<_>>();
            for bit in 0..4 {
                let b = e.raw(format!("and {V} {ix}, {}", splat_const(16, "i8", 1 << bit)));
                let c = e.raw(format!("icmp ne {V} {b}, zeroinitializer"));
                choices = choices
                    .chunks_exact(2)
                    .map(|p| e.raw(format!("select <16 x i1> {c}, {V} {}, {V} {}", p[1], p[0])))
                    .collect();
            }
            let valid = e.raw(format!("icmp ult {V} {ix}, {}", splat_const(16, "i8", 16)));
            let res = e.raw(format!(
                "select <16 x i1> {valid}, {V} {}, {V} zeroinitializer",
                choices[0]
            ));
            put(e, V, res);
            return Ok(());
        }
        15..=20 => {
            let (w, st, ty) = match op {
                15 => (8, "i8", Ty::I32),
                16 => (16, "i16", Ty::I32),
                17 => (32, "i32", Ty::I32),
                18 => (64, "i64", Ty::I64),
                19 => (32, "float", Ty::F32),
                _ => (64, "double", Ty::F64),
            };
            let a = e.pop(ty)?;
            let x = if w < 32 {
                e.raw(format!("trunc i32 {} to {st}", a.v))
            } else {
                a.v
            };
            let x = splat(e, 128 / w, st, &x);
            put(e, &vt(128 / w, st), x);
            return Ok(());
        }
        21..=34 => {
            let (w, st, ty, replace, signed) = match op {
                21 => (8, "i8", Ty::I32, false, true),
                22 => (8, "i8", Ty::I32, false, false),
                23 => (8, "i8", Ty::I32, true, false),
                24 => (16, "i16", Ty::I32, false, true),
                25 => (16, "i16", Ty::I32, false, false),
                26 => (16, "i16", Ty::I32, true, false),
                27 => (32, "i32", Ty::I32, false, false),
                28 => (32, "i32", Ty::I32, true, false),
                29 => (64, "i64", Ty::I64, false, false),
                30 => (64, "i64", Ty::I64, true, false),
                31 => (32, "float", Ty::F32, false, false),
                32 => (32, "float", Ty::F32, true, false),
                33 => (64, "double", Ty::F64, false, false),
                _ => (64, "double", Ty::F64, true, false),
            };
            let t = vt(128 / w, st);
            if replace {
                let b = e.pop(ty)?;
                let a = pop(e, &t)?;
                let b = if w < 32 {
                    e.raw(format!("trunc i32 {} to {st}", b.v))
                } else {
                    b.v
                };
                let x = e.raw(format!("insertelement {t} {a}, {st} {b}, i32 {}", i.imm));
                put(e, &t, x)
            } else {
                let a = pop(e, &t)?;
                let x = e.raw(format!("extractelement {t} {a}, i32 {}", i.imm));
                let x = if w < 32 {
                    e.raw(format!(
                        "{} {st} {x} to i32",
                        if signed { "sext" } else { "zext" }
                    ))
                } else {
                    x
                };
                e.push(Val { ty, v: x })
            }
            return Ok(());
        }
        35..=76 | 214..=219 => {
            let (w, st, pred, float) = if op <= 64 {
                let (w, k) = if op < 45 {
                    (8, op - 35)
                } else if op < 55 {
                    (16, op - 45)
                } else {
                    (32, op - 55)
                };
                (
                    w,
                    format!("i{w}"),
                    [
                        "eq", "ne", "slt", "ult", "sgt", "ugt", "sle", "ule", "sge", "uge",
                    ][k as usize],
                    false,
                )
            } else if op <= 76 {
                let w = if op < 71 { 32 } else { 64 };
                (
                    w,
                    if w == 32 { "float" } else { "double" }.to_string(),
                    ["oeq", "une", "olt", "ogt", "ole", "oge"]
                        [(op - if w == 32 { 65 } else { 71 }) as usize],
                    true,
                )
            } else {
                (
                    64,
                    "i64".into(),
                    ["eq", "ne", "slt", "sgt", "sle", "sge"][(op - 214) as usize],
                    false,
                )
            };
            let t = vt(128 / w, &st);
            let b = pop(e, &t)?;
            let a = pop(e, &t)?;
            let c = e.raw(format!(
                "{} {pred} {t} {a}, {b}",
                if float { "fcmp" } else { "icmp" }
            ));
            let x = e.raw(format!("sext <{} x i1> {c} to {}", 128 / w, it(w)));
            put(e, &it(w), x);
            return Ok(());
        }
        77..=83 => {
            if op == 83 {
                let a = pop(e, V)?;
                let a = e.raw(format!("bitcast {V} {a} to i128"));
                let x = e.raw(format!("icmp ne i128 {a}, 0"));
                let x = e.value(Ty::I32, format!("zext i1 {x} to i32"));
                e.push(x);
                return Ok(());
            }
            if op == 77 {
                let a = pop(e, V)?;
                let x = e.raw(format!("xor {V} {a}, {}", splat_const(16, "i8", -1)));
                put(e, V, x);
                return Ok(());
            }
            let mask = if op == 82 { Some(pop(e, V)?) } else { None };
            let b = pop(e, V)?;
            let a = pop(e, V)?;
            let x = if let Some(m) = mask {
                let am = e.raw(format!("and {V} {a}, {m}"));
                let inv = e.raw(format!("xor {V} {m}, {}", splat_const(16, "i8", -1)));
                let bm = e.raw(format!("and {V} {b}, {inv}"));
                e.raw(format!("or {V} {am}, {bm}"))
            } else if op == 79 {
                let inv = e.raw(format!("xor {V} {b}, {}", splat_const(16, "i8", -1)));
                e.raw(format!("and {V} {a}, {inv}"))
            } else {
                e.raw(format!(
                    "{} {V} {a}, {b}",
                    match op {
                        78 => "and",
                        80 => "or",
                        _ => "xor",
                    }
                ))
            };
            put(e, V, x);
            return Ok(());
        }
        94 => {
            let a = pop(e, "<2 x double>")?;
            let x = e.raw(format!("fptrunc <2 x double> {a} to <2 x float>"));
            let x = shuffle(e, "<2 x float>", &x, "zeroinitializer", vec![0, 1, 2, 3]);
            put(e, "<4 x float>", x);
            return Ok(());
        }
        95 => {
            let a = pop(e, "<4 x float>")?;
            let x = shuffle(e, "<4 x float>", &a, "zeroinitializer", vec![0, 1]);
            let x = e.raw(format!("fpext <2 x float> {x} to <2 x double>"));
            put(e, "<2 x double>", x);
            return Ok(());
        }
        _ => {}
    }
    // Floating rounding instructions are interleaved with integer encodings.
    if matches!(op, 0x67 | 0x68 | 0x69 | 0x6a | 0x74 | 0x75 | 0x7a | 0x94) {
        let (n, st, name) = match op {
            0x67 => (4, "float", "ceil"),
            0x68 => (4, "float", "floor"),
            0x69 => (4, "float", "trunc"),
            0x6a => (4, "float", "roundeven"),
            0x74 => (2, "double", "ceil"),
            0x75 => (2, "double", "floor"),
            0x7a => (2, "double", "trunc"),
            _ => (2, "double", "roundeven"),
        };
        let t = vt(n, st);
        let a = pop(e, &t)?;
        let x = intrinsic(e, name, &format!("v{n}f{}", 128 / n), &t, &[a]);
        put(e, &t, x);
        return Ok(());
    }
    if (0xe0..=0xf7).contains(&op) {
        let (w, k) = if op < 0xec {
            (32, op - 0xe0)
        } else {
            (64, op - 0xec)
        };
        let n = 128 / w;
        let st = if w == 32 { "float" } else { "double" };
        let t = vt(n, st);
        let suf = format!("v{n}f{w}");
        let x = if k < 4 {
            let a = pop(e, &t)?;
            match k {
                0 => intrinsic(e, "fabs", &suf, &t, &[a]),
                1 => e.raw(format!("fneg {t} {a}")),
                3 => intrinsic(e, "sqrt", &suf, &t, &[a]),
                _ => return Err("reserved SIMD opcode".into()),
            }
        } else {
            let b = pop(e, &t)?;
            let a = pop(e, &t)?;
            match k {
                4..=7 => e.raw(format!(
                    "{} {t} {a}, {b}",
                    ["fadd", "fsub", "fmul", "fdiv"][(k - 4) as usize]
                )),
                8 | 9 => intrinsic(
                    e,
                    if k == 8 { "minimum" } else { "maximum" },
                    &suf,
                    &t,
                    &[a, b],
                ),
                10 | 11 => {
                    let c = e.raw(format!(
                        "fcmp olt {t} {}, {}",
                        if k == 10 { &b } else { &a },
                        if k == 10 { &a } else { &b }
                    ));
                    e.raw(format!("select <{n} x i1> {c}, {t} {b}, {t} {a}"))
                }
                _ => return Err("reserved SIMD opcode".into()),
            }
        };
        put(e, &t, x);
        return Ok(());
    }
    if op >= 0xf8 {
        match op {
            0xf8 | 0xf9 | 0xfc | 0xfd => {
                let (n, ft) = if op < 0xfc {
                    (4, "float")
                } else {
                    (2, "double")
                };
                let src = vt(n, ft);
                let dest = vt(n, "i32");
                let a = pop(e, &src)?;
                let name = if op % 2 == 0 { "fptosi" } else { "fptoui" };
                let intr = format!(
                    "@llvm.{name}.sat.v{n}i32.v{n}f{}",
                    if n == 4 { 32 } else { 64 }
                );
                e.declare(format!("declare {dest} {intr}({src})"));
                let mut x = e.raw(format!("call {dest} {intr}({src} {a})"));
                if n == 2 {
                    x = shuffle(e, &dest, &x, "zeroinitializer", vec![0, 1, 2, 3]);
                }
                put(e, "<4 x i32>", x)
            }
            0xfa | 0xfb | 0xfe | 0xff => {
                let a = pop(e, "<4 x i32>")?;
                let (src, dst, a) = if op < 0xfe {
                    ("<4 x i32>", "<4 x float>", a)
                } else {
                    let a = shuffle(e, "<4 x i32>", &a, "zeroinitializer", vec![0, 1]);
                    ("<2 x i32>", "<2 x double>", a)
                };
                let x = e.raw(format!(
                    "{} {src} {a} to {dst}",
                    if op % 2 == 0 { "sitofp" } else { "uitofp" }
                ));
                put(e, dst, x)
            }
            _ => return Err("reserved SIMD opcode".into()),
        }
        return Ok(());
    }
    if (0x7c..=0x7f).contains(&op) {
        let w = if op < 0x7e { 8 } else { 16 };
        let signed = op % 2 == 0;
        let src = it(w);
        let dst = it(w * 2);
        let a = pop(e, &src)?;
        let n = 64 / w;
        let even = shuffle(
            e,
            &src,
            &a,
            "zeroinitializer",
            (0..n).map(|i| i * 2).collect(),
        );
        let odd = shuffle(
            e,
            &src,
            &a,
            "zeroinitializer",
            (0..n).map(|i| i * 2 + 1).collect(),
        );
        let half = vt(n, &format!("i{w}"));
        let conv = if signed { "sext" } else { "zext" };
        let a = e.raw(format!("{conv} {half} {even} to {dst}"));
        let b = e.raw(format!("{conv} {half} {odd} to {dst}"));
        let x = e.raw(format!("add {dst} {a}, {b}"));
        put(e, &dst, x);
        return Ok(());
    }
    let w = if op < 0x80 {
        8
    } else if op < 0xa0 {
        16
    } else if op < 0xc0 {
        32
    } else {
        64
    };
    let k = op & 31;
    let t = it(w);
    let n = 128 / w;
    let suf = format!("v{n}i{w}");
    match k {
        0 | 1 => {
            let a = pop(e, &t)?;
            let x = if k == 1 {
                e.raw(format!("sub {t} zeroinitializer, {a}"))
            } else {
                e.declare(format!("declare {t} @llvm.abs.{suf}({t}, i1)"));
                e.raw(format!("call {t} @llvm.abs.{suf}({t} {a}, i1 false)"))
            };
            put(e, &t, x)
        }
        2 if w == 8 => {
            let a = pop(e, &t)?;
            let x = intrinsic(e, "ctpop", &suf, &t, &[a]);
            put(e, &t, x)
        }
        2 if w == 16 => {
            let b = pop(e, &t)?;
            let a = pop(e, &t)?;
            let big = "<8 x i32>";
            let a = e.raw(format!("sext {t} {a} to {big}"));
            let b = e.raw(format!("sext {t} {b} to {big}"));
            let x = e.raw(format!("mul {big} {a}, {b}"));
            let x = e.raw(format!("add {big} {x}, {}", splat_const(8, "i32", 16384)));
            let x = e.raw(format!("ashr {big} {x}, {}", splat_const(8, "i32", 15)));
            let x = intrinsic(e, "smin", "v8i32", big, &[x, splat_const(8, "i32", 32767)]);
            let x = e.raw(format!("trunc {big} {x} to {t}"));
            put(e, &t, x)
        }
        3 => {
            let a = pop(e, &t)?;
            let c = e.raw(format!("icmp ne {t} {a}, zeroinitializer"));
            e.declare(format!(
                "declare i1 @llvm.vector.reduce.and.v{n}i1(<{n} x i1>)"
            ));
            let x = e.raw(format!(
                "call i1 @llvm.vector.reduce.and.v{n}i1(<{n} x i1> {c})"
            ));
            let x = e.value(Ty::I32, format!("zext i1 {x} to i32"));
            e.push(x)
        }
        4 => {
            let a = pop(e, &t)?;
            let c = e.raw(format!("icmp slt {t} {a}, zeroinitializer"));
            let bits = e.raw(format!("bitcast <{n} x i1> {c} to i{n}"));
            let x = e.value(Ty::I32, format!("zext i{n} {bits} to i32"));
            e.push(x)
        }
        5 | 6 if w <= 16 => {
            let src = it(w * 2);
            let b = pop(e, &src)?;
            let a = pop(e, &src)?;
            let signed = k == 5;
            let low = if signed { -(1i64 << (w - 1)) } else { 0 };
            let high = if signed {
                (1i64 << (w - 1)) - 1
            } else {
                (1i64 << w) - 1
            };
            let sn = 64 / w;
            let ss = format!("v{sn}i{}", w * 2);
            let st = format!("i{}", w * 2);
            let mut halves = vec![];
            for x in [a, b] {
                let x = intrinsic(e, "smax", &ss, &src, &[x, splat_const(sn, &st, low)]);
                let x = intrinsic(e, "smin", &ss, &src, &[x, splat_const(sn, &st, high)]);
                halves.push(e.raw(format!("trunc {src} {x} to {}", vt(sn, &format!("i{w}")))))
            }
            let ht = vt(sn, &format!("i{w}"));
            let x = shuffle(e, &ht, &halves[0], &halves[1], (0..n).collect());
            put(e, &t, x)
        }
        7..=10 if w >= 16 => {
            let src = it(w / 2);
            let a = pop(e, &src)?;
            let x = extend_half(e, &a, w / 2, k % 2 == 0, k < 9);
            put(e, &t, x)
        }
        11..=13 => {
            let count = e.pop(Ty::I32)?;
            let a = pop(e, &t)?;
            let c = e.raw(format!("and i32 {}, {}", count.v, w - 1));
            let c = if w < 32 {
                e.raw(format!("trunc i32 {c} to i{w}"))
            } else if w > 32 {
                e.raw(format!("zext i32 {c} to i{w}"))
            } else {
                c
            };
            let c = splat(e, n, &format!("i{w}"), &c);
            let x = e.raw(format!(
                "{} {t} {a}, {c}",
                ["shl", "ashr", "lshr"][(k - 11) as usize]
            ));
            put(e, &t, x)
        }
        14 => return int_bin(e, w, "add", false),
        17 => return int_bin(e, w, "sub", false),
        21 if w >= 16 => return int_bin(e, w, "mul", false),
        15 | 16 | 18 | 19 if w <= 16 => {
            return int_bin(
                e,
                w,
                match k {
                    15 => "sadd.sat",
                    16 => "uadd.sat",
                    18 => "ssub.sat",
                    _ => "usub.sat",
                },
                true,
            )
        }
        22..=25 if w <= 32 => {
            return int_bin(
                e,
                w,
                ["smin", "umin", "smax", "umax"][(k - 22) as usize],
                true,
            )
        }
        26 if w == 32 => {
            let b = pop(e, "<8 x i16>")?;
            let a = pop(e, "<8 x i16>")?;
            let a = e.raw(format!("sext <8 x i16> {a} to <8 x i32>"));
            let b = e.raw(format!("sext <8 x i16> {b} to <8 x i32>"));
            let x = e.raw(format!("mul <8 x i32> {a}, {b}"));
            let a = shuffle(e, "<8 x i32>", &x, "zeroinitializer", vec![0, 2, 4, 6]);
            let b = shuffle(e, "<8 x i32>", &x, "zeroinitializer", vec![1, 3, 5, 7]);
            let x = e.raw(format!("add {t} {a}, {b}"));
            put(e, &t, x)
        }
        27 if w <= 16 => {
            let b = pop(e, &t)?;
            let a = pop(e, &t)?;
            let wide = vt(n, &format!("i{}", w * 2));
            let a = e.raw(format!("zext {t} {a} to {wide}"));
            let b = e.raw(format!("zext {t} {b} to {wide}"));
            let x = e.raw(format!("add {wide} {a}, {b}"));
            let x = e.raw(format!(
                "add {wide} {x}, {}",
                splat_const(n, &format!("i{}", w * 2), 1)
            ));
            let x = e.raw(format!(
                "lshr {wide} {x}, {}",
                splat_const(n, &format!("i{}", w * 2), 1)
            ));
            let x = e.raw(format!("trunc {wide} {x} to {t}"));
            put(e, &t, x)
        }
        28..=31 if w >= 16 => {
            let src = it(w / 2);
            let b = pop(e, &src)?;
            let a = pop(e, &src)?;
            let a = extend_half(e, &a, w / 2, k % 2 == 1, k < 30);
            let b = extend_half(e, &b, w / 2, k % 2 == 1, k < 30);
            let x = e.raw(format!("mul {t} {a}, {b}"));
            put(e, &t, x)
        }
        _ => return Err(format!("unsupported SIMD opcode {op:x}")),
    }
    Ok(())
}

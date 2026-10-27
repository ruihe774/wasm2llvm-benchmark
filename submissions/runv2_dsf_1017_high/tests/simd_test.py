#!/usr/bin/env python3
"""Exhaustive-ish SIMD instruction coverage test against w2l-ref."""
import importlib.util
import os
import random
import struct
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
spec = importlib.util.spec_from_file_location("cmp", os.path.join(ROOT, "tests", "compare.py"))
cmp = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cmp)
TMP = os.environ.get("TMPDIR", "/tmp")


def h(s):
    return s.encode().hex()


def s32(v):
    return struct.pack("<I", v & 0xFFFFFFFF).hex() + "00" * 12


def s64(v):
    return struct.pack("<Q", v & 0xFFFFFFFFFFFFFFFF).hex() + "00" * 8


def v128b(b):
    assert len(b) == 16
    return b.hex()


OPS = []  # (export_name, params, results, body_instructions, float_lane)


def op(name, params, results, body, float_lane=None):
    OPS.append((name, params, results, body, float_lane))


def binop(name, params, results, body):
    op(name, params, results, body)


def simple(name, opname, params, results):
    body = " ".join("local.get %d" % i for i in range(len(params))) + " " + opname
    op(name, params, results, body)


# --- integer lane ops ---
for sh, suffix in [("i8x16", 16), ("i16x8", 8), ("i32x4", 4)]:
    for o in ["eq", "ne", "lt_s", "lt_u", "gt_s", "gt_u", "le_s", "le_u", "ge_s", "ge_u"]:
        simple("%s_%s" % (sh, o), "%s.%s" % (sh, o), ["v128", "v128"], ["v128"])
for o in ["eq", "ne", "lt_s", "gt_s", "le_s", "ge_s"]:
    simple("i64x2_%s" % o, "i64x2.%s" % o, ["v128", "v128"], ["v128"])
for sh in ["i8x16", "i16x8", "i32x4", "i64x2"]:
    for o in ["add", "sub"]:
        simple("%s_%s" % (sh, o), "%s.%s" % (sh, o), ["v128", "v128"], ["v128"])
for sh in ["i16x8", "i32x4", "i64x2"]:
    simple("%s_mul" % sh, "%s.mul" % sh, ["v128", "v128"], ["v128"])
for sh in ["i8x16", "i16x8"]:
    for o in ["add_sat_s", "add_sat_u", "sub_sat_s", "sub_sat_u", "min_s", "min_u",
              "max_s", "max_u", "avgr_u"]:
        simple("%s_%s" % (sh, o), "%s.%s" % (sh, o), ["v128", "v128"], ["v128"])
for sh in ["i32x4"]:
    for o in ["min_s", "min_u", "max_s", "max_u"]:
        simple("%s_%s" % (sh, o), "%s.%s" % (sh, o), ["v128", "v128"], ["v128"])
simple("i16x8_q15mulr_sat_s", "i16x8.q15mulr_sat_s", ["v128", "v128"], ["v128"])
simple("i32x4_dot_i16x8_s", "i32x4.dot_i16x8_s", ["v128", "v128"], ["v128"])
for sh in ["i8x16", "i16x8", "i32x4", "i64x2"]:
    simple("%s_abs" % sh, "%s.abs" % sh, ["v128"], ["v128"])
    simple("%s_neg" % sh, "%s.neg" % sh, ["v128"], ["v128"])
simple("i8x16_popcnt", "i8x16.popcnt", ["v128"], ["v128"])
for sh in ["i8x16", "i16x8", "i32x4", "i64x2"]:
    simple("%s_all_true" % sh, "%s.all_true" % sh, ["v128"], ["i32"])
    simple("%s_bitmask" % sh, "%s.bitmask" % sh, ["v128"], ["i32"])
for sh in ["i8x16", "i16x8", "i32x4", "i64x2"]:
    for o in ["shl", "shr_s", "shr_u"]:
        simple("%s_%s" % (sh, o), "%s.%s" % (sh, o), ["v128", "i32"], ["v128"])
for sh, o in [("i8x16", "narrow_i16x8_s"), ("i8x16", "narrow_i16x8_u"),
              ("i16x8", "narrow_i32x4_s"), ("i16x8", "narrow_i32x4_u")]:
    simple("%s_%s" % (sh, o), "%s.%s" % (sh, o), ["v128", "v128"], ["v128"])
for src, dst, o in [
    ("i8x16", "i16x8", "extend_low_i8x16_s"), ("i8x16", "i16x8", "extend_high_i8x16_s"),
    ("i8x16", "i16x8", "extend_low_i8x16_u"), ("i8x16", "i16x8", "extend_high_i8x16_u"),
    ("i16x8", "i32x4", "extend_low_i16x8_s"), ("i16x8", "i32x4", "extend_high_i16x8_s"),
    ("i16x8", "i32x4", "extend_low_i16x8_u"), ("i16x8", "i32x4", "extend_high_i16x8_u"),
    ("i32x4", "i64x2", "extend_low_i32x4_s"), ("i32x4", "i64x2", "extend_high_i32x4_s"),
    ("i32x4", "i64x2", "extend_low_i32x4_u"), ("i32x4", "i64x2", "extend_high_i32x4_u"),
]:
    simple("%s_%s" % (dst, o), "%s.%s" % (dst, o), ["v128"], ["v128"])
for sh, o in [("i16x8", "extadd_pairwise_i8x16_s"), ("i16x8", "extadd_pairwise_i8x16_u"),
              ("i32x4", "extadd_pairwise_i16x8_s"), ("i32x4", "extadd_pairwise_i16x8_u")]:
    simple("%s_%s" % (sh, o), "%s.%s" % (sh, o), ["v128"], ["v128"])
for src, dst in [("i8x16", "i16x8"), ("i16x8", "i32x4"), ("i32x4", "i64x2")]:
    for o in ["extmul_low_%s_s" % src, "extmul_high_%s_s" % src,
              "extmul_low_%s_u" % src, "extmul_high_%s_u" % src]:
        simple("%s_%s" % (dst, o), "%s.%s" % (dst, o), ["v128", "v128"], ["v128"])

# --- float lanes ---
for o in ["eq", "ne", "lt", "gt", "le", "ge"]:
    simple("f32x4_%s" % o, "f32x4.%s" % o, ["v128", "v128"], ["v128"])
    simple("f64x2_%s" % o, "f64x2.%s" % o, ["v128", "v128"], ["v128"])
for sh, fl in [("f32x4", 4), ("f64x2", 8)]:
    for o in ["abs", "neg", "sqrt", "ceil", "floor", "trunc", "nearest"]:
        simple("%s_%s" % (sh, o), "%s.%s" % (sh, o), ["v128"], ["v128"], )
    for o in ["add", "sub", "mul", "div", "min", "max", "pmin", "pmax"]:
        simple("%s_%s" % (sh, o), "%s.%s" % (sh, o), ["v128", "v128"], ["v128"])
# mark float-lane ops for NaN normalization
FLOAT_LANE = {}
for name, params, results, body, fl in OPS:
    pass

# --- v128 bitwise ---
simple("v128_not", "v128.not", ["v128"], ["v128"])
for o in ["and", "andnot", "or", "xor"]:
    simple("v128_%s" % o, "v128.%s" % o, ["v128", "v128"], ["v128"])
op("v128_bitselect", ["v128", "v128", "v128"], ["v128"],
   "local.get 0 local.get 1 local.get 2 v128.bitselect")
simple("v128_any_true", "v128.any_true", ["v128"], ["i32"])
simple("i8x16_swizzle", "i8x16.swizzle", ["v128", "v128"], ["v128"])
op("i8x16_shuffle", ["v128", "v128"], ["v128"],
   "local.get 0 local.get 1 i8x16.shuffle 0 17 2 19 4 21 6 23 8 25 10 27 12 29 14 31")
# splats
simple("i8x16_splat", "i8x16.splat", ["i32"], ["v128"])
simple("i16x8_splat", "i16x8.splat", ["i32"], ["v128"])
simple("i32x4_splat", "i32x4.splat", ["i32"], ["v128"])
simple("i64x2_splat", "i64x2.splat", ["i64"], ["v128"])
simple("f32x4_splat", "f32x4.splat", ["f32"], ["v128"])
simple("f64x2_splat", "f64x2.splat", ["f64"], ["v128"])
# lanes
for sh, lane, sc in [("i8x16", 15, "i32"), ("i16x8", 7, "i32")]:
    simple("%s_extract_s" % sh, "%s.extract_lane_s %d" % (sh, lane), ["v128"], [sc])
    simple("%s_extract_u" % sh, "%s.extract_lane_u %d" % (sh, lane), ["v128"], [sc])
    simple("%s_replace" % sh, "%s.replace_lane %d" % (sh, lane), ["v128", sc], ["v128"])
for sh, lane, sc in [("i32x4", 2, "i32"), ("i64x2", 1, "i64"),
                     ("f32x4", 2, "f32"), ("f64x2", 1, "f64")]:
    simple("%s_extract" % sh, "%s.extract_lane %d" % (sh, lane), ["v128"], [sc])
    simple("%s_replace" % sh, "%s.replace_lane %d" % (sh, lane), ["v128", sc], ["v128"])
# conversions
simple("i32x4_trunc_sat_f32x4_s", "i32x4.trunc_sat_f32x4_s", ["v128"], ["v128"])
simple("i32x4_trunc_sat_f32x4_u", "i32x4.trunc_sat_f32x4_u", ["v128"], ["v128"])
simple("f32x4_convert_i32x4_s", "f32x4.convert_i32x4_s", ["v128"], ["v128"])
simple("f32x4_convert_i32x4_u", "f32x4.convert_i32x4_u", ["v128"], ["v128"])
simple("i32x4_trunc_sat_f64x2_s_zero", "i32x4.trunc_sat_f64x2_s_zero", ["v128"], ["v128"])
simple("i32x4_trunc_sat_f64x2_u_zero", "i32x4.trunc_sat_f64x2_u_zero", ["v128"], ["v128"])
simple("f64x2_convert_low_i32x4_s", "f64x2.convert_low_i32x4_s", ["v128"], ["v128"])
simple("f64x2_convert_low_i32x4_u", "f64x2.convert_low_i32x4_u", ["v128"], ["v128"])
simple("f32x4_demote_f64x2_zero", "f32x4.demote_f64x2_zero", ["v128"], ["v128"])
simple("f64x2_promote_low_f32x4", "f64x2.promote_low_f32x4", ["v128"], ["v128"])

# --- memory ops ---
simple("m_load", "v128.load", ["i32"], ["v128"])
simple("m_load_off", "v128.load offset=8", ["i32"], ["v128"])
for o in ["load8x8_s", "load8x8_u", "load16x4_s", "load16x4_u", "load32x2_s",
          "load32x2_u", "load8_splat", "load16_splat", "load32_splat",
          "load64_splat", "load32_zero", "load64_zero"]:
    simple("m_" + o, "v128." + o, ["i32"], ["v128"])
op("m_store", ["i32", "v128"], [], "local.get 0 local.get 1 v128.store")
for o in ["load8_lane 3", "load16_lane 2", "load32_lane 1", "load64_lane 1"]:
    simple("m_" + o.split()[0], "v128." + o, ["i32", "v128"], ["v128"])
for o in ["store8_lane 3", "store16_lane 2", "store32_lane 1", "store64_lane 1"]:
    op("m_" + o.split()[0], ["i32", "v128"], [],
       "local.get 0 local.get 1 v128." + o)

FLOAT_RES = set()
for n in ["f32x4", "f64x2"]:
    for o in ["abs", "neg", "sqrt", "ceil", "floor", "trunc", "nearest",
              "add", "sub", "mul", "div", "min", "max", "pmin", "pmax",
              "demote_f64x2_zero", "promote_low_f32x4", "convert_low_i32x4_s",
              "convert_low_i32x4_u"]:
        FLOAT_RES.add("%s_%s" % (n, o))
FLOAT_RES.add("f32x4_convert_i32x4_s")
FLOAT_RES.add("f32x4_convert_i32x4_u")
for n in ["f32x4", "i32x4"]:
    pass

VALS = []
for _ in range(10):
    VALS.append(struct.pack("<16B", *[random.randrange(256) for _ in range(16)]))
VALS += [b"\x00" * 16, b"\xff" * 16,
         struct.pack("<4I", 0, 1, 0x7FFFFFFF, 0x80000000),
         struct.pack("<4f", 0.0, -0.0, float("inf"), float("nan")),
         struct.pack("<2d", 0.0, -0.0),
         struct.pack("<2d", float("inf"), float("nan")),
         struct.pack("<8h", 32767, -32768, 1, -1, 0, 255, -255, 1234),
         struct.pack("<16b", *[(-1) ** i * i for i in range(16)])]

SCAL32 = [0, 1, -1, 0x7FFFFFFF, -0x80000000, 31, 32, 33, 0xFF, 0x12345678]
SCAL64 = [0, 1, -1, 0x7FFFFFFFFFFFFFFF, -0x8000000000000000, 63, 64, 65]
F32B = [0, 0x80000000, 0x3F800000, 0xBF800000, 0x7F800000, 0xFF800000,
        0x7FC00000, 0x7F800001, 0x4F000000, 0xCF000000]
F64B = [0, 0x8000000000000000, 0x3FF0000000000000, 0xBFF0000000000000,
        0x7FF0000000000000, 0xFFF0000000000000, 0x7FF8000000000000,
        0x7FF0000000000001, 0x43E0000000000000, 0xC3E0000000000000]


def gen_module():
    lines = ["(module", '  (memory (export "mem") 2)']
    for name, params, results, body, _fl in OPS:
        ps = " ".join("(param %s)" % p for p in params)
        rs = " ".join("(result %s)" % r for r in results)
        lines.append("  (func (export \"%s\") %s %s %s)" % (name, ps, rs, body))
    lines.append(")")
    return "\n".join(lines) + "\n"


def nan4(b):
    v = struct.unpack("<I", b)[0]
    return (v & 0x7F800000) == 0x7F800000 and (v & 0x007FFFFF) != 0


def nan8(b):
    v = struct.unpack("<Q", b)[0]
    return (v & 0x7FF0000000000000) == 0x7FF0000000000000 and \
        (v & 0x000FFFFFFFFFFFFF) != 0


def norm_slot(name, slot):
    b = bytes.fromhex(slot)
    if name in FLOAT_RES:
        out = bytearray(b)
        if name.startswith("f64x2") or name in ("f32x4_demote_f64x2_zero",):
            for i in (0, 8):
                if nan8(b[i:i+8]):
                    out[i:i+8] = b"\x01\x00\x00\x00\x00\x00\xf8\x7f"
        else:
            for i in (0, 4, 8, 12):
                if nan4(b[i:i+4]):
                    out[i:i+4] = b"\x01\x00\xc0\x7f"
        return bytes(out).hex()
    return slot


def build_commands():
    rnd = random.Random(999)
    cmds = ["load a MODPATH", "init a"]
    data = bytes([(i * 7 + 3) & 0xFF for i in range(64)])
    cmds.append("memwrite a 6d656d 0 " + data.hex())
    for name, params, results, body, _fl in OPS:
        if name.startswith("m_"):
            if name in ("m_store", "m_store8_lane", "m_store16_lane",
                        "m_store32_lane", "m_store64_lane"):
                continue
            for addr in [0, 1, 17, 65530, 131060, 131070]:
                vs = rnd.choice(VALS)
                args = [s32(addr)] + ([v128b(vs)] if len(params) > 1 else [])
                cmds.append("invoke a %s %d %s" % (
                    h(name), len(results), " ".join(args)))
            continue
        for _ in range(14):
            args = []
            for p in params:
                if p == "v128":
                    args.append(v128b(rnd.choice(VALS)))
                elif p == "i32":
                    args.append(s32(rnd.choice(SCAL32)))
                elif p == "i64":
                    args.append(s64(rnd.choice(SCAL64)))
                elif p == "f32":
                    args.append(s32(rnd.choice(F32B)))
                else:
                    args.append(s64(rnd.choice(F64B)))
            cmds.append("invoke a %s %d %s" % (
                h(name), len(results), " ".join(args)))
    # integer lane stores, then hash
    for addr in [0, 3, 17, 131060]:
        cmds.append("invoke a %s 0 %s" % (
            h("m_store"), s32(addr) + " " + v128b(b"\x11\x22\x33\x44" * 4)))
    cmds.append("memhash a 6d656d")
    # float stores last (may write different NaN payloads)
    for addr in [32, 64]:
        cmds.append("invoke a %s 0 %s" % (
            h("m_store"), s32(addr) + " " + v128b(b"\x01\x00\xc0\x7f" * 4)))
    for o in ["m_store8_lane", "m_store16_lane", "m_store32_lane", "m_store64_lane"]:
        for addr in [40, 41]:
            cmds.append("invoke a %s 0 %s" % (h(o), s32(addr) + " " + v128b(b"\x7f\x80\x01\x02" * 4)))
    cmds.append("memhash a 6d656d")
    cmds.append("quit")
    return cmds


def main():
    wat = gen_module()
    wp = os.path.join(TMP, "simd_all.wat")
    open(wp, "w").write(wat)
    wasm = os.path.join(TMP, "simd_all.wasm")
    r = subprocess.run(["wasm-tools", "parse", wp, "-o", wasm], capture_output=True, text=True)
    if r.returncode != 0:
        print("parse failed:", r.stderr[:2000])
        return 1
    cmds = build_commands()
    names = []
    for c in cmds:
        if c.startswith("invoke a "):
            names.append(bytes.fromhex(c.split()[2]).decode())
        else:
            names.append(None)
    ref = cmp.ref_run(wasm, cmds)
    refn = []
    for c, line, nm in zip(cmds, ref, names):
        if nm is None or not line.startswith("ok "):
            refn.append(line)
            continue
        parts = line.split()
        vals = [norm_slot(nm, sl) for sl in parts[1:]]
        refn.append(" ".join(["ok"] + vals))
    ok = True
    for opt in ("-O0", "-O2"):
        so = cmp.build(wasm, opt, TMP)
        ours = cmp.our_run(so, cmds)
        bad = 0
        for i in range(min(len(refn), len(ours))):
            nm = names[i]
            if nm is not None and ours[i].startswith("ok ") and refn[i].startswith("ok "):
                parts = ours[i].split()
                o = " ".join(["ok"] + [norm_slot(nm, sl) for sl in parts[1:]])
            else:
                o = ours[i]
            if refn[i] != o:
                bad += 1
                if bad <= 10:
                    print("MISMATCH %s line %d %s\n  ref =%s\n  our =%s" %
                          (opt, i + 1, cmds[i][:120], refn[i][:200], o[:200]))
        if len(refn) != len(ours):
            print("length mismatch", len(refn), len(ours))
            bad += 1
        print(("ok %s (%d checks)" % (opt, len(cmds))) if not bad else ("FAIL %s" % opt))
        if bad:
            ok = False
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())

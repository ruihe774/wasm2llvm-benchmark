#!/usr/bin/env python3
"""Numeric semantics differential tests against w2l-ref."""
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


F32 = [0x00000000, 0x80000000, 0x3f800000, 0xbf800000, 0x3fc00000, 0x40200000,
       0x40600000, 0x40400000, 0x4f000000, 0x4f000001, 0xcf000000, 0xcf000001,
       0x4f800000, 0x5f000000, 0xdf000000, 0x5f800000, 0x7f800000, 0xff800000,
       0x7fc00000, 0x7f800001, 0xffc00000, 0x00000001, 0x00800000, 0x3f7fffff,
       0xbf7fffff, 0x4b000000, 0x4b7fffff, 0x3f000000]

F64 = [0x0000000000000000, 0x8000000000000000, 0x3ff0000000000000,
       0xbff0000000000000, 0x3ff8000000000000, 0x4004000000000000,
       0x41e0000000000000, 0x41e0000000200000, 0xc1e0000000000000,
       0xc1e0000020000000, 0x41f0000000000000, 0x43e0000000000000,
       0xc3e0000000000000, 0x43f0000000000000, 0x7ff0000000000000,
       0xfff0000000000000, 0x7ff8000000000000, 0x7ff0000000000001,
       0xfff8000000000000, 0x0000000000000001, 0x0010000000000000,
       0x3fefffffffffffff, 0x41dfffffffc00000, 0x41dfffffffffffff,
       0x4008000000000000, 0x4330000000000000]

INT32 = [0, 1, -1, 2, 31, 32, 33, 63, 64, 0x7FFFFFFF, -0x80000000, 0x55555555,
         0x12345678]
INT64 = [0, 1, -1, 2, 63, 64, 65, 127, 128, 0x7FFFFFFFFFFFFFFF,
         -0x8000000000000000, 0x5555555555555555, 0x123456789ABCDEF0]


def nan32(bits):
    return (bits & 0x7F800000) == 0x7F800000 and (bits & 0x007FFFFF) != 0


def nan64(bits):
    return (bits & 0x7FF0000000000000) == 0x7FF0000000000000 and \
        (bits & 0x000FFFFFFFFFFFFF) != 0


# name -> (param kinds list, result kinds list)
CASES = {}


def add(name, params, results):
    CASES[name] = (params, results)


for op in ["add", "sub", "mul", "div", "min", "max", "copysign"]:
    add("f32_" + op, ["f32", "f32"], ["f32"])
    add("f64_" + op, ["f64", "f64"], ["f64"])
for op in ["eq", "ne", "lt", "ge"]:
    add("f32_" + op, ["f32", "f32"], ["i32"])
add("f64_eq", ["f64", "f64"], ["i32"])
for op in ["abs", "neg", "ceil", "floor", "trunc", "nearest", "sqrt"]:
    add("f32_" + op, ["f32"], ["f32"])
    add("f64_" + op, ["f64"], ["f64"])
add("i32_reinterpret_f32", ["f32"], ["i32"])
add("f32_reinterpret_i32", ["i32"], ["f32"])
for conv, arg in [("i32_trunc_f32_s", "f32"), ("i32_trunc_f32_u", "f32"),
                  ("i32_trunc_f64_s", "f64"), ("i32_trunc_f64_u", "f64"),
                  ("i64_trunc_f32_s", "f32"), ("i64_trunc_f32_u", "f32"),
                  ("i64_trunc_f64_s", "f64"), ("i64_trunc_f64_u", "f64")]:
    add(conv, [arg], ["i64" if conv.startswith("i64") else "i32"])
for conv, arg in [("i32_trunc_sat_f32_s", "f32"), ("i32_trunc_sat_f32_u", "f32"),
                  ("i32_trunc_sat_f64_s", "f64"), ("i32_trunc_sat_f64_u", "f64"),
                  ("i64_trunc_sat_f32_s", "f32"), ("i64_trunc_sat_f32_u", "f32"),
                  ("i64_trunc_sat_f64_s", "f64"), ("i64_trunc_sat_f64_u", "f64")]:
    add(conv, [arg], ["i64" if conv.startswith("i64") else "i32"])
for name, arg, res in [
    ("f32_convert_i32_s", "i32", "f32"), ("f32_convert_i32_u", "i32", "f32"),
    ("f32_convert_i64_s", "i64", "f32"), ("f32_convert_i64_u", "i64", "f32"),
    ("f64_convert_i32_s", "i32", "f64"), ("f64_convert_i32_u", "i32", "f64"),
    ("f64_convert_i64_s", "i64", "f64"), ("f64_convert_i64_u", "i64", "f64"),
    ("f32_demote_f64", "f64", "f32"), ("f64_promote_f32", "f32", "f64"),
]:
    add(name, [arg], [res])
for op in ["clz", "ctz", "popcnt", "extend8_s", "extend16_s"]:
    add("i32_" + op, ["i32"], ["i32"])
add("i64_extend8_s", ["i64"], ["i64"])
add("i64_extend16_s", ["i64"], ["i64"])
add("i64_extend32_s", ["i64"], ["i64"])
for op in ["clz", "ctz", "popcnt"]:
    add("i64_" + op, ["i64"], ["i64"])
for op in ["rotl", "rotr", "shl", "shr_s", "shr_u", "div_s", "div_u", "rem_s", "rem_u"]:
    add("i32_" + op, ["i32", "i32"], ["i32"])
for op in ["rotl", "rotr", "shl", "shr_s", "shr_u", "div_s", "div_u", "rem_s", "rem_u"]:
    add("i64_" + op, ["i64", "i64"], ["i64"])


def slot(kind, value):
    if kind in ("i32", "f32"):
        return s32(value)
    return s64(value)


def build_commands():
    rnd = random.Random(12345)
    cmds = ["load a MODPATH", "init a"]
    for name, (params, results) in CASES.items():
        vals = []
        for p in params:
            if p == "f32":
                vals.append(rnd.choice(F32))
            elif p == "f64":
                vals.append(rnd.choice(F64))
            elif p == "i32":
                vals.append(rnd.choice(INT32))
            else:
                vals.append(rnd.choice(INT64))
        # Random extra patterns
        for _ in range(24):
            args = []
            for p in params:
                if p == "f32":
                    args.append(rnd.choice(F32) if rnd.random() < 0.7 else rnd.getrandbits(32))
                elif p == "f64":
                    args.append(rnd.choice(F64) if rnd.random() < 0.7 else rnd.getrandbits(64))
                elif p == "i32":
                    args.append(rnd.choice(INT32) if rnd.random() < 0.7 else rnd.getrandbits(32))
                else:
                    args.append(rnd.choice(INT64) if rnd.random() < 0.7 else rnd.getrandbits(64))
            cmds.append("invoke a %s %d %s" % (
                h(name), len(results), " ".join(slot(p, v) for p, v in zip(params, args))))
    cmds.append("quit")
    return cmds


F32NAN = "NAN" * 8


def norm(line, results):
    if not line.startswith("ok "):
        return line
    parts = line.split()
    if len(parts) - 1 != len(results):
        return line
    out = ["ok"]
    for r, slotstr in zip(results, parts[1:]):
        if r == "f32" and nan32(int.from_bytes(bytes.fromhex(slotstr[0:8]), "little")):
            slotstr = F32NAN
        elif r == "f64" and nan64(int.from_bytes(bytes.fromhex(slotstr[0:16]), "little")):
            slotstr = F32NAN
        out.append(slotstr)
    return " ".join(out)


def main():
    wat = os.path.join(ROOT, "tests", "wat", "float.wat")
    wasm = os.path.join(TMP, "float.wasm")
    r = subprocess.run(["wasm-tools", "parse", wat, "-o", wasm], capture_output=True, text=True)
    if r.returncode != 0:
        print("parse failed:", r.stderr)
        return 1
    cmds = build_commands()
    rt = []
    for c in cmds:
        if c.startswith("invoke a "):
            nm = bytes.fromhex(c.split()[2]).decode()
            rt.append(CASES[nm][1])
        else:
            rt.append([])
    ref = cmp.ref_run(wasm, cmds)
    ref = [norm(a, t) for a, t in zip(ref, rt)]
    ok = True
    for opt in ("-O0", "-O2"):
        so = cmp.build(wasm, opt, TMP)
        ours = cmp.our_run(so, cmds)
        ours = [norm(a, t) for a, t in zip(ours, rt)]
        bad = 0
        for i in range(min(len(ref), len(ours))):
            if ref[i] != ours[i]:
                bad += 1
                if bad <= 8:
                    print("MISMATCH %s line %d: %s\n  ref =%s\n  our =%s" %
                          (opt, i + 1, cmds[i][:110], ref[i], ours[i]))
        if len(ref) != len(ours):
            print("length mismatch", len(ref), len(ours))
            bad += 1
        if bad:
            ok = False
        else:
            print("ok %s (%d checks)" % (opt, len(cmds)))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())

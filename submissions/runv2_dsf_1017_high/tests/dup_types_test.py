#!/usr/bin/env python3
"""call_indirect with structurally-equal duplicate types, multi-value indirect
calls, and globals of every type."""
import importlib.util
import os
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


def uleb(n):
    out = bytearray()
    while True:
        b = n & 0x7F
        n >>= 7
        if n:
            out.append(b | 0x80)
        else:
            out.append(b)
            return bytes(out)


def sec(id, payload):
    return bytes([id]) + uleb(len(payload)) + payload


def vec(items):
    return uleb(len(items)) + b"".join(items)


def name(s):
    b = s.encode()
    return uleb(len(b)) + b


FUNC = b"\x60"
I32, I64 = b"\x7f", b"\x7e"


def build_dup_module():
    # types
    t0 = FUNC + uleb(1) + I32 + uleb(1) + I32          # (i32)->i32
    t1 = t0                                             # duplicate
    t2 = FUNC + uleb(2) + I32 + I32 + uleb(1) + I32     # (i32,i32)->i32
    t3 = FUNC + uleb(1) + I32 + uleb(2) + I32 + I64     # (i32)->(i32,i64)
    t4 = FUNC + uleb(2) + I32 + I32 + uleb(2) + I32 + I64
    types = sec(1, vec([t0, t1, t2, t3, t4]))
    funcs = sec(3, vec([uleb(0), uleb(3), uleb(2), uleb(4)]))
    table = sec(4, vec([b"\x70\x00" + uleb(4)]))
    exports = sec(7, vec([
        name("ci") + b"\x00" + uleb(2),
        name("cim") + b"\x00" + uleb(3),
    ]))
    # elem active at 0: [0,1]
    elem_payload = uleb(1) + uleb(0) + b"\x41\x00\x0b" + vec([uleb(0), uleb(1)])
    elems = sec(9, elem_payload)
    bodies = []
    # f0: (i32)->i32 : local.get 0 ; i32.const 1 ; i32.add ; end
    b0 = uleb(0) + b"\x20\x00\x41\x01\x6a\x0b"
    # f1: (i32)->(i32,i64): local.get 0 ; i64.const 9 ; end
    b1 = uleb(0) + b"\x20\x00\x42\x09\x0b"
    # f2: (i32,i32)->i32: local.get 0 local.get 1 call_indirect type1 table0 end
    b2 = uleb(0) + b"\x20\x00\x20\x01\x11" + uleb(1) + uleb(0) + b"\x0b"
    # f3: (i32,i32)->(i32,i64): local.get 0 local.get 1 call_indirect type3 end
    b3 = uleb(0) + b"\x20\x00\x20\x01\x11" + uleb(3) + uleb(0) + b"\x0b"
    for b in (b0, b1, b2, b3):
        bodies.append(uleb(len(b)) + b)
    code = sec(10, vec(bodies))
    return b"\0asm\x01\0\0\0" + types + funcs + table + exports + elems + code


GLOBALS_WAT = """(module
  (global (export "gi") i32 (i32.const -5))
  (global (export "gl") i64 (i64.const -7))
  (global (export "gf") f32 (f32.const 1.5))
  (global (export "gd") f64 (f64.const -2.25))
  (global (export "gv") v128 (v128.const i32x4 1 2 3 4))
  (global $m (export "gm") (mut i32) (i32.const 0))
  (global $n (export "gn") (mut i64) (i64.const 0))
  (global $f (export "gfm") (mut f32) (f32.const 0))
  (global $d (export "gdm") (mut f64) (f64.const 0))
  (global $v (export "gvm") (mut v128) (v128.const i32x4 0 0 0 0))
  (func (export "setm") (param i32) (global.set $m (local.get 0)))
  (func (export "setn") (param i64) (global.set $n (local.get 0)))
  (func (export "setf") (param f32) (global.set $f (local.get 0)))
  (func (export "setd") (param f64) (global.set $d (local.get 0)))
  (func (export "setv") (param v128) (global.set $v (local.get 0)))
)"""


def main():
    ok = True
    bp = os.path.join(TMP, "dup_types.wasm")
    open(bp, "wb").write(build_dup_module())
    c = ["load a MODPATH", "init a",
         "invoke a %s 1 %s" % (h("ci"), s32(5) + " " + s32(0)),
         "invoke a %s 1 %s" % (h("ci"), s32(5) + " " + s32(1)),
         "invoke a %s 1 %s" % (h("ci"), s32(5) + " " + s32(2)),
         "invoke a %s 1 %s" % (h("ci"), s32(5) + " " + s32(4)),
         "invoke a %s 2 %s" % (h("cim"), s32(5) + " " + s32(1)),
         "invoke a %s 2 %s" % (h("cim"), s32(5) + " " + s32(0)),
         "quit"]
    ok &= cmp.compare(bp, c, verbose=True)

    gw = os.path.join(TMP, "globals.wat")
    open(gw, "w").write(GLOBALS_WAT + "\n")
    gb = os.path.join(TMP, "globals.wasm")
    r = subprocess.run(["wasm-tools", "parse", gw, "-o", gb], capture_output=True, text=True)
    if r.returncode != 0:
        print("parse failed:", r.stderr)
        return 1
    vv = bytes(range(16)).hex()
    c = ["load a MODPATH", "init a"]
    for g in ["gi", "gl", "gf", "gd", "gv", "gm", "gn", "gfm", "gdm", "gvm"]:
        c.append("global a %s" % h(g))
    c += ["invoke a %s 0 %s" % (h("setm"), s32(-123)),
          "invoke a %s 0 %s" % (h("setn"), s64(-456)),
          "invoke a %s 0 %s" % (h("setf"), s32(0x40490FDB)),
          "invoke a %s 0 %s" % (h("setd"), s64(0x400921FB54442D18)),
          "invoke a %s 0 %s" % (h("setv"), vv),
          "global a %s" % h("gm"), "global a %s" % h("gn"),
          "global a %s" % h("gfm"), "global a %s" % h("gdm"),
          "global a %s" % h("gvm"),
          "quit"]
    ok &= cmp.compare(gb, c, verbose=True)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Tests for imported functions (direct, exported, indirect) using expected
values, since w2l-ref only provides the standard host imports."""
import os
import struct
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TMP = os.environ.get("TMPDIR", tempfile.gettempdir())
HARNESS = os.path.join(ROOT, "tests", ".harness")
W2L = os.path.join(ROOT, "target", "release", "wasm2llvm")


def h(s):
    return s.encode().hex()


def s32(v):
    return struct.pack("<I", v & 0xFFFFFFFF).hex() + "00" * 12


def s64(v):
    return struct.pack("<Q", v & 0xFFFFFFFFFFFFFFFF).hex() + "00" * 8


def v128(b):
    return b.hex()


def run(so, cmds):
    r = subprocess.run(HARNESS, input="\n".join(c.replace("MODPATH", so) for c in cmds) + "\n",
                       capture_output=True, text=True)
    return r.stdout.splitlines()


def main():
    subprocess.run(["clang", "-O1", "-rdynamic", "-o", HARNESS,
                    os.path.join(ROOT, "tests", "harness.c"), "-ldl"], check=True)
    wasm = os.path.join(TMP, "imports.wasm")
    r = subprocess.run(["wasm-tools", "parse", os.path.join(ROOT, "tests", "wat", "imports.wat"),
                        "-o", wasm], capture_output=True, text=True)
    if r.returncode != 0:
        print("parse failed:", r.stderr)
        return 1
    ok = True
    for opt in ("-O0", "-O2"):
        ll = os.path.join(TMP, "imports.ll")
        subprocess.run([W2L, wasm, "-o", ll], check=True)
        so = os.path.join(TMP, "imports%s.so" % opt)
        r = subprocess.run(["clang", opt, "-shared", "-fPIC", "-Wl,-Bsymbolic", "-o", so,
                            ll, "-lm"], capture_output=True, text=True)
        if r.returncode != 0:
            print("compile failed:", r.stderr[:1000])
            return 1
        v1 = bytes(range(16))
        v2 = bytes([0xFF - i for i in range(16)])
        a32 = 0xFFFFFFFB
        a64 = 0xFFFFFFFFFFFFFFFB
        f32 = 0x3FC00000
        f64 = 0xC002000000000000
        cmds = ["load a MODPATH", "init a",
                "invoke a %s 5 %s %s %s %s %s" % (
                    h("call_echo"), s32(a32), s64(a64), s32(f32), s64(f64), v128(v1)),
                "invoke a %s 1 %s" % (h("call_sum"), s32(3) + " " + s32(4)),
                "invoke a %s 1 %s" % (h("call_sum"), s32(0x7FFFFFFF) + " " + s32(1)),
                "invoke a %s 1 %s" % (h("call_vadd"), v128(v1) + " " + v128(v2)),
                "invoke a %s 2" % h("call_rm"),
                "invoke a %s 1 %s" % (h("imported_sum"), s32(2) + " " + s32(3)),
                "invoke a %s 1 %s" % (h("indirect_sum"), s32(2) + " " + s32(3) + " " + s32(0)),
                "invoke a %s 1 %s" % (h("indirect_sum"), s32(2) + " " + s32(3) + " " + s32(1)),
                "invoke a %s 1 %s" % (h("indirect_sum"), s32(2) + " " + s32(3) + " " + s32(2)),
                "quit"]
        got = run(so, cmds)
        xorv = bytes(a ^ b for a, b in zip(v1, v2))
        expect = [
            "ok",
            "ok",
            "ok %s %s %s %s %s" % (s32(a32), s64(a64), s32(f32), s64(f64), v128(v1)),
            "ok " + s32(7),
            "ok " + s32(-0x80000000),
            "ok " + v128(xorv),
            "ok " + s32(7) + " " + s64(8),
            "ok " + s32(5),
            "ok " + s32(5),
            "trap 6",
            "trap 5",
        ]
        if got != expect:
            ok = False
            print("MISMATCH (%s)" % opt)
            for i in range(max(len(got), len(expect))):
                g = got[i] if i < len(got) else "<missing>"
                e = expect[i] if i < len(expect) else "<missing>"
                if g != e:
                    print("  line %d:\n    want %s\n    got  %s" % (i + 1, e, g))
        else:
            print("ok %s" % opt)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())

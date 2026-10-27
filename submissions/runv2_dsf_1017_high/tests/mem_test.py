#!/usr/bin/env python3
"""Memory growth / zero-page / init-trap tests."""
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


MEM = h("memory")

MEM_WAT = """(module
  (memory (export "memory") 0 4)
  (func (export "grow") (param i32) (result i32) (memory.grow (local.get 0)))
  (func (export "size") (result i32) (memory.size))
  (func (export "ld") (param i32) (result i32) (i32.load (local.get 0)))
  (func (export "st") (param i32 i32) (i32.store (local.get 0) (local.get 1)))
)"""

BIG_WAT = """(module
  (memory (export "memory") 1 300)
  (func (export "grow") (param i32) (result i32) (memory.grow (local.get 0)))
  (func (export "size") (result i32) (memory.size))
)"""

OOB_DATA_WAT = """(module
  (memory 0)
  (data (i32.const 0) "abc")
)"""

OOB_DATA2_WAT = """(module
  (memory 1)
  (data (i32.const 65535) "abc")
)"""

OOB_ELEM_WAT = """(module
  (table 1 funcref)
  (func $f)
  (elem (i32.const 1) $f)
)"""

OK_EMPTY_DATA_WAT = """(module
  (memory 0)
  (data (i32.const 0) "")
  (func (export "size") (result i32) (memory.size))
)"""


def parse(name, wat):
    wp = os.path.join(TMP, name + ".wat")
    bp = os.path.join(TMP, name + ".wasm")
    open(wp, "w").write(wat + "\n")
    r = subprocess.run(["wasm-tools", "parse", wp, "-o", bp], capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError(r.stderr)
    return bp


def main():
    ok = True
    wasm = parse("mem_edge", MEM_WAT)
    c = ["load a MODPATH", "init a",
         "invoke a %s 1" % h("size"),
         "invoke a %s 1 %s" % (h("grow"), s32(0)),
         "invoke a %s 1 %s" % (h("ld"), s32(0)),
         "invoke a %s 1 %s" % (h("grow"), s32(1)),
         "invoke a %s 1" % h("size"),
         "invoke a %s 0 %s" % (h("st"), s32(0) + " " + s32(0x11223344)),
         "invoke a %s 1 %s" % (h("grow"), s32(1)),
         "invoke a %s 1 %s" % (h("ld"), s32(0)),
         "invoke a %s 0 %s" % (h("st"), s32(65532) + " " + s32(0x55667788)),
         "invoke a %s 1 %s" % (h("grow"), s32(2)),
         "invoke a %s 1" % h("size"),
         "invoke a %s 1 %s" % (h("ld"), s32(65532)),
         "invoke a %s 1 %s" % (h("grow"), s32(1)),
         "invoke a %s 1 %s" % (h("grow"), s32(0)),
         "invoke a %s 1 %s" % (h("grow"), s32(-1)),
         "invoke a %s 1" % h("size"),
         "memhash a %s" % MEM,
         "quit"]
    ok &= cmp.compare(wasm, c, verbose=True)

    wasm = parse("mem_big", BIG_WAT)
    c = ["load a MODPATH", "init a",
         "invoke a %s 1 %s" % (h("grow"), s32(299)),
         "invoke a %s 1" % h("size"),
         "invoke a %s 1 %s" % (h("grow"), s32(1)),
         "invoke a %s 1 %s" % (h("grow"), s32(0)),
         "quit"]
    ok &= cmp.compare(wasm, c, verbose=True)

    for name, wat, expected in [
        ("oob_data", OOB_DATA_WAT, "trap 4"),
        ("oob_data2", OOB_DATA2_WAT, "trap 4"),
        ("oob_elem", OOB_ELEM_WAT, "trap 5"),
        ("ok_empty_data", OK_EMPTY_DATA_WAT, "ok"),
    ]:
        wasm = parse(name, wat)
        c = ["load a MODPATH", "init a", "quit"]
        # compare: our harness must match the reference trap/ok
        ok &= cmp.compare(wasm, c, verbose=True)
        ref = cmp.ref_run(wasm, c)
        if not ref[1].startswith(expected):
            print("unexpected reference reply for %s: %s" % (name, ref[1]))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())

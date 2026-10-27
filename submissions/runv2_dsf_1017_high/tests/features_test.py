#!/usr/bin/env python3
"""Targeted feature tests (tables, bulk memory, control flow, memory limits)."""
import importlib.util
import os
import struct
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "tests"))
spec = importlib.util.spec_from_file_location("cmp", os.path.join(ROOT, "tests", "compare.py"))
cmp = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cmp)

TMP = os.environ.get("TMPDIR", "/tmp")


def h(s):
    return s.encode().hex()


def i32(v):
    return struct.pack("<i", v).hex() + "00" * 12


def i64(v):
    return struct.pack("<q", v).hex() + "00" * 8


def v128(*lanes):
    return struct.pack("<4I", *lanes).hex()


MEM = h("memory")


def main():
    wat = os.path.join(ROOT, "tests", "wat", "features.wat")
    wasm = os.path.join(TMP, "features.wasm")
    r = subprocess.run(["wasm-tools", "parse", wat, "-o", wasm], capture_output=True, text=True)
    if r.returncode != 0:
        print("parse failed:", r.stderr)
        return 1
    c = ["load a MODPATH", "init a"]
    c.append("global a " + h("counter"))
    c.append("global a " + h("vg"))
    c.append("memread a %s 100 5" % MEM)
    # initial table
    for i in [0, 1, 2, 8, 100]:
        c.append("invoke a %s 1 %s" % (h("call_ind"), i32(i)))
    c.append("invoke a %s 1 %s" % (h("call_ind_ty"), i32(0)))
    # passive elem init and drop
    c.append("invoke a %s 0 %s" % (h("tinit"), " ".join(i32(x) for x in (2, 0, 2))))
    for i in [2, 3]:
        c.append("invoke a %s 1 %s" % (h("call_ind"), i32(i)))
    c.append("invoke a %s 0 %s" % (h("tinit"), " ".join(i32(x) for x in (7, 0, 2))))
    c.append("invoke a %s 0 %s" % (h("tinit"), " ".join(i32(x) for x in (0, 1, 2))))
    c.append("invoke a %s 0 %s" % (h("tcopy"), " ".join(i32(x) for x in (4, 2, 2))))
    c.append("invoke a %s 1 %s" % (h("call_ind"), i32(4)))
    c.append("invoke a %s 0 %s" % (h("tcopy"), " ".join(i32(x) for x in (7, 0, 2))))
    c.append("invoke a %s 0" % h("edrop"))
    c.append("invoke a %s 0 %s" % (h("tinit"), " ".join(i32(x) for x in (0, 0, 1))))
    c.append("memhash a %s" % MEM)
    # data segments
    c.append("invoke a %s 0 %s" % (h("minit"), " ".join(i32(x) for x in (0, 0, 5))))
    c.append("memread a %s 0 5" % MEM)
    c.append("invoke a %s 0 %s" % (h("minit"), " ".join(i32(x) for x in (10, 5, 0))))
    c.append("invoke a %s 0 %s" % (h("minit"), " ".join(i32(x) for x in (0, 5, 1))))
    c.append("invoke a %s 0 %s" % (h("minit"), " ".join(i32(x) for x in (65536, 0, 1))))
    c.append("invoke a %s 0 %s" % (h("mcopy"), " ".join(i32(x) for x in (20, 0, 5))))
    c.append("memread a %s 20 5" % MEM)
    c.append("invoke a %s 0 %s" % (h("mfill"), " ".join(i32(x) for x in (30, 66, 4))))
    c.append("memread a %s 30 4" % MEM)
    c.append("invoke a %s 0 %s" % (h("mcopy"), " ".join(i32(x) for x in (65535, 0, 2))))
    c.append("invoke a %s 0 %s" % (h("mfill"), " ".join(i32(x) for x in (65535, 1, 2))))
    c.append("invoke a %s 0" % h("ddrop"))
    c.append("invoke a %s 0 %s" % (h("minit"), " ".join(i32(x) for x in (0, 0, 5))))
    # memory limits (1 page now)
    c.append("invoke a %s 1" % h("msize"))
    for addr in [65532, 65533, 65535, 65536, 0]:
        c.append("invoke a %s 1 %s" % (h("ld"), i32(addr)))
    c.append("invoke a %s 1 %s" % (h("ld_off"), i32(0)))
    c.append("invoke a %s 1 %s" % (h("ld16_off"), i32(0)))
    c.append("invoke a %s 0 %s" % (h("st"), i32(65532) + " " + i32(123456)))
    c.append("invoke a %s 0 %s" % (h("st"), i32(65533) + " " + i32(123456)))
    c.append("invoke a %s 0 %s" % (h("st8"), i32(65535) + " " + i32(0x1234)))
    c.append("invoke a %s 1 %s" % (h("ld64"), i32(65528)))
    c.append("invoke a %s 1 %s" % (h("ld64"), i32(65529)))
    c.append("invoke a %s 0 %s" % (h("i64st32"), i32(65532) + " " + i64(0x1122334455667788)))
    c.append("invoke a %s 0 %s" % (h("i64st32"), i32(65533) + " " + i64(7)))
    c.append("memhash a %s" % MEM)
    # grow
    c.append("invoke a %s 1 %s" % (h("grow"), i32(1)))
    c.append("invoke a %s 1" % h("msize"))
    c.append("invoke a %s 1 %s" % (h("grow"), i32(1)))
    c.append("invoke a %s 1 %s" % (h("grow"), i32(0)))
    c.append("invoke a %s 1 %s" % (h("grow"), i32(-1)))
    c.append("invoke a %s 1" % h("msize"))
    c.append("invoke a %s 1 %s" % (h("ld"), i32(65533)))
    c.append("memread a %s 65536 8" % MEM)
    # control flow
    c.append("invoke a %s 0" % h("unreach"))
    for n in [0, 1, 2, 5, 10]:
        c.append("invoke a %s 1 %s" % (h("loopparam"), i32(n)))
    for n in [0, 1, 2, 5, 0x7FFFFFFF]:
        c.append("invoke a %s 1 %s" % (h("brtable"), i32(n)))
    c.append("invoke a %s 2 %s" % (h("blockmulti"), i32(3) + " " + i32(4)))
    c.append("invoke a %s 1 %s" % (h("ifparam"), i32(7) + " " + i32(0)))
    c.append("invoke a %s 1 %s" % (h("ifparam"), i32(7) + " " + i32(1)))
    c.append("invoke a %s 1 %s" % (h("brifvals"), i32(5) + " " + i32(0)))
    c.append("invoke a %s 1 %s" % (h("brifvals"), i32(5) + " " + i32(1)))
    c.append("invoke a %s 1 %s" % (h("sel"), " ".join(i32(x) for x in (1, 2, 0))))
    c.append("invoke a %s 1 %s" % (h("sel"), " ".join(i32(x) for x in (1, 2, 1))))
    c.append("invoke a %s 1 %s" % (h("selv"), " ".join([v128(1, 2, 3, 4), v128(5, 6, 7, 8), i32(0)])))
    c.append("invoke a %s 1 %s" % (h("selv"), " ".join([v128(1, 2, 3, 4), v128(5, 6, 7, 8), i32(1)])))
    c.append("invoke a %s 0 %s" % (h("setcount"), i32(123)))
    c.append("invoke a %s 1" % h("getcount"))
    c.append("quit")
    ok = cmp.compare(wasm, c, verbose=True)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())

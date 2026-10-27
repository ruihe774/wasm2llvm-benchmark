#!/usr/bin/env python3
"""Miscellaneous corner cases: odd export names, wide signatures, br_table
shapes, many locals, nesting."""
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


def v128(*lanes):
    return struct.pack("<4I", *lanes).hex()


def parse(name, wat):
    wp = os.path.join(TMP, name + ".wat")
    bp = os.path.join(TMP, name + ".wasm")
    open(wp, "w").write(wat + "\n")
    r = subprocess.run(["wasm-tools", "parse", wp, "-o", bp], capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError(r.stderr)
    return bp


WIDE_WAT = """(module
  (func (export "wide") (param %s) (result i32 i64 i32 i32 i32)
    local.get 0
    i64.const 3
    local.get 1
    local.get 2
    i32.const 5)
)""" % " ".join(["i32"] * 25)

MULTIV_WAT = """(module
  (func (export "mv") (param v128 i32) (result v128 v128 i32 i64 f64)
    local.get 0 local.get 0 local.get 1 i64.const -9 f64.const 2.5)
  (func (export "mv2") (result i32 i32 i32)
    i32.const 1 i32.const 2 i32.const 3)
)"""

SEL_WAT = """(module
  (func (export "self32") (param f32 f32 i32) (result f32)
    local.get 0 local.get 1 local.get 2 select)
  (func (export "self64") (param f64 f64 i32) (result f64)
    local.get 0 local.get 1 local.get 2 select)
)"""

NEST_WAT = "(module (func (export \"nest\") (result i32) " + \
    "block (result i32) " * 40 + "i32.const 7 " + "end " * 40 + "))"

MANY_LOCALS = "(module (func (export \"ml\") (result i32) (local " + \
    " ".join(["i64"] * 3000) + ") local.get 2999 i32.wrap_i64))"

BRTABLE0_WAT = """(module
  (func (export "bt0") (param i32) (result i32)
    block (result i32)
      i32.const 5
      local.get 0
      br_table 0
    end)
)"""

BIGNAME_WAT = r"""(module
  (func $f (result i32) i32.const 42)
  (func $g (result i32) i32.const 43)
  (export "" (func $f))
  (export "a\"b\\c" (func $g))
  (export "é中文" (func $f))
)"""


def make_bigtable():
    n = 100
    lines = ["(module (func (export \"bt\") (param i32) (result i32)",
             "  block $top (result i32)"]
    for i in range(n - 1, -1, -1):
        lines.append("    block $l%d (result i32)" % i)
    lines.append("      i32.const 1000")
    lines.append("      local.get 0")
    lines.append("      br_table " + " ".join("$l%d" % i for i in range(n)) + " $top")
    for i in range(n):
        lines.append("    end")
        lines.append("    i32.const %d" % i)
        lines.append("    i32.add")
    lines.append("  end")
    lines.append("  ))")
    return "\n".join(lines)



BRFN_WAT = """(module
  (func (export "brfn") (param i32) (result i32)
    block $b (result i32)
      local.get 0
      br 1
    end)
  (func (export "brtabfn") (param i32) (result i32)
    local.get 0
    local.get 0
    br_table 0 0)
  (func (export "retmid") (param i32) (result i32)
    local.get 0
    if
      i32.const 55
      return
    end
    i32.const 66)
  (func (export "dead2") (result i32)
    block (result i32)
      i32.const 5
      br 0
      unreachable
    end)
  (func (export "dead3") (param i32) (result i32)
    block $a (result i32)
      local.get 0
      if (result i32)
        i32.const 7
        br 1
      else
        i32.const 3
      end
    end)
  (func (export "dead4") (param i32) (result i32)
    local.get 0
    if (result i32)
      unreachable
    else
      i32.const 4
    end)
  (func (export "brif2") (param i32 i32 i32) (result i32 i32)
    block (result i32 i32)
      local.get 0
      local.get 1
      local.get 2
      br_if 0
      drop
      drop
      i32.const 8
      i32.const 9
    end)
  (func (export "brtab2") (param i32) (result i32 i32)
    block $a (result i32 i32)
      block $b (result i32 i32)
        i32.const 4
        i32.const 5
        local.get 0
        br_table $b $a
      end
      i32.const 10
      i32.add
    end)
  (func (export "dead5") (result i32)
    block $b (result i32)
      i32.const 1
      br $b
      block (result i32)
        i32.const 2
      end
      drop
      i32.const 3
    end)
)"""


def main():
    ok = True
    # odd export names
    bp = parse("bigname", BIGNAME_WAT)
    c = ["load a MODPATH", "init a",
         "invoke a %s 1" % "-",
         "invoke a %s 1" % h('a"b\\c'),
         "invoke a %s 1" % h("é中文"),
         "quit"]
    ok &= cmp.compare(bp, c, verbose=True)

    # wide signature
    bp = parse("wide", WIDE_WAT)
    args = " ".join(s32(i + 1) for i in range(25))
    c = ["load a MODPATH", "init a",
         "invoke a %s 5 %s" % (h("wide"), args), "quit"]
    ok &= cmp.compare(bp, c, verbose=True)

    # multi-value with v128
    bp = parse("multiv", MULTIV_WAT)
    c = ["load a MODPATH", "init a",
         "invoke a %s 5 %s" % (h("mv"), v128(1, 2, 3, 4) + " " + s32(77)),
         "invoke a %s 3" % h("mv2"), "quit"]
    ok &= cmp.compare(bp, c, verbose=True)

    # float select
    bp = parse("sel", SEL_WAT)
    c = ["load a MODPATH", "init a",
         "invoke a %s 1 %s" % (h("self32"), s32(0x3F800000) + " " + s32(0x40000000) + " " + s32(0)),
         "invoke a %s 1 %s" % (h("self32"), s32(0x3F800000) + " " + s32(0x40000000) + " " + s32(1)),
         "invoke a %s 1 %s" % (h("self64"), s32(0) + " " + s32(0) + " " + s32(0)),
         "quit"]
    ok &= cmp.compare(bp, c, verbose=True)

    # deep nesting
    bp = parse("nest", NEST_WAT)
    c = ["load a MODPATH", "init a", "invoke a %s 1" % h("nest"), "quit"]
    ok &= cmp.compare(bp, c, verbose=True)

    # many locals
    bp = parse("manylocals", MANY_LOCALS)
    c = ["load a MODPATH", "init a", "invoke a %s 1" % h("ml"), "quit"]
    ok &= cmp.compare(bp, c, verbose=True)

    # br_table with no cases / many cases
    bp = parse("bt0", BRTABLE0_WAT)
    c = ["load a MODPATH", "init a",
         "invoke a %s 1 %s" % (h("bt0"), s32(0)),
         "invoke a %s 1 %s" % (h("bt0"), s32(5)),
         "quit"]
    ok &= cmp.compare(bp, c, verbose=True)

    bp = parse("bigtable", make_bigtable())
    c = ["load a MODPATH", "init a"]
    for i in [0, 1, 50, 99, 100, 1000, -1]:
        c.append("invoke a %s 1 %s" % (h("bt"), s32(i)))
    c.append("quit")
    ok &= cmp.compare(bp, c, verbose=True)

    bp = parse("brfn", BRFN_WAT)
    c = ["load a MODPATH", "init a",
         "invoke a %s 1 %s" % (h("brfn"), s32(9)),
         "invoke a %s 1 %s" % (h("brtabfn"), s32(-2)),
         "invoke a %s 1 %s" % (h("retmid"), s32(0)),
         "invoke a %s 1 %s" % (h("retmid"), s32(1)),
         "invoke a %s 1" % h("dead2"),
         "invoke a %s 1 %s" % (h("dead3"), s32(0)),
         "invoke a %s 1 %s" % (h("dead3"), s32(1)),
         "invoke a %s 1 %s" % (h("dead4"), s32(0)),
         "invoke a %s 1 %s" % (h("dead4"), s32(1)),
         "invoke a %s 1" % h("dead5"),
         "invoke a %s 2 %s" % (h("brif2"), s32(3) + " " + s32(4) + " " + s32(0)),
         "invoke a %s 2 %s" % (h("brif2"), s32(3) + " " + s32(4) + " " + s32(1)),
         "invoke a %s 2 %s" % (h("brtab2"), s32(0)),
         "invoke a %s 2 %s" % (h("brtab2"), s32(1)),
         "invoke a %s 2 %s" % (h("brtab2"), s32(9)),
         "quit"]
    ok &= cmp.compare(bp, c, verbose=True)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())

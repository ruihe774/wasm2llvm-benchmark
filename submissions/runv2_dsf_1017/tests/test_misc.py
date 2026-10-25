#!/usr/bin/env python3
"""Miscellaneous differential and self-checked tests: exact float constants,
codegen corner cases, duplicate function types, imported start functions, and
reference-type/table instructions (self-checked, since the w2l-ref runner is
built without reference types)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from drive import *

WATDIR = os.path.join(ROOT, "tests/wat")
WORK = os.path.join(os.environ["TMPDIR"], "w2l-fuzz")


def wasm_of(name):
    os.makedirs(WORK, exist_ok=True)
    out = os.path.join(WORK, name + ".wasm")
    subprocess.run([os.path.join(TOOLS, "wat2wasm"),
                    os.path.join(WATDIR, name + ".wat"), "-o", out], check=True)
    return out


def sc_consts(s):
    t = [s.send("init a")]
    for v in [1, 0]:
        t.append(s.send(f"invoke a {hname('f32c')} 1 {slot_i32(v)}"))
        t.append(s.send(f"invoke a {hname('f64c')} 1 {slot_i32(v)}"))
    t.append(s.send(f"invoke a {hname('v128c')} 1 {slot_i32(0)}"))
    for a, b in [(0x7F800001, 0x7F800002), (0xFFC00001, 0x7F800001), (0x7FC00000, 0x7F800001)]:
        t.append(s.send(f"invoke a {hname('f32sub')} 1 {slot_f32b(a)} {slot_f32b(b)}"))
    return t


def sc_edge(s):
    t = [s.send("init a")]
    for i in [0, 1, 2, 3, -1]:
        t.append(s.send(f"invoke a {hname('ci_mv')} 2 {slot_i32(i)}"))
    for c in [0, 1, 2, -1, 0x1234]:
        t.append(s.send(f"invoke a {hname('sel_const')} 1 {slot_i32(c)}"))
        t.append(s.send(f"invoke a {hname('if_const')} 1 {slot_i32(c)}"))
        t.append(s.send(f"invoke a {hname('phi_const')} 1 {slot_i32(c)}"))
    for i in [0, 1, 2]:
        t.append(s.send(f"invoke a {hname('brt_func')} 2 {slot_i32(11)} {slot_i32(22)} {slot_i32(i)}"))
    for i in [0, 1, 2, 3]:
        t.append(s.send(f"invoke a {hname('ci_vv')} 1 {slot_i32(i)} {slot_i32(0x01020304)} {slot_i32(0x0A0B0C0D)}"))
    return t


def sc_mvv(s):
    t = [s.send("init a")]
    t.append(s.send(f"invoke a {hname('use')} 1"))
    t.append(s.send(f"invoke a {hname('pass')} 1 01000000020000000300000004000000 {slot_i32(9)}"))
    t.append(s.send(f"invoke a {hname('ret2')} 2"))
    return t


def sc_brt0(s):
    t = [s.send("init a")]
    for i in [0, 1, -1, 5]:
        t.append(s.send(f"invoke a {hname('f')} 1 {slot_i32(i)}"))
    return t


def sc_duptype(s):
    t = [s.send("init a")]
    for i in [0, 1, -1]:
        t.append(s.send(f"invoke a {hname('go')} 1"))
    return t


def sc_importstart(s):
    t = [s.send("init a")]
    t.append(s.send("output a"))
    t.append(s.send(f"invoke a {hname('f')} 0"))
    t.append(s.send(f"invoke a {hname('p_alias')} 0"))
    t.append(s.send("output a"))
    return t


# ---------------------------------------------------------------------------
# expect() based tests (no reference engine needed)
# ---------------------------------------------------------------------------

def expect(cond, msg):
    if not cond:
        raise AssertionError(msg)


def run_selfcheck(name, steps):
    """steps: list of (command, expected response or None)."""
    so = translate(wasm_of(name), "O2", tag="-self-" + name)
    s = Session([HARNESS])
    s.send(f"load a {so}")
    trace = []
    for cmd, exp in steps:
        resp = s.send(cmd)
        trace.append(resp)
        if exp is not None:
            expect(resp == exp, f"{name}: {cmd!r}: got {resp!r}, want {exp!r}")
    s.close()
    return trace


def ok_i32(v):
    return "ok " + slot_i32(v)


def selfcheck_reftable(sess_cmds):
    return run_selfcheck("reftable", sess_cmds)


def selfcheck_reftypes2():
    i = lambda v: slot_i32(v)
    steps = [("init a", "ok"),
             (f"invoke a {hname('g_isnull')} 1", ok_i32(1)),
             (f"invoke a {hname('g2_isnull')} 1", ok_i32(0)),
             (f"invoke a {hname('e_isnull')} 1", ok_i32(1)),
             (f"invoke a {hname('set_e')} 0 {i(1)}", "ok"),
             (f"invoke a {hname('e_isnull')} 1", ok_i32(1)),
             (f"invoke a {hname('ci0')} 1 {i(0)}", ok_i32(1)),
             (f"invoke a {hname('ci0')} 1 {i(1)}", "trap 6"),
             (f"invoke a {hname('ci1')} 1 {i(0)}", "trap 6"),
             (f"invoke a {hname('t1_init')} 0", "ok"),
             (f"invoke a {hname('ci1')} 1 {i(0)}", ok_i32(2)),
             (f"invoke a {hname('ci1')} 1 {i(1)}", "trap 6"),
             (f"invoke a {hname('ci1')} 1 {i(3)}", "trap 6"),
             (f"invoke a {hname('tset')} 0 {i(2)} {i(0)}", "ok"),
             (f"invoke a {hname('ci0')} 1 {i(2)}", ok_i32(2)),
             (f"invoke a {hname('selref')} 1 {i(0)}", ok_i32(1)),
             (f"invoke a {hname('selref')} 1 {i(1)}", ok_i32(0)),
             (f"invoke a {hname('selref')} 1 {i(2)}", ok_i32(0)),
             (f"invoke a {hname('ci_global')} 1", "trap 6"),
             (f"invoke a {hname('init_g')} 0", "ok"),
             (f"invoke a {hname('g_isnull')} 1", ok_i32(0)),
             (f"invoke a {hname('ci_global')} 1", ok_i32(2))]
    return run_selfcheck("reftypes2", steps)


def selfcheck_reftable_ops():
    i = lambda v: slot_i32(v)
    steps = [("init a", "ok"),
             (f"invoke a {hname('size')} 1", ok_i32(4)),
             (f"invoke a {hname('ci')} 1 {i(0)}", ok_i32(11)),
             (f"invoke a {hname('ci')} 1 {i(1)}", ok_i32(22)),
             (f"invoke a {hname('ci')} 1 {i(2)}", "trap 6"),
             (f"invoke a {hname('ci')} 1 {i(4)}", "trap 5"),
             (f"invoke a {hname('isnull')} 1 {i(2)}", ok_i32(1)),
             (f"invoke a {hname('set')} 0 {i(2)} {i(1)}", "ok"),
             (f"invoke a {hname('ci')} 1 {i(2)}", ok_i32(22)),
             (f"invoke a {hname('set')} 0 {i(3)} {i(0)}", "ok"),
             (f"invoke a {hname('ci')} 1 {i(3)}", ok_i32(11)),
             (f"invoke a {hname('fill')} 0 {i(0)} {i(1)} {i(2)}", "ok"),
             (f"invoke a {hname('ci')} 1 {i(0)}", ok_i32(22)),
             (f"invoke a {hname('ci')} 1 {i(1)}", ok_i32(22)),
             (f"invoke a {hname('grow')} 1 {i(1)} {i(2)}", ok_i32(4)),
             (f"invoke a {hname('size')} 1", ok_i32(6)),
             (f"invoke a {hname('ci')} 1 {i(4)}", ok_i32(22)),
             (f"invoke a {hname('ci')} 1 {i(5)}", ok_i32(22)),
             (f"invoke a {hname('ci')} 1 {i(6)}", "trap 5"),
             (f"invoke a {hname('grow')} 1 {i(0)} {i(10)}", ok_i32(6)),
             (f"invoke a {hname('size')} 1", ok_i32(16)),
             (f"invoke a {hname('ci')} 1 {i(15)}", ok_i32(22)),
             (f"invoke a {hname('ci')} 1 {i(16)}", "trap 5"),
             # Now the size is 16 and delta 0xFFFFFFF0 pushes the result to
             # exactly 2^32, past the implicit maximum: grow fails without a
             # huge allocation.
             (f"invoke a {hname('grow')} 1 {i(0)} {i(-16)}", ok_i32(-1)),
             (f"invoke a {hname('size')} 1", ok_i32(16)),
             (f"invoke a {hname('fill')} 0 {i(17)} {i(0)} {i(0)}", "trap 5"),
             (f"invoke a {hname('fill')} 0 {i(16)} {i(0)} {i(0)}", "ok"),
             (f"invoke a {hname('set')} 0 {i(0)} {i(3)}", "ok"),
             (f"invoke a {hname('ci')} 1 {i(0)}", ok_i32(11))]
    return run_selfcheck("reftable", steps)


def main():
    opt = sys.argv[1] if len(sys.argv) > 1 else "O2"
    allok = True
    cases = [("consts", sc_consts), ("edge", sc_edge), ("mvv", sc_mvv),
             ("brt0", sc_brt0), ("duptype", sc_duptype), ("importstart", sc_importstart)]
    for name, sc in cases:
        ok = compare_scenario(wasm_of(name), sc, opt, tag="-misc-" + name)
        print(f"misc/{name} [{opt}]:", "PASS" if ok else "FAIL")
        allok &= ok
    # Reference-type self-checks are optimization independent; run once.
    if opt == "O2":
        try:
            selfcheck_reftypes2()
            print("misc/reftypes2 [selfcheck]: PASS")
        except AssertionError as e:
            print("misc/reftypes2 [selfcheck]: FAIL", e)
            allok = False
        try:
            selfcheck_reftable_ops()
            print("misc/reftable [selfcheck]: PASS")
        except AssertionError as e:
            print("misc/reftable [selfcheck]: FAIL", e)
            allok = False
    return 0 if allok else 1


if __name__ == "__main__":
    sys.exit(main())

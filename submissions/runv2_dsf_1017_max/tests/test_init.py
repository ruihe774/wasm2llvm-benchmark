#!/usr/bin/env python3
"""Instantiation semantics: active segment bounds, start functions, empty
modules, zero-page memories."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from drive import *

DIR = os.path.join(ROOT, "tests/wat/init")


def wasm(name):
    return os.path.join(os.environ["TMPDIR"], name + ".wasm")


def sc_simple(s):
    t = [s.send("init a")]
    t.append(s.send("output a"))
    return t


def sc_zero(s):
    t = [s.send("init a")]
    t.append(s.send(f"memhash a {hname('memory')}"))
    t.append(s.send(f"invoke a {hname('size')} 1"))
    t.append(s.send(f"invoke a {hname('grow')} 1 {slot_i32(1)}"))
    t.append(s.send(f"invoke a {hname('size')} 1"))
    t.append(s.send(f"invoke a {hname('sl')} 1 {slot_i32(0)} {slot_i32(0x11223344)}"))
    t.append(s.send(f"invoke a {hname('grow')} 1 {slot_i32(1)}"))
    t.append(s.send(f"invoke a {hname('size')} 1"))
    t.append(s.send(f"invoke a {hname('grow')} 1 {slot_i32(1)}"))
    t.append(s.send(f"invoke a {hname('grow')} 1 {slot_i32(0)}"))
    t.append(s.send(f"invoke a {hname('sl')} 1 {slot_i32(131071)} {slot_i32(7)}"))
    t.append(s.send(f"invoke a {hname('sl')} 1 {slot_i32(131072)} {slot_i32(7)}"))
    t.append(s.send(f"memhash a {hname('memory')}"))
    return t


def sc_order(s):
    t = [s.send("init a")]
    t.append(s.send(f"global a {hname('g')}"))
    t.append(s.send(f"memhash a {hname('memory')}"))
    return t


def main():
    opt = sys.argv[1] if len(sys.argv) > 1 else "O2"
    cases = [
        ("oob_data", sc_simple),
        ("oob_data_empty", sc_simple),
        ("oob_data_edge", sc_simple),
        ("oob_elem", sc_simple),
        ("start_trap", sc_simple),
        ("empty", sc_simple),
        ("zero_mem", sc_zero),
        ("order", sc_order),
    ]
    allok = True
    for name, sc in cases:
        ok = compare_scenario(wasm(name), sc, opt, tag="-init-" + name)
        print(f"init/{name} [{opt}]:", "PASS" if ok else "FAIL")
        allok &= ok
    return 0 if allok else 1


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Trap codes, memory bounds, tables, call_indirect, recursion."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from drive import *

WAT = os.path.join(ROOT, "tests/wat/traps.wat")
WASM = os.path.join(os.environ["TMPDIR"], "w2l-fuzz", "traps.wasm")


def build():
    os.makedirs(os.path.dirname(WASM), exist_ok=True)
    subprocess.run([os.path.join(TOOLS, "wat2wasm"), WAT, "-o", WASM], check=True)


def sc(seed):
    def f(s):
        t = []
        t.append(s.send("init a"))
        t.append(s.send(f"invoke a {hname('bump')} 1"))
        t.append(s.send(f"invoke a {hname('bump')} 1"))
        t.append(s.send(f"invoke a {hname('getg')} 1"))
        t.append(s.send(f"invoke a {hname('nop')} 0"))
        t.append(s.send(f"invoke a {hname('unreachable')} 0"))
        t.append(s.send(f"invoke a {hname('bump')} 1"))

        # integer division
        for a, b in [(7, 2), (-7, 2), (7, 0), (-2**31, -1), (10, -1), (-2**31, 1), (0, -1)]:
            t.append(s.send(f"invoke a {hname('div0')} 1 {slot_i32(a)} {slot_i32(b)}"))
            t.append(s.send(f"invoke a {hname('divu0')} 1 {slot_i32(a)} {slot_i32(b)}"))
            t.append(s.send(f"invoke a {hname('rem0')} 1 {slot_i32(a)} {slot_i32(b)}"))
        t.append(s.send(f"invoke a {hname('remmin')} 1 {slot_i32(-2**31)}"))
        t.append(s.send(f"invoke a {hname('divmin')} 1 {slot_i32(-2**31)}"))
        t.append(s.send(f"invoke a {hname('divmin')} 1 {slot_i32(5)}"))
        t.append(s.send(f"invoke a {hname('divmin64')} 1 {slot_i64(-2**63)}"))
        t.append(s.send(f"invoke a {hname('remmin64')} 1 {slot_i64(-2**63)}"))
        t.append(s.send(f"invoke a {hname('divmin64')} 1 {slot_i64(-2**63 + 1)}"))

        # float->int traps and saturation
        for bits in [0x7fc00000, 0x7f800000, 0xff800000, 0x4f000000, 0xcf000000, 0x4f7fffff, 0x3f800000]:
            t.append(s.send(f"invoke a {hname('trunc_nan')} 1 {slot_f32b(bits)}"))
        for bits in [0x7ff0000000000000, 0xfff0000000000000, 0x43e0000000000000, 0xc3e0000000000000, 0x41e0000000000001, 0xc1e0000000200000, 0x3ff0000000000000]:
            t.append(s.send(f"invoke a {hname('trunc_big')} 1 {slot_f64b(bits)}"))
            t.append(s.send(f"invoke a {hname('sat_big')} 1 {slot_f64b(bits)}"))
            t.append(s.send(f"invoke a {hname('sat_u_big')} 1 {slot_f64b(bits)}"))
        for bits in [0xbf800000, 0xc0000000, 0xbf7fffff, 0x7f800000, 0xff800000, 0x7fc00001]:
            t.append(s.send(f"invoke a {hname('trunc_u_neg')} 1 {slot_f32b(bits)}"))
            t.append(s.send(f"invoke a {hname('sat_nan')} 1 {slot_f32b(bits)}"))

        # memory loads/stores
        for addr in [0, 1, 3, 100, 65532, 65533, 65535, 65536, -1, 0x7fffffff]:
            t.append(s.send(f"invoke a {hname('load8')} 1 {slot_i32(addr)}"))
            t.append(s.send(f"invoke a {hname('load16s')} 1 {slot_i32(addr)}"))
            t.append(s.send(f"invoke a {hname('load64')} 1 {slot_i32(addr)}"))
            t.append(s.send(f"invoke a {hname('loadoff')} 1 {slot_i32(addr)}"))
        t.append(s.send(f"invoke a {hname('store8')} 0 {slot_i32(65535)} {slot_i32(0x1234)}"))
        t.append(s.send(f"invoke a {hname('store8')} 0 {slot_i32(65536)} {slot_i32(0x1234)}"))
        t.append(s.send(f"invoke a {hname('store64')} 0 {slot_i32(65529)} {slot_i64(0x1122334455667788)}"))
        t.append(s.send(f"invoke a {hname('store64')} 0 {slot_i32(65530)} {slot_i64(0x1122334455667788)}"))
        t.append(s.send(f"invoke a {hname('storef32')} 0 {slot_i32(65532)} {slot_f32b(0x3f800000)}"))
        t.append(s.send(f"invoke a {hname('load_oob')} 1"))
        t.append(s.send(f"invoke a {hname('load_ok_edge')} 1"))

        # memory size/grow
        t.append(s.send(f"invoke a {hname('memsize')} 1"))
        t.append(s.send(f"invoke a {hname('memgrow')} 1 {slot_i32(1)}"))
        t.append(s.send(f"invoke a {hname('memsize')} 1"))
        t.append(s.send(f"invoke a {hname('memgrow')} 1 {slot_i32(10)}"))
        t.append(s.send(f"invoke a {hname('memgrow')} 1 {slot_i32(0)}"))
        t.append(s.send(f"invoke a {hname('memgrow_probe')} 1 {slot_i32(65536*2)} {slot_i32(0)}"))
        t.append(s.send(f"memhash a {hname('memory')}"))
        t.append(s.send(f"invoke a {hname('memgrow')} 1 {slot_i32(2)}"))
        t.append(s.send(f"invoke a {hname('memsize')} 1"))
        t.append(s.send(f"invoke a {hname('memgrow')} 1 {slot_i32(-1)}"))
        t.append(s.send(f"memhash a {hname('memory')}"))

        # bulk memory
        t.append(s.send(f"invoke a {hname('mi')} 0 {slot_i32(200)} {slot_i32(0)} {slot_i32(6)}"))
        t.append(s.send(f"memread a {hname('memory')} 200 8"))
        t.append(s.send(f"invoke a {hname('mi')} 0 {slot_i32(200)} {slot_i32(0)} {slot_i32(1)}"))
        t.append(s.send(f"invoke a {hname('drop')} 0"))
        t.append(s.send(f"invoke a {hname('mi')} 0 {slot_i32(200)} {slot_i32(0)} {slot_i32(1)}"))
        t.append(s.send(f"invoke a {hname('mi')} 0 {slot_i32(200)} {slot_i32(0)} {slot_i32(0)}"))
        t.append(s.send(f"invoke a {hname('mi')} 0 {slot_i32(200)} {slot_i32(1)} {slot_i32(0)}"))
        t.append(s.send(f"invoke a {hname('mc')} 0 {slot_i32(300)} {slot_i32(100)} {slot_i32(5)}"))
        t.append(s.send(f"invoke a {hname('mc')} 0 {slot_i32(102)} {slot_i32(100)} {slot_i32(5)}"))
        t.append(s.send(f"memread a {hname('memory')} 96 16"))
        t.append(s.send(f"invoke a {hname('mc')} 0 {slot_i32(65530)} {slot_i32(0)} {slot_i32(10)}"))
        t.append(s.send(f"invoke a {hname('mc')} 0 {slot_i32(65536)} {slot_i32(0)} {slot_i32(0)}"))
        t.append(s.send(f"invoke a {hname('mc')} 0 {slot_i32(0)} {slot_i32(65536)} {slot_i32(0)}"))
        t.append(s.send(f"invoke a {hname('mf')} 0 {slot_i32(400)} {slot_i32(0x41)} {slot_i32(8)}"))
        t.append(s.send(f"memread a {hname('memory')} 400 8"))
        t.append(s.send(f"invoke a {hname('mf')} 0 {slot_i32(65533)} {slot_i32(0)} {slot_i32(4)}"))
        t.append(s.send(f"invoke a {hname('mf')} 0 {slot_i32(65536)} {slot_i32(0)} {slot_i32(0)}"))

        # tables / call_indirect
        t.append(s.send(f"invoke a {hname('call_ind')} 1 {slot_i32(0)}"))
        t.append(s.send(f"invoke a {hname('call_ind')} 1 {slot_i32(1)}"))
        t.append(s.send(f"invoke a {hname('call_ind')} 1 {slot_i32(2)}"))
        t.append(s.send(f"invoke a {hname('call_ind')} 1 {slot_i32(3)}"))
        t.append(s.send(f"invoke a {hname('call_ind')} 1 {slot_i32(4)}"))
        t.append(s.send(f"invoke a {hname('call_ind')} 1 {slot_i32(-1)}"))
        t.append(s.send(f"invoke a {hname('call_ind_arg')} 1 {slot_i32(0)} {slot_i32(5)}"))
        t.append(s.send(f"invoke a {hname('call_ind_arg')} 1 {slot_i32(1)} {slot_i32(5)}"))

        # recursion
        for n in [0, 1, 5, 100, 1000, 10000]:
            t.append(s.send(f"invoke a {hname('recurse')} 1 {slot_i32(n)}"))
        t.append(s.send(f"invoke a {hname('deepframe')} 1 {slot_i32(10000)}"))
        # stack exhaustion; then the instance must still work
        t.append(s.send(f"invoke a {hname('inf')} 1"))
        t.append(s.send(f"invoke a {hname('bump')} 1"))
        t.append(s.send(f"invoke a {hname('getg')} 1"))
        t.append(s.send(f"invoke a {hname('recurse')} 1 {slot_i32(10)}"))
        t.append(s.send(f"memhash a {hname('memory')}"))
        t.append(s.send(f"invoke a {hname('bump')} 1"))
        return t
    return f


def main():
    opt = sys.argv[1] if len(sys.argv) > 1 else "O2"
    build()
    ok = compare_scenario(WASM, sc(1), opt, tag="-traps")
    print(f"traps [{opt}]:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Control-flow / multi-value torture tests."""
import os
import random
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from drive import *

WAT = os.path.join(ROOT, "tests/wat/control.wat")
WASM = os.path.join(os.environ["TMPDIR"], "w2l-fuzz", "control.wasm")


def build():
    os.makedirs(os.path.dirname(WASM), exist_ok=True)
    subprocess.run([os.path.join(TOOLS, "wat2wasm"), WAT, "-o", WASM], check=True)


def sc(seed):
    def f(s):
        rng = random.Random(seed)
        t = [s.send("init a")]
        t.append(s.send(f"invoke a {hname('mv_block')} 3"))
        for _ in range(6):
            a = rng.getrandbits(32)
            b = rng.getrandbits(32)
            t.append(s.send(f"invoke a {hname('block_params')} 2 {slot_i32(a)} {slot_i32(b)}"))
            t.append(s.send(f"invoke a {hname('loop_brif')} 1 {slot_i32(a % 50)} {slot_i32(b)}"))
            t.append(s.send(f"invoke a {hname('many')} 1 " + " ".join(slot_i32(rng.getrandbits(32)) for _ in range(16))))
        for n in [0, 1, 2, 3, 10, 100, 1000]:
            t.append(s.send(f"invoke a {hname('loop_sum')} 2 {slot_i32(n)}"))
            t.append(s.send(f"invoke a {hname('loop_brif')} 1 {slot_i32(n)} {slot_i32(7)}"))
        for i in [0, 1, 2, 3, 4, -1]:
            t.append(s.send(f"invoke a {hname('brt')} 1 {slot_i32(i)}"))
            t.append(s.send(f"invoke a {hname('nested')} 1 {slot_i32(i)}"))
        for c in [0, 1, 2, -1]:
            t.append(s.send(f"invoke a {hname('brif_val')} 1 {slot_i32(c)}"))
            t.append(s.send(f"invoke a {hname('br_func')} 2 {slot_i32(c)}"))
            for a, b in [(1, 2), (0x7FFFFFFF, -5)]:
                t.append(s.send(f"invoke a {hname('sel_v128')} 1 {slot_i32(a)} {slot_i32(b)} {slot_i32(c)}"))
        for a, b, c in [(3, 4, 0), (3, 4, 1), (0x7FFFFFFF, 1, 0), (10, 20, -5)]:
            t.append(s.send(f"invoke a {hname('if_params')} 2 {slot_i32(a)} {slot_i32(b)} {slot_i32(c)}"))
            t.append(s.send(f"invoke a {hname('if_params_noelse')} 2 {slot_i32(a)} {slot_i32(b)} {slot_i32(c)}"))
        for a, b in [(0, 0), (5, 0), (0, 5), (5, 5), (100, 3), (3, 100), (0xFFFFFFFF, 0x80000000)]:
            t.append(s.send(f"invoke a {hname('loop2')} 2 {slot_i32(a)} {slot_i32(b)}"))
        for n in [0, 1, 2, 5, 20, 1000]:
            t.append(s.send(f"invoke a {hname('brt_loop')} 1 {slot_i32(n)}"))
        for idx in [0, 1, 2, 3]:
            t.append(s.send(f"invoke a {hname('ci_v128')} 1 {slot_i32(idx)} {slot_i32(0x01020304)}"))
            t.append(s.send(f"invoke a {hname('ci_i2v')} 1 {slot_i32(idx)} {slot_i32(0x55)}"))
        t.append(s.send(f"memhash a {hname('memory')}"))
        return t
    return f


def main():
    opt = sys.argv[1] if len(sys.argv) > 1 else "O2"
    build()
    ok = compare_scenario(WASM, sc(5), opt, tag="-ctl")
    print(f"control [{opt}]:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Locate the first differing byte in minicelt memory between ref and our .so."""
import os
import random
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from drive import *

WASM = os.path.join(ROOT, "payloads/minicelt/minicelt.wasm")


def run(sess, wasm_or_so, is_ref, seed=11):
    s = Session([os.path.join(TOOLS, "w2l-ref")] if is_ref else [HARNESS])
    path = wasm_or_so
    s.send(f"load a {path}")
    rng = random.Random(seed)
    out = {}
    out["init"] = s.send("init a")
    ch, br, cx, vbr, dtx = 1, 64000, 11, 0, 0
    out["m_init_call"] = s.send(f"invoke a {hname('minicelt_init')} 1 {slot_i32(ch)} {slot_i32(br)} {slot_i32(cx)} {slot_i32(vbr)} {slot_i32(dtx)}")
    out["mem_after_init"] = s.send(f"memread a {hname('memory')} 0 131072")
    n = 960 * ch
    samples = b"".join(struct.pack("<f", rng.uniform(-1.0, 1.0)) for _ in range(n))
    r = s.send(f"invoke a {hname('minicelt_input_ptr')} 1")
    out["input_ptr"] = r
    ptr = slot_result_u32(r)
    out["memwrite"] = s.send(f"memwrite a {hname('memory')} {ptr} {hx(samples)}")
    out["encode"] = s.send(f"invoke a {hname('minicelt_encode')} 1")
    out["mem_after_encode"] = s.send(f"memread a {hname('memory')} 0 131072")
    out["output_ptr"] = s.send(f"invoke a {hname('minicelt_output_ptr')} 1")
    s.close()
    return out


def diff_mem(a_hex, b_hex, label):
    a = bytes.fromhex(a_hex.split()[1])
    b = bytes.fromhex(b_hex.split()[1])
    if len(a) != len(b):
        print(label, "length differs", len(a), len(b))
        return
    diffs = [i for i in range(len(a)) if a[i] != b[i]]
    print(f"{label}: {len(diffs)} differing bytes")
    for i in diffs[:20]:
        lo = max(0, i - 8)
        hi = min(len(a), i + 24)
        print(f"  off {i} (0x{i:x}) ref={a[lo:hi].hex()} got={b[lo:hi].hex()}")


def main():
    so = translate(WASM, "O2", tag="-dbg")
    ref = run(None, WASM, True)
    got = run(None, so, False)
    for k in ref:
        if ref[k] != got[k] and not k.startswith("mem_"):
            print(f"{k}: ref={ref[k][:100]} got={got[k][:100]}")
    diff_mem(ref["mem_after_init"], got["mem_after_init"], "after init")
    diff_mem(ref["mem_after_encode"], got["mem_after_encode"], "after encode")


if __name__ == "__main__":
    main()

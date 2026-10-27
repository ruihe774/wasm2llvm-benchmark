#!/usr/bin/env python3
"""Differential tests for Rust-compiled wasm modules."""
import os
import random
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from drive import *

DIR = os.path.join(os.environ["TMPDIR"], "rusttest")


def ensure_built():
    os.makedirs(DIR, exist_ok=True)
    src = os.path.join(ROOT, "tests/rust/lib.rs")
    for target, name in [("wasm32v1-none", "rust_v1"),
                         ("wasm32-unknown-unknown", "rust_unknown")]:
        out = os.path.join(DIR, name + ".wasm")
        if os.path.exists(out):
            continue
        try:
            subprocess.run(["rustc", "--target", target, "-O", "--crate-type", "cdylib",
                            "--edition", "2021", src, "-o", out], check=True,
                           capture_output=True)
        except (FileNotFoundError, subprocess.CalledProcessError):
            pass


def sc(seed):
    def f(s):
        rng = random.Random(seed)
        t = [s.send("init a")]
        t.append(s.send(f"memhash a {hname('memory')}"))

        for _ in range(12):
            a = rng.getrandbits(32)
            b = rng.getrandbits(32)
            t.append(s.send(f"invoke a {hname('add')} 1 {slot_i32(a)} {slot_i32(b)}"))
        for n in [0, 1, 2, 5, 10, 15, 20]:
            t.append(s.send(f"invoke a {hname('fib')} 1 {slot_i32(n)}"))
        for i in [0, 1, 7, 8, 100, 0x7fffffff, -1]:
            t.append(s.send(f"invoke a {hname('table_at')} 1 {slot_i32(i)}"))
        for bits in [0x7ff8000000000000, 0x7ff0000000000000, 0xfff0000000000000,
                     0x0000000000000000, 0x8000000000000000, 0xbff0000000000000,
                     0x3ff0000000000000, 0x0000000000000001, 0xbff0000000000001]:
            t.append(s.send(f"invoke a {hname('classify')} 1 {slot_f64b(bits)}"))
        for bits in [0x3ff0000000000000, 0xbff0000000000000, 0x7ff8000000000000,
                     0x7ff0000000000000, 0xfff0000000000000, 0x43e0000000000000,
                     0xc3e0000000000000, 0x43dfffffffffffff, 0xc3e0000000000001,
                     0x3fe0000000000000, 0xbfe0000000000000, 0x4197d78400000000]:
            t.append(s.send(f"invoke a {hname('truncz')} 1 {slot_f64b(bits)}"))
        for _ in range(10):
            a = rng.getrandbits(64)
            b = rng.getrandbits(64)
            t.append(s.send(f"invoke a {hname('mulh')} 1 {slot_i64(a)} {slot_i64(b)}"))

        # dot product over f32 arrays
        base_a = 1050000
        base_b = 1060000
        vals_a = [rng.uniform(-1e3, 1e3) for _ in range(64)]
        vals_b = [rng.uniform(-1e3, 1e3) for _ in range(64)]
        ba = b"".join(struct.pack("<f", v) for v in vals_a)
        bb = b"".join(struct.pack("<f", v) for v in vals_b)
        t.append(s.send(f"memwrite a {hname('memory')} {base_a} {hx(ba)}"))
        t.append(s.send(f"memwrite a {hname('memory')} {base_b} {hx(bb)}"))
        for n in [0, 1, 3, 16, 64]:
            t.append(s.send(f"invoke a {hname('dot')} 1 {slot_i32(base_a)} {slot_i32(base_b)} {slot_i32(n)}"))
        # sum bytes
        base_s = 1070000
        bytes_s = bytes(rng.randrange(256) for _ in range(200))
        t.append(s.send(f"memwrite a {hname('memory')} {base_s} {hx(bytes_s)}"))
        for n in [0, 1, 7, 100, 200]:
            t.append(s.send(f"invoke a {hname('sum_bytes')} 1 {slot_i32(base_s)} {slot_i32(n)}"))
        # memops overlap copy
        base_m = 1080000
        blob = bytes(range(64))
        t.append(s.send(f"memwrite a {hname('memory')} {base_m} {hx(blob)}"))
        t.append(s.send(f"invoke a {hname('memops')} 0 {slot_i32(base_m + 4)} {slot_i32(base_m)} {slot_i32(32)}"))
        t.append(s.send(f"memread a {hname('memory')} {base_m} 64"))
        t.append(s.send(f"invoke a {hname('memops')} 0 {slot_i32(base_m)} {slot_i32(base_m + 4)} {slot_i32(32)}"))
        t.append(s.send(f"memread a {hname('memory')} {base_m} 64"))
        # trait objects / call_indirect
        for which in [0, 1, 2]:
            for x in [0, 1, -1, 12345, -2**31]:
                t.append(s.send(f"invoke a {hname('run_op')} 1 {slot_i32(which)} {slot_i32(x)}"))
                t.append(s.send(f"invoke a {hname('run_closure')} 1 {slot_i32(which)} {slot_i32(x)}"))
        # SIMD
        va = bytes(range(16))
        vb = bytes(range(16, 32))
        base_v = 1090000
        t.append(s.send(f"memwrite a {hname('memory')} {base_v} {hx(va)}"))
        t.append(s.send(f"memwrite a {hname('memory')} {base_v+16} {hx(vb)}"))
        t.append(s.send(f"invoke a {hname('simd_add')} 0 {slot_i32(base_v)} {slot_i32(base_v+16)} {slot_i32(base_v+32)}"))
        t.append(s.send(f"memread a {hname('memory')} {base_v+32} 16"))
        base_sb = 1090100
        blob2 = bytes((i * 37 + 11) & 0xff for i in range(1024))
        t.append(s.send(f"memwrite a {hname('memory')} {base_sb} {hx(blob2)}"))
        for n in [0, 1, 15, 16, 31, 32, 1024]:
            t.append(s.send(f"invoke a {hname('simd_sum_bytes')} 1 {slot_i32(base_sb)} {slot_i32(n)}"))
        for n in [0, 1, 10, 1000]:
            t.append(s.send(f"invoke a {hname('bench')} 1 {slot_i32(n)}"))
        # out of bounds memory access via dot with an OOB pointer
        t.append(s.send(f"invoke a {hname('dot')} 1 {slot_i32(17*65536 - 4)} {slot_i32(base_b)} {slot_i32(4)}"))
        t.append(s.send(f"memhash a {hname('memory')}"))
        return t
    return f


CLI_TESTS = [
    ("add", (1234, 5678)),
    ("add", (0, 0)),
    ("fib", (10,)),
    ("fib", (15,)),
    ("run_op", (0, 100)),
    ("run_op", (1, 100)),
    ("run_closure", (0, 100)),
    ("run_closure", (1, 100)),
    ("bench", (1000,)),
    ("bench", (100000,)),
    ("table_at", (5,)),
    ("table_at", (0x7FFFFFFF,)),
]


def cli_result(wasm, name, args):
    r = subprocess.run(["wasmtime", "run", "--invoke", name, wasm] + [str(a) for a in args],
                       capture_output=True, text=True)
    for line in r.stdout.splitlines():
        if line.strip() and not line.startswith("warning"):
            return line.strip()
    return None


def compare_via_cli(wasm, opt, name, tag):
    so = translate(wasm, opt, tag)
    s = Session([HARNESS])
    s.send(f"load a {so}")
    s.send("init a")
    ok = True
    for fname, args in CLI_TESTS:
        resp = s.send(f"invoke a {hname(fname)} 1 " + " ".join(slot_i32(a) for a in args))
        if not resp.startswith("ok"):
            got = resp
        else:
            got = str(slot_result_u32(resp))
        want = cli_result(wasm, fname, args)
        try:
            same = want is not None and (ord(got[0]) if False else int(got) & 0xFFFFFFFF) == (int(want) & 0xFFFFFFFF)
        except (TypeError, ValueError):
            same = got == want
        if not same:
            ok = False
            print(f"CLI mismatch {fname}{args}: ours={got} wasmtime={want}")
    s.close()
    return ok


def ref_can_load(wasm):
    r = subprocess.run([os.path.join(TOOLS, "w2l-ref")],
                       input=f"load a {wasm}\nquit\n", capture_output=True, text=True)
    return r.stdout.startswith("ok")


def main():
    opt = sys.argv[1] if len(sys.argv) > 1 else "O2"
    ensure_built()
    allok = True
    for name in ["rust_v1", "rust_unknown"]:
        wasm = os.path.join(DIR, name + ".wasm")
        if not os.path.exists(wasm):
            print(f"rust/{name} [{opt}]: SKIP (no rustc wasm target)")
            continue
        if not ref_can_load(wasm):
            # The w2l-ref runner rejects this module (padded LEB table index
            # with reference types disabled); compare simple exports through
            # the wasmtime CLI instead.
            ok = compare_via_cli(wasm, opt, name, "-cli-" + name)
            print(f"rust/{name} [{opt}]:", "PASS (cli)" if ok else "FAIL (cli)")
            allok &= ok
            continue
        ok = compare_scenario(wasm, sc(3), opt, tag="-rs-" + name)
        print(f"rust/{name} [{opt}]:", "PASS" if ok else "FAIL")
        allok &= ok
    return 0 if allok else 1


if __name__ == "__main__":
    sys.exit(main())

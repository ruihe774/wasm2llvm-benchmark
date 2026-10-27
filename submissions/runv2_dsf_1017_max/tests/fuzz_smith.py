#!/usr/bin/env python3
"""Differential fuzzing: wasm-tools smith generated modules vs w2l-ref."""
import os
import random
import struct
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from drive import *

TOOLS = os.environ.get("W2L_TOOLS", "/mnt/workspace/wasm2llvm/deps/bin")
WORK = os.path.join(os.environ.get("TMPDIR", "/tmp"), "w2l-fuzz")
os.makedirs(WORK, exist_ok=True)

EXPORTS_WAT = os.path.join(ROOT, "tests/smith/exports.wat")
EXPORTS_WASM = os.path.join(WORK, "exports.wasm")

SIGS = [
    ("f_ii", [("i32", 0), ("i32", 0)], 1),
    ("f_ll", [("i64", 0), ("i64", 0)], 1),
    ("f_fd", [("f32", 0), ("f64", 0)], 1),
    ("f_multi", [], 2),
    ("f_multi2", [("i32", 0), ("i32", 0)], 2),
    ("f_void", [("i32", 0), ("i64", 0)], 0),
    ("f_v1", [("v128", 0), ("v128", 0)], 1),
    ("f_v2", [("i32", 0), ("f64", 0)], 1),
    ("f_v3", [("v128", 0)], 1),
    ("f_mix", [("i32", 0), ("i64", 0), ("f32", 0), ("f64", 0), ("v128", 0)], 1),
]


def ensure_exports_wasm():
    if not os.path.exists(EXPORTS_WASM):
        subprocess.run([os.path.join(TOOLS, "wat2wasm"), EXPORTS_WAT, "-o", EXPORTS_WASM], check=True)


SMITH_FLAGS = [
    "--simd-enabled", "true",
    "--bulk-memory-enabled", "true",
    "--reference-types-enabled", "false",
    "--multi-value-enabled", "true",
    "--sign-extension-ops-enabled", "true",
    "--saturating-float-to-int-enabled", "true",
    "--exceptions-enabled", "false",
    "--tail-call-enabled", "false",
    "--threads-enabled", "false",
    "--memory64-enabled", "false",
    "--gc-enabled", "false",
    "--relaxed-simd-enabled", "false",
    "--extended-const-enabled", "false",
    "--wide-arithmetic-enabled", "false",
    "--custom-page-sizes-enabled", "false",
    "--canonicalize-nans", "true",
    "--ensure-termination",
    "-f", "5000",
]


def gen_module(seed_bytes, path, extra=()):
    cmd = [os.path.join(TOOLS, "wasm-tools"), "smith", "-o", path,
           "--module-shape", EXPORTS_WASM,
           "--max-funcs", "12", "--max-instructions", "5000",
           "--max-tables", "2", "--max-memory32-bytes", "1048576"] + SMITH_FLAGS + list(extra)
    if os.environ.get("W2L_FUZZ_NOTRAP"):
        cmd = cmd + ["--disallow-traps", "true"]
    r = subprocess.run(cmd, input=seed_bytes, capture_output=True)
    return r.returncode == 0


def rand_arg(rng, ty):
    if ty == "i32":
        return slot_u32(rng.getrandbits(32))
    if ty == "i64":
        return slot_u64(rng.getrandbits(64))
    if ty == "f32":
        return slot_f32b(rng.getrandbits(32))
    if ty == "f64":
        return slot_f64b(rng.getrandbits(64))
    if ty == "v128":
        return bytes(rng.getrandbits(8) for _ in range(16)).hex()
    raise ValueError(ty)


def make_scenario(seed):
    def sc(s):
        rng = random.Random(seed)
        trace = []
        r = s.send("init a")
        trace.append(r)
        if not r.startswith("ok"):
            # A trapping instantiation leaves the reference runner's instance
            # unusable; there is nothing further to compare.
            return trace
        for rnd in range(3):
            for (name, params, nres) in SIGS:
                args = " ".join(rand_arg(rng, t) for (t, _) in params)
                cmd = f"invoke a {hname(name)} {nres}"
                if args:
                    cmd += " " + args
                trace.append(s.send(cmd))
                trace.append(s.send(f"memhash a {hname('memory')}"))
            trace.append(s.send(f"global a {hname('g_i32')}"))
            trace.append(s.send(f"global a {hname('g_i64')}"))
        trace.append(s.send("output a"))
        return trace
    return sc


def main():
    ensure_exports_wasm()
    start = int(sys.argv[1]) if len(sys.argv) > 1 else 0
    count = int(sys.argv[2]) if len(sys.argv) > 2 else 20
    opt = sys.argv[3] if len(sys.argv) > 3 else "O2"
    fail = 0
    for i in range(start, start + count):
        srng = random.Random(i * 7919 + 13)
        seed = srng.randbytes(2000)
        wasm = os.path.join(WORK, f"s{i}.wasm")
        if not gen_module(seed, wasm):
            continue
        try:
            ok = compare_scenario(wasm, make_scenario(i), opt, tag=f"-fz{i}")
        except Exception as e:
            print(f"seed {i}: EXCEPTION {e}")
            ok = False
        if not ok:
            print(f"seed {i}: FAIL (saved {wasm})")
            fail += 1
            if fail >= 5:
                print("stopping after 5 failures")
                return 1
    print(f"done: {count} seeds, {fail} failures")
    return 1 if fail else 0


if __name__ == "__main__":
    sys.exit(main())

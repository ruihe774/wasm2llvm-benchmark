#!/usr/bin/env python3
"""Randomized differential testing against w2l-ref using wasm-tools smith.

Usage: python3 -I tests/fuzz.py [COUNT] [START_SEED]
"""
import os
import random
import shutil
import struct
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
W2L = os.path.join(ROOT, "target", "release", "wasm2llvm")
TMP = os.environ.get("TMPDIR", tempfile.gettempdir())
HARNESS = os.path.join(ROOT, "tests", ".harness")
SHAPE = os.path.join(ROOT, "tests", ".shape.wasm")

SMITH_FLAGS = [
    "--bulk-memory-enabled", "true",
    "--multi-value-enabled", "true",
    "--saturating-float-to-int-enabled", "true",
    "--sign-extension-ops-enabled", "true",
    "--simd-enabled", "true",
    "--reference-types-enabled", "false",
    "--relaxed-simd-enabled", "false",
    "--exceptions-enabled", "false",
    "--gc-enabled", "false",
    "--memory64-enabled", "false",
    "--custom-page-sizes-enabled", "false",
    "--extended-const-enabled", "false",
    "--tail-call-enabled", "false",
    "--threads-enabled", "false",
    "--shared-everything-threads-enabled", "false",
    "--wide-arithmetic-enabled", "false",
    "--compact-imports-enabled", "false",
    "--custom-descriptors-enabled", "false",
    "--max-memories", "0",
    "--max-tables", "1",
    "--max-imports", "0",
    "--min-imports", "0",
    "--max-memory32-bytes", "1048576",
    "--memory-max-size-required", "false",
    "--canonicalize-nans", "true",
    "--ensure-termination",
    "--fuel", "10000000",
]

SIMD_FLAGS = [
    "--allowed-instructions",
    "vector,numeric,control,memory,parametric,variable,table",
    "--max-instructions", "1500",
    "--max-funcs", "15",
    "--max-nesting-depth", "6",
]


def nan_f32(h):
    try:
        b = int.from_bytes(bytes.fromhex(h[0:8]), "little")
    except ValueError:
        return False
    return (b & 0x7F800000) == 0x7F800000 and (b & 0x007FFFFF) != 0


def nan_f64(h):
    try:
        b = int.from_bytes(bytes.fromhex(h[0:16]), "little")
    except ValueError:
        return False
    return (b & 0x7FF0000000000000) == 0x7FF0000000000000 and (b & 0x000FFFFFFFFFFFFF) != 0


def normalize_reply(line, rtypes):
    if not line.startswith("ok "):
        return line
    parts = line.split()
    if len(parts) - 1 != len(rtypes):
        return line
    out = ["ok"]
    for t, slot in zip(rtypes, parts[1:]):
        if (t == "f32" and nan_f32(slot)) or (t == "f64" and nan_f64(slot)):
            slot = "NAN" * 8
        out.append(slot)
    return " ".join(out)

# exported function type map: name -> (params, results)
TYPES = {
    "f_ii": ("i32 i32", "i32"),
    "f_ll": ("i64 i64", "i64"),
    "f_ff": ("f32 f32", "f32"),
    "f_dd": ("f64 f64", "f64"),
    "f_mix": ("i32 f64 i64 f32", "i32"),
    "f_multi": ("i32 i32", "i32 i64"),
    "f_v": ("v128 v128", "v128"),
    "f_vs": ("v128 i32", "v128"),
    "f_void": ("i32", ""),
    "f_many": ("i32 i64 f32 f64 v128 i32", "i32 v128"),
    "f_v1": ("v128", "i32"),
    "f_s": ("i32 i32 i32", "i32"),
}

INT_BITS32 = [0, 1, 2, -1, 0x7FFFFFFF, -0x80000000, 0x55555555, 12345]
INT_BITS64 = [0, 1, 2, -1, 0x7FFFFFFFFFFFFFFF, -0x8000000000000000, 1234567890123]
F32_BITS = [0x00000000, 0x80000000, 0x3F800000, 0xBF800000, 0x7F800000, 0xFF800000,
            0x7FC00000, 0x7F800001, 0x00000001, 0x7F7FFFFF]
F64_BITS = [0x0000000000000000, 0x8000000000000000, 0x3FF0000000000000,
            0xBFF0000000000000, 0x7FF0000000000000, 0xFFF0000000000000,
            0x7FF8000000000000, 0x7FF0000000000001, 0x0000000000000001]


def slot_for(ty, rnd):
    if ty == "i32":
        v = rnd.choice(INT_BITS32)
        if rnd.random() < 0.3:
            v = rnd.getrandbits(32)
        return struct.pack("<I", v & 0xFFFFFFFF).hex() + "00" * 12
    if ty == "i64":
        v = rnd.choice(INT_BITS64)
        if rnd.random() < 0.3:
            v = rnd.getrandbits(64)
        return struct.pack("<Q", v & 0xFFFFFFFFFFFFFFFF).hex() + "00" * 8
    if ty == "f32":
        v = rnd.choice(F32_BITS)
        if rnd.random() < 0.3:
            v = rnd.getrandbits(32)
        return struct.pack("<I", v).hex() + "00" * 12
    if ty == "f64":
        v = rnd.choice(F64_BITS)
        if rnd.random() < 0.3:
            v = rnd.getrandbits(64)
        return struct.pack("<Q", v).hex() + "00" * 8
    # v128
    if rnd.random() < 0.5:
        bs = bytes(rnd.getrandbits(8) for _ in range(16))
    else:
        lane = rnd.choice([0, 1, 0xFFFFFFFF, 0x7F7FFFFF, 0x3F800000])
        bs = struct.pack("<4I", lane, lane, lane, lane)
    return bs.hex()


def make_cmds(rnd, calls=3):
    cmds = ["load a MODPATH", "init a", "memhash a 6d656d"]
    names = list(TYPES.keys())
    rnd.shuffle(names)
    for _ in range(calls):
        for name in names:
            params, results = TYPES[name]
            args = [slot_for(t, rnd) for t in params.split()] if params else []
            nres = len(results.split()) if results else 0
            cmds.append("invoke a %s %d%s" % (
                name.encode().hex(), nres,
                (" " + " ".join(args)) if args else ""))
            cmds.append("memhash a 6d656d")
    cmds.append("quit")
    return cmds


def sh(cmd, inp=None):
    return subprocess.run(cmd, capture_output=True, text=True, input=inp)


def run_stdin(exe, cmds):
    r = subprocess.run(exe, input="\n".join(cmds) + "\n", capture_output=True, text=True)
    if r.returncode != 0:
        return ["<crashed rc=%d>" % r.returncode] + r.stdout.splitlines() + r.stderr.splitlines()
    return r.stdout.splitlines()


def one(seed, workdir, verbose=False, mode="normal"):
    if mode == "simd":
        seed_bytes = bytes([(seed * 131 + i * 37 + (i * i >> 3)) & 0xFF
                            for i in range(2000 + (seed * 97) % 4000)])
    else:
        seed_bytes = bytes([(seed >> (8 * (i % 4))) & 0xFF for i in range(64 + (seed * 37) % 2000)])
    wasm = os.path.join(workdir, "m.wasm")
    with open(os.path.join(workdir, "seed.bin"), "wb") as f:
        f.write(seed_bytes)
    flags = SMITH_FLAGS + (SIMD_FLAGS if mode == "simd" else [])
    r = sh(["wasm-tools", "smith", "--exports", SHAPE] + flags +
           ["-o", wasm, os.path.join(workdir, "seed.bin")])
    if r.returncode != 0:
        return ("skip", "smith: " + r.stderr.strip()[:200])
    rnd = random.Random(seed * 7919 + 13)
    cmds = make_cmds(rnd)
    ref = run_stdin("w2l-ref", [c.replace("MODPATH", wasm) for c in cmds])
    ref_rejects = bool(ref) and ref[0].startswith("error")
    ll = os.path.join(workdir, "m.ll")
    r = sh([W2L, wasm, "-o", ll])
    if r.returncode != 0:
        if ref_rejects:
            return ("skip", "both reject: " + r.stderr.strip()[:120])
        return ("translate-error", r.stderr.strip()[:500])
    if ref_rejects:
        return ("mismatch", "we accept but ref rejects: " + ref[0][:200])
    result = ("ok", None)
    for opt in ("-O0", "-O2"):
        so = os.path.join(workdir, "m%s.so" % opt)
        r = sh(["clang", opt, "-shared", "-fPIC", "-Wl,-Bsymbolic", "-o", so, ll, "-lm"])
        if r.returncode != 0:
            return ("clang-error", opt + ": " + r.stderr.strip()[:1500])
        ours = run_stdin(HARNESS, [c.replace("MODPATH", so) for c in cmds])
        refn = []
        oursn = []
        for c, a, b in zip(cmds, ref, ours):
            rt = []
            if c.startswith("invoke a "):
                name = bytes.fromhex(c.split()[2]).decode()
                rt = TYPES.get(name, ("", ""))[1].split() if name in TYPES else []
            refn.append(normalize_reply(a, rt))
            oursn.append(normalize_reply(b, rt))
        ref, ours = refn, oursn
        # If instantiation itself traps, later commands are meaningless (the
        # reference runner refuses to run them); compare only up to that point.
        if len(ref) > 1 and len(ours) > 1 and ref[1].startswith("trap"):
            ref = ref[:2]
            ours = ours[:2]
        if ours != ref:
            diffs = []
            for i in range(max(len(ref), len(ours))):
                a = ref[i] if i < len(ref) else "<missing>"
                b = ours[i] if i < len(ours) else "<missing>"
                if a != b:
                    diffs.append("line %d ref=%s ours=%s" % (i + 1, a[:200], b[:200]))
            return ("mismatch", opt + ": " + " | ".join(diffs[:6]))
    return result


def main():
    count = int(sys.argv[1]) if len(sys.argv) > 1 else 50
    start = int(sys.argv[2]) if len(sys.argv) > 2 else 0
    mode = sys.argv[3] if len(sys.argv) > 3 else "normal"
    if not os.path.exists(HARNESS):
        subprocess.run(["clang", "-O1", "-rdynamic", "-o", HARNESS,
                        os.path.join(ROOT, "tests", "harness.c"), "-ldl"], check=True)
    if not os.path.exists(SHAPE):
        r = sh(["wasm-tools", "parse", os.path.join(ROOT, "tests", "shape.wat"), "-o", SHAPE])
        if r.returncode != 0:
            print("shape parse failed:", r.stderr)
            return 1
    bad = 0
    skipped = 0
    for seed in range(start, start + count):
        workdir = tempfile.mkdtemp(prefix="w2lfuzz")
        try:
            status, msg = one(seed, workdir, mode=mode)
        finally:
            shutil.rmtree(workdir, ignore_errors=True)
        if status == "ok":
            print("seed %d ok" % seed, flush=True)
        elif status == "skip":
            skipped += 1
            print("seed %d SKIP %s" % (seed, msg), flush=True)
        else:
            bad += 1
            print("seed %d %s: %s" % (seed, status.upper(), msg), flush=True)
    print("done: %d ok, %d bad, %d skipped" % (count - bad - skipped, bad, skipped))
    return 0 if bad == 0 else 1


if __name__ == "__main__":
    sys.exit(main())

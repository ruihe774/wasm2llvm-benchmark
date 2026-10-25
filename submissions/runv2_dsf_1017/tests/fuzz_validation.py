#!/usr/bin/env python3
"""Differential validation fuzzing: mutate valid modules and compare our
translator's accept/reject decision with `wasm-tools validate -f wasm2`."""
import os
import random
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TOOLS = os.environ.get("W2L_TOOLS", "/mnt/workspace/wasm2llvm/deps/bin")
WASM2LLVM = os.environ.get("W2L_BIN", os.path.join(ROOT, "target", "release", "wasm2llvm"))
WORK = os.path.join(os.environ.get("TMPDIR", "/tmp"), "w2l-valfuzz")
os.makedirs(WORK, exist_ok=True)


def oracle(path):
    r = subprocess.run([os.path.join(TOOLS, "wasm-tools"), "validate", "-f", "wasm2", path],
                       capture_output=True)
    return r.returncode == 0


def ours(path):
    out = os.path.join(WORK, "out.ll")
    try:
        os.unlink(out)
    except FileNotFoundError:
        pass
    r = subprocess.run([WASM2LLVM, path, "-o", out], capture_output=True)
    return r.returncode == 0, r.stderr.decode(errors="replace")[:300]


def mutate(data, rng):
    b = bytearray(data)
    kind = rng.random()
    if kind < 0.65:
        for _ in range(rng.randint(1, 3)):
            if not b:
                break
            i = rng.randrange(len(b))
            b[i] = rng.randrange(256)
    elif kind < 0.8:
        # truncate
        del b[rng.randrange(len(b)):]
    elif kind < 0.9:
        # insert bytes
        i = rng.randrange(len(b))
        b[i:i] = bytes(rng.randrange(256) for _ in range(rng.randint(1, 4)))
    else:
        # delete bytes
        i = rng.randrange(len(b))
        n = min(rng.randint(1, 4), len(b) - i)
        del b[i:i + n]
    return bytes(b)


def main():
    base_paths = sys.argv[1:]
    if not base_paths:
        print("usage: fuzz_validation.py base.wasm...")
        return 2
    bases = [open(p, "rb").read() for p in base_paths]
    rng = random.Random(20241025)
    n = 4000
    mismatches = 0
    for i in range(n):
        data = mutate(rng.choice(bases), rng)
        p = os.path.join(WORK, "m.wasm")
        open(p, "wb").write(data)
        o = oracle(p)
        g, err = ours(p)
        if o != g:
            mismatches += 1
            save = os.path.join(WORK, f"mismatch{mismatches}.wasm")
            open(save, "wb").write(data)
            print(f"MISMATCH #{mismatches}: oracle={o} ours={g} saved={save}")
            if not o and g:
                print("  oracle rejects, we accept")
            else:
                print("  oracle accepts, we reject:", err)
            if mismatches >= 20:
                break
    print(f"done {i+1} mutants, {mismatches} mismatches")
    return 1 if mismatches else 0


if __name__ == "__main__":
    sys.exit(main())

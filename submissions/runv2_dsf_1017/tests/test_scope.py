#!/usr/bin/env python3
"""Out-of-scope proposal modules must be rejected (as must the wasm2 oracle)."""
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TOOLS = os.environ.get("W2L_TOOLS", "/mnt/workspace/wasm2llvm/deps/bin")
WASM2LLVM = os.environ.get("W2L_BIN", os.path.join(ROOT, "target", "release", "wasm2llvm"))
DIR = os.path.join(ROOT, "tests/wat/scope")
WORK = os.path.join(os.environ["TMPDIR"], "w2l-scope")
os.makedirs(WORK, exist_ok=True)


def build(wat):
    out = os.path.join(WORK, os.path.basename(wat).replace(".wat", ".wasm"))
    for cmd in ([os.path.join(TOOLS, "wat2wasm"), wat, "-o", out],
                [os.path.join(TOOLS, "wasm-tools"), "parse", wat, "-o", out]):
        r = subprocess.run(cmd, capture_output=True)
        if r.returncode == 0:
            return out
    return None


def main():
    ok = True
    names = sorted(f for f in os.listdir(DIR) if f.endswith(".wat"))
    for f in names:
        wasm = build(os.path.join(DIR, f))
        name = f[:-4]
        if wasm is None:
            print(f"{name}: SKIP (cannot assemble)")
            continue
        ours = subprocess.run([WASM2LLVM, wasm, "-o", "/dev/null"], capture_output=True)
        oracle = subprocess.run([os.path.join(TOOLS, "wasm-tools"), "validate", "-f", "wasm2", wasm],
                                capture_output=True)
        good = ours.returncode != 0 and oracle.returncode != 0
        if not good:
            ok = False
        print(f"{name}: ours_reject={ours.returncode != 0} oracle_wasm2_reject={oracle.returncode != 0}")
    print("scope:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())

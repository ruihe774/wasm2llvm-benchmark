#!/usr/bin/env python3
"""Mutation testing: randomly corrupt valid modules and compare the
translator's accept/reject verdict with w2l-ref's, and check that the
translator never panics."""
import os
import random
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
W2L = os.path.join(ROOT, "target", "release", "wasm2llvm")
TMP = os.environ.get("TMPDIR", tempfile.gettempdir())
BASES = [
    os.path.join(ROOT, "payloads", "multi-value", "multi-value.wasm"),
    os.path.join(ROOT, "tests", ".shape.wasm"),
    os.path.join(ROOT, "payloads", "qoa-simd", "qoa-simd.wasm"),
    os.path.join(ROOT, "payloads", "minicelt", "minicelt.wasm"),
    os.path.join(ROOT, "payloads", "minisilk", "minisilk.wasm"),
]


def ensure_bases():
    if not os.path.exists(BASES[1]):
        subprocess.run(["wasm-tools", "parse", os.path.join(ROOT, "tests", "shape.wat"),
                        "-o", BASES[1]], check=True)
    ex = os.path.join(TMP, "mut_example.wasm")
    if not os.path.exists(ex):
        subprocess.run(["wasm-tools", "parse", os.path.join(ROOT, "examples", "abi", "example.wat"),
                        "-o", ex], check=True)
    BASES.append(ex)
    js = os.path.join(TMP, "mut_js.wasm")
    if not os.path.exists(js):
        # small JSON-ish module with a table, bulk memory segments, refs-free code
        wat = """(module
          (memory 1 2) (table 4 funcref)
          (data "hello world") (data (i32.const 32) "active")
          (elem (i32.const 0) $a $b) (elem func $a $b)
          (type $t (func (param i32) (result i32)))
          (func $a (type $t) local.get 0 i32.const 1 i32.add)
          (func $b (type $t) local.get 0 i32.const 2 i32.mul)
          (global $g (mut i32) (i32.const 7))
          (func (export "call") (param i32 i32) (result i32)
            local.get 0 local.get 1 call_indirect (type $t))
          (func (export "mi") (param i32 i32 i32)
            memory.init 0 (local.get 0) (local.get 1) (local.get 2))
          (func (export "ti") (param i32 i32 i32)
            table.init 1 (local.get 0) (local.get 1) (local.get 2))
          (func (export "g") (result i32) global.get $g))
        """
        wp = os.path.join(TMP, "mut_js.wat")
        open(wp, "w").write(wat)
        subprocess.run(["wasm-tools", "parse", wp, "-o", js], check=True)
    BASES.append(js)


def run_w2l(path, out):
    try:
        r = subprocess.run([W2L, path, "-o", out], capture_output=True, text=True, timeout=20)
        return r.returncode, r.stderr
    except subprocess.TimeoutExpired:
        return "timeout", ""


def run_ref(path):
    r = subprocess.run("w2l-ref", input="load a %s\nquit\n" % path,
                       capture_output=True, text=True, timeout=20)
    line = (r.stdout.strip().splitlines() or [""])[0]
    return line.startswith("ok"), line


def main():
    count = int(sys.argv[1]) if len(sys.argv) > 1 else 1000
    start = int(sys.argv[2]) if len(sys.argv) > 2 else 0
    ensure_bases()
    rnd = random.Random(424242 + start)
    workdir = tempfile.mkdtemp(prefix="w2lmut")
    bad = 0
    panics = 0
    accepted = 0
    for i in range(start, start + count):
        base = rnd.choice(BASES)
        data = bytearray(open(base, "rb").read())
        nmut = rnd.choice([1, 1, 1, 2, 3])
        for _ in range(nmut):
            op = rnd.random()
            if op < 0.75 and data:
                pos = rnd.randrange(len(data))
                data[pos] = rnd.randrange(256)
            elif op < 0.9 and data:
                pos = rnd.randrange(len(data))
                del data[pos]
            else:
                pos = rnd.randrange(len(data) + 1)
                data[pos:pos] = bytes([rnd.randrange(256)])
        path = os.path.join(workdir, "m.wasm")
        open(path, "wb").write(data)
        rc, err = run_w2l(path, os.path.join(workdir, "m.ll"))
        if rc == "timeout":
            print("seed %d TIMEOUT" % i)
            bad += 1
            continue
        if "panicked at" in err or "RUST_BACKTRACE" in err or rc not in (0, 1):
            panics += 1
            print("seed %d PANIC rc=%s %s" % (i, rc, err.strip()[:200]))
            open(os.path.join(TMP, "panic_%d.wasm" % i), "wb").write(data)
            bad += 1
            continue
        ours_ok = rc == 0
        ref_ok, refline = run_ref(path)
        if ours_ok:
            accepted += 1
        if ours_ok != ref_ok:
            bad += 1
            kind = "WE-ACCEPT" if ours_ok else "WE-REJECT"
            print("seed %d %s base=%s ours=%s ref=%s %s" % (
                i, kind, os.path.basename(base), "ok" if ours_ok else "err",
                "ok" if ref_ok else "err", refline[:150]))
            if bad <= 20:
                open(os.path.join(TMP, "agree_%d.wasm" % i), "wb").write(data)
        if (i - start) % 200 == 199:
            print("  ... %d/%d done (%d disagreements, %d accepted, %d panics)" % (
                i - start + 1, count, bad, accepted, panics), flush=True)
    print("done: %d mutations, %d disagreements, %d accepted, %d panics" % (
        count, bad, accepted, panics))
    return 0 if bad == 0 else 1


if __name__ == "__main__":
    sys.exit(main())

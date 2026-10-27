#!/usr/bin/env python3
"""Rough performance comparison against the wasmtime-based reference runner."""
import importlib.util
import math
import os
import struct
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
spec = importlib.util.spec_from_file_location("cmp", os.path.join(ROOT, "tests", "compare.py"))
cmp = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cmp)
TMP = os.environ.get("TMPDIR", "/tmp")
HARNESS = os.path.join(ROOT, "tests", ".harness")


def h(s):
    return s.encode().hex()


def si(v):
    return struct.pack("<i", v).hex() + "00" * 12


def run_timed(exe, cmds):
    data = "\n".join(cmds) + "\n"
    t = time.time()
    r = subprocess.run(exe, input=data, capture_output=True, text=True)
    dt = time.time() - t
    return dt, r.stdout.splitlines()


def main():
    cmp.ensure_harness()
    reps = 30
    # minicelt: 2ch, 960 frames
    ns = 1920
    samples = [0.4 * math.sin(i * 0.013) for i in range(ns)]
    data = struct.pack("<%df" % ns, *samples).hex()
    MEM = h("memory")
    cmds = ["load a MODPATH", "init a",
            "invoke a %s 1 %s" % (h("minicelt_init"), " ".join(si(x) for x in (2, 64000, 10, 1, 0)))]
    for _ in range(reps):
        cmds.append("memwrite a %s 19328 %s" % (MEM, data))
        cmds.append("invoke a %s 1" % h("minicelt_encode"))
    cmds.append("quit")
    wasm = os.path.join(ROOT, "payloads", "minicelt", "minicelt.wasm")
    so = cmp.build(wasm, "-O2", TMP)
    ref_cmds = [c.replace("MODPATH", wasm) for c in cmds]
    our_cmds = [c.replace("MODPATH", so) for c in cmds]
    dt_ref, out_ref = run_timed("w2l-ref", ref_cmds)
    dt_our, out_our = run_timed(HARNESS, our_cmds)
    ok = out_ref == out_our
    print("minicelt encode x%d: ref %.3fs ours %.3fs ratio %.2fx match=%s" %
          (reps, dt_ref, dt_our, dt_our / dt_ref if dt_ref else 0, ok))

    # qoa-simd decode repeated
    cmds = ["load a MODPATH", "init a"]
    # encode a frame then decode it many times
    gen = subprocess.run([sys.executable, "-I", os.path.join(ROOT, "tests", "gen_qoa_cmds.py")],
                         capture_output=True, text=True).stdout.splitlines()
    # take commands from gen up to the first memread as setup
    setup = gen[:7]
    dst = None
    for line in setup:
        if "memread" in line:
            dst = int(line.split()[3])
    cmds = setup + ["invoke a %s 1 %s" % (h("decode_frame"),
                    " ".join(si(x) for x in (dst, 1276, 40000)))] * reps
    cmds.append("quit")
    wasm = os.path.join(ROOT, "payloads", "qoa-simd", "qoa-simd.wasm")
    so = cmp.build(wasm, "-O2", TMP)
    dt_ref, out_ref = run_timed("w2l-ref", [c.replace("MODPATH", wasm) for c in cmds])
    dt_our, out_our = run_timed(HARNESS, [c.replace("MODPATH", so) for c in cmds])
    print("qoa decode x%d: ref %.3fs ours %.3fs ratio %.2fx match=%s" %
          (reps, dt_ref, dt_our, dt_our / dt_ref if dt_ref else 0, out_ref == out_our))
    return 0


if __name__ == "__main__":
    sys.exit(main())

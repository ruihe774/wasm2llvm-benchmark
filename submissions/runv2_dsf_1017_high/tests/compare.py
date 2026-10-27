#!/usr/bin/env python3
"""Translate a wasm module, build it at -O0/-O2, and compare the ABI behaviour
against the w2l-ref reference runner.

Usage:
  python3 -I tests/compare.py MODULE.wasm [-c COMMAND | -f CMDFILE] [--keep]

Commands use the literal token MODPATH where the module path should go.
Default commands: load a MODPATH / init a / quit
"""
import os
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
W2L = os.path.join(ROOT, "target", "release", "wasm2llvm")
TMP = os.environ.get("TMPDIR", tempfile.gettempdir())
HARNESS = os.path.join(ROOT, "tests", ".harness")


def sh(cmd, **kw):
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def ensure_harness():
    src = os.path.join(ROOT, "tests", "harness.c")
    if not os.path.exists(HARNESS) or os.path.getmtime(src) > os.path.getmtime(HARNESS):
        r = sh(["clang", "-O1", "-rdynamic", "-o", HARNESS, src, "-ldl"])
        if r.returncode != 0:
            print("harness build failed:\n" + r.stderr)
            sys.exit(1)


def translate(wasm, ll):
    r = sh([W2L, wasm, "-o", ll])
    if r.returncode != 0:
        return r.stderr.strip()
    return None


def build(wasm, opt, workdir):
    base = os.path.basename(wasm).rsplit(".", 1)[0]
    ll = os.path.join(workdir, base + ".ll")
    so = os.path.join(workdir, base + "." + opt + ".so")
    err = translate(wasm, ll)
    if err:
        raise RuntimeError("translate: " + err)
    r = sh(["clang", opt, "-shared", "-fPIC", "-Wl,-Bsymbolic", "-o", so, ll, "-lm"])
    if r.returncode != 0:
        raise RuntimeError("clang " + opt + ":\n" + r.stderr)
    return so


def run_stdin(exe, cmds):
    r = subprocess.run(exe, input="\n".join(cmds) + "\n", capture_output=True, text=True)
    if r.returncode != 0:
        return ["<crashed rc=%d>" % r.returncode] + r.stdout.splitlines() + r.stderr.splitlines()
    return r.stdout.splitlines()


def ref_run(wasm, cmds):
    lines = [c.replace("MODPATH", wasm) for c in cmds]
    return run_stdin("w2l-ref", lines)


def our_run(so, cmds):
    lines = [c.replace("MODPATH", so) for c in cmds]
    return run_stdin(HARNESS, lines)


def compare(wasm, cmds, opts=("-O0", "-O2"), keep=False, verbose=False):
    ensure_harness()
    workdir = tempfile.mkdtemp(prefix="w2lcmp")
    try:
        ref = ref_run(wasm, cmds)
        ok = True
        for opt in opts:
            so = build(wasm, opt, workdir)
            ours = our_run(so, cmds)
            if ours != ref:
                ok = False
                print("MISMATCH (%s) %s" % (opt, wasm))
                for i in range(max(len(ref), len(ours))):
                    r = ref[i] if i < len(ref) else "<missing>"
                    o = ours[i] if i < len(ours) else "<missing>"
                    if r != o:
                        def trunc(x):
                            return x if len(x) <= 300 else x[:300] + "...<%d chars>" % len(x)
                        print("  line %d:\n    ref : %s\n    ours: %s" % (i + 1, trunc(r), trunc(o)))
                if verbose:
                    print("  commands:", [c if len(c) < 200 else c[:200] + "..." for c in cmds])
            elif verbose:
                print("ok %s %s" % (opt, wasm))
        return ok
    finally:
        if not keep:
            shutil.rmtree(workdir, ignore_errors=True)


def main():
    args = sys.argv[1:]
    if not args:
        print(__doc__)
        return 2
    wasm = args[0]
    cmds = ["load a MODPATH", "init a", "quit"]
    keep = False
    i = 1
    while i < len(args):
        if args[i] == "-c":
            i += 1
            cmds = [args[i]]
        elif args[i] == "-f":
            i += 1
            cmds = [l.rstrip("\n") for l in open(args[i]) if l.strip()]
        elif args[i] == "--keep":
            keep = True
        i += 1
    ok = compare(wasm, cmds, keep=keep, verbose=True)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())

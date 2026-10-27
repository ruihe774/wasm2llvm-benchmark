#!/usr/bin/env python3
"""Run every test harness in this directory."""
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TESTS = [
    ("features", "features_test.py"),
    ("float", "float_test.py"),
    ("simd", "simd_test.py"),
    ("imports", "imports_test.py"),
    ("memory", "mem_test.py"),
    ("dup-types", "dup_types_test.py"),
    ("misc", "misc_test.py"),
    ("validation", "valid_test.py"),
]


def main():
    env = dict(os.environ)
    fail = 0
    for name, script in TESTS:
        r = subprocess.run([sys.executable, "-I", os.path.join(ROOT, "tests", script)],
                           capture_output=True, text=True, env=env, cwd=ROOT)
        status = "PASS" if r.returncode == 0 else "FAIL"
        print("[%s] %s" % (status, name))
        if r.returncode != 0:
            fail += 1
            tail = (r.stdout + r.stderr).splitlines()[-12:]
            for line in tail:
                print("    " + line[:200])
    print("%d/%d test groups passed" % (len(TESTS) - fail, len(TESTS)))
    return 0 if fail == 0 else 1


if __name__ == "__main__":
    sys.exit(main())

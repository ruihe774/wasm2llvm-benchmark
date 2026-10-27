#!/usr/bin/env python3
"""Insert call-sequence logging into every function of a WAT module.

Log area: page 2 onward (address 131072), record stride 32 bytes:
  [funcidx, arg0, arg1, arg2, arg3]  (i32 each)
The memory minimum is raised to `pages` so the area is private.
Usage: instrument_calls.py IN.wat OUT.wat [pages] [base]
"""
import re
import sys

def prologue(idx, ptypes, base, stride):
    ins = []
    rbase = base + 64
    def addr():
        ins.append(f"i32.const {rbase}")
        ins.append(f"i32.const {base}")
        ins.append("i32.load")
        ins.append(f"i32.const {stride}")
        ins.append("i32.mul")
        ins.append("i32.add")
    addr()
    ins.append(f"i32.const {idx}")
    ins.append("i32.store")
    for k, t in enumerate(ptypes[:4]):
        addr()
        ins.append(f"local.get {k}")
        if t == "i64":
            ins.append("i32.wrap_i64")
        elif t == "f32":
            ins.append("i32.reinterpret_f32")
        elif t == "f64":
            ins.append("i64.reinterpret_f64")
            ins.append("i32.wrap_i64")
        elif t == "v128":
            # leave as zero: skip logging this arg
            ins.pop()  # remove local.get
            # remove the address computation we just added
            del ins[-6:]
            continue
        ins.append(f"i32.store offset={4 * (k + 1)}")
    ins.append(f"i32.const {base}")
    ins.append(f"i32.const {base}")
    ins.append("i32.load")
    ins.append("i32.const 1")
    ins.append("i32.add")
    ins.append("i32.store")
    return ["    " + s for s in ins]


def main():
    src, dst = sys.argv[1], sys.argv[2]
    pages = int(sys.argv[3]) if len(sys.argv) > 3 else 32
    base = int(sys.argv[4]) if len(sys.argv) > 4 else 131072
    stride = 32
    lines = open(src).read().split("\n")
    out = []
    i = 0
    nfunc = 0
    while i < len(lines):
        line = lines[i]
        m = re.match(r"^  \(func \(;(\d+);\) \(type \d+\)(.*)$", line)
        if m:
            idx = int(m.group(1))
            rest = m.group(2)
            params = re.findall(r"\(param ([^)]*)\)", rest)
            ptypes = params[0].split() if params else []
            out.append(line)
            i += 1
            # optional local declaration line
            if i < len(lines) and lines[i].lstrip().startswith("(local "):
                out.append(lines[i])
                i += 1
            out.extend(prologue(idx, ptypes, base, stride))
            nfunc += 1
            continue
        if re.match(r"^  \(memory \(;0;\) \d+\)$", line):
            out.append(f"  (memory (;0;) {pages})")
            i += 1
            continue
        out.append(line)
        i += 1
    open(dst, "w").write("\n".join(out))
    print(f"instrumented {nfunc} functions", file=sys.stderr)


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Insert call-site logging before every direct `call` instruction.

Record layout at base+64 + n*16: [callee, caller, line] as i32.
Memory minimum raised to `pages`.
"""
import re
import sys

def main():
    src, dst = sys.argv[1], sys.argv[2]
    pages = int(sys.argv[3]) if len(sys.argv) > 3 else 32
    base = int(sys.argv[4]) if len(sys.argv) > 4 else 131072
    stride = 16
    rbase = base + 64
    lines = open(src).read().split("\n")
    out = []
    cur_func = None
    nsite = 0
    for lineno, line in enumerate(lines, 1):
        m = re.match(r"^  \(func \(;(\d+);\)", line)
        if m:
            cur_func = int(m.group(1))
        if re.match(r"^  \(memory \(;0;\) \d+\)$", line):
            out.append(f"  (memory (;0;) {pages})")
            continue
        m = re.match(r"^(\s*)call (\d+)$", line)
        if m and cur_func is not None:
            indent, callee = m.group(1), int(m.group(2))
            ins = []
            def addr():
                ins.append(f"i32.const {rbase}")
                ins.append(f"i32.const {base}")
                ins.append("i32.load")
                ins.append(f"i32.const {stride}")
                ins.append("i32.mul")
                ins.append("i32.add")
            addr(); ins.append(f"i32.const {callee}"); ins.append("i32.store")
            addr(); ins.append(f"i32.const {cur_func}"); ins.append("i32.store offset=4")
            addr(); ins.append(f"i32.const {lineno}"); ins.append("i32.store offset=8")
            ins.append(f"i32.const {base}")
            ins.append(f"i32.const {base}")
            ins.append("i32.load")
            ins.append("i32.const 1")
            ins.append("i32.add")
            ins.append("i32.store")
            for s in ins:
                out.append(indent + s)
            nsite += 1
        out.append(line)
    open(dst, "w").write("\n".join(out))
    print(f"instrumented {nsite} call sites", file=sys.stderr)


if __name__ == "__main__":
    main()

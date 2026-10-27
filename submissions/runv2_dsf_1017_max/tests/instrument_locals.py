#!/usr/bin/env python3
"""Log selected local values before every instruction of one function.

Records at base+64 + n*stride: [line, l0, l1, ...] (i32).
Usage: instrument_locals.py IN.wat OUT.wat FUNCINDEX LOCALS [pages] [base]
where LOCALS is a comma separated list of local indices.
"""
import re
import sys

def main():
    src, dst = sys.argv[1], sys.argv[2]
    funcidx = int(sys.argv[3])
    locs = [int(x) for x in sys.argv[4].split(",")]
    pages = int(sys.argv[5]) if len(sys.argv) > 5 else 512
    base = int(sys.argv[6]) if len(sys.argv) > 6 else 131072
    nvals = 1 + len(locs)
    stride = ((nvals * 4 + 31) // 32) * 32
    rbase = base + 64
    lines = open(src).read().split("\n")
    out = []
    cur_func = None
    in_func = False
    nsite = 0
    for lineno, line in enumerate(lines, 1):
        m = re.match(r"^  \(func \(;(\d+);\)", line)
        if m:
            in_func = int(m.group(1)) == funcidx
        if re.match(r"^  \(memory \(;0;\) \d+\)$", line):
            out.append(f"  (memory (;0;) {pages})")
            continue
        stripped = line.strip()
        # is this an instruction line inside the target function?
        is_instr = (
            in_func
            and lineno > 0
            and stripped != ""
            and not stripped.startswith("(func ")
            and not stripped.startswith("(local ")
            and not stripped.startswith(")")
            and not stripped.startswith(";;")
        )
        if is_instr:
            indent = line[: len(line) - len(line.lstrip())]
            ins = []
            def addr():
                ins.append(f"i32.const {rbase}")
                ins.append(f"i32.const {base}")
                ins.append("i32.load")
                ins.append(f"i32.const {stride}")
                ins.append("i32.mul")
                ins.append("i32.add")
            addr(); ins.append(f"i32.const {lineno}"); ins.append("i32.store")
            for k, l in enumerate(locs):
                addr(); ins.append(f"local.get {l}")
                ins.append(f"i32.store offset={4 * (k + 1)}")
            ins.append(f"i32.const {base}")
            ins.append(f"i32.const {base}")
            ins.append("i32.load")
            ins.append("i32.const 1")
            ins.append("i32.add")
            ins.append("i32.store")
            for s in ins:
                out.append(indent + s)
        out.append(line)
    open(dst, "w").write("\n".join(out))
    print(f"instrumented function {funcidx}", file=sys.stderr)


if __name__ == "__main__":
    main()

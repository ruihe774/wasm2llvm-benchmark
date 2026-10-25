#!/usr/bin/env python3
"""Reference *baseline* translator for validating the benchmark pipeline (hidden).

  w2c_translator.py IN.wasm -o OUT.ll

Uses wasm2c + clang -emit-llvm + generated w2l ABI glue. This is exactly the
approach agents are forbidden to use; it exists only to sanity-check the harness,
ABI, test data and verifier, and to give a baseline score. Known deviations:
wasm2c reports table OOB as memory OOB (trap 4 instead of 5) and folds null /
signature-mismatch call_indirect into one trap (reported as 7), and it does not
validate as strictly as the spec requires.
"""

import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
TP = HERE / "third_party"
BIN = ROOT / "deps" / "bin"
WASM2C = ROOT / "deps" / "toolchains" / "wabt-1.0.42" / "bin" / "wasm2c"
RT = TP / "wabt-src" / "wasm2c"
DEFS = ["-DWASM_RT_USE_MMAP=0", "-DWASM_RT_MEMCHECK_BOUNDS_CHECK=1", "-DWASM_RT_STACK_DEPTH_COUNT=1",
        "-DWASM_RT_MAX_CALL_STACK_DEPTH=40000", "-DWASM_RT_SKIP_SIGNAL_RECOVERY=1"]
INC = [f"-I{RT}", f"-I{TP / 'simde'}"]

CSIZE = {"u32": 4, "s32": 4, "u64": 8, "s64": 8, "f32": 4, "f64": 8, "v128": 16}
TRAPMAP = {"WASM_RT_TRAP_OOB": 4, "WASM_RT_TRAP_INT_OVERFLOW": 2, "WASM_RT_TRAP_DIV_BY_ZERO": 1,
           "WASM_RT_TRAP_INVALID_CONVERSION": 3, "WASM_RT_TRAP_UNREACHABLE": 0, "WASM_RT_TRAP_CALL_INDIRECT": 7,
           "WASM_RT_TRAP_NULL_REF": 6, "WASM_RT_TRAP_EXHAUSTION": 8}


def export_names(wasm):
    b = Path(wasm).read_bytes()
    pos, out = 8, []

    def leb(p):
        r = s = 0
        while True:
            x = b[p]; p += 1
            r |= (x & 0x7F) << s; s += 7
            if x < 0x80:
                return r, p

    while pos < len(b):
        sid = b[pos]; pos += 1
        size, pos = leb(pos)
        if sid == 7:
            p = pos
            n, p = leb(p)
            for _ in range(n):
                ln, p = leb(p)
                name = b[p:p + ln]; p += ln
                kind = b[p]; p += 1
                _, p = leb(p)
                out.append((name, kind))
        pos += size
    return out


def parse_header(h):
    lines = h.splitlines()
    structs = {}
    cur = None
    for ln in lines:
        m = re.match(r"struct (wasm_multi_\w+) \{", ln)
        if m:
            cur = m.group(1); structs[cur] = []
            continue
        if cur:
            if ln.startswith("}"):
                cur = None
            else:
                t, n = ln.strip().rstrip(";").split()
                structs[cur].append((t, n))
    imports, exports, inst_params = [], [], None
    for i, ln in enumerate(lines):
        m = re.match(r"/\* import: '(.*)' '(.*)' \*/", ln)
        if m:
            imports.append((m.group(1).encode().hex(), m.group(2).encode().hex(), lines[i + 1]))
        if ln.startswith("/* export: "):
            exports.append(lines[i + 1])
        m = re.match(r"void wasm2c_m_instantiate\((.*)\);", ln)
        if m:
            inst_params = [p.strip() for p in m.group(1).split(",")]
    return structs, imports, exports, inst_params


DECL = re.compile(r"^(.*?)\s*\b(w2c_\w+)\((.*)\);$")


def gen_glue(wasm, header):
    structs, imports, exports, inst_params = parse_header(header)
    names = export_names(wasm)
    if len(names) != len(exports):
        raise SystemExit(f"export count mismatch {len(names)} vs {len(exports)}")
    g = ['#include <stdint.h>', '#include <string.h>', '#include "m.h"',
         'extern void w2l_trap(int32_t) __attribute__((noreturn));',
         'static w2c_m inst;',
         'extern WASM_RT_THREAD_LOCAL uint32_t wasm_rt_call_stack_depth;',
         'void wasm_rt_trap(wasm_rt_trap_t c) {', '  int code = 9;', '  switch (c) {']
    for k, v in TRAPMAP.items():
        g.append(f'    case {k}: code = {v}; break;')
    g += ['    default: break;', '  }', '  wasm_rt_call_stack_depth = 0;', '  w2l_trap(code);', '}']

    def store_ret(rt, var):
        if rt == "void":
            return []
        if rt.startswith("struct "):
            out = []
            for i, (t, n) in enumerate(structs[rt.split()[1]]):
                out.append(f'  memcpy((char*)res + {16 * i}, &{var}.{n}, {CSIZE[t]});')
            return out
        return [f'  memcpy(res, &{var}, {CSIZE[rt]});']

    # imports
    for mhex, nhex, decl in imports:
        m = DECL.match(decl)
        rt, fname, params = m.group(1), m.group(2), [p.strip() for p in m.group(3).split(",")]
        ptypes = params[1:]
        sym = f"w2l_import_{mhex}_{nhex}"
        g.append(f'extern void {sym}(void*, void*);')
        plist = ", ".join([f"{params[0]} self"] + [f"{t} a{i}" for i, t in enumerate(ptypes)])
        g.append(f'{rt} {fname}({plist}) {{')
        g.append(f'  _Alignas(16) unsigned char args[{16 * len(ptypes) + 16}] = {{0}}, res[16 * 16] = {{0}};')
        for i, t in enumerate(ptypes):
            g.append(f'  memcpy(args + {16 * i}, &a{i}, {CSIZE[t]});')
        g.append(f'  {sym}(args, res);')
        if rt != "void":
            g.append(f'  {rt} r;')
            if rt.startswith("struct "):
                for i, (t, n) in enumerate(structs[rt.split()[1]]):
                    g.append(f'  memcpy(&r.{n}, res + {16 * i}, {CSIZE[t]});')
            else:
                g.append(f'  memcpy(&r, res, {CSIZE[rt]});')
            g.append('  return r;')
        g.append('}')
    # exports
    for (name, kind), decl in zip(names, exports):
        hx = name.hex()
        m = DECL.match(decl)
        rt, fname, params = m.group(1), m.group(2), [p.strip() for p in m.group(3).split(",")]
        if kind == 0:
            ptypes = params[1:]
            g.append(f'void w2l_export_{hx}(void* args, void* res) {{')
            for i, t in enumerate(ptypes):
                g.append(f'  {t} a{i}; memcpy(&a{i}, (char*)args + {16 * i}, {CSIZE[t]});')
            g.append('  wasm_rt_call_stack_depth = 0;')
            call = f'{fname}(' + ", ".join(["&inst"] + [f"a{i}" for i in range(len(ptypes))]) + ')'
            if rt == "void":
                g.append(f'  {call};')
            else:
                g.append(f'  {rt} r = {call};')
                g += store_ret(rt, "r")
            g.append('}')
        elif kind == 3:
            t = rt.rstrip("*").strip()
            g.append(f'void w2l_global_{hx}(void* out) {{ memcpy(out, {fname}(&inst), {CSIZE[t]}); }}')
        elif kind == 2:
            g.append(f'uint8_t* w2l_memory_{hx}(uint64_t* size) {{ wasm_rt_memory_t* m = {fname}(&inst); '
                     f'*size = m->size; return m->data; }}')
    nulls = ", ".join(["&inst"] + ["NULL"] * (len(inst_params) - 1))
    g += ['void w2l_init(void) {', '  wasm_rt_init();', '  wasm_rt_call_stack_depth = 0;',
          f'  wasm2c_m_instantiate({nulls});', '}']
    return "\n".join(g) + "\n"


def main():
    inp, out = sys.argv[1], sys.argv[3]
    clang, link = str(BIN / "clang"), str(BIN / "llvm-link")
    with tempfile.TemporaryDirectory() as td:
        td = Path(td)
        r = subprocess.run([str(WASM2C), "-n", "m", inp, "-o", str(td / "m.c")], capture_output=True, text=True)
        if r.returncode:
            sys.stderr.write(r.stderr)
            return 1
        glue = gen_glue(inp, (td / "m.h").read_text())
        (td / "glue.c").write_text(glue)
        bcs = []
        for src, extra in [(td / "m.c", []), (td / "glue.c", []),
                           (RT / "wasm-rt-impl.c", ["-Dwasm_rt_trap=w2c_unused_rt_trap"]),
                           (RT / "wasm-rt-mem-impl.c", [])]:
            bc = td / (src.stem + ".bc")
            r = subprocess.run([clang, "-O2", "-fPIC", "-c", "-emit-llvm", "-w", *DEFS, *INC, *extra, str(src),
                                "-o", str(bc)], capture_output=True, text=True)
            if r.returncode:
                sys.stderr.write(r.stderr[-3000:])
                return 1
            bcs.append(str(bc))
        r = subprocess.run([link, "-S", *bcs, "-o", out], capture_output=True, text=True)
        if r.returncode:
            sys.stderr.write(r.stderr)
            return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())

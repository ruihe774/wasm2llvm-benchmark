#!/usr/bin/env python3
"""Generate differential test scripts from random wasm-smith modules.

  gen_smith.py OUT_DIR FIRST_SEED COUNT

Each test: a wasm-smith module restricted to the benchmark feature set
(fuel-bounded, NaN-canonicalized, no imports, everything exported), invoked with
random/interesting arguments; expectations are recorded with the reference
runner (wasmtime). Scripts are truncated at the first stack exhaustion because
fuel-vs-stack-depth ordering is implementation-defined.
"""

import concurrent.futures as cf
import json
import random
import shutil
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

sys.dont_write_bytecode = True  # keep the harness pristine
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "verification" / "harness"))
import w2l  # noqa: E402

SMITH_FLAGS = [
    "--ensure-termination", "--canonicalize-nans", "true",
    "--max-imports", "0", "--export-everything", "true",
    "--bulk-memory-enabled", "true", "--multi-value-enabled", "true", "--simd-enabled", "true",
    "--saturating-float-to-int-enabled", "true", "--sign-extension-ops-enabled", "true",
    "--reference-types-enabled", "false", "--relaxed-simd-enabled", "false", "--gc-enabled", "false",
    "--exceptions-enabled", "false", "--tail-call-enabled", "false", "--threads-enabled", "false",
    "--memory64-enabled", "false", "--wide-arithmetic-enabled", "false", "--extended-const-enabled", "false",
    "--custom-page-sizes-enabled", "false", "--shared-everything-threads-enabled", "false",
    "--max-memories", "1", "--max-tables", "1", "--max-memory32-bytes", str(16 * 65536),
    "--max-table-elements", "200", "--max-funcs", "40", "--max-instructions", "2000",
    "--generate-custom-sections", "true", "--allow-start-export", "true", "--min-funcs", "3",
]

INTERESTING = {
    "i32": [0, 1, 2, 0x7FFFFFFF, 0x80000000, 0xFFFFFFFF, 0xFFFFFFFE, 31, 32, 0x10000, 0xFFFF],
    "i64": [0, 1, 0x7FFFFFFFFFFFFFFF, 0x8000000000000000, 0xFFFFFFFFFFFFFFFF, 63, 64, 0x100000000, 0xFFFFFFFF],
    "f32": [0, 0x80000000, 0x3F800000, 0xBF800000, 0x7F800000, 0xFF800000, 0x7FC00000, 0x00000001,
            0x4F000000, 0xCF000000, 0x3F000000, 0x40490FDB],
    "f64": [0, 0x8000000000000000, 0x3FF0000000000000, 0xBFF0000000000000, 0x7FF0000000000000,
            0xFFF0000000000000, 0x7FF8000000000000, 1, 0x43E0000000000000, 0xC3E0000000000000, 0x3FE0000000000000],
}


def rand_value(rng, t):
    if t == "v128":
        lt = rng.choice(["i8", "i16", "i32", "i64"])
        n = {"i8": 16, "i16": 8, "i32": 4, "i64": 2}[lt]
        bits = {"i8": 8, "i16": 16, "i32": 32, "i64": 64}[lt]
        lanes = [rng.choice([0, 1, (1 << bits) - 1, 1 << (bits - 1), rng.getrandbits(bits)]) for _ in range(n)]
        return {"type": "v128", "lane_type": lt, "value": [str(x) for x in lanes]}
    width = 32 if t in ("i32", "f32") else 64
    if rng.random() < 0.5:
        v = rng.choice(INTERESTING[t])
    elif t.startswith("f") and rng.random() < 0.5:
        v = struct.unpack("<I", struct.pack("<f", rng.uniform(-1e4, 1e4)))[0] if width == 32 else \
            struct.unpack("<Q", struct.pack("<d", rng.uniform(-1e9, 1e9)))[0]
    else:
        v = rng.getrandbits(width) >> rng.choice([0, 0, 0, width // 2, width - 4])
    return {"type": t, "value": str(v)}


def _params(b):
    """Parse type section param lists."""
    pos = 8
    out = []

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
        if sid == 1:
            p = pos
            n, p = leb(p)
            for _ in range(n):
                p += 1
                np_, p = leb(p)
                out.append([w2l.ResultTypes.VT.get(x, "?") for x in b[p:p + np_]]); p += np_
                nr, p = leb(p); p += nr
        pos += size
    return out


def gen_one(seed, out_root, work):
    rng = random.Random(seed)
    tmp = Path(tempfile.mkdtemp(dir=work))
    flags = list(SMITH_FLAGS)
    if rng.random() < 0.2:
        flags += ["--min-uleb-size", str(rng.choice([2, 3, 5]))]
    wasm = tmp / f"smith{seed}.wasm"
    seedfile = tmp / "seed.bin"
    seedfile.write_bytes(rng.randbytes(rng.choice([10000, 40000, 120000])))
    r = subprocess.run(["wasm-tools", "smith", *flags, str(seedfile), "-o", str(wasm)], capture_output=True, text=True)
    if r.returncode != 0:
        shutil.rmtree(tmp)
        return seed, f"smith failed: {r.stderr[:200]}"
    v = subprocess.run(["wasm-tools", "validate", w2l.FEATURES_ARG, str(wasm)], capture_output=True, text=True)
    if v.returncode != 0:
        shutil.rmtree(tmp)
        return seed, f"invalid: {v.stderr[:200]}"
    rt = w2l.ResultTypes(tmp)
    types, funcs, globs, exports = rt._parse(wasm)
    params = _params(wasm.read_bytes())
    cmds = [{"type": "module", "line": 1, "filename": wasm.name}]
    line = 2
    fexports = [(n, i) for n, (k, i) in exports.items() if k == 0]
    for _round in range(2):
        for name, fidx in fexports:
            ps = params[funcs[fidx]]
            if any(p not in ("i32", "i64", "f32", "f64", "v128") for p in ps + types[funcs[fidx]]):
                continue
            for _ in range(rng.choice([1, 2, 3])):
                cmds.append({"type": "action", "line": line,
                             "action": {"type": "invoke", "field": name, "args": [rand_value(rng, p) for p in ps]}})
                line += 1
        for name, (k, i) in exports.items():
            if k == 3 and globs[i] in ("i32", "i64", "f32", "f64", "v128"):
                cmds.append({"type": "action", "line": line, "action": {"type": "get", "field": name}})
                line += 1
            elif k == 2:
                cmds.append({"type": "record_memhash", "line": line, "memory": name})
                line += 1
    raw = tmp / "_raw.json"
    raw.write_text(json.dumps({"commands": cmds}))
    rec = tmp / "rec.json"
    try:
        w2l.record_script(raw, rec, work / f"w{seed}")
    except SystemExit as e:
        shutil.rmtree(tmp)
        return seed, f"record failed: {e}"
    data = json.loads(rec.read_text())
    out_cmds = []
    for c in data["commands"]:
        if c["type"] == "assert_exhaustion":
            break
        out_cmds.append(c)
    if len(out_cmds) < 2:
        shutil.rmtree(tmp)
        return seed, "no usable commands"
    out = out_root / f"smith{seed}"
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)
    shutil.copy(wasm, out / wasm.name)
    (out / f"smith{seed}.json").write_text(json.dumps({"source_filename": wasm.name, "commands": out_cmds}, indent=1))
    shutil.rmtree(tmp)
    return seed, f"ok {len(out_cmds) - 1} commands"


def main():
    out_root = Path(sys.argv[1]).resolve()
    first, count = int(sys.argv[2]), int(sys.argv[3])
    out_root.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="smith-"))
    ok = 0
    with cf.ThreadPoolExecutor(max_workers=16) as ex:
        for seed, msg in ex.map(lambda s: gen_one(s, out_root, work), range(first, first + count)):
            ok += msg.startswith("ok")
            if not msg.startswith("ok"):
                print(seed, msg)
    shutil.rmtree(work, ignore_errors=True)
    print(f"generated {ok}/{count}")


if __name__ == "__main__":
    main()

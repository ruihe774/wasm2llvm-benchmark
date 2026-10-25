#!/usr/bin/env python3
"""Generate one tiny module per SIMD instruction for the SIMD lowering-quality check.

  gen_simd_quality.py OUT_DIR

Writes OUT_DIR/<name>/<name>.{wasm,json} and OUT_DIR/_manifest.json:
[{"op", "file", "script", "allow"}]. `allow` is the maximum number of
extractelement+insertelement instructions tolerated in the module after
`opt -O2` (a scalarized lowering needs >= 6); `scalar_ok` marks ops that
may lose all vector types (see scalar_ok()). The script holds random
invocations of the op with expectations recorded from wasmtime; an op only
counts as well-lowered if its script also passes.
"""

import json
import random
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

sys.dont_write_bytecode = True  # keep agent/harness pristine
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "agent" / "harness"))
import w2l  # noqa: E402

CMP = ["eq", "ne", "lt_s", "lt_u", "gt_s", "gt_u", "le_s", "le_u", "ge_s", "ge_u"]
FCMP = ["eq", "ne", "lt", "gt", "le", "ge"]

ops = []  # (op, signature kind, allow)


def add(op, kind, allow=2):
    ops.append((op, kind, allow))


for o in CMP + ["add", "add_sat_s", "add_sat_u", "sub", "sub_sat_s", "sub_sat_u", "min_s", "min_u", "max_s", "max_u",
                "avgr_u", "narrow_i16x8_s", "narrow_i16x8_u"]:
    add(f"i8x16.{o}", "vv_v")
for o in CMP + ["add", "add_sat_s", "add_sat_u", "sub", "sub_sat_s", "sub_sat_u", "mul", "min_s", "min_u", "max_s",
                "max_u", "avgr_u", "narrow_i32x4_s", "narrow_i32x4_u", "q15mulr_sat_s", "extmul_low_i8x16_s",
                "extmul_low_i8x16_u", "extmul_high_i8x16_s", "extmul_high_i8x16_u"]:
    add(f"i16x8.{o}", "vv_v")
for o in CMP + ["add", "sub", "mul", "min_s", "min_u", "max_s", "max_u", "dot_i16x8_s", "extmul_low_i16x8_s",
                "extmul_low_i16x8_u", "extmul_high_i16x8_s", "extmul_high_i16x8_u"]:
    add(f"i32x4.{o}", "vv_v")
for o in ["eq", "ne", "lt_s", "gt_s", "le_s", "ge_s", "add", "sub", "mul", "extmul_low_i32x4_s", "extmul_low_i32x4_u",
          "extmul_high_i32x4_s", "extmul_high_i32x4_u"]:
    add(f"i64x2.{o}", "vv_v")
for s in ["f32x4", "f64x2"]:
    for o in FCMP + ["add", "sub", "mul", "div", "min", "max", "pmin", "pmax"]:
        add(f"{s}.{o}", "vv_v")
for o in ["and", "andnot", "or", "xor"]:
    add(f"v128.{o}", "vv_v")
add("v128.bitselect", "vvv_v")
add("i8x16.swizzle", "vv_v", allow=None)  # informational: no portable dynamic shuffle in LLVM IR

for o in ["v128.not", "i8x16.abs", "i8x16.neg", "i8x16.popcnt",
          "i16x8.abs", "i16x8.neg", "i16x8.extadd_pairwise_i8x16_s", "i16x8.extadd_pairwise_i8x16_u",
          "i16x8.extend_low_i8x16_s", "i16x8.extend_low_i8x16_u", "i16x8.extend_high_i8x16_s", "i16x8.extend_high_i8x16_u",
          "i32x4.abs", "i32x4.neg", "i32x4.extadd_pairwise_i16x8_s", "i32x4.extadd_pairwise_i16x8_u",
          "i32x4.extend_low_i16x8_s", "i32x4.extend_low_i16x8_u", "i32x4.extend_high_i16x8_s", "i32x4.extend_high_i16x8_u",
          "i32x4.trunc_sat_f32x4_s", "i32x4.trunc_sat_f32x4_u", "i32x4.trunc_sat_f64x2_s_zero", "i32x4.trunc_sat_f64x2_u_zero",
          "i64x2.abs", "i64x2.neg", "i64x2.extend_low_i32x4_s", "i64x2.extend_low_i32x4_u", "i64x2.extend_high_i32x4_s",
          "i64x2.extend_high_i32x4_u",
          "f32x4.abs", "f32x4.neg", "f32x4.sqrt", "f32x4.ceil", "f32x4.floor", "f32x4.trunc", "f32x4.nearest",
          "f32x4.convert_i32x4_s", "f32x4.convert_i32x4_u", "f32x4.demote_f64x2_zero",
          "f64x2.abs", "f64x2.neg", "f64x2.sqrt", "f64x2.ceil", "f64x2.floor", "f64x2.trunc", "f64x2.nearest",
          "f64x2.convert_low_i32x4_s", "f64x2.convert_low_i32x4_u", "f64x2.promote_low_f32x4"]:
    add(o, "v_v")
for s in ["i8x16", "i16x8", "i32x4", "i64x2"]:
    for o in ["shl", "shr_s", "shr_u"]:
        add(f"{s}.{o}", "vi_v", allow=3)
add("v128.any_true", "v_i")
for s in ["i8x16", "i16x8", "i32x4", "i64x2"]:
    add(f"{s}.all_true", "v_i")
    add(f"{s}.bitmask", "v_i")
SCAL = {"i8x16": "i32", "i16x8": "i32", "i32x4": "i32", "i64x2": "i64", "f32x4": "f32", "f64x2": "f64"}
LANES = {"i8x16": 16, "i16x8": 8, "i32x4": 4, "i64x2": 2, "f32x4": 4, "f64x2": 2}
for s, t in SCAL.items():
    add(f"{s}.splat", f"splat:{t}", allow=3)
    ext = ["extract_lane_s", "extract_lane_u"] if s in ("i8x16", "i16x8") else ["extract_lane"]
    for e in ext:
        add(f"{s}.{e}", f"extract:{t}:{LANES[s] - 1}", allow=3)
    add(f"{s}.replace_lane", f"replace:{t}:{LANES[s] // 2}", allow=3)
add("i8x16.shuffle", "shuffle")
for o in ["v128.load", "v128.load8x8_s", "v128.load8x8_u", "v128.load16x4_s", "v128.load16x4_u", "v128.load32x2_s",
          "v128.load32x2_u"]:
    add(o, "load")
for o in ["v128.load8_splat", "v128.load16_splat", "v128.load32_splat", "v128.load64_splat", "v128.load32_zero",
          "v128.load64_zero"]:
    add(o, "load", allow=3)
for o, lane in [("v128.load8_lane", 7), ("v128.load16_lane", 3), ("v128.load32_lane", 1), ("v128.load64_lane", 1)]:
    add(o, f"loadlane:{lane}", allow=3)
add("v128.store", "store")
for o, lane in [("v128.store8_lane", 9), ("v128.store16_lane", 5), ("v128.store32_lane", 2), ("v128.store64_lane", 0)]:
    add(o, f"storelane:{lane}", allow=3)


def wat_for(op, kind):
    k = kind.split(":")
    data = "".join(f"\\{(i * 37 + 11) & 255:02x}" for i in range(512))
    mem = f'(memory (export "mem") 1) (data (i32.const 0) "{data}")'
    if kind == "vv_v":
        body = f"(param v128 v128) (result v128) ({op} (local.get 0) (local.get 1))"
    elif kind == "vvv_v":
        body = f"(param v128 v128 v128) (result v128) ({op} (local.get 0) (local.get 1) (local.get 2))"
    elif kind == "v_v":
        body = f"(param v128) (result v128) ({op} (local.get 0))"
    elif kind == "vi_v":
        body = f"(param v128 i32) (result v128) ({op} (local.get 0) (local.get 1))"
    elif kind == "v_i":
        body = f"(param v128) (result i32) ({op} (local.get 0))"
    elif k[0] == "splat":
        body = f"(param {k[1]}) (result v128) ({op} (local.get 0))"
    elif k[0] == "extract":
        body = f"(param v128) (result {k[1]}) ({op} {k[2]} (local.get 0))"
    elif k[0] == "replace":
        body = f"(param v128 {k[1]}) (result v128) ({op} {k[2]} (local.get 0) (local.get 1))"
    elif kind == "shuffle":
        body = f"(param v128 v128) (result v128) ({op} 0 17 2 19 4 21 6 23 8 25 10 27 12 29 14 31 (local.get 0) (local.get 1))"
    elif kind == "load":
        body = f"(param i32) (result v128) ({op} offset=3 (local.get 0))"
    elif k[0] == "loadlane":
        body = f"(param i32 v128) (result v128) ({op} offset=5 {k[1]} (local.get 0) (local.get 1))"
    elif kind == "store":
        body = f"(param i32 v128) ({op} offset=7 (local.get 0) (local.get 1))"
    elif k[0] == "storelane":
        body = f"(param i32 v128) ({op} offset=2 {k[1]} (local.get 0) (local.get 1))"
    else:
        raise ValueError(kind)
    return f'(module {mem} (func (export "f") {body}))'


def params_for(kind):
    k = kind.split(":")
    return {"vv_v": ["v128", "v128"], "vvv_v": ["v128"] * 3, "v_v": ["v128"], "vi_v": ["v128", "i32"],
            "v_i": ["v128"], "shuffle": ["v128", "v128"], "load": ["ptr"], "store": ["ptr", "v128"]}.get(kind) or {
        "splat": [k[-1]], "extract": ["v128"], "replace": ["v128", k[1] if len(k) > 1 else "i32"],
        "loadlane": ["ptr", "v128"], "storelane": ["ptr", "v128"]}[k[0]]


def rand_arg(rng, t):
    if t == "ptr":
        return {"type": "i32", "value": str(rng.randrange(0, 400))}
    if t == "v128":
        lt = rng.choice(["i8", "i16", "i32", "i64"])
        bits = {"i8": 8, "i16": 16, "i32": 32, "i64": 64}[lt]
        return {"type": "v128", "lane_type": lt,
                "value": [str(rng.choice([0, 1, (1 << bits) - 1, 1 << (bits - 1), rng.getrandbits(bits)]))
                          for _ in range(128 // bits)]}
    if t == "i32" and rng.random() < 0.5:
        return {"type": "i32", "value": str(rng.randrange(0, 200))}
    return {"type": t, "value": str(rng.getrandbits(32 if t in ("i32", "f32") else 64))}


def scalar_ok(op, kind):
    """Ops whose best lowering may contain no vector type after `opt -O2` (scalar operand
    or result next to the memory-based ABI); verify.py then bounds scalar memory accesses."""
    return kind == "v_i" or kind.split(":")[0] in ("extract", "storelane") or (kind == "load" and op.endswith("_zero"))


def float_result_shape(op):
    """Lane type of float-valued v128 results (NaN lanes then accept any NaN)."""
    if op.startswith("f32x4.") and not any(c in op for c in (".eq", ".ne", ".lt", ".gt", ".le", ".ge", "extract")):
        return "f32"
    if op.startswith("f64x2.") and not any(c in op for c in (".eq", ".ne", ".lt", ".gt", ".le", ".ge", "extract")):
        return "f64"
    return None


def relax_nans(cmd, lane):
    import struct
    for e in cmd.get("expected", []):
        if e["type"] != "v128":
            continue
        raw = bytes(int(x) for x in e["value"])
        fmt, n, w = ("<I", 4, 32) if lane == "f32" else ("<Q", 8, 64)
        vals = []
        for i in range(0, 16, n):
            bits = struct.unpack_from(fmt, raw, i)[0]
            vals.append("nan:any" if w2l._float_match(bits, "nan:any", w) else str(bits))
        e.update(lane_type=lane, value=vals)


def main():
    out = Path(sys.argv[1])
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)
    work = Path(tempfile.mkdtemp(prefix="simdq-"))
    manifest = []
    for op, kind, allow in ops:
        name = op.replace(".", "_")
        d = out / name
        d.mkdir()
        wat = d / f"{name}.wat"
        wat.write_text(wat_for(op, kind) + "\n")
        subprocess.run(["wasm-tools", "parse", str(wat), "-o", str(d / f"{name}.wasm")], check=True)
        wat.unlink()
        rng = random.Random(name)
        cmds = [{"type": "module", "line": 1, "filename": f"{name}.wasm"}]
        for i in range(12):
            cmds.append({"type": "action", "line": 2 + i, "action": {
                "type": "invoke", "field": "f", "args": [rand_arg(rng, t) for t in params_for(kind)]}})
        cmds.append({"type": "record_memhash", "line": 20, "memory": "mem"})
        raw = d / "_raw.json"
        raw.write_text(json.dumps({"commands": cmds}))
        w2l.record_script(raw, d / f"{name}.json", work)
        raw.unlink()
        lane = float_result_shape(op)
        if lane:
            data = json.loads((d / f"{name}.json").read_text())
            for c in data["commands"]:
                relax_nans(c, lane)
            (d / f"{name}.json").write_text(json.dumps(data, indent=1))
        manifest.append({"op": op, "file": f"{name}/{name}.wasm", "script": f"{name}/{name}.json", "allow": allow,
                         "scalar_ok": scalar_ok(op, kind)})
    (out / "_manifest.json").write_text(json.dumps(manifest, indent=1))
    shutil.rmtree(work, ignore_errors=True)
    print(len(manifest), "ops")


if __name__ == "__main__":
    main()

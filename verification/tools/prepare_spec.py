#!/usr/bin/env python3
"""Convert WebAssembly spec .wast files into filtered w2l test scripts.

  prepare_spec.py OUT_DIR WAST_OR_DIR...

For each .wast: run `wasm-tools json-from-wast`, then keep only commands that
fall inside the benchmark scope:
  * modules must validate with exactly the benchmark feature set and may only
    import supported host functions (spectest.print*, env.*);
  * commands depending on dropped modules, `register`, assert_unlinkable and
    text-format assert_malformed/assert_invalid are dropped;
  * binary assert_invalid / assert_malformed are kept if the module is rejected
    under the benchmark feature set;
  * every kept assertion is then checked against the reference backend
    (wasmtime); disagreeing assertions are dropped and reported.
"""

import concurrent.futures as cf
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

sys.dont_write_bytecode = True  # keep agent/harness pristine
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "agent" / "harness"))
import w2l  # noqa: E402

HOST_FUNCS = {
    ("spectest", "print"), ("spectest", "print_i32"), ("spectest", "print_i64"), ("spectest", "print_f32"),
    ("spectest", "print_f64"), ("spectest", "print_i32_f32"), ("spectest", "print_f64_f64"),
    ("env", "print_i32"), ("env", "print_i64"), ("env", "print_f32"), ("env", "print_f64"),
    ("env", "print_v128"), ("env", "putchar"),
}


def valid(wasm):
    r = subprocess.run(["wasm-tools", "validate", w2l.FEATURES_ARG, str(wasm)], capture_output=True)
    return r.returncode == 0


def imports(wasm):
    """Returns list of (module, name, kind) imports."""
    b = Path(wasm).read_bytes()
    pos, out = 8, []

    def leb(p):
        r = s = 0
        while True:
            x = b[p]; p += 1
            r |= (x & 0x7F) << s; s += 7
            if x < 0x80:
                return r, p

    def name(p):
        n, p = leb(p)
        return b[p:p + n].decode("utf-8", "replace"), p + n

    while pos < len(b):
        sid = b[pos]; pos += 1
        size, pos = leb(pos)
        if sid == 2:
            p = pos
            n, p = leb(p)
            for _ in range(n):
                m, p = name(p); f, p = name(p)
                k = b[p]; p += 1
                out.append((m, f, k))
                if k == 0:
                    _, p = leb(p)
                elif k == 1:
                    p += 1; fl = b[p]; p += 1; _, p = leb(p)
                    if fl & 1: _, p = leb(p)
                elif k == 2:
                    fl = b[p]; p += 1; _, p = leb(p)
                    if fl & 1: _, p = leb(p)
                elif k == 3:
                    p += 2
        pos += size
    return out


def in_scope(wasm):
    if not valid(wasm):
        return False, "invalid under benchmark features"
    for m, f, k in imports(wasm):
        if k != 0 or (m, f) not in HOST_FUNCS:
            return False, f"unsupported import {m}.{f} kind {k}"
    return True, ""


def uses_value_types(cmd):
    """True if all values in the command are of supported types."""
    vals = list(cmd.get("action", {}).get("args", [])) + list(cmd.get("expected", []))
    return all(v.get("type") in ("i32", "i64", "f32", "f64", "v128") for v in vals)


def prepare(wast, out_root, work):
    name = wast.stem
    tmp = Path(tempfile.mkdtemp(dir=work))
    r = subprocess.run(["wasm-tools", "json-from-wast", "--wasm-dir", str(tmp), "-o", str(tmp / "x.json"), str(wast)],
                       capture_output=True, text=True)
    if r.returncode != 0:
        return name, {"error": r.stderr.strip()[:500]}
    data = json.loads((tmp / "x.json").read_text())
    kept, dropped = [], {}
    cur_ok = False
    named_ok = {}

    def drop(why):
        dropped[why] = dropped.get(why, 0) + 1

    for cmd in data["commands"]:
        t = cmd["type"]
        if t == "module":
            if cmd.get("module_type", "binary") != "binary":
                cur_ok = False
                drop("text module")
                continue
            ok, why = in_scope(tmp / cmd["filename"])
            cur_ok = ok
            if cmd.get("name"):
                named_ok[cmd["name"]] = ok
            if ok:
                kept.append(cmd)
            else:
                drop("module: " + why.split(" kind")[0])
        elif t in ("assert_return", "assert_trap", "assert_exhaustion", "action"):
            act = cmd["action"]
            mok = named_ok.get(act["module"], False) if act.get("module") else cur_ok
            if not mok:
                drop("depends on dropped module")
            elif not uses_value_types(cmd):
                drop("reference-typed values")
            elif t == "assert_return" and any("either" in e or e.get("type") == "either" for e in cmd.get("expected", [])):
                drop("either")
            else:
                kept.append(cmd)
        elif t in ("assert_invalid", "assert_malformed"):
            if cmd.get("module_type", "binary") != "binary":
                drop(f"{t} (text)")
            elif valid(tmp / cmd["filename"]):
                drop(f"{t} but valid under benchmark features")
            else:
                kept.append(cmd)
        elif t == "assert_uninstantiable":
            ok, why = in_scope(tmp / cmd["filename"])
            if ok:
                kept.append(cmd)
            else:
                drop("uninstantiable: " + why)
        else:
            drop(t)

    if not any(c["type"] != "module" for c in kept):
        shutil.rmtree(tmp)
        return name, {"kept": 0, "dropped": dropped}

    # Copy referenced files into the output directory.
    out = out_root / name
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)
    for c in kept:
        if "filename" in c:
            shutil.copy(tmp / c["filename"], out / c["filename"])
    script = out / f"{name}.json"
    script.write_text(json.dumps({"source_filename": wast.name, "commands": kept}, indent=1))

    # Cross-check with the reference backend; drop disagreeing assertions.
    be = w2l.RefBackend(work / "ref")
    res = w2l.run_script(script, be)
    by_line = {}
    for rr in res:
        by_line.setdefault(rr["line"], []).append(rr)
    final, ref_fail = [], []
    i = 0
    # run_script records one result per executed command in order (modules too,
    # text-format asserts are skipped); match by order.
    exec_cmds = [c for c in kept if not (c["type"] in ("assert_invalid", "assert_malformed") and c.get("module_type", "binary") != "binary")]
    assert len(exec_cmds) == len(res), (name, len(exec_cmds), len(res))
    for c, rr in zip(exec_cmds, res):
        if rr["ok"] or c["type"] == "module":
            final.append(c)
            if not rr["ok"]:
                ref_fail.append((c.get("line"), c["type"], rr["msg"]))
        else:
            ref_fail.append((c.get("line"), c["type"], rr["msg"]))
    script.write_text(json.dumps({"source_filename": wast.name, "commands": final}, indent=1))
    shutil.rmtree(tmp)
    n = sum(1 for c in final if c["type"] != "module")
    return name, {"kept": n, "dropped": dropped, "ref_disagree": ref_fail}


def main():
    out_root = Path(sys.argv[1]).resolve()
    wasts = []
    for p in sys.argv[2:]:
        p = Path(p)
        wasts += sorted(p.rglob("*.wast")) if p.is_dir() else [p]
    out_root.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="prep-"))
    report = {}
    with cf.ThreadPoolExecutor(max_workers=16) as ex:
        for name, info in ex.map(lambda w: prepare(w, out_root, work), wasts):
            report[name] = info
            print(f"{name:40s} kept={info.get('kept')} ref_disagree={len(info.get('ref_disagree', []))} "
                  f"{'ERROR ' + info['error'] if 'error' in info else ''}")
    (out_root / "_prepare_report.json").write_text(json.dumps(report, indent=1))
    shutil.rmtree(work, ignore_errors=True)
    print("total kept assertions:", sum(v.get("kept", 0) for v in report.values()))


if __name__ == "__main__":
    main()

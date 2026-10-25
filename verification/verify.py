#!/usr/bin/env python3
"""Hidden verifier for the wasm2llvm benchmark.

  verify.py SUBMISSION_DIR [--out REPORT_DIR] [-j N] [--quick] [--skip-perf] [--skip-build]

Steps
  1. Build the submission (`cargo build --release --offline`) from a clean copy,
     with an isolated CARGO_HOME and no network.
  2. Wrap the translator in a bubblewrap jail: no network, empty environment,
     /mnt/workspace and /home hidden, input copied to a private dir.
  3. Correctness suites (native backend at clang -O0 and -O2; an assertion passes
     only if it passes at both levels):
       spec      WebAssembly 2.0 core + SIMD testsuite, filtered to scope
       spec3     tests new/changed in the 3.0 testsuite, filtered to scope
       payloads  Rust programs, each at O0/O3/Os
       real      real-world payloads (QOA, CELT/SILK Opus encoders) with hidden inputs
       smith     wasm-smith differential tests vs wasmtime
     Assertions are split into "execution" (everything except assert_invalid /
     assert_malformed) and "validation" (rejecting invalid/malformed binaries).
  4. Secondary metrics: SIMD lowering quality, performance vs wasmtime.
Writes REPORT_DIR/report.json and REPORT_DIR/report.md.
"""

import argparse
import json
import math
import os
import shutil
import stat
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
VER = ROOT / "verification"
TOOLBIN = ROOT / "deps" / "bin"
os.environ["PATH"] = f"{TOOLBIN}:{os.environ.get('PATH', '')}"
sys.dont_write_bytecode = True  # keep the harness pristine
sys.path.insert(0, str(VER / "harness"))
import w2l  # noqa: E402  (verifier uses its own pristine copy of the driver)

SUITES = {
    # name: (list of dirs, weight in correctness score)
    "spec": ([VER / "tests" / "spec"], 0.30),
    "spec3": ([VER / "tests" / "spec3"], 0.10),
    "payloads": ([VER / "tests" / "payloads"], 0.20),
    "real": ([VER / "tests" / "real"], 0.20),
    "smith": ([VER / "tests" / "smith"], 0.20),
}
VALIDATION_TYPES = ("assert_invalid", "assert_malformed")
# Small scripts whose setup assertions are mostly trivial; a codec that computes
# wrong output must not collect partial credit for "init returned 0".
STRICT_SUITES = {"real"}


def log(*a):
    print(*a, file=sys.stderr, flush=True)


# ---------------------------------------------------------------------------
# Build

def build(sub, work):
    src = work / "src"
    if src.exists():
        shutil.rmtree(src)
    shutil.copytree(sub, src, ignore=shutil.ignore_patterns("target", ".git", ".w2l-work", ".claude"))
    if not (src / "Cargo.toml").exists():
        return None, "no Cargo.toml at submission root"
    home = work / "cargo-home"
    home.mkdir(exist_ok=True)
    tdir = work / "target"
    cmd = ["bwrap", "--ro-bind", "/", "/", "--dev", "/dev", "--proc", "/proc", "--tmpfs", "/tmp",
           "--tmpfs", "/mnt/workspace", "--bind", str(work), str(work),
           "--ro-bind", str(Path.home() / ".rustup"), str(Path.home() / ".rustup"),
           "--unshare-net", "--die-with-parent", "--chdir", str(src),
           "--setenv", "CARGO_HOME", str(home), "--setenv", "CARGO_TARGET_DIR", str(tdir),
           "--setenv", "RUSTUP_HOME", str(Path.home() / ".rustup"),
           "--setenv", "PATH", "/usr/bin:/bin", "--setenv", "HOME", str(work),
           "cargo", "build", "--release", "--offline"]
    t0 = time.time()
    r = subprocess.run(cmd, capture_output=True, text=True, timeout=3600)
    log(f"build: exit {r.returncode} in {time.time() - t0:.0f}s")
    binp = tdir / "release" / "wasm2llvm"
    if r.returncode != 0 or not binp.exists():
        return None, (r.stdout + r.stderr)[-5000:]
    final = work / "wasm2llvm"
    shutil.copy(binp, final)
    return final, None


def make_jail(binary, work):
    """Wrapper script running the translator inside bubblewrap."""
    wrapper = work / "translator-jail.sh"
    # Also hide system LLVM installations (e.g. /usr/bin/clang -> /usr/lib/llvm-21).
    hide = " ".join(f"--tmpfs {d}" for d in sorted(Path("/usr/lib").glob("llvm*")) if d.is_dir())
    wrapper.write_text(f"""#!/bin/bash
# usage: translator-jail.sh IN.wasm -o OUT.ll
set -u
in="$1"; out="$3"
priv=$(mktemp -d)
trap 'rm -rf "$priv"' EXIT
cp "$in" "$priv/input.wasm"
mkdir "$priv/out"
bwrap --ro-bind / / --dev /dev --proc /proc --tmpfs /tmp --tmpfs /mnt/workspace --tmpfs /home {hide} \\
  --ro-bind {binary} /tmp/w2l/wasm2llvm --ro-bind "$priv/input.wasm" /tmp/w2l/input.wasm \\
  --bind "$priv/out" /tmp/w2l/out --unshare-all --die-with-parent --clearenv --setenv PATH /usr/bin:/bin \\
  --chdir /tmp/w2l/out -- /tmp/w2l/wasm2llvm /tmp/w2l/input.wasm -o /tmp/w2l/out/output.ll
rc=$?
if [ $rc -eq 0 ] && [ -f "$priv/out/output.ll" ]; then mv "$priv/out/output.ll" "$out"; fi
exit $rc
""")
    wrapper.chmod(wrapper.stat().st_mode | stat.S_IXUSR)
    return wrapper


# ---------------------------------------------------------------------------
# Correctness

def suite_scores(res, labels, scripts_root_map):
    """Per-script pass fractions for execution and validation assertions."""
    out = {"execution": {}, "validation": {}, "failures": {}, "modules": [0, 0]}
    for s, by in res.items():
        rows = [by.get(l, []) for l in labels]
        n = max(len(r) for r in rows)
        cnt = {"execution": [0, 0], "validation": [0, 0]}
        fails = []
        for i in range(n):
            entries = [r[i] if i < len(r) else None for r in rows]
            typ = next((e["type"] for e in entries if e), "?")
            cat = "validation" if typ in VALIDATION_TYPES else "execution"
            ok = all(e is not None and e["ok"] for e in entries)
            if typ == "module":
                # instantiation is implied by the assertions that follow; only
                # tracked to keep the validation metric honest
                out["modules"][0] += ok
                out["modules"][1] += 1
                continue
            cnt[cat][1] += 1
            cnt[cat][0] += ok
            if not ok and len(fails) < 10:
                e = next((e for e in entries if e is not None and not e["ok"]), entries[0] or {})
                fails.append(f"line {e.get('line')} {typ} [{labels[entries.index(e)] if e in entries else '?'}]: {str(e.get('msg'))[:200]}")
        name = scripts_root_map(s)
        for cat in ("execution", "validation"):
            if cnt[cat][1]:
                out[cat][name] = cnt[cat]
        if fails:
            out["failures"][name] = fails
    return out


def agg(per_script):
    """Mean of per-script pass fractions, plus raw totals."""
    if not per_script:
        return {"score": None, "scripts": 0, "passed": 0, "total": 0, "scripts_fully_passing": 0}
    fr = [p / t for p, t in per_script.values()]
    return {"score": sum(fr) / len(fr), "scripts": len(fr),
            "passed": sum(p for p, _ in per_script.values()), "total": sum(t for _, t in per_script.values()),
            "scripts_fully_passing": sum(1 for p, t in per_script.values() if p == t)}


def strict_agg(per_script):
    """All-or-nothing per script: one failed assertion scores the script 0."""
    if not per_script:
        return {"score": None, "scripts": 0, "passed": 0, "total": 0, "scripts_fully_passing": 0}
    ok = [p == t for p, t in per_script.values()]
    return {"score": sum(ok) / len(ok), "scripts": len(ok),
            "passed": sum(ok), "total": len(ok), "scripts_fully_passing": sum(ok)}


def run_correctness(translator, work, jobs, quick):
    backends = [w2l.NativeBackend(str(translator), opt, work / "native") for opt in (0, 2)]
    labels = [b.label() for b in backends]
    results = {}
    for name, (dirs, weight) in SUITES.items():
        scripts = w2l.find_scripts([d for d in dirs if d.exists()])
        if quick:
            scripts = scripts[::10]
        t0 = time.time()
        res = w2l.run_many(scripts, backends, jobs, False)
        def rel(s):
            p = Path(s)
            for d in dirs:
                try:
                    return str(p.relative_to(d))
                except ValueError:
                    pass
            return str(p)
        sc = suite_scores(res, labels, rel)
        raw = agg(sc["execution"])
        strict = name in STRICT_SUITES
        results[name] = {"weight": weight, "strict": strict,
                         "execution": strict_agg(sc["execution"]) if strict else raw,
                         "raw_execution": raw, "validation": agg(sc["validation"]),
                         "per_script": {k: v for k, v in sc["execution"].items()},
                         "per_script_validation": sc["validation"], "failures": sc["failures"],
                         "modules_ok": sc["modules"],
                         "seconds": round(time.time() - t0)}
        e = results[name]["execution"]
        if strict:
            log(f"{name}: execution score {e['score']} (scripts {e['scripts_fully_passing']}/{e['scripts']}; "
                f"raw assertions {raw['passed']}/{raw['total']}) in {results[name]['seconds']}s")
        else:
            log(f"{name}: execution score {e['score']} ({e['passed']}/{e['total']}) in {results[name]['seconds']}s")
    return results


# ---------------------------------------------------------------------------
# SIMD lowering quality

MAX_SCALAR_MEM_OPS = 3


def _scalar_mem_ops(lines):
    """Loads/stores in wasm code (not w2l_init or the memory/global accessors) whose address
    is not a global (module state)."""
    n, fn = 0, None
    for ln in lines:
        if ln.startswith("define "):
            fn = ln
        elif ln.startswith("}"):
            fn = None
        elif fn and not any(a in fn for a in ("@w2l_init(", "@w2l_memory_", "@w2l_global_")):
            if " = load " in ln or ln.lstrip().startswith("store "):
                n += ", ptr @" not in ln
    return n


def run_simd_quality(translator, work, jobs):
    """An op is well-lowered iff its random-input script passes at -O2, the
    optimized IR contains vector types, and the number of extractelement +
    insertelement instructions is within the op's allowance."""
    d = VER / "tests" / "simd_quality"
    manifest = json.loads((d / "_manifest.json").read_text())
    be = w2l.NativeBackend(str(translator), 2, work / "native")
    res = w2l.run_many([d / m["script"] for m in manifest], [be], jobs, False)
    opt = shutil.which("opt")
    rows = []
    for m in manifest:
        row = {"op": m["op"], "allow": m["allow"]}
        r_script = res[str(d / m["script"])][be.label()]
        row["correct"] = bool(r_script) and all(x["ok"] for x in r_script)
        ok, ll = be.translate(d / m["file"])
        if not ok:
            row.update(status="translate-failed")
            rows.append(row)
            continue
        r = subprocess.run([opt, "-O2", "-S", str(ll), "-o", "-"], capture_output=True, text=True, timeout=300)
        if r.returncode != 0:
            row.update(status="opt-failed")
            rows.append(row)
            continue
        lines = r.stdout.splitlines()
        lane = sum(1 for ln in lines if " = extractelement " in ln or " = insertelement " in ln)
        vec = sum(1 for ln in lines if any(t in ln for t in ("<16 x i8>", "<8 x i16>", "<4 x i32>", "<2 x i64>",
                                                             "<4 x float>", "<2 x double>")))
        row.update(lane_ops=lane, vector_lines=vec)
        if not row["correct"]:
            row["status"] = "incorrect"
        elif m["allow"] is None:
            row["status"] = "info"
        elif lane > m["allow"]:
            row["status"] = "scalarized"
        elif vec == 0:
            # Ops with a scalar operand or result (extract/store lane, load*_zero, reductions)
            # may legitimately lose all vector types once opt narrows the ABI slot accesses;
            # a per-lane lowering still needs one scalar access per lane.
            mem = _scalar_mem_ops(lines)
            row["mem_ops"] = mem
            row["status"] = "pass" if m.get("scalar_ok") and mem <= MAX_SCALAR_MEM_OPS else "scalarized"
        else:
            row["status"] = "pass"
        rows.append(row)
    scored = [r for r in rows if r["allow"] is not None]
    passed = sum(1 for r in scored if r["status"] == "pass")
    log(f"simd quality: {passed}/{len(scored)}")
    return {"score": passed / len(scored), "passed": passed, "total": len(scored), "ops": rows}


# ---------------------------------------------------------------------------
# Performance

def time_bench(backend, script, reps=3):
    data = json.loads(Path(script).read_text())
    base = Path(script).parent
    mod = next(c for c in data["commands"] if c["type"] == "module")
    inv = next(c for c in data["commands"] if c["type"] == "assert_return")
    ok, art = backend.prepare(base / mod["filename"])
    if not ok:
        return None, f"prepare failed: {art[:300]}"
    best = None
    for _ in range(reps):
        p = backend.new_proc()
        try:
            if p.cmd(f"load m {art}") != "ok" or p.cmd("init m") != "ok":
                return None, "load/init failed"
            act = inv["action"]
            args = " ".join(w2l.encode_value(a) for a in act["args"])
            t0 = time.perf_counter()
            r = p.cmd(f"invoke m {w2l.hexname(act['field'])} {len(inv['expected'])} {args}", timeout=600)
            dt = time.perf_counter() - t0
        except w2l.Crash as e:
            return None, f"crash: {e}"
        finally:
            p.close()
        parts = r.split()
        if parts[0] != "ok" or not all(w2l.match_value(s, e) for s, e in zip(parts[1:], inv["expected"])):
            return None, f"wrong result: {r}"
        best = dt if best is None else min(best, dt)
    return best, None


def run_perf(translator, work):
    native = w2l.NativeBackend(str(translator), 2, work / "native")
    ref = w2l.RefBackend(work / "ref")
    rows = []
    for script in sorted(VER.glob("tests/payloads/*/_bench.json")):
        name = script.parent.name
        tn, en = time_bench(native, script)
        tr, er = time_bench(ref, script)
        row = {"bench": name, "native_s": tn, "wasmtime_s": tr, "error": en or er}
        if tn and tr:
            row["speedup_vs_wasmtime"] = tr / tn
        rows.append(row)
        log(f"perf {name}: native {tn} wasmtime {tr} {en or ''}")
    sp = [r["speedup_vs_wasmtime"] for r in rows if "speedup_vs_wasmtime" in r]
    geo = math.exp(sum(math.log(x) for x in sp) / len(sp)) if sp else None
    return {"geomean_speedup_vs_wasmtime": geo, "benches_ok": len(sp), "benches": rows}


# ---------------------------------------------------------------------------

def write_report(rep, out):
    out.mkdir(parents=True, exist_ok=True)
    (out / "report.json").write_text(json.dumps(rep, indent=1))
    L = ["# wasm2llvm verification report", ""]
    L.append(f"- build: **{'ok' if rep['build']['ok'] else 'FAILED'}**")
    if not rep["build"]["ok"]:
        L += ["", "```", rep["build"]["log"], "```"]
        (out / "report.md").write_text("\n".join(L) + "\n")
        return
    c = rep["correctness"]
    L.append(f"- **correctness score (primary): {c['score']:.4f}**")
    L.append(f"- validation score (secondary; invalid-rejection rate x valid-module acceptance rate): {c['validation_score']:.4f}"
             if c["validation_score"] is not None else "- validation: n/a")
    if rep.get("simd_quality"):
        s = rep["simd_quality"]
        L.append(f"- SIMD lowering quality (secondary): {s['passed']}/{s['total']} = {s['score']:.3f}")
    if rep.get("perf"):
        g = rep["perf"]["geomean_speedup_vs_wasmtime"]
        L.append(f"- performance (secondary): geomean speedup vs wasmtime = {g:.3f}" if g else "- performance: n/a")
    L += ["", "| suite | weight | exec score | scripts fully passing | assertions | validation |", "|---|---|---|---|---|---|"]
    for name, s in c["suites"].items():
        e, v = s["execution"], s["validation"]
        raw = s.get("raw_execution", e)
        vs = f"{v['passed']}/{v['total']}" if v["total"] else "-"
        sc = f"{e['score']:.4f}" if e["score"] is not None else "-"
        L.append(f"| {name} | {s['weight']} | {sc} | {e['scripts_fully_passing']}/{e['scripts']} | "
                 f"{raw['passed']}/{raw['total']} | {vs} |")
    if any(s.get("strict") for s in c["suites"].values()):
        L += ["", "The `real` suite is scored all-or-nothing per script; its assertion totals are diagnostic."]
    L += ["", "## Failures (first per script)", ""]
    for name, s in c["suites"].items():
        for scr, f in sorted(s["failures"].items())[:200]:
            L.append(f"- `{name}:{scr}`: {f[0]}")
    if rep.get("simd_quality"):
        bad = [r for r in rep["simd_quality"]["ops"] if r["status"] not in ("pass", "info")]
        L += ["", "## SIMD ops not passing the lowering check", ""]
        L += [f"- {r['op']}: {r['status']} (lane ops {r.get('lane_ops')}, allowed {r['allow']})" for r in bad] or ["- none"]
    if rep.get("perf"):
        L += ["", "## Performance", "", "| bench | native -O2 (s) | wasmtime (s) | speedup |", "|---|---|---|---|"]
        for r in rep["perf"]["benches"]:
            fmt = lambda x: f"{x:.3f}" if isinstance(x, float) else "-"
            L.append(f"| {r['bench']} | {fmt(r['native_s'])} | {fmt(r['wasmtime_s'])} | {fmt(r.get('speedup_vs_wasmtime'))} {r['error'] or ''} |")
    (out / "report.md").write_text("\n".join(L) + "\n")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("submission")
    ap.add_argument("--out", default=None)
    ap.add_argument("-j", "--jobs", type=int, default=os.cpu_count())
    ap.add_argument("--quick", action="store_true", help="run every 10th script only")
    ap.add_argument("--skip-perf", action="store_true")
    ap.add_argument("--translator", help="use this prebuilt translator instead of building (debugging only)")
    ap.add_argument("--no-jail", action="store_true", help="do not sandbox the translator (debugging only)")
    a = ap.parse_args()
    sub = Path(a.submission).resolve()
    out = Path(a.out).resolve() if a.out else sub.parent / f"{sub.name}-report"
    work = Path(tempfile.mkdtemp(prefix="w2l-verify-"))
    log(f"work dir: {work}")
    rep = {"submission": str(sub), "started": time.strftime("%Y-%m-%d %H:%M:%S")}
    if a.translator:
        binary, err = Path(a.translator).resolve(), None
    else:
        binary, err = build(sub, work)
    rep["build"] = {"ok": binary is not None, "log": err or ""}
    if binary is None:
        write_report(rep, out)
        log(f"build failed; report in {out}")
        return 1
    jail = binary if a.no_jail else make_jail(binary, work)
    suites = run_correctness(jail, work, a.jobs, a.quick)
    tot_w = sum(s["weight"] for s in suites.values() if s["execution"]["score"] is not None)
    score = sum(s["weight"] * s["execution"]["score"] for s in suites.values() if s["execution"]["score"] is not None) / tot_w
    # Rejecting invalid modules only counts as much as valid modules are accepted
    # (otherwise "reject everything" would score 1.0).
    vals = [s["validation"] for s in suites.values() if s["validation"]["total"]]
    mods = [sum(s["modules_ok"][i] for s in suites.values()) for i in (0, 1)]
    accept = mods[0] / mods[1] if mods[1] else 0.0
    vscore = (sum(v["passed"] for v in vals) / sum(v["total"] for v in vals)) * accept if vals else None
    rep["correctness"] = {"score": score, "validation_score": vscore, "suites": suites}
    rep["simd_quality"] = run_simd_quality(jail, work, a.jobs)
    if not a.skip_perf:
        rep["perf"] = run_perf(jail, work)
    rep["finished"] = time.strftime("%Y-%m-%d %H:%M:%S")
    write_report(rep, out)
    log(f"correctness {score:.4f}; report written to {out}")
    shutil.rmtree(work, ignore_errors=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())

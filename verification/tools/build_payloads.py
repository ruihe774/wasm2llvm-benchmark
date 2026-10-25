#!/usr/bin/env python3
"""Build Rust payloads and generate w2l test scripts with reference expectations.

  build_payloads.py [NAME ...]

For every crate in verification/payloads/{visible,hidden}/ and every profile
(dev=O0, release=O3, os=Os):
  * cargo build --target wasm32v1-none with the benchmark target features
  * validate the .wasm against the benchmark feature set
  * write <dest>/<name>-<opt>/<name>-<opt>.{wasm,json}; the script invokes each
    entry of payload.json "invocations" then checks host output and the final
    memory hash, with expectations recorded from the reference runner.
  * if payload.json has "bench", also write _bench.json (used for perf scoring).
All payloads go to verification/tests/payloads (hidden); sources are in
verification/payloads/{visible,hidden}.
"""

import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PAY = ROOT / "verification" / "payloads"
sys.dont_write_bytecode = True  # keep the harness pristine
sys.path.insert(0, str(ROOT / "verification" / "harness"))
import w2l  # noqa: E402

FEATURES = "+bulk-memory,+multivalue,+nontrapping-fptoint,+sign-ext,+simd128"
PROFILES = [("dev", "O0", "debug"), ("release", "O3", "release"), ("os", "Os", "os")]
DEST = {"visible": ROOT / "verification" / "tests" / "payloads", "hidden": ROOT / "verification" / "tests" / "payloads"}


def build(name, profile):
    env = dict(os.environ, RUSTC_BOOTSTRAP="1", RUSTFLAGS=f"-C target-feature={FEATURES}")
    subprocess.run(["cargo", "build", "-q", "--profile", profile, "--target", "wasm32v1-none", "-p", name],
                   cwd=PAY, env=env, check=True)


def script_for(wasm_name, invocations, opt, checks=True):
    # Invocations may be restricted to some opt levels, e.g. deep recursion at O0
    # spills to the shadow stack, making the trap kind implementation-defined.
    invocations = [i for i in invocations if opt in i.get("opts", [opt])]
    cmds = [{"type": "module", "line": 1, "filename": wasm_name}]
    for i, inv in enumerate(invocations):
        cmds.append({"type": "action", "line": 2 + i,
                     "action": {"type": "invoke", "field": inv["field"], "args": inv.get("args", [])}})
    n = 2 + len(invocations)
    if not checks:  # bench scripts: only the return value matters
        return {"source_filename": wasm_name, "commands": cmds}
    cmds.append({"type": "record_output", "line": n})
    if invocations and invocations[0].get("memhash", True):
        cmds.append({"type": "record_memhash", "line": n + 1, "memory": "memory"})
    return {"source_filename": wasm_name, "commands": cmds}


def check_no_traps(script, cfg):
    """Guard against payload bugs: a trap is only acceptable if the payload says so."""
    if cfg.get("may_trap"):
        return
    for c in json.loads(Path(script).read_text())["commands"]:
        if c["type"] in ("assert_trap", "assert_exhaustion"):
            sys.exit(f"{script}: unexpected reference trap at line {c.get('line')}: {c.get('text')}")


def main():
    only = set(sys.argv[1:])
    work = Path(tempfile.mkdtemp(prefix="payloads-"))
    for kind in ("visible", "hidden"):
        for crate in sorted((PAY / kind).iterdir()) if (PAY / kind).exists() else []:
            if not (crate / "Cargo.toml").exists() or (only and crate.name not in only):
                continue
            cfg = json.loads((crate / "payload.json").read_text())
            for profile, opt, tdir in PROFILES:
                if opt not in cfg.get("opts", ["O0", "O3", "Os"]):
                    continue
                build(crate.name, profile)
                src = PAY / "target" / "wasm32v1-none" / tdir / f"{crate.name}.wasm"
                r = subprocess.run(["wasm-tools", "validate", w2l.FEATURES_ARG, str(src)], capture_output=True, text=True)
                if r.returncode:
                    sys.exit(f"{crate.name} {opt}: invalid under benchmark features: {r.stderr}")
                tag = f"{crate.name}-{opt}"
                out = DEST[kind] / tag
                if out.exists():
                    shutil.rmtree(out)
                out.mkdir(parents=True)
                shutil.copy(src, out / f"{tag}.wasm")
                raw = out / "_raw.json"
                raw.write_text(json.dumps(script_for(f"{tag}.wasm", cfg["invocations"], opt)))
                w2l.record_script(raw, out / f"{tag}.json", work)
                raw.unlink()
                check_no_traps(out / f"{tag}.json", cfg)
                if "bench" in cfg and opt == "O3":
                    raw.write_text(json.dumps(script_for(f"{tag}.wasm", cfg["bench"], opt, checks=False)))
                    w2l.record_script(raw, out / "_bench.json", work)
                    raw.unlink()
                    check_no_traps(out / "_bench.json", {})
                print(f"{kind:8s} {tag:28s} {src.stat().st_size:8d} bytes")
    shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    main()

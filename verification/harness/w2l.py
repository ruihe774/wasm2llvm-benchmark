#!/usr/bin/env python3
"""w2l test driver.

Runs test scripts against either
  * the native backend: wasm --(your translator)--> .ll --(clang)--> .so,
    executed by w2l_harness, or
  * the reference backend: the .wasm executed by w2l-ref (wasmtime).

Test scripts use the JSON format produced by `wasm-tools json-from-wast`
(a.k.a. wast2json), plus two extension commands:
  {"type": "assert_output", "line": N, "module": <optional name>, "hex": "<hex bytes>"}
      Host output (see ../../agent/ABI.md, host imports) produced since the
      previous assert_output of that module must equal the given bytes.
  {"type": "assert_memhash", "line": N, "module": <optional name>, "memory": "<export name>",
   "size": <bytes>, "fnv1a64": "<16 hex digits>"}
      Exported memory must have the given byte size and FNV-1a-64 hash.

Bare actions may also be {"type": "memwrite", "memory", "offset", "hex"},
{"type": "memwrite_fn", "field" (pointer function), "memory", "hex"} or
{"type": "memwrite_global", "field" (i32 global), "memory", "hex"} to fill
input buffers. `memwrite_fn` checks the named export's signature (no
parameters, one i32/i64 result), calls it, and writes at the returned offset;
it is implemented by the driver as invoke + memwrite. See
verification/harness/PROTOCOL.md.

Usage:
  w2l.py run [--translator PATH] [--opt 0,2] [-j N] [--backend native|ref]
             [--out results.json] [--work DIR] [-v] TEST.json|DIR ...
  w2l.py record [--work DIR] SCRIPT.json OUT.json
      Execute SCRIPT with the reference backend and write a copy in which every
      bare "action" command is replaced by an assert_return / assert_trap
      holding the reference result (useful to make your own tests).

Only the Python standard library is used.
"""

import argparse
import concurrent.futures as cf
import hashlib
import json
import os
import shutil
import struct
import subprocess
import sys
import threading
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent

TRAP_CODES = {
    "unreachable": 0,
    "integer divide by zero": 1,
    "integer overflow": 2,
    "invalid conversion to integer": 3,
    "out of bounds memory access": 4,
    "out of bounds table access": 5,
    "undefined element": 5,
    "uninitialized element": 6,
    "indirect call type mismatch": 7,
    "call stack exhausted": 8,
}
TRAP_NAMES = {0: "unreachable", 1: "integer divide by zero", 2: "integer overflow",
              3: "invalid conversion to integer", 4: "out of bounds memory access",
              5: "out of bounds table access / undefined element", 6: "uninitialized element",
              7: "indirect call type mismatch", 8: "call stack exhausted", 9: "other"}

TRANSLATE_TIMEOUT = float(os.environ.get("W2L_TRANSLATE_TIMEOUT", 120))
COMPILE_TIMEOUT = float(os.environ.get("W2L_COMPILE_TIMEOUT", 600))
COMMAND_TIMEOUT = float(os.environ.get("W2L_COMMAND_TIMEOUT", 30))
MAX_RESTARTS = 25


def trap_code_for(text):
    for k, v in TRAP_CODES.items():
        if text.startswith(k):
            return v
    return None


def which(tool):
    p = shutil.which(tool)
    if not p:
        sys.exit(f"error: required tool '{tool}' not found in PATH")
    return p


def hexname(name):
    return name.encode("utf-8").hex() or "-"


# ---------------------------------------------------------------------------
# Values

def _lane_fmt(lane_type):
    return {"i8": ("<B", 1), "i16": ("<H", 2), "i32": ("<I", 4), "i64": ("<Q", 8),
            "f32": ("<I", 4), "f64": ("<Q", 8)}[lane_type]


def encode_value(v):
    """wast2json value -> 32 hex char slot."""
    t = v["type"]
    b = bytearray(16)
    if t in ("i32", "f32"):
        struct.pack_into("<I", b, 0, int(v["value"]) & 0xFFFFFFFF)
    elif t in ("i64", "f64"):
        struct.pack_into("<Q", b, 0, int(v["value"]) & 0xFFFFFFFFFFFFFFFF)
    elif t == "v128":
        fmt, sz = _lane_fmt(v["lane_type"])
        for i, lane in enumerate(v["value"]):
            struct.pack_into(fmt, b, i * sz, int(lane) & ((1 << (8 * sz)) - 1))
    else:
        raise ValueError(f"unsupported value type {t}")
    return b.hex()


def _float_match(bits, exp, width):
    """bits: actual integer bits. exp: expected string (decimal bits or nan pattern)."""
    if width == 32:
        exp_mask, frac_mask, quiet = 0x7F800000, 0x007FFFFF, 0x00400000
    else:
        exp_mask, frac_mask, quiet = 0x7FF0000000000000, 0x000FFFFFFFFFFFFF, 0x0008000000000000
    is_nan = (bits & exp_mask) == exp_mask and (bits & frac_mask) != 0
    if exp == "nan:canonical":
        return is_nan and (bits & frac_mask) == quiet
    if exp == "nan:arithmetic":
        return is_nan and (bits & quiet) != 0
    if exp == "nan:any":
        return is_nan
    return bits == (int(exp) & ((1 << width) - 1))


def match_value(slot_hex, v):
    b = bytes.fromhex(slot_hex)
    t = v["type"]
    if t == "i32":
        return struct.unpack_from("<I", b)[0] == int(v["value"]) & 0xFFFFFFFF
    if t == "i64":
        return struct.unpack_from("<Q", b)[0] == int(v["value"]) & 0xFFFFFFFFFFFFFFFF
    if t == "f32":
        return _float_match(struct.unpack_from("<I", b)[0], v["value"], 32)
    if t == "f64":
        return _float_match(struct.unpack_from("<Q", b)[0], v["value"], 64)
    if t == "v128":
        lt = v["lane_type"]
        fmt, sz = _lane_fmt(lt)
        for i, lane in enumerate(v["value"]):
            got = struct.unpack_from(fmt, b, i * sz)[0]
            if lt in ("f32", "f64"):
                if not _float_match(got, lane, 8 * sz):
                    return False
            elif got != int(lane) & ((1 << (8 * sz)) - 1):
                return False
        return True
    raise ValueError(f"unsupported value type {t}")


def slot_to_value(slot_hex, t):
    """Reference result slot -> wast2json expected value (for `record`)."""
    b = bytes.fromhex(slot_hex)
    if t == "i32":
        return {"type": t, "value": str(struct.unpack_from("<I", b)[0])}
    if t == "i64":
        return {"type": t, "value": str(struct.unpack_from("<Q", b)[0])}
    if t in ("f32", "f64"):
        w = 32 if t == "f32" else 64
        bits = struct.unpack_from("<I" if w == 32 else "<Q", b)[0]
        if _float_match(bits, "nan:canonical", w):
            return {"type": t, "value": "nan:canonical"}
        if _float_match(bits, "nan:arithmetic", w):
            return {"type": t, "value": "nan:arithmetic"}
        return {"type": t, "value": str(bits)}
    if t == "v128":
        return {"type": t, "lane_type": "i8", "value": [str(x) for x in b]}
    raise ValueError(t)


# ---------------------------------------------------------------------------
# Harness processes

class Crash(Exception):
    pass


class Proc:
    """A harness process speaking the line protocol (see PROTOCOL.md)."""

    def __init__(self, argv, cwd):
        self.p = subprocess.Popen(argv, cwd=cwd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                  stderr=subprocess.PIPE, text=True, bufsize=1)
        self.stderr_chunks = []
        self._t = threading.Thread(target=self._drain, daemon=True)
        self._t.start()
        self.lines = []
        self.cv = threading.Condition()
        self._r = threading.Thread(target=self._read, daemon=True)
        self._r.start()

    def _drain(self):
        for chunk in self.p.stderr:
            if sum(map(len, self.stderr_chunks)) < 65536:
                self.stderr_chunks.append(chunk)

    def _read(self):
        for line in self.p.stdout:
            with self.cv:
                self.lines.append(line.rstrip("\n"))
                self.cv.notify()
        with self.cv:
            self.lines.append(None)
            self.cv.notify()

    def cmd(self, line, timeout=COMMAND_TIMEOUT):
        try:
            self.p.stdin.write(line + "\n")
            self.p.stdin.flush()
        except (BrokenPipeError, OSError):
            raise Crash(self._crash_reason())
        deadline = time.monotonic() + timeout
        with self.cv:
            while not self.lines:
                left = deadline - time.monotonic()
                if left <= 0:
                    self.kill()
                    raise Crash("timeout")
                self.cv.wait(left)
            r = self.lines.pop(0)
        if r is None:
            raise Crash(self._crash_reason())
        return r

    def _crash_reason(self):
        try:
            rc = self.p.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.kill()
            rc = "?"
        return f"harness died (exit status {rc})"

    def kill(self):
        try:
            self.p.kill()
            self.p.wait(timeout=5)
        except Exception:
            pass

    def close(self):
        try:
            self.p.stdin.write("quit\n")
            self.p.stdin.flush()
            self.p.wait(timeout=5)
        except Exception:
            self.kill()


def build_harness(work):
    out = Path(work) / "bin" / "w2l_harness"
    src = HERE / "w2l_harness.c"
    if out.exists() and out.stat().st_mtime >= src.stat().st_mtime:
        return out
    out.parent.mkdir(parents=True, exist_ok=True)
    tmp = out.with_suffix(f".tmp{os.getpid()}")
    subprocess.run([which("clang"), "-O2", "-g", "-o", str(tmp), str(src), "-ldl", "-lpthread", "-rdynamic"],
                   check=True)
    os.replace(tmp, out)
    return out


class NativeBackend:
    name = "native"

    def __init__(self, translator, opt, work, translator_env=None):
        self.translator = str(Path(translator).resolve())
        self.opt = opt
        self.work = Path(work).resolve()
        self.harness = str(build_harness(self.work))
        self.clang = which("clang")
        self.translator_env = translator_env
        self.cache = self.work / "cache"
        self.cache.mkdir(parents=True, exist_ok=True)
        self._locks = {}
        self._glock = threading.Lock()

    def label(self):
        return f"native-O{self.opt}"

    def new_proc(self):
        return Proc([self.harness], cwd=str(self.work))

    def _lock(self, key):
        with self._glock:
            return self._locks.setdefault(key, threading.Lock())

    def translate(self, wasm):
        """Returns (ok, ll_path_or_error_message)."""
        # The cached .so path is passed directly to dlopen() by the harness.
        # Keying only on content would alias byte-identical modules that a
        # script loads as separate instances: dlopen() returns the same
        # mapping, so the second init would call w2l_init() on an
        # already-initialized instance. ABI.md says it is called exactly once.
        key = hashlib.sha256(
            str(Path(wasm).resolve()).encode() + b"\0" + Path(wasm).read_bytes()
        ).hexdigest()[:24]
        ll = self.cache / f"{key}.ll"
        err = self.cache / f"{key}.translate-error"
        with self._lock(key + "t"):
            if ll.exists():
                return True, ll
            if err.exists():
                return False, err.read_text()
            tmp = self.cache / f"{key}.tmp{threading.get_ident()}.ll"
            try:
                r = subprocess.run([self.translator, str(Path(wasm).resolve()), "-o", str(tmp)],
                                   capture_output=True, text=True, timeout=TRANSLATE_TIMEOUT,
                                   env=self.translator_env, cwd=str(self.cache), errors="replace")
                ok = r.returncode == 0 and tmp.exists()
                msg = f"translator exit status {r.returncode}: {r.stderr[-2000:]}"
            except subprocess.TimeoutExpired:
                ok, msg = False, "translator timeout"
            if ok:
                os.replace(tmp, ll)
                return True, ll
            tmp.unlink(missing_ok=True)
            err.write_text(msg)
            return False, msg

    def rejects(self, wasm):
        """For assert_invalid/assert_malformed: True if the translator refuses the module."""
        ok, _ = self.translate(wasm)
        return not ok

    def prepare(self, wasm):
        ok, ll = self.translate(wasm)
        if not ok:
            return False, f"translation failed: {ll}"
        so = ll.with_suffix(f".O{self.opt}.so")
        err = ll.with_suffix(f".O{self.opt}.compile-error")
        with self._lock(str(so)):
            if so.exists():
                return True, str(so)
            if err.exists():
                return False, err.read_text()
            tmp = so.with_suffix(f".tmp{threading.get_ident()}.so")
            try:
                r = subprocess.run([self.clang, f"-O{self.opt}", "-shared", "-fPIC", "-Wl,-Bsymbolic", "-w",
                                    "-o", str(tmp), str(ll), "-lm"],
                                   capture_output=True, text=True, timeout=COMPILE_TIMEOUT, errors="replace")
                ok = r.returncode == 0
                msg = f"clang failed: {r.stderr[-2000:]}"
            except subprocess.TimeoutExpired:
                ok, msg = False, "clang timeout"
            if ok:
                os.replace(tmp, so)
                return True, str(so)
            tmp.unlink(missing_ok=True)
            err.write_text(msg)
            return False, msg


class RefBackend:
    name = "ref"

    def __init__(self, work):
        self.ref = which("w2l-ref")
        self.work = Path(work).resolve()
        self.work.mkdir(parents=True, exist_ok=True)
        self.validator = which("wasm-tools")

    def label(self):
        return "ref"

    def new_proc(self):
        return Proc([self.ref], cwd=str(self.work))

    def rejects(self, wasm):
        r = subprocess.run([self.validator, "validate", FEATURES_ARG, str(wasm)], capture_output=True)
        return r.returncode != 0

    def prepare(self, wasm):
        return True, str(Path(wasm).resolve())


FEATURES_ARG = "--features=-all,mutable-global,bulk-memory,multi-value,saturating-float-to-int,sign-extension,simd,floats"


# ---------------------------------------------------------------------------
# Script execution

class Session:
    """Harness process plus a replay log so we can recover from crashes."""

    def __init__(self, backend):
        self.backend = backend
        self.proc = None
        self.log = []  # state-changing commands successfully sent
        self.restarts = 0

    def _ensure(self):
        if self.proc is None:
            self.proc = self.backend.new_proc()
            for line in self.log:
                try:
                    self.proc.cmd(line)
                except Crash:
                    self.proc = None
                    raise Crash("crash while replaying state after a previous crash")

    def cmd(self, line, stateful=True):
        if self.restarts > MAX_RESTARTS:
            raise Crash("too many harness crashes in this script")
        self._ensure()
        try:
            r = self.proc.cmd(line)
        except Crash as e:
            self.proc = None
            self.restarts += 1
            raise e
        if stateful:
            self.log.append(line)
        return r

    def close(self):
        if self.proc:
            self.proc.close()


def run_script(script_path, backend, verbose=False):
    script_path = Path(script_path)
    data = json.loads(script_path.read_text())
    base = script_path.parent
    sess = Session(backend)
    results = []
    cur = None          # id of current module (None if current module unavailable)
    cur_err = None      # why the current module is unavailable
    named = {}          # $name -> id or None
    named_paths = {}    # $name -> wasm path
    types = ResultTypes(base)
    nmod = 0
    out_seen = {}

    def record(cmd, ok, msg=""):
        results.append({"line": cmd.get("line"), "type": cmd["type"], "ok": ok, "msg": msg})
        if verbose and not ok:
            print(f"  FAIL {script_path.name}:{cmd.get('line')} {cmd['type']}: {msg}", file=sys.stderr)

    def module_id(cmd_or_action):
        name = cmd_or_action.get("module")
        if name is None:
            return cur, cur_err
        if name not in named:
            return None, f"unknown module {name}"
        return named[name], (None if named[name] else f"module {name} unavailable")

    def do_action(action):
        mid, err = module_id(action)
        if mid is None:
            raise Crash(err or "no module")
        if action["type"] == "invoke":
            args = " ".join(encode_value(a) for a in action.get("args", []))
            nres = action.get("_nres", 0)
            r = sess.cmd(f"invoke {mid} {hexname(action['field'])} {nres} {args}".rstrip())
        elif action["type"] == "get":
            r = sess.cmd(f"global {mid} {hexname(action['field'])}", stateful=False)
        elif action["type"] == "memwrite":
            r = sess.cmd(f"memwrite {mid} {hexname(action['memory'])} {int(action['offset'])} {action['hex']}")
        elif action["type"] == "memwrite_fn":
            wasm = named_paths.get(action["module"]) if action.get("module") else types.current
            if wasm is None:
                raise Crash("no module")
            try:
                params, results = types.func_sig(wasm, action["field"])
            except KeyError:
                raise Crash(f"memwrite_fn: no such export {action['field']}")
            except Exception as e:
                raise Crash(f"memwrite_fn: {e}")
            if params or len(results) != 1 or results[0] not in ("i32", "i64"):
                raise Crash(f"memwrite_fn {action['field']}: expected a function with no "
                            f"parameters and one i32/i64 result")
            r = sess.cmd(f"invoke {mid} {hexname(action['field'])} 1")
            if not r.startswith("ok "):
                return r
            b = bytes.fromhex(r.split()[1])
            off = int.from_bytes(b[:4], "little") if results[0] == "i32" else int.from_bytes(b[:8], "little")
            r = sess.cmd(f"memwrite {mid} {hexname(action['memory'])} {off} {action['hex']}")
        elif action["type"] == "memwrite_global":
            r = sess.cmd(f"memwrite_global {mid} {hexname(action['field'])} "
                         f"{hexname(action['memory'])} {action['hex']}")
        else:
            raise Crash(f"unsupported action {action['type']}")
        return r

    def load_module(filename):
        nonlocal nmod
        ok, art = backend.prepare(base / filename)
        if not ok:
            return None, art
        mid = f"m{nmod}"
        nmod += 1
        try:
            r = sess.cmd(f"load {mid} {art}")
            if not r.startswith("ok"):
                return None, r
            return mid, sess.cmd(f"init {mid}")
        except Crash as e:
            return None, str(e)

    try:
        for cmd in data["commands"]:
            t = cmd["type"]
            try:
                if t == "module":
                    mid, r = load_module(cmd["filename"])
                    types.current = base / cmd["filename"]
                    if mid is not None and r == "ok":
                        cur, cur_err = mid, None
                        record(cmd, True)
                    else:
                        cur, cur_err = None, f"module at line {cmd.get('line')} unavailable: {r}"
                        record(cmd, False, cur_err)
                    if cmd.get("name"):
                        named[cmd["name"]] = cur
                        named_paths[cmd["name"]] = types.current
                elif t in ("assert_return", "action"):
                    exp = cmd.get("expected", [])
                    cmd["action"]["_nres"] = len(exp)
                    r = do_action(cmd["action"])
                    if t == "action":
                        record(cmd, r.startswith("ok"), r)
                        continue
                    parts = r.split()
                    if parts[0] != "ok":
                        record(cmd, False, f"got '{r}'")
                    elif len(parts) - 1 != len(exp):
                        record(cmd, False, f"result count {len(parts) - 1} != {len(exp)}")
                    else:
                        bad = [i for i, (s, e) in enumerate(zip(parts[1:], exp)) if not match_value(s, e)]
                        record(cmd, not bad, "" if not bad else f"result {bad[0]}: got {parts[1 + bad[0]]}, expected {exp[bad[0]]}")
                elif t in ("assert_trap", "assert_exhaustion"):
                    want = 8 if t == "assert_exhaustion" else trap_code_for(cmd.get("text", ""))
                    cmd["action"]["_nres"] = len(cmd.get("expected", []))
                    r = do_action(cmd["action"])
                    record(cmd, r == f"trap {want}", f"got '{r}', expected trap {want} ({TRAP_NAMES.get(want)})")
                elif t == "assert_uninstantiable":
                    want = trap_code_for(cmd.get("text", ""))
                    mid, r = load_module(cmd["filename"])
                    record(cmd, mid is not None and r == f"trap {want}", f"got '{r}', expected trap {want}")
                elif t in ("assert_invalid", "assert_malformed"):
                    if cmd.get("module_type", "binary") != "binary":
                        continue
                    rej = backend.rejects(base / cmd["filename"])
                    record(cmd, rej, "" if rej else "module was accepted")
                elif t == "assert_output":
                    mid, err = module_id(cmd)
                    if mid is None:
                        raise Crash(err or "no module")
                    r = sess.cmd(f"output {mid}", stateful=False)
                    if not r.startswith("ok "):
                        record(cmd, False, f"got '{r}'")
                        continue
                    got = r.split()[1]
                    got = "" if got == "-" else got
                    record(cmd, got == cmd["hex"], f"host output mismatch: got {bytes.fromhex(got)[:200]!r}, "
                                                   f"expected {bytes.fromhex(cmd['hex'])[:200]!r}")
                elif t == "assert_memhash":
                    mid, err = module_id(cmd)
                    if mid is None:
                        raise Crash(err or "no module")
                    r = sess.cmd(f"memhash {mid} {hexname(cmd['memory'])}", stateful=False)
                    want = f"ok {cmd['size']} {cmd['fnv1a64']}"
                    record(cmd, r == want, f"got '{r}', expected '{want}'")
                else:
                    # register, assert_unlinkable, text modules etc. are not used by this benchmark
                    continue
            except Crash as e:
                record(cmd, False, str(e))
    finally:
        sess.close()
    return results


def find_scripts(paths):
    out = []
    for p in paths:
        p = Path(p)
        if p.is_dir():
            out += sorted(x for x in p.rglob("*.json")
                          if not any(part.startswith("_") for part in x.relative_to(p).parts))
        else:
            out.append(p)
    return out


def run_many(scripts, backends, jobs, verbose):
    """Returns {script: {backend_label: [results]}}."""
    res = {str(s): {} for s in scripts}
    with cf.ThreadPoolExecutor(max_workers=jobs) as ex:
        futs = {ex.submit(run_script, s, b, verbose): (s, b) for s in scripts for b in backends}
        for f in cf.as_completed(futs):
            s, b = futs[f]
            try:
                res[str(s)][b.label()] = f.result()
            except Exception as e:  # driver bug or malformed script
                res[str(s)][b.label()] = [{"line": None, "type": "driver", "ok": False, "msg": repr(e)}]
    return res


def summarize(res, labels):
    """An assertion passes only if it passes under every backend label."""
    total = passed = 0
    per_script = {}
    for s, by in res.items():
        rows = [by.get(l, []) for l in labels]
        n = max(len(r) for r in rows) if rows else 0
        sp = 0
        fails = []
        for i in range(n):
            entries = [r[i] if i < len(r) else None for r in rows]
            ok = all(e is not None and e["ok"] for e in entries)
            sp += ok
            if not ok:
                e = next((e for e in entries if e is not None and not e["ok"]), entries[0] or {})
                lab = labels[entries.index(e)] if e in entries else "?"
                fails.append({"line": e.get("line"), "type": e.get("type"), "backend": lab, "msg": e.get("msg", "")})
        per_script[s] = {"total": n, "passed": sp, "failures": fails}
        total += n
        passed += sp
    return {"total": total, "passed": passed, "scripts": per_script}


def cmd_run(a):
    scripts = find_scripts(a.tests)
    if not scripts:
        sys.exit("no test scripts found")
    work = Path(a.work)
    if a.backend == "ref":
        backends = [RefBackend(work / "ref")]
    else:
        tr = a.translator or str(Path.cwd() / "target" / "release" / "wasm2llvm")
        if not Path(tr).exists():
            sys.exit(f"translator not found: {tr} (build it or pass --translator)")
        backends = [NativeBackend(tr, int(o), work) for o in a.opt.split(",")]
    res = run_many(scripts, backends, a.jobs, a.verbose)
    labels = [b.label() for b in backends]
    summ = summarize(res, labels)
    for s, v in sorted(summ["scripts"].items()):
        mark = "PASS" if v["passed"] == v["total"] else "FAIL"
        print(f"{mark} {v['passed']:6d}/{v['total']:<6d} {s}")
        if a.verbose or a.show_failures:
            for f in v["failures"][: a.show_failures or 20]:
                print(f"      line {f['line']} {f['type']} [{f['backend']}]: {f['msg'][:300]}")
    print(f"\nTOTAL {summ['passed']}/{summ['total']} assertions passed ({', '.join(labels)})")
    if a.out:
        Path(a.out).write_text(json.dumps({"labels": labels, "summary": summ, "raw": res}, indent=1))
    return 0 if summ["passed"] == summ["total"] else 1


def record_script(script_path, out_path, work):
    """Replace every bare `action` by an assertion holding the reference result."""
    script_path = Path(script_path)
    data = json.loads(script_path.read_text())
    be = RefBackend(Path(work) / "ref")
    sess = Session(be)
    named, cur, n = {}, None, 0
    base = script_path.parent
    out_cmds = []
    types = ResultTypes(base)
    try:
        for cmd in data["commands"]:
            t = cmd["type"]
            if t == "module":
                mid = f"m{n}"; n += 1
                sess.cmd(f"load {mid} {(base / cmd['filename']).resolve()}")
                r = sess.cmd(f"init {mid}")
                if r != "ok":
                    raise SystemExit(f"{script_path}: module at line {cmd.get('line')} failed under reference: {r}")
                cur = mid
                types.current = base / cmd["filename"]
                if cmd.get("name"):
                    named[cmd["name"]] = (mid, types.current)
                out_cmds.append(cmd)
            elif t == "action":
                act = cmd["action"]
                mid, wasm = (named[act["module"]] if act.get("module") else (cur, types.current))
                if act["type"] == "memwrite_fn":
                    try:
                        params, results = types.func_sig(wasm, act["field"])
                    except KeyError:
                        raise SystemExit(f"{script_path}: memwrite_fn: no such export {act['field']}")
                    except Exception as e:
                        raise SystemExit(f"{script_path}: memwrite_fn: {e}")
                    if params or len(results) != 1 or results[0] not in ("i32", "i64"):
                        raise SystemExit(f"{script_path}: memwrite_fn {act['field']}: expected a function "
                                         f"with no parameters and one i32/i64 result")
                    r = sess.cmd(f"invoke {mid} {hexname(act['field'])} 1")
                    if not r.startswith("ok "):
                        raise SystemExit(f"{script_path}: memwrite_fn pointer call failed: {r}")
                    b = bytes.fromhex(r.split()[1])
                    off = int.from_bytes(b[:4], "little") if results[0] == "i32" else int.from_bytes(b[:8], "little")
                    r = sess.cmd(f"memwrite {mid} {hexname(act['memory'])} {off} {act['hex']}")
                    if not r.startswith("ok"):
                        raise SystemExit(f"{script_path}: memwrite_fn write failed: {r}")
                    out_cmds.append(cmd)
                    continue
                if act["type"] in ("memwrite", "memwrite_global"):
                    if act["type"] == "memwrite":
                        line = f"memwrite {mid} {hexname(act['memory'])} {int(act['offset'])} {act['hex']}"
                    else:
                        line = (f"memwrite_global {mid} {hexname(act['field'])} "
                                f"{hexname(act['memory'])} {act['hex']}")
                    r = sess.cmd(line)
                    if not r.startswith("ok"):
                        raise SystemExit(f"{script_path}: {act['type']} failed under reference: {r}")
                    out_cmds.append(cmd)
                    continue
                if act["type"] == "invoke":
                    rtypes = types.results(wasm, act["field"])
                    args = " ".join(encode_value(x) for x in act.get("args", []))
                    r = sess.cmd(f"invoke {mid} {hexname(act['field'])} {len(rtypes)} {args}".rstrip())
                else:
                    rtypes = [types.global_type(wasm, act["field"])]
                    r = sess.cmd(f"global {mid} {hexname(act['field'])}")
                parts = r.split()
                if parts[0] == "ok":
                    out_cmds.append({"type": "assert_return", "line": cmd.get("line"), "action": act,
                                     "expected": [slot_to_value(s, ty) for s, ty in zip(parts[1:], rtypes)]})
                elif parts[0] == "trap":
                    code = int(parts[1])
                    if code == 8:
                        out_cmds.append({"type": "assert_exhaustion", "line": cmd.get("line"), "action": act,
                                         "text": "call stack exhausted"})
                    elif code in TRAP_NAMES and code != 9:
                        text = {5: "out of bounds table access"}.get(code, TRAP_NAMES[code])
                        out_cmds.append({"type": "assert_trap", "line": cmd.get("line"), "action": act, "text": text})
                    else:
                        raise SystemExit(f"{script_path}: unexpected reference trap {r}")
                else:
                    raise SystemExit(f"{script_path}: reference error: {r}")
            elif t == "record_output":
                mid = named[cmd["module"]][0] if cmd.get("module") else cur
                r = sess.cmd(f"output {mid}", stateful=False)
                h = r.split()[1]
                out_cmds.append({"type": "assert_output", "line": cmd.get("line"), "module": cmd.get("module"),
                                 "hex": "" if h == "-" else h})
            elif t == "record_memhash":
                mid = named[cmd["module"]][0] if cmd.get("module") else cur
                r = sess.cmd(f"memhash {mid} {hexname(cmd['memory'])}", stateful=False).split()
                out_cmds.append({"type": "assert_memhash", "line": cmd.get("line"), "module": cmd.get("module"),
                                 "memory": cmd["memory"], "size": int(r[1]), "fnv1a64": r[2]})
            else:
                out_cmds.append(cmd)
    finally:
        sess.close()
    data["commands"] = out_cmds
    Path(out_path).write_text(json.dumps(data, indent=1))


class ResultTypes:
    """Minimal wasm binary reader: export -> function result types / global type."""

    VT = {0x7F: "i32", 0x7E: "i64", 0x7D: "f32", 0x7C: "f64", 0x7B: "v128"}

    def __init__(self, base):
        self.cache = {}
        self.current = None

    def _parse(self, path):
        if path in self.cache:
            return self.cache[path]
        b = Path(path).read_bytes()
        pos = 8
        types, funcs, globs, exports = [], [], [], {}
        nimp_funcs = []

        def leb(p):
            r = s = 0
            while True:
                x = b[p]; p += 1
                r |= (x & 0x7F) << s; s += 7
                if x < 0x80:
                    return r, p

        def name(p):
            n, p = leb(p)
            return b[p:p + n].decode("utf-8"), p + n

        def limits(p):
            fl = b[p]; p += 1
            _, p = leb(p)
            if fl & 1:
                _, p = leb(p)
            return p

        while pos < len(b):
            sid = b[pos]; pos += 1
            size, pos = leb(pos)
            end = pos + size
            p = pos
            if sid == 1:
                n, p = leb(p)
                for _ in range(n):
                    p += 1  # 0x60
                    np_, p = leb(p)
                    params = [self.VT.get(x, "?") for x in b[p:p + np_]]; p += np_
                    nr, p = leb(p)
                    results = [self.VT.get(x, "?") for x in b[p:p + nr]]; p += nr
                    types.append((params, results))
            elif sid == 2:
                n, p = leb(p)
                for _ in range(n):
                    _, p = name(p); _, p = name(p)
                    k = b[p]; p += 1
                    if k == 0:
                        ti, p = leb(p); funcs.append(ti)
                    elif k == 1:
                        p += 1; p = limits(p)
                    elif k == 2:
                        p = limits(p)
                    elif k == 3:
                        globs.append(self.VT.get(b[p], "?")); p += 2
            elif sid == 3:
                n, p = leb(p)
                for _ in range(n):
                    ti, p = leb(p); funcs.append(ti)
            elif sid == 6:
                n, p = leb(p)
                for _ in range(n):
                    globs.append(self.VT.get(b[p], "?"))
                    p += 2
                    # skip init expr up to `end` (0x0b); constant exprs here contain no nested 0x0b
                    # except inside immediates, so decode the single instruction.
                    op = b[p]; p += 1
                    if op in (0x41, 0x42):
                        while b[p] & 0x80: p += 1
                        p += 1
                    elif op == 0x43: p += 4
                    elif op == 0x44: p += 8
                    elif op == 0x23: _, p = leb(p)
                    elif op == 0xD2: _, p = leb(p)
                    elif op == 0xD0: p += 1
                    elif op == 0xFD: _, p = leb(p); p += 16
                    p += 1  # end
            elif sid == 7:
                n, p = leb(p)
                for _ in range(n):
                    nm, p = name(p)
                    k = b[p]; p += 1
                    idx, p = leb(p)
                    exports[nm] = (k, idx)
            pos = end
        self.cache[path] = (types, funcs, globs, exports)
        return self.cache[path]

    def func_sig(self, path, field):
        types, funcs, _, exports = self._parse(path)
        k, idx = exports[field]
        if k != 0:
            raise ValueError(f"{field} is not a function export")
        return types[funcs[idx]]

    def results(self, path, field):
        return self.func_sig(path, field)[1]

    def global_type(self, path, field):
        _, _, globs, exports = self._parse(path)
        k, idx = exports[field]
        return globs[idx]


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("run")
    r.add_argument("tests", nargs="+")
    r.add_argument("--translator")
    r.add_argument("--backend", choices=["native", "ref"], default="native")
    r.add_argument("--opt", default="0,2", help="comma-separated clang -O levels (default 0,2)")
    r.add_argument("-j", "--jobs", type=int, default=os.cpu_count())
    r.add_argument("--work", default=".w2l-work")
    r.add_argument("--out")
    r.add_argument("-v", "--verbose", action="store_true")
    r.add_argument("--show-failures", type=int, default=0, metavar="N")
    rc = sub.add_parser("record")
    rc.add_argument("script")
    rc.add_argument("out")
    rc.add_argument("--work", default=".w2l-work")
    a = ap.parse_args()
    if a.cmd == "run":
        sys.exit(cmd_run(a))
    else:
        record_script(a.script, a.out, a.work)


if __name__ == "__main__":
    main()

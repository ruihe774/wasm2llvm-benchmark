"""Differential test driver: compares wasm2llvm-generated .so behavior with
w2l-ref (wasmtime).  Test-only; not part of the translator."""
import os
import struct
import subprocess

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TOOLS = os.environ.get("W2L_TOOLS", "/mnt/workspace/wasm2llvm/deps/bin")
HARNESS = os.environ.get("W2L_HARNESS", os.path.join(os.environ.get("TMPDIR", "/tmp"), "w2l-harness"))
WASM2LLVM = os.environ.get("W2L_BIN", os.path.join(ROOT, "target", "release", "wasm2llvm"))
BUILD = os.path.join(os.environ.get("TMPDIR", "/tmp"), "w2l-build")
os.makedirs(BUILD, exist_ok=True)


def translate(wasm_path, opt="O2", tag=""):
    name = os.path.basename(wasm_path).replace(".wasm", "") + tag
    ll = os.path.join(BUILD, f"{name}.ll")
    so = os.path.join(BUILD, f"{name}.{opt}.so")
    r = subprocess.run([WASM2LLVM, wasm_path, "-o", ll], capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError(f"translate failed for {wasm_path}: {r.stderr}")
    cmd = ["clang", f"-{opt}", "-shared", "-fPIC", "-Wl,-Bsymbolic", "-o", so, ll, "-lm"]
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError(f"clang failed for {ll}: {r.stderr[:4000]}")
    return so


class Session:
    def __init__(self, argv):
        self.p = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                  stderr=subprocess.PIPE, text=True, bufsize=1)
        self.cmds = []

    def send(self, line):
        self.cmds.append(line)
        self.p.stdin.write(line + "\n")
        self.p.stdin.flush()
        resp = self.p.stdout.readline()
        if resp == "":
            err = self.p.stderr.read()
            raise RuntimeError(f"session died on {line!r}: {err[:2000]}")
        return resp.rstrip("\n")

    def close(self):
        try:
            self.p.stdin.write("quit\n")
            self.p.stdin.flush()
        except Exception:
            pass
        self.p.wait(timeout=30)


def run_scenario(wasm_path, so_path, scenario):
    """Run a scenario against both engines; returns (ref_trace, got_trace)."""
    ref = Session([os.path.join(TOOLS, "w2l-ref")])
    ref.send(f"load a {wasm_path}")
    rtrace = scenario(ref)
    ref.close()

    got = Session([HARNESS])
    got.send(f"load a {so_path}")
    gtrace = scenario(got)
    got.close()
    global LAST_REF_CMDS, LAST_GOT_CMDS
    LAST_REF_CMDS = ref.cmds
    LAST_GOT_CMDS = got.cmds
    return rtrace, gtrace


LAST_REF_CMDS = []
LAST_GOT_CMDS = []


def compare_scenario(wasm_path, scenario, opt="O2", verbose=False, tag=""):
    if verbose:
        print("translating", wasm_path)
    so = translate(wasm_path, opt, tag)
    ref, got = run_scenario(wasm_path, so, scenario)
    ok = True
    for i in range(max(len(ref), len(got))):
        a = ref[i] if i < len(ref) else "<missing>"
        b = got[i] if i < len(got) else "<missing>"
        if a != b:
            ok = False
            cmd = LAST_REF_CMDS[i + 1] if i + 1 < len(LAST_REF_CMDS) else "?"
            print(f"MISMATCH response {i} (cmd {cmd!r}):\n  ref: {a}\n  got: {b}")
    return ok


def slot_i32(v):
    return struct.pack("<I", v & 0xFFFFFFFF).hex() + "0" * 24


def slot_u32(v):
    return struct.pack("<I", v).hex() + "0" * 24


def slot_i64(v):
    return struct.pack("<Q", v & 0xFFFFFFFFFFFFFFFF).hex() + "0" * 16


def slot_u64(v):
    return struct.pack("<Q", v).hex() + "0" * 16


def slot_f32(v):
    return struct.pack("<f", v).hex() + "0" * 24


def slot_f32b(bits):
    return struct.pack("<I", bits).hex() + "0" * 24


def slot_f64(v):
    return struct.pack("<d", v).hex() + "0" * 16


def slot_f64b(bits):
    return struct.pack("<Q", bits).hex() + "0" * 16


def slot_raw(b):
    assert len(b) <= 16
    return b.hex() + "0" * (32 - 2 * len(b))


def hname(s):
    return s.encode().hex()


def hx(b):
    return b.hex() if b else "-"


def slot_result_i32(resp):
    """Parse the first result slot of an 'ok ...' response as i32."""
    parts = resp.split()
    return struct.unpack("<i", bytes.fromhex(parts[1])[:4])[0]


def slot_result_u32(resp):
    parts = resp.split()
    return struct.unpack("<I", bytes.fromhex(parts[1])[:4])[0]


def slot_result_i64(resp):
    parts = resp.split()
    return struct.unpack("<q", bytes.fromhex(parts[1])[:8])[0]

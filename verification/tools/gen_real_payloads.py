#!/usr/bin/env python3
"""Generate hidden tests for the real-world payloads.

  gen_real_payloads.py [CASE ...]

Reads the payload binaries from verification/payloads/real and writes
verification/tests/real/<case>/{<case>.json,<case>.wasm}. Expectations
(return values, frame metadata and memory hashes) are recorded with the
wasmtime reference runner. Inputs are deterministic and are not part of the
visible payload directory.
"""

import json
import math
import os
import random
import shutil
import struct
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
VER = ROOT / "verification"
SRC = VER / "payloads" / "real"
OUT = VER / "tests" / "real"
os.environ["PATH"] = f"{ROOT / 'deps' / 'bin'}:{os.environ.get('PATH', '')}"

sys.dont_write_bytecode = True
sys.path.insert(0, str(VER / "harness"))
import w2l  # noqa: E402


def i32_args(vals):
    return [{"type": "i32", "value": str(v)} for v in vals]


def f32_bytes(xs):
    return b"".join(struct.pack("<f", x) for x in xs)


def i16_bytes(xs):
    out = bytearray()
    for x in xs:
        v = max(-32768, min(32767, int(round(x))))
        out += struct.pack("<h", v)
    return bytes(out)


def sine(n, freq, rate, amp, phase=0.0):
    return [amp * math.sin(2 * math.pi * freq * i / rate + phase) for i in range(n)]


def noise(n, amp, seed):
    rng = random.Random(seed)
    return [rng.uniform(-amp, amp) for _ in range(n)]


def ref_global(wasm, name, work):
    """Read an i32 global from a fresh reference instance."""
    be = w2l.RefBackend(work / "ref")
    p = be.new_proc()
    try:
        r = p.cmd(f"load m {Path(wasm).resolve()}")
        if r != "ok":
            sys.exit(f"{wasm}: load failed: {r}")
        r = p.cmd("init m")
        if r != "ok":
            sys.exit(f"{wasm}: init failed: {r}")
        r = p.cmd(f"global m {w2l.hexname(name)}")
        if not r.startswith("ok "):
            sys.exit(f"{wasm}: global {name} failed: {r}")
        slot = bytes.fromhex(r.split()[1])
        return int.from_bytes(slot[:4], "little")
    finally:
        p.close()


def check_no_traps(path):
    for c in json.loads(Path(path).read_text())["commands"]:
        if c["type"] in ("assert_trap", "assert_exhaustion"):
            sys.exit(f"{path}: reference trapped at line {c.get('line')}: {c.get('text')}")


def write_case(name, wasm, commands, work):
    out = OUT / name
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)
    shutil.copy(wasm, out / f"{name}.wasm")
    commands[0]["filename"] = f"{name}.wasm"
    raw = out / "_raw.json"
    raw.write_text(json.dumps({"source_filename": f"{name}.wasm", "commands": commands}))
    w2l.record_script(raw, out / f"{name}.json", work)
    raw.unlink()
    check_no_traps(out / f"{name}.json")
    print(f"{name:22s} {wasm.name}")


def qoa_case(name, channels, samples, rate, wave, work, avail=None, dec_dst=None):
    wasm = SRC / "qoa-simd.wasm"
    heap = ref_global(wasm, "heap_base", work)
    data = i16_bytes(wave)
    in_bytes = len(data)
    dst = heap + ((in_bytes + 15) // 16) * 16
    dec_dst = dec_dst if dec_dst is not None else dst + 20000
    avail = avail if avail is not None else 16384
    cmds = [
        {"type": "module", "line": 1},
        {"type": "action", "line": 2,
         "action": {"type": "invoke", "field": "encode_init", "args": i32_args([channels])}},
        {"type": "action", "line": 3,
         "action": {"type": "memwrite_global", "field": "heap_base", "memory": "memory", "hex": data.hex()}},
        {"type": "action", "line": 4,
         "action": {"type": "invoke", "field": "encode_frame",
                    "args": i32_args([heap, channels, rate, samples, dst])}},
        {"type": "record_memhash", "line": 5, "memory": "memory"},
        {"type": "action", "line": 6,
         "action": {"type": "invoke", "field": "decode_frame", "args": i32_args([dst, avail, dec_dst])}},
        {"type": "action", "line": 7, "action": {"type": "get", "field": "frame_channels"}},
        {"type": "action", "line": 8, "action": {"type": "get", "field": "frame_samplerate"}},
        {"type": "action", "line": 9, "action": {"type": "get", "field": "frame_samples"}},
        {"type": "record_memhash", "line": 10, "memory": "memory"},
        {"type": "record_output", "line": 11},
    ]
    write_case(name, wasm, cmds, work)


def celt_case(name, wasm_name, channels, kind, wave, work, bitrate=None, complexity=10):
    wasm = SRC / wasm_name
    bitrate = bitrate if bitrate is not None else (96000 if channels == 2 else 64000)
    data = f32_bytes(wave) if kind == "f32" else i16_bytes(wave)
    ptr = "minicelt_input_ptr" if kind == "f32" else "minicelt_input_i16_ptr"
    enc = "minicelt_encode" if kind == "f32" else "minicelt_encode_i16"
    cmds = [
        {"type": "module", "line": 1},
        {"type": "action", "line": 2,
         "action": {"type": "invoke", "field": "minicelt_init",
                    "args": i32_args([channels, bitrate, complexity, 0, 0])}},
        {"type": "action", "line": 3,
         "action": {"type": "memwrite_fn", "field": ptr, "memory": "memory", "hex": data.hex()}},
        {"type": "action", "line": 4, "action": {"type": "invoke", "field": enc, "args": []}},
        {"type": "action", "line": 5, "action": {"type": "invoke", "field": "minicelt_final_range", "args": []}},
        {"type": "record_memhash", "line": 6, "memory": "memory"},
        {"type": "record_output", "line": 7},
    ]
    write_case(name, wasm, cmds, work)


def silk_case(name, wasm_name, kind, wave, work, bitrate=32000, complexity=10):
    wasm = SRC / wasm_name
    data = f32_bytes(wave) if kind == "f32" else i16_bytes(wave)
    ptr = "minisilk_input_ptr" if kind == "f32" else "minisilk_input_i16_ptr"
    enc = "minisilk_encode" if kind == "f32" else "minisilk_encode_i16"
    cmds = [
        {"type": "module", "line": 1},
        {"type": "action", "line": 2,
         "action": {"type": "invoke", "field": "minisilk_init",
                    "args": i32_args([bitrate, complexity, 0])}},
        {"type": "action", "line": 3,
         "action": {"type": "memwrite_fn", "field": ptr, "memory": "memory", "hex": data.hex()}},
        {"type": "action", "line": 4, "action": {"type": "invoke", "field": enc, "args": []}},
        {"type": "action", "line": 5, "action": {"type": "invoke", "field": "minisilk_final_range", "args": []}},
        {"type": "record_memhash", "line": 6, "memory": "memory"},
        {"type": "record_output", "line": 7},
    ]
    write_case(name, wasm, cmds, work)


def main():
    only = set(sys.argv[1:])
    work = Path(tempfile.mkdtemp(prefix="real-payloads-"))
    OUT.mkdir(parents=True, exist_ok=True)

    def want(name):
        return not only or name in only

    if want("qoa-mono"):
        qoa_case("qoa-mono", 1, 5120, 44100,
                 [12000 * math.sin(2 * math.pi * 440 * i / 44100)
                  + 3000 * math.sin(2 * math.pi * 1320 * i / 44100) for i in range(5120)], work)
    if want("qoa-stereo"):
        left = sine(5120, 440, 44100, 10000)
        right = sine(5120, 660, 44100, 10000)
        qoa_case("qoa-stereo", 2, 5120, 44100,
                 [v for pair in zip(left, right) for v in pair], work)

    if want("celt-mono-f32"):
        celt_case("celt-mono-f32", "minicelt.wasm", 1, "f32",
                  sine(960, 440, 48000, 0.5), work)
    if want("celt-stereo-f32"):
        left = sine(960, 440, 48000, 0.4)
        right = sine(960, 880, 48000, 0.4)
        celt_case("celt-stereo-f32", "minicelt.wasm", 2, "f32",
                  [v for pair in zip(left, right) for v in pair], work)
    if want("celt-mono-i16"):
        celt_case("celt-mono-i16", "minicelt.wasm", 1, "i16",
                  [int(12000 * math.sin(2 * math.pi * 440 * i / 48000)) for i in range(960)], work)
    if want("celt-noise-f32"):
        celt_case("celt-noise-f32", "minicelt.wasm", 1, "f32", noise(960, 0.5, 101), work)
    if want("celt-checked-f32"):
        celt_case("celt-checked-f32", "minicelt-checked.wasm", 1, "f32",
                  sine(960, 523, 48000, 0.4), work, bitrate=48000)

    if want("silk-mono-f32"):
        silk_case("silk-mono-f32", "minisilk.wasm", "f32", sine(320, 440, 16000, 0.4), work)
    if want("silk-mono-i16"):
        silk_case("silk-mono-i16", "minisilk.wasm", "i16",
                  [int(12000 * math.sin(2 * math.pi * 440 * i / 16000)) for i in range(320)], work)
    if want("silk-noise-f32"):
        silk_case("silk-noise-f32", "minisilk.wasm", "f32", noise(320, 0.4, 202), work, bitrate=24000)
    if want("silk-checked-f32"):
        silk_case("silk-checked-f32", "minisilk-checked.wasm", "f32",
                  sine(320, 523, 16000, 0.4), work, bitrate=24000)

    shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    main()

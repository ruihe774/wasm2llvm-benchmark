#!/usr/bin/env python3
"""Deterministic ABI differential tests against w2l-ref, using only Python stdlib.

Run: python3 tests/test.py [--quick] [--no-build] [--only NAME] [--keep]
The native harness is built with exported host symbols and checks both -O0/-O2.
"""
import argparse
import math
import os
from pathlib import Path
import random
import select
import shutil
import struct
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]


def name(text):
    return text.encode().hex() or "-"


def slot(value, kind="i32"):
    if kind == "v128":
        raw = bytes(value)
        assert len(raw) == 16
    elif kind in ("f32", "f64"):
        raw = struct.pack("<f" if kind == "f32" else "<d", value)
    else:
        width = 4 if kind == "i32" else 8
        raw = (int(value) % (1 << (width * 8))).to_bytes(width, "little")
    return raw.ljust(16, b"\0").hex()


def integer(value, signed=False, width=4):
    return int.from_bytes(bytes.fromhex(value)[:width], "little", signed=signed)


def run(argv, **kwargs):
    result = subprocess.run([str(a) for a in argv], text=True, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, timeout=180, **kwargs)
    if result.returncode:
        raise AssertionError(f"command failed ({result.returncode}): {' '.join(map(str, argv))}\n"
                             f"{result.stdout}\n{result.stderr}")
    return result.stdout


class Driver:
    def __init__(self, argv, timeout=30):
        self.argv = list(map(str, argv))
        self.timeout = timeout
        self.errors = tempfile.TemporaryFile()
        self.process = subprocess.Popen(self.argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=self.errors, bufsize=0)

    def command(self, command):
        process = self.process
        if process.poll() is not None:
            self.errors.seek(0)
            raise AssertionError(f"driver exited {process.returncode}: {self.argv}\n"
                                 f"{self.errors.read().decode(errors='replace')}")
        process.stdin.write((command + "\n").encode())
        process.stdin.flush()
        ready, _, _ = select.select([process.stdout], [], [], self.timeout)
        if not ready:
            raise AssertionError(f"driver timed out: {command[:200]}")
        reply = process.stdout.readline().decode().strip()
        if not reply:
            self.errors.seek(0)
            raise AssertionError(f"driver returned EOF: {command[:200]}\n"
                                 f"{self.errors.read().decode(errors='replace')}")
        return reply

    def close(self):
        p = self.process
        if p.poll() is None:
            try:
                p.stdin.write(b"quit\n")
                p.stdin.flush()
                p.wait(timeout=2)
            except (BrokenPipeError, subprocess.TimeoutExpired):
                p.kill()
                p.wait()
        p.stdin.close()
        p.stdout.close()
        self.errors.close()

    def __enter__(self):
        return self

    def __exit__(self, *args):
        self.close()


def equivalent_slots(a, b, kind):
    width = {"i32": 4, "i64": 8, "f32": 4, "f64": 8, "v128": 16}[kind]
    x, y = bytes.fromhex(a)[:width], bytes.fromhex(b)[:width]
    if x == y:
        return True
    if kind in ("f32", "f64"):
        fmt = "<f" if kind == "f32" else "<d"
        return math.isnan(struct.unpack(fmt, x)[0]) and math.isnan(struct.unpack(fmt, y)[0])
    return False


class Pair:
    def __init__(self, reference, native):
        self.reference = reference
        self.native = native
        self.commands = 0

    def command(self, cmd, types=None):
        expected = self.reference.command(cmd)
        actual = self.native.command(cmd)
        self.commands += 1
        okay = expected == actual
        if types is not None and expected.split()[:1] == actual.split()[:1] == ["ok"]:
            e, a = expected.split()[1:], actual.split()[1:]
            okay = len(e) == len(a) == len(types) and all(
                equivalent_slots(x, y, kind) for x, y, kind in zip(e, a, types))
        if not okay:
            raise AssertionError(f"differential mismatch at command #{self.commands}\n"
                                 f"{cmd[:500]}\nreference: {expected}\nnative:    {actual}")
        if expected.startswith("error"):
            raise AssertionError(f"driver error: {cmd[:200]} -> {expected}")
        return expected

    def invoke(self, export, args=(), results=("i32",)):
        cmd = f"invoke m {name(export)} {len(results)}" + "".join(" " + a for a in args)
        reply = self.command(cmd, types=results)
        return reply.split()[1:] if reply.startswith("ok") else reply

    def global_(self, export, kind="i32"):
        return self.command(f"global m {name(export)}", types=(kind,)).split()[1]

    def memory_hash(self, export="memory"):
        return self.command(f"memhash m {name(export)}")

    def write(self, offset, data, export="memory"):
        return self.command(f"memwrite m {name(export)} {offset} {bytes(data).hex()}")

    def read(self, offset, length, export="memory"):
        return self.command(f"memread m {name(export)} {offset} {length}").split(maxsplit=1)[-1]


def build_module(wasm, work, opt, translator):
    wasm, work = Path(wasm), Path(work)
    work.mkdir(parents=True, exist_ok=True)
    ll = work / (wasm.stem + ".ll")
    library = work / (wasm.stem + f"-O{opt}.so")
    if not ll.exists():
        run([translator, wasm, "-o", ll])
    run(["clang", f"-O{opt}", "-shared", "-fPIC", "-Wl,-Bsymbolic", "-o", library, ll, "-lm"])
    return library


SCALAR_WAT = r'''(module
  (import "env" "print_i32" (func $print (param i32)))
  (memory (export "memory") 1 2)
  (global $g (export "g") (mut i32) (i32.const 9))
  (func $start i32.const 7 call $print)
  (start $start)
  (func (export "add") (param i32 i32) (result i32) local.get 0 local.get 1 i32.add)
  (func (export "rot") (param i32 i32) (result i32) local.get 0 local.get 1 i32.rotl)
  (func (export "div") (param i32 i32) (result i32) local.get 0 local.get 1 i32.div_s)
  (func (export "rem") (param i32 i32) (result i32) local.get 0 local.get 1 i32.rem_s)
  (func (export "i64div") (param i64 i64) (result i64) local.get 0 local.get 1 i64.div_s)
  (func (export "ext") (param i64) (result i64) local.get 0 i64.extend32_s)
  (func (export "sat") (param f64) (result i32) local.get 0 i32.trunc_sat_f64_s)
  (func (export "trunc") (param f64) (result i32) local.get 0 i32.trunc_f64_s)
  (func (export "min") (param f64 f64) (result f64) local.get 0 local.get 1 f64.min)
  (func (export "max") (param f32 f32) (result f32) local.get 0 local.get 1 f32.max)
  (func (export "nearest") (param f64) (result f64) local.get 0 f64.nearest)
  (func (export "choose") (param i32 i32 i32) (result i32) local.get 0 local.get 1 local.get 2 select)
  (func (export "sum") (param i32) (result i32) (local i32)
    block $end loop $again
      local.get 0 i32.eqz br_if $end
      local.get 1 local.get 0 i32.add local.set 1
      local.get 0 i32.const 1 i32.sub local.set 0
      br $again
    end end local.get 1)
  (func (export "pair") (param i32 i64) (result i64 i32)
    local.get 0 local.get 1
    block (param i32 i64) (result i64 i32)
      local.set 1 local.set 0 local.get 1 local.get 0
    end)
  (func (export "ret") (param i32) (result i32)
    block local.get 0 return end unreachable)
  (func (export "store") (param i32 i64) local.get 0 local.get 1 i64.store align=1)
  (func (export "load") (param i32) (result i64) local.get 0 i64.load align=1)
  (func (export "size") (result i32) memory.size)
  (func (export "grow") (param i32) (result i32) local.get 0 memory.grow)
  (func (export "bump") (result i32) global.get $g i32.const 1 i32.add global.set $g global.get $g)
  (func (export "fail") (param i32) local.get 0 global.set $g unreachable)
)'''


def scalar_tests(p):
    p.command("output m")
    rng = random.Random(123456)
    for a, b in [(0, 0), (-1, 1), (-(1 << 31), -1), ((1 << 31)-1, 33)] + [
            (rng.randrange(-(1 << 31), 1 << 31), rng.randrange(-64, 64)) for _ in range(24)]:
        for op in ("add", "rot", "div", "rem"):
            p.invoke(op, [slot(a), slot(b)])
        p.invoke("choose", [slot(a), slot(b), slot(a & 1)])
        p.invoke("i64div", [slot(a << 25, "i64"), slot(b, "i64")], ("i64",))
        p.invoke("ext", [slot(a * 0x123456789, "i64")], ("i64",))
    for x in [-math.inf, -2147483649.0, -2147483648.75, -0.0, 0.0, 0.5, 1.5, 2.5,
              -0.5, -1.5, 2147483647.5, 2147483648.0, math.inf, math.nan]:
        p.invoke("sat", [slot(x, "f64")])
        p.invoke("trunc", [slot(x, "f64")])
        p.invoke("nearest", [slot(x, "f64")], ("f64",))
        for y in [0.0, -0.0, 1.0, math.nan]:
            p.invoke("min", [slot(x, "f64"), slot(y, "f64")], ("f64",))
            p.invoke("max", [slot(x, "f32"), slot(y, "f32")], ("f32",))
    for n in [0, 1, 10, 1000]:
        p.invoke("sum", [slot(n)])
        p.invoke("ret", [slot(n)])
        p.invoke("pair", [slot(n), slot(n * 100000000003, "i64")], ("i64", "i32"))
    for offset in [0, 1, 65528, 65529, -1]:
        p.invoke("store", [slot(offset), slot(0xabcdef0123456789, "i64")], ())
        p.invoke("load", [slot(offset)], ("i64",))
    p.invoke("fail", [slot(37)], ())
    p.global_("g")
    p.invoke("bump")
    p.invoke("grow", [slot(1)])
    p.invoke("size")
    p.read(65536, 32)
    p.invoke("grow", [slot(1)])
    p.invoke("grow", [slot(-1)])
    p.memory_hash()


CONTROL_WAT = r'''(module
  (func (export "dead_nested") (result i32)
    block (result i32)
      i32.const 17 br 0
      block (result i64) unreachable end drop
    end)
  (func (export "loop_pair") (param $n i32) (result i64 i32) (local $acc i64)
    local.get $n i64.const 0
    loop (param i32 i64) (result i64 i32)
      local.set $acc local.set $n
      local.get $n i32.eqz
      if (result i64 i32)
        local.get $acc i32.const 55
      else
        local.get $n i32.const 1 i32.sub
        local.get $acc local.get $n i64.extend_i32_u i64.add
        br 1
      end
    end)
  (func (export "conditional_backedge") (param i32) (result i32)
    local.get 0
    loop (param i32) (result i32)
      i32.const 1 i32.sub local.tee 0
      local.get 0 br_if 0
    end)
  (func (export "implicit_else") (param i32) (result i64)
    i64.const 42 local.get 0
    if (param i64) (result i64) i64.const 1 i64.add end)
  (func (export "dead_else") (param i32) (result i32)
    local.get 0 if (result i32)
      i32.const 9
    else
      unreachable
      block (result i64) unreachable end drop
    end)
  (func (export "polymorphic_table") (result i32)
    block (result i32)
      block (result i64)
        unreachable br_table 0 1
      end
      drop unreachable
    end)
  (func (export "nested_returns") (param i32) (result i64 i32)
    block
      loop
        local.get 0 if
          i64.const 123 i32.const 456 return
        else
          i32.const 1 local.set 0 br 1
        end
      end
    end
    unreachable)
)'''


def control_tests(p):
    p.invoke("dead_nested")
    for n in [0, 1, 2, 10, 99]:
        p.invoke("loop_pair", [slot(n)], ("i64", "i32"))
        p.invoke("implicit_else", [slot(n)], ("i64",))
        p.invoke("dead_else", [slot(n)])
        p.invoke("nested_returns", [slot(n)], ("i64", "i32"))
    for n in [1, 2, 10, 100]:
        p.invoke("conditional_backedge", [slot(n)])
    p.invoke("polymorphic_table")


BULK_WAT = r'''(module
  (type $unary (func (param i32) (result i32)))
  (memory (export "memory") 1 2)
  (table 5 funcref)
  (data (i32.const 4) "ABCD")
  (data $bytes "0123456789abcdef")
  (func $add (type $unary) local.get 0 i32.const 10 i32.add)
  (func $sub (type $unary) local.get 0 i32.const 10 i32.sub)
  (func $wrong (result i64) i64.const 9)
  (elem (i32.const 0) func $add $wrong)
  (elem $funcs func $sub $add)
  (elem declare func $wrong)
  (func (export "init") (param i32 i32 i32) local.get 0 local.get 1 local.get 2 memory.init $bytes)
  (func (export "drop") data.drop $bytes)
  (func (export "copy") (param i32 i32 i32) local.get 0 local.get 1 local.get 2 memory.copy)
  (func (export "fill") (param i32 i32 i32) local.get 0 local.get 1 local.get 2 memory.fill)
  (func (export "tinit") (param i32 i32 i32) local.get 0 local.get 1 local.get 2 table.init $funcs)
  (func (export "tcopy") (param i32 i32 i32) local.get 0 local.get 1 local.get 2 table.copy)
  (func (export "tdrop") elem.drop $funcs)
  (func (export "indirect") (param i32 i32) (result i32) local.get 1 local.get 0 call_indirect (type $unary))
)'''


def bulk_tests(p):
    for i in [0, 1, 2, 5, -1]:
        p.invoke("indirect", [slot(i), slot(7)])
    p.invoke("tinit", [slot(2), slot(0), slot(2)], ())
    for i in [2, 3]:
        p.invoke("indirect", [slot(i), slot(7)])
    p.invoke("tcopy", [slot(1), slot(2), slot(3)], ())
    for i in range(5):
        p.invoke("indirect", [slot(i), slot(-22)])
    p.invoke("tdrop", results=())
    p.invoke("tinit", [slot(5), slot(0), slot(0)], ())
    p.invoke("tinit", [slot(0), slot(0), slot(1)], ())
    p.invoke("init", [slot(32), slot(0), slot(16)], ())
    p.invoke("copy", [slot(35), slot(32), slot(13)], ())
    p.invoke("fill", [slot(50), slot(0x1234), slot(11)], ())
    for op, args in [("init", [65536,16,0]), ("copy", [65536,65536,0]),
                     ("fill", [65536,1,0]), ("init", [65535,0,2]),
                     ("copy", [65535,0,2]), ("fill", [65535,0,2])]:
        p.invoke(op, list(map(slot, args)), ())
    p.invoke("drop", results=())
    p.invoke("init", [slot(65536), slot(0), slot(0)], ())
    p.invoke("init", [slot(0), slot(0), slot(1)], ())
    p.memory_hash()


SIMD_WAT = r'''(module
  (memory (export "memory") 1)
  (func (export "add") (param v128 v128) (result v128) local.get 0 local.get 1 i8x16.add_sat_s)
  (func (export "shift") (param v128 i32) (result v128) local.get 0 local.get 1 i64x2.shr_u)
  (func (export "bits") (param v128) (result i32) local.get 0 i16x8.bitmask)
  (func (export "shuffle") (param v128 v128) (result v128)
    local.get 0 local.get 1 i8x16.shuffle 31 0 30 1 29 2 28 3 27 4 26 5 25 6 24 7)
  (func (export "swizzle") (param v128 v128) (result v128) local.get 0 local.get 1 i8x16.swizzle)
  (func (export "convert") (param v128) (result v128) local.get 0 i32x4.trunc_sat_f32x4_s)
  (func (export "dot") (param v128 v128) (result v128) local.get 0 local.get 1 i32x4.dot_i16x8_s)
  (func (export "store") (param i32 v128) local.get 0 local.get 1 v128.store align=1)
  (func (export "load") (param i32) (result v128) local.get 0 v128.load align=1)
  (func (export "lane") (param i32 v128) (result v128) local.get 0 local.get 1 v128.load16_lane 7)
)'''


def simd_tests(p):
    rng = random.Random(654321)
    patterns = [bytes(16), bytes([255]*16), bytes(range(16)), bytes(range(31,15,-1))]
    patterns.extend(bytes(rng.randrange(256) for _ in range(16)) for _ in range(20))
    for i, a in enumerate(patterns):
        b = patterns[(i+1) % len(patterns)]
        for op in ["add", "shuffle", "swizzle", "dot"]:
            p.invoke(op, [slot(a,"v128"),slot(b,"v128")], ("v128",))
        p.invoke("bits", [slot(a,"v128")])
        p.invoke("convert", [slot(a,"v128")], ("v128",))
        p.invoke("shift", [slot(a,"v128"),slot([0,1,63,64,-1][i%5])], ("v128",))
    for offset in [0,1,65520,65521]:
        p.invoke("store", [slot(offset),slot(patterns[2],"v128")], ())
        p.invoke("load", [slot(offset)], ("v128",))
        p.invoke("lane", [slot(offset),slot(patterns[3],"v128")], ("v128",))
    p.memory_hash()


def multivalue_payload(p):
    for a,b in [(0,0), (1,2), (-1,0x7fffffff), (-0x80000000,99)]:
        p.invoke("swap", [slot(a),slot(b)], ("i32","i32"))
        p.invoke("sum_diff", [slot(a),slot(b)], ("i32","i32"))
        p.invoke("block_pair", [slot(a)], ("i32","i32"))
        p.invoke("block_param", [slot(a),slot(b)])
    p.memory_hash()


def opus_payload(p, codec, frames=2):
    channels = 1
    init = [channels, 64000, 5, 0, 0] if codec == "minicelt" else [24000, 5, 0]
    count = 960 if codec == "minicelt" else 320
    p.invoke(codec+"_init", list(map(slot,init)))
    p.invoke(codec+"_lookahead")
    output = integer(p.invoke(codec+"_output_ptr")[0])
    rng = random.Random(101)
    for frame in range(frames):
        pointer = integer(p.invoke(codec+"_input_i16_ptr")[0])
        samples = [0]*count if frame == 0 else [rng.randrange(-14000,14000) for _ in range(count)]
        p.write(pointer, struct.pack("<"+"h"*count,*samples))
        encoded = integer(p.invoke(codec+"_encode_i16")[0],signed=True)
        if encoded < 0:
            raise AssertionError(f"{codec} encode failed: {encoded}")
        p.read(output, encoded)
        p.invoke(codec+"_final_range")
    # Check externally observable packets, not every floating-point scratch NaN bit.
    p.command("output m")


def qoa_payload(p):
    heap = integer(p.global_("heap_base"))
    p.global_("encoder_state")
    p.global_("encoder_state_size")
    channels, samples = 2, 80
    rng = random.Random(202)
    data = struct.pack("<"+"h"*(samples*channels),
                       *(rng.randrange(-32768,32768) for _ in range(samples*channels)))
    src = heap
    dst = src + ((len(data)+15)//16)*16
    decoded = dst + 4096
    p.invoke("encode_init", [slot(channels)], ())
    p.write(src,data)
    size = integer(p.invoke("encode_frame", list(map(slot,[src,channels,48000,samples,dst])))[0], signed=True)
    if size <= 0:
        raise AssertionError(f"qoa encoding failed: {size}")
    p.read(dst,size)
    p.invoke("decode_frame", list(map(slot,[dst,size,decoded])))
    for field in ["frame_channels","frame_samplerate","frame_samples"]:
        p.global_(field)
    p.read(decoded,len(data))
    p.memory_hash()


def rejection_tests(translator, work):
    # Malformed encodings and valid binaries with invalid stack/index/type usage.
    header = b"\0asm\x01\0\0\0"
    def function(code):
        return header + bytes([1,4,1,0x60,0,0,3,2,1,0,10,len(code)+3,1,len(code)+1,0]) + code
    cases = [b"", b"badmagic", header+bytes([1,1,0,1,1,0]),
             header+bytes([0,1,0xff]), header+bytes([12,1,1]),
             function(bytes([0x6a,0x0b])), function(bytes([0x20,0,0x0b])),
             function(bytes([0xd0,0x70,0x1a,0x0b])),
             function(bytes([2,0xc0,0x7f,0x0b,0x0b])),
             function(bytes([0xfc,15,0,0x0b])), function(bytes([0xfd,0x80,2,0x0b]))]
    for n,data in enumerate(cases):
        wasm = work/f"invalid-{n}.wasm"
        wasm.write_bytes(data)
        proc = subprocess.run([str(translator),str(wasm),"-o",str(work/"invalid.ll")],
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=10)
        if proc.returncode == 0:
            raise AssertionError(f"translator accepted invalid binary {n}: {data.hex()}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--quick", action="store_true", help="skip large codec payloads")
    parser.add_argument("--no-build", action="store_true")
    parser.add_argument("--only", default="", help="run names containing this substring")
    parser.add_argument("--keep", action="store_true", help="keep generated wasm/IR/libraries")
    parser.add_argument("--opt", choices=["0","2","both"], default="both")
    args = parser.parse_args()
    translator = ROOT/"target/release/wasm2llvm"
    if not args.no_build:
        run(["cargo","build","--release","--offline"],cwd=ROOT)
        run(["cargo","test","--offline"],cwd=ROOT)
    for tool in ["clang","wasm-tools","w2l-ref"]:
        if not shutil.which(tool):
            parser.error(f"required tool is not on PATH: {tool}")
    temporary = None
    if args.keep:
        work = Path(tempfile.mkdtemp(prefix="wasm2llvm-tests-"))
    else:
        temporary = tempfile.TemporaryDirectory(prefix="wasm2llvm-tests-")
        work = Path(temporary.name)
    print(f"test work directory: {work}",flush=True)
    try:
        harness = work/"harness"
        run(["clang","-std=c11","-Wall","-Wextra","-O2","-rdynamic",ROOT/"tests/harness.c","-o",harness,"-ldl"])
        cases = []
        for label,wat,action in [("scalar",SCALAR_WAT,scalar_tests),("control",CONTROL_WAT,control_tests),("bulk",BULK_WAT,bulk_tests),("simd",SIMD_WAT,simd_tests)]:
            watpath,wasm = work/(label+".wat"),work/(label+".wasm")
            watpath.write_text(wat)
            run(["wasm-tools","parse",watpath,"-o",wasm])
            cases.append((label,wasm,action))
        cases.append(("multi-value",ROOT/"payloads/multi-value/multi-value.wasm",multivalue_payload))
        if not args.quick:
            for codec in ["minicelt","minisilk"]:
                cases.append((codec,ROOT/f"payloads/{codec}/{codec}.wasm",lambda p,c=codec: opus_payload(p,c)))
            cases.append(("qoa-simd",ROOT/"payloads/qoa-simd/qoa-simd.wasm",qoa_payload))
        total_commands,passed = 0,0
        optimizations = [0,2] if args.opt == "both" else [int(args.opt)]
        for label,wasm,action in cases:
            if args.only and args.only not in label:
                continue
            for opt in optimizations:
                start = time.monotonic()
                library = build_module(wasm,work/label,opt,translator)
                with Driver(["w2l-ref"]) as reference, Driver([harness]) as native:
                    expected = reference.command(f"load m {wasm}")
                    actual = native.command(f"load m {library}")
                    if expected != "ok" or actual != "ok":
                        raise AssertionError(f"load failed: reference={expected}, native={actual}")
                    pair = Pair(reference,native)
                    pair.command("init m")
                    action(pair)
                    total_commands += pair.commands
                passed += 1
                print(f"PASS {label} -O{opt}: {pair.commands} comparisons ({time.monotonic()-start:.2f}s)",flush=True)
        if not args.only or "invalid" in args.only:
            rejection_tests(translator,work)
            print("PASS malformed/invalid/out-of-scope rejection",flush=True)
        print(f"PASS: {passed} differential runs, {total_commands} command comparisons",flush=True)
    finally:
        if temporary:
            temporary.cleanup()
        elif args.keep:
            print(f"retained generated files: {work}",flush=True)


if __name__ == "__main__":
    try:
        main()
    except (AssertionError,subprocess.TimeoutExpired) as error:
        print(f"FAIL: {error}",file=sys.stderr)
        sys.exit(1)

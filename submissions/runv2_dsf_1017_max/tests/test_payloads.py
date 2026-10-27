#!/usr/bin/env python3
"""Differential tests for the payload modules."""
import os
import random
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from drive import *

PAYLOADS = os.path.join(ROOT, "payloads")


def multi_value_scenario(seed):
    def sc(s):
        rng = random.Random(seed)
        trace = []
        trace.append(s.send("init a"))
        for _ in range(20):
            a = rng.randrange(-2**31, 2**31)
            b = rng.randrange(-2**31, 2**31)
            trace.append(s.send(f"invoke a {hname('swap')} 2 {slot_i32(a)} {slot_i32(b)}"))
            trace.append(s.send(f"invoke a {hname('sum_diff')} 2 {slot_i32(a)} {slot_i32(b)}"))
            trace.append(s.send(f"invoke a {hname('block_pair')} 2 {slot_i32(a)}"))
            trace.append(s.send(f"invoke a {hname('block_param')} 1 {slot_i32(a)} {slot_i32(b)}"))
        return trace
    return sc


def minicelt_scenario(seed):
    def sc(s):
        rng = random.Random(seed)
        trace = []
        trace.append(s.send("init a"))
        for trial in range(8):
            if trial == 0:
                ch, br, cx, vbr, dtx = 1, 64000, 11, 0, 0
            elif trial == 1:
                ch, br, cx, vbr, dtx = 2, 96000, 5, 1, 0
            elif trial == 2:
                ch, br, cx, vbr, dtx = 1, -1000, 0, 1, 1
            else:
                ch = rng.choice([1, 2])
                br = rng.choice([-1000, -1, 6000, 16000, 32000, 64000, 128000])
                cx = rng.randrange(0, 11)
                vbr = rng.randrange(0, 2)
                dtx = rng.randrange(0, 2)
            trace.append(s.send(f"invoke a {hname('minicelt_init')} 1 {slot_i32(ch)} {slot_i32(br)} {slot_i32(cx)} {slot_i32(vbr)} {slot_i32(dtx)}"))
            n = 960 * ch
            samples = b"".join(struct.pack("<f", rng.uniform(-1.0, 1.0)) for _ in range(n))
            r = s.send(f"invoke a {hname('minicelt_input_ptr')} 1")
            trace.append(r)
            ptr = slot_result_u32(r)
            trace.append(s.send(f"memwrite a {hname('memory')} {ptr} {hx(samples)}"))
            trace.append(s.send(f"invoke a {hname('minicelt_encode')} 1"))
            r = s.send(f"invoke a {hname('minicelt_output_ptr')} 1")
            trace.append(r)
            optr = slot_result_u32(r)
            # read a fixed 1276-byte window to compare the packet
            trace.append(s.send(f"memread a {hname('memory')} {optr} 1276"))
            trace.append(s.send(f"memhash a {hname('memory')}"))
            trace.append(s.send("output a"))
            trace.append(s.send(f"invoke a {hname('minicelt_final_range')} 1"))
            trace.append(s.send(f"invoke a {hname('minicelt_lookahead')} 1"))
            trace.append(s.send(f"invoke a {hname('minicelt_in_dtx')} 1"))
        return trace
    return sc


def minisilk_scenario(seed):
    def sc(s):
        rng = random.Random(seed)
        trace = []
        trace.append(s.send("init a"))
        for trial in range(8):
            if trial == 0:
                br, cx, dtx = 16000, 10, 0
            elif trial == 1:
                br, cx, dtx = -1, 0, 1
            else:
                br = rng.choice([-1000, -1, 6000, 8000, 12000, 16000, 24000, 32000])
                cx = rng.randrange(0, 11)
                dtx = rng.randrange(0, 2)
            trace.append(s.send(f"invoke a {hname('minisilk_init')} 1 {slot_i32(br)} {slot_i32(cx)} {slot_i32(dtx)}"))
            samples = b"".join(struct.pack("<f", rng.uniform(-1.0, 1.0)) for _ in range(320))
            r = s.send(f"invoke a {hname('minisilk_input_ptr')} 1")
            trace.append(r)
            ptr = slot_result_u32(r)
            trace.append(s.send(f"memwrite a {hname('memory')} {ptr} {hx(samples)}"))
            trace.append(s.send(f"invoke a {hname('minisilk_encode')} 1"))
            r = s.send(f"invoke a {hname('minisilk_output_ptr')} 1")
            trace.append(r)
            optr = slot_result_u32(r)
            trace.append(s.send(f"memread a {hname('memory')} {optr} 1276"))
            trace.append(s.send(f"memhash a {hname('memory')}"))
            trace.append(s.send(f"invoke a {hname('minisilk_final_range')} 1"))
            trace.append(s.send(f"invoke a {hname('minisilk_lookahead')} 1"))
        return trace
    return sc


def qoa_scenario(seed):
    def sc(s):
        rng = random.Random(seed)
        trace = []
        trace.append(s.send("init a"))
        for ch in [1, 2, 3, 8]:
            trace.append(s.send(f"invoke a {hname('encode_init')} 0 {slot_i32(ch)}"))
            for (rate, flen) in [(8000, 64), (44100, 256), (48000, 512)]:
                nbytes = flen * ch * 2
                samples = bytes(rng.randrange(256) for _ in range(nbytes))
                src = 65536
                dst = 65536 + ((nbytes + 15) // 16) * 16
                trace.append(s.send(f"memwrite a {hname('memory')} {src} {hx(samples)}"))
                r = s.send(f"invoke a {hname('encode_frame')} 1 {slot_i32(src)} {slot_i32(ch)} {slot_i32(rate)} {slot_i32(flen)} {slot_i32(dst)}")
                trace.append(r)
                trace.append(s.send(f"memread a {hname('memory')} {dst} 4096"))
                trace.append(s.send(f"memhash a {hname('memory')}"))
                ok = r.startswith("ok")
                if ok:
                    enc_n = slot_result_i32(r)
                    if enc_n > 0 and enc_n <= 4096:
                        fr = s.send(f"memread a {hname('memory')} {dst} {enc_n}")
                        trace.append(fr)
                        frame = bytes.fromhex(fr.split()[1])
                        trace.append(s.send(f"memwrite a {hname('memory')} 100000 {hx(frame)}"))
                        d = s.send(f"invoke a {hname('decode_frame')} 1 {slot_i32(100000)} {slot_i32(enc_n)} {slot_i32(110000)}")
                        trace.append(d)
                        trace.append(s.send(f"global a {hname('frame_channels')}"))
                        trace.append(s.send(f"global a {hname('frame_samplerate')}"))
                        trace.append(s.send(f"global a {hname('frame_samples')}"))
                        trace.append(s.send(f"memread a {hname('memory')} 110000 8192"))
                        trace.append(s.send(f"memhash a {hname('memory')}"))
        return trace
    return sc


def main():
    opt = sys.argv[1] if len(sys.argv) > 1 else "O2"
    which = sys.argv[2] if len(sys.argv) > 2 else "all"
    tests = {
        "multi-value": (os.path.join(PAYLOADS, "multi-value/multi-value.wasm"), lambda: multi_value_scenario(7)),
        "minicelt": (os.path.join(PAYLOADS, "minicelt/minicelt.wasm"), lambda: minicelt_scenario(11)),
        "minisilk": (os.path.join(PAYLOADS, "minisilk/minisilk.wasm"), lambda: minisilk_scenario(12)),
        "qoa-simd": (os.path.join(PAYLOADS, "qoa-simd/qoa-simd.wasm"), lambda: qoa_scenario(13)),
    }
    okall = True
    for name, (wasm, mk) in tests.items():
        if which not in ("all", name):
            continue
        try:
            ok = compare_scenario(wasm, mk(), opt, tag="-"+name)
        except Exception as e:
            print(f"{name}: EXCEPTION {e}")
            ok = False
        print(f"{name} [{opt}]:", "PASS" if ok else "FAIL")
        okall &= ok
    return 0 if okall else 1


if __name__ == "__main__":
    sys.exit(main())

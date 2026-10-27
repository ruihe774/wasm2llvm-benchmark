#!/usr/bin/env python3
"""Generate driver commands for minicelt / minisilk."""
import math
import struct
import sys

def hx(s):
    return s.encode().hex()

def si(v):
    return struct.pack("<i", v).hex() + "00" * 12

def slots(*vs):
    return " ".join(si(v) for v in vs)

MEM = hx("memory")
which = sys.argv[1] if len(sys.argv) > 1 else "minicelt"
cmds = ["load a MODPATH", "init a"]

if which == "minicelt":
    inp, outp = 19328, 27968
    nch = 2
    ns = 960 * nch
    cmds.append("invoke a %s 1 %s" % (hx("minicelt_init"), slots(2, 64000, 10, 1, 0)))
    cmds.append("invoke a %s 1 %s" % (hx("minicelt_set_bitrate"), slots(48000)))
    cmds.append("invoke a %s 1 %s" % (hx("minicelt_set_complexity"), slots(8)))
    cmds.append("invoke a %s 1" % hx("minicelt_lookahead"))
    samples = []
    for i in range(ns):
        ch = i % 2
        t = i // 2
        v = 0.4 * math.sin(t * (0.013 + 0.004 * ch)) + 0.1 * math.sin(t * 0.11)
        samples.append(v)
    data = struct.pack("<%df" % ns, *samples)
    cmds.append("memwrite a %s %d %s" % (MEM, inp, data.hex()))
    cmds.append("invoke a %s 1" % hx("minicelt_encode"))
    cmds.append("memread a %s %d 1276" % (MEM, outp))
    cmds.append("invoke a %s 1" % hx("minicelt_final_range"))
    cmds.append("memhash a %s" % MEM)
    # i16 input variant
    i16 = [max(-32768, min(32767, int(s * 32767))) for s in samples]
    d2 = struct.pack("<%dh" % ns, *i16)
    cmds.append("memwrite a %s %d %s" % (MEM, inp, d2.hex()))
    cmds.append("invoke a %s 1" % hx("minicelt_encode_i16"))
    cmds.append("memread a %s %d 1276" % (MEM, outp))
    cmds.append("memhash a %s" % MEM)
    # silence
    d3 = struct.pack("<%df" % ns, *([0.0] * ns))
    cmds.append("memwrite a %s %d %s" % (MEM, inp, d3.hex()))
    cmds.append("invoke a %s 1" % hx("minicelt_encode"))
    cmds.append("memhash a %s" % MEM)
    cmds.append("invoke a %s 1" % hx("minicelt_in_dtx"))
else:
    inp, outp = 0x10550, 0xdee0
    ns = 320
    cmds.append("invoke a %s 1 %s" % (hx("minisilk_init"), slots(24000, 10, 0)))
    samples = [0.5 * math.sin(i * 0.07) + 0.2 * math.sin(i * 0.31) for i in range(ns)]
    d = struct.pack("<%df" % ns, *samples)
    cmds.append("memwrite a %s %d %s" % (MEM, inp, d.hex()))
    cmds.append("invoke a %s 1" % hx("minisilk_encode"))
    cmds.append("memread a %s %d 1276" % (MEM, outp))
    cmds.append("invoke a %s 1" % hx("minisilk_final_range"))
    cmds.append("memhash a %s" % MEM)
    i16 = [max(-32768, min(32767, int(s * 32767))) for s in samples]
    d2 = struct.pack("<%dh" % ns, *i16)
    cmds.append("memwrite a %s %d %s" % (MEM, inp, d2.hex()))
    cmds.append("invoke a %s 1" % hx("minisilk_encode_i16"))
    cmds.append("memhash a %s" % MEM)
    cmds.append("invoke a %s 1" % hx("minisilk_in_dtx"))

cmds.append("quit")
print("\n".join(cmds))

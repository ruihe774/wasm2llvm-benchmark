#!/usr/bin/env python3
"""Generate qoa-simd driver commands."""
import math
import struct
import sys

def slot_i32(v):
    return struct.pack("<i", v).hex() + "00" * 12

def slots(*vs):
    return " ".join(slot_i32(v) for v in vs)

channels = 2
frame_len = 5120
samples = []
for i in range(frame_len * channels):
    v = int(20000 * math.sin(i * 0.01) + 3000 * math.sin(i * 0.37))
    samples.append(max(-32768, min(32767, v)))
data = struct.pack("<%dh" % len(samples), *samples).hex()

heap = 256
src = heap
dst = heap + ((frame_len * channels * 2 + 15) // 16) * 16
dec = 40000

cmds = []
cmds.append("load a MODPATH")
cmds.append("init a")
cmds.append("memwrite a 6d656d6f7279 %d %s" % (src, data))
cmds.append("invoke a %s 0 %s" % ("656e636f64655f696e6974", slots(channels)))
r = cmds.append
r("invoke a %s 1 %s" % ("656e636f64655f6672616d65", slots(src, channels, 48000, frame_len, dst)))
r("memhash a 6d656d6f7279")
r("memread a 6d656d6f7279 %d 1276" % dst)
r("invoke a %s 1 %s" % ("6465636f64655f6672616d65", slots(dst, 1276, dec)))
r("global a 6672616d655f6368616e6e656c73")
r("global a 6672616d655f73616d706c6572617465")
r("global a 6672616d655f73616d706c6573")
r("memread a 6d656d6f7279 %d 8192" % dec)
r("memhash a 6d656d6f7279")
r("quit")
print("\n".join(cmds))

#!/usr/bin/env python3
"""Numeric and SIMD edge case differential tests."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from drive import *

WAT = os.path.join(ROOT, "tests/wat/numeric.wat")
WASM = os.path.join(os.environ["TMPDIR"], "w2l-fuzz", "numeric.wasm")

F32 = [0x00000000, 0x80000000, 0x3f800000, 0xbf800000, 0x3f000000, 0xbf000000,
       0x3fc00000, 0xbfc00000, 0x40000000, 0xc0490fdb, 0x4b000000, 0x7f7fffff,
       0x00800000, 0x00000001, 0x4f000000, 0xcf000000, 0x4effffff, 0xcf000001,
       0x7f800000, 0xff800000, 0x7fc00000, 0x7fc00001, 0x7f800001, 0xff800001]
F64 = [0x0000000000000000, 0x8000000000000000, 0x3ff0000000000000,
       0xbff0000000000000, 0x3fe0000000000000, 0x4000000000000000,
       0x7fefffffffffffff, 0x0010000000000000, 0x0000000000000001,
       0x43e0000000000000, 0xc3e0000000000000, 0x43dfffffffffffff,
       0xc3e0000000000001, 0x41e0000000000000, 0xc1e0000000000000,
       0xc1e0000000200000, 0x7ff0000000000000, 0xfff0000000000000,
       0x7ff8000000000000, 0x7ff8000000000001, 0x7ff0000000000001,
       0xfff0000000000001, 0x4330000000000000, 0xc330000000000000]


def build():
    os.makedirs(os.path.dirname(WASM), exist_ok=True)
    subprocess.run([os.path.join(TOOLS, "wat2wasm"), WAT, "-o", WASM], check=True)


def sc(seed):
    def f(s):
        t = []

        def call(name, nres, *args):
            t.append(s.send(f"invoke a {hname(name)} {nres} " + " ".join(args)))

        t.append(s.send("init a"))
        t.append(s.send(f"memwrite a {hname('memory')} 0 78563412efcdab001122334455667788"))
        # integer shifts and rotates
        for op in ["i32_shl", "i32_shr_s", "i32_shr_u", "i32_rotl", "i32_rotr"]:
            for v in [0x12345678, 0x80000000, -1, 1]:
                for c in [0, 1, 7, 15, 31, 32, 33, 63, 64, 65, -1, 0x7fffffff]:
                    call(op, 1, slot_i32(v), slot_i32(c))
        for op in ["i64_shl", "i64_shr_s", "i64_shr_u", "i64_rotl", "i64_rotr"]:
            for v in [0x123456789abcdef0, -1, 1]:
                for c in [0, 1, 31, 32, 33, 63, 64, 65, 127, -1]:
                    call(op, 1, slot_i64(v), slot_i64(c))
        for op in ["i32_clz", "i32_ctz", "i32_popcnt"]:
            for v in [0, 1, 2, 0x80000000, -1, 0x00f00000, 0xffff0000]:
                call(op, 1, slot_i32(v))
        for op in ["i64_clz", "i64_ctz", "i64_popcnt"]:
            for v in [0, 1, 2, 0x8000000000000000, -1, 0x0000f00000000000]:
                call(op, 1, slot_i64(v))
        for op in ["i32_ext8", "i32_ext16"]:
            for v in [0x7f, 0x80, 0xff, 0x1ff, 0x8000, 0xffff, -1, 0x12345678]:
                call(op, 1, slot_i32(v))
        for op in ["i64_ext8", "i64_ext16", "i64_ext32"]:
            for v in [0x7f, 0x80, 0x8000, 0x80000000, -1, 0x123456789abcdef0]:
                call(op, 1, slot_i64(v))
        for v1 in [0x7fffffff, 0x80000000, -1, 12345]:
            for v2 in [1, 2, -1, 0x10000]:
                call("i32_add", 1, slot_i32(v1), slot_i32(v2))
                call("i32_mul", 1, slot_i32(v1), slot_i32(v2))
                call("i64_mul", 1, slot_i64(v1), slot_i64(v2))

        # conversions
        for bits in F32:
            call("i32_trunc_f32_s", 1, slot_f32b(bits))
            call("i32_trunc_f32_u", 1, slot_f32b(bits))
            call("i64_trunc_f32_s", 1, slot_f32b(bits))
            call("i64_trunc_f32_u", 1, slot_f32b(bits))
            call("i32_sat_f32_s", 1, slot_f32b(bits))
            call("i32_sat_f32_u", 1, slot_f32b(bits))
            call("i64_sat_f32_s", 1, slot_f32b(bits))
            call("i64_sat_f32_u", 1, slot_f32b(bits))
        for bits in F64:
            call("i32_trunc_f64_s", 1, slot_f64b(bits))
            call("i32_trunc_f64_u", 1, slot_f64b(bits))
            call("i64_trunc_f64_s", 1, slot_f64b(bits))
            call("i64_trunc_f64_u", 1, slot_f64b(bits))
            call("i32_sat_f64_s", 1, slot_f64b(bits))
            call("i32_sat_f64_u", 1, slot_f64b(bits))
            call("i64_sat_f64_s", 1, slot_f64b(bits))
            call("i64_sat_f64_u", 1, slot_f64b(bits))

        # float arithmetic bits (finite inputs only)
        fin32 = [b for b in F32 if (b & 0x7f800000) != 0x7f800000]
        fin64 = [b for b in F64 if (b & 0x7ff0000000000000) != 0x7ff0000000000000]
        for a in fin32:
            for b in fin32:
                call("f32_add", 1, slot_f32b(a), slot_f32b(b))
                call("f32_mul", 1, slot_f32b(a), slot_f32b(b))
                call("f32_min", 1, slot_f32b(a), slot_f32b(b))
                call("f32_max", 1, slot_f32b(a), slot_f32b(b))
                call("f32_copysign", 1, slot_f32b(a), slot_f32b(b))
                call("f32_eq", 1, slot_f32b(a), slot_f32b(b))
                call("f32_ne", 1, slot_f32b(a), slot_f32b(b))
                call("f32_lt", 1, slot_f32b(a), slot_f32b(b))
                call("fmin_sel", 1, slot_f32b(a), slot_f32b(b))
        for a in fin32:
            call("f32_abs", 1, slot_f32b(a))
            call("f32_neg", 1, slot_f32b(a))
            call("f32_ceil", 1, slot_f32b(a))
            call("f32_floor", 1, slot_f32b(a))
            call("f32_trunc", 1, slot_f32b(a))
            call("f32_nearest", 1, slot_f32b(a))
            if (a & 0x80000000) == 0:
                call("f32_sqrt", 1, slot_f32b(a))
        for a in fin64:
            for b in fin64:
                call("f64_add", 1, slot_f64b(a), slot_f64b(b))
                call("f64_min", 1, slot_f64b(a), slot_f64b(b))
                call("f64_max", 1, slot_f64b(a), slot_f64b(b))
                call("f64_le", 1, slot_f64b(a), slot_f64b(b))
        for a in fin64:
            call("f64_sqrt", 1, slot_f64b(a)) if (a & 0x8000000000000000) == 0 else None
            call("f64_nearest", 1, slot_f64b(a))
            call("f32_demote", 1, slot_f64b(a))
        for a in fin32:
            call("f64_promote", 1, slot_f32b(a))
        for v in [0, 1, -1, 0x7fffffff, 0x80000000, 0x01000001, 0xffffff00]:
            call("f32_convert_s", 1, slot_i32(v))
            call("f32_convert_u", 1, slot_i32(v))
        for v in [0, 1, -1, 0x7fffffffffffffff, 0x8000000000000000, 0x20000000000001, 0xffffffffffffffff]:
            call("f32_convert_l_s", 1, slot_i64(v))
            call("f32_convert_l_u", 1, slot_i64(v))
            call("f64_convert_l_s", 1, slot_i64(v))
            call("f64_convert_l_u", 1, slot_i64(v))

        # NaN predicates
        nan32 = [0x7fc00000, 0x7fc00001, 0x7f800001, 0xff800001, 0xffc00000]
        nan64 = [0x7ff8000000000000, 0x7ff8000000000001, 0x7ff0000000000001, 0xfff0000000000001]
        for a in nan32:
            for b in nan32 + [0x3f800000, 0x00000000]:
                call("min_naninfo", 1, slot_f32b(a), slot_f32b(b))
                call("max_naninfo", 1, slot_f32b(a), slot_f32b(b))
                call("add_naninfo", 1, slot_f32b(a), slot_f32b(b))
                call("v_f32min_naninfo", 1, slot_f32b(a), slot_f32b(b))
                call("v_pmin_bits", 1, slot_f32b(a), slot_f32b(b))
                call("v_pmax_bits", 1, slot_f32b(a), slot_f32b(b))
            call("sqrt_naninfo", 1, slot_f32b(a))
            call("promote_naninfo", 1, slot_f32b(a))
        for a in nan64:
            call("demote_naninfo", 1, slot_f64b(a))
        for a in nan64:
            for b in nan64 + [0x3ff0000000000000]:
                call("f64_min_naninfo", 1, slot_f64b(a), slot_f64b(b))
                call("f64_max_naninfo", 1, slot_f64b(a), slot_f64b(b))
        for a in nan32:
            for b in nan32 + [0x3f800000]:
                call("v_f32max_naninfo", 1, slot_f32b(a), slot_f32b(b))
                call("v_f32add_naninfo", 1, slot_f32b(a), slot_f32b(b))
        for a in nan32:
            call("f32_abs_nan", 1, slot_f32b(a))
            call("f32_neg_nan", 1, slot_f32b(a))
            for b in [0x3f800000, 0xbf800000, 0x00000000, 0x80000000]:
                call("f32_copysign_nan", 1, slot_f32b(a), slot_f32b(b))

        # SIMD edge cases
        for v in [0x7fffffff, 0x80000000, 0x0000ffff, 0xffff0000, 0, -1, 32767, 32768, -32768, -32769, 0x12345678]:
            call("v_narrow_s", 1, slot_i32(v))
            call("v_narrow_u", 1, slot_i32(v))
            call("v_sat_trunc", 1, slot_i32(v))
            call("v_sat_trunc_u", 1, slot_i32(v))
            call("v_convert", 1, slot_i32(v))
        for a in [32767, 32768, -32768, -32769, 0, 1, -1, 16384, -16384, 12345]:
            for b in [32767, -32768, 1, -1, 0, 16384, -16384]:
                call("v_q15", 1, slot_i32(a), slot_i32(b))
                call("v_dot", 1, slot_i32(a), slot_i32(b))
                call("v_extmul", 1, slot_i32(a), slot_i32(b))
        for a in [0, 1, 127, 128, 255, 200]:
            for b in [0, 1, 127, 128, 255, 100]:
                call("v_avgr", 1, slot_i32(a), slot_i32(b))
        for v in [0, 1, -1, 0x80, 0x100, 0x7fffffff]:
            call("v_bitmask", 1, slot_i32(v))
            call("v_alltrue", 1, slot_i32(v))
            call("v_extadd", 1, slot_i32(v))
            call("v_shuffle", 1, slot_i32(v))
        for a in [0, 1, 0x55, 0xaa, 0xff]:
            for idx in [0, 1, 5, 15, 16, 17, 100, 255]:
                call("v_swizzle", 1, slot_i32(a), slot_i32(idx))
            for c in [0, 1, 7, 8, 9, 15, 16, 255]:
                call("v_shl", 1, slot_i32(a), slot_i32(c))
                call("v_shr_s", 1, slot_i32(a), slot_i32(c))
        for v in [0, 1, -1, 0x0102030405060708, 0x8000000000000000]:
            call("v_anytrue", 1, slot_i64(v))
        call("v_load_lane", 1, slot_i32(0))

        t.append(s.send(f"memhash a {hname('memory')}"))
        return t
    return f


def main():
    opt = sys.argv[1] if len(sys.argv) > 1 else "O2"
    build()
    ok = compare_scenario(WASM, sc(1), opt, tag="-num")
    print(f"numeric [{opt}]:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())

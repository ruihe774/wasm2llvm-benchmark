#!/usr/bin/env python3
"""Check that the translator accepts valid modules and rejects invalid /
out-of-scope ones, using w2l-ref as the oracle."""
import os
import struct
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
W2L = os.path.join(ROOT, "target", "release", "wasm2llvm")
TMP = os.environ.get("TMPDIR", tempfile.gettempdir())

INVALID_WAT = {
    "type_mismatch": """(module (func (result i32) i64.const 0))""",
    "unknown_local": """(module (func local.get 5 drop))""",
    "unknown_label": """(module (func br 2))""",
    "immutable_global": """(module (global i32 (i32.const 0))
        (func (global.set 0 (i32.const 1))))""",
    "dup_export": """(module (func $f) (export "x" (func $f)) (export "x" (func $f)))""",
    "start_bad_sig": """(module (func $f (param i32)) (start $f))""",
    "memory_too_big": """(module (memory 65537))""",
    "memory_min_gt_max": """(module (memory 2 1))""",
    "bad_align": """(module (memory 1) (func (param i32) (result i32)
        i32.load align=8 (local.get 0)))""",
    "lane_oob": """(module (func (param v128) (result i32)
        i8x16.extract_lane_s 16 (local.get 0)))""",
    "shuffle_oob": """(module (func (param v128 v128) (result v128)
        i8x16.shuffle 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 32 (local.get 0) (local.get 1)))""",
    "if_no_else_result": """(module (func (result i32) i32.const 1
        (if (result i32) (then i32.const 2))))""",
    "brtable_arity": """(module (func (param i32) (result i32)
        block (result i32) block (result i32 i32)
        i32.const 1 i32.const 2 local.get 0 br_table 0 1
        end drop end))""",
    "elem_bad_func": """(module (table 1 funcref) (elem (i32.const 0) 5))""",
    "data_bad_offset": """(module (memory 1) (data (i64.const 0) "x"))""",
    "global_init_type": """(module (global i32 (i64.const 0)))""",
    "select_mismatch": """(module (func (param i32 i64 i32) (result i32)
        local.get 0 local.get 1 local.get 2 select))""",
    "unreachable_poly": """(module (func unreachable i32.const 0 i64.add drop))""",
    "funcref_value": """(module (func (param funcref)))""",
    "table_get": """(module (table 1 funcref) (func (result funcref)
        i32.const 0 table.get 0))""",
    "ref_null": """(module (func (result funcref) ref.null func))""",
    "ref_func": """(module (func $f) (func (result funcref) ref.func $f))""",
    "memory64": """(module (memory i64 1))""",
    "shared_memory": """(module (memory 1 1 shared))""",
    "tail_call": """(module (func $f (result i32) return_call $f))""",
    "exception_tag": """(module (tag) (func throw 0))""",
    "extended_const": """(module (global i32 (i32.add (i32.const 1) (i32.const 2))))""",
    "relaxed_simd": """(module (func (param v128 v128) (result v128)
        i8x16.relaxed_swizzle (local.get 0) (local.get 1)))""",
    "multiple_tables": """(module (table 1 funcref) (table 1 funcref))""",
    "typed_select": """(module (func (param externref externref i32) (result externref)
        local.get 0 local.get 1 local.get 2 select (result externref)))""",
    "memory_init_no_datacount": None,  # binary-level
    "global_get_constexpr": """(module (global i32 (i32.const 0))
        (global i32 (global.get 0)))""",
}

VALID_WAT = {
    "simple": """(module (func (export "f") (result i32) i32.const 1))""",
    "multi_value": """(module (func (export "f") (result i32 i64) i32.const 1 i64.const 2))""",
    "signext": """(module (func (export "f") (param i32) (result i32) (i32.extend8_s (local.get 0))))""",
    "trunc_sat": """(module (func (export "f") (param f32) (result i32) (i32.trunc_sat_f32_s (local.get 0))))""",
    "bulk": """(module (memory 1) (data "abc")
        (func (export "f") (memory.init 0 (i32.const 0) (i32.const 0) (i32.const 1))))""",
    "simd": """(module (func (export "f") (param v128) (result v128) (i32x4.abs (local.get 0))))""",
    "passive_elem": """(module (table 1 funcref) (elem func $f) (func $f)
        (func (export "g") (table.init 0 (i32.const 0) (i32.const 0) (i32.const 1))))""",
    "declarative_elem": """(module (table 1 funcref) (elem declare func $f) (func $f))""",
    "v128_global": """(module (global v128 (v128.const i32x4 1 2 3 4)))""",
    "untyped_select_v128": """(module (func (export "f") (param v128 v128 i32) (result v128)
        local.get 0 local.get 1 local.get 2 select))""",
    "if_no_else_params": """(module (func (export "f") (param i32) (result i32)
        local.get 0 local.get 0 (if (param i32) (result i32) (then))))""",
}


def binary_cases():
    cases = {}
    cases["empty"] = b""
    cases["bad_magic"] = b"\0asx\x01\0\0\0"
    cases["bad_version"] = b"\0asm\x02\0\0\0"
    cases["truncated_section"] = b"\0asm\x01\0\0\0\x01\x05\x01"
    cases["section_size_mismatch"] = b"\0asm\x01\0\0\0\x01\x02\x01\x60"
    cases["unknown_section"] = b"\0asm\x01\0\0\0\x20\x01\x00"
    cases["duplicate_type_section"] = (
        b"\0asm\x01\0\0\0" + b"\x01\x04\x01\x60\x00\x00" + b"\x01\x04\x01\x60\x00\x00"
    )
    cases["out_of_order"] = (
        b"\0asm\x01\0\0\0" + b"\x02\x01\x00" + b"\x01\x04\x01\x60\x00\x00"
    )
    cases["bad_valtype"] = b"\0asm\x01\0\0\0\x01\x05\x01\x60\x01\x7a\x00"
    # function section with one func but no code section
    cases["func_no_code"] = b"\0asm\x01\0\0\0" + b"\x01\x04\x01\x60\x00\x00" + b"\x03\x02\x01\x00"
    # code section with one body but no function section
    cases["code_no_func"] = b"\0asm\x01\0\0\0" + b"\x0a\x04\x01\x02\x00\x0b"
    # data count mismatch
    cases["datacount_mismatch"] = (
        b"\0asm\x01\0\0\0"
        + b"\x0c\x01\x02"  # datacount 2
        + b"\x0b\x04\x01\x01\x01a"  # one data segment
    )
    # memory.init without data count section
    cases["meminit_no_datacount"] = (
        b"\0asm\x01\0\0\0"
        + b"\x01\x04\x01\x60\x00\x00"
        + b"\x03\x02\x01\x00"
        + b"\x05\x03\x01\x00\x01"
        + b"\x0a\x0e\x01\x0c\x00\x41\x00\x41\x00\x41\x00\xfc\x08\x00\x00\x0b"
    )
    # function body size mismatch (body claims 3 bytes but has more)
    cases["body_size_mismatch"] = (
        b"\0asm\x01\0\0\0"
        + b"\x01\x04\x01\x60\x00\x00"
        + b"\x03\x02\x01\x00"
        + b"\x0a\x05\x01\x03\x00\x0b\x0b"
    )
    # invalid opcode 0x06
    cases["bad_opcode"] = (
        b"\0asm\x01\0\0\0"
        + b"\x01\x04\x01\x60\x00\x00"
        + b"\x03\x02\x01\x00"
        + b"\x0a\x05\x01\x03\x00\x06\x0b"
    )
    # invalid utf8 export name
    cases["bad_utf8"] = (
        b"\0asm\x01\0\0\0"
        + b"\x01\x04\x01\x60\x00\x00"
        + b"\x03\x02\x01\x00"
        + b"\x07\x05\x01\x01\xff\x00\x00"
        + b"\x0a\x04\x01\x02\x00\x0b"
    )
    # overlong u32 LEB (6 bytes) in section size
    cases["overlong_leb"] = b"\0asm\x01\0\0\0\x01\x84\x80\x80\x80\x80\x00"
    return cases


def check(wasm_path):
    r = subprocess.run([W2L, wasm_path, "-o", os.path.join(TMP, "v.ll")],
                       capture_output=True, text=True)
    ours_ok = r.returncode == 0
    rr = subprocess.run("w2l-ref", input="load a %s\nquit\n" % wasm_path,
                        capture_output=True, text=True)
    ref_line = rr.stdout.strip().splitlines()[0] if rr.stdout.strip() else "<none>"
    ref_ok = ref_line == "ok"
    return ours_ok, ref_ok, r.stderr.strip()[:160], ref_line[:120]


def main():
    bad = 0
    workdir = tempfile.mkdtemp(prefix="w2lvalid")
    for name, wat in list(INVALID_WAT.items()) + list(VALID_WAT.items()):
        if wat is None:
            continue
        expect_ok = name in VALID_WAT
        wp = os.path.join(workdir, name + ".wat")
        bp = os.path.join(workdir, name + ".wasm")
        open(wp, "w").write(wat + "\n")
        r = subprocess.run(["wasm-tools", "parse", wp, "-o", bp], capture_output=True, text=True)
        if r.returncode != 0:
            print("%-28s PARSE-FAIL %s" % (name, r.stderr.strip()[:100]))
            continue
        ours_ok, ref_ok, err, ref_line = check(bp)
        status = "ok"
        if ours_ok != ref_ok:
            status = "DISAGREE"
            bad += 1
        elif expect_ok and not ours_ok and ref_ok:
            status = "UNEXPECTED-REJECT"
            bad += 1
        elif not expect_ok and ours_ok and ref_ok:
            status = "UNEXPECTED-ACCEPT"
            bad += 1
        print("%-28s ours=%-5s ref=%-5s %s %s" % (
            name, "ok" if ours_ok else "err", "ok" if ref_ok else "err", status,
            ("" if status == "ok" else (err + " | " + ref_line))))
    for name, blob in binary_cases().items():
        bp = os.path.join(workdir, name + ".wasm")
        open(bp, "wb").write(blob)
        ours_ok, ref_ok, err, ref_line = check(bp)
        status = "ok"
        if ours_ok or ref_ok:
            status = "UNEXPECTED"
            bad += 1
        print("%-28s ours=%-5s ref=%-5s %s %s" % (
            name, "ok" if ours_ok else "err", "ok" if ref_ok else "err", status,
            ("" if status == "ok" else (err + " | " + ref_line))))
    print("bad:", bad)
    return 0 if bad == 0 else 1


if __name__ == "__main__":
    sys.exit(main())

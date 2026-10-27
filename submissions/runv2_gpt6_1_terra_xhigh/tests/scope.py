#!/usr/bin/env python3
"""Probe reference engine feature switches with minimal independent modules."""
import pathlib, subprocess, tempfile, os
from run import ROOT
CASES = {
'ref-null': '(module (func (result funcref) ref.null func))',
'ref-is-null': '(module (table 1 funcref) (func (result i32) ref.null func ref.is_null))',
'externref': '(module (func (param externref)))',
'table-get': '(module (table 1 funcref) (func (result funcref) i32.const 0 table.get))',
'table-size': '(module (table 1 funcref) (func (result i32) table.size))',
'typed-select': '(module (func (param i32 i32 i32) (result i32) local.get 0 local.get 1 local.get 2 select (result i32)))',
'multiple-tables': '(module (table 1 funcref) (table 1 funcref))',
'bulk-memory': '(module (memory 1) (func i32.const 0 i32.const 0 i32.const 0 memory.fill))',
'bulk-table': '(module (table 1 funcref) (elem $e func) (func i32.const 0 i32.const 0 i32.const 0 table.init $e))',
'multi-value': '(module (func (result i32 i32) i32.const 0 i32.const 0))',
'sign-ext': '(module (func (result i32) i32.const 1 i32.extend8_s))',
'saturation': '(module (func (result i32) f32.const 1 i32.trunc_sat_f32_s))',
'simd': '(module (func (result v128) v128.const i32x4 0 0 0 0))',
'extended-const': '(module (global i32 (i32.add (i32.const 1) (i32.const 2))))',
'tail-call': '(module (func $f) (func return_call $f))',
'relaxed-simd': '(module (func (result v128) v128.const i32x4 0 0 0 0 v128.const i32x4 0 0 0 0 i8x16.relaxed_swizzle))',
}
with tempfile.TemporaryDirectory(dir=os.environ.get('TMPDIR')) as d:
    for name,wat in CASES.items():
        w=pathlib.Path(d)/(name+'.wat');b=w.with_suffix('.wasm');w.write_text(wat)
        p=subprocess.run(['wasm-tools','parse',str(w),'-o',str(b)],capture_output=True,text=True)
        if p.returncode:print(name,'parse-error',p.stderr);continue
        p=subprocess.run(['w2l-ref'],input=f'load x {b}\nquit\n',capture_output=True,text=True)
        r=p.stdout.splitlines()[0]
        trans=subprocess.run([str(ROOT/'target/release/wasm2llvm'),str(b),'-o',str(w.with_suffix('.ll'))],capture_output=True,text=True)
        print(name,'reference:',r,'translator:', 'ok' if trans.returncode==0 else trans.stderr.strip(),flush=True)

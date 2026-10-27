#!/usr/bin/env python3
"""Check malformed binaries, stack typing, and proposal exclusions."""
import pathlib, random, subprocess, tempfile, os
from run import ROOT
INVALID_WAT=[
'(module (func (result i32)))',
'(module (func i64.const 0 i32.eqz drop))',
'(module (func (param i32) local.get 1 drop))',
'(module (func block br 2 end))',
'(module (func if end))',
'(module (func block (result i32) end drop))',
'(module (func i32.const 1 i64.const 2 i32.const 1 select drop))',
'(module (func i32.const 1 drop drop))',
'(module (global i32 (i64.const 0)))',
'(module (memory 1) (func i32.const 0 i64.load align=16 drop))',
'(module (func (result i32 i64) i64.const 0 i32.const 1))',
'(module (func $f (param i32)) (start $f))',
'(module (func (param i32) i32.const 0 i32.const 0 if (result i64) i32.const 1 else i64.const 2 end drop))',
'(module (func (result i32) block (result i32) i64.const 1 br 0 end))',
'(module (func (param i32) (result i32) local.get 0 if (result i32) i32.const 1 end))',
'(module (func (param i32) (result i32) local.get 0 local.tee 0 local.set 0))',
'(module (global i32 (i32.const 0)) (func i32.const 1 global.set 0))',
'(module (func (result i32) i32.const 1 i32.const 2))',
'(module (func (result i32) unreachable i64.const 0))',
'(module (func (result i32) block (result i32) i32.const 0 br 0 i64.const 0 end))',
]
OUTSIDE_WAT=[
'(module (func block (result funcref) unreachable end drop))',
'(module (func (result funcref) ref.null func))',
'(module (table 1 funcref) (func (result i32) table.size))',
'(module (func (param i32 i32 i32) (result i32) local.get 0 local.get 1 local.get 2 select (result i32)))',
'(module (table 1 funcref) (table 1 funcref))',
'(module (global i32 (i32.add (i32.const 1) (i32.const 2))))',
'(module (func $f) (func return_call $f))',
'(module (func (result v128) v128.const i32x4 0 0 0 0 v128.const i32x4 0 0 0 0 i8x16.relaxed_swizzle))',
]

def run():
    translator=ROOT/'target/release/wasm2llvm';checked=0
    with tempfile.TemporaryDirectory(dir=os.environ.get('TMPDIR')) as tmp:
        tmp=pathlib.Path(tmp);wat=tmp/'input.wat';wasm=tmp/'input.wasm';ll=tmp/'out.ll'
        for i,text in enumerate(INVALID_WAT+OUTSIDE_WAT):
            wat.write_text(text)
            parsed=subprocess.run(['wasm-tools','parse',str(wat),'-o',str(wasm)],capture_output=True,text=True)
            if parsed.returncode:
                print('parser rejected fixture',i,parsed.stderr.strip());continue
            if i<len(INVALID_WAT):
                check=subprocess.run(['wasm-tools','validate',str(wasm)],capture_output=True,text=True)
                assert check.returncode, f'fixture {i} is not invalid'
            else:
                check=subprocess.run(['w2l-ref'],input=f'load x {wasm}\nquit\n',capture_output=True,text=True)
                assert check.stdout.startswith('error'), f'fixture {i} is in scope'
            actual=subprocess.run([str(translator),str(wasm),'-o',str(ll)],capture_output=True,text=True)
            assert actual.returncode, f'accepted fixture {i}: {text}'
            checked+=1
        original=(ROOT/'payloads/multi-value/multi-value.wasm').read_bytes()
        variants=[original[:i] for i in range(len(original))]
        rng=random.Random(12345)
        for _ in range(200):
            mutated=bytearray(original)
            for _ in range(rng.randrange(1,4)):mutated[rng.randrange(len(mutated))]^=1<<rng.randrange(8)
            variants.append(mutated)
        for i,b in enumerate(variants):
            wasm.write_bytes(b)
            valid=subprocess.run(['wasm-tools','validate',str(wasm)],capture_output=True)
            if valid.returncode==0:continue
            actual=subprocess.run([str(translator),str(wasm),'-o',str(ll)],capture_output=True)
            if actual.returncode==0:
                print('accepted invalid mutated binary',i, b.hex());return False
            checked+=1
    print('validation passed',checked,'invalid or out-of-scope cases')
    return True
if __name__=='__main__':raise SystemExit(not run())

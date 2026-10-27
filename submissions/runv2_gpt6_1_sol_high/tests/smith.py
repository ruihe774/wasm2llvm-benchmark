#!/usr/bin/env python3
"""Deterministic wasm-smith differential execution, restricted to the supported scope."""
import argparse
from pathlib import Path
import random
import struct
import subprocess
import sys
ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'tests/out/smith'
OUT.mkdir(parents=True,exist_ok=True)
SHAPE='''(module
(memory (export "memory") 1 4)
(global (export "g0") (mut i32) (i32.const 0))
(global (export "g1") (mut i64) (i64.const 0))
(global (export "g2") (mut v128) (v128.const i32x4 0 0 0 0))
(func (export "f0") (param i32) (result i32) local.get 0)
(func (export "f1") (param i32 i32) (result i64) i64.const 0)
(func (export "f2") (param f32 f32) (result f32) local.get 0)
(func (export "f3") (param v128 v128 i32) (result v128) local.get 0)
(func (export "f4") (param i32 i64) (result i32 i64) local.get 0 local.get 1)
(func (export "f5") (param f64 f64) (result f64) local.get 0)
)'''
FLAGS=['--wide-arithmetic-enabled','false','--custom-page-sizes-enabled','false','--shared-everything-threads-enabled','false','--max-imports','0','--gc-enabled','false','--custom-descriptors-enabled','false','--compact-imports-enabled','false','--exceptions-enabled','false','--memory64-enabled','false','--reference-types-enabled','false','--relaxed-simd-enabled','false','--tail-call-enabled','false','--threads-enabled','false','--extended-const-enabled','false','--max-memories','0','--max-tables','1','--max-memory32-bytes','262144','--max-table-elements','32','--max-funcs','10','--max-types','10','--max-globals','8','--max-instructions','120','--max-data-segments','4','--max-element-segments','4','--max-elements','12','--canonicalize-nans','true','--ensure-termination','--fuel','300']
SIGS=[(['i32'],['i32']),(['i32','i32'],['i64']),(['f32','f32'],['f32']),(['v128','v128','i32'],['v128']),(['i32','i64'],['i32','i64']),(['f64','f64'],['f64'])]
SIZE={'i32':4,'i64':8,'f32':4,'f64':8,'v128':16}
def run(argv,**kw):return subprocess.run([str(x) for x in argv],capture_output=True,**kw)
def main():
    ap=argparse.ArgumentParser();ap.add_argument('--count',type=int,default=100);ap.add_argument('--start',type=int,default=0);a=ap.parse_args()
    (OUT/'shape.wat').write_text(SHAPE)
    p=run(['wasm-tools','parse',OUT/'shape.wat','-o',OUT/'shape.wasm'])
    if p.returncode:raise RuntimeError(p.stderr)
    for seed in range(a.start,a.start+a.count):
        rng=random.Random(seed+527555)
        wasm=OUT/('m%04d.wasm'%seed);ll=wasm.with_suffix('.ll')
        p=run(['wasm-tools','smith','--exports',OUT/'shape.wasm',*FLAGS,'-o',wasm],input=rng.randbytes(20000))
        if p.returncode:raise RuntimeError(p.stderr)
        scope=run(['w2l-ref'],input='load m '+str(wasm)+'\nquit\n',text=True,timeout=30)
        if scope.stdout.strip()!='ok':
            print('smith',seed,'skipped out of scope:',scope.stdout.strip(),flush=True)
            continue
        p=run([ROOT/'target/release/wasm2llvm',wasm,'-o',ll])
        if p.returncode:raise RuntimeError('translate seed %d: %s'%(seed,p.stderr))
        cmds=['load m '+str(wasm),'init m'];meta=[None,None]
        for r in range(4):
            for j,(ps,rs) in enumerate(SIGS):
                args=[]
                for t in ps:
                    if t=='i32':b=struct.pack('<I',rng.choice([0,1,2,16,0xffffffff,rng.getrandbits(32)]))
                    elif t=='i64':b=struct.pack('<Q',rng.getrandbits(64))
                    else:b=rng.randbytes(SIZE[t])
                    args.append(b.ljust(16,b'\0').hex())
                cmds.append('invoke m %s %d %s'%(('f%d'%j).encode().hex(),len(rs),' '.join(args)));meta.append(rs)
            for j,t in enumerate(['i32','i64','v128']):cmds.append('global m '+('g%d'%j).encode().hex());meta.append([t])
        # Hashes can be large; compare the complete memory state after the calls.
        cmds.append('memhash m '+b'memory'.hex());meta.append(None)
        ref=run(['w2l-ref'],input='\n'.join(cmds)+'\nquit\n',text=True,timeout=30)
        lines=ref.stdout.splitlines()
        if len(lines)<2 or lines[0]!='ok':raise RuntimeError('scope/ref load seed %d: %s'%(seed,ref.stdout))
        if lines[1]!='ok':
            # Both engines must agree on instantiation traps, but there is nothing to call.
            cmds=cmds[:2];meta=meta[:2];lines=lines[:2]
        for opt in ('O0','O2'):
            so=OUT/('m%04d.%s.so'%(seed,opt));p=run(['clang','-'+opt,'-shared','-fPIC','-Wl,-Bsymbolic',ll,'-lm','-o',so],timeout=90)
            if p.returncode:raise RuntimeError('clang seed %d %s: %s'%(seed,opt,p.stderr))
            native=cmds.copy();native[0]='load m '+str(so)
            p=run([ROOT/'tests/out/harness'],input='\n'.join(native)+'\nquit\n',text=True,timeout=30)
            got=p.stdout.splitlines()
            if len(got)!=len(lines):raise RuntimeError('response count seed %d %s: %s %s'%(seed,opt,p.stdout,p.stderr))
            for cmd,rs,x,y in zip(cmds,meta,lines,got):
                if x==y:continue
                if rs and x.startswith('ok ') and y.startswith('ok '):
                    xs=x.split()[1:];ys=y.split()[1:]
                    if len(xs)==len(ys)==len(rs) and all(a[:SIZE[t]*2]==b[:SIZE[t]*2] for a,b,t in zip(xs,ys,rs)):continue
                raise RuntimeError('mismatch seed %d %s\n%s\nref %s\ngot %s'%(seed,opt,cmd,x,y))
        if seed%10==0:print('smith',seed,'passed',flush=True)
    print('smith:',a.count,'modules passed O0/O2')
if __name__=='__main__':main()

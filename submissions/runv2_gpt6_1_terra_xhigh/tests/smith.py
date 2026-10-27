#!/usr/bin/env python3
"""Deterministic, terminating wasm-smith differential smoke test."""
import os,pathlib,random,subprocess,tempfile,sys
from run import ROOT, Engine, slot, hx, equivalent
FLAGS=['--max-memories','0','--wide-arithmetic-enabled','false','--reference-types-enabled','false','--gc-enabled','false','--exceptions-enabled','false','--memory64-enabled','false','--relaxed-simd-enabled','false','--threads-enabled','false','--tail-call-enabled','false','--extended-const-enabled','false','--custom-descriptors-enabled','false','--compact-imports-enabled','false','--max-imports','0','--max-funcs','8','--max-instructions','80','--max-types','16','--max-table-elements','40','--max-memory32-bytes','262144','--max-element-segments','3','--max-data-segments','3','--canonicalize-nans','true','--ensure-termination','--fuel','300']
TEMPLATE='''(module
 (func (export "i32") (param i32 i32) (result i32) unreachable)
 (func (export "i64") (param i64 i64) (result i64) unreachable)
 (func (export "f32") (param f32 f32) (result f32) unreachable)
 (func (export "f64") (param f64 f64) (result f64) unreachable)
 (func (export "pair") (param i32 i64) (result i32 i64) unreachable)
 (memory (export "memory") 1 4))'''
def save_failure(wasm,seed):
    path=ROOT/'tests'/f'smith-failure-{seed}.wat'
    if not path.exists():subprocess.run(['wasm-tools','print',str(wasm),'-o',str(path)])

def run(n=100):
    with tempfile.TemporaryDirectory(dir=os.environ.get('TMPDIR')) as temp:
        d=pathlib.Path(temp); driver=d/'driver'; shape=d/'shape.wat';shape.write_text(TEMPLATE); shape_bin=d/'shape.wasm'
        subprocess.run(['clang','-O2','-rdynamic',str(ROOT/'tests/driver.c'),'-ldl','-o',str(driver)],check=True)
        subprocess.run(['wasm-tools','parse',str(shape),'-o',str(shape_bin)],check=True)
        accepted=0
        for i in range(n):
            seed=d/'seed';seed.write_bytes(random.Random(i+4851).randbytes(8192));wasm=d/'random.wasm';ll=d/'random.ll';so=d/'random.so'
            c=subprocess.run(['wasm-tools','smith',*FLAGS,'--exports',str(shape_bin),str(seed),'-o',str(wasm)],capture_output=True,text=True)
            if c.returncode: print('smith fail',i,c.stderr);continue
            try:
                ref=Engine(['w2l-ref'],True,wasm,init=False)
            except ValueError as e:
                print('outside reference scope',i,e,flush=True)
                continue
            try:
                c=subprocess.run([str(ROOT/'target/release/wasm2llvm'),str(wasm),'-o',str(ll)],capture_output=True,text=True)
                if c.returncode:
                    print('translator failed seed',i,c.stderr,flush=True);save_failure(wasm,i);return False
                for opt in ('-O0','-O2'):
                    c=subprocess.run(['clang',opt,'-shared','-fPIC','-Wl,-Bsymbolic',str(ll),'-lm','-o',str(so)],capture_output=True,text=True)
                    if c.returncode:print('clang failed seed',i,opt,c.stderr,flush=True);return False
                    if opt=='-O2':ref.close();ref=Engine(['w2l-ref'],True,wasm,init=False)
                    act=Engine([str(driver),str(so)],init=False)
                    try:
                        rinit=ref.run('init');ainit=act.run('init')
                        if rinit!=ainit:print('init mismatch',i,opt,rinit,ainit,flush=True);return False
                        if rinit!='ok':continue
                        for name,ty,args in [('i32',['i32'],[0,8]),('i64',['i64'],[0,8]),('f32',['f32'],[0x3fc00000,0xc0200000]),('f64',['f64'],[0x3ff8000000000000,0xc004000000000000]),('pair',['i32','i64'],[13,29])]:
                            params=[hx(name),str(len(ty)),*[slot(x) for x in args]];expected=ref.run('invoke',*params);actual=act.run('invoke',*params)
                            if not equivalent(expected,actual,ty):print('mismatch seed',i,opt,name,expected,actual,flush=True);save_failure(wasm,i);return False
                    finally:act.close()
            finally:ref.close()
            accepted+=1
            if i%10==0:print('smith',i,'passed',flush=True)
    print('smith passed',accepted,'in-scope modules from',n,'seeds');return True
if __name__=='__main__':sys.exit(not run(int(sys.argv[1]) if len(sys.argv)>1 else 100))

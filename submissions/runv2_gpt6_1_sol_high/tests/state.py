#!/usr/bin/env python3
"""Exact NaN global initialization, empty export names, and full wasm32 memory."""
from pathlib import Path
import subprocess
ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'tests/out'
OUT.mkdir(parents=True,exist_ok=True)
def invoke(name,*args,results=1):
    return 'invoke m %s %d %s'%(name.encode().hex(),results,' '.join((x&0xffffffff).to_bytes(16,'little').hex() for x in args))
def main():
    subprocess.run(['wasm-tools','parse',str(ROOT/'tests/state.wat'),'-o',str(OUT/'state.wasm')],check=True)
    subprocess.run([str(ROOT/'target/release/wasm2llvm'),str(OUT/'state.wasm'),'-o',str(OUT/'state.ll')],check=True)
    cmds=['load m '+str(OUT/'state.wasm'),'init m','global m -','global m '+b'double'.hex(),'global m '+b'vector'.hex(),
          invoke('grow',0),invoke('read',0),invoke('fill',0,7,0,results=0),invoke('fill',1,7,0,results=0),invoke('grow',65536),invoke('grow',1),
          invoke('read',0xffffffff),invoke('offset',0),invoke('offset',1),invoke('write',0xffffffff,171,results=0),invoke('read',0xffffffff),
          invoke('fill',0xffffffff,255,0,results=0),invoke('fill',0xffffffff,255,1,results=0),invoke('read',0xffffffff),invoke('fill',0xffffffff,255,2,results=0),
          invoke('read',0xffffffff)]
    p=subprocess.run(['w2l-ref'],input='\n'.join(cmds)+'\nquit\n',text=True,capture_output=True,timeout=30,check=True)
    ref=p.stdout.splitlines()
    for opt in ['O0','O2']:
        so=OUT/('state.'+opt+'.so')
        subprocess.run(['clang','-'+opt,'-shared','-fPIC','-Wl,-Bsymbolic',str(OUT/'state.ll'),'-lm','-o',str(so)],check=True)
        native=cmds.copy();native[0]='load m '+str(so)
        p=subprocess.run([str(OUT/'harness')],input='\n'.join(native)+'\nquit\n',text=True,capture_output=True,timeout=30,check=True)
        got=p.stdout.splitlines()
        if len(ref)!=len(got):raise RuntimeError(p.stdout)
        for j,(a,b) in enumerate(zip(ref,got)):
            n=32 if j==4 else 16 if j==3 else 8
            if a.startswith('ok ') and b.startswith('ok '):a=a[:3+n];b=b[:3+n]
            if a!=b:raise RuntimeError((opt,cmds[j],a,b))
        print(opt,'exact global bits and 4 GiB memory checks passed')
if __name__=='__main__':main()

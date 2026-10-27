#!/usr/bin/env python3
"""Deep finite calls, nontermination-by-recursion, and recovery after traps."""
from pathlib import Path
import subprocess
ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'tests/out'
OUT.mkdir(parents=True,exist_ok=True)

def main():
    subprocess.run(['wasm-tools','parse',str(ROOT/'tests/recursion.wat'),'-o',str(OUT/'recursion.wasm')],check=True)
    subprocess.run([str(ROOT/'target/release/wasm2llvm'),str(OUT/'recursion.wasm'),'-o',str(OUT/'recursion.ll')],check=True)
    def invoke(name,n=None):return 'invoke m %s %d %s'%(name.encode().hex(),int(n is not None),'' if n is None else n.to_bytes(16,'little').hex())
    cmds=['load m '+str(OUT/'recursion.wasm'),'init m',invoke('count',10000),invoke('tail',10000),invoke('forever'),invoke('count',10000),invoke('mutual'),invoke('tail',10000),invoke('trap'),invoke('count',10000)]
    ref=subprocess.run(['w2l-ref'],input='\n'.join(cmds)+'\nquit\n',text=True,capture_output=True,check=True,timeout=15).stdout.splitlines()
    for opt in ['O0','O2']:
        so=OUT/('recursion.'+opt+'.so')
        subprocess.run(['clang','-'+opt,'-shared','-fPIC','-Wl,-Bsymbolic',str(OUT/'recursion.ll'),'-lm','-o',str(so)],check=True)
        native=cmds.copy();native[0]='load m '+str(so)
        p=subprocess.run([str(OUT/'harness')],input='\n'.join(native)+'\nquit\n',text=True,capture_output=True,timeout=15)
        got=p.stdout.splitlines()
        for cmd,a,b in zip(cmds,ref,got):
            if cmd.startswith('invoke') and a.startswith('ok ') and b.startswith('ok '):a=a[:11];b=b[:11]
            if a!=b:raise RuntimeError((opt,cmd,a,b))
        if len(got)!=len(ref):raise RuntimeError((opt,p.returncode,p.stdout,p.stderr))
        print(opt,'recursion depth 10000, direct/mutual unbounded recursion and recovery passed')
if __name__=='__main__':main()

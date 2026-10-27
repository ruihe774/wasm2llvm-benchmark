#!/usr/bin/env python3
"""Translate and exercise the supplied audio codecs against w2l-ref."""
import os
import pathlib
import struct
import subprocess
import sys
import tempfile
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parent))
from run import ROOT, Engine, hx, slot, compare_pair

def call(ref,act,name,args=(),results=1):
    request=[hx(name),str(results),*[slot(x) for x in args]]
    expected=ref.run('invoke',*request);actual=act.run('invoke',*request)
    if results:
        from run import equivalent
        if not equivalent(expected,actual,['i32']*results):raise AssertionError((name,args,expected,actual))
        if not expected.startswith('ok '):return None
        return int.from_bytes(bytes.fromhex(expected.split()[1])[:4],'little')
    if expected!=actual:raise AssertionError((name,args,expected,actual))

def global_i32(ref,act,name):
    r=ref.run('global',hx(name));a=act.run('global',hx(name))
    from run import equivalent
    if not equivalent(r,a,['i32']):raise AssertionError((name,r,a))
    return int.from_bytes(bytes.fromhex(r.split()[1])[:4],'little')

def write(ref,act,addr,data):
    compare_pair(ref,act,'memwrite',[hx('memory'),str(addr),data.hex()])

def memory(ref,act,addr,n):
    compare_pair(ref,act,'memread',[hx('memory'),str(addr),str(n)])

def qoa(ref,act):
    heap=global_i32(ref,act,'heap_base')
    src=(heap+15)&~15;dst=src+4096;decoded=dst+8192
    call(ref,act,'encode_init',[1],0)
    for count in (1,20,64,127):
        samples=b''.join(struct.pack('<h', ((i*377+count*13)%50000)-25000) for i in range(count))
        write(ref,act,src,samples)
        length=call(ref,act,'encode_frame',[src,1,44100,count,dst])
        if length is None or length>4096:raise AssertionError(('encode length',length))
        memory(ref,act,dst,length)
        call(ref,act,'decode_frame',[dst,length,decoded])
        memory(ref,act,decoded,count*2)
    compare_pair(ref,act,'memhash',[hx('memory')])

def opus(ref,act,kind):
    prefix='mini'+kind
    init_args=[1,64000,5,0,0] if kind=='celt' else [24000,5,0]
    status=call(ref,act,prefix+'_init',init_args)
    if status != 0:raise AssertionError(('init',kind,status))
    ptr=call(ref,act,prefix+'_input_i16_ptr');out=call(ref,act,prefix+'_output_ptr')
    samples=960 if kind=='celt' else 320
    for frame in range(3):
        audio=b''.join(struct.pack('<h', ((i*977+frame*1979)%50000)-25000) for i in range(samples))
        write(ref,act,ptr,audio)
        n=call(ref,act,prefix+'_encode_i16')
        if n is None or n>1276:raise AssertionError(('encode',kind,n))
        memory(ref,act,out,n)
        call(ref,act,prefix+'_final_range')
    compare_pair(ref,act,'memhash',[hx('memory')])

def main():
    subprocess.run(['cargo','build','--release','--offline'],cwd=ROOT,check=True)
    with tempfile.TemporaryDirectory(dir=os.environ.get('TMPDIR')) as d:
        d=pathlib.Path(d);driver=d/'driver'
        subprocess.run(['clang','-O2','-rdynamic',str(ROOT/'tests/driver.c'),'-ldl','-o',str(driver)],check=True)
        for name,run in [('qoa-simd',qoa),('minicelt',lambda a,b:opus(a,b,'celt')),('minisilk',lambda a,b:opus(a,b,'silk'))]:
            wasm=ROOT/'payloads'/name/(name+'.wasm');ll=d/(name+'.ll')
            subprocess.run([str(ROOT/'target/release/wasm2llvm'),str(wasm),'-o',str(ll)],check=True)
            for opt in (0,2):
                so=d/f'{name}-O{opt}.so'
                subprocess.run(['clang',f'-O{opt}','-shared','-fPIC','-Wl,-Bsymbolic',str(ll),'-lm','-o',str(so)],check=True)
                ref=Engine(['w2l-ref'],True,wasm);act=Engine([str(driver),str(so)])
                try:run(ref,act)
                finally:ref.close();act.close()
                print(name,'-O'+str(opt),'passed',flush=True)
if __name__=='__main__':main()

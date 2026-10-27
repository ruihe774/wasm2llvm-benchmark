#!/usr/bin/env python3
"""Ensure run-length encoded locals do not allocate one slot per unused local."""
import os,pathlib,subprocess,tempfile
from run import ROOT, Engine, hx

def leb(n):
    out=[]
    while True:
        b=n&127;n>>=7;out.append(b|(128 if n else 0))
        if not n:return bytes(out)
def sec(i,b):return bytes([i])+leb(len(b))+b
body=b'\x01'+leb(1_000_001)+b'\x7f\x20'+leb(1_000_000)+b'\x0b'
wasm=(b'\x00asm\x01\x00\x00\x00'+sec(1,b'\x01\x60\x00\x01\x7f')+sec(3,b'\x01\x00')+
      sec(7,b'\x01\x05value\x00\x00')+sec(10,b'\x01'+leb(len(body))+body))
with tempfile.TemporaryDirectory(dir=os.environ.get('TMPDIR')) as d:
    d=pathlib.Path(d);w=d/'m.wasm';ll=d/'m.ll';so=d/'m.so';driver=d/'driver';w.write_bytes(wasm)
    subprocess.run([str(ROOT/'target/release/wasm2llvm'),str(w),'-o',str(ll)],check=True)
    assert ll.stat().st_size<30000, ll.stat().st_size
    subprocess.run(['clang','-O2','-shared','-fPIC','-Wl,-Bsymbolic',str(ll),'-lm','-o',str(so)],check=True)
    subprocess.run(['clang','-O2','-rdynamic',str(ROOT/'tests/driver.c'),'-ldl','-o',str(driver)],check=True)
    engine=Engine([str(driver),str(so)])
    try: assert engine.run('invoke',hx('value'),'1')=='ok '+('00'*16)
    finally:engine.close()
print('sparse locals passed')

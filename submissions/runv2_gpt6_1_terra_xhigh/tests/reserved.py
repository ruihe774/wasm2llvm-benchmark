#!/usr/bin/env python3
"""Probe padded reserved table-index immediates against the reference."""
import tempfile, pathlib, subprocess, os
from run import ROOT

def leb(n):
    r=[]
    while True:
        b=n&127;n>>=7;r.append(b|(128 if n else 0))
        if not n:return bytes(r)
def sec(i,b):return bytes([i])+leb(len(b))+b
def module(body):
    f=b'\x00'+body+b'\x0b'
    return b'\x00asm\x01\x00\x00\x00'+sec(1,b'\x01\x60\x00\x00')+sec(3,b'\x01\x00')+sec(4,b'\x01\x70\x00\x01')+sec(10,b'\x01'+leb(len(f))+f)
with tempfile.TemporaryDirectory(dir=os.environ.get('TMPDIR')) as d:
    for name,body in [('normal-indirect',bytes.fromhex('4100110000')),('padded-indirect',bytes.fromhex('410011008000')),('normal-table-copy',bytes.fromhex('410041004100fc0e0000')),('padded-table-copy',bytes.fromhex('410041004100fc0e800000'))]:
        p=pathlib.Path(d)/(name+'.wasm');p.write_bytes(module(body))
        ref=subprocess.run(['w2l-ref'],input=f'load x {p}\nquit\n',text=True,capture_output=True).stdout.strip()
        trans=subprocess.run([str(ROOT/'target/release/wasm2llvm'),str(p),'-o',str(p.with_suffix('.ll'))],capture_output=True,text=True)
        allowed=name!='padded-indirect'
        assert (ref.startswith('ok'))==allowed,(name,ref)
        assert (trans.returncode==0)==allowed,(name,trans.stderr)
        print(name,'passed')

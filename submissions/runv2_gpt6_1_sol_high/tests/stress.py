#!/usr/bin/env python3
"""Exercise multi-megabyte decoding/lowering without involving LLVM code generation."""
from pathlib import Path
import subprocess
import time
ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'tests/out'

def leb(n):
    b=bytearray()
    while n>=128:b.append((n&127)|128);n>>=7
    b.append(n)
    return bytes(b)
def section(tag,payload):return bytes([tag])+leb(len(payload))+payload

def main():
    OUT.mkdir(parents=True,exist_ok=True)
    body=b'\0'+bytes([0x41,1,0x41,2,0x6a,0x1a])*300000+b'\x0b'
    data=bytes(range(256))*16384
    wasm=(b'\0asm\1\0\0\0'+section(1,b'\1\x60\0\0')+section(3,b'\1\0')+
          section(5,b'\1\0'+leb(64))+section(10,b'\1'+leb(len(body))+body)+
          section(11,b'\1\0\x41\0\x0b'+leb(len(data))+data))
    path=OUT/'stress.wasm';path.write_bytes(wasm)
    start=time.monotonic()
    subprocess.run([str(ROOT/'target/release/wasm2llvm'),str(path),'-o',str(OUT/'stress.ll')],check=True,timeout=60)
    elapsed=time.monotonic()-start
    print('stress: %d input bytes, %d output bytes, %.3f seconds'%(len(wasm),(OUT/'stress.ll').stat().st_size,elapsed))
if __name__=='__main__':main()

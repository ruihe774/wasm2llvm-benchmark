#!/usr/bin/env python3
"""Exercise all scalar numeric instructions, including conversion boundaries."""
from pathlib import Path
import math
import random
import re
import struct
import subprocess
ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'tests/out'
OUT.mkdir(parents=True,exist_ok=True)
SIZES={'i32':4,'i64':8,'f32':4,'f64':8}

def sig(op):
    if 69<=op<=90:return (['i32' if op<80 else 'i64']*(1 if op in (69,80) else 2),['i32'])
    if 91<=op<=102:return (['f32' if op<97 else 'f64']*2,['i32'])
    if 103<=op<=138:
        ty='i32' if op<121 else 'i64';k=op-(103 if op<121 else 121)
        return [ty]*(1 if k<3 else 2),[ty]
    if 139<=op<=166:
        ty='f32' if op<153 else 'f64';k=op-(139 if op<153 else 153)
        return [ty]*(1 if k<7 else 2),[ty]
    if op==167:return ['i64'],['i32']
    if 168<=op<=171:return ['f32' if op<170 else 'f64'],['i32']
    if 172<=op<=173:return ['i32'],['i64']
    if 174<=op<=177:return ['f32' if op<176 else 'f64'],['i64']
    if 178<=op<=181:return ['i32' if op<180 else 'i64'],['f32']
    if op==182:return ['f64'],['f32']
    if 183<=op<=186:return ['i32' if op<185 else 'i64'],['f64']
    if op==187:return ['f32'],['f64']
    if 188<=op<=191:return [dict(zip(range(188,192),['f32','f64','i32','i64']))[op]],[dict(zip(range(188,192),['i32','i64','f32','f64']))[op]]
    if 192<=op<=196:return ['i32' if op<194 else 'i64'],['i32' if op<194 else 'i64']
    k=op-0xfc00
    return ['f32' if k%4<2 else 'f64'],['i32' if k<4 else 'i64']
def main():
    entries=[]
    text=(ROOT/'docs/wasm-reference-interpreter/binary/decode.ml').read_text()
    for h,name in re.findall(r'^  \| 0x([0-9a-f]+) -> ((?:i32|i64|f32|f64)_\w+)\s*$',text,re.M):
        op=int(h,16)
        if 69<=op<=196:entries.append((op,name.replace('_','.',1)))
    for k in range(8):entries.append((0xfc00+k,('i32' if k<4 else 'i64')+'.trunc_sat_'+('f32' if k%4<2 else 'f64')+('_s' if k%2==0 else '_u')))
    assert len(entries)==136,len(entries)
    wat=['(module']
    for op,name in entries:
        ps,rs=sig(op)
        wat.append('(func (export "%x") %s %s %s %s)'%(op,''.join('(param %s)'%t for t in ps),''.join('(result %s)'%t for t in rs),' '.join('local.get %d'%j for j in range(len(ps))),name))
    wat.append(')')
    (OUT/'scalar.wat').write_text('\n'.join(wat))
    subprocess.run(['wasm-tools','parse',str(OUT/'scalar.wat'),'-o',str(OUT/'scalar.wasm')],check=True)
    subprocess.run([str(ROOT/'target/release/wasm2llvm'),str(OUT/'scalar.wasm'),'-o',str(OUT/'scalar.ll')],check=True)
    rng=random.Random(123)
    pools={}
    for ty,w in [('i32',32),('i64',64)]:
        xs=[0,1,2,3,7,8,15,16,31,32,63,64,127,128,255,256,(1<<(w-1))-1,1<<(w-1),(1<<w)-1,(1<<w)-2]+[rng.getrandbits(w) for _ in range(20)]
        pools[ty]=[x.to_bytes(w//8,'little').ljust(16,b'\0').hex() for x in xs]
    for ty,w,fmt in [('f32',32,'<f'),('f64',64,'<d')]:
        xs=[0.,-0.,1.,-1.,.5,-.5,1.5,-1.5,2.5,-2.5,1.e-30,-1.e-30,float('inf'),float('-inf'),float('nan')]
        for n in [31,32,63,64]:
            for x in [float(2**n),-float(2**n)]:xs.extend([x,math.nextafter(x,float('inf')),math.nextafter(x,float('-inf')),x-1,x+1])
        bits=[struct.pack(fmt,x) for x in xs]
        if w==32:bits += [struct.pack('<I',x) for x in [0x7f800001,0xff800001,0x7fc00000,0x7fffffff,0xcf000001,0xcf000000,0xceffffff,0x4effffff,0x4f000000,0x4f7fffff,0x4f800000,0xbf7fffff,0xbf800000]]
        else:bits += [struct.pack('<Q',x) for x in [0x7ff0000000000001,0xfff0000000000001,0x7fffffffffffffff,0xc1e00000001fffff,0xc1e0000000200000,0x43dfffffffffffff,0x43e0000000000000,0x43efffffffffffff,0x43f0000000000000]]
        bits += [rng.randbytes(w//8) for _ in range(20)]
        pools[ty]=[x.ljust(16,b'\0').hex() for x in bits]
    cmds=['load m '+str(OUT/'scalar.wasm'),'init m'];meta=[None,None]
    for op,name in entries:
        ps,rs=sig(op)
        if len(ps)==1:argslist=[[a] for a in pools[ps[0]]]
        else:
            argslist=[[a,b] for a in pools[ps[0]][:20] for b in pools[ps[1]][:20]]
            argslist += [[rng.choice(pools[t]) for t in ps] for _ in range(100)]
        for args in argslist:
            cmds.append('invoke m %s 1 %s'% (('%x'%op).encode().hex(),' '.join(args)));meta.append((name,rs[0]))
    proc=subprocess.run(['w2l-ref'],input='\n'.join(cmds)+'\nquit\n',text=True,capture_output=True,check=True)
    ref=proc.stdout.splitlines()
    for opt in ['O0','O2']:
        so=OUT/('scalar.'+opt+'.so')
        subprocess.run(['clang','-'+opt,'-shared','-fPIC','-Wl,-Bsymbolic',str(OUT/'scalar.ll'),'-lm','-o',str(so)],check=True)
        native=cmds.copy();native[0]='load m '+str(so)
        proc=subprocess.run([str(OUT/'harness')],input='\n'.join(native)+'\nquit\n',text=True,capture_output=True)
        got=proc.stdout.splitlines();errors=[]
        for j in range(max(len(got),len(ref))):
            a=ref[j] if j<len(ref) else 'MISSING';b=got[j] if j<len(got) else 'MISSING'
            if a==b:continue
            if meta[j] and a.startswith('ok ') and b.startswith('ok '):
                name,t=meta[j];n=SIZES[t];x=bytes.fromhex(a.split()[1])[:n];y=bytes.fromhex(b.split()[1])[:n]
                if x==y:continue
                if t.startswith('f') and not any(z in name for z in ['reinterpret','abs','neg','copysign']):
                    fmt='<f' if t=='f32' else '<d'
                    if math.isnan(struct.unpack(fmt,x)[0]) and math.isnan(struct.unpack(fmt,y)[0]):continue
            errors.append((cmds[j],a,b))
        for error in errors[:20]:print(error)
        print(opt,'scalar',len(cmds),'commands',len(errors),'mismatches')
        if errors:raise SystemExit(1)
if __name__=='__main__':main()

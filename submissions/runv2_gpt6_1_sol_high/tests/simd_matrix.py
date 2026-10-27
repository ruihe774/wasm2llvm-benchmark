#!/usr/bin/env python3
"""Generate every SIMD 1.0 opcode, then compare edge/random executions with w2l-ref."""
import json
import math
from pathlib import Path
import random
import re
import struct
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'tests/out'
OUT.mkdir(exist_ok=True)

def signature(name):
    if name == 'v128.const': return [], ['v128']
    if name.startswith('v128.load'):
        return (['i32', 'v128'] if '_lane' in name else ['i32']), ['v128']
    if name.startswith('v128.store'): return ['i32', 'v128'], []
    if name.endswith('.splat'):
        return [{'i8x16':'i32','i16x8':'i32','i32x4':'i32','i64x2':'i64','f32x4':'f32','f64x2':'f64'}[name.split('.')[0]]], ['v128']
    if '.extract_lane' in name:
        return ['v128'], [{'i8x16':'i32','i16x8':'i32','i32x4':'i32','i64x2':'i64','f32x4':'f32','f64x2':'f64'}[name.split('.')[0]]]
    if '.replace_lane' in name:
        return ['v128', {'i8x16':'i32','i16x8':'i32','i32x4':'i32','i64x2':'i64','f32x4':'f32','f64x2':'f64'}[name.split('.')[0]]], ['v128']
    if name.endswith(('.all_true','.any_true','.bitmask')): return ['v128'], ['i32']
    if name.endswith(('.shl','.shr_s','.shr_u')): return ['v128','i32'], ['v128']
    if name == 'v128.bitselect': return ['v128'] * 3, ['v128']
    tail = name.split('.')[1]
    if tail in ['abs','neg','popcnt','not','ceil','floor','trunc','nearest','sqrt'] or tail.startswith(('extend_','extadd_','convert_','trunc_sat_','demote_','promote_')):
        return ['v128'], ['v128']
    return ['v128','v128'], ['v128']

def build():
    entries = re.findall(r'^\| `([^`]+)`\s*\|\s*`0x([0-9a-f]+)`', (ROOT/'docs/wasm-proposals/simd/BinarySIMD.md').read_text(), re.M)
    entries = sorted([(int(code,16),name) for name,code in entries])
    assert len(entries) == 236
    wat = ['(module (memory (export "memory") 1 2)', '(data (i32.const 0) "' + ''.join('\\%02x' % (i*73%256) for i in range(256)) + '")']
    cases=[]
    for code,name in entries:
        params,results = signature(name)
        variants=[0]
        if 'lane' in name:
            if '8_lane' in name or name.startswith('i8x16'): lanes=16
            elif '16_lane' in name or name.startswith('i16x8'): lanes=8
            elif '32_lane' in name or name.startswith(('i32x4','f32x4')): lanes=4
            else: lanes=2
            variants = [0,lanes-1]
        for lane in variants:
            export = '%02x_%d' % (code,lane)
            extra=''
            if name == 'v128.const': extra = ' i8x16 ' + ' '.join(str(i*17) for i in range(16))
            elif name == 'i8x16.shuffle': extra = ' ' + ' '.join(str((i*13+7)%32) for i in range(16))
            elif 'lane' in name: extra = ' %d' % lane
            wat.append('(func (export "%s") %s %s %s %s%s)' % (export, ''.join('(param %s)'%p for p in params), ''.join('(result %s)'%r for r in results), ' '.join('local.get %d'%i for i in range(len(params))), name,extra))
            cases.append((export,name,params,results))
    wat.append(')')
    (OUT/'simd.wat').write_text('\n'.join(wat))
    subprocess.run(['wasm-tools','parse',str(OUT/'simd.wat'),'-o',str(OUT/'simd.wasm')],check=True)
    (OUT/'simd-cases.json').write_text(json.dumps(cases))
    return cases

def vectors(rng):
    result=[bytes(16), bytes([255])*16, bytes(range(16)), bytes([128,127,0,255]*4), bytes([0,1,2,15,16,17,31,32,63,64,127,128,254,255,0,7])]
    result += [struct.pack('<4I',*v) for v in [(0x80000000,0,0x7f800000,0xff800000), (0x7fa12345,0x7fc54321,0xffc12345,0xffa43210), (0x4f000000,0x4f800000,0xcf000000,0xcf800000), (0x3f000000,0xbf000000,0x3fc00000,0xbfc00000)]]
    result += [struct.pack('<2Q',*v) for v in [(0x8000000000000000,0),(0x7ff0000000000000,0xfff0000000000000),(0x7ff0123456789abc,0xfff8123456789abc),(0x43e0000000000000,0xc3e0000000000000),(0x41e0000000000000,0x41f0000000000000)]]
    result += [rng.randbytes(16) for _ in range(14)]
    return result

def commands(cases,rounds=36):
    rng=random.Random(621989)
    vs=vectors(rng)
    cmds=['load a '+str(OUT/'simd.wasm'),'init a']
    meta=[None,None]
    for export,name,params,results in cases:
        for r in range(rounds):
            args=[]
            for j,t in enumerate(params):
                if t=='v128': b=vs[(r+j*7)%len(vs)]
                elif t=='i32':
                    if name.startswith('v128.load') or name.startswith('v128.store'):
                        x=[0,1,65520,65535,65536,0xffffffff][r%6]
                    else: x=[0,1,7,8,15,16,31,32,63,64,127,128,0xffffffff,rng.getrandbits(32)][r%14]
                    b=struct.pack('<I',x)
                elif t=='i64': b=struct.pack('<Q',rng.getrandbits(64))
                elif t=='f32': b=vs[r%len(vs)][:4]
                else: b=vs[r%len(vs)][:8]
                args.append(b.ljust(16,b'\x00').hex())
            cmds.append('invoke a %s %d %s' % (export.encode().hex(),len(results),' '.join(args)))
            meta.append((name,results))
    cmds += ['memhash a '+b'memory'.hex()]
    meta += [None]
    return cmds,meta

def equal(a,b,metadata):
    if a==b: return True
    if not metadata or not a.startswith('ok ') or not b.startswith('ok '): return False
    name,ts=metadata
    aa=a.split()[1:];bb=b.split()[1:]
    if len(aa)!=len(bb) or len(aa)!=len(ts):return False
    for x,y,t in zip(aa,bb,ts):
        x=bytes.fromhex(x);y=bytes.fromhex(y)
        n={'i32':4,'f32':4,'i64':8,'f64':8,'v128':16}[t]
        x=x[:n];y=y[:n]
        if x==y:continue
        is_arith=any(z in name for z in ('.add','.sub','.mul','.div','.sqrt','.min','.max','.nearest','.ceil','.floor','.trunc','.demote','.promote'))
        ft=None
        if t in ('f32','f64'):ft=t
        elif t=='v128' and is_arith and name.startswith('f32x4'):ft='f32'
        elif t=='v128' and is_arith and name.startswith('f64x2'):ft='f64'
        if ft is None:return False
        size=4 if ft=='f32' else 8;fmt='<f' if size==4 else '<d'
        if any(x[i:i+size]!=y[i:i+size] and not(math.isnan(struct.unpack(fmt,x[i:i+size])[0]) and math.isnan(struct.unpack(fmt,y[i:i+size])[0])) for i in range(0,n,size)):return False
    return True

def run_matrix(harness):
    cases=build()
    subprocess.run([str(ROOT/'target/release/wasm2llvm'),str(OUT/'simd.wasm'),'-o',str(OUT/'simd.ll')],check=True)
    cmds,meta=commands(cases)
    ref=subprocess.run(['w2l-ref'],input='\n'.join(cmds)+'\nquit\n',text=True,capture_output=True,check=True).stdout.splitlines()
    for opt in ('O0','O2'):
        so=OUT/('simd.'+opt+'.so')
        subprocess.run(['clang','-'+opt,'-shared','-fPIC','-Wl,-Bsymbolic',str(OUT/'simd.ll'),'-lm','-o',str(so)],check=True)
        nativecmd=cmds.copy();nativecmd[0]='load a '+str(so)
        proc=subprocess.run([str(harness)],input='\n'.join(nativecmd)+'\nquit\n',text=True,capture_output=True)
        got=proc.stdout.splitlines()
        errors=[]
        for j in range(max(len(got),len(ref))):
            if j>=len(got) or j>=len(ref) or not equal(ref[j],got[j],meta[j]):
                errors.append((j,cmds[j] if j<len(cmds) else '',ref[j] if j<len(ref) else 'MISSING',got[j] if j<len(got) else 'MISSING'))
        for error in errors[:20]:print(error)
        print(opt, 'SIMD',len(cmds),'commands',len(errors),'mismatches', 'exit',proc.returncode)
        if errors or proc.returncode: raise SystemExit(1)

if __name__=='__main__':
    if len(sys.argv)>1:run_matrix(Path(sys.argv[1]).resolve())
    else:build()

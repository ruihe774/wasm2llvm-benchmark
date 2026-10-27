#!/usr/bin/env python3
"""Offline ABI/differential smoke tests; requires clang, wasm-tools and w2l-ref."""
import math
import os
import pathlib
import random
import struct
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
TRANSLATOR = ROOT / 'target/release/wasm2llvm'

def command(*args):
    subprocess.run(args, cwd=ROOT, check=True)

def hx(n):
    return n.encode().hex() or '-'

def slot(x):
    return (int(x) & ((1 << 128)-1)).to_bytes(16, 'little').hex()

class Engine:
    def __init__(self, argv, ref=False, wasm=None, init=True):
        self.p = subprocess.Popen(argv, cwd=ROOT, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1)
        self.ref = ref
        if ref:
            response=self.raw(f'load x {wasm}')
            if response != 'ok':
                self.p.stdin.write('quit\n'); self.p.stdin.flush(); self.p.wait(timeout=5)
                raise ValueError(response)
        if init:
            assert self.run('init') == 'ok'
    def raw(self, s):
        self.p.stdin.write(s + '\n'); self.p.stdin.flush()
        r = self.p.stdout.readline().strip()
        if not r:
            raise RuntimeError(f'engine exited on {s!r} (status {self.p.poll()})')
        return r
    def run(self, op, *args):
        return self.raw(' '.join([op] + (['x'] if self.ref else []) + list(args)))
    def close(self):
        self.raw('ping')
        self.p.stdin.write('quit\n');self.p.stdin.flush();self.p.wait(timeout=5)

def equivalent(a,b,result_types=()):
    if not a.startswith('ok ') or not b.startswith('ok '): return a == b
    x, y = a.split()[1:], b.split()[1:]
    if len(x)!=len(y): return False
    for i,(aa,bb) in enumerate(zip(x,y)):
        t=result_types[i] if i<len(result_types) else None
        if t is None:
            if aa!=bb: return False
            continue
        width={'i32':4,'i64':8,'f32':4,'f64':8,'v128':16}[t]
        aa,bb=bytes.fromhex(aa)[:width],bytes.fromhex(bb)[:width]
        if aa==bb: continue
        if t in ('f32','f64'):
            fmt='<f' if t=='f32' else '<d'
            if math.isnan(struct.unpack(fmt,aa)[0]) and math.isnan(struct.unpack(fmt,bb)[0]): continue
        return False
    return True

def compare_pair(ref, actual, op, args, types=()):
    expected=ref.run(op,*args); got=actual.run(op,*args)
    if not equivalent(expected,got,types):
        raise AssertionError(f'{op} {args}: reference={expected}, actual={got}')

def run_pair(driver, so, wasm, f):
    ref=Engine(['w2l-ref'],True,wasm)
    actual=Engine([str(driver),str(so)])
    try: f(ref,actual)
    finally: ref.close();actual.close()

def core_test(ref, actual):
    def inv(name,*vals,result='i32'):
        compare_pair(ref,actual,'invoke',[hx(name), '1' if result else '0',*[slot(v) for v in vals]], [result] if result else [])
    for x in (0,1,2,20):
        inv('if',x,32);inv('loop',x);inv('loop_params',x);inv('branch',x);inv('branch_table',x)
    for x in (0,1,2,3):inv('indirect',x,21);inv('indirect_duplicate_type',x,21)
    inv('table_init',result=None);inv('indirect',2,21);inv('table_drop',result=None)
    inv('table_init_range',4,0,0,result=None);inv('table_init_range',5,0,0,result=None);inv('table_init_range',0,1,0,result=None)
    inv('table_copy',1,0,2,result=None);inv('indirect',1,21);inv('indirect',2,21)
    inv('table_copy',4,4,0,result=None);inv('table_copy',5,0,0,result=None)
    for args in ((2,3,0),(2,3,1),(-1,5,1)): inv('select',*args)
    inv('load',4);inv('load8s',4);inv('load',65535)
    inv('store',65534,1,result=None);inv('store',25,0xdeadbeef,result=None);inv('load',25)
    inv('fill',100,0xab,20,result=None);inv('copy',105,100,20,result=None)
    inv('fill',65536,0,0,result=None);inv('fill',65537,0,0,result=None)
    inv('copy',65536,65536,0,result=None);inv('copy',65537,0,0,result=None)
    inv('data_init',200,1,4,result=None);inv('data_init',200,8,1,result=None);inv('data_drop',result=None);inv('data_init',0,0,1,result=None)
    inv('data_init',0,0,0,result=None);inv('data_init',0,1,0,result=None)
    inv('grow',1);inv('grow',100);inv('increment_global',result=None)
    compare_pair(ref,actual,'global',[hx('g')],['i32'])
    compare_pair(ref,actual,'memhash',[hx('memory')])
    inv('boom',result=None);inv('polymorphic_select',result=None);inv('after')
    inv('');inv('名前😀',123)
    inv('write_then_trap',result=None);inv('load8s',500)
    inv('say',-19,result=None);compare_pair(ref,actual,'output',[])
    inv('recurse',100);inv('recurse',10000)
    inv('unbounded',result=None);inv('after');inv('recurse',100)

def simd_memory_wat():
    instrs=['load','load8x8_s','load8x8_u','load16x4_s','load16x4_u','load32x2_s','load32x2_u','load8_splat','load16_splat','load32_splat','load64_splat','load32_zero','load64_zero']
    lines=['(module (memory (export "memory") 1)']
    for op in instrs:
        lines.append(f'(func (export "{op}") (param i32) (result v128) local.get 0 v128.{op})')
    lines.append('(func (export "store") (param i32 v128) local.get 0 local.get 1 v128.store)')
    for width in (8,16,32,64):
        for lane in sorted({0,128//width//2,128//width-1}):
            for op in ('load','store'):
                ret='(result v128)' if op=='load' else ''
                lines.append(f'(func (export "{op}{width}_lane{lane}") (param i32 v128) {ret} local.get 0 local.get 1 v128.{op}{width}_lane {lane})')
    return '\n'.join(lines)+')',instrs

def simd_memory_test(instrs):
    def execute(ref,actual):
        for addr in (0,65520):
            compare_pair(ref,actual,'memwrite',[hx('memory'),str(addr),bytes(range(128,144)).hex()])
        for op in instrs:
            for addr in (0,1,65520,65528,65535,65536,0xffffffff):
                compare_pair(ref,actual,'invoke',[hx(op),'1',slot(addr)],['v128'])
        value=int.from_bytes(bytes(range(16)),'little')
        compare_pair(ref,actual,'invoke',[hx('store'),'0',slot(20),slot(value)])
        for width in (8,16,32,64):
            for lane in sorted({0,128//width//2,128//width-1}):
                for addr in (0,65528,65535):
                    name=f'load{width}_lane{lane}'
                    compare_pair(ref,actual,'invoke',[hx(name),'1',slot(addr),slot(value)],['v128'])
                name=f'store{width}_lane{lane}'
                compare_pair(ref,actual,'invoke',[hx(name),'0',slot(32),slot(value)])
        compare_pair(ref,actual,'memhash',[hx('memory')])
    return execute

def memory_grow_test(ref,actual):
    def inv(name,*vals,result='i32'):
        compare_pair(ref,actual,'invoke',[hx(name), '1' if result else '0',*[slot(v) for v in vals]], [result] if result else [])
    inv('size'); inv('grow',1); inv('store8',65535,0xef,result=None);inv('load8',65535)
    inv('grow',65535);inv('size');inv('load8',0xffffffff);inv('store8',0xffffffff,0x91,result=None);inv('load8',0xffffffff);inv('load32',0xffffffff);inv('grow',1)

def multi_test(ref,actual):
    for n,values in [('swap',(1,2)),('sum_diff',(17,5)),('block_pair',(3,))]:
        compare_pair(ref,actual,'invoke',[hx(n),'2',*[slot(v) for v in values]],['i32','i32'])
    compare_pair(ref,actual,'invoke',[hx('block_param'),'1',slot(4),slot(5)],['i32'])

def numeric_wat():
    funcs=[]; tests=[]
    def add(op,args,ret,inputs):
        name=op.replace('.','_'); params=' '.join('(param '+t+')' for t in args)
        body=' '.join('local.get '+str(i) for i in range(len(args)))
        funcs.append(f'(func (export "{name}") {params} (result {ret}) {body} {op})')
        tests.append((name,ret,inputs))
    ints=['eq','ne','lt_s','lt_u','gt_s','gt_u','le_s','le_u','ge_s','ge_u','add','sub','mul','div_s','div_u','rem_s','rem_u','and','or','xor','shl','shr_s','shr_u','rotl','rotr']
    for t,bits in [('i32',32),('i64',64)]:
        for op in ints:
            ret='i32' if op in ints[:10] else t
            add(t+'.'+op,[t,t],ret,[(0,0),(1,2),((1<<(bits-1)),(1<<bits)-1),((1<<bits)-1,3),(13,bits+1)])
        for op in ['eqz','clz','ctz','popcnt']:
            add(t+'.'+op,[t], 'i32' if op=='eqz' else t, [(0,),(1,),((1<<bits)-1,)])
    for t,bits in [('f32',32),('f64',64)]:
        def f(x): return int.from_bytes(struct.pack('<f' if bits==32 else '<d',x),'little')
        vals=[(f(0.0),f(-0.0)),(f(1.5),f(-2.5)),(f(float('nan')),f(1.0)),(f(float('inf')),f(1.0))]
        for op in ['eq','ne','lt','gt','le','ge','add','sub','mul','div','min','max','copysign']:
            add(t+'.'+op,[t,t], 'i32' if op in ['eq','ne','lt','gt','le','ge'] else t, vals)
        for op in ['abs','neg','ceil','floor','trunc','nearest','sqrt']:
            add(t+'.'+op,[t],t,[(f(x),) for x in [0.,-0.,1.5,2.5,-1.5,float('inf'),float('nan')]])
        for it in ['i32','i64']:
            for op in ['trunc','trunc_sat']:
                for sign in ['s','u']:
                    values=[0.,-0.9,-1.1,1.9,2147483648.,-2147483648.,float('inf'),float('nan')]
                    if bits==64:values.extend([-2147483648.9,2147483647.9,-4294967295.5,4294967295.9,float(2**63),float(2**64)])
                    add(it+'.'+op+'_'+t+'_'+sign,[t],it,[(f(x),) for x in values])
    add('i32.wrap_i64',['i64'],'i32',[(0,),(0x12345678fedcba98,),((1<<64)-1,)])
    for sign in ['s','u']:
        add('i64.extend_i32_'+sign,['i32'],'i64',[(0,),(0x80000000,), (0xffffffff,)])
        for it in ['i32','i64']:
            for ft in ['f32','f64']:
                width=32 if it=='i32' else 64
                vals=[0,1,(1<<(width-1))-1,1<<(width-1),(1<<width)-1]
                add(ft+'.convert_'+it+'_'+sign,[it],ft,[(x,) for x in vals])
    add('f32.demote_f64',['f64'],'f32',[(int.from_bytes(struct.pack('<d',x),'little'),) for x in [0.,-0.,1.5,2**-126,2**128,float('nan')]])
    add('f64.promote_f32',['f32'],'f64',[(int.from_bytes(struct.pack('<f',x),'little'),) for x in [0.,-0.,1.5,2**-126,float('nan')]])
    for it,ft,bits in [('i32','f32',32),('i64','f64',64)]:
        vals=[0,1,(1<<(bits-1)),(1<<bits)-1,0x7fc00001 if bits==32 else 0x7ff8000000000001]
        add(it+'.reinterpret_'+ft,[ft],it,[(v,) for v in vals])
        add(ft+'.reinterpret_'+it,[it],ft,[(v,) for v in vals])
        for narrow in ([8,16] if bits==32 else [8,16,32]):
            add(it+'.extend'+str(narrow)+'_s',[it],it,[(0,),(1<<(narrow-1),),((1<<narrow)-1,)])
    return '(module\n'+'\n'.join(funcs)+'\n)', tests

def numeric_test(tests):
    def execute(ref,actual):
        for name,ret,inputs in tests:
            for inp in inputs:
                compare_pair(ref,actual,'invoke',[hx(name),'1',*[slot(v) for v in inp]],[ret])
    return execute

def main():
    command('cargo','build','--release','--offline')
    with tempfile.TemporaryDirectory(dir=os.environ.get('TMPDIR')) as d:
        d=pathlib.Path(d);driver=d/'driver'
        command('clang','-O2','-rdynamic',str(ROOT/'tests/driver.c'),'-ldl','-o',str(driver))
        numeric,tests=numeric_wat();(d/'numeric.wat').write_text(numeric)
        simd_memory,mem_instrs=simd_memory_wat();(d/'simd-memory.wat').write_text(simd_memory)
        cases=[('multi',ROOT/'payloads/multi-value/multi-value.wasm',multi_test),('core',ROOT/'tests/core.wat',core_test),('memory_grow',ROOT/'tests/memory_grow.wat',memory_grow_test),('numeric',d/'numeric.wat',numeric_test(tests)),('simd-memory',d/'simd-memory.wat',simd_memory_test(mem_instrs))]
        for name,source,check in cases:
            wasm=d/(name+'.wasm')
            if str(source).endswith('.wat'): command('wasm-tools','parse',str(source),'-o',str(wasm))
            else: wasm=source
            ll=d/(name+'.ll'); command(str(TRANSLATOR),str(wasm),'-o',str(ll))
            for opt in ('-O0','-O2'):
                so=d/(name+opt+'.so');command('clang',opt,'-shared','-fPIC','-Wl,-Bsymbolic',str(ll),'-lm','-o',str(so))
                run_pair(driver,so,wasm,check)
                print(name,opt,'passed',flush=True)

if __name__=='__main__': main()

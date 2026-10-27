//! Structured wasm control flow lowered to LLVM blocks and fixed operand slots.
use crate::{numeric,simd};
use crate::wasm::ValType::{self, I32, I64, F32, F64, V128, FuncRef, ExternRef};
use std::collections::BTreeSet;
use std::fmt::Write;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sig { pub params: Vec<ValType>, pub results: Vec<ValType> }
#[derive(Clone)]
pub struct Env {
    pub types: Vec<Sig>, pub funcs: Vec<u32>, pub globals: Vec<(ValType,bool)>,
    pub memory: bool, pub tables: Vec<ValType>, pub data_count: usize, pub elem_types: Vec<ValType>,
}
pub struct FunctionIR { pub body: String, pub numeric: BTreeSet<u32>, pub simd: BTreeSet<(u32,Vec<u8>)>, pub indirect: BTreeSet<(u32,u32)> }
pub fn simd_name(op:u32, imm:&[u8])->String {let suffix:String=imm.iter().map(|b|format!("{b:02x}")).collect();format!("w2l_simd_{op:x}_{suffix}")}

pub struct Reader<'a> { bytes: &'a [u8], pos: usize }
impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self { Self { bytes, pos: 0 } }
    fn byte(&mut self) -> Result<u8,String> { let b=*self.bytes.get(self.pos).ok_or("unexpected end of function")?; self.pos+=1; Ok(b) }
    fn bytes(&mut self, n:usize) -> Result<&'a [u8],String> { let end=self.pos.checked_add(n).ok_or("length overflow")?; let b=self.bytes.get(self.pos..end).ok_or("truncated immediate")?; self.pos=end; Ok(b) }
    fn u32(&mut self) -> Result<u32,String> {
        let mut n=0u64;
        for i in 0..5 { let b=self.byte()?; n|=((b&127) as u64)<<(7*i); if b&128==0 { return u32::try_from(n).map_err(|_|"integer too large".into()); } }
        Err("integer representation too long".into())
    }
    fn signed(&mut self, bits:u32) -> Result<i64,String> {
        let mut n=0i128; let mut shift=0u32;
        for _ in 0..((bits+6)/7) { let b=self.byte()?; n|=((b&127) as i128)<<shift; shift+=7; if b&128==0 { if b&64!=0 { n |= (!0i128)<<shift; } let min=-(1i128<<(bits-1)); let max=(1i128<<(bits-1))-1; if n<min||n>max {return Err("signed integer too large".into())} return Ok(n as i64); } }
        Err("signed integer representation too long".into())
    }
    fn done(&self)->bool {self.pos==self.bytes.len()}
}
fn value_type(b:u8)->Result<ValType,String> { match b {0x7f=>Ok(I32),0x7e=>Ok(I64),0x7d=>Ok(F32),0x7c=>Ok(F64),0x7b=>Ok(V128),0x70=>Ok(FuncRef),0x6f=>Ok(ExternRef),_=>Err(format!("invalid value type {b:#x}"))} }

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind { Function, Block, Loop, If }
#[derive(Clone)]
struct Frame { kind:Kind, base:usize, params:Vec<ValType>, results:Vec<ValType>, target:String, end:String, otherwise:String, has_else:bool, unreachable:bool }
impl Frame { fn branch_types(&self)->&[ValType] { if self.kind==Kind::Loop { &self.params } else { &self.results } } }

struct Compiler<'a> {
    env:&'a Env, r:Reader<'a>, out:String, stack:Vec<Option<ValType>>, frames:Vec<Frame>,
    params:Vec<ValType>, local_runs:Vec<(u32,ValType)>, locals_used:BTreeSet<u32>, max_stack:usize, max_args:usize, max_results:usize,
    next_id:usize, numeric:BTreeSet<u32>, simd:BTreeSet<(u32,Vec<u8>)>, indirect:BTreeSet<(u32,u32)>,
}
impl<'a> Compiler<'a> {
    fn line(&mut self, line:impl AsRef<str>) { writeln!(self.out,"  {}",line.as_ref()).unwrap(); }
    fn fresh(&mut self)->String { let n=self.next_id; self.next_id+=1; format!("%v{n}") }
    fn label(&mut self)->String { let n=self.next_id; self.next_id+=1; format!("b{n}") }
    fn put_label(&mut self,l:&str) { writeln!(self.out,"{l}:").unwrap(); }
    fn top(&self)->&Frame { self.frames.last().expect("frame") }
    fn touch(&mut self, i:usize) {self.max_stack=self.max_stack.max(i+1);}
    fn pop(&mut self, want:Option<ValType>)->Result<usize,String> {
        let f=self.top(); let (base,unreachable)=(f.base,f.unreachable);
        if self.stack.len()==base {
            if unreachable { self.touch(base); return Ok(base); }
            return Err("operand stack underflow".into());
        }
        let i=self.stack.len()-1; let actual=self.stack.pop().unwrap();
        if let (Some(x),Some(y))=(actual,want) { if x!=y { return Err(format!("operand type mismatch: {x:?} vs {y:?}")); } }
        Ok(i)
    }
    fn push(&mut self,t:Option<ValType>)->usize {let i=self.stack.len(); self.stack.push(t); self.touch(i); i}
    fn load(&mut self, i:usize)->String {self.touch(i); let v=self.fresh(); self.line(format!("{v} = load i128, ptr %s{i}, align 16")); v}
    fn store(&mut self,i:usize,v:&str) {self.touch(i); self.line(format!("store i128 {v}, ptr %s{i}, align 16"));}
    fn push_val(&mut self,t:ValType,v:&str) {let i=self.push(Some(t));self.store(i,v);}
    fn get_pop(&mut self,t:ValType)->Result<String,String> {let i=self.pop(Some(t))?; Ok(self.load(i))}
    fn i32(&mut self,v:&str)->String {let x=self.fresh();self.line(format!("{x} = trunc i128 {v} to i32"));x}
    fn widen(&mut self,v:&str,ty:&str)->String {let x=self.fresh();self.line(format!("{x} = zext {ty} {v} to i128"));x}
    fn dead(&mut self) {let f=self.frames.last_mut().unwrap(); self.stack.truncate(f.base);f.unreachable=true;let lab=self.label();self.put_label(&lab);}
    fn check_args(&mut self, types:&[ValType])->Result<Vec<usize>,String> {
        let mut v=Vec::new(); for &t in types.iter().rev() { v.push(self.pop(Some(t))?); } v.reverse(); Ok(v)
    }
    fn check_branch(&mut self, frame:&Frame)->Result<Vec<usize>,String> {
        let orig=self.stack.clone(); let args=self.check_args(frame.branch_types())?; self.stack=orig; Ok(args)
    }
    fn move_values(&mut self, from:&[usize], base:usize) {
        let values:Vec<_>=from.iter().map(|&i|self.load(i)).collect();
        for (i,v) in values.iter().enumerate() { self.store(base+i,v); }
    }
    fn finish_arm(&mut self, frame:&Frame)->Result<(),String> {
        let values=self.check_args(&frame.results)?;
        if self.stack.len()!=frame.base { return Err("values left on stack at end of block".into()); }
        self.move_values(&values,frame.base);
        self.line(format!("br label %{}",frame.end));
        Ok(())
    }
    fn block_sig(&mut self)->Result<Sig,String> {
        let b=self.r.byte()?;
        if b==0x40 {return Ok(Sig{params:vec![],results:vec![]})}
        if let Ok(t)=value_type(b) {if t.is_ref(){return Err("reference types are outside the supported scope".into())}return Ok(Sig{params:vec![],results:vec![t]})}
        self.r.pos-=1; let i=self.r.signed(33)?; if i<0 {return Err("invalid block type".into())}
        self.env.types.get(i as usize).cloned().ok_or("block type index out of bounds".into())
    }
    fn start_control(&mut self,kind:Kind)->Result<(),String> {
        let sig=self.block_sig()?;
        let cond=if kind==Kind::If { let v=self.get_pop(I32)?;Some(self.i32(&v)) } else {None};
        let vals=self.check_args(&sig.params)?;
        let base=self.stack.len();
        let loaded:Vec<_>=vals.iter().map(|&i|self.load(i)).collect();
        for (&t,v) in sig.params.iter().zip(loaded.iter()) {self.push_val(t,v);}
        let end=self.label(); let otherwise=if kind==Kind::If {self.label()} else {String::new()};
        let target=if kind==Kind::Loop {let l=self.label();self.line(format!("br label %{l}"));self.put_label(&l);l} else {end.clone()};
        if let Some(cond)=cond {let yes=self.label();let test=self.fresh();self.line(format!("{test} = icmp ne i32 {cond}, 0"));self.line(format!("br i1 {test}, label %{yes}, label %{otherwise}"));self.put_label(&yes);}
        self.frames.push(Frame{kind,base,params:sig.params,results:sig.results,target,end,otherwise,has_else:false,unreachable:false});
        Ok(())
    }
    fn else_arm(&mut self)->Result<(),String> {
        let frame=self.top().clone();
        if frame.kind!=Kind::If || frame.has_else {return Err("unexpected else".into())}
        self.finish_arm(&frame)?;
        self.put_label(&frame.otherwise);
        self.stack.truncate(frame.base);
        for t in &frame.params { self.push(Some(*t)); }
        let f=self.frames.last_mut().unwrap(); f.has_else=true;f.unreachable=false;
        Ok(())
    }
    /// Returns true for the function's closing end.
    fn end(&mut self)->Result<bool,String> {
        let f=self.top().clone();
        self.finish_arm(&f)?;
        if f.kind==Kind::If && !f.has_else {
            if f.params!=f.results {return Err("if without else has incompatible result".into())}
            self.put_label(&f.otherwise); self.line(format!("br label %{}",f.end));
        }
        self.frames.pop();
        if f.kind==Kind::Function {return Ok(true)}
        self.put_label(&f.end); self.stack.truncate(f.base); for t in f.results {self.push(Some(t));}
        Ok(false)
    }
    fn frame_at(&self,depth:u32)->Result<Frame,String> {let i=self.frames.len().checked_sub(depth as usize+1).ok_or("invalid branch depth")?; Ok(self.frames[i].clone())}
    fn do_branch(&mut self,depth:u32, conditional:bool)->Result<(),String> {
        let cond=if conditional {let v=self.get_pop(I32)?;Some(self.i32(&v))} else {None};
        let f=self.frame_at(depth)?; let args=self.check_branch(&f)?;
        if let Some(cond)=cond {
            let taken=self.label();let next=self.label();let c=self.fresh();
            self.line(format!("{c} = icmp ne i32 {cond}, 0"));self.line(format!("br i1 {c}, label %{taken}, label %{next}"));
            self.put_label(&taken);self.move_values(&args,f.base);self.line(format!("br label %{}",f.target));self.put_label(&next);
        } else {self.move_values(&args,f.base);self.line(format!("br label %{}",f.target));self.dead();}
        Ok(())
    }
    fn br_table(&mut self)->Result<(),String> {
        let n=self.r.u32()? as usize;if n>self.r.bytes.len(){return Err("br_table too long".into())}
        let mut depths=Vec::with_capacity(n+1);for _ in 0..=n {depths.push(self.r.u32()?);}
        let sel=self.get_pop(I32)?;let sel=self.i32(&sel);
        let frames:Vec<_>=depths.iter().map(|&d|self.frame_at(d)).collect::<Result<_,_>>()?;
        let ty=frames.last().unwrap().branch_types().to_vec();
        for f in &frames {if f.branch_types()!=ty {return Err("br_table label types differ".into())}}
        let args=self.check_branch(frames.last().unwrap())?;
        let labs:Vec<_>=(0..=n).map(|_|self.label()).collect();
        self.line(format!("switch i32 {sel}, label %{} [",labs[n]));
        for i in 0..n {self.line(format!("  i32 {i}, label %{}",labs[i]));}self.line("]");
        for (f,l) in frames.iter().zip(&labs) {self.put_label(l);self.move_values(&args,f.base);self.line(format!("br label %{}",f.target));}
        self.dead();Ok(())
    }
    fn local_type(&self,i:u32)->Result<ValType,String> {
        if let Some(t)=self.params.get(i as usize) {return Ok(*t)}
        let n=self.local_runs.partition_point(|&(end,_)|end<=i);
        self.local_runs.get(n).map(|&(_,t)|t).ok_or_else(||"invalid local index".into())
    }
    fn local_get(&mut self,i:u32)->Result<(),String> {let t=self.local_type(i)?;self.locals_used.insert(i);let v=self.fresh();self.line(format!("{v} = load i128, ptr %l{i}, align 16"));self.push_val(t,&v);Ok(())}
    fn local_set(&mut self,i:u32,tee:bool)->Result<(),String> {let t=self.local_type(i)?;self.locals_used.insert(i);let idx=self.pop(Some(t))?;let v=self.load(idx);self.line(format!("store i128 {v}, ptr %l{i}, align 16"));if tee {self.push_val(t,&v)}Ok(())}
    fn global(&mut self,i:u32,set:bool)->Result<(),String> {let (t,mutable)=*self.env.globals.get(i as usize).ok_or("invalid global index")?;if set {if !mutable{return Err("immutable global set".into())}let v=self.get_pop(t)?;self.line(format!("store i128 {v}, ptr @w2l_g{i}, align 16"));}else {let v=self.fresh();self.line(format!("{v} = load i128, ptr @w2l_g{i}, align 16"));self.push_val(t,&v);}Ok(())}
    fn call(&mut self,index:u32, indirect:Option<u32>)->Result<(),String> {
        let tid=if indirect.is_some(){index}else{*self.env.funcs.get(index as usize).ok_or("invalid function index")?};
        let sig=self.env.types.get(tid as usize).ok_or("invalid function type")?.clone();
        let table_index=if let Some(tab)=indirect {if self.env.tables.get(tab as usize)!=Some(&FuncRef){return Err("invalid indirect-call table".into())}let i=self.get_pop(I32)?;Some(self.i32(&i))}else{None};
        let args=self.check_args(&sig.params)?;self.max_args=self.max_args.max(sig.params.len());self.max_results=self.max_results.max(sig.results.len());
        for (j,i) in args.iter().enumerate() {let v=self.load(*i);let p=self.fresh();self.line(format!("{p} = getelementptr i128, ptr %callargs, i64 {j}"));self.line(format!("store i128 {v}, ptr {p}, align 16"));}
        if let Some(x)=table_index {let table=indirect.unwrap();self.indirect.insert((tid,table));self.line(format!("call void @w2l_indirect_{tid}_{table}(i32 {x}, ptr %callargs, ptr %callresults)"));}
        else {self.line(format!("call void @w2l_f{index}(ptr %callargs, ptr %callresults)"));}
        for (j,t) in sig.results.iter().enumerate() {let p=self.fresh();self.line(format!("{p} = getelementptr i128, ptr %callresults, i64 {j}"));let v=self.fresh();self.line(format!("{v} = load i128, ptr {p}, align 16"));self.push_val(*t,&v);}
        Ok(())
    }
    fn numeric(&mut self,op:u32)->Result<(),String> {
        let (types,ret)=numeric::signature(op).ok_or_else(||format!("unsupported numeric opcode {op:x}"))?;
        let args=self.check_args(&types)?;let a=self.load(args[0]);let b=if args.len()>1 {self.load(args[1])} else {"0".into()};let v=self.fresh();
        self.line(format!("{v} = call i128 @w2l_num_{op:x}(i128 {a}, i128 {b})"));self.push_val(ret,&v);self.numeric.insert(op);Ok(())
    }
    fn memop(&mut self,op:u8)->Result<(),String> {
        if !self.env.memory {return Err("memory instruction without memory".into())}
        let (t,width,store,signed):(ValType,u32,bool,bool)=match op {
            0x28=>(I32,4,false,false),0x29=>(I64,8,false,false),0x2a=>(F32,4,false,false),0x2b=>(F64,8,false,false),
            0x2c=>(I32,1,false,true),0x2d=>(I32,1,false,false),0x2e=>(I32,2,false,true),0x2f=>(I32,2,false,false),
            0x30=>(I64,1,false,true),0x31=>(I64,1,false,false),0x32=>(I64,2,false,true),0x33=>(I64,2,false,false),0x34=>(I64,4,false,true),0x35=>(I64,4,false,false),
            0x36=>(I32,4,true,false),0x37=>(I64,8,true,false),0x38=>(F32,4,true,false),0x39=>(F64,8,true,false),
            0x3a=>(I32,1,true,false),0x3b=>(I32,2,true,false),0x3c=>(I64,1,true,false),0x3d=>(I64,2,true,false),0x3e=>(I64,4,true,false),_=>unreachable!()};
        let align=self.r.u32()?;let offset=self.r.u32()?;if align>width.trailing_zeros(){return Err("invalid memory alignment".into())}
        let value=if store {Some(self.get_pop(t)?)} else {None};let addr=self.get_pop(I32)?;let addr=self.i32(&addr);
        let ptr=self.fresh();self.line(format!("{ptr} = call ptr @w2l_mem_ptr(i32 {addr}, i64 {offset}, i64 {width})"));let wt=format!("i{}",width*8);
        if let Some(value)=value {let v=self.fresh();self.line(format!("{v} = trunc i128 {value} to {wt}"));self.line(format!("store {wt} {v}, ptr {ptr}, align 1"));}
        else {let v=self.fresh();self.line(format!("{v} = load {wt}, ptr {ptr}, align 1"));let out=if signed {let to=if t==I32 {"i32"}else{"i64"};let ext=self.fresh();self.line(format!("{ext} = sext {wt} {v} to {to}"));self.widen(&ext,to)} else {self.widen(&v,&wt)};self.push_val(t,&out);}
        Ok(())
    }
    fn simd(&mut self)->Result<(),String> {
        let op=self.r.u32()?;
        if (0..=11).contains(&op)||(84..=93).contains(&op) {return self.simd_memory(op)}
        if op==12 {let b=self.r.bytes(16)?;let value=u128::from_le_bytes(b.try_into().unwrap());self.push_val(V128,&value.to_string());return Ok(())}
        let (types,result)=simd::signature(op).ok_or_else(||format!("unsupported SIMD opcode {op}"))?;
        let imm=self.r.bytes(simd::immediate_len(op))?.to_vec();
        let name=simd_name(op,&imm);
        // Immediate validity is part of validation even when the instruction is dead.
        simd::helper(op,&imm,&name)?;
        let args=self.check_args(&types)?;let mut vals=vec![];for i in args {vals.push(self.load(i));}while vals.len()<3 {vals.push("0".into());}
        let v=self.fresh();self.line(format!("{v} = call i128 @{name}(i128 {}, i128 {}, i128 {})",vals[0],vals[1],vals[2]));self.push_val(result,&v);self.simd.insert((op,imm));Ok(())
    }
    fn simd_memory(&mut self,op:u32)->Result<(),String> {
        if !self.env.memory {return Err("SIMD memory instruction without memory".into())}
        let laneop=(84..=91).contains(&op);
        let (width,align_max) = match op {0|11=>(16,4),1..=6=>(8,3),7=>(1,0),8=>(2,1),9=>(4,2),10=>(8,3),84..=91=>{let n=(op-84)%4;(1u32<<n,n)},92=>(4,2),93=>(8,3),_=>return Err("invalid SIMD memory instruction".into())};
        let align=self.r.u32()?;let offset=self.r.u32()?;if align>align_max{return Err("invalid SIMD memory alignment".into())}
        let lane=if laneop {let l=self.r.byte()?;if l as u32>=16/width{return Err("SIMD lane out of bounds".into())}Some(l)}else{None};
        let is_store=op==11||(88..=91).contains(&op);
        let vector=if is_store || laneop {Some(self.get_pop(V128)?)}else{None};let addr=self.get_pop(I32)?;let addr=self.i32(&addr);
        let p=self.fresh();self.line(format!("{p} = call ptr @w2l_mem_ptr(i32 {addr}, i64 {offset}, i64 {width})"));
        if op==11 {self.line(format!("store i128 {}, ptr {p}, align 1",vector.unwrap()));return Ok(())}
        if (88..=91).contains(&op) {let n=16/width;let bits=width*8;let vt=format!("<{n} x i{bits}>");let v=self.fresh();let x=self.fresh();self.line(format!("{v} = bitcast i128 {} to {vt}",vector.unwrap()));self.line(format!("{x} = extractelement {vt} {v}, i32 {}",lane.unwrap()));self.line(format!("store i{bits} {x}, ptr {p}, align 1"));return Ok(())}
        let raw=self.fresh();let it=format!("i{}",width*8);self.line(format!("{raw} = load {it}, ptr {p}, align 1"));
        let result=match op {
            0=>raw,
            1..=6=>{let (lanes,bits,signed)=match op {1=>(8,8,true),2=>(8,8,false),3=>(4,16,true),4=>(4,16,false),5=>(2,32,true),_=>(2,32,false)};let from=format!("<{lanes} x i{bits}>");let to=format!("<{lanes} x i{}>",bits*2);let a=self.fresh();let b=self.fresh();let c=self.fresh();let ext=if signed {"sext"}else{"zext"};self.line(format!("{a} = bitcast i64 {raw} to {from}"));self.line(format!("{b} = {ext} {from} {a} to {to}"));self.line(format!("{c} = bitcast {to} {b} to i128"));c},
            7..=10=>{let lanes=16/width;let vt=format!("<{lanes} x i{}>",width*8);let a=self.fresh();let b=self.fresh();let c=self.fresh();self.line(format!("{a} = insertelement {vt} zeroinitializer, {it} {raw}, i32 0"));self.line(format!("{b} = shufflevector {vt} {a}, {vt} zeroinitializer, <{lanes} x i32> zeroinitializer"));self.line(format!("{c} = bitcast {vt} {b} to i128"));c},
            84..=87=>{let lanes=16/width;let vt=format!("<{lanes} x i{}>",width*8);let a=self.fresh();let b=self.fresh();let c=self.fresh();self.line(format!("{a} = bitcast i128 {} to {vt}",vector.unwrap()));self.line(format!("{b} = insertelement {vt} {a}, {it} {raw}, i32 {}",lane.unwrap()));self.line(format!("{c} = bitcast {vt} {b} to i128"));c},
            92|93=>self.widen(&raw,&it),_=>unreachable!(),
        };self.push_val(V128,&result);Ok(())
    }
    fn bulk(&mut self)->Result<(),String> {
        let op=self.r.u32()?;
        if (15..=17).contains(&op) {return Err("reference types are outside the supported scope".into())}
        if op<=7 {return self.numeric(0x100+op)}
        match op {
            8=>{let data=self.r.u32()?;let mem=self.r.u32()?;if !self.env.memory||mem!=0||data as usize>=self.env.data_count{return Err("invalid memory.init index".into())}let n=self.get_pop(I32)?;let src=self.get_pop(I32)?;let dst=self.get_pop(I32)?;let n=self.i32(&n);let src=self.i32(&src);let dst=self.i32(&dst);self.line(format!("call void @w2l_data_init_{data}(i32 {dst}, i32 {src}, i32 {n})"));}
            9=>{let data=self.r.u32()?;if data as usize>=self.env.data_count{return Err("invalid data.drop index".into())}self.line(format!("store i1 true, ptr @w2l_data_dropped_{data}"));}
            10=>{let a=self.r.u32()?;let b=self.r.u32()?;if !self.env.memory||a!=0||b!=0{return Err("invalid memory.copy index".into())}let n=self.get_pop(I32)?;let src=self.get_pop(I32)?;let dst=self.get_pop(I32)?;let n=self.i32(&n);let src=self.i32(&src);let dst=self.i32(&dst);self.line(format!("call void @w2l_mem_copy(i32 {dst}, i32 {src}, i32 {n})"));}
            11=>{let mem=self.r.u32()?;if !self.env.memory||mem!=0{return Err("invalid memory.fill index".into())}let n=self.get_pop(I32)?;let byte=self.get_pop(I32)?;let dst=self.get_pop(I32)?;let n=self.i32(&n);let byte=self.i32(&byte);let dst=self.i32(&dst);self.line(format!("call void @w2l_mem_fill(i32 {dst}, i32 {byte}, i32 {n})"));}
            12=>{let e=self.r.u32()?;let t=self.r.u32()?;let et=self.env.elem_types.get(e as usize).ok_or("invalid element index")?;if self.env.tables.get(t as usize)!=Some(et){return Err("table.init type mismatch".into())}let n=self.get_pop(I32)?;let src=self.get_pop(I32)?;let dst=self.get_pop(I32)?;let n=self.i32(&n);let src=self.i32(&src);let dst=self.i32(&dst);let p=self.fresh();let q=self.fresh();let size=self.fresh();self.line(format!("{p} = call ptr @w2l_table_ptr_{t}(i32 {dst}, i32 {n})"));self.line(format!("{q} = call ptr @w2l_elem_ptr_{e}(i32 {src}, i32 {n})"));self.line(format!("{size} = zext i32 {n} to i64"));let bytes=self.fresh();self.line(format!("{bytes} = shl i64 {size}, 4"));self.line(format!("call void @llvm.memcpy.p0.p0.i64(ptr {p}, ptr {q}, i64 {bytes}, i1 false)"));}
            13=>{let e=self.r.u32()?;if e as usize>=self.env.elem_types.len(){return Err("invalid elem.drop index".into())}self.line(format!("store i1 true, ptr @w2l_elem_dropped_{e}"));}
            14=>{let a=self.r.u32()?;let b=self.r.u32()?;let ta=self.env.tables.get(a as usize).ok_or("invalid table index")?;if self.env.tables.get(b as usize)!=Some(ta){return Err("table.copy type mismatch".into())}let n=self.get_pop(I32)?;let src=self.get_pop(I32)?;let dst=self.get_pop(I32)?;let n=self.i32(&n);let src=self.i32(&src);let dst=self.i32(&dst);let p=self.fresh();let q=self.fresh();let size=self.fresh();let bytes=self.fresh();self.line(format!("{p} = call ptr @w2l_table_ptr_{a}(i32 {dst}, i32 {n})"));self.line(format!("{q} = call ptr @w2l_table_ptr_{b}(i32 {src}, i32 {n})"));self.line(format!("{size} = zext i32 {n} to i64"));self.line(format!("{bytes} = shl i64 {size}, 4"));self.line(format!("call void @llvm.memmove.p0.p0.i64(ptr {p}, ptr {q}, i64 {bytes}, i1 false)"));}
            15=>{let t=self.r.u32()?;let ty=*self.env.tables.get(t as usize).ok_or("invalid table index")?;let delta=self.get_pop(I32)?;let delta=self.i32(&delta);let val=self.get_pop(ty)?;let r=self.fresh();self.line(format!("{r} = call i32 @w2l_table_grow_{t}(i128 {val}, i32 {delta})"));let w=self.widen(&r,"i32");self.push_val(I32,&w);}
            16=>{let t=self.r.u32()?;if self.env.tables.get(t as usize).is_none(){return Err("invalid table index".into())}let s=self.fresh();let n=self.fresh();let w=self.fresh();self.line(format!("{s} = load i64, ptr @w2l_table_size_{t}"));self.line(format!("{n} = trunc i64 {s} to i32"));self.line(format!("{w} = zext i32 {n} to i128"));self.push_val(I32,&w);}
            17=>{let t=self.r.u32()?;let ty=*self.env.tables.get(t as usize).ok_or("invalid table index")?;let n=self.get_pop(I32)?;let n=self.i32(&n);let val=self.get_pop(ty)?;let start=self.get_pop(I32)?;let start=self.i32(&start);self.line(format!("call void @w2l_table_fill_{t}(i32 {start}, i128 {val}, i32 {n})"));}
            _=>return Err(format!("unsupported 0xfc instruction {op}")),
        }Ok(())
    }
    fn instruction(&mut self,op:u8)->Result<bool,String> {
        if matches!(op,0x1c|0x25|0x26|0xd0..=0xd2) {return Err("reference types are outside the supported scope".into())}
        match op {
            0x00=>{self.line("call void @w2l_trap(i32 0)");self.line("unreachable");self.dead();}
            0x01=>{},0x02=>self.start_control(Kind::Block)?,0x03=>self.start_control(Kind::Loop)?,0x04=>self.start_control(Kind::If)?,0x05=>self.else_arm()?,0x0b=>return self.end(),
            0x0c=>{let i=self.r.u32()?;self.do_branch(i,false)?},0x0d=>{let i=self.r.u32()?;self.do_branch(i,true)?},0x0e=>self.br_table()?,
            0x0f=>{let i=(self.frames.len()-1) as u32;self.do_branch(i,false)?},
            0x10=>{let i=self.r.u32()?;self.call(i,None)?},0x11=>{let i=self.r.u32()?;let t=self.r.u32()?;self.call(i,Some(t))?},
            0x1a=>{self.pop(None)?;},
            0x1b|0x1c=>{
                let ty=if op==0x1c {let n=self.r.u32()?;if n!=1{return Err("invalid typed select".into())}Some(value_type(self.r.byte()?)?)}else{None};
                let c=self.get_pop(I32)?;let c=self.i32(&c);let yt=if self.stack.len()>self.top().base { self.stack.last().copied().flatten() } else { None };let yidx=self.pop(ty)?;
                // If the second operand was popped from a polymorphic stack its type is unknown.
                let expected=ty.or(yt);let xidx=self.pop(expected)?;let x=self.load(xidx);let y=self.load(yidx);
                let typ=expected;if op==0x1b&&matches!(typ,Some(FuncRef|ExternRef)){return Err("untyped select on reference".into())}
                let p=self.fresh();let v=self.fresh();self.line(format!("{p} = icmp ne i32 {c}, 0"));self.line(format!("{v} = select i1 {p}, i128 {x}, i128 {y}"));let idx=self.push(typ);self.store(idx,&v);
            }
            0x20=>{let i=self.r.u32()?;self.local_get(i)?},0x21=>{let i=self.r.u32()?;self.local_set(i,false)?},0x22=>{let i=self.r.u32()?;self.local_set(i,true)?},0x23=>{let i=self.r.u32()?;self.global(i,false)?},0x24=>{let i=self.r.u32()?;self.global(i,true)?},
            0x25=>{let t=self.r.u32()?;let ty=*self.env.tables.get(t as usize).ok_or("invalid table index")?;let index=self.get_pop(I32)?;let index=self.i32(&index);let v=self.fresh();self.line(format!("{v} = call i128 @w2l_table_get_{t}(i32 {index})"));self.push_val(ty,&v);}
            0x26=>{let t=self.r.u32()?;let ty=*self.env.tables.get(t as usize).ok_or("invalid table index")?;let val=self.get_pop(ty)?;let index=self.get_pop(I32)?;let index=self.i32(&index);self.line(format!("call void @w2l_table_set_{t}(i32 {index}, i128 {val})"));}
            0x28..=0x3e=>self.memop(op)?,
            0x3f=>{let mem=self.r.u32()?;if mem!=0||!self.env.memory{return Err("invalid memory.size".into())}let v=self.fresh();let n=self.fresh();let i=self.fresh();self.line(format!("{v} = load i64, ptr @w2l_mem_bytes"));self.line(format!("{n} = lshr i64 {v}, 16"));self.line(format!("{i} = zext i64 {n} to i128"));self.push_val(I32,&i);}
            0x40=>{let mem=self.r.u32()?;if mem!=0||!self.env.memory{return Err("invalid memory.grow".into())}let d=self.get_pop(I32)?;let d=self.i32(&d);let r=self.fresh();self.line(format!("{r} = call i32 @w2l_mem_grow(i32 {d})"));let r=self.widen(&r,"i32");self.push_val(I32,&r);}
            0x41=>{let x=self.r.signed(32)? as u32;self.push_val(I32,&x.to_string());},0x42=>{let x=self.r.signed(64)? as u64;self.push_val(I64,&x.to_string());},
            0x43=>{let b=self.r.bytes(4)?;let x=u32::from_le_bytes(b.try_into().unwrap());self.push_val(F32,&x.to_string());},0x44=>{let b=self.r.bytes(8)?;let x=u64::from_le_bytes(b.try_into().unwrap());self.push_val(F64,&x.to_string());},
            0x45..=0xc4=>self.numeric(op as u32)?,
            0xd0=>{let ty=value_type(self.r.byte()?)?;if !matches!(ty,FuncRef|ExternRef){return Err("invalid ref.null type".into())}self.push_val(ty,"0");}
            0xd1=>{let ty=if self.stack.len()>self.top().base{self.stack.last().copied().flatten()}else{None};if let Some(t)=ty {if !matches!(t,FuncRef|ExternRef){return Err("ref.is_null expects a reference".into())}}let i=self.pop(None)?;let v=self.load(i);let p=self.fresh();let r=self.fresh();self.line(format!("{p} = icmp eq i128 {v}, 0"));self.line(format!("{r} = zext i1 {p} to i128"));self.push_val(I32,&r);}
            0xd2=>{let i=self.r.u32()?;if self.env.funcs.get(i as usize).is_none(){return Err("invalid ref.func index".into())}self.push_val(FuncRef,&(i as u64+1).to_string());}
            0xfc=>self.bulk()?,0xfd=>self.simd()?,
            _=>return Err(format!("unsupported opcode 0x{op:02x}")),
        }Ok(false)
    }
}

pub fn compile(index:u32, env:&Env, sig:&Sig, locals:&[crate::wasm::LocalDecl], code:&[u8])->Result<FunctionIR,String> {
    let mut end=u32::try_from(sig.params.len()).map_err(|_|"too many parameters")?;
    let mut local_runs=Vec::new();
    for decl in locals {end=end.checked_add(decl.count).ok_or("too many locals")?;if decl.count!=0 {local_runs.push((end,decl.ty));}}
    let frame=Frame{kind:Kind::Function,base:0,params:vec![],results:sig.results.clone(),target:"return".into(),end:"return".into(),otherwise:String::new(),has_else:false,unreachable:false};
    let mut c=Compiler{env,r:Reader::new(code),out:String::new(),stack:vec![],frames:vec![frame],params:sig.params.clone(),local_runs,locals_used:BTreeSet::new(),max_stack:0,max_args:0,max_results:0,next_id:0,numeric:BTreeSet::new(),simd:BTreeSet::new(),indirect:BTreeSet::new()};
    loop {let op=c.r.byte()?;if c.instruction(op)? {break}}
    if !c.r.done() {return Err("bytes after function end".into())}
    c.put_label("return");
    for i in 0..sig.results.len() {let v=c.load(i);let p=c.fresh();c.line(format!("{p} = getelementptr i128, ptr %results, i64 {i}"));c.line(format!("store i128 {v}, ptr {p}, align 1"));}
    c.line("%depth_after = load i32, ptr @w2l_depth");c.line("%depth_less = sub i32 %depth_after, 1");c.line("store i32 %depth_less, ptr @w2l_depth");c.line("ret void");
    let mut s=format!("define internal void @w2l_f{index}(ptr %args, ptr %results) {{\nentry:\n");
    writeln!(s,"  %depth_before = load i32, ptr @w2l_depth\n  %depth_next = add i32 %depth_before, 1\n  store i32 %depth_next, ptr @w2l_depth\n  %depth_exceeded = icmp ugt i32 %depth_next, 50000\n  br i1 %depth_exceeded, label %exhausted, label %body\nexhausted:\n  call void @w2l_trap(i32 8)\n  unreachable\nbody:").unwrap();
    for i in 0..c.max_stack {writeln!(s,"  %s{i} = alloca i128, align 16").unwrap();}
    for &i in &c.locals_used {writeln!(s,"  %l{i} = alloca i128, align 16").unwrap();}
    writeln!(s,"  %callargs = alloca [{} x i128], align 16\n  %callresults = alloca [{} x i128], align 16",c.max_args.max(1),c.max_results.max(1)).unwrap();
    for &i in &c.locals_used { if (i as usize)<sig.params.len() {writeln!(s,"  %p{i} = getelementptr i128, ptr %args, i64 {i}\n  %arg{i} = load i128, ptr %p{i}, align 1\n  store i128 %arg{i}, ptr %l{i}, align 16").unwrap();} else {writeln!(s,"  store i128 0, ptr %l{i}, align 16").unwrap();} }
    s.push_str(&c.out);s.push_str("}\n\n");
    Ok(FunctionIR{body:s,numeric:c.numeric,simd:c.simd,indirect:c.indirect})
}

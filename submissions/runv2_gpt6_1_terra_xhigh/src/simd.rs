//! Fixed-width WebAssembly SIMD lowered to poison-free LLVM vector operations.
use crate::wasm::ValType::{self, F32, F64, I32, I64, V128};
use std::fmt::Write;

#[derive(Clone, Copy)]
enum Inst {
    Shuffle, Swizzle, Splat(u32, ValType), Extract(u32, ValType, bool), Replace(u32, ValType),
    ICmp(u32, &'static str), FCmp(u32, &'static str),
    Not, Bit(&'static str), AndNot, Bitselect, Any,
    Abs(u32), Neg(u32), Popcnt, All(u32), Bitmask(u32),
    Narrow(u32, bool), Shift(u32, &'static str), IntBin(u32, &'static str),
    Sat(u32, &'static str), MinMax(u32, &'static str), Average(u32),
    Extend(u32, bool, bool), ExtMul(u32, bool, bool), Pairwise(u32, bool), Dot, Q15,
    FloatUnary(u32, &'static str), FloatBin(u32, &'static str), PMinMax(u32, bool),
    Demote, Promote, TruncSat(u32, bool), Convert(u32, bool),
}

// Width parameters are the output lane widths except for Extract and Replace.
fn decode(op: u32) -> Option<Inst> {
    use Inst::*;
    let i = match op {
        13 => Shuffle, 14 => Swizzle,
        15 => Splat(8,I32), 16 => Splat(16,I32), 17 => Splat(32,I32),
        18 => Splat(64,I64), 19 => Splat(32,F32), 20 => Splat(64,F64),
        21 => Extract(8,I32,true), 22 => Extract(8,I32,false), 23 => Replace(8,I32),
        24 => Extract(16,I32,true), 25 => Extract(16,I32,false), 26 => Replace(16,I32),
        27 => Extract(32,I32,false), 28 => Replace(32,I32),
        29 => Extract(64,I64,false), 30 => Replace(64,I64),
        31 => Extract(32,F32,false), 32 => Replace(32,F32),
        33 => Extract(64,F64,false), 34 => Replace(64,F64),
        35..=64 => {
            let n = op - 35;
            ICmp([8,16,32][(n/10) as usize], ["eq","ne","slt","ult","sgt","ugt","sle","ule","sge","uge"][(n%10) as usize])
        }
        65..=76 => {
            let n=op-65;
            FCmp(if n<6 {32} else {64}, ["oeq","une","olt","ogt","ole","oge"][(n%6) as usize])
        }
        77 => Not, 78 => Bit("and"), 79 => AndNot, 80 => Bit("or"), 81 => Bit("xor"), 82 => Bitselect, 83 => Any,
        94 => Demote, 95 => Promote,
        96|128|160|192 => Abs(8 << ((op-96)/32)),
        97|129|161|193 => Neg(8 << ((op-97)/32)),
        98 => Popcnt,
        99|131|163|195 => All(8 << ((op-99)/32)),
        100|132|164|196 => Bitmask(8 << ((op-100)/32)),
        101|102 => Narrow(8,op==102), 133|134 => Narrow(16,op==134),
        103..=106 => FloatUnary(32,["ceil","floor","trunc","roundeven"][(op-103) as usize]),
        116 => FloatUnary(64,"ceil"), 117 => FloatUnary(64,"floor"), 122 => FloatUnary(64,"trunc"), 148 => FloatUnary(64,"roundeven"),
        107..=109|139..=141|171..=173|203..=205 => {
            let (base,w)=if op<139 {(107,8)} else if op<171 {(139,16)} else if op<203 {(171,32)} else {(203,64)};
            Shift(w,["shl","ashr","lshr"][(op-base) as usize])
        }
        110|142|174|206 => IntBin(8 << ((op-110)/32),"add"),
        113|145|177|209 => IntBin(8 << ((op-113)/32),"sub"),
        149|181|213 => IntBin(16 << ((op-149)/32),"mul"),
        111|143 => Sat(if op==111 {8}else{16},"sadd"),
        112|144 => Sat(if op==112 {8}else{16},"uadd"),
        114|146 => Sat(if op==114 {8}else{16},"ssub"),
        115|147 => Sat(if op==115 {8}else{16},"usub"),
        118..=121|150..=153|182..=185 => {
            let (base,w)=if op<150 {(118,8)} else if op<182 {(150,16)} else {(182,32)};
            MinMax(w,["smin","umin","smax","umax"][(op-base) as usize])
        }
        123 => Average(8), 155 => Average(16),
        124..=127 => Pairwise(if op<126 {16}else{32}, op%2==1),
        130 => Q15,
        135..=138|167..=170|199..=202 => {
            let (base,w)=if op<167 {(135,16)} else if op<199 {(167,32)} else {(199,64)};
            let k=op-base; Extend(w,k%2==1,k>=2)
        }
        156..=159|188..=191|220..=223 => {
            let (base,w)=if op<188 {(156,16)} else if op<220 {(188,32)} else {(220,64)};
            let k=op-base; ExtMul(w,k%2==1,k>=2)
        }
        186 => Dot,
        214..=219 => ICmp(64,["eq","ne","slt","sgt","sle","sge"][(op-214) as usize]),
        224|236 => FloatUnary(if op==224 {32}else{64},"fabs"),
        225|237 => FloatUnary(if op==225 {32}else{64},"neg"),
        227|239 => FloatUnary(if op==227 {32}else{64},"sqrt"),
        228..=233|240..=245 => {
            let (base,w)=if op<240 {(228,32)}else{(240,64)};
            FloatBin(w,["fadd","fsub","fmul","fdiv","minimum","maximum"][(op-base) as usize])
        }
        234|235|246|247 => PMinMax(if op<240 {32}else{64}, op%2==1),
        248|249 => TruncSat(32,op==249), 252|253 => TruncSat(64,op==253),
        250|251 => Convert(32,op==251), 254|255 => Convert(64,op==255),
        _ => return None,
    };
    Some(i)
}

pub fn signature(op: u32) -> Option<(Vec<ValType>, ValType)> {
    use Inst::*;
    let i=decode(op)?;
    let (args,ret) = match i {
        Splat(_,t) => (vec![t],V128), Extract(_,t,_) => (vec![V128],t),
        Replace(_,t) => (vec![V128,t],V128),
        Any|All(_)|Bitmask(_) => (vec![V128],I32),
        Shift(..) => (vec![V128,I32],V128),
        Bitselect => (vec![V128,V128,V128],V128),
        Shuffle|Swizzle|ICmp(..)|FCmp(..)|Bit(_)|AndNot|Narrow(..)|IntBin(..)|Sat(..)|MinMax(..)|Average(_)|ExtMul(..)|Dot|Q15|FloatBin(..)|PMinMax(..) => (vec![V128,V128],V128),
        _ => (vec![V128],V128),
    };
    Some((args,ret))
}

pub fn immediate_len(op: u32) -> usize {
    match op { 13 => 16, 21..=34 => 1, _ => 0 }
}

fn int_vec(w:u32) -> String { format!("<{} x i{w}>",128/w) }
fn float_vec(w:u32) -> String { if w==32 {"<4 x float>".into()} else {"<2 x double>".into()} }
fn suff(w:u32) -> String { format!("v{}i{w}",128/w) }
fn fsuff(w:u32) -> &'static str { if w==32 {"v4f32"} else {"v2f64"} }
fn ints(n:u32, w:u32, value:i64) -> String {
    format!("<{}>", (0..n).map(|_|format!("i{w} {value}")).collect::<Vec<_>>().join(", "))
}
fn mask(indices: impl IntoIterator<Item=u32>) -> String {
    format!("<{}>",indices.into_iter().map(|i|format!("i32 {i}")).collect::<Vec<_>>().join(", "))
}
fn convert_input(s:&mut String, w:u32, float:bool, binary:bool) -> String {
    let ty=if float {float_vec(w)} else {int_vec(w)};
    writeln!(s,"  %x = bitcast i128 %a to {ty}").unwrap();
    if binary { writeln!(s,"  %y = bitcast i128 %b to {ty}").unwrap(); }
    ty
}
fn vector_return(s:&mut String, ty:&str, name:&str) {
    writeln!(s,"  %result = bitcast {ty} {name} to i128\n  ret i128 %result").unwrap();
}
fn extend_half(s:&mut String, w:u32, high:bool, unsigned:bool, binary:bool) -> String {
    let n=128/w; let small=w/2; let full=int_vec(small); let half=format!("<{n} x i{small}>"); let out=int_vec(w);
    let m=mask((0..n).map(|i|i+if high {n}else{0})); let conv=if unsigned {"zext"}else{"sext"};
    for (slot,v) in if binary {vec![("a","x"),("b","y")]} else {vec![("a","x")]} {
        writeln!(s,"  %{v}0 = bitcast i128 %{slot} to {full}\n  %{v}1 = shufflevector {full} %{v}0, {full} zeroinitializer, <{n} x i32> {m}\n  %{v} = {conv} {half} %{v}1 to {out}").unwrap();
    }
    out
}

/// Intrinsics used by the helpers, independent of the set of opcodes emitted.
pub fn declarations() -> String {
    let mut s=String::new();
    for w in [8,16,32,64] {
        let ty=int_vec(w); let suf=suff(w);
        writeln!(s,"declare {ty} @llvm.abs.{suf}({ty}, i1)").unwrap();
        for f in ["smin","umin","smax","umax"] { writeln!(s,"declare {ty} @llvm.{f}.{suf}({ty}, {ty})").unwrap(); }
        if w<=16 { for f in ["sadd","uadd","ssub","usub"] { writeln!(s,"declare {ty} @llvm.{f}.sat.{suf}({ty}, {ty})").unwrap(); } }
    }
    writeln!(s,"declare <16 x i8> @llvm.ctpop.v16i8(<16 x i8>)").unwrap();
    for w in [32,64] {
        let ty=float_vec(w); let suf=fsuff(w);
        for f in ["fabs","ceil","floor","trunc","roundeven","sqrt"] { writeln!(s,"declare {ty} @llvm.{f}.{suf}({ty})").unwrap(); }
        for f in ["minimum","maximum"] { writeln!(s,"declare {ty} @llvm.{f}.{suf}({ty}, {ty})").unwrap(); }
        let dst=if w==32 {"<4 x i32>"}else{"<2 x i32>"}; let ds=if w==32 {"v4i32"}else{"v2i32"};
        for f in ["fptosi","fptoui"] { writeln!(s,"declare {dst} @llvm.{f}.sat.{ds}.{suf}({ty})").unwrap(); }
    }
    s
}

/// Return a helper consuming and producing raw 16-byte slot bit patterns.
pub fn helper(op:u32, imm:&[u8], name:&str) -> Result<String,String> {
    use Inst::*;
    let inst=decode(op).ok_or_else(||format!("unsupported SIMD opcode {op}"))?;
    if imm.len()!=immediate_len(op) { return Err(format!("invalid SIMD immediate length for opcode {op}")); }
    match inst {
        Shuffle if imm.iter().any(|&i|i>=32) => return Err("shuffle lane out of range".into()),
        Extract(w,..)|Replace(w,..) if imm[0] as u32>=128/w => return Err("SIMD lane out of range".into()),
        _=>{}
    }
    let mut s=format!("define internal i128 @{name}(i128 %a, i128 %b, i128 %c) {{\nentry:\n");
    match inst {
        Shuffle => {
            let ty=convert_input(&mut s,8,false,true); let m=mask(imm.iter().map(|&x|x as u32));
            writeln!(s,"  %r = shufflevector {ty} %x, {ty} %y, <16 x i32> {m}").unwrap(); vector_return(&mut s,&ty,"%r");
        }
        Swizzle => {
            let ty=convert_input(&mut s,8,false,true);
            // Compare all indices in parallel. Unlike dynamic extractelement, no
            // out-of-range index can produce poison; unmatched lanes stay zero.
            for i in 0..16 {
                let m=mask(std::iter::repeat_n(i,16)); let c=ints(16,8,i as i64);
                let prev=if i==0 {"zeroinitializer".to_string()} else {format!("%r{}",i-1)};
                writeln!(s,"  %s{i} = shufflevector {ty} %x, {ty} zeroinitializer, <16 x i32> {m}\n  %p{i} = icmp eq {ty} %y, {c}\n  %r{i} = select <16 x i1> %p{i}, {ty} %s{i}, {ty} {prev}").unwrap();
            }
            vector_return(&mut s,&ty,"%r15");
        }
        Splat(w,_) => {
            let ty=int_vec(w); let n=128/w;
            writeln!(s,"  %scalar = trunc i128 %a to i{w}\n  %one = insertelement {ty} zeroinitializer, i{w} %scalar, i32 0\n  %r = shufflevector {ty} %one, {ty} zeroinitializer, <{n} x i32> zeroinitializer").unwrap(); vector_return(&mut s,&ty,"%r");
        }
        Extract(w,t,signed) => {
            let ty=convert_input(&mut s,w,false,false); let lane=imm[0];
            writeln!(s,"  %lane = extractelement {ty} %x, i32 {lane}").unwrap();
            let outw=if matches!(t,I32|F32) {32}else{64};
            if w<outw { writeln!(s,"  %ext = {} i{w} %lane to i{outw}",if signed {"sext"}else{"zext"}).unwrap(); }
            writeln!(s,"  %result = zext i{outw} {} to i128\n  ret i128 %result",if w<outw {"%ext"}else{"%lane"}).unwrap();
        }
        Replace(w,_) => {
            let ty=convert_input(&mut s,w,false,false); let lane=imm[0];
            writeln!(s,"  %scalar = trunc i128 %b to i{w}\n  %r = insertelement {ty} %x, i{w} %scalar, i32 {lane}").unwrap(); vector_return(&mut s,&ty,"%r");
        }
        ICmp(w,pred)|FCmp(w,pred) => {
            let float=matches!(inst,FCmp(..)); let ty=convert_input(&mut s,w,float,true); let n=128/w; let it=int_vec(w);
            writeln!(s,"  %cmp = {} {pred} {ty} %x, %y\n  %r = sext <{n} x i1> %cmp to {it}",if float {"fcmp"}else{"icmp"}).unwrap(); vector_return(&mut s,&it,"%r");
        }
        Not => { writeln!(s,"  %r = xor i128 %a, -1\n  ret i128 %r").unwrap(); }
        Bit(f) => { writeln!(s,"  %r = {f} i128 %a, %b\n  ret i128 %r").unwrap(); }
        AndNot => { writeln!(s,"  %n = xor i128 %b, -1\n  %r = and i128 %a, %n\n  ret i128 %r").unwrap(); }
        Bitselect => { writeln!(s,"  %n = xor i128 %c, -1\n  %l = and i128 %a, %c\n  %h = and i128 %b, %n\n  %r = or i128 %l, %h\n  ret i128 %r").unwrap(); }
        Any => { writeln!(s,"  %p = icmp ne i128 %a, 0\n  %r = zext i1 %p to i128\n  ret i128 %r").unwrap(); }
        Abs(w) => { let ty=convert_input(&mut s,w,false,false); let suf=suff(w); writeln!(s,"  %r = call {ty} @llvm.abs.{suf}({ty} %x, i1 false)").unwrap(); vector_return(&mut s,&ty,"%r"); }
        Neg(w) => { let ty=convert_input(&mut s,w,false,false); writeln!(s,"  %r = sub {ty} zeroinitializer, %x").unwrap(); vector_return(&mut s,&ty,"%r"); }
        Popcnt => { let ty=convert_input(&mut s,8,false,false); writeln!(s,"  %r = call {ty} @llvm.ctpop.v16i8({ty} %x)").unwrap(); vector_return(&mut s,&ty,"%r"); }
        All(w)|Bitmask(w) => {
            let ty=convert_input(&mut s,w,false,false); let n=128/w;
            if matches!(inst,All(_)) {
                writeln!(s,"  %p = icmp eq {ty} %x, zeroinitializer\n  %bits = bitcast <{n} x i1> %p to i{n}\n  %all = icmp eq i{n} %bits, 0\n  %r = zext i1 %all to i128\n  ret i128 %r").unwrap();
            } else {
                writeln!(s,"  %p = icmp slt {ty} %x, zeroinitializer\n  %bits = bitcast <{n} x i1> %p to i{n}\n  %r = zext i{n} %bits to i128\n  ret i128 %r").unwrap();
            }
        }
        Narrow(w,unsigned) => {
            let n=64/w; let wide=2*w; let src=int_vec(wide); let all=format!("<{} x i{wide}>",2*n); let out=int_vec(w);
            let lo=if unsigned {0} else {-(1i64<<(w-1))}; let hi=if unsigned {(1i64<<w)-1} else {(1i64<<(w-1))-1};
            let m=mask(0..2*n); let low=ints(2*n,wide,lo); let high=ints(2*n,wide,hi);
            writeln!(s,"  %x = bitcast i128 %a to {src}\n  %y = bitcast i128 %b to {src}\n  %both = shufflevector {src} %x, {src} %y, <{} x i32> {m}\n  %below = icmp slt {all} %both, {low}\n  %clo = select <{} x i1> %below, {all} {low}, {all} %both\n  %above = icmp sgt {all} %clo, {high}\n  %chi = select <{} x i1> %above, {all} {high}, {all} %clo\n  %r = trunc {all} %chi to {out}",2*n,2*n,2*n).unwrap(); vector_return(&mut s,&out,"%r");
        }
        Shift(w,f) => {
            let ty=convert_input(&mut s,w,false,false); let n=128/w;
            writeln!(s,"  %b32 = trunc i128 %b to i32\n  %count32 = and i32 %b32, {}",w-1).unwrap();
            if w<32 {writeln!(s,"  %count = trunc i32 %count32 to i{w}").unwrap();}
            else if w>32 {writeln!(s,"  %count = zext i32 %count32 to i{w}").unwrap();}
            writeln!(s,"  %one = insertelement {ty} zeroinitializer, i{w} {}, i32 0\n  %counts = shufflevector {ty} %one, {ty} zeroinitializer, <{n} x i32> zeroinitializer\n  %r = {f} {ty} %x, %counts",if w==32 {"%count32"}else{"%count"}).unwrap(); vector_return(&mut s,&ty,"%r");
        }
        IntBin(w,f) => { let ty=convert_input(&mut s,w,false,true); writeln!(s,"  %r = {f} {ty} %x, %y").unwrap(); vector_return(&mut s,&ty,"%r"); }
        Sat(w,f) => { let ty=convert_input(&mut s,w,false,true); let suf=suff(w); writeln!(s,"  %r = call {ty} @llvm.{f}.sat.{suf}({ty} %x, {ty} %y)").unwrap(); vector_return(&mut s,&ty,"%r"); }
        MinMax(w,f) => { let ty=convert_input(&mut s,w,false,true); let suf=suff(w); writeln!(s,"  %r = call {ty} @llvm.{f}.{suf}({ty} %x, {ty} %y)").unwrap(); vector_return(&mut s,&ty,"%r"); }
        Average(w) => {
            let ty=convert_input(&mut s,w,false,true); let n=128/w; let wt=format!("<{n} x i{}>",2*w); let ones=ints(n,2*w,1);
            writeln!(s,"  %wx = zext {ty} %x to {wt}\n  %wy = zext {ty} %y to {wt}\n  %sum = add {wt} %wx, %wy\n  %rounded = add {wt} %sum, {ones}\n  %half = lshr {wt} %rounded, {ones}\n  %r = trunc {wt} %half to {ty}").unwrap(); vector_return(&mut s,&ty,"%r");
        }
        Extend(w,high,unsigned)|ExtMul(w,high,unsigned) => {
            let binary=matches!(inst,ExtMul(..)); let ty=extend_half(&mut s,w,high,unsigned,binary);
            if binary {writeln!(s,"  %r = mul {ty} %x, %y").unwrap(); vector_return(&mut s,&ty,"%r");}
            else {vector_return(&mut s,&ty,"%x");}
        }
        Pairwise(w,unsigned) => {
            let small=w/2; let n=128/w; let src=int_vec(small); let wide=format!("<{} x i{w}>",2*n); let out=int_vec(w); let me=mask((0..n).map(|i|2*i)); let mo=mask((0..n).map(|i|2*i+1));
            writeln!(s,"  %x = bitcast i128 %a to {src}\n  %wide = {} {src} %x to {wide}\n  %even = shufflevector {wide} %wide, {wide} zeroinitializer, <{n} x i32> {me}\n  %odd = shufflevector {wide} %wide, {wide} zeroinitializer, <{n} x i32> {mo}\n  %r = add {out} %even, %odd",if unsigned {"zext"}else{"sext"}).unwrap(); vector_return(&mut s,&out,"%r");
        }
        Dot => {
            writeln!(s,"  %x = bitcast i128 %a to <8 x i16>\n  %y = bitcast i128 %b to <8 x i16>\n  %xx = sext <8 x i16> %x to <8 x i32>\n  %yy = sext <8 x i16> %y to <8 x i32>\n  %m = mul <8 x i32> %xx, %yy\n  %even = shufflevector <8 x i32> %m, <8 x i32> zeroinitializer, <4 x i32> <i32 0, i32 2, i32 4, i32 6>\n  %odd = shufflevector <8 x i32> %m, <8 x i32> zeroinitializer, <4 x i32> <i32 1, i32 3, i32 5, i32 7>\n  %r = add <4 x i32> %even, %odd").unwrap(); vector_return(&mut s,"<4 x i32>","%r");
        }
        Q15 => {
            let ty=convert_input(&mut s,16,false,true); let wide="<8 x i32>"; let bias=ints(8,32,16384); let shifts=ints(8,32,15); let max=ints(8,32,32767);
            writeln!(s,"  %wx = sext {ty} %x to {wide}\n  %wy = sext {ty} %y to {wide}\n  %mul = mul {wide} %wx, %wy\n  %sum = add {wide} %mul, {bias}\n  %shifted = ashr {wide} %sum, {shifts}\n  %high = icmp sgt {wide} %shifted, {max}\n  %clamped = select <8 x i1> %high, {wide} {max}, {wide} %shifted\n  %r = trunc {wide} %clamped to {ty}").unwrap(); vector_return(&mut s,&ty,"%r");
        }
        FloatUnary(w,f) => { let ty=convert_input(&mut s,w,true,false); if f=="neg" {writeln!(s,"  %r = fneg {ty} %x").unwrap();} else {let suf=fsuff(w); writeln!(s,"  %r = call {ty} @llvm.{f}.{suf}({ty} %x)").unwrap();} vector_return(&mut s,&ty,"%r"); }
        FloatBin(w,f) => { let ty=convert_input(&mut s,w,true,true); if f.starts_with('f') {writeln!(s,"  %r = {f} {ty} %x, %y").unwrap();} else {let suf=fsuff(w); writeln!(s,"  %r = call {ty} @llvm.{f}.{suf}({ty} %x, {ty} %y)").unwrap();} vector_return(&mut s,&ty,"%r"); }
        PMinMax(w,max) => { let ty=convert_input(&mut s,w,true,true); let n=128/w; let (a,b)=if max {("%x","%y")}else{("%y","%x")}; writeln!(s,"  %p = fcmp olt {ty} {a}, {b}\n  %r = select <{n} x i1> %p, {ty} %y, {ty} %x").unwrap(); vector_return(&mut s,&ty,"%r"); }
        Demote => { let ty=convert_input(&mut s,64,true,false); writeln!(s,"  %small = fptrunc {ty} %x to <2 x float>\n  %r = shufflevector <2 x float> %small, <2 x float> zeroinitializer, <4 x i32> <i32 0, i32 1, i32 2, i32 3>").unwrap(); vector_return(&mut s,"<4 x float>","%r"); }
        Promote => { let ty=convert_input(&mut s,32,true,false); writeln!(s,"  %low = shufflevector {ty} %x, {ty} zeroinitializer, <2 x i32> <i32 0, i32 1>\n  %r = fpext <2 x float> %low to <2 x double>").unwrap(); vector_return(&mut s,"<2 x double>","%r"); }
        TruncSat(w,unsigned) => {
            let ty=convert_input(&mut s,w,true,false); let suf=fsuff(w); let f=if unsigned {"fptoui"}else{"fptosi"}; let it=if w==32 {"<4 x i32>"}else{"<2 x i32>"}; let is=if w==32 {"v4i32"}else{"v2i32"};
            writeln!(s,"  %v = call {it} @llvm.{f}.sat.{is}.{suf}({ty} %x)").unwrap();
            if w==64 {writeln!(s,"  %r = shufflevector {it} %v, {it} zeroinitializer, <4 x i32> <i32 0, i32 1, i32 2, i32 3>").unwrap(); vector_return(&mut s,"<4 x i32>","%r");} else {vector_return(&mut s,it,"%v");}
        }
        Convert(w,unsigned) => {
            let ty=convert_input(&mut s,32,false,false); let conv=if unsigned {"uitofp"}else{"sitofp"};
            if w==32 {writeln!(s,"  %r = {conv} {ty} %x to <4 x float>").unwrap(); vector_return(&mut s,"<4 x float>","%r");}
            else {writeln!(s,"  %low = shufflevector {ty} %x, {ty} zeroinitializer, <2 x i32> <i32 0, i32 1>\n  %r = {conv} <2 x i32> %low to <2 x double>").unwrap(); vector_return(&mut s,"<2 x double>","%r");}
        }
    }
    s.push_str("}\n\n");
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_width_opcode_coverage_and_signatures() {
        let supported = (0..=255).filter(|&op| signature(op).is_some()).count();
        assert_eq!(supported, 213);
        for op in 0..=255 {
            if let Some((args, _)) = signature(op) {
                assert!((1..=3).contains(&args.len()), "opcode {op}");
                let imm = vec![0; immediate_len(op)];
                assert!(helper(op, &imm, "test").is_ok(), "opcode {op}");
            } else {
                assert!(helper(op, &[], "test").is_err(), "opcode {op}");
            }
        }
        assert!(helper(256, &[], "test").is_err());
        assert_eq!(signature(82), Some((vec![V128, V128, V128], V128)));
        assert_eq!(signature(172), Some((vec![V128, I32], V128)));
        assert_eq!(signature(31), Some((vec![V128], F32)));
        assert_eq!(signature(34), Some((vec![V128, F64], V128)));
    }

    #[test]
    fn validate_lane_and_shuffle_immediates() {
        for (lo, hi, count) in [(21, 23, 16), (24, 26, 8), (27, 28, 4),
                                (29, 30, 2), (31, 32, 4), (33, 34, 2)] {
            for op in lo..=hi {
                assert!(helper(op, &[0], "test").is_ok());
                assert!(helper(op, &[count - 1], "test").is_ok());
                assert!(helper(op, &[count], "test").is_err());
                assert!(helper(op, &[], "test").is_err());
                assert!(helper(op, &[0, 0], "test").is_err());
            }
        }
        assert!(helper(13, &[31; 16], "test").is_ok());
        assert!(helper(13, &[32; 16], "test").is_err());
        assert!(helper(13, &[0; 15], "test").is_err());
        assert!(helper(13, &[0; 17], "test").is_err());
        assert!(helper(77, &[0], "test").is_err());
    }
}

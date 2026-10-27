//! Complete static validation of the supported numeric WebAssembly instruction set.
use crate::model::*;
use std::collections::HashSet;
use Ty::*;
type Result<T> = std::result::Result<T, String>;
type Signature = (&'static [Ty], &'static [Ty]);

/// SIMD 1.0 only; holes in its opcode space and relaxed SIMD are not instructions.
pub fn simd_signature(sub: u32) -> Option<Signature> {
    let sig: Signature = match sub {
        0x00..=0x0a | 0x5c..=0x5d => (&[I32], &[V128]),
        0x0b | 0x58..=0x5b => (&[I32, V128], &[]),
        0x0c => (&[], &[V128]),
        0x0f..=0x11 => (&[I32], &[V128]),
        0x12 => (&[I64], &[V128]),
        0x13 => (&[F32], &[V128]),
        0x14 => (&[F64], &[V128]),
        0x15..=0x16 | 0x18..=0x19 | 0x1b => (&[V128], &[I32]),
        0x17 | 0x1a | 0x1c => (&[V128, I32], &[V128]),
        0x1d => (&[V128], &[I64]),
        0x1e => (&[V128, I64], &[V128]),
        0x1f => (&[V128], &[F32]),
        0x20 => (&[V128, F32], &[V128]),
        0x21 => (&[V128], &[F64]),
        0x22 => (&[V128, F64], &[V128]),
        0x52 => (&[V128, V128, V128], &[V128]),
        0x53 | 0x63..=0x64 | 0x83..=0x84 | 0xa3..=0xa4 | 0xc3..=0xc4 => (&[V128], &[I32]),
        0x54..=0x57 => (&[I32, V128], &[V128]),
        0x6b..=0x6d | 0x8b..=0x8d | 0xab..=0xad | 0xcb..=0xcd => (&[V128, I32], &[V128]),
        0x4d
        | 0x5e..=0x62
        | 0x67..=0x6a
        | 0x74..=0x75
        | 0x7a
        | 0x7c..=0x81
        | 0x87..=0x8a
        | 0x94
        | 0xa0..=0xa1
        | 0xa7..=0xaa
        | 0xc0..=0xc1
        | 0xc7..=0xca
        | 0xe0..=0xe1
        | 0xe3
        | 0xec..=0xed
        | 0xef
        | 0xf8..=0xff => (&[V128], &[V128]),
        0x0d..=0x0e
        | 0x23..=0x4c
        | 0x4e..=0x51
        | 0x65..=0x66
        | 0x6e..=0x73
        | 0x76..=0x79
        | 0x7b
        | 0x82
        | 0x85..=0x86
        | 0x8e..=0x93
        | 0x95..=0x99
        | 0x9b..=0x9f
        | 0xae
        | 0xb1
        | 0xb5..=0xba
        | 0xbc..=0xbf
        | 0xce
        | 0xd1
        | 0xd5..=0xdf
        | 0xe4..=0xeb
        | 0xf0..=0xf7 => (&[V128, V128], &[V128]),
        _ => return None,
    };
    Some(sig)
}

fn numeric_signature(op: u32) -> Option<Signature> {
    Some(match op {
        0x41 => (&[], &[I32]),
        0x42 => (&[], &[I64]),
        0x43 => (&[], &[F32]),
        0x44 => (&[], &[F64]),
        0x45 => (&[I32], &[I32]),
        0x46..=0x4f => (&[I32, I32], &[I32]),
        0x50 => (&[I64], &[I32]),
        0x51..=0x5a => (&[I64, I64], &[I32]),
        0x5b..=0x60 => (&[F32, F32], &[I32]),
        0x61..=0x66 => (&[F64, F64], &[I32]),
        0x67..=0x69 | 0xc0..=0xc1 => (&[I32], &[I32]),
        0x6a..=0x78 => (&[I32, I32], &[I32]),
        0x79..=0x7b | 0xc2..=0xc4 => (&[I64], &[I64]),
        0x7c..=0x8a => (&[I64, I64], &[I64]),
        0x8b..=0x91 => (&[F32], &[F32]),
        0x92..=0x98 => (&[F32, F32], &[F32]),
        0x99..=0x9f => (&[F64], &[F64]),
        0xa0..=0xa6 => (&[F64, F64], &[F64]),
        0xa7 => (&[I64], &[I32]),
        0xa8..=0xa9 | 0xfc00..=0xfc01 => (&[F32], &[I32]),
        0xaa..=0xab | 0xfc02..=0xfc03 => (&[F64], &[I32]),
        0xac..=0xad => (&[I32], &[I64]),
        0xae..=0xaf | 0xfc04..=0xfc05 => (&[F32], &[I64]),
        0xb0..=0xb1 | 0xfc06..=0xfc07 => (&[F64], &[I64]),
        0xb2..=0xb3 => (&[I32], &[F32]),
        0xb4..=0xb5 => (&[I64], &[F32]),
        0xb6 => (&[F64], &[F32]),
        0xb7..=0xb8 => (&[I32], &[F64]),
        0xb9..=0xba => (&[I64], &[F64]),
        0xbb => (&[F32], &[F64]),
        0xbc => (&[F32], &[I32]),
        0xbd => (&[F64], &[I64]),
        0xbe => (&[I32], &[F32]),
        0xbf => (&[I64], &[F64]),
        _ => return None,
    })
}

fn block_type(m: &Module, imm: i64) -> Result<FuncType> {
    let result = match imm {
        -64 => None,
        -1 => Some(I32),
        -2 => Some(I64),
        -3 => Some(F32),
        -4 => Some(F64),
        -5 => Some(V128),
        n if n >= 0 => {
            return m
                .types
                .get(n as usize)
                .cloned()
                .ok_or_else(|| "unknown block type index".into())
        }
        _ => return Err("invalid or unsupported block type".into()),
    };
    Ok(FuncType {
        params: vec![],
        results: result.into_iter().collect(),
    })
}

fn limits(l: &Limits, memory: bool) -> Result<()> {
    if l.max.is_some_and(|max| max < l.min)
        || (memory && (l.min > 65536 || l.max.is_some_and(|max| max > 65536)))
    {
        return Err("invalid limits".into());
    }
    Ok(())
}

fn constant(code: &[Instr], ty: Ty) -> Result<()> {
    if code.len() != 2 || code[1].op != 0x0b {
        return Err("constant expression must contain exactly one constant and end".into());
    }
    let actual = match code[0].op {
        0x41 => I32,
        0x42 => I64,
        0x43 => F32,
        0x44 => F64,
        0xfd0c => V128,
        // Imported globals cannot be represented by the ABI, so global.get is never constant here.
        _ => {
            return Err(
                "invalid constant expression (extended const/reference operations unsupported)"
                    .into(),
            )
        }
    };
    if actual != ty {
        return Err("constant expression type mismatch".into());
    }
    Ok(())
}

/// Validate every index, initializer, declaration, and instruction, including dead code.
pub fn validate(m: &Module) -> Result<()> {
    if let Some(l) = &m.memory {
        limits(l, true)?;
    }
    if let Some(l) = &m.table {
        limits(l, false)?;
    }
    if let Some(n) = m.data_count {
        if n as usize != m.data.len() {
            return Err("data count does not match data section".into());
        }
    }
    let mut saw_defined = false;
    for f in &m.funcs {
        if f.type_idx as usize >= m.types.len() {
            return Err("unknown function type index".into());
        }
        if f.import.is_some() {
            if saw_defined || !f.locals.is_empty() || !f.code.is_empty() {
                return Err("invalid imported function representation".into());
            }
        } else {
            saw_defined = true;
        }
        if m.types[f.type_idx as usize]
            .params
            .len()
            .checked_add(f.locals.len())
            .is_none_or(|n| n > u32::MAX as usize)
        {
            return Err("too many function locals".into());
        }
    }
    for g in &m.globals {
        constant(&g.init, g.ty)?;
    }
    let mut names = HashSet::new();
    for e in &m.exports {
        if !names.insert(&e.name) {
            return Err("duplicate export name".into());
        }
        let count = match e.kind {
            0 => m.funcs.len(),
            1 => usize::from(m.table.is_some()),
            2 => usize::from(m.memory.is_some()),
            3 => m.globals.len(),
            _ => return Err("invalid export kind".into()),
        };
        if e.index as usize >= count {
            return Err("unknown export index".into());
        }
    }
    if let Some(index) = m.start {
        let f = m
            .funcs
            .get(index as usize)
            .ok_or("unknown start function index")?;
        let ty = &m.types[f.type_idx as usize];
        if !ty.params.is_empty() || !ty.results.is_empty() {
            return Err("start function must have type [] -> []".into());
        }
    }
    for e in &m.elems {
        if e.passive && e.declarative {
            return Err("invalid element segment mode".into());
        }
        if !e.passive && !e.declarative {
            if m.table.is_none() {
                return Err("active element segment requires a table".into());
            }
            constant(&e.offset, I32)?;
        } else if !e.offset.is_empty() {
            return Err("passive/declarative element segment has an offset".into());
        }
        for &f in &e.funcs {
            if f as usize >= m.funcs.len() {
                return Err("unknown function in element segment".into());
            }
        }
    }
    for d in &m.data {
        if !d.passive {
            if m.memory.is_none() {
                return Err("active data segment requires a memory".into());
            }
            constant(&d.offset, I32)?;
        } else if !d.offset.is_empty() {
            return Err("passive data segment has an offset".into());
        }
    }
    for (index, f) in m.funcs.iter().enumerate() {
        if f.import.is_none() {
            validate_function(m, f).map_err(|e| format!("function {index}: {e}"))?;
        }
    }
    Ok(())
}

struct Frame {
    kind: u32,
    start: Vec<Ty>,
    end: Vec<Ty>,
    height: usize,
    unreachable: bool,
}
struct Stack {
    vals: Vec<Option<Ty>>,
    ctrls: Vec<Frame>,
}
impl Stack {
    fn pop(&mut self, expected: Option<Ty>) -> Result<Option<Ty>> {
        let c = self.ctrls.last().ok_or("instruction after function end")?;
        if self.vals.len() == c.height && c.unreachable {
            return Ok(None);
        }
        if self.vals.len() <= c.height {
            return Err("operand stack underflow".into());
        }
        let actual = self.vals.pop().unwrap();
        if actual.is_some() && expected.is_some() && actual != expected {
            return Err("operand type mismatch".into());
        }
        Ok(actual)
    }
    fn pop_types(&mut self, types: &[Ty]) -> Result<()> {
        for &ty in types.iter().rev() {
            self.pop(Some(ty))?;
        }
        Ok(())
    }
    fn push_types(&mut self, types: &[Ty]) {
        self.vals.extend(types.iter().copied().map(Some));
    }
    fn effect(&mut self, inputs: &[Ty], outputs: &[Ty]) -> Result<()> {
        self.pop_types(inputs)?;
        self.push_types(outputs);
        Ok(())
    }
    fn dead(&mut self) {
        let c = self.ctrls.last_mut().unwrap();
        self.vals.truncate(c.height);
        c.unreachable = true;
    }
    fn label(&self, index: u32) -> Result<Vec<Ty>> {
        let c = self
            .ctrls
            .len()
            .checked_sub(index as usize + 1)
            .and_then(|i| self.ctrls.get(i))
            .ok_or("unknown branch label")?;
        Ok(if c.kind == 3 {
            c.start.clone()
        } else {
            c.end.clone()
        })
    }
    fn close(&mut self) -> Result<Frame> {
        let types = self.ctrls.last().ok_or("unexpected end")?.end.clone();
        self.pop_types(&types)?;
        let c = self.ctrls.pop().unwrap();
        if self.vals.len() != c.height {
            return Err("excess operands at control boundary".into());
        }
        Ok(c)
    }
}

fn memory(m: &Module) -> Result<()> {
    if m.memory.is_none() {
        Err("memory instruction requires a memory".into())
    } else {
        Ok(())
    }
}
fn table(m: &Module) -> Result<()> {
    if m.table.is_none() {
        Err("table instruction requires a table".into())
    } else {
        Ok(())
    }
}
fn align(i: &Instr, max: u32) -> Result<()> {
    if i.a > max {
        Err("memory alignment exceeds natural alignment".into())
    } else {
        Ok(())
    }
}
fn lane(i: &Instr, count: i64) -> Result<()> {
    if i.imm < 0 || i.imm >= count {
        Err("SIMD lane index out of range".into())
    } else {
        Ok(())
    }
}

fn validate_function(m: &Module, f: &Func) -> Result<()> {
    let ty = &m.types[f.type_idx as usize];
    let mut locals = ty.params.clone();
    locals.extend_from_slice(&f.locals);
    let mut s = Stack {
        vals: vec![],
        ctrls: vec![Frame {
            kind: 0xff,
            start: vec![],
            end: ty.results.clone(),
            height: 0,
            unreachable: false,
        }],
    };
    for (pc, i) in f.code.iter().enumerate() {
        if s.ctrls.is_empty() {
            return Err("instruction after function end".into());
        }
        match i.op {
            0x00 => s.dead(),
            0x01 => (),
            0x02..=0x04 => {
                let bt = block_type(m, i.imm)?;
                if i.op == 4 {
                    s.pop(Some(I32))?;
                }
                s.pop_types(&bt.params)?;
                s.ctrls.push(Frame {
                    kind: i.op,
                    start: bt.params.clone(),
                    end: bt.results,
                    height: s.vals.len(),
                    unreachable: false,
                });
                s.push_types(&bt.params);
            }
            0x05 => {
                if s.ctrls.last().unwrap().kind != 4 {
                    return Err("else without matching if, or duplicate else".into());
                }
                let c = s.close()?;
                s.push_types(&c.start);
                s.ctrls.push(Frame {
                    kind: 5,
                    start: c.start,
                    end: c.end,
                    height: c.height,
                    unreachable: false,
                });
            }
            0x0b => {
                let c = s.close()?;
                if c.kind == 4 && c.start != c.end {
                    return Err("if without else requires identical parameter/result types".into());
                }
                s.push_types(&c.end);
                if s.ctrls.is_empty() && pc + 1 != f.code.len() {
                    return Err("instructions after function end".into());
                }
            }
            0x0c => {
                let types = s.label(i.a)?;
                s.pop_types(&types)?;
                s.dead();
            }
            0x0d => {
                let types = s.label(i.a)?;
                s.pop(Some(I32))?;
                s.pop_types(&types)?;
                s.push_types(&types);
            }
            0x0e => {
                s.pop(Some(I32))?;
                let default = s.label(i.a)?;
                for &index in &i.list {
                    let types = s.label(index)?;
                    if types.len() != default.len() {
                        return Err("br_table target arities differ".into());
                    }
                    // Preserve actual unknowns: in unreachable code equal arity, differing
                    // label types can be valid when every argument is polymorphic.
                    let mut actual = Vec::with_capacity(types.len());
                    for &t in types.iter().rev() {
                        actual.push(s.pop(Some(t))?);
                    }
                    s.vals.extend(actual.into_iter().rev());
                }
                s.pop_types(&default)?;
                s.dead();
            }
            0x0f => {
                s.pop_types(&ty.results)?;
                s.dead();
            }
            0x10 | 0x11 => {
                let called = if i.op == 0x10 {
                    let target = m.funcs.get(i.a as usize).ok_or("unknown function index")?;
                    &m.types[target.type_idx as usize]
                } else {
                    table(m)?;
                    if i.b != 0 {
                        return Err("unknown indirect-call table index".into());
                    }
                    let t = m
                        .types
                        .get(i.a as usize)
                        .ok_or("unknown indirect-call type index")?;
                    s.pop(Some(I32))?;
                    t
                };
                s.effect(&called.params, &called.results)?;
            }
            0x1a => {
                s.pop(None)?;
            }
            0x1b => {
                s.pop(Some(I32))?;
                let a = s.pop(None)?;
                let b = s.pop(None)?;
                if a.is_some() && b.is_some() && a != b {
                    return Err("select operand types differ".into());
                }
                s.vals.push(a.or(b));
            }
            0x20..=0x22 => {
                let t = *locals.get(i.a as usize).ok_or("unknown local index")?;
                if i.op != 0x20 {
                    s.pop(Some(t))?;
                }
                if i.op != 0x21 {
                    s.vals.push(Some(t));
                }
            }
            0x23..=0x24 => {
                let g = m.globals.get(i.a as usize).ok_or("unknown global index")?;
                if i.op == 0x23 {
                    s.vals.push(Some(g.ty));
                } else {
                    if !g.mutable {
                        return Err("global.set requires a mutable global".into());
                    }
                    s.pop(Some(g.ty))?;
                }
            }
            0x28..=0x3e => {
                memory(m)?;
                let (t, a) = match i.op {
                    0x28 | 0x36 => (I32, 2),
                    0x29 | 0x37 => (I64, 3),
                    0x2a | 0x38 => (F32, 2),
                    0x2b | 0x39 => (F64, 3),
                    0x2c..=0x2d | 0x3a => (I32, 0),
                    0x2e..=0x2f | 0x3b => (I32, 1),
                    0x30..=0x31 | 0x3c => (I64, 0),
                    0x32..=0x33 | 0x3d => (I64, 1),
                    0x34..=0x35 | 0x3e => (I64, 2),
                    _ => unreachable!(),
                };
                align(i, a)?;
                if i.op >= 0x36 {
                    s.pop(Some(t))?;
                }
                s.pop(Some(I32))?;
                if i.op < 0x36 {
                    s.vals.push(Some(t));
                }
            }
            0x3f..=0x40 => {
                memory(m)?;
                if i.a != 0 {
                    return Err("unknown memory index".into());
                }
                if i.op == 0x40 {
                    s.pop(Some(I32))?;
                }
                s.vals.push(Some(I32));
            }
            0xfc08..=0xfc0e => {
                match i.op {
                    0xfc08 | 0xfc09 => {
                        if m.data_count.is_none() {
                            return Err(
                                "memory.init/data.drop requires a data count section".into()
                            );
                        }
                        if i.a as usize >= m.data.len() {
                            return Err("unknown data segment index".into());
                        }
                        if i.op == 0xfc08 {
                            memory(m)?;
                            if i.b != 0 {
                                return Err("unknown memory index".into());
                            }
                        }
                    }
                    0xfc0a => {
                        memory(m)?;
                        if i.a != 0 || i.b != 0 {
                            return Err("unknown memory index".into());
                        }
                    }
                    0xfc0b => {
                        memory(m)?;
                        if i.a != 0 {
                            return Err("unknown memory index".into());
                        }
                    }
                    0xfc0c | 0xfc0d => {
                        if i.a as usize >= m.elems.len() {
                            return Err("unknown element segment index".into());
                        }
                        if i.op == 0xfc0c {
                            table(m)?;
                            if i.b != 0 {
                                return Err("unknown table index".into());
                            }
                        }
                    }
                    0xfc0e => {
                        table(m)?;
                        if i.a != 0 || i.b != 0 {
                            return Err("unknown table index".into());
                        }
                    }
                    _ => unreachable!(),
                }
                if i.op != 0xfc09 && i.op != 0xfc0d {
                    s.pop_types(&[I32, I32, I32])?;
                }
            }
            0xfd00..=0xfdff => {
                let sub = i.op - 0xfd00;
                let (inputs, outputs) = simd_signature(sub).ok_or("invalid SIMD opcode")?;
                match sub {
                    0..=0x0b | 0x54..=0x5d => {
                        memory(m)?;
                        let a = match sub {
                            0 | 0x0b => 4,
                            1..=6 | 0x0a | 0x5d => 3,
                            7 => 0,
                            8 => 1,
                            9 | 0x5c => 2,
                            0x54..=0x57 => sub - 0x54,
                            0x58..=0x5b => sub - 0x58,
                            _ => unreachable!(),
                        };
                        align(i, a)?;
                        if (0x54..=0x5b).contains(&sub) {
                            lane(i, 16 >> (sub & 3))?;
                        }
                    }
                    0x0d => {
                        if i.bytes.iter().any(|&b| b >= 32) {
                            return Err("SIMD shuffle lane index out of range".into());
                        }
                    }
                    0x15..=0x17 => lane(i, 16)?,
                    0x18..=0x1a => lane(i, 8)?,
                    0x1b..=0x1c | 0x1f..=0x20 => lane(i, 4)?,
                    0x1d..=0x1e | 0x21..=0x22 => lane(i, 2)?,
                    _ => (),
                }
                s.effect(inputs, outputs)?;
            }
            op => {
                let (inputs, outputs) =
                    numeric_signature(op).ok_or_else(|| format!("unsupported opcode {op:#x}"))?;
                s.effect(inputs, outputs)?;
            }
        }
    }
    if !s.ctrls.is_empty() {
        return Err("unterminated function/control construct".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ins(op: u32) -> Instr {
        Instr {
            op,
            ..Instr::default()
        }
    }
    fn sample(params: Vec<Ty>, results: Vec<Ty>, code: Vec<Instr>) -> Module {
        Module {
            types: vec![FuncType { params, results }],
            funcs: vec![Func {
                type_idx: 0,
                import: None,
                locals: vec![],
                code,
            }],
            ..Module::default()
        }
    }
    #[test]
    fn simd_opcode_set() {
        assert_eq!(
            (0..=255).filter(|&op| simd_signature(op).is_some()).count(),
            236
        );
        for op in [
            0x9a, 0xa2, 0xa5, 0xa6, 0xaf, 0xb0, 0xb2, 0xb3, 0xb4, 0xbb, 0xc2, 0xc5, 0xc6, 0xcf,
            0xd0, 0xd2, 0xd3, 0xd4, 0xe2, 0xee, 0x100,
        ] {
            assert!(simd_signature(op).is_none(), "opcode {op:#x}");
        }
        for op in 0..=255 {
            if let Some((inputs, outputs)) = simd_signature(op) {
                let mut code = Vec::new();
                for n in 0..inputs.len() {
                    let mut get = ins(0x20);
                    get.a = n as u32;
                    code.push(get);
                }
                code.push(ins(0xfd00 + op));
                code.push(ins(0x0b));
                let mut m = sample(inputs.to_vec(), outputs.to_vec(), code);
                m.memory = Some(Limits { min: 0, max: None });
                assert!(validate(&m).is_ok(), "SIMD opcode {op:#x}");
            }
        }
    }
    #[test]
    fn multivalue_and_implicit_else() {
        let mut block = ins(2);
        block.imm = 0;
        let mut get0 = ins(0x20);
        get0.a = 0;
        let mut get1 = ins(0x20);
        get1.a = 1;
        let m = sample(
            vec![I32, I64],
            vec![I32, I64],
            vec![get0, get1, block, ins(0x0b), ins(0x0b)],
        );
        assert!(validate(&m).is_ok());
        let mut if_i = ins(4);
        if_i.imm = 0;
        let mut m = sample(
            vec![I32],
            vec![I32],
            vec![ins(0x20), ins(0x41), if_i, ins(0x0b), ins(0x0b)],
        );
        assert!(validate(&m).is_ok());
        m.types[0].results = vec![I64];
        assert!(validate(&m).is_err());
    }
    #[test]
    fn polymorphic_br_table() {
        let mut outer = ins(2);
        outer.imm = -1;
        let mut inner = ins(2);
        inner.imm = -2;
        let mut branch = ins(0x0e);
        branch.list = vec![0];
        branch.a = 1;
        let code = vec![
            outer,
            inner,
            ins(0),
            branch,
            ins(0x0b),
            ins(0x1a),
            ins(0),
            ins(0x0b),
            ins(0x0b),
        ];
        assert!(validate(&sample(vec![], vec![I32], code)).is_ok());
    }
    #[test]
    fn bulk_indices_and_data_count() {
        let mut m = sample(vec![], vec![], vec![ins(0xfc09), ins(0x0b)]);
        m.data.push(Data {
            passive: true,
            offset: vec![],
            bytes: vec![],
        });
        assert!(validate(&m).is_err());
        m.data_count = Some(1);
        assert!(validate(&m).is_ok());
        m.data_count = Some(0);
        assert!(validate(&m).is_err());
    }
}

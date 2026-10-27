//! Strict, bounded WebAssembly binary decoding for the supported feature set.
use crate::model::*;

type Result<T> = std::result::Result<T, String>;

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}
impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }
    fn remaining(&self) -> usize {
        self.bytes.len() - self.pos
    }
    fn byte(&mut self) -> Result<u8> {
        let b = *self.bytes.get(self.pos).ok_or("unexpected end of binary")?;
        self.pos += 1;
        Ok(b)
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if n > self.remaining() {
            return Err("unexpected end of binary".into());
        }
        let start = self.pos;
        self.pos += n;
        Ok(&self.bytes[start..self.pos])
    }
    fn uint(&mut self) -> Result<u32> {
        let mut value = 0u32;
        for n in 0..5 {
            let b = self.byte()?;
            if n == 4 && b & 0xf0 != 0 {
                return Err("invalid u32 LEB128 encoding".into());
            }
            value |= ((b & 127) as u32) << (n * 7);
            if b & 128 == 0 {
                return Ok(value);
            }
        }
        Err("u32 LEB128 too long".into())
    }
    fn sint(&mut self, bits: u32) -> Result<i64> {
        let count = bits.div_ceil(7);
        let mut value = 0u64;
        for n in 0..count {
            let b = self.byte()?;
            let shift = n * 7;
            if n + 1 == count {
                let used = bits - shift;
                let mask = 0x7fu8 << used;
                let sign = 1u8 << (used - 1);
                if b & 128 != 0 || (b & mask != if b & sign != 0 { mask & 127 } else { 0 }) {
                    return Err(format!("invalid signed {bits}-bit LEB128 encoding"));
                }
            }
            value |= ((b & 127) as u64) << shift;
            if b & 128 == 0 {
                let width = shift + 7;
                if b & 64 != 0 && width < 64 {
                    value |= (!0u64) << width;
                }
                return Ok(value as i64);
            }
        }
        Err("signed LEB128 too long".into())
    }
    fn zero(&mut self) -> Result<()> {
        if self.byte()? != 0 {
            return Err("nonzero reserved byte (multiple memories/tables are unsupported)".into());
        }
        Ok(())
    }
    fn count(&mut self) -> Result<usize> {
        let n = self.uint()? as usize;
        if n > self.remaining() {
            return Err("vector count exceeds available binary data".into());
        }
        Ok(n)
    }
    fn name(&mut self) -> Result<String> {
        let n = self.uint()? as usize;
        let bytes = self.take(n)?;
        Ok(std::str::from_utf8(bytes)
            .map_err(|_| "name is not valid UTF-8")?
            .to_owned())
    }
    fn ty(&mut self) -> Result<Ty> {
        match self.byte()? {
            0x7f => Ok(Ty::I32),
            0x7e => Ok(Ty::I64),
            0x7d => Ok(Ty::F32),
            0x7c => Ok(Ty::F64),
            0x7b => Ok(Ty::V128),
            _ => Err("unsupported or invalid value type".into()),
        }
    }
    fn types(&mut self) -> Result<Vec<Ty>> {
        let n = self.count()?;
        (0..n).map(|_| self.ty()).collect()
    }
    fn limits(&mut self, memory: bool) -> Result<Limits> {
        let flags = self.byte()?;
        if flags > 1 {
            return Err("shared, 64-bit, and other extended limits are unsupported".into());
        }
        let min = self.uint()?;
        let max = if flags == 1 { Some(self.uint()?) } else { None };
        if max.is_some_and(|x| x < min)
            || (memory && (min > 65536 || max.is_some_and(|x| x > 65536)))
        {
            return Err("invalid limits".into());
        }
        Ok(Limits { min, max })
    }
    fn memarg(&mut self, i: &mut Instr) -> Result<()> {
        i.a = self.uint()?;
        i.b = self.uint()?;
        Ok(())
    }
    fn instruction(&mut self) -> Result<Instr> {
        let op = self.byte()? as u32;
        let mut i = Instr {
            op,
            ..Instr::default()
        };
        match op {
            0x02..=0x04 => {
                // Empty/value block types are literal bytes, not arbitrary signed
                // LEB encodings of their negative values. Only indices use s33.
                let first = *self.bytes.get(self.pos).ok_or("missing block type")?;
                if first == 0x40 || (0x7b..=0x7f).contains(&first) {
                    self.byte()?;
                    i.imm = first as i64 - 128;
                } else {
                    i.imm = self.sint(33)?;
                    if i.imm < 0 {
                        return Err("invalid negative block type index".into());
                    }
                }
            }
            0x0c | 0x0d | 0x10 | 0x20..=0x24 => {
                i.a = self.uint()?;
            }
            0x0e => {
                let n = self.count()?;
                for _ in 0..n {
                    i.list.push(self.uint()?);
                }
                i.a = self.uint()?;
            }
            0x11 => {
                i.a = self.uint()?;
                self.zero()?;
            }
            0x28..=0x3e => self.memarg(&mut i)?,
            0x3f | 0x40 => self.zero()?,
            0x41 => i.imm = self.sint(32)?,
            0x42 => i.imm = self.sint(64)?,
            0x43 => i.bytes[..4].copy_from_slice(self.take(4)?),
            0x44 => i.bytes[..8].copy_from_slice(self.take(8)?),
            0xfc => {
                let sub = self.uint()?;
                if sub > 14 {
                    return Err(format!("unsupported 0xfc opcode {sub}"));
                }
                i.op = 0xfc00 + sub;
                match sub {
                    0..=7 => (),
                    8 => {
                        i.a = self.uint()?;
                        self.zero()?;
                    }
                    9 | 13 => {
                        i.a = self.uint()?;
                    }
                    10 => {
                        self.zero()?;
                        self.zero()?;
                    }
                    11 => self.zero()?,
                    12 | 14 => {
                        // Bulk table indices are u32 even without reference types;
                        // unlike call_indirect, padded zero encodings are valid.
                        i.a = self.uint()?;
                        i.b = self.uint()?;
                    }
                    _ => unreachable!(),
                }
            }
            0xfd => {
                let sub = self.uint()?;
                if crate::validate::simd_signature(sub).is_none() {
                    return Err(format!("invalid or unsupported SIMD opcode {sub:#x}"));
                }
                i.op = 0xfd00 + sub;
                match sub {
                    0..=11 | 0x5c | 0x5d => self.memarg(&mut i)?,
                    12 | 13 => i.bytes.copy_from_slice(self.take(16)?),
                    0x15..=0x22 => i.imm = self.byte()? as i64,
                    0x54..=0x5b => {
                        self.memarg(&mut i)?;
                        i.imm = self.byte()? as i64;
                    }
                    _ => (),
                }
            }
            0x00 | 0x01 | 0x05 | 0x0b | 0x0f | 0x1a | 0x1b | 0x45..=0xc4 => (),
            _ => return Err(format!("unsupported or invalid opcode {op:#x}")),
        }
        Ok(i)
    }
    fn expr(&mut self) -> Result<Vec<Instr>> {
        let mut code = Vec::new();
        let mut depth = 1usize;
        while depth != 0 {
            let i = self.instruction()?;
            match i.op {
                2..=4 => depth += 1,
                0x0b => depth -= 1,
                _ => (),
            }
            code.push(i);
        }
        Ok(code)
    }
    fn done(&self) -> Result<()> {
        if self.remaining() != 0 {
            Err("trailing bytes in section or function body".into())
        } else {
            Ok(())
        }
    }
}

/// Decode and fully validate a binary module before exposing it to code generation.
pub fn parse(bytes: &[u8]) -> Result<Module> {
    let mut r = Reader::new(bytes);
    if r.take(8)? != b"\0asm\x01\0\0\0" {
        return Err("invalid WebAssembly magic or version".into());
    }
    let mut m = Module::default();
    let mut seen = [false; 13];
    let mut last_rank = 0;
    let mut defined_types = Vec::new();
    let mut bodies = 0usize;
    while r.remaining() != 0 {
        let id = r.byte()? as usize;
        if id > 12 {
            return Err(format!("unsupported section id {id}"));
        }
        let n = r.uint()? as usize;
        let mut s = Reader::new(r.take(n)?);
        if id == 0 {
            s.name()?;
            continue;
        }
        let rank = match id {
            12 => 10,
            10 => 11,
            11 => 12,
            _ => id,
        };
        if seen[id] || rank <= last_rank {
            return Err("duplicate or out-of-order section".into());
        }
        seen[id] = true;
        last_rank = rank;
        match id {
            1 => {
                let n = s.count()?;
                for _ in 0..n {
                    if s.byte()? != 0x60 {
                        return Err("unsupported non-function type".into());
                    }
                    m.types.push(FuncType {
                        params: s.types()?,
                        results: s.types()?,
                    });
                }
            }
            2 => {
                let n = s.count()?;
                for _ in 0..n {
                    let module = s.name()?;
                    let name = s.name()?;
                    if s.byte()? != 0 {
                        return Err("non-function imports are unsupported by this ABI".into());
                    }
                    m.funcs.push(Func {
                        type_idx: s.uint()?,
                        import: Some((module, name)),
                        locals: vec![],
                        code: vec![],
                    });
                }
            }
            3 => {
                let n = s.count()?;
                for _ in 0..n {
                    defined_types.push(s.uint()?);
                }
            }
            4 => {
                let n = s.count()?;
                if n > 1 {
                    return Err("multiple tables are unsupported".into());
                }
                if n == 1 {
                    if s.byte()? != 0x70 {
                        return Err("only funcref tables are supported".into());
                    }
                    m.table = Some(s.limits(false)?);
                }
            }
            5 => {
                let n = s.count()?;
                if n > 1 {
                    return Err("multiple memories are unsupported".into());
                }
                if n == 1 {
                    m.memory = Some(s.limits(true)?);
                }
            }
            6 => {
                let n = s.count()?;
                for _ in 0..n {
                    let ty = s.ty()?;
                    let mutable = match s.byte()? {
                        0 => false,
                        1 => true,
                        _ => return Err("invalid global mutability".into()),
                    };
                    m.globals.push(Global {
                        ty,
                        mutable,
                        init: s.expr()?,
                    });
                }
            }
            7 => {
                let n = s.count()?;
                for _ in 0..n {
                    m.exports.push(Export {
                        name: s.name()?,
                        kind: s.byte()?,
                        index: s.uint()?,
                    });
                }
            }
            8 => m.start = Some(s.uint()?),
            9 => {
                let n = s.count()?;
                for _ in 0..n {
                    let flags = s.uint()?;
                    if flags > 3 {
                        return Err("reference-expression element segments are unsupported".into());
                    }
                    if flags == 2 && s.uint()? != 0 {
                        return Err("unknown table index".into());
                    }
                    let offset = if flags == 0 || flags == 2 {
                        s.expr()?
                    } else {
                        vec![]
                    };
                    if flags != 0 && s.byte()? != 0 {
                        return Err("invalid element kind".into());
                    }
                    let count = s.count()?;
                    let funcs = (0..count).map(|_| s.uint()).collect::<Result<Vec<_>>>()?;
                    m.elems.push(Elem {
                        passive: flags == 1,
                        declarative: flags == 3,
                        offset,
                        funcs,
                    });
                }
            }
            10 => {
                bodies = s.count()?;
                if bodies != defined_types.len() {
                    return Err("function and code section counts differ".into());
                }
                for &type_idx in &defined_types {
                    let size = s.uint()? as usize;
                    let mut body = Reader::new(s.take(size)?);
                    let n = body.count()?;
                    let mut locals = Vec::new();
                    for _ in 0..n {
                        let count = body.uint()? as usize;
                        let ty = body.ty()?;
                        let len = locals.len().checked_add(count).ok_or("too many locals")?;
                        // Keep adversarial compressed local declarations from exhausting the translator.
                        if len > 1_000_000 {
                            return Err(
                                "implementation limit: more than one million function locals"
                                    .into(),
                            );
                        }
                        locals.resize(len, ty);
                    }
                    let code = body.expr()?;
                    body.done()?;
                    m.funcs.push(Func {
                        type_idx,
                        import: None,
                        locals,
                        code,
                    });
                }
            }
            11 => {
                let n = s.count()?;
                for _ in 0..n {
                    let flags = s.uint()?;
                    if flags > 2 {
                        return Err("invalid data segment flags".into());
                    }
                    if flags == 2 && s.uint()? != 0 {
                        return Err("unknown memory index".into());
                    }
                    let offset = if flags != 1 { s.expr()? } else { vec![] };
                    let size = s.uint()? as usize;
                    m.data.push(Data {
                        passive: flags == 1,
                        offset,
                        bytes: s.take(size)?.to_vec(),
                    });
                }
            }
            12 => m.data_count = Some(s.uint()?),
            _ => unreachable!(),
        }
        s.done()?;
    }
    if bodies != defined_types.len() {
        return Err("missing function bodies".into());
    }
    crate::validate::validate(&m)?;
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn module(code: &[u8], result: Option<u8>) -> Vec<u8> {
        let mut b = b"\0asm\x01\0\0\0".to_vec();
        let mut ty = vec![1, 0x60, 0, u8::from(result.is_some())];
        if let Some(r) = result {
            ty.push(r);
        }
        b.extend([1, ty.len() as u8]);
        b.extend(ty);
        b.extend([3, 2, 1, 0]);
        b.extend([10, (code.len() + 3) as u8, 1, (code.len() + 1) as u8, 0]);
        b.extend_from_slice(code);
        b
    }
    #[test]
    fn leb_boundaries_and_padding() {
        assert_eq!(
            Reader::new(&[0xff, 0xff, 0xff, 0xff, 0x0f]).uint().unwrap(),
            u32::MAX
        );
        assert!(Reader::new(&[0xff, 0xff, 0xff, 0xff, 0x1f]).uint().is_err());
        assert_eq!(
            Reader::new(&[0xff, 0xff, 0xff, 0xff, 0x7f])
                .sint(32)
                .unwrap(),
            -1
        );
        assert_eq!(
            Reader::new(&[0x80, 0x80, 0x80, 0x80, 0x78])
                .sint(32)
                .unwrap(),
            i32::MIN as i64
        );
        assert!(Reader::new(&[0xff, 0xff, 0xff, 0xff, 0x0f])
            .sint(32)
            .is_err());
        assert_eq!(
            Reader::new(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00])
                .sint(64)
                .unwrap(),
            i64::MAX
        );
        assert_eq!(
            Reader::new(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x7f])
                .sint(64)
                .unwrap(),
            i64::MIN
        );
        assert!(Reader::new(&[0xff; 10]).sint(64).is_err());
    }
    #[test]
    fn bulk_table_indices_are_leb_but_reserved_bytes_are_not() {
        for code in [
            &[0xfc, 12, 0, 0x80, 0][..],
            &[0xfc, 14, 0x80, 0, 0x80, 0][..],
        ] {
            let mut r = Reader::new(code);
            let i = r.instruction().unwrap();
            assert_eq!((i.a, i.b), (0, 0));
            r.done().unwrap();
        }
        for code in [
            &[0x11, 0, 0x80, 0][..],
            &[0xfc, 8, 0, 0x80, 0][..],
            &[0xfc, 10, 0x80, 0, 0][..],
        ] {
            assert!(Reader::new(code).instruction().is_err());
        }
        assert!(Reader::new(&[0x80, 0, 0]).limits(false).is_err());
        assert!(Reader::new(&[0x80, 0, 0]).limits(true).is_err());
    }
    #[test]
    fn basic_decode_and_type_check() {
        assert!(parse(&module(&[0x41, 42, 0x0b], Some(0x7f))).is_ok());
        assert!(parse(&module(&[0x42, 42, 0x0b], Some(0x7f))).is_err());
        assert!(parse(&module(&[0x41, 1, 0x0b], None)).is_err());
        assert!(parse(&module(&[0x6a, 0x0b], Some(0x7f))).is_err());
        assert!(parse(&module(&[0, 0x6a, 0x0b], Some(0x7f))).is_ok());
        assert!(parse(&module(&[0, 0x43, 0, 0, 0, 0, 0x6a, 0x0b], Some(0x7f))).is_err());
        assert!(parse(&module(&[0, 0x20, 0, 0x0b], None)).is_err());
        assert!(parse(&module(&[0x05, 0x0b], None)).is_err());
    }
    #[test]
    fn control_boundaries_and_block_encoding() {
        assert!(parse(&module(&[2, 0x40, 0x0b, 0x0b], None)).is_ok());
        assert!(parse(&module(&[2, 0xc0, 0x7f, 0x0b, 0x0b], None)).is_err());
        assert!(parse(&module(
            &[0x41, 1, 4, 0x7f, 0x41, 2, 0x0b, 0x0b],
            Some(0x7f)
        ))
        .is_err());
        assert!(parse(&module(
            &[0x41, 1, 4, 0x7f, 0x41, 2, 5, 0x41, 3, 0x0b, 0x0b],
            Some(0x7f)
        ))
        .is_ok());
        assert!(parse(&module(&[0, 0x0c, 1, 0x0b], None)).is_err());
    }
    #[test]
    fn floats_remain_little_endian() {
        let m = parse(&module(&[0x43, 0x78, 0x56, 0x34, 0x12, 0x0b], Some(0x7d))).unwrap();
        assert_eq!(&m.funcs[0].code[0].bytes[..4], &[0x78, 0x56, 0x34, 0x12]);
    }
    #[test]
    fn simd_and_scope() {
        let mut code = vec![0xfd, 12];
        code.extend([0; 16]);
        code.extend([0xfd, 0x15, 16, 0x0b]);
        assert!(parse(&module(&code, Some(0x7f))).is_err());
        let n = code.len();
        code[n - 2] = 15;
        assert!(parse(&module(&code, Some(0x7f))).is_ok());
        assert!(parse(&module(&[0xd0, 0x70, 0x1a, 0x0b], None)).is_err());
        assert!(parse(&module(&[0xfd, 0x80, 2, 0x0b], None)).is_err());
        assert!(parse(&module(&[0, 0x28, 3, 0, 0x0b], Some(0x7f))).is_err());
    }
    #[test]
    fn malformed_sections() {
        let empty = b"\0asm\x01\0\0\0";
        assert!(parse(empty).is_ok());
        for bytes in [
            vec![1, 1, 0, 1, 1, 0],
            vec![0, 1, 0xff],
            vec![12, 1, 1],
            vec![8, 1, 0],
        ] {
            let mut b = empty.to_vec();
            b.extend(bytes);
            assert!(parse(&b).is_err());
        }
        let mut b = module(&[0x0b], None);
        b.push(0);
        assert!(parse(&b).is_err());
    }
}

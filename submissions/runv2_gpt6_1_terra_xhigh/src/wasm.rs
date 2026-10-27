//! WebAssembly 2.0 binary decoder, independent of the rest of the translator.
//!
//! `parse` performs binary and structural checks, plus inexpensive module/index
//! checks. It is not a complete WebAssembly operand-stack/type validator; the
//! code generator must still validate instruction types and control-flow merges.
//! Function bodies retain their exact instruction bytes, including the final
//! `end`. Locals are run-length encoded so a tiny binary cannot force a huge
//! allocation. All index spaces include imports before definitions.

use std::collections::HashSet;
use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub offset: usize,
    pub message: String,
}

impl Error {
    fn new(offset: usize, message: impl Into<String>) -> Self {
        Self { offset, message: message.into() }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "wasm at byte 0x{:x}: {}", self.offset, self.message)
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ValType { I32, I64, F32, F64, V128, FuncRef, ExternRef }

impl ValType {
    pub fn is_ref(self) -> bool { matches!(self, Self::FuncRef | Self::ExternRef) }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuncType { pub params: Vec<ValType>, pub results: Vec<ValType> }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits { pub min: u32, pub max: Option<u32> }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableType { pub element: ValType, pub limits: Limits }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryType { pub limits: Limits }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlobalType { pub ty: ValType, pub mutable: bool }

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportDesc { Func(u32), Table(TableType), Memory(MemoryType), Global(GlobalType) }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import { pub module: String, pub name: String, pub desc: ImportDesc }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportDesc { Func(u32), Table(u32), Memory(u32), Global(u32) }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Export { pub name: String, pub desc: ExportDesc }

/// A 2.0 constant expression. Float constants retain their exact bit patterns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstOp {
    I32(i32), I64(i64), F32(u32), F64(u64), V128([u8; 16]),
    GlobalGet(u32), RefNull(ValType), RefFunc(u32),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstExpr {
    pub op: ConstOp,
    /// Exact bytes, including the terminating `end`.
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Global { pub ty: GlobalType, pub init: ConstExpr }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalDecl { pub count: u32, pub ty: ValType }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    pub type_index: u32,
    /// Declared locals only, not parameters. Zero-count groups are preserved.
    pub locals: Vec<LocalDecl>,
    /// Exact encoded instructions, including the final `end`.
    pub body: Vec<u8>,
    /// Absolute input byte offset of `body`.
    pub body_offset: usize,
}
impl Function {
    pub fn local_count(&self) -> u32 { self.locals.iter().map(|l| l.count as u64).sum::<u64>() as u32 }
    #[cfg(test)]
    pub fn local_types(&self) -> impl Iterator<Item = ValType> + '_ {
        self.locals.iter().flat_map(|l| std::iter::repeat(l.ty).take(l.count as usize))
    }
    pub fn reader(&self) -> Reader<'_> { Reader::with_offset(&self.body, self.body_offset) }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ElementMode { Active { table: u32, offset: ConstExpr }, Passive, Declarative }
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ElementInit { Func(u32), Expr(ConstExpr) }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Element { pub ty: ValType, pub mode: ElementMode, pub init: Vec<ElementInit> }
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DataMode { Active { memory: u32, offset: ConstExpr }, Passive }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Data { pub mode: DataMode, pub bytes: Vec<u8> }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomSection { pub name: String, pub data: Vec<u8> }

/// Definitions exclude imports; exported and instruction indices include them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Module {
    pub types: Vec<FuncType>,
    pub imports: Vec<Import>,
    pub functions: Vec<Function>,
    pub tables: Vec<TableType>,
    pub memories: Vec<MemoryType>,
    pub globals: Vec<Global>,
    pub exports: Vec<Export>,
    pub start: Option<u32>,
    pub elements: Vec<Element>,
    pub data: Vec<Data>,
    pub data_count: Option<u32>,
    pub customs: Vec<CustomSection>,
}
impl Module {
    pub fn imported_memory_count(&self) -> usize { self.imports.iter().filter(|i| matches!(i.desc, ImportDesc::Memory(_))).count() }
    pub fn memory_type(&self, index: u32) -> Option<MemoryType> {
        let imported = self.imported_memory_count();
        if (index as usize) < imported {
            self.imports.iter().filter_map(|i| match i.desc { ImportDesc::Memory(t) => Some(t), _ => None }).nth(index as usize)
        } else { self.memories.get(index as usize - imported).copied() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockType { Empty, Value(ValType), TypeIndex(u32) }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemArg { pub align: u32, pub offset: u32 }
/// Immediates are in binary order, e.g. `call_indirect` is (type, table).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Immediate {
    None, Block(BlockType), Index(u32), IndexPair(u32, u32),
    BrTable { targets: Vec<u32>, default: u32 }, Types(Vec<ValType>),
    MemArg(MemArg), I32(i32), I64(i64), F32(u32), F64(u64),
    RefType(ValType), V128([u8; 16]), Shuffle([u8; 16]),
    Lane(u8), MemLane(MemArg, u8),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instruction {
    pub opcode: u8,
    /// Subopcode for 0xfc and 0xfd, decoded as a u32 LEB.
    pub subopcode: Option<u32>,
    pub immediate: Immediate,
}

/// Bounds-checked reader; `position` is relative, `offset` absolute.
#[derive(Clone, Debug)]
pub struct Reader<'a> { bytes: &'a [u8], pos: usize, base: usize }
impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self { Self::with_offset(bytes, 0) }
    pub fn with_offset(bytes: &'a [u8], base: usize) -> Self { Self { bytes, pos: 0, base } }
    pub fn position(&self) -> usize { self.pos }
    pub fn offset(&self) -> usize { self.base.saturating_add(self.pos) }
    pub fn remaining(&self) -> usize { self.bytes.len() - self.pos }
    pub fn is_empty(&self) -> bool { self.pos == self.bytes.len() }
    pub fn peek_u8(&self) -> Result<u8> {
        self.bytes.get(self.pos).copied().ok_or_else(|| Error::new(self.offset(), "unexpected end of input"))
    }
    pub fn read_u8(&mut self) -> Result<u8> { let b = self.peek_u8()?; self.pos += 1; Ok(b) }
    pub fn read_bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        if n > self.remaining() { return Err(Error::new(self.offset(), "unexpected end of input")); }
        let start = self.pos; self.pos += n; Ok(&self.bytes[start..self.pos])
    }
    fn unsigned(&mut self, bits: u32) -> Result<u64> {
        let start = self.offset();
        let mut value = 0u64;
        let count = (bits + 6) / 7;
        for i in 0..count {
            let b = self.read_u8()?;
            let used = (bits - i * 7).min(7);
            let payload = (b & 0x7f) as u64;
            if used < 7 && payload >= (1u64 << used) { return Err(Error::new(start, "unsigned LEB128 overflow")); }
            value |= payload << (i * 7);
            if b & 0x80 == 0 { return Ok(value); }
        }
        Err(Error::new(start, "unsigned LEB128 too long"))
    }
    fn signed(&mut self, bits: u32) -> Result<i64> {
        let start = self.offset();
        let mut value = 0u128;
        let count = (bits + 6) / 7;
        for i in 0..count {
            let b = self.read_u8()?;
            let used = (bits - i * 7).min(7);
            let payload = b & 0x7f;
            if used < 7 {
                let high = payload >> (used - 1);
                if high != 0 && high != ((1u8 << (8 - used)) - 1) {
                    return Err(Error::new(start, "signed LEB128 overflow"));
                }
            }
            value |= (payload as u128) << (i * 7);
            if b & 0x80 == 0 {
                let width = (i + 1) * 7;
                let signed = if b & 0x40 != 0 { (value as i128) - (1i128 << width) } else { value as i128 };
                return Ok(signed as i64);
            }
        }
        Err(Error::new(start, "signed LEB128 too long"))
    }
    pub fn read_u32(&mut self) -> Result<u32> { Ok(self.unsigned(32)? as u32) }
    pub fn read_i32(&mut self) -> Result<i32> { Ok(self.signed(32)? as i32) }
    pub fn read_i33(&mut self) -> Result<i64> { self.signed(33) }
    pub fn read_i64(&mut self) -> Result<i64> { self.signed(64) }
    pub fn read_f32_bits(&mut self) -> Result<u32> { Ok(u32::from_le_bytes(self.read_bytes(4)?.try_into().unwrap())) }
    pub fn read_f64_bits(&mut self) -> Result<u64> { Ok(u64::from_le_bytes(self.read_bytes(8)?.try_into().unwrap())) }
    pub fn read_name(&mut self) -> Result<String> {
        let n = self.read_u32()? as usize;
        let offset = self.offset();
        let b = self.read_bytes(n)?;
        Ok(std::str::from_utf8(b).map_err(|_| Error::new(offset, "invalid UTF-8 name"))?.to_owned())
    }
    pub fn read_val_type(&mut self) -> Result<ValType> {
        let at = self.offset();
        match self.read_u8()? {
            0x7f => Ok(ValType::I32), 0x7e => Ok(ValType::I64),
            0x7d => Ok(ValType::F32), 0x7c => Ok(ValType::F64),
            0x7b => Ok(ValType::V128), 0x70 => Ok(ValType::FuncRef),
            0x6f => Ok(ValType::ExternRef),
            _ => Err(Error::new(at, "invalid or unsupported value type")),
        }
    }
    pub fn read_ref_type(&mut self) -> Result<ValType> {
        let at = self.offset(); let t = self.read_val_type()?;
        if !t.is_ref() { return Err(Error::new(at, "expected reference type")); } Ok(t)
    }
    pub fn read_block_type(&mut self) -> Result<BlockType> {
        match self.peek_u8()? {
            0x40 => { self.read_u8()?; Ok(BlockType::Empty) }
            0x7b..=0x7f | 0x70 | 0x6f => Ok(BlockType::Value(self.read_val_type()?)),
            _ => {
                let at = self.offset(); let n = self.read_i33()?;
                if n < 0 { Err(Error::new(at, "invalid or unsupported block type")) } else { Ok(BlockType::TypeIndex(n as u32)) }
            }
        }
    }
    pub fn read_memarg(&mut self) -> Result<MemArg> {
        let at = self.offset(); let align = self.read_u32()?;
        if align >= 32 { return Err(Error::new(at, "invalid or unsupported memory argument flags")); }
        Ok(MemArg { align, offset: self.read_u32()? })
    }
    fn zero(&mut self) -> Result<()> {
        let at = self.offset(); if self.read_u8()? != 0 { Err(Error::new(at, "expected reserved zero byte")) } else { Ok(()) }
    }
    fn vector<T>(&mut self, mut f: impl FnMut(&mut Self) -> Result<T>) -> Result<Vec<T>> {
        let at = self.offset(); let n = self.read_u32()? as usize;
        // All vectors decoded by this helper have elements of at least one byte.
        if n > self.remaining() { return Err(Error::new(at, "vector length exceeds remaining input")); }
        let mut out = Vec::new();
        out.try_reserve(n).map_err(|_| Error::new(at, "vector allocation failed"))?;
        for _ in 0..n { out.push(f(self)?); } Ok(out)
    }
    fn array16(&mut self) -> Result<[u8; 16]> { Ok(self.read_bytes(16)?.try_into().unwrap()) }
    fn lane(&mut self, lanes: u8) -> Result<u8> {
        let at = self.offset(); let lane = self.read_u8()?;
        if lane >= lanes { return Err(Error::new(at, "lane index out of range")); } Ok(lane)
    }
    /// Decode one 2.0 instruction, including all immediate bytes. Structured
    /// instructions are emitted as separate block/else/end instructions.
    pub fn read_instruction(&mut self) -> Result<Instruction> {
        use Immediate::*;
        let at = self.offset(); let opcode = self.read_u8()?;
        let mut subopcode = Option::None;
        let immediate = match opcode {
            0x00 | 0x01 | 0x05 | 0x0b | 0x0f | 0x1a | 0x1b | 0x45..=0xc4 | 0xd1 => None,
            0x02..=0x04 => Block(self.read_block_type()?),
            0x0c | 0x0d | 0x10 | 0x20..=0x26 | 0xd2 => Index(self.read_u32()?),
            0x0e => { let targets = self.vector(|r| r.read_u32())?; let default = self.read_u32()?; BrTable { targets, default } },
            0x11 => { let ty = self.read_u32()?; self.zero()?; IndexPair(ty, 0) },
            0x1c => Types(self.vector(|r| r.read_val_type())?),
            0x28..=0x3e => MemArg(self.read_memarg()?),
            0x3f | 0x40 => { self.zero()?; None },
            0x41 => I32(self.read_i32()?), 0x42 => I64(self.read_i64()?),
            0x43 => F32(self.read_f32_bits()?), 0x44 => F64(self.read_f64_bits()?),
            0xd0 => RefType(self.read_ref_type()?),
            0xfc => {
                let op = self.read_u32()?; subopcode = Some(op);
                match op {
                    0..=7 => None,
                    8 => { let data = self.read_u32()?; self.zero()?; IndexPair(data, 0) },
                    9 => Index(self.read_u32()?),
                    10 => { self.zero()?; self.zero()?; IndexPair(0, 0) },
                    11 => { self.zero()?; Index(0) },
                    12 | 14 => IndexPair(self.read_u32()?, self.read_u32()?),
                    13 | 15..=17 => Index(self.read_u32()?),
                    _ => return Err(Error::new(at, format!("unsupported 0xfc opcode {op}"))),
                }
            }
            0xfd => {
                let op = self.read_u32()?; subopcode = Some(op);
                match op {
                    0..=11 | 92 | 93 => MemArg(self.read_memarg()?),
                    12 => V128(self.array16()?),
                    13 => { let lanes = self.array16()?; if lanes.iter().any(|&l| l >= 32) { return Err(Error::new(at, "shuffle lane index out of range")); } Shuffle(lanes) },
                    21..=23 => Lane(self.lane(16)?), 24..=26 => Lane(self.lane(8)?),
                    27..=28 | 31..=32 => Lane(self.lane(4)?), 29..=30 | 33..=34 => Lane(self.lane(2)?),
                    84..=91 => { let mem = self.read_memarg()?; let lanes = 16 >> ((op - 84) % 4); MemLane(mem, self.lane(lanes as u8)?) },
                    14..=20 | 35..=83 | 94..=153 | 155..=161 | 163..=164 |
                    167..=174 | 177 | 181..=186 | 188..=193 | 195..=196 |
                    199..=206 | 209 | 213..=225 | 227..=237 | 239..=255 => None,
                    _ => return Err(Error::new(at, format!("unsupported 0xfd opcode {op}"))),
                }
            }
            _ => return Err(Error::new(at, format!("invalid or unsupported opcode 0x{opcode:02x}"))),
        };
        Ok(Instruction { opcode, subopcode, immediate })
    }
}

fn limits(r: &mut Reader<'_>, memory: bool) -> Result<Limits> {
    let at = r.offset();
    let flag = r.read_u8()?;
    if flag > 1 { return Err(Error::new(at, "unsupported limits flags (shared memory, memory64, etc.)")); }
    let min = r.read_u32()?;
    let max = if flag == 1 { Some(r.read_u32()?) } else { None };
    if max.map_or(false, |m| m < min) { return Err(Error::new(at, "maximum limit is smaller than minimum")); }
    if memory && (min > 65536 || max.map_or(false, |m| m > 65536)) { return Err(Error::new(at, "memory exceeds 65536 pages")); }
    Ok(Limits { min, max })
}
fn table_type(r: &mut Reader<'_>) -> Result<TableType> { Ok(TableType { element: r.read_ref_type()?, limits: limits(r, false)? }) }
fn memory_type(r: &mut Reader<'_>) -> Result<MemoryType> { Ok(MemoryType { limits: limits(r, true)? }) }
fn global_type(r: &mut Reader<'_>) -> Result<GlobalType> {
    let ty = r.read_val_type()?; let at = r.offset();
    let mutable = match r.read_u8()? { 0 => false, 1 => true, _ => return Err(Error::new(at, "invalid mutability")) };
    Ok(GlobalType { ty, mutable })
}
fn const_expr(r: &mut Reader<'_>) -> Result<ConstExpr> {
    use Immediate::*;
    let start = r.position(); let at = r.offset(); let inst = r.read_instruction()?;
    let op = match (inst.opcode, inst.subopcode, inst.immediate) {
        (0x41, _, I32(v)) => ConstOp::I32(v), (0x42, _, I64(v)) => ConstOp::I64(v),
        (0x43, _, F32(v)) => ConstOp::F32(v), (0x44, _, F64(v)) => ConstOp::F64(v),
        (0xfd, Some(12), V128(v)) => ConstOp::V128(v),
        (0x23, _, Index(v)) => ConstOp::GlobalGet(v),
        (0xd0, _, RefType(v)) => ConstOp::RefNull(v), (0xd2, _, Index(v)) => ConstOp::RefFunc(v),
        _ => return Err(Error::new(at, "invalid or unsupported constant expression")),
    };
    if r.read_u8()? != 0x0b { return Err(Error::new(r.offset() - 1, "constant expression must end after one instruction")); }
    Ok(ConstExpr { op, bytes: r.bytes[start..r.position()].to_vec() })
}
fn element(r: &mut Reader<'_>) -> Result<Element> {
    let at = r.offset(); let flags = r.read_u32()?;
    if flags > 3 { return Err(Error::new(at, "reference-expression element segments are outside the supported scope")); }
    let mode = match flags {
        0 | 4 => ElementMode::Active { table: 0, offset: const_expr(r)? },
        2 | 6 => ElementMode::Active { table: r.read_u32()?, offset: const_expr(r)? },
        1 | 5 => ElementMode::Passive,
        _ => ElementMode::Declarative,
    };
    let ty = if flags == 0 || flags == 4 { ValType::FuncRef }
        else if flags < 4 { r.zero()?; ValType::FuncRef }
        else { r.read_ref_type()? };
    let init = if flags < 4 { r.vector(|r| Ok(ElementInit::Func(r.read_u32()?)))? }
        else { r.vector(|r| Ok(ElementInit::Expr(const_expr(r)?)))? };
    Ok(Element { ty, mode, init })
}
fn data(r: &mut Reader<'_>) -> Result<Data> {
    let at = r.offset();
    let mode = match r.read_u32()? {
        0 => DataMode::Active { memory: 0, offset: const_expr(r)? },
        1 => DataMode::Passive,
        2 => DataMode::Active { memory: r.read_u32()?, offset: const_expr(r)? },
        _ => return Err(Error::new(at, "invalid data segment flags")),
    };
    let n = r.read_u32()? as usize;
    Ok(Data { mode, bytes: r.read_bytes(n)?.to_vec() })
}

/// Decode a core-version-1 binary using the WebAssembly 2.0 feature set.
pub fn parse(bytes: &[u8]) -> Result<Module> {
    let mut r = Reader::new(bytes);
    if r.read_bytes(4)? != b"\0asm" { return Err(Error::new(0, "bad WebAssembly magic")); }
    if r.read_bytes(4)? != [1, 0, 0, 0] { return Err(Error::new(4, "unsupported WebAssembly version")); }
    let mut module = Module::default();
    let mut func_types = Vec::new();
    let mut bodies: Vec<(Vec<LocalDecl>, Vec<u8>, usize)> = Vec::new();
    let mut previous_order = 0u8;
    while !r.is_empty() {
        let section_at = r.offset(); let id = r.read_u8()?;
        let size = r.read_u32()? as usize;
        let base = r.offset(); let contents = r.read_bytes(size)?;
        let mut s = Reader::with_offset(contents, base);
        if id != 0 {
            let order = match id { 1..=9 => id, 12 => 10, 10 => 11, 11 => 12, _ => return Err(Error::new(section_at, "unknown or unsupported section")) };
            if order <= previous_order { return Err(Error::new(section_at, "duplicate or out-of-order section")); }
            previous_order = order;
        }
        match id {
            0 => { let name = s.read_name()?; let data = s.read_bytes(s.remaining())?.to_vec(); module.customs.push(CustomSection { name, data }); }
            1 => { module.types = s.vector(|r| {
                let at = r.offset(); if r.read_u8()? != 0x60 { return Err(Error::new(at, "unsupported type definition")); }
                Ok(FuncType { params: r.vector(|r| r.read_val_type())?, results: r.vector(|r| r.read_val_type())? })
            })?; }
            2 => { module.imports = s.vector(|r| {
                let module = r.read_name()?; let name = r.read_name()?; let at = r.offset();
                let desc = match r.read_u8()? {
                    0 => ImportDesc::Func(r.read_u32()?), 1 => ImportDesc::Table(table_type(r)?),
                    2 => ImportDesc::Memory(memory_type(r)?), 3 => ImportDesc::Global(global_type(r)?),
                    _ => return Err(Error::new(at, "invalid or unsupported import kind")),
                }; Ok(Import { module, name, desc })
            })?; }
            3 => { func_types = s.vector(|r| r.read_u32())?; }
            4 => { module.tables = s.vector(table_type)?; }
            5 => { module.memories = s.vector(memory_type)?; }
            6 => { module.globals = s.vector(|r| Ok(Global { ty: global_type(r)?, init: const_expr(r)? }))?; }
            7 => { module.exports = s.vector(|r| {
                let name = r.read_name()?; let at = r.offset(); let kind = r.read_u8()?; let index = r.read_u32()?;
                let desc = match kind { 0 => ExportDesc::Func(index), 1 => ExportDesc::Table(index), 2 => ExportDesc::Memory(index), 3 => ExportDesc::Global(index), _ => return Err(Error::new(at, "invalid or unsupported export kind")) };
                Ok(Export { name, desc })
            })?; }
            8 => { module.start = Some(s.read_u32()?); }
            9 => { module.elements = s.vector(element)?; }
            10 => { bodies = s.vector(|r| {
                let size = r.read_u32()? as usize; let base = r.offset(); let bytes = r.read_bytes(size)?;
                let mut body = Reader::with_offset(bytes, base);
                let mut total = 0u32;
                let locals = body.vector(|r| {
                    let at = r.offset(); let count = r.read_u32()?; let ty = r.read_val_type()?;
                    total = total.checked_add(count).ok_or_else(|| Error::new(at, "too many locals"))?;
                    Ok(LocalDecl { count, ty })
                })?;
                let body_offset = body.offset(); let instructions = body.read_bytes(body.remaining())?.to_vec();
                Ok((locals, instructions, body_offset))
            })?; }
            11 => { module.data = s.vector(data)?; }
            12 => { module.data_count = Some(s.read_u32()?); }
            _ => unreachable!(),
        }
        if !s.is_empty() { return Err(Error::new(s.offset(), "section has trailing bytes")); }
    }
    if func_types.len() != bodies.len() { return Err(Error::new(bytes.len(), "function and code section lengths differ")); }
    module.functions = func_types.into_iter().zip(bodies).map(|(type_index, (locals, body, body_offset))| Function { type_index, locals, body, body_offset }).collect();
    if module.data_count.map_or(false, |n| n as usize != module.data.len()) { return Err(Error::new(bytes.len(), "data count does not match data section")); }
    check_module(&module)?;
    Ok(module)
}

/// Module-level index spaces, constructed once to keep validation linear even
/// when a module contains many imports and instructions.
struct Context<'a> {
    types: &'a [FuncType],
    funcs: Vec<&'a FuncType>,
    tables: Vec<TableType>,
    memories: Vec<MemoryType>,
    globals: Vec<GlobalType>,
    imported_globals: usize,
    elements: &'a [Element],
    data_count: Option<u32>,
}

fn check_const(ctx: &Context<'_>, expr: &ConstExpr, expected: ValType) -> Result<()> {
    let ty = match expr.op {
        ConstOp::I32(_) => ValType::I32, ConstOp::I64(_) => ValType::I64,
        ConstOp::F32(_) => ValType::F32, ConstOp::F64(_) => ValType::F64,
        ConstOp::V128(_) => ValType::V128, ConstOp::RefNull(t) => t,
        ConstOp::RefFunc(idx) => {
            if idx as usize >= ctx.funcs.len() { return Err(Error::new(0, "constant expression function index out of bounds")); }
            ValType::FuncRef
        }
        ConstOp::GlobalGet(idx) => {
            if idx as usize >= ctx.imported_globals { return Err(Error::new(0, "constant global.get must reference an imported global")); }
            let global = ctx.globals[idx as usize];
            if global.mutable { return Err(Error::new(0, "constant global.get references a mutable global")); }
            global.ty
        }
    };
    if ty != expected { return Err(Error::new(0, "constant expression type mismatch")); }
    Ok(())
}

fn check_module(module: &Module) -> Result<()> {
    let mut ctx = Context {
        types: &module.types, funcs: Vec::new(), tables: Vec::new(), memories: Vec::new(),
        globals: Vec::new(), imported_globals: 0, elements: &module.elements, data_count: module.data_count,
    };
    for import in &module.imports {
        match import.desc {
            ImportDesc::Func(idx) => ctx.funcs.push(module.types.get(idx as usize).ok_or_else(|| Error::new(0, "import function type index out of bounds"))?),
            ImportDesc::Table(t) => ctx.tables.push(t),
            ImportDesc::Memory(t) => ctx.memories.push(t),
            ImportDesc::Global(t) => ctx.globals.push(t),
        }
    }
    ctx.imported_globals = ctx.globals.len();
    ctx.tables.extend(module.tables.iter().copied());
    ctx.memories.extend(module.memories.iter().copied());
    ctx.globals.extend(module.globals.iter().map(|g| g.ty));
    for func in &module.functions {
        ctx.funcs.push(module.types.get(func.type_index as usize).ok_or_else(|| Error::new(func.body_offset, "function type index out of bounds"))?);
    }
    if ctx.memories.len() > 1 { return Err(Error::new(0, "multiple memories are outside WebAssembly 2.0 scope")); }
    for global in &module.globals { check_const(&ctx, &global.init, global.ty.ty)?; }
    let mut names = HashSet::new();
    for export in &module.exports {
        if !names.insert(&export.name) { return Err(Error::new(0, "duplicate export name")); }
        let valid = match export.desc {
            ExportDesc::Func(i) => (i as usize) < ctx.funcs.len(),
            ExportDesc::Table(i) => (i as usize) < ctx.tables.len(),
            ExportDesc::Memory(i) => (i as usize) < ctx.memories.len(),
            ExportDesc::Global(i) => (i as usize) < ctx.globals.len(),
        };
        if !valid { return Err(Error::new(0, "export index out of bounds")); }
    }
    if let Some(start) = module.start {
        let ty = ctx.funcs.get(start as usize).ok_or_else(|| Error::new(0, "start function index out of bounds"))?;
        if !ty.params.is_empty() || !ty.results.is_empty() { return Err(Error::new(0, "start function must have type [] -> []")); }
    }
    for element in &module.elements {
        if let ElementMode::Active { table, offset } = &element.mode {
            let t = ctx.tables.get(*table as usize).ok_or_else(|| Error::new(0, "element table index out of bounds"))?;
            if t.element != element.ty { return Err(Error::new(0, "element segment/table type mismatch")); }
            check_const(&ctx, offset, ValType::I32)?;
        }
        for item in &element.init {
            match item {
                ElementInit::Func(i) => {
                    if *i as usize >= ctx.funcs.len() { return Err(Error::new(0, "element function index out of bounds")); }
                }
                ElementInit::Expr(e) => check_const(&ctx, e, element.ty)?,
            }
        }
    }
    for data in &module.data {
        if let DataMode::Active { memory, offset } = &data.mode {
            if *memory as usize >= ctx.memories.len() { return Err(Error::new(0, "data memory index out of bounds")); }
            check_const(&ctx, offset, ValType::I32)?;
        }
    }
    for func in &module.functions { check_body(&ctx, func)?; }
    Ok(())
}

// The maximum valid log2 alignment, by memory opcode.
fn scalar_align(op: u8) -> u32 {
    match op {
        0x28 | 0x2a | 0x34 | 0x35 | 0x36 | 0x38 | 0x3e => 2,
        0x29 | 0x2b | 0x37 | 0x39 => 3,
        0x2e | 0x2f | 0x32 | 0x33 | 0x3b | 0x3d => 1,
        _ => 0,
    }
}
fn simd_align(op: u32) -> u32 {
    match op {
        0 | 11 => 4, 1..=6 | 10 | 87 | 91 | 93 => 3,
        9 | 86 | 90 | 92 => 2, 8 | 85 | 89 => 1, _ => 0,
    }
}

fn check_body(ctx: &Context<'_>, func: &Function) -> Result<()> {
    let ty = &ctx.types[func.type_index as usize];
    let local_count = (ty.params.len() as u64) + func.local_count() as u64;
    if local_count > u32::MAX as u64 { return Err(Error::new(func.body_offset, "too many parameters and locals")); }
    let mut r = func.reader();
    // 0=root/block/loop, 1=if without else, 2=if with else.
    let mut control = vec![0u8];
    loop {
        let at = r.offset();
        let instruction = r.read_instruction()?;
        let op = instruction.opcode;
        use Immediate::*;
        match (&instruction.immediate, op) {
            (Block(BlockType::TypeIndex(i)), 0x02..=0x04) if *i as usize >= ctx.types.len() => return Err(Error::new(at, "block type index out of bounds")),
            (_, 0x02 | 0x03) => control.push(0),
            (_, 0x04) => control.push(1),
            (_, 0x05) => {
                if control.last() != Some(&1) { return Err(Error::new(at, "unexpected else")); }
                *control.last_mut().unwrap() = 2;
            }
            (_, 0x0b) => {
                control.pop();
                if control.is_empty() {
                    if !r.is_empty() { return Err(Error::new(r.offset(), "bytes after function end")); }
                    break;
                }
            }
            (Index(i), 0x0c | 0x0d) if *i as usize >= control.len() => return Err(Error::new(at, "branch label out of bounds")),
            (BrTable { targets, default }, 0x0e) if *default as usize >= control.len() || targets.iter().any(|i| *i as usize >= control.len()) => return Err(Error::new(at, "branch table label out of bounds")),
            (Index(i), 0x10 | 0xd2) if *i as usize >= ctx.funcs.len() => return Err(Error::new(at, "function index out of bounds")),
            (IndexPair(t, table), 0x11) => {
                if *t as usize >= ctx.types.len() { return Err(Error::new(at, "call_indirect type index out of bounds")); }
                let table = ctx.tables.get(*table as usize).ok_or_else(|| Error::new(at, "call_indirect table index out of bounds"))?;
                if table.element != ValType::FuncRef { return Err(Error::new(at, "call_indirect requires funcref table")); }
            }
            (Types(types), 0x1c) if types.len() != 1 => return Err(Error::new(at, "typed select requires exactly one type")),
            (Index(i), 0x20..=0x22) if *i as u64 >= local_count => return Err(Error::new(at, "local index out of bounds")),
            (Index(i), 0x23 | 0x24) => {
                let global = ctx.globals.get(*i as usize).ok_or_else(|| Error::new(at, "global index out of bounds"))?;
                if op == 0x24 && !global.mutable { return Err(Error::new(at, "global.set on immutable global")); }
            }
            (Index(i), 0x25 | 0x26) if *i as usize >= ctx.tables.len() => return Err(Error::new(at, "table index out of bounds")),
            (MemArg(m), 0x28..=0x3e) => {
                if ctx.memories.is_empty() { return Err(Error::new(at, "memory instruction without memory")); }
                if m.align > scalar_align(op) { return Err(Error::new(at, "memory alignment too large")); }
            }
            (_, 0x3f | 0x40) if ctx.memories.is_empty() => return Err(Error::new(at, "memory instruction without memory")),
            (_, 0xfc) => {
                match (instruction.subopcode.unwrap(), &instruction.immediate) {
                    (8, IndexPair(i, _)) | (9, Index(i)) => {
                        let n = ctx.data_count.ok_or_else(|| Error::new(at, "data index requires a data count section"))?;
                        if *i >= n { return Err(Error::new(at, "data index out of bounds")); }
                        if instruction.subopcode == Some(8) && ctx.memories.is_empty() { return Err(Error::new(at, "memory.init without memory")); }
                    }
                    (10 | 11, _) if ctx.memories.is_empty() => return Err(Error::new(at, "bulk memory instruction without memory")),
                    (12, IndexPair(elem, table)) => {
                        let e = ctx.elements.get(*elem as usize).ok_or_else(|| Error::new(at, "element index out of bounds"))?;
                        let t = ctx.tables.get(*table as usize).ok_or_else(|| Error::new(at, "table index out of bounds"))?;
                        if e.ty != t.element { return Err(Error::new(at, "table.init type mismatch")); }
                    }
                    (13, Index(i)) if *i as usize >= ctx.elements.len() => return Err(Error::new(at, "element index out of bounds")),
                    (14, IndexPair(dst, src)) => {
                        let dst = ctx.tables.get(*dst as usize).ok_or_else(|| Error::new(at, "table index out of bounds"))?;
                        let src = ctx.tables.get(*src as usize).ok_or_else(|| Error::new(at, "table index out of bounds"))?;
                        if dst.element != src.element { return Err(Error::new(at, "table.copy type mismatch")); }
                    }
                    (15..=17, Index(i)) if *i as usize >= ctx.tables.len() => return Err(Error::new(at, "table index out of bounds")),
                    _ => {}
                }
            }
            (_, 0xfd) => {
                let memarg = match &instruction.immediate { MemArg(m) | MemLane(m, _) => Some(m), _ => Option::None };
                if let Some(m) = memarg {
                    if ctx.memories.is_empty() { return Err(Error::new(at, "SIMD memory instruction without memory")); }
                    if m.align > simd_align(instruction.subopcode.unwrap()) { return Err(Error::new(at, "SIMD memory alignment too large")); }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leb(mut n: usize) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let mut b = (n & 0x7f) as u8; n >>= 7;
            if n != 0 { b |= 0x80; }
            out.push(b);
            if n == 0 { return out; }
        }
    }
    fn header() -> Vec<u8> { b"\0asm\x01\0\0\0".to_vec() }
    fn section(module: &mut Vec<u8>, id: u8, bytes: &[u8]) {
        module.push(id); module.extend(leb(bytes.len())); module.extend(bytes);
    }
    fn one_function(body: &[u8]) -> Vec<u8> {
        let mut module = header();
        section(&mut module, 1, &[1, 0x60, 0, 0]);
        section(&mut module, 3, &[1, 0]);
        let mut code = vec![1]; code.extend(leb(body.len())); code.extend(body);
        section(&mut module, 10, &code);
        module
    }

    #[test]
    fn leb_limits_and_noncanonical() {
        assert_eq!(Reader::new(&[0xff, 0xff, 0xff, 0xff, 0x0f]).read_u32().unwrap(), u32::MAX);
        assert!(Reader::new(&[0xff, 0xff, 0xff, 0xff, 0x10]).read_u32().is_err());
        assert!(Reader::new(&[0x80, 0x80, 0x80, 0x80, 0x80, 0]).read_u32().is_err());
        assert_eq!(Reader::new(&[0x80, 0]).read_u32().unwrap(), 0);
        assert_eq!(Reader::new(&[0xff, 0x7f]).read_i32().unwrap(), -1);
        assert_eq!(Reader::new(&[0x80, 0x80, 0x80, 0x80, 0x78]).read_i32().unwrap(), i32::MIN);
        assert_eq!(Reader::new(&[0xff, 0xff, 0xff, 0xff, 0x07]).read_i32().unwrap(), i32::MAX);
        assert!(Reader::new(&[0x80, 0x80, 0x80, 0x80, 0x08]).read_i32().is_err());
        assert_eq!(Reader::new(&[0xff, 0xff, 0xff, 0xff, 0x0f]).read_i33().unwrap(), u32::MAX as i64);
        assert_eq!(Reader::new(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x7f]).read_i64().unwrap(), i64::MIN);
    }

    #[test]
    fn body_structure_and_scope() {
        let valid = one_function(&[1, 2, 0x7f, 0x02, 0x40, 0x0c, 0, 0x0b, 0x0b]);
        let m = parse(&valid).unwrap();
        assert_eq!(m.functions[0].local_types().collect::<Vec<_>>(), [ValType::I32, ValType::I32]);
        assert_eq!(m.functions[0].reader().read_instruction().unwrap().immediate, Immediate::Block(BlockType::Empty));
        for body in [
            &[0, 0x02, 0x40, 0x0b][..],
            &[0, 0x05, 0x0b],
            &[0, 0x02, 0x40, 0x05, 0x0b, 0x0b],
            &[0, 0x0b, 0x01],
            &[0, 0x0c, 1, 0x0b],
            &[0, 0xfd, 0x9a, 0x01, 0x0b],
            &[0, 0xfd, 0x80, 0x02, 0x0b],
            &[0, 0xfe, 0, 0x0b],
            &[0, 0xfc, 9, 0, 0x0b],
            &[0, 0x3f, 0x80, 0, 0x0b],
        ] { assert!(parse(&one_function(body)).is_err(), "accepted {body:?}"); }
    }

    #[test]
    fn section_order_size_names_and_counts() {
        let mut module = header(); section(&mut module, 0, &[0]); section(&mut module, 1, &[0]);
        section(&mut module, 12, &[0]); section(&mut module, 10, &[0]); section(&mut module, 11, &[0]);
        assert!(parse(&module).is_ok());
        let mut module = header(); section(&mut module, 10, &[0]); section(&mut module, 12, &[0]);
        assert!(parse(&module).is_err());
        let mut module = header(); section(&mut module, 1, &[0]); section(&mut module, 1, &[0]);
        assert!(parse(&module).is_err());
        let mut module = header(); section(&mut module, 0, &[1, 0xff]);
        assert!(parse(&module).is_err());
        let mut module = header(); section(&mut module, 3, &[1, 0]);
        assert!(parse(&module).is_err());
        let mut module = header(); section(&mut module, 12, &[1]);
        assert!(parse(&module).is_err());
        let mut module = header(); section(&mut module, 5, &[1, 0, 0]); section(&mut module, 5, &[1, 0, 0]);
        assert!(parse(&module).is_err());
        let mut module = header(); module.extend([1, 3, 0]);
        assert!(parse(&module).is_err());
    }

    #[test]
    fn simd_immediates_and_reserved_opcodes() {
        let mut r = Reader::new(&[0xfd, 0x0c, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
        assert_eq!(r.read_instruction().unwrap().immediate, Immediate::V128([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]));
        assert!(Reader::new(&[0xfd, 21, 16]).read_instruction().is_err());
        assert!(Reader::new(&[0xfd, 13, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 32]).read_instruction().is_err());
        for op in 0..=255 {
            let mut bytes = vec![0xfd]; bytes.extend(leb(op)); bytes.extend([0; 20]);
            let reserved = [154, 162, 165, 166, 175, 176, 178, 179, 180, 187, 194, 197, 198, 207, 208, 210, 211, 212, 226, 238].contains(&op);
            assert_eq!(Reader::new(&bytes).read_instruction().is_err(), reserved, "SIMD opcode {op}");
        }
    }

    #[test]
    fn generated_corpus_when_requested() {
        if let Some(dir) = std::env::var_os("WASM_PARSER_CORPUS") {
            let mut count = 0;
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.extension().and_then(|s| s.to_str()) != Some("wasm") { continue; }
                let bytes = std::fs::read(&path).unwrap();
                parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                count += 1;
            }
            assert_ne!(count, 0);
        }
    }

    #[test]
    fn shipped_payloads() {
        for path in ["payloads/minicelt/minicelt.wasm", "payloads/minisilk/minisilk.wasm", "payloads/multi-value/multi-value.wasm", "payloads/qoa-simd/qoa-simd.wasm"] {
            let bytes = std::fs::read(path).unwrap();
            parse(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
        }
    }
}

// WebAssembly binary parser for the supported feature scope:
// WebAssembly 2.0 core without reference types, plus the 128-bit SIMD proposal.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ValType {
    I32,
    I64,
    F32,
    F64,
    V128,
}

impl ValType {
    pub fn name(&self) -> &'static str {
        match self {
            ValType::I32 => "i32",
            ValType::I64 => "i64",
            ValType::F32 => "f32",
            ValType::F64 => "f64",
            ValType::V128 => "v128",
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FuncType {
    pub params: Vec<ValType>,
    pub results: Vec<ValType>,
}

#[derive(Clone, Debug)]
pub struct Import {
    pub module: String,
    pub name: String,
    pub ty: u32,
}

#[derive(Clone, Debug)]
pub struct GlobalDef {
    pub ty: ValType,
    pub mutable: bool,
    pub init: Vec<Instr>,
}

#[derive(Clone, Debug)]
pub struct Export {
    pub name: String,
    pub kind: u8, // 0 func, 1 table, 2 mem, 3 global
    pub idx: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockType {
    Empty,
    Val(ValType),
    TypeIdx(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElemMode {
    Active,
    Passive,
    Declarative,
}

#[derive(Clone, Debug)]
pub struct ElemSeg {
    pub mode: ElemMode,
    pub table: u32,
    pub offset: Vec<Instr>,
    pub funcs: Vec<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataMode {
    Active,
    Passive,
}

#[derive(Clone, Debug)]
pub struct DataSeg {
    pub mode: DataMode,
    pub mem: u32,
    pub offset: Vec<Instr>,
    pub data: Vec<u8>,
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub min: u64,
    pub max: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct FuncBody {
    pub locals: Vec<ValType>,
    pub body: Vec<Instr>,
}

#[derive(Clone, Debug)]
pub struct SimdOp {
    pub sub: u32,
    pub kind: SimdKind,
}

#[derive(Clone, Debug)]
pub enum SimdKind {
    Plain,
    Mem { align: u32, offset: u64 },
    MemLane { align: u32, offset: u64, lane: u8 },
    Lane { lane: u8 },
    Const([u8; 16]),
    Shuffle([u8; 16]),
}

#[derive(Clone, Debug)]
pub enum Instr {
    Unreachable,
    Nop,
    Block(BlockType),
    Loop(BlockType),
    If(BlockType),
    Else,
    End,
    Br(u32),
    BrIf(u32),
    BrTable(Vec<u32>, u32),
    Return,
    Call(u32),
    CallIndirect { ty: u32, table: u32 },
    Drop,
    Select,
    LocalGet(u32),
    LocalSet(u32),
    LocalTee(u32),
    GlobalGet(u32),
    GlobalSet(u32),
    Load { op: u8, align: u32, offset: u64 },
    Store { op: u8, align: u32, offset: u64 },
    MemorySize,
    MemoryGrow,
    I32Const(i32),
    I64Const(i64),
    F32Const(u32),
    F64Const(u64),
    Num(u16),
    TruncSat(u32),
    MemoryInit(u32),
    DataDrop(u32),
    MemoryCopy,
    MemoryFill,
    TableInit { elem: u32, table: u32 },
    ElemDrop(u32),
    TableCopy { dst: u32, src: u32 },
    Simd(SimdOp),
}

pub struct Module {
    pub types: Vec<FuncType>,
    pub imports: Vec<Import>,
    pub func_types: Vec<u32>, // type index for each defined function
    pub tables: Vec<Limits>,
    pub memories: Vec<Limits>,
    pub globals: Vec<GlobalDef>,
    pub exports: Vec<Export>,
    pub start: Option<u32>,
    pub elems: Vec<ElemSeg>,
    pub datas: Vec<DataSeg>,
    pub codes: Vec<FuncBody>,
    pub data_count: Option<u32>,
}

impl Module {
    pub fn num_funcs(&self) -> usize {
        self.imports.len() + self.func_types.len()
    }
    pub fn func_type_idx(&self, idx: u32) -> Option<u32> {
        let i = idx as usize;
        if i < self.imports.len() {
            Some(self.imports[i].ty)
        } else {
            self.func_types.get(i - self.imports.len()).copied()
        }
    }
    pub fn func_type(&self, idx: u32) -> Option<&FuncType> {
        let ti = self.func_type_idx(idx)?;
        self.types.get(ti as usize)
    }
    pub fn block_sig(&self, bt: BlockType) -> Option<(Vec<ValType>, Vec<ValType>)> {
        match bt {
            BlockType::Empty => Some((vec![], vec![])),
            BlockType::Val(v) => Some((vec![], vec![v])),
            BlockType::TypeIdx(i) => {
                let t = self.types.get(i as usize)?;
                Some((t.params.clone(), t.results.clone()))
            }
        }
    }
}

type R<T> = Result<T, String>;

pub struct Reader<'a> {
    pub data: &'a [u8],
    pub pos: usize,
    pub base: usize,
    pub end: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Reader { data, pos: 0, base: 0, end: data.len() }
    }

    fn err<T>(&self, msg: &str) -> R<T> {
        Err(format!("{} at offset 0x{:x}", msg, self.base + self.pos))
    }

    pub fn eof(&self) -> bool {
        self.pos >= self.end
    }

    pub fn u8(&mut self) -> R<u8> {
        if self.pos >= self.end {
            return self.err("unexpected end of section or function");
        }
        let b = self.data[self.pos];
        self.pos += 1;
        Ok(b)
    }


    pub fn bytes(&mut self, n: usize) -> R<&'a [u8]> {
        if self.end - self.pos < n {
            return self.err("unexpected end of section or function");
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    pub fn sub(&mut self, n: usize) -> R<Reader<'a>> {
        let start = self.pos;
        if self.end - self.pos < n {
            return self.err("length out of bounds");
        }
        self.pos += n;
        Ok(Reader { data: self.data, pos: start, base: self.base + start, end: start + n })
    }

    pub fn u32(&mut self) -> R<u32> {
        let mut result: u64 = 0;
        let mut shift = 0u32;
        loop {
            let b = self.u8()?;
            if shift == 28 {
                if b & 0x80 != 0 {
                    return self.err("integer representation too long");
                }
                if b & 0xf0 != 0 {
                    return self.err("integer too large");
                }
                result |= (b as u64) << shift;
                break;
            }
            result |= ((b & 0x7f) as u64) << shift;
            if b & 0x80 == 0 {
                break;
            }
            shift += 7;
        }
        Ok(result as u32)
    }


    pub fn s32(&mut self) -> R<i32> {
        let v = self.sleb(32)?;
        Ok(v as i32)
    }

    pub fn s64(&mut self) -> R<i64> {
        let v = self.sleb(64)?;
        Ok(v)
    }

    pub fn s33(&mut self) -> R<i64> {
        let v = self.sleb(33)?;
        Ok(v)
    }

    fn sleb(&mut self, bits: u32) -> R<i64> {
        let mut result: i64 = 0;
        let mut shift = 0u32;
        let max_bytes = (bits + 6) / 7;
        let mut count = 0;
        loop {
            let b = self.u8()?;
            count += 1;
            if count > max_bytes {
                return self.err("integer representation too long");
            }
            let low = (b & 0x7f) as i64;
            if shift < 64 {
                result |= low << shift;
            }
            shift += 7;
            if b & 0x80 == 0 {
                // sign extension check for the final byte
                if count == max_bytes {
                    let used = bits - (max_bytes - 1) * 7; // bits used in final byte
                    if used < 7 {
                        let v = (b & 0x7f) as i8;
                        // sign extend from 'used' bits within this byte
                        let shift_in_byte = used - 1;
                        let sign_bit = (v >> shift_in_byte) & 1;
                        let mask: i8 = if shift_in_byte >= 7 {
                            0
                        } else {
                            let hi = (1i16 << (7 - shift_in_byte)) as i8 - 1;
                            // bits above shift_in_byte must equal sign
                            let above = ((v as u8) >> used) & ((1u8 << (7 - used)) - 1);
                            let want = if sign_bit == 1 {
                                ((1u8 << (7 - used)) - 1) as u8
                            } else {
                                0u8
                            };
                            let _ = hi;
                            if above != want {
                                return self.err("integer too large");
                            }
                            0
                        };
                        let _ = mask;
                    }
                }
                if shift < 64 && (b & 0x40) != 0 {
                    result |= -1i64 << shift;
                }
                break;
            }
        }
        Ok(result)
    }

    pub fn name(&mut self) -> R<String> {
        let len = self.u32()? as usize;
        let bs = self.bytes(len)?;
        match std::str::from_utf8(bs) {
            Ok(s) => Ok(s.to_string()),
            Err(_) => self.err("malformed UTF-8 encoding"),
        }
    }

    pub fn valtype(&mut self) -> R<ValType> {
        let b = self.u8()?;
        match b {
            0x7f => Ok(ValType::I32),
            0x7e => Ok(ValType::I64),
            0x7d => Ok(ValType::F32),
            0x7c => Ok(ValType::F64),
            0x7b => Ok(ValType::V128),
            0x70 | 0x6f => self.err("reference types support is not enabled"),
            0x63 | 0x64 => self.err("unsupported reference type"),
            _ => self.err("malformed value type"),
        }
    }

    pub fn limits(&mut self, is_memory: bool) -> R<Limits> {
        let flags = self.u8()?;
        match flags {
            0x00 => {
                let min = if is_memory { self.u32()? as u64 } else { self.u32()? as u64 };
                Ok(Limits { min, max: None })
            }
            0x01 => {
                let min = self.u32()? as u64;
                let max = self.u32()? as u64;
                Ok(Limits { min, max: Some(max) })
            }
            0x02 => self.err("shared memory is not supported"),
            0x03 => self.err("shared memory is not supported"),
            0x04 | 0x05 | 0x06 | 0x07 => self.err("memory64 is not supported"),
            _ => self.err("malformed limits flags"),
        }
    }
}

fn read_section_vec_count(r: &mut Reader) -> R<u32> {
    r.u32()
}

pub fn parse_module(data: &[u8]) -> R<Module> {
    let mut r = Reader::new(data);
    let magic = r.bytes(4)?;
    if magic != b"\0asm" {
        return Err("magic header not detected".to_string());
    }
    let version = r.bytes(4)?;
    if version != [1, 0, 0, 0] {
        return Err("unknown binary version".to_string());
    }

    let mut m = Module {
        types: Vec::new(),
        imports: Vec::new(),
        func_types: Vec::new(),
        tables: Vec::new(),
        memories: Vec::new(),
        globals: Vec::new(),
        exports: Vec::new(),
        start: None,
        elems: Vec::new(),
        datas: Vec::new(),
        codes: Vec::new(),
        data_count: None,
    };

    let mut last_rank: i32 = 0;
    let mut seen_code = false;
    let mut seen_data = false;
    while !r.eof() {
        let id = r.u8()?;
        let size = r.u32()? as usize;
        let mut s = r.sub(size)?;
        if id != 0 {
            let rank = match id {
                1 => 1,
                2 => 2,
                3 => 3,
                4 => 4,
                5 => 5,
                6 => 6,
                7 => 7,
                8 => 8,
                9 => 9,
                12 => 10,
                10 => 11,
                11 => 12,
                13 => return Err("exceptions proposal is not enabled".to_string()),
                _ => return Err(format!("malformed section id {}", id)),
            };
            if rank <= last_rank {
                return Err(format!("unexpected section id {}", id));
            }
            last_rank = rank;
        }
        match id {
            0 => {
                // custom section: its name must be valid UTF-8; contents ignored
                let _ = s.name()?;
            }
            1 => {
                let n = read_section_vec_count(&mut s)?;
                for _ in 0..n {
                    let form = s.u8()?;
                    if form != 0x60 {
                        return Err("malformed type: expected func type".to_string());
                    }
                    let np = s.u32()?;
                    let mut params = Vec::with_capacity(np.min(1024) as usize);
                    for _ in 0..np {
                        params.push(s.valtype()?);
                    }
                    let nr = s.u32()?;
                    let mut results = Vec::with_capacity(nr.min(1024) as usize);
                    for _ in 0..nr {
                        results.push(s.valtype()?);
                    }
                    m.types.push(FuncType { params, results });
                }
            }
            2 => {
                let n = s.u32()?;
                for _ in 0..n {
                    let module = s.name()?;
                    let name = s.name()?;
                    let kind = s.u8()?;
                    match kind {
                        0x00 => {
                            let ty = s.u32()?;
                            m.imports.push(Import { module, name, ty });
                        }
                        0x01 | 0x02 | 0x03 | 0x04 => {
                            return Err(format!(
                                "unsupported import kind {} (only function imports are supported)",
                                kind
                            ));
                        }
                        _ => return Err("malformed import kind".to_string()),
                    }
                }
            }
            3 => {
                let n = s.u32()?;
                for _ in 0..n {
                    m.func_types.push(s.u32()?);
                }
            }
            4 => {
                let n = s.u32()?;
                if n > 1 {
                    return Err("multiple tables".to_string());
                }
                for _ in 0..n {
                    let et = s.u8()?;
                    if et != 0x70 {
                        return Err("unsupported table element type".to_string());
                    }
                    let lim = s.limits(false)?;
                    m.tables.push(lim);
                }
            }
            5 => {
                let n = s.u32()?;
                if n > 1 {
                    return Err("multiple memories".to_string());
                }
                for _ in 0..n {
                    let lim = s.limits(true)?;
                    m.memories.push(lim);
                }
            }
            6 => {
                let n = s.u32()?;
                for _ in 0..n {
                    let ty = s.valtype()?;
                    let mutable = match s.u8()? {
                        0 => false,
                        1 => true,
                        _ => return Err("malformed mutability".to_string()),
                    };
                    let init = parse_expr(&mut s)?;
                    m.globals.push(GlobalDef { ty, mutable, init });
                }
            }
            7 => {
                let n = s.u32()?;
                for _ in 0..n {
                    let name = s.name()?;
                    let kind = s.u8()?;
                    if kind > 3 {
                        return Err("malformed export kind".to_string());
                    }
                    let idx = s.u32()?;
                    m.exports.push(Export { name, kind, idx });
                }
            }
            8 => {
                m.start = Some(s.u32()?);
            }
            9 => {
                let n = s.u32()?;
                for _ in 0..n {
                    let flags = s.u32()?;
                    let (mode, table, offset, funcs) = match flags {
                        0 => {
                            let off = parse_expr(&mut s)?;
                            let cnt = s.u32()?;
                            let mut f = Vec::with_capacity(cnt.min(100000) as usize);
                            for _ in 0..cnt {
                                f.push(s.u32()?);
                            }
                            (ElemMode::Active, 0, off, f)
                        }
                        1 => {
                            let kind = s.u8()?;
                            if kind != 0 {
                                return Err("malformed element kind".to_string());
                            }
                            let cnt = s.u32()?;
                            let mut f = Vec::with_capacity(cnt.min(100000) as usize);
                            for _ in 0..cnt {
                                f.push(s.u32()?);
                            }
                            (ElemMode::Passive, 0, vec![], f)
                        }
                        2 => {
                            let table = s.u32()?;
                            let off = parse_expr(&mut s)?;
                            let kind = s.u8()?;
                            if kind != 0 {
                                return Err("malformed element kind".to_string());
                            }
                            let cnt = s.u32()?;
                            let mut f = Vec::with_capacity(cnt.min(100000) as usize);
                            for _ in 0..cnt {
                                f.push(s.u32()?);
                            }
                            (ElemMode::Active, table, off, f)
                        }
                        3 => {
                            let kind = s.u8()?;
                            if kind != 0 {
                                return Err("malformed element kind".to_string());
                            }
                            let cnt = s.u32()?;
                            let mut f = Vec::with_capacity(cnt.min(100000) as usize);
                            for _ in 0..cnt {
                                f.push(s.u32()?);
                            }
                            (ElemMode::Declarative, 0, vec![], f)
                        }
                        4..=7 => {
                            return Err("reference types support is not enabled".to_string());
                        }
                        _ => return Err("malformed element segment flags".to_string()),
                    };
                    m.elems.push(ElemSeg { mode, table, offset, funcs });
                }
            }
            12 => {
                m.data_count = Some(s.u32()?);
            }
            10 => {
                seen_code = true;
                let n = s.u32()?;
                for _ in 0..n {
                    let body_size = s.u32()? as usize;
                    let mut b = s.sub(body_size)?;
                    let mut locals: Vec<ValType> = Vec::new();
                    let nlocals = b.u32()?;
                    let mut total: u64 = 0;
                    for _ in 0..nlocals {
                        let cnt = b.u32()? as u64;
                        total += cnt;
                        if total > 1_000_000 {
                            return Err("too many locals".to_string());
                        }
                        let ty = b.valtype()?;
                        for _ in 0..cnt {
                            locals.push(ty);
                        }
                    }
                    let body = parse_expr(&mut b)?;
                    if !b.eof() {
                        return Err("section size mismatch".to_string());
                    }
                    m.codes.push(FuncBody { locals, body });
                }
            }
            11 => {
                seen_data = true;
                let n = s.u32()?;
                for _ in 0..n {
                    let flags = s.u32()?;
                    let (mode, mem, offset, data) = match flags {
                        0 => {
                            let off = parse_expr(&mut s)?;
                            let len = s.u32()? as usize;
                            let d = s.bytes(len)?.to_vec();
                            (DataMode::Active, 0, off, d)
                        }
                        1 => {
                            let len = s.u32()? as usize;
                            let d = s.bytes(len)?.to_vec();
                            (DataMode::Passive, 0, vec![], d)
                        }
                        2 => {
                            let mem = s.u32()?;
                            if mem != 0 {
                                return Err("multiple memories".to_string());
                            }
                            let off = parse_expr(&mut s)?;
                            let len = s.u32()? as usize;
                            let d = s.bytes(len)?.to_vec();
                            (DataMode::Active, 0, off, d)
                        }
                        _ => return Err("malformed data segment flags".to_string()),
                    };
                    m.datas.push(DataSeg { mode, mem, offset, data });
                }
            }
            _ => return Err(format!("malformed section id {}", id)),
        }
        if id != 0 && !s.eof() {
            return Err(format!("section size mismatch (section {})", id));
        }
    }
    let _ = seen_code;
    let _ = seen_data;
    Ok(m)
}

fn parse_expr(r: &mut Reader) -> R<Vec<Instr>> {
    let mut out = Vec::new();
    let mut depth: i64 = 1;
    loop {
        if r.eof() {
            return Err("unexpected end of function body".to_string());
        }
        let ins = parse_instr(r)?;
        match &ins {
            Instr::Block(_) | Instr::Loop(_) | Instr::If(_) => depth += 1,
            Instr::End => {
                depth -= 1;
                out.push(ins);
                if depth == 0 {
                    break;
                }
                continue;
            }
            _ => {}
        }
        out.push(ins);
    }
    Ok(out)
}

fn block_type(r: &mut Reader) -> R<BlockType> {
    let v = r.s33()?;
    if v >= 0 {
        if v > u32::MAX as i64 {
            return Err("type index too large".to_string());
        }
        Ok(BlockType::TypeIdx(v as u32))
    } else {
        match v {
            -64 => Ok(BlockType::Empty),
            -1 => Ok(BlockType::Val(ValType::I32)),
            -2 => Ok(BlockType::Val(ValType::I64)),
            -3 => Ok(BlockType::Val(ValType::F32)),
            -4 => Ok(BlockType::Val(ValType::F64)),
            -5 => Ok(BlockType::Val(ValType::V128)),
            -16 => Err("reference types support is not enabled".to_string()),
            -17 => Err("reference types support is not enabled".to_string()),
            _ => Err(format!("malformed block type 0x{:x}", v)),
        }
    }
}

fn memarg(r: &mut Reader) -> R<(u32, u64)> {
    let align = r.u32()?;
    let offset = r.u32()? as u64;
    Ok((align, offset))
}

fn parse_instr(r: &mut Reader) -> R<Instr> {
    let op = r.u8()?;
    Ok(match op {
        0x00 => Instr::Unreachable,
        0x01 => Instr::Nop,
        0x02 => Instr::Block(block_type(r)?),
        0x03 => Instr::Loop(block_type(r)?),
        0x04 => Instr::If(block_type(r)?),
        0x05 => Instr::Else,
        0x0b => Instr::End,
        0x0c => Instr::Br(r.u32()?),
        0x0d => Instr::BrIf(r.u32()?),
        0x0e => {
            let cnt = r.u32()?;
            let mut v = Vec::with_capacity(cnt.min(100000) as usize);
            for _ in 0..cnt {
                v.push(r.u32()?);
            }
            let default = r.u32()?;
            Instr::BrTable(v, default)
        }
        0x0f => Instr::Return,
        0x10 => Instr::Call(r.u32()?),
        0x11 => {
            let ty = r.u32()?;
            let table = r.u32()?;
            Instr::CallIndirect { ty, table }
        }
        0x12 | 0x13 => return Err("tail calls support is not enabled".to_string()),
        0x14 | 0x15 => return Err("typed function references are not enabled".to_string()),
        0x1a => Instr::Drop,
        0x1b => Instr::Select,
        0x1c => return Err("reference types support is not enabled".to_string()),
        0x20 => Instr::LocalGet(r.u32()?),
        0x21 => Instr::LocalSet(r.u32()?),
        0x22 => Instr::LocalTee(r.u32()?),
        0x23 => Instr::GlobalGet(r.u32()?),
        0x24 => Instr::GlobalSet(r.u32()?),
        0x25 | 0x26 => return Err("reference types support is not enabled".to_string()),
        0x28..=0x35 => {
            let (align, offset) = memarg(r)?;
            Instr::Load { op, align, offset }
        }
        0x36..=0x3e => {
            let (align, offset) = memarg(r)?;
            Instr::Store { op, align, offset }
        }
        0x3f => {
            let mem = r.u32()?;
            if mem != 0 {
                return Err("multiple memories".to_string());
            }
            Instr::MemorySize
        }
        0x40 => {
            let mem = r.u32()?;
            if mem != 0 {
                return Err("multiple memories".to_string());
            }
            Instr::MemoryGrow
        }
        0x41 => Instr::I32Const(r.s32()?),
        0x42 => Instr::I64Const(r.s64()?),
        0x43 => {
            let b = r.bytes(4)?;
            Instr::F32Const(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        }
        0x44 => {
            let b = r.bytes(8)?;
            Instr::F64Const(u64::from_le_bytes([
                b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
            ]))
        }
        0x45..=0xc4 => Instr::Num(op as u16),
        0xd0..=0xd2 => return Err("reference types support is not enabled".to_string()),
        0xfc => {
            let sub = r.u32()?;
            match sub {
                0..=7 => Instr::TruncSat(sub),
                8 => {
                    let data = r.u32()?;
                    let mem = r.u32()?;
                    if mem != 0 {
                        return Err("multiple memories".to_string());
                    }
                    Instr::MemoryInit(data)
                }
                9 => Instr::DataDrop(r.u32()?),
                10 => {
                    let d = r.u32()?;
                    let s = r.u32()?;
                    if d != 0 || s != 0 {
                        return Err("multiple memories".to_string());
                    }
                    Instr::MemoryCopy
                }
                11 => {
                    let mem = r.u32()?;
                    if mem != 0 {
                        return Err("multiple memories".to_string());
                    }
                    Instr::MemoryFill
                }
                12 => {
                    let elem = r.u32()?;
                    let table = r.u32()?;
                    Instr::TableInit { elem, table }
                }
                13 => Instr::ElemDrop(r.u32()?),
                14 => {
                    let dst = r.u32()?;
                    let src = r.u32()?;
                    Instr::TableCopy { dst, src }
                }
                15..=17 => return Err("reference types support is not enabled".to_string()),
                _ => return Err(format!("unknown 0xfc opcode {}", sub)),
            }
        }
        0xfd => {
            let sub = r.u32()?;
            let kind = match sub {
                0..=11 => {
                    let (align, offset) = memarg(r)?;
                    SimdKind::Mem { align, offset }
                }
                12 => {
                    let b = r.bytes(16)?;
                    let mut a = [0u8; 16];
                    a.copy_from_slice(b);
                    SimdKind::Const(a)
                }
                13 => {
                    let b = r.bytes(16)?;
                    let mut a = [0u8; 16];
                    a.copy_from_slice(b);
                    SimdKind::Shuffle(a)
                }
                21..=34 => SimdKind::Lane { lane: r.u8()? },
                84..=91 => {
                    let (align, offset) = memarg(r)?;
                    let lane = r.u8()?;
                    SimdKind::MemLane { align, offset, lane }
                }
                92 | 93 => {
                    let (align, offset) = memarg(r)?;
                    SimdKind::Mem { align, offset }
                }
                _ => SimdKind::Plain,
            };
            Instr::Simd(SimdOp { sub, kind })
        }
        0xfe => return Err("threads support is not enabled".to_string()),
        _ => return Err(format!("illegal opcode 0x{:02x}", op)),
    })
}

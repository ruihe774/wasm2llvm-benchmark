use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ValType {
    I32,
    I64,
    F32,
    F64,
    V128,
}

impl ValType {
    pub fn llvm(self) -> &'static str {
        match self {
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::F32 => "float",
            Self::F64 => "double",
            Self::V128 => "<16 x i8>",
        }
    }

    pub fn natural_align(self) -> usize {
        match self {
            Self::I32 | Self::F32 => 4,
            Self::I64 | Self::F64 => 8,
            Self::V128 => 16,
        }
    }
}

impl fmt::Display for ValType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::F32 => "f32",
            Self::F64 => "f64",
            Self::V128 => "v128",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuncType {
    pub params: Vec<ValType>,
    pub results: Vec<ValType>,
}

#[derive(Clone, Debug)]
pub enum ConstExpr {
    I32(i32),
    I64(i64),
    F32(u32),
    F64(u64),
}

impl ConstExpr {
    pub fn ty(&self) -> ValType {
        match self {
            Self::I32(_) => ValType::I32,
            Self::I64(_) => ValType::I64,
            Self::F32(_) => ValType::F32,
            Self::F64(_) => ValType::F64,
        }
    }
}

#[derive(Clone, Debug)]
pub enum BlockType {
    Empty,
    Value(ValType),
    TypeIndex(u32),
}

#[derive(Clone, Debug)]
pub struct BlockSig {
    pub params: Vec<ValType>,
    pub results: Vec<ValType>,
}

#[derive(Clone, Debug)]
pub enum SimdImmediate {
    None,
    MemArg { align: u32, offset: u32 },
    Bytes([u8; 16]),
    Lane(u8),
    MemLane { align: u32, offset: u32, lane: u8 },
}

#[derive(Clone, Debug)]
pub enum BulkOp {
    MemoryInit { data: u32, memory: u32 },
    DataDrop { data: u32 },
    MemoryCopy { dst: u32, src: u32 },
    MemoryFill { memory: u32 },
    TableInit { element: u32, table: u32 },
    ElementDrop { element: u32 },
    TableCopy { dst: u32, src: u32 },
    TableGrow { table: u32 },
    TableSize { table: u32 },
    TableFill { table: u32 },
}

#[derive(Clone, Debug)]
pub enum Instr {
    Unreachable,
    Nop,
    Block(BlockSig, Vec<Instr>),
    Loop(BlockSig, Vec<Instr>),
    If(BlockSig, Vec<Instr>, Option<Vec<Instr>>),
    Br(u32),
    BrIf(u32),
    BrTable(Vec<u32>, u32),
    Return,
    Call(u32),
    CallIndirect { ty: u32, table: u32 },
    Drop,
    Select(Option<ValType>),
    LocalGet(u32),
    LocalSet(u32),
    LocalTee(u32),
    GlobalGet(u32),
    GlobalSet(u32),
    Load { op: u8, align: u32, offset: u32 },
    Store { op: u8, align: u32, offset: u32 },
    MemorySize(u32),
    MemoryGrow(u32),
    Const(ConstExpr),
    Numeric(u8),
    SignExtend(u8),
    Saturating(u32),
    Bulk(BulkOp),
    Simd { op: u32, immediate: SimdImmediate },
}

#[derive(Clone, Debug)]
pub struct Function {
    pub type_idx: u32,
    pub import: Option<(String, String)>,
    pub locals: Vec<ValType>,
    pub body: Vec<Instr>,
    pub max_stack: usize,
}

#[derive(Clone, Debug)]
pub struct Limits {
    pub min: u32,
    pub max: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct Memory {
    pub limits: Limits,
}

#[derive(Clone, Debug)]
pub struct Table {
    pub limits: Limits,
}

#[derive(Clone, Debug)]
pub struct Global {
    pub ty: ValType,
    pub mutable: bool,
    pub init: ConstExpr,
}

#[derive(Clone, Debug)]
pub enum SegmentMode {
    Active { index: u32, offset: ConstExpr },
    Passive,
    Declared,
}

#[derive(Clone, Debug)]
pub struct DataSegment {
    pub mode: SegmentMode,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct ElementSegment {
    pub mode: SegmentMode,
    pub functions: Vec<u32>,
}

#[derive(Clone, Debug)]
pub enum ExportKind {
    Function(u32),
    Table(u32),
    Memory(u32),
    Global(u32),
}

#[derive(Clone, Debug)]
pub struct Export {
    pub name: String,
    pub kind: ExportKind,
}

#[derive(Clone, Debug, Default)]
pub struct Module {
    pub types: Vec<FuncType>,
    pub functions: Vec<Function>,
    pub imported_functions: usize,
    pub memories: Vec<Memory>,
    pub tables: Vec<Table>,
    pub globals: Vec<Global>,
    pub exports: Vec<Export>,
    pub start: Option<u32>,
    pub elements: Vec<ElementSegment>,
    pub data: Vec<DataSegment>,
    pub declared_data_count: Option<u32>,
}

pub fn parse(bytes: &[u8]) -> Result<Module, String> {
    if bytes.len() < 8 || &bytes[..4] != b"\0asm" {
        return Err("invalid WebAssembly magic".into());
    }
    if bytes[4..8] != [1, 0, 0, 0] {
        return Err("unsupported WebAssembly version".into());
    }

    let mut module = Module::default();
    let mut reader = Reader::new(&bytes[8..]);
    let mut last_rank = 0u8;
    let mut seen = [false; 13];
    let mut defined_type_indices = Vec::new();

    while !reader.eof() {
        let id = reader.byte()?;
        let size = reader.u32()? as usize;
        let mut section = reader.subreader(size)?;
        if id == 0 {
            let _ = section.name()?;
            continue;
        }
        if id > 12 || id == 0 {
            return Err(format!("unknown section id {id}"));
        }
        let rank = match id {
            1..=9 => id,
            12 => 10,
            10 => 11,
            11 => 12,
            _ => unreachable!(),
        };
        if seen[id as usize] {
            return Err(format!("duplicate section id {id}"));
        }
        if rank <= last_rank {
            return Err(format!("section {id} is out of order"));
        }
        seen[id as usize] = true;
        last_rank = rank;

        match id {
            1 => parse_types(&mut section, &mut module)?,
            2 => parse_imports(&mut section, &mut module)?,
            3 => parse_function_section(&mut section, &mut module, &mut defined_type_indices)?,
            4 => parse_tables(&mut section, &mut module)?,
            5 => parse_memories(&mut section, &mut module)?,
            6 => parse_globals(&mut section, &mut module)?,
            7 => parse_exports(&mut section, &mut module)?,
            8 => {
                let index = section.u32()?;
                module.start = Some(index);
            }
            9 => parse_elements(&mut section, &mut module)?,
            10 => parse_code(&mut section, &mut module)?,
            11 => parse_data(&mut section, &mut module)?,
            12 => module.declared_data_count = Some(section.u32()?),
            _ => unreachable!(),
        }
        section.finish()?;
    }

    let defined_count = defined_type_indices.len();
    if module.functions.len() != module.imported_functions + defined_count {
        return Err("function and code section counts do not match".into());
    }
    if defined_count != 0 && !seen[10] {
        return Err("function section is missing code section".into());
    }
    if seen[10] && module.functions.len() != module.imported_functions + defined_count {
        return Err("function and code section counts do not match".into());
    }
    if let Some(n) = module.declared_data_count {
        if n as usize != module.data.len() {
            return Err("data count does not match data section".into());
        }
    }
    validate_indices(&module)?;
    Ok(module)
}

fn parse_types(r: &mut Reader<'_>, module: &mut Module) -> Result<(), String> {
    for _ in 0..r.count()? {
        if r.byte()? != 0x60 {
            return Err("unsupported type form".into());
        }
        let params = read_types(r)?;
        let results = read_types(r)?;
        module.types.push(FuncType { params, results });
    }
    Ok(())
}

fn parse_imports(r: &mut Reader<'_>, module: &mut Module) -> Result<(), String> {
    for _ in 0..r.count()? {
        let module_name = r.name()?;
        let field_name = r.name()?;
        let kind = r.byte()?;
        match kind {
            0 => {
                let type_idx = r.u32()?;
                module.functions.push(Function {
                    type_idx,
                    import: Some((module_name, field_name)),
                    locals: Vec::new(),
                    body: Vec::new(),
                    max_stack: 0,
                });
                module.imported_functions += 1;
            }
            1 => return Err("imported tables are outside the supported ABI".into()),
            2 => return Err("imported memories are outside the supported ABI".into()),
            3 => return Err("imported globals are outside the supported ABI".into()),
            _ => return Err(format!("invalid import kind {kind}")),
        }
    }
    Ok(())
}

fn parse_function_section(
    r: &mut Reader<'_>,
    module: &mut Module,
    defined_type_indices: &mut Vec<u32>,
) -> Result<(), String> {
    for _ in 0..r.count()? {
        let type_idx = r.u32()?;
        defined_type_indices.push(type_idx);
        module.functions.push(Function {
            type_idx,
            import: None,
            locals: Vec::new(),
            body: Vec::new(),
            max_stack: 0,
        });
    }
    Ok(())
}

fn parse_tables(r: &mut Reader<'_>, module: &mut Module) -> Result<(), String> {
    let count = r.count()?;
    if count > 1 {
        return Err("multiple tables are outside the supported scope".into());
    }
    for _ in 0..count {
        if r.byte()? != 0x70 {
            return Err("only funcref tables are supported".into());
        }
        let limits = read_limits(r, false)?;
        module.tables.push(Table { limits });
    }
    Ok(())
}

fn parse_memories(r: &mut Reader<'_>, module: &mut Module) -> Result<(), String> {
    let count = r.count()?;
    if count > 1 {
        return Err("multiple memories are outside the supported scope".into());
    }
    for _ in 0..count {
        let limits = read_limits(r, true)?;
        if limits.min > 65536 || limits.max.is_some_and(|m| m > 65536) {
            return Err("memory limits exceed the wasm32 page limit".into());
        }
        module.memories.push(Memory { limits });
    }
    Ok(())
}

fn read_limits(r: &mut Reader<'_>, memory: bool) -> Result<Limits, String> {
    let flags = r.u32()?;
    if flags & !1 != 0 {
        return Err(if memory {
            "shared and memory64 memories are outside the supported scope".into()
        } else {
            "unsupported table limits flags".into()
        });
    }
    let min = r.u32()?;
    let max = if flags & 1 != 0 { Some(r.u32()?) } else { None };
    if max.is_some_and(|m| m < min) {
        return Err("limits maximum is smaller than minimum".into());
    }
    Ok(Limits { min, max })
}

fn parse_globals(r: &mut Reader<'_>, module: &mut Module) -> Result<(), String> {
    for _ in 0..r.count()? {
        let ty = read_valtype(r.byte()?)?;
        let mutable = match r.byte()? {
            0 => false,
            1 => true,
            other => return Err(format!("invalid global mutability {other}")),
        };
        let init = read_const_expr(r)?;
        if init.ty() != ty {
            return Err(format!("global initializer has type {}, expected {ty}", init.ty()));
        }
        module.globals.push(Global { ty, mutable, init });
    }
    Ok(())
}

fn parse_exports(r: &mut Reader<'_>, module: &mut Module) -> Result<(), String> {
    for _ in 0..r.count()? {
        let name = r.name()?;
        let kind = match r.byte()? {
            0 => ExportKind::Function(r.u32()?),
            1 => ExportKind::Table(r.u32()?),
            2 => ExportKind::Memory(r.u32()?),
            3 => ExportKind::Global(r.u32()?),
            other => return Err(format!("invalid export kind {other}")),
        };
        if module.exports.iter().any(|e| e.name == name) {
            return Err(format!("duplicate export name {name:?}"));
        }
        module.exports.push(Export { name, kind });
    }
    Ok(())
}

fn parse_elements(r: &mut Reader<'_>, module: &mut Module) -> Result<(), String> {
    for _ in 0..r.count()? {
        let flags = r.u32()?;
        let (mode, has_elemkind) = match flags {
            0 => (SegmentMode::Active { index: 0, offset: read_const_expr(r)? }, false),
            1 => (SegmentMode::Passive, true),
            2 => {
                let table = r.u32()?;
                let offset = read_const_expr(r)?;
                (SegmentMode::Active { index: table, offset }, true)
            }
            3 => (SegmentMode::Declared, true),
            4..=7 => return Err("reference-type element expressions are outside the supported scope".into()),
            _ => return Err(format!("invalid element segment flags {flags}")),
        };
        if has_elemkind && r.byte()? != 0 {
            return Err("only funcref element segments are supported".into());
        }
        let mut functions = Vec::new();
        for _ in 0..r.count()? {
            functions.push(r.u32()?);
        }
        module.elements.push(ElementSegment { mode, functions });
    }
    Ok(())
}

fn parse_data(r: &mut Reader<'_>, module: &mut Module) -> Result<(), String> {
    for _ in 0..r.count()? {
        let flags = r.u32()?;
        let mode = match flags {
            0 => SegmentMode::Active { index: 0, offset: read_const_expr(r)? },
            1 => SegmentMode::Passive,
            2 => {
                let memory = r.u32()?;
                let offset = read_const_expr(r)?;
                SegmentMode::Active { index: memory, offset }
            }
            _ => return Err(format!("invalid data segment flags {flags}")),
        };
        let len = r.count()?;
        let bytes = r.bytes(len)?.to_vec();
        module.data.push(DataSegment { mode, bytes });
    }
    Ok(())
}

fn parse_code(r: &mut Reader<'_>, module: &mut Module) -> Result<(), String> {
    let count = r.count()?;
    let defined_count = module.functions.len().saturating_sub(module.imported_functions);
    if count != defined_count {
        return Err("function and code section counts do not match".into());
    }
    for body_index in 0..count {
        let size = r.count()?;
        let mut body = r.subreader(size)?;
        let mut locals = Vec::new();
        for _ in 0..body.count()? {
            let n = body.count()?;
            let ty = read_valtype(body.byte()?)?;
            if locals.len().saturating_add(n) > 10_000_000 {
                return Err("too many locals".into());
            }
            locals.resize(locals.len() + n, ty);
        }
        let types = &module.types;
        let (instructions, end) = parse_instr_seq(&mut body, types, false)?;
        if end != SeqEnd::End {
            return Err("unexpected else at function end".into());
        }
        body.finish()?;
        let function = &mut module.functions[module.imported_functions + body_index];
        function.locals = locals;
        function.body = instructions;
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SeqEnd {
    End,
    Else,
}

fn parse_instr_seq(
    r: &mut Reader<'_>,
    types: &[FuncType],
    allow_else: bool,
) -> Result<(Vec<Instr>, SeqEnd), String> {
    let mut out = Vec::new();
    loop {
        let op = r.byte()?;
        match op {
            0x0b => return Ok((out, SeqEnd::End)),
            0x05 if allow_else => return Ok((out, SeqEnd::Else)),
            0x05 => return Err("unexpected else instruction".into()),
            0x00 => out.push(Instr::Unreachable),
            0x01 => out.push(Instr::Nop),
            0x02..=0x04 => {
                let sig = resolve_blocktype(read_blocktype(r)?, types)?;
                if op == 0x04 {
                    let (then_body, ending) = parse_instr_seq(r, types, true)?;
                    let else_body = if ending == SeqEnd::Else {
                        let (body, end) = parse_instr_seq(r, types, false)?;
                        if end != SeqEnd::End {
                            return Err("unexpected else at if end".into());
                        }
                        Some(body)
                    } else {
                        None
                    };
                    out.push(Instr::If(sig, then_body, else_body));
                } else {
                    let (body, ending) = parse_instr_seq(r, types, false)?;
                    if ending != SeqEnd::End {
                        return Err("unexpected else in block".into());
                    }
                    if op == 0x02 {
                        out.push(Instr::Block(sig, body));
                    } else {
                        out.push(Instr::Loop(sig, body));
                    }
                }
            }
            0x0c => out.push(Instr::Br(r.u32()?)),
            0x0d => out.push(Instr::BrIf(r.u32()?)),
            0x0e => {
                let n = r.count()?;
                let mut targets = Vec::with_capacity(n);
                for _ in 0..n {
                    targets.push(r.u32()?);
                }
                out.push(Instr::BrTable(targets, r.u32()?));
            }
            0x0f => out.push(Instr::Return),
            0x10 => out.push(Instr::Call(r.u32()?)),
            0x11 => out.push(Instr::CallIndirect { ty: r.u32()?, table: r.u32()? }),
            0x1a => out.push(Instr::Drop),
            0x1b => out.push(Instr::Select(None)),
            0x1c => return Err("typed select is outside the supported scope".into()),
            0x20 => out.push(Instr::LocalGet(r.u32()?)),
            0x21 => out.push(Instr::LocalSet(r.u32()?)),
            0x22 => out.push(Instr::LocalTee(r.u32()?)),
            0x23 => out.push(Instr::GlobalGet(r.u32()?)),
            0x24 => out.push(Instr::GlobalSet(r.u32()?)),
            0x28..=0x35 => {
                let align = r.u32()?;
                let offset = r.u32()?;
                out.push(Instr::Load { op, align, offset });
            }
            0x36..=0x3e => {
                let align = r.u32()?;
                let offset = r.u32()?;
                out.push(Instr::Store { op, align, offset });
            }
            0x3f => out.push(Instr::MemorySize(r.u32()?)),
            0x40 => out.push(Instr::MemoryGrow(r.u32()?)),
            0x41 => out.push(Instr::Const(ConstExpr::I32(r.i32()?))),
            0x42 => out.push(Instr::Const(ConstExpr::I64(r.i64()?))),
            0x43 => out.push(Instr::Const(ConstExpr::F32(u32::from_le_bytes(r.bytes(4)?.try_into().unwrap())))),
            0x44 => out.push(Instr::Const(ConstExpr::F64(u64::from_le_bytes(r.bytes(8)?.try_into().unwrap())))),
            0x45..=0xbf => out.push(Instr::Numeric(op)),
            0xc0..=0xc4 => out.push(Instr::SignExtend(op)),
            0xfc => {
                let misc = r.u32()?;
                let instruction = match misc {
                    0..=7 => Instr::Saturating(misc),
                    8 => {
                        let data = r.u32()?;
                        let memory = r.u32()?;
                        Instr::Bulk(BulkOp::MemoryInit { data, memory })
                    }
                    9 => {
                        let data = r.u32()?;
                        Instr::Bulk(BulkOp::DataDrop { data })
                    }
                    10 => Instr::Bulk(BulkOp::MemoryCopy { dst: r.u32()?, src: r.u32()? }),
                    11 => Instr::Bulk(BulkOp::MemoryFill { memory: r.u32()? }),
                    12 => Instr::Bulk(BulkOp::TableInit { element: r.u32()?, table: r.u32()? }),
                    13 => Instr::Bulk(BulkOp::ElementDrop { element: r.u32()? }),
                    14 => Instr::Bulk(BulkOp::TableCopy { dst: r.u32()?, src: r.u32()? }),
                    15 => Instr::Bulk(BulkOp::TableGrow { table: r.u32()? }),
                    16 => Instr::Bulk(BulkOp::TableSize { table: r.u32()? }),
                    17 => Instr::Bulk(BulkOp::TableFill { table: r.u32()? }),
                    _ => return Err(format!("unsupported 0xfc opcode {misc}")),
                };
                out.push(instruction);
            }
            0xfd => {
                let sub = r.u32()?;
                let immediate = parse_simd_immediate(r, sub)?;
                out.push(Instr::Simd { op: sub, immediate });
            }
            _ => return Err(format!("unsupported opcode 0x{op:02x}")),
        }
    }
}

fn parse_simd_immediate(r: &mut Reader<'_>, op: u32) -> Result<SimdImmediate, String> {
    match op {
        0x00..=0x0b | 0x54..=0x5d => {
            let align = r.u32()?;
            let offset = r.u32()?;
            if (0x54..=0x5b).contains(&op) {
                Ok(SimdImmediate::MemLane { align, offset, lane: r.byte()? })
            } else {
                Ok(SimdImmediate::MemArg { align, offset })
            }
        }
        0x0c => Ok(SimdImmediate::Bytes(r.bytes(16)?.try_into().unwrap())),
        0x0d => Ok(SimdImmediate::Bytes(r.bytes(16)?.try_into().unwrap())),
        0x15..=0x22 => Ok(SimdImmediate::Lane(r.byte()?)),
        _ if is_known_simd_opcode(op) => Ok(SimdImmediate::None),
        _ => Err(format!("unsupported SIMD opcode 0x{op:x}")),
    }
}

pub fn is_known_simd_opcode(op: u32) -> bool {
    matches!(op,
        0x00..=0x5f |
        0x60..=0x7f |
        0x80..=0x8f |
        0x90..=0x99 |
        0x9b..=0x9f |
        0xa0 | 0xa1 | 0xa3 | 0xa4 | 0xa7..=0xae |
        0xb1 | 0xb5..=0xba | 0xbc..=0xbf |
        0xc0..=0xc1 | 0xc3..=0xc4 |
        0xc7..=0xce | 0xd1 |
        0xd5..=0xdf |
        0xe0..=0xeb |
        0xec..=0xf7 |
        0xf8..=0xff
    )
}

fn read_types(r: &mut Reader<'_>) -> Result<Vec<ValType>, String> {
    let count = r.count()?;
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(read_valtype(r.byte()?)?);
    }
    Ok(values)
}

pub fn read_valtype(byte: u8) -> Result<ValType, String> {
    match byte {
        0x7f => Ok(ValType::I32),
        0x7e => Ok(ValType::I64),
        0x7d => Ok(ValType::F32),
        0x7c => Ok(ValType::F64),
        0x7b => Ok(ValType::V128),
        0x70 | 0x6f => Err("reference types are outside the supported scope".into()),
        _ => Err(format!("invalid value type 0x{byte:02x}")),
    }
}

fn read_blocktype(r: &mut Reader<'_>) -> Result<BlockType, String> {
    let value = r.sleb(33)?;
    match value {
        -64 => Ok(BlockType::Empty),
        -1 => Ok(BlockType::Value(ValType::I32)),
        -2 => Ok(BlockType::Value(ValType::I64)),
        -3 => Ok(BlockType::Value(ValType::F32)),
        -4 => Ok(BlockType::Value(ValType::F64)),
        -5 => Ok(BlockType::Value(ValType::V128)),
        v if v >= 0 && v <= u32::MAX as i64 => Ok(BlockType::TypeIndex(v as u32)),
        _ => Err("invalid block type".into()),
    }
}

fn resolve_blocktype(block: BlockType, types: &[FuncType]) -> Result<BlockSig, String> {
    match block {
        BlockType::Empty => Ok(BlockSig { params: Vec::new(), results: Vec::new() }),
        BlockType::Value(result) => Ok(BlockSig { params: Vec::new(), results: vec![result] }),
        BlockType::TypeIndex(index) => types.get(index as usize)
            .map(|ty| BlockSig { params: ty.params.clone(), results: ty.results.clone() })
            .ok_or_else(|| format!("block type index {index} is out of bounds")),
    }
}

fn read_const_expr(r: &mut Reader<'_>) -> Result<ConstExpr, String> {
    let value = match r.byte()? {
        0x41 => ConstExpr::I32(r.i32()?),
        0x42 => ConstExpr::I64(r.i64()?),
        0x43 => ConstExpr::F32(u32::from_le_bytes(r.bytes(4)?.try_into().unwrap())),
        0x44 => ConstExpr::F64(u64::from_le_bytes(r.bytes(8)?.try_into().unwrap())),
        op => return Err(format!("unsupported constant expression opcode 0x{op:02x}")),
    };
    if r.byte()? != 0x0b {
        return Err("constant expression is missing end".into());
    }
    Ok(value)
}

fn validate_indices(module: &Module) -> Result<(), String> {
    for (index, function) in module.functions.iter().enumerate() {
        if function.type_idx as usize >= module.types.len() {
            return Err(format!("function {index} has an invalid type index"));
        }
    }
    for (index, global) in module.globals.iter().enumerate() {
        let _ = (index, global);
    }
    if module.tables.len() > 1 || module.memories.len() > 1 {
        return Err("multiple tables or memories are unsupported".into());
    }
    for export in &module.exports {
        match export.kind {
            ExportKind::Function(i) if i as usize >= module.functions.len() => return Err("exported function index is out of bounds".into()),
            ExportKind::Table(i) if i as usize >= module.tables.len() => return Err("exported table index is out of bounds".into()),
            ExportKind::Memory(i) if i as usize >= module.memories.len() => return Err("exported memory index is out of bounds".into()),
            ExportKind::Global(i) if i as usize >= module.globals.len() => return Err("exported global index is out of bounds".into()),
            _ => {}
        }
    }
    if let Some(index) = module.start {
        let function = module.functions.get(index as usize).ok_or("start function index is out of bounds")?;
        let ty = &module.types[function.type_idx as usize];
        if !ty.params.is_empty() || !ty.results.is_empty() {
            return Err("start function must have type [] -> []".into());
        }
    }
    for (index, element) in module.elements.iter().enumerate() {
        if let SegmentMode::Active { index: table, offset } = &element.mode {
            if *table != 0 || module.tables.is_empty() {
                return Err(format!("element segment {index} uses an invalid table"));
            }
            if offset.ty() != ValType::I32 {
                return Err(format!("element segment {index} offset is not i32"));
            }
        }
        for &func in &element.functions {
            if func as usize >= module.functions.len() {
                return Err(format!("element segment {index} function index is out of bounds"));
            }
        }
    }
    for (index, data) in module.data.iter().enumerate() {
        if let SegmentMode::Active { index: memory, offset } = &data.mode {
            if *memory != 0 || module.memories.is_empty() {
                return Err(format!("data segment {index} uses an invalid memory"));
            }
            if offset.ty() != ValType::I32 {
                return Err(format!("data segment {index} offset is not i32"));
            }
        }
    }
    Ok(())
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self { Self { bytes, pos: 0 } }
    fn eof(&self) -> bool { self.pos == self.bytes.len() }
    fn remaining(&self) -> usize { self.bytes.len() - self.pos }

    fn byte(&mut self) -> Result<u8, String> {
        let value = *self.bytes.get(self.pos).ok_or("unexpected end of WebAssembly input")?;
        self.pos += 1;
        Ok(value)
    }

    fn bytes(&mut self, len: usize) -> Result<&'a [u8], String> {
        let end = self.pos.checked_add(len).ok_or("input length overflow")?;
        let result = self.bytes.get(self.pos..end).ok_or("unexpected end of WebAssembly input")?;
        self.pos = end;
        Ok(result)
    }

    fn subreader(&mut self, len: usize) -> Result<Reader<'a>, String> {
        Ok(Reader::new(self.bytes(len)?))
    }

    fn u32(&mut self) -> Result<u32, String> {
        Ok(self.uleb(32)? as u32)
    }

    fn count(&mut self) -> Result<usize, String> {
        let n = self.u32()? as usize;
        if n > 10_000_000 || n > self.remaining().saturating_add(1) {
            return Err("vector length exceeds the remaining input".into());
        }
        Ok(n)
    }

    fn i32(&mut self) -> Result<i32, String> { Ok(self.sleb(32)? as i32) }
    fn i64(&mut self) -> Result<i64, String> { self.sleb(64) }

    fn uleb(&mut self, bits: u32) -> Result<u64, String> {
        let max = (bits as usize + 6) / 7;
        let mut result = 0u64;
        for index in 0..max {
            let byte = self.byte()?;
            let payload = (byte & 0x7f) as u64;
            let shift = index * 7;
            if shift < 64 {
                result |= payload << shift;
            }
            if byte & 0x80 == 0 {
                let used = bits as usize - shift;
                if used < 7 && payload >> used != 0 {
                    return Err("unsigned LEB128 integer is out of range".into());
                }
                return Ok(result);
            }
        }
        Err("unterminated or oversized unsigned LEB128 integer".into())
    }

    fn sleb(&mut self, bits: u32) -> Result<i64, String> {
        let max = (bits as usize + 6) / 7;
        let mut result = 0i128;
        let mut shift = 0usize;
        for _ in 0..max {
            let byte = self.byte()?;
            result |= ((byte & 0x7f) as i128) << shift;
            shift += 7;
            if byte & 0x80 == 0 {
                if byte & 0x40 != 0 {
                    result |= (!0i128) << shift;
                }
                let min = -(1i128 << (bits - 1));
                let max_value = (1i128 << (bits - 1)) - 1;
                if result < min || result > max_value {
                    return Err("signed LEB128 integer is out of range".into());
                }
                return Ok(result as i64);
            }
        }
        Err("unterminated or oversized signed LEB128 integer".into())
    }

    fn name(&mut self) -> Result<String, String> {
        let n = self.u32()? as usize;
        let bytes = self.bytes(n)?;
        std::str::from_utf8(bytes).map(str::to_owned).map_err(|_| "name is not valid UTF-8".into())
    }

    fn finish(&self) -> Result<(), String> {
        if self.eof() { Ok(()) } else { Err("trailing bytes in section".into()) }
    }
}

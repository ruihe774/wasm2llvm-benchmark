// Module structures and binary decoding.

use crate::instr::{decode_expr, Instr};
use crate::reader::{Reader, RResult};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ValType {
    I32,
    I64,
    F32,
    F64,
    V128,
    FuncRef,
    ExternRef,
}

impl ValType {
    pub fn from_byte(b: u8) -> Option<ValType> {
        match b {
            0x7f => Some(ValType::I32),
            0x7e => Some(ValType::I64),
            0x7d => Some(ValType::F32),
            0x7c => Some(ValType::F64),
            0x7b => Some(ValType::V128),
            0x70 => Some(ValType::FuncRef),
            0x6f => Some(ValType::ExternRef),
            _ => None,
        }
    }
    pub fn is_num(&self) -> bool {
        matches!(
            self,
            ValType::I32 | ValType::I64 | ValType::F32 | ValType::F64
        )
    }
    pub fn is_vec(&self) -> bool {
        matches!(self, ValType::V128)
    }
    pub fn is_ref(&self) -> bool {
        matches!(self, ValType::FuncRef | ValType::ExternRef)
    }
    pub fn name(&self) -> &'static str {
        match self {
            ValType::I32 => "i32",
            ValType::I64 => "i64",
            ValType::F32 => "f32",
            ValType::F64 => "f64",
            ValType::V128 => "v128",
            ValType::FuncRef => "funcref",
            ValType::ExternRef => "externref",
        }
    }
}

pub fn read_valtype(r: &mut Reader) -> RResult<ValType> {
    let b = r.byte()?;
    ValType::from_byte(b).ok_or_else(|| format!("invalid value type 0x{:02x}", b))
}

pub fn read_reftype(r: &mut Reader) -> RResult<ValType> {
    let b = r.byte()?;
    match ValType::from_byte(b) {
        Some(t) if t.is_ref() => Ok(t),
        _ => Err(format!("invalid reference type 0x{:02x}", b)),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuncType {
    pub params: Vec<ValType>,
    pub results: Vec<ValType>,
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub min: u32,
    pub max: Option<u32>,
}

#[derive(Clone, Copy, Debug)]
pub struct TableType {
    pub elem: ValType,
    pub limits: Limits,
}

#[derive(Clone, Copy, Debug)]
pub struct MemType {
    pub limits: Limits,
}

#[derive(Clone, Copy, Debug)]
pub struct GlobalType {
    pub val: ValType,
    pub mutable: bool,
}

#[derive(Clone, Debug)]
pub enum ImportDesc {
    Func(u32),
    Table(TableType),
    Mem(MemType),
    Global(GlobalType),
}

#[derive(Clone, Debug)]
pub struct Import {
    pub module: Vec<u8>,
    pub name: Vec<u8>,
    pub desc: ImportDesc,
}

#[derive(Clone, Debug)]
pub struct Func {
    pub type_idx: u32,
    pub locals: Vec<ValType>,
    pub body: Vec<Instr>,
}

#[derive(Clone, Debug)]
pub struct Global {
    pub ty: GlobalType,
    pub init: Vec<Instr>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportKind {
    Func,
    Table,
    Mem,
    Global,
}

#[derive(Clone, Debug)]
pub struct Export {
    pub name: Vec<u8>,
    pub kind: ExportKind,
    pub idx: u32,
}

#[derive(Clone, Debug)]
pub enum ElemMode {
    Active { table: u32, offset: Vec<Instr> },
    Passive,
    Declarative,
}

#[derive(Clone, Debug)]
pub struct Elem {
    pub ty: ValType,
    pub init: Vec<Vec<Instr>>,
    pub mode: ElemMode,
}

#[derive(Clone, Debug)]
pub enum DataMode {
    Active { mem: u32, offset: Vec<Instr> },
    Passive,
}

#[derive(Clone, Debug)]
pub struct Data {
    pub init: Vec<u8>,
    pub mode: DataMode,
}

#[allow(dead_code)]
pub struct Module {
    pub types: Vec<FuncType>,
    pub imports: Vec<Import>,
    pub funcs: Vec<Func>,
    pub tables: Vec<TableType>,
    pub mems: Vec<MemType>,
    pub globals: Vec<Global>,
    pub exports: Vec<Export>,
    pub start: Option<u32>,
    pub elems: Vec<Elem>,
    pub datas: Vec<Data>,
    pub datacount: Option<u32>,

    // Index spaces with imports first.
    pub func_types: Vec<u32>,
    pub table_types: Vec<TableType>,
    pub mem_types: Vec<MemType>,
    pub global_types: Vec<GlobalType>,
    pub imported_funcs: Vec<(Vec<u8>, Vec<u8>, u32)>, // module, name, type idx
    pub num_imported_funcs: usize,
    pub num_imported_tables: usize,
    pub num_imported_mems: usize,
    pub num_imported_globals: usize,
}

fn read_limits(r: &mut Reader, max_allowed: Option<u32>, what: &str) -> RResult<Limits> {
    let flags = r.byte()?;
    let (min, max) = match flags {
        0x00 => (r.u32()?, None),
        0x01 => {
            let min = r.u32()?;
            let max = r.u32()?;
            (min, Some(max))
        }
        0x02 | 0x03 => {
            return Err(format!("{} with shared memory/threads flag is not supported", what))
        }
        0x04 | 0x05 => {
            return Err(format!("{} with 64-bit index flag is not supported", what))
        }
        _ => return Err(format!("invalid {} limits flags 0x{:02x}", what, flags)),
    };
    if let Some(m) = max {
        if m < min {
            return Err(format!("{} limits maximum is smaller than minimum", what));
        }
    }
    if let Some(limit) = max_allowed {
        if min > limit {
            return Err(format!("{} minimum exceeds limit {}", what, limit));
        }
        if let Some(m) = max {
            if m > limit {
                return Err(format!("{} maximum exceeds limit {}", what, limit));
            }
        }
    }
    Ok(Limits { min, max })
}

fn read_tabletype(r: &mut Reader) -> RResult<TableType> {
    let elem = read_reftype(r)?;
    let limits = read_limits(r, None, "table")?;
    Ok(TableType { elem, limits })
}

fn read_memtype(r: &mut Reader) -> RResult<MemType> {
    let limits = read_limits(r, Some(65536), "memory")?;
    Ok(MemType { limits })
}

fn read_globaltype(r: &mut Reader) -> RResult<GlobalType> {
    let val = read_valtype(r)?;
    let m = r.byte()?;
    let mutable = match m {
        0x00 => false,
        0x01 => true,
        _ => return Err(format!("invalid global mutability 0x{:02x}", m)),
    };
    Ok(GlobalType { val, mutable })
}

fn vec_len(r: &mut Reader) -> RResult<usize> {
    let n = r.u32()? as usize;
    // Every vector element takes at least one byte; this also guards allocations.
    if n > r.remaining() {
        return Err("vector length exceeds input size".to_string());
    }
    Ok(n)
}

const SEC_CUSTOM: u8 = 0;
const SEC_TYPE: u8 = 1;
const SEC_IMPORT: u8 = 2;
const SEC_FUNC: u8 = 3;
const SEC_TABLE: u8 = 4;
const SEC_MEM: u8 = 5;
const SEC_GLOBAL: u8 = 6;
const SEC_EXPORT: u8 = 7;
const SEC_START: u8 = 8;
const SEC_ELEM: u8 = 9;
const SEC_CODE: u8 = 10;
const SEC_DATA: u8 = 11;
const SEC_DATACOUNT: u8 = 12;

fn section_rank(id: u8) -> Option<u32> {
    match id {
        SEC_TYPE => Some(1),
        SEC_IMPORT => Some(2),
        SEC_FUNC => Some(3),
        SEC_TABLE => Some(4),
        SEC_MEM => Some(5),
        SEC_GLOBAL => Some(6),
        SEC_EXPORT => Some(7),
        SEC_START => Some(8),
        SEC_ELEM => Some(9),
        SEC_DATACOUNT => Some(10),
        SEC_CODE => Some(11),
        SEC_DATA => Some(12),
        _ => None,
    }
}

pub fn parse_module(buf: &[u8]) -> RResult<Module> {
    let mut r = Reader::new(buf);
    let magic = r.bytes(4)?;
    if magic != b"\0asm" {
        return Err("bad magic number".to_string());
    }
    let ver = r.bytes(4)?;
    if ver != [1, 0, 0, 0] {
        return Err("unsupported binary version".to_string());
    }

    let mut types: Vec<FuncType> = Vec::new();
    let mut imports: Vec<Import> = Vec::new();
    let mut func_type_idxs: Option<Vec<u32>> = None;
    let mut tables: Vec<TableType> = Vec::new();
    let mut mems: Vec<MemType> = Vec::new();
    let mut globals: Vec<Global> = Vec::new();
    let mut exports: Vec<Export> = Vec::new();
    let mut start: Option<u32> = None;
    let mut elems: Vec<Elem> = Vec::new();
    let mut datas: Vec<Data> = Vec::new();
    let mut datacount: Option<u32> = None;
    let mut code_bodies: Option<Vec<(Vec<ValType>, Vec<Instr>)>> = None;

    let mut last_rank = 0u32;
    let mut seen = [false; 13];

    while !r.eof() {
        let id = r.byte()?;
        let size = r.u32()? as usize;
        if size > r.remaining() {
            return Err("section size exceeds input".to_string());
        }
        let start_pos = r.pos;
        let end_pos = start_pos + size;

        if id != SEC_CUSTOM {
            let rank = section_rank(id).ok_or_else(|| format!("invalid section id {}", id))?;
            if seen[id as usize] {
                return Err(format!("duplicate section id {}", id));
            }
            if rank < last_rank {
                return Err(format!("section id {} out of order", id));
            }
            seen[id as usize] = true;
            last_rank = rank;
        }

        match id {
            SEC_CUSTOM => {
                // Name plus uninterpreted bytes.
                let _name = r.name()?;
                r.pos = end_pos;
            }
            SEC_TYPE => {
                let n = vec_len(&mut r)?;
                for _ in 0..n {
                    let tag = r.byte()?;
                    if tag != 0x60 {
                        return Err(format!("unsupported type definition 0x{:02x}", tag));
                    }
                    let np = vec_len(&mut r)?;
                    let mut params = Vec::with_capacity(np);
                    for _ in 0..np {
                        params.push(read_valtype(&mut r)?);
                    }
                    let nr = vec_len(&mut r)?;
                    let mut results = Vec::with_capacity(nr);
                    for _ in 0..nr {
                        results.push(read_valtype(&mut r)?);
                    }
                    types.push(FuncType { params, results });
                }
            }
            SEC_IMPORT => {
                let n = vec_len(&mut r)?;
                for _ in 0..n {
                    let module = r.name()?;
                    let name = r.name()?;
                    let kind = r.byte()?;
                    let desc = match kind {
                        0x00 => ImportDesc::Func(r.u32()?),
                        0x01 => ImportDesc::Table(read_tabletype(&mut r)?),
                        0x02 => ImportDesc::Mem(read_memtype(&mut r)?),
                        0x03 => ImportDesc::Global(read_globaltype(&mut r)?),
                        _ => {
                            return Err(format!(
                                "unsupported import kind 0x{:02x} (tags/exception handling are out of scope)",
                                kind
                            ))
                        }
                    };
                    imports.push(Import { module, name, desc });
                }
            }
            SEC_FUNC => {
                let n = vec_len(&mut r)?;
                let mut v = Vec::with_capacity(n);
                for _ in 0..n {
                    v.push(r.u32()?);
                }
                func_type_idxs = Some(v);
            }
            SEC_TABLE => {
                let n = vec_len(&mut r)?;
                for _ in 0..n {
                    tables.push(read_tabletype(&mut r)?);
                }
            }
            SEC_MEM => {
                let n = vec_len(&mut r)?;
                for _ in 0..n {
                    mems.push(read_memtype(&mut r)?);
                }
            }
            SEC_GLOBAL => {
                let n = vec_len(&mut r)?;
                for _ in 0..n {
                    let ty = read_globaltype(&mut r)?;
                    let init = decode_expr(&mut r)?;
                    globals.push(Global { ty, init });
                }
            }
            SEC_EXPORT => {
                let n = vec_len(&mut r)?;
                for _ in 0..n {
                    let name = r.name()?;
                    let kind = r.byte()?;
                    let k = match kind {
                        0x00 => ExportKind::Func,
                        0x01 => ExportKind::Table,
                        0x02 => ExportKind::Mem,
                        0x03 => ExportKind::Global,
                        _ => {
                            return Err(format!(
                                "unsupported export kind 0x{:02x} (tags/exception handling are out of scope)",
                                kind
                            ))
                        }
                    };
                    let idx = r.u32()?;
                    exports.push(Export { name, kind: k, idx });
                }
            }
            SEC_START => {
                start = Some(r.u32()?);
            }
            SEC_ELEM => {
                let n = vec_len(&mut r)?;
                for _ in 0..n {
                    elems.push(read_elem(&mut r)?);
                }
            }
            SEC_DATACOUNT => {
                datacount = Some(r.u32()?);
            }
            SEC_CODE => {
                let n = vec_len(&mut r)?;
                let mut v = Vec::with_capacity(n);
                for _ in 0..n {
                    let body_size = r.u32()? as usize;
                    if body_size > r.remaining() {
                        return Err("function body size exceeds input".to_string());
                    }
                    let body_end = r.pos + body_size;
                    let nlocals = vec_len(&mut r)?;
                    let mut locals: Vec<ValType> = Vec::new();
                    let mut total: u64 = 0;
                    for _ in 0..nlocals {
                        let count = r.u32()?;
                        let t = read_valtype(&mut r)?;
                        total += count as u64;
                        if total > 1_000_000 {
                            return Err("too many locals".to_string());
                        }
                        for _ in 0..count {
                            locals.push(t);
                        }
                    }
                    let body = decode_expr(&mut r)?;
                    if r.pos != body_end {
                        return Err("function body size mismatch".to_string());
                    }
                    v.push((locals, body));
                }
                code_bodies = Some(v);
            }
            SEC_DATA => {
                let n = vec_len(&mut r)?;
                for _ in 0..n {
                    let flags = r.u32()?;
                    let seg = match flags {
                        0 => {
                            let offset = decode_expr(&mut r)?;
                            let len = r.u32()? as usize;
                            if len > r.remaining() {
                                return Err("data segment length exceeds input".to_string());
                            }
                            let init = r.bytes(len)?.to_vec();
                            Data {
                                init,
                                mode: DataMode::Active { mem: 0, offset },
                            }
                        }
                        1 => {
                            let len = r.u32()? as usize;
                            if len > r.remaining() {
                                return Err("data segment length exceeds input".to_string());
                            }
                            let init = r.bytes(len)?.to_vec();
                            Data {
                                init,
                                mode: DataMode::Passive,
                            }
                        }
                        2 => {
                            let mem = r.u32()?;
                            let offset = decode_expr(&mut r)?;
                            let len = r.u32()? as usize;
                            if len > r.remaining() {
                                return Err("data segment length exceeds input".to_string());
                            }
                            let init = r.bytes(len)?.to_vec();
                            Data {
                                init,
                                mode: DataMode::Active { mem, offset },
                            }
                        }
                        _ => return Err(format!("invalid data segment flags {}", flags)),
                    };
                    datas.push(seg);
                }
            }
            _ => return Err(format!("invalid section id {}", id)),
        }

        if r.pos != end_pos {
            return Err(format!("section id {} size mismatch", id));
        }
    }

    let func_type_idxs = func_type_idxs.unwrap_or_default();
    let code_bodies = code_bodies.unwrap_or_default();
    if func_type_idxs.len() != code_bodies.len() {
        return Err("function and code section have inconsistent lengths".to_string());
    }

    let mut funcs: Vec<Func> = Vec::with_capacity(func_type_idxs.len());
    for (i, tidx) in func_type_idxs.iter().enumerate() {
        let (locals, body) = code_bodies[i].clone();
        funcs.push(Func {
            type_idx: *tidx,
            locals,
            body,
        });
    }

    // Check the data-count section consistency.
    if let Some(dc) = datacount {
        if dc as usize != datas.len() {
            return Err("data count section does not match data section".to_string());
        }
    } else {
        for f in &funcs {
            for ins in &f.body {
                if matches!(ins, Instr::MemoryInit(_) | Instr::DataDrop(_)) {
                    return Err(
                        "data count section required when using data segment instructions"
                            .to_string(),
                    );
                }
            }
        }
    }

    // Build index spaces.
    let mut imported_funcs = Vec::new();
    let mut func_types: Vec<u32> = Vec::new();
    let mut table_types: Vec<TableType> = Vec::new();
    let mut mem_types: Vec<MemType> = Vec::new();
    let mut global_types: Vec<GlobalType> = Vec::new();
    let mut num_imported_funcs = 0;
    let mut num_imported_tables = 0;
    let mut num_imported_mems = 0;
    let mut num_imported_globals = 0;
    for imp in &imports {
        match &imp.desc {
            ImportDesc::Func(t) => {
                imported_funcs.push((imp.module.clone(), imp.name.clone(), *t));
                func_types.push(*t);
                num_imported_funcs += 1;
            }
            ImportDesc::Table(tt) => {
                table_types.push(*tt);
                num_imported_tables += 1;
            }
            ImportDesc::Mem(mt) => {
                mem_types.push(*mt);
                num_imported_mems += 1;
            }
            ImportDesc::Global(gt) => {
                global_types.push(*gt);
                num_imported_globals += 1;
            }
        }
    }
    for f in &funcs {
        func_types.push(f.type_idx);
    }
    table_types.extend_from_slice(&tables);
    mem_types.extend_from_slice(&mems);
    for g in &globals {
        global_types.push(g.ty);
    }

    Ok(Module {
        types,
        imports,
        funcs,
        tables,
        mems,
        globals,
        exports,
        start,
        elems,
        datas,
        datacount,
        func_types,
        table_types,
        mem_types,
        global_types,
        imported_funcs,
        num_imported_funcs,
        num_imported_tables,
        num_imported_mems,
        num_imported_globals,
    })
}

fn read_elem(r: &mut Reader) -> RResult<Elem> {
    let flags = r.u32()?;
    // decode a funcidx vector into ref.func expressions
    fn func_exprs(r: &mut Reader) -> RResult<Vec<Vec<Instr>>> {
        let n = vec_len(r)?;
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            let idx = r.u32()?;
            v.push(vec![Instr::RefFunc(idx), Instr::End]);
        }
        Ok(v)
    }
    fn expr_list(r: &mut Reader) -> RResult<Vec<Vec<Instr>>> {
        let n = vec_len(r)?;
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            v.push(decode_expr(r)?);
        }
        Ok(v)
    }
    match flags {
        0 => {
            let offset = decode_expr(r)?;
            let init = func_exprs(r)?;
            Ok(Elem {
                ty: ValType::FuncRef,
                init,
                mode: ElemMode::Active { table: 0, offset },
            })
        }
        1 => {
            let kind = r.byte()?;
            if kind != 0x00 {
                return Err(format!("invalid element kind 0x{:02x}", kind));
            }
            let init = func_exprs(r)?;
            Ok(Elem {
                ty: ValType::FuncRef,
                init,
                mode: ElemMode::Passive,
            })
        }
        2 => {
            let table = r.u32()?;
            let offset = decode_expr(r)?;
            let kind = r.byte()?;
            if kind != 0x00 {
                return Err(format!("invalid element kind 0x{:02x}", kind));
            }
            let init = func_exprs(r)?;
            Ok(Elem {
                ty: ValType::FuncRef,
                init,
                mode: ElemMode::Active { table, offset },
            })
        }
        3 => {
            let kind = r.byte()?;
            if kind != 0x00 {
                return Err(format!("invalid element kind 0x{:02x}", kind));
            }
            let init = func_exprs(r)?;
            Ok(Elem {
                ty: ValType::FuncRef,
                init,
                mode: ElemMode::Declarative,
            })
        }
        4 => {
            let offset = decode_expr(r)?;
            let init = expr_list(r)?;
            Ok(Elem {
                ty: ValType::FuncRef,
                init,
                mode: ElemMode::Active { table: 0, offset },
            })
        }
        5 => {
            let ty = read_reftype(r)?;
            let init = expr_list(r)?;
            Ok(Elem {
                ty,
                init,
                mode: ElemMode::Passive,
            })
        }
        6 => {
            let table = r.u32()?;
            let offset = decode_expr(r)?;
            let ty = read_reftype(r)?;
            let init = expr_list(r)?;
            Ok(Elem {
                ty,
                init,
                mode: ElemMode::Active { table, offset },
            })
        }
        7 => {
            let ty = read_reftype(r)?;
            let init = expr_list(r)?;
            Ok(Elem {
                ty,
                init,
                mode: ElemMode::Declarative,
            })
        }
        _ => Err(format!("invalid element segment flags {}", flags)),
    }
}

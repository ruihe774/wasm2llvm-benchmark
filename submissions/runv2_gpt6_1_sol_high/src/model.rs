#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ty {
    I32,
    I64,
    F32,
    F64,
    V128,
}
impl Ty {
    pub fn llvm(self) -> &'static str {
        match self {
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::F32 => "float",
            Self::F64 => "double",
            Self::V128 => "<16 x i8>",
        }
    }
    pub fn zero(self) -> &'static str {
        match self {
            Self::F32 | Self::F64 => "0.0",
            Self::V128 => "zeroinitializer",
            _ => "0",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FuncType {
    pub params: Vec<Ty>,
    pub results: Vec<Ty>,
}
#[derive(Clone, Debug, Default)]
pub struct Instr {
    pub op: u32,
    pub a: u32,
    pub b: u32,
    pub imm: i64,
    pub bytes: [u8; 16],
    pub list: Vec<u32>,
}
// Ordinary opcode = its byte. Prefixed opcodes = 0xfc00/sub or 0xfd00/sub (addition).
// block/loop/if: imm = signed blocktype. Integers: imm; floats/vector const: bytes.
// memarg: a = alignment exponent, b = offset. SIMD lane immediates: imm.
// br_table: list = explicit labels, a = default. call_indirect: a = type, b = table.
// fc immediates: a = first index, b = second index.
#[derive(Clone, Debug)]
pub struct Func {
    pub type_idx: u32,
    pub import: Option<(String, String)>,
    pub locals: Vec<Ty>,
    pub code: Vec<Instr>,
}
#[derive(Clone, Debug)]
pub struct Limits {
    pub min: u32,
    pub max: Option<u32>,
}
#[derive(Clone, Debug)]
pub struct Global {
    pub ty: Ty,
    pub mutable: bool,
    pub init: Vec<Instr>,
}
#[derive(Clone, Debug)]
pub struct Export {
    pub name: String,
    pub kind: u8,
    pub index: u32,
}
#[derive(Clone, Debug)]
pub struct Data {
    pub passive: bool,
    pub offset: Vec<Instr>,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug)]
pub struct Elem {
    pub passive: bool,
    pub declarative: bool,
    pub offset: Vec<Instr>,
    pub funcs: Vec<u32>,
}
#[derive(Clone, Debug, Default)]
pub struct Module {
    pub types: Vec<FuncType>,
    pub funcs: Vec<Func>,
    pub memory: Option<Limits>,
    pub table: Option<Limits>,
    pub globals: Vec<Global>,
    pub exports: Vec<Export>,
    pub start: Option<u32>,
    pub data: Vec<Data>,
    pub elems: Vec<Elem>,
    pub data_count: Option<u32>,
}

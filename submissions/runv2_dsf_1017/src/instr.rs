// Instruction representation and binary decoding.
//
// Supported scope: WebAssembly 2.0 core (including reference types) plus the
// proposals exercised by the payloads: bulk memory, multi-value,
// non-trapping float-to-int, sign extension and full SIMD.  Anything outside
// that scope is rejected with an error.

use crate::module::{read_reftype, read_valtype, ValType};
use crate::reader::{Reader, RResult};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemArg {
    pub align: u32,
    pub offset: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadKind {
    I32,
    I64,
    F32,
    F64,
    I32_8S,
    I32_8U,
    I32_16S,
    I32_16U,
    I64_8S,
    I64_8U,
    I64_16S,
    I64_16U,
    I64_32S,
    I64_32U,
}

impl LoadKind {
    /// Natural access size in bytes.
    pub fn bytes(&self) -> u32 {
        match self {
            LoadKind::I32 | LoadKind::F32 | LoadKind::I64_32S | LoadKind::I64_32U => 4,
            LoadKind::I64 | LoadKind::F64 => 8,
            LoadKind::I32_8S | LoadKind::I32_8U | LoadKind::I64_8S | LoadKind::I64_8U => 1,
            LoadKind::I32_16S | LoadKind::I32_16U | LoadKind::I64_16S | LoadKind::I64_16U => 2,
        }
    }
    pub fn result(&self) -> ValType {
        match self {
            LoadKind::I32
            | LoadKind::I32_8S
            | LoadKind::I32_8U
            | LoadKind::I32_16S
            | LoadKind::I32_16U => ValType::I32,
            LoadKind::I64
            | LoadKind::I64_8S
            | LoadKind::I64_8U
            | LoadKind::I64_16S
            | LoadKind::I64_16U
            | LoadKind::I64_32S
            | LoadKind::I64_32U => ValType::I64,
            LoadKind::F32 => ValType::F32,
            LoadKind::F64 => ValType::F64,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreKind {
    I32,
    I64,
    F32,
    F64,
    I32_8,
    I32_16,
    I64_8,
    I64_16,
    I64_32,
}

impl StoreKind {
    pub fn bytes(&self) -> u32 {
        match self {
            StoreKind::I32 | StoreKind::F32 | StoreKind::I64_32 => 4,
            StoreKind::I64 | StoreKind::F64 => 8,
            StoreKind::I32_8 | StoreKind::I64_8 => 1,
            StoreKind::I32_16 | StoreKind::I64_16 => 2,
        }
    }
    pub fn value_type(&self) -> ValType {
        match self {
            StoreKind::I32 | StoreKind::I32_8 | StoreKind::I32_16 => ValType::I32,
            StoreKind::I64 | StoreKind::I64_8 | StoreKind::I64_16 | StoreKind::I64_32 => ValType::I64,
            StoreKind::F32 => ValType::F32,
            StoreKind::F64 => ValType::F64,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    I32Clz,
    I32Ctz,
    I32Popcnt,
    I64Clz,
    I64Ctz,
    I64Popcnt,
    F32Abs,
    F32Neg,
    F32Ceil,
    F32Floor,
    F32Trunc,
    F32Nearest,
    F32Sqrt,
    F64Abs,
    F64Neg,
    F64Ceil,
    F64Floor,
    F64Trunc,
    F64Nearest,
    F64Sqrt,
    I32Extend8S,
    I32Extend16S,
    I64Extend8S,
    I64Extend16S,
    I64Extend32S,
}

impl UnOp {
    pub fn ty(&self) -> ValType {
        match self {
            UnOp::I32Clz
            | UnOp::I32Ctz
            | UnOp::I32Popcnt
            | UnOp::I32Extend8S
            | UnOp::I32Extend16S => ValType::I32,
            UnOp::I64Clz
            | UnOp::I64Ctz
            | UnOp::I64Popcnt
            | UnOp::I64Extend8S
            | UnOp::I64Extend16S
            | UnOp::I64Extend32S => ValType::I64,
            UnOp::F32Abs
            | UnOp::F32Neg
            | UnOp::F32Ceil
            | UnOp::F32Floor
            | UnOp::F32Trunc
            | UnOp::F32Nearest
            | UnOp::F32Sqrt => ValType::F32,
            _ => ValType::F64,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    I32Add,
    I32Sub,
    I32Mul,
    I32DivS,
    I32DivU,
    I32RemS,
    I32RemU,
    I32And,
    I32Or,
    I32Xor,
    I32Shl,
    I32ShrS,
    I32ShrU,
    I32Rotl,
    I32Rotr,
    I64Add,
    I64Sub,
    I64Mul,
    I64DivS,
    I64DivU,
    I64RemS,
    I64RemU,
    I64And,
    I64Or,
    I64Xor,
    I64Shl,
    I64ShrS,
    I64ShrU,
    I64Rotl,
    I64Rotr,
    F32Add,
    F32Sub,
    F32Mul,
    F32Div,
    F32Min,
    F32Max,
    F32Copysign,
    F64Add,
    F64Sub,
    F64Mul,
    F64Div,
    F64Min,
    F64Max,
    F64Copysign,
}

impl BinOp {
    pub fn ty(&self) -> ValType {
        match self {
            BinOp::I32Add
            | BinOp::I32Sub
            | BinOp::I32Mul
            | BinOp::I32DivS
            | BinOp::I32DivU
            | BinOp::I32RemS
            | BinOp::I32RemU
            | BinOp::I32And
            | BinOp::I32Or
            | BinOp::I32Xor
            | BinOp::I32Shl
            | BinOp::I32ShrS
            | BinOp::I32ShrU
            | BinOp::I32Rotl
            | BinOp::I32Rotr => ValType::I32,
            BinOp::I64Add
            | BinOp::I64Sub
            | BinOp::I64Mul
            | BinOp::I64DivS
            | BinOp::I64DivU
            | BinOp::I64RemS
            | BinOp::I64RemU
            | BinOp::I64And
            | BinOp::I64Or
            | BinOp::I64Xor
            | BinOp::I64Shl
            | BinOp::I64ShrS
            | BinOp::I64ShrU
            | BinOp::I64Rotl
            | BinOp::I64Rotr => ValType::I64,
            BinOp::F32Add
            | BinOp::F32Sub
            | BinOp::F32Mul
            | BinOp::F32Div
            | BinOp::F32Min
            | BinOp::F32Max
            | BinOp::F32Copysign => ValType::F32,
            _ => ValType::F64,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestOp {
    I32Eqz,
    I64Eqz,
}

impl TestOp {
    pub fn ty(&self) -> ValType {
        match self {
            TestOp::I32Eqz => ValType::I32,
            TestOp::I64Eqz => ValType::I64,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelOp {
    I32Eq,
    I32Ne,
    I32LtS,
    I32LtU,
    I32GtS,
    I32GtU,
    I32LeS,
    I32LeU,
    I32GeS,
    I32GeU,
    I64Eq,
    I64Ne,
    I64LtS,
    I64LtU,
    I64GtS,
    I64GtU,
    I64LeS,
    I64LeU,
    I64GeS,
    I64GeU,
    F32Eq,
    F32Ne,
    F32Lt,
    F32Gt,
    F32Le,
    F32Ge,
    F64Eq,
    F64Ne,
    F64Lt,
    F64Gt,
    F64Le,
    F64Ge,
}

impl RelOp {
    pub fn ty(&self) -> ValType {
        match self {
            RelOp::I32Eq
            | RelOp::I32Ne
            | RelOp::I32LtS
            | RelOp::I32LtU
            | RelOp::I32GtS
            | RelOp::I32GtU
            | RelOp::I32LeS
            | RelOp::I32LeU
            | RelOp::I32GeS
            | RelOp::I32GeU => ValType::I32,
            RelOp::I64Eq
            | RelOp::I64Ne
            | RelOp::I64LtS
            | RelOp::I64LtU
            | RelOp::I64GtS
            | RelOp::I64GtU
            | RelOp::I64LeS
            | RelOp::I64LeU
            | RelOp::I64GeS
            | RelOp::I64GeU => ValType::I64,
            RelOp::F32Eq | RelOp::F32Ne | RelOp::F32Lt | RelOp::F32Gt | RelOp::F32Le
            | RelOp::F32Ge => ValType::F32,
            _ => ValType::F64,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CvtOp {
    I32WrapI64,
    I32TruncF32S,
    I32TruncF32U,
    I32TruncF64S,
    I32TruncF64U,
    I64ExtendI32S,
    I64ExtendI32U,
    I64TruncF32S,
    I64TruncF32U,
    I64TruncF64S,
    I64TruncF64U,
    F32ConvertI32S,
    F32ConvertI32U,
    F32ConvertI64S,
    F32ConvertI64U,
    F32DemoteF64,
    F64ConvertI32S,
    F64ConvertI32U,
    F64ConvertI64S,
    F64ConvertI64U,
    F64PromoteF32,
    I32ReinterpretF32,
    I64ReinterpretF64,
    F32ReinterpretI32,
    F64ReinterpretI64,
    I32TruncSatF32S,
    I32TruncSatF32U,
    I32TruncSatF64S,
    I32TruncSatF64U,
    I64TruncSatF32S,
    I64TruncSatF32U,
    I64TruncSatF64S,
    I64TruncSatF64U,
}

impl CvtOp {
    pub fn sig(&self) -> (ValType, ValType) {
        use CvtOp::*;
        match self {
            I32WrapI64 => (ValType::I64, ValType::I32),
            I32TruncF32S | I32TruncF32U | I32TruncSatF32S | I32TruncSatF32U => {
                (ValType::F32, ValType::I32)
            }
            I32TruncF64S | I32TruncF64U | I32TruncSatF64S | I32TruncSatF64U => {
                (ValType::F64, ValType::I32)
            }
            I64ExtendI32S | I64ExtendI32U => (ValType::I32, ValType::I64),
            I64TruncF32S | I64TruncF32U | I64TruncSatF32S | I64TruncSatF32U => {
                (ValType::F32, ValType::I64)
            }
            I64TruncF64S | I64TruncF64U | I64TruncSatF64S | I64TruncSatF64U => {
                (ValType::F64, ValType::I64)
            }
            F32ConvertI32S | F32ConvertI32U => (ValType::I32, ValType::F32),
            F32ConvertI64S | F32ConvertI64U => (ValType::I64, ValType::F32),
            F32DemoteF64 => (ValType::F64, ValType::F32),
            F64ConvertI32S | F64ConvertI32U => (ValType::I32, ValType::F64),
            F64ConvertI64S | F64ConvertI64U => (ValType::I64, ValType::F64),
            F64PromoteF32 => (ValType::F32, ValType::F64),
            I32ReinterpretF32 => (ValType::F32, ValType::I32),
            I64ReinterpretF64 => (ValType::F64, ValType::I64),
            F32ReinterpretI32 => (ValType::I32, ValType::F32),
            F64ReinterpretI64 => (ValType::I64, ValType::F64),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimdLaneOp {
    I8x16ExtractS,
    I8x16ExtractU,
    I8x16Replace,
    I16x8ExtractS,
    I16x8ExtractU,
    I16x8Replace,
    I32x4Extract,
    I32x4Replace,
    I64x2Extract,
    I64x2Replace,
    F32x4Extract,
    F32x4Replace,
    F64x2Extract,
    F64x2Replace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimdMemLaneOp {
    Load8,
    Load16,
    Load32,
    Load64,
    Store8,
    Store16,
    Store32,
    Store64,
}

impl SimdMemLaneOp {
    pub fn bytes(&self) -> u32 {
        match self {
            SimdMemLaneOp::Load8 | SimdMemLaneOp::Store8 => 1,
            SimdMemLaneOp::Load16 | SimdMemLaneOp::Store16 => 2,
            SimdMemLaneOp::Load32 | SimdMemLaneOp::Store32 => 4,
            SimdMemLaneOp::Load64 | SimdMemLaneOp::Store64 => 8,
        }
    }
    pub fn is_store(&self) -> bool {
        matches!(
            self,
            SimdMemLaneOp::Store8
                | SimdMemLaneOp::Store16
                | SimdMemLaneOp::Store32
                | SimdMemLaneOp::Store64
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimdLoadOp {
    Load8x8S,
    Load8x8U,
    Load16x4S,
    Load16x4U,
    Load32x2S,
    Load32x2U,
    Load8Splat,
    Load16Splat,
    Load32Splat,
    Load64Splat,
    Load32Zero,
    Load64Zero,
}

impl SimdLoadOp {
    pub fn bytes(&self) -> u32 {
        match self {
            SimdLoadOp::Load8x8S | SimdLoadOp::Load8x8U => 8,
            SimdLoadOp::Load16x4S | SimdLoadOp::Load16x4U => 8,
            SimdLoadOp::Load32x2S | SimdLoadOp::Load32x2U => 8,
            SimdLoadOp::Load8Splat => 1,
            SimdLoadOp::Load16Splat => 2,
            SimdLoadOp::Load32Splat | SimdLoadOp::Load32Zero => 4,
            SimdLoadOp::Load64Splat | SimdLoadOp::Load64Zero => 8,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimdOp {
    // bitwise
    Not,
    And,
    AndNot,
    Or,
    Xor,
    Bitselect,
    AnyTrue,
    // splats
    I8x16Splat,
    I16x8Splat,
    I32x4Splat,
    I64x2Splat,
    F32x4Splat,
    F64x2Splat,
    I8x16Swizzle,
    // comparisons
    I8x16Eq,
    I8x16Ne,
    I8x16LtS,
    I8x16LtU,
    I8x16GtS,
    I8x16GtU,
    I8x16LeS,
    I8x16LeU,
    I8x16GeS,
    I8x16GeU,
    I16x8Eq,
    I16x8Ne,
    I16x8LtS,
    I16x8LtU,
    I16x8GtS,
    I16x8GtU,
    I16x8LeS,
    I16x8LeU,
    I16x8GeS,
    I16x8GeU,
    I32x4Eq,
    I32x4Ne,
    I32x4LtS,
    I32x4LtU,
    I32x4GtS,
    I32x4GtU,
    I32x4LeS,
    I32x4LeU,
    I32x4GeS,
    I32x4GeU,
    I64x2Eq,
    I64x2Ne,
    I64x2LtS,
    I64x2GtS,
    I64x2LeS,
    I64x2GeS,
    F32x4Eq,
    F32x4Ne,
    F32x4Lt,
    F32x4Gt,
    F32x4Le,
    F32x4Ge,
    F64x2Eq,
    F64x2Ne,
    F64x2Lt,
    F64x2Gt,
    F64x2Le,
    F64x2Ge,
    // i8x16
    I8x16Abs,
    I8x16Neg,
    I8x16Popcnt,
    I8x16AllTrue,
    I8x16Bitmask,
    I8x16NarrowI16x8S,
    I8x16NarrowI16x8U,
    I8x16Shl,
    I8x16ShrS,
    I8x16ShrU,
    I8x16Add,
    I8x16AddSatS,
    I8x16AddSatU,
    I8x16Sub,
    I8x16SubSatS,
    I8x16SubSatU,
    I8x16MinS,
    I8x16MinU,
    I8x16MaxS,
    I8x16MaxU,
    I8x16AvgrU,
    // i16x8
    I16x8ExtaddPairwiseI8x16S,
    I16x8ExtaddPairwiseI8x16U,
    I16x8Abs,
    I16x8Neg,
    I16x8Q15mulrSatS,
    I16x8AllTrue,
    I16x8Bitmask,
    I16x8NarrowI32x4S,
    I16x8NarrowI32x4U,
    I16x8ExtendLowI8x16S,
    I16x8ExtendHighI8x16S,
    I16x8ExtendLowI8x16U,
    I16x8ExtendHighI8x16U,
    I16x8Shl,
    I16x8ShrS,
    I16x8ShrU,
    I16x8Add,
    I16x8AddSatS,
    I16x8AddSatU,
    I16x8Sub,
    I16x8SubSatS,
    I16x8SubSatU,
    I16x8Mul,
    I16x8MinS,
    I16x8MinU,
    I16x8MaxS,
    I16x8MaxU,
    I16x8AvgrU,
    I16x8ExtmulLowI8x16S,
    I16x8ExtmulHighI8x16S,
    I16x8ExtmulLowI8x16U,
    I16x8ExtmulHighI8x16U,
    // i32x4
    I32x4ExtaddPairwiseI16x8S,
    I32x4ExtaddPairwiseI16x8U,
    I32x4Abs,
    I32x4Neg,
    I32x4AllTrue,
    I32x4Bitmask,
    I32x4ExtendLowI16x8S,
    I32x4ExtendHighI16x8S,
    I32x4ExtendLowI16x8U,
    I32x4ExtendHighI16x8U,
    I32x4Shl,
    I32x4ShrS,
    I32x4ShrU,
    I32x4Add,
    I32x4Sub,
    I32x4Mul,
    I32x4MinS,
    I32x4MinU,
    I32x4MaxS,
    I32x4MaxU,
    I32x4DotI16x8S,
    I32x4ExtmulLowI16x8S,
    I32x4ExtmulHighI16x8S,
    I32x4ExtmulLowI16x8U,
    I32x4ExtmulHighI16x8U,
    // i64x2
    I64x2Abs,
    I64x2Neg,
    I64x2AllTrue,
    I64x2Bitmask,
    I64x2ExtendLowI32x4S,
    I64x2ExtendHighI32x4S,
    I64x2ExtendLowI32x4U,
    I64x2ExtendHighI32x4U,
    I64x2Shl,
    I64x2ShrS,
    I64x2ShrU,
    I64x2Add,
    I64x2Sub,
    I64x2Mul,
    I64x2ExtmulLowI32x4S,
    I64x2ExtmulHighI32x4S,
    I64x2ExtmulLowI32x4U,
    I64x2ExtmulHighI32x4U,
    // f32x4
    F32x4Ceil,
    F32x4Floor,
    F32x4Trunc,
    F32x4Nearest,
    F32x4Abs,
    F32x4Neg,
    F32x4Sqrt,
    F32x4Add,
    F32x4Sub,
    F32x4Mul,
    F32x4Div,
    F32x4Min,
    F32x4Max,
    F32x4Pmin,
    F32x4Pmax,
    // f64x2
    F64x2Ceil,
    F64x2Floor,
    F64x2Trunc,
    F64x2Nearest,
    F64x2Abs,
    F64x2Neg,
    F64x2Sqrt,
    F64x2Add,
    F64x2Sub,
    F64x2Mul,
    F64x2Div,
    F64x2Min,
    F64x2Max,
    F64x2Pmin,
    F64x2Pmax,
    // conversions
    I32x4TruncSatF32x4S,
    I32x4TruncSatF32x4U,
    F32x4ConvertI32x4S,
    F32x4ConvertI32x4U,
    I32x4TruncSatF64x2SZero,
    I32x4TruncSatF64x2UZero,
    F64x2ConvertLowI32x4S,
    F64x2ConvertLowI32x4U,
    F32x4DemoteF64x2Zero,
    F64x2PromoteLowF32x4,
}

#[derive(Clone, Copy, Debug)]
pub enum BlockType {
    Empty,
    Val(ValType),
    TypeIdx(u32),
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
    CallIndirect(u32, u32),
    Drop,
    Select,
    SelectT(ValType),
    LocalGet(u32),
    LocalSet(u32),
    LocalTee(u32),
    GlobalGet(u32),
    GlobalSet(u32),
    TableGet(u32),
    TableSet(u32),
    TableInit(u32, u32),
    ElemDrop(u32),
    TableCopy(u32, u32),
    TableGrow(u32),
    TableSize(u32),
    TableFill(u32),
    MemorySize,
    MemoryGrow,
    MemoryInit(u32),
    DataDrop(u32),
    MemoryCopy,
    MemoryFill,
    Load(LoadKind, MemArg),
    Store(StoreKind, MemArg),
    I32Const(i32),
    I64Const(i64),
    F32Const(u32),
    F64Const(u64),
    RefNull(ValType),
    RefIsNull,
    RefFunc(u32),
    Un(UnOp),
    Bin(BinOp),
    Test(TestOp),
    Rel(RelOp),
    Cvt(CvtOp),
    V128Const([u8; 16]),
    V128Load(MemArg),
    V128Store(MemArg),
    V128LoadLane(SimdMemLaneOp, MemArg, u8),
    V128LoadExt(SimdLoadOp, MemArg),
    I8x16Shuffle([u8; 16]),
    SimdLane(SimdLaneOp, u8),
    Simd(SimdOp),
}

fn read_memarg(r: &mut Reader) -> RResult<MemArg> {
    let align = r.u32()?;
    let offset = r.u32()?;
    Ok(MemArg { align, offset })
}

fn read_blocktype(r: &mut Reader) -> RResult<BlockType> {
    let b = r.peek()?;
    if b == 0x40 {
        r.byte()?;
        return Ok(BlockType::Empty);
    }
    if let Some(t) = ValType::from_byte(b) {
        r.byte()?;
        return Ok(BlockType::Val(t));
    }
    let x = r.s33()?;
    if x < 0 || x > u32::MAX as i64 {
        return Err("invalid block type".to_string());
    }
    Ok(BlockType::TypeIdx(x as u32))
}

/// Decode an expression (instruction sequence terminated by `end` at depth 0).
pub fn decode_expr(r: &mut Reader) -> RResult<Vec<Instr>> {
    let mut v = Vec::new();
    let mut depth: u32 = 0;
    loop {
        let op = r.byte()?;
        match op {
            0x00 => v.push(Instr::Unreachable),
            0x01 => v.push(Instr::Nop),
            0x02 => {
                let bt = read_blocktype(r)?;
                v.push(Instr::Block(bt));
                depth += 1;
            }
            0x03 => {
                let bt = read_blocktype(r)?;
                v.push(Instr::Loop(bt));
                depth += 1;
            }
            0x04 => {
                let bt = read_blocktype(r)?;
                v.push(Instr::If(bt));
                depth += 1;
            }
            0x05 => v.push(Instr::Else),
            0x0b => {
                v.push(Instr::End);
                if depth == 0 {
                    return Ok(v);
                }
                depth -= 1;
            }
            0x0c => v.push(Instr::Br(r.u32()?)),
            0x0d => v.push(Instr::BrIf(r.u32()?)),
            0x0e => {
                let n = r.u32()? as usize;
                if n > r.remaining() {
                    return Err("br_table target count exceeds input".to_string());
                }
                let mut targets = Vec::with_capacity(n);
                for _ in 0..n {
                    targets.push(r.u32()?);
                }
                let default = r.u32()?;
                v.push(Instr::BrTable(targets, default));
            }
            0x0f => v.push(Instr::Return),
            0x10 => v.push(Instr::Call(r.u32()?)),
            0x11 => {
                let ty = r.u32()?;
                let table = r.u32()?;
                v.push(Instr::CallIndirect(ty, table));
            }
            0x12 | 0x13 | 0x15 => {
                return Err("tail-call instructions are out of scope".to_string())
            }
            0x14 => {
                return Err("typed function reference instructions are out of scope".to_string())
            }
            0x1a => v.push(Instr::Drop),
            0x1b => v.push(Instr::Select),
            0x1c => {
                let n = r.u32()?;
                if n != 1 {
                    return Err("select with more than one type is out of scope".to_string());
                }
                let t = read_valtype(r)?;
                v.push(Instr::SelectT(t));
            }
            0x20 => v.push(Instr::LocalGet(r.u32()?)),
            0x21 => v.push(Instr::LocalSet(r.u32()?)),
            0x22 => v.push(Instr::LocalTee(r.u32()?)),
            0x23 => v.push(Instr::GlobalGet(r.u32()?)),
            0x24 => v.push(Instr::GlobalSet(r.u32()?)),
            0x25 => v.push(Instr::TableGet(r.u32()?)),
            0x26 => v.push(Instr::TableSet(r.u32()?)),
            0x28 => v.push(Instr::Load(LoadKind::I32, read_memarg(r)?)),
            0x29 => v.push(Instr::Load(LoadKind::I64, read_memarg(r)?)),
            0x2a => v.push(Instr::Load(LoadKind::F32, read_memarg(r)?)),
            0x2b => v.push(Instr::Load(LoadKind::F64, read_memarg(r)?)),
            0x2c => v.push(Instr::Load(LoadKind::I32_8S, read_memarg(r)?)),
            0x2d => v.push(Instr::Load(LoadKind::I32_8U, read_memarg(r)?)),
            0x2e => v.push(Instr::Load(LoadKind::I32_16S, read_memarg(r)?)),
            0x2f => v.push(Instr::Load(LoadKind::I32_16U, read_memarg(r)?)),
            0x30 => v.push(Instr::Load(LoadKind::I64_8S, read_memarg(r)?)),
            0x31 => v.push(Instr::Load(LoadKind::I64_8U, read_memarg(r)?)),
            0x32 => v.push(Instr::Load(LoadKind::I64_16S, read_memarg(r)?)),
            0x33 => v.push(Instr::Load(LoadKind::I64_16U, read_memarg(r)?)),
            0x34 => v.push(Instr::Load(LoadKind::I64_32S, read_memarg(r)?)),
            0x35 => v.push(Instr::Load(LoadKind::I64_32U, read_memarg(r)?)),
            0x36 => v.push(Instr::Store(StoreKind::I32, read_memarg(r)?)),
            0x37 => v.push(Instr::Store(StoreKind::I64, read_memarg(r)?)),
            0x38 => v.push(Instr::Store(StoreKind::F32, read_memarg(r)?)),
            0x39 => v.push(Instr::Store(StoreKind::F64, read_memarg(r)?)),
            0x3a => v.push(Instr::Store(StoreKind::I32_8, read_memarg(r)?)),
            0x3b => v.push(Instr::Store(StoreKind::I32_16, read_memarg(r)?)),
            0x3c => v.push(Instr::Store(StoreKind::I64_8, read_memarg(r)?)),
            0x3d => v.push(Instr::Store(StoreKind::I64_16, read_memarg(r)?)),
            0x3e => v.push(Instr::Store(StoreKind::I64_32, read_memarg(r)?)),
            0x3f => {
                let b = r.byte()?;
                if b != 0 {
                    return Err("nonzero memory index (multi-memory) is out of scope".to_string());
                }
                v.push(Instr::MemorySize);
            }
            0x40 => {
                let b = r.byte()?;
                if b != 0 {
                    return Err("nonzero memory index (multi-memory) is out of scope".to_string());
                }
                v.push(Instr::MemoryGrow);
            }
            0x41 => v.push(Instr::I32Const(r.s32()?)),
            0x42 => v.push(Instr::I64Const(r.s64()?)),
            0x43 => v.push(Instr::F32Const(r.f32bits()?)),
            0x44 => v.push(Instr::F64Const(r.f64bits()?)),
            0x45..=0xc4 => {
                v.push(decode_numeric(op)?);
            }
            0xd0 => {
                let t = read_reftype(r)?;
                v.push(Instr::RefNull(t));
            }
            0xd1 => v.push(Instr::RefIsNull),
            0xd2 => v.push(Instr::RefFunc(r.u32()?)),
            0xd3..=0xd6 => {
                return Err("typed function reference instructions are out of scope".to_string())
            }
            0xfb => {
                return Err("GC instructions are out of scope".to_string());
            }
            0xfc => {
                v.push(decode_fc(r)?);
            }
            0xfd => {
                v.push(decode_simd(r)?);
            }
            0xfe => {
                return Err("threads/atomic instructions are out of scope".to_string());
            }
            0x06 | 0x07 | 0x08 | 0x09 | 0x0a | 0x18 | 0x19 | 0x1f => {
                return Err("exception handling instructions are out of scope".to_string());
            }
            _ => {
                return Err(format!("unsupported opcode 0x{:02x}", op));
            }
        }
    }
}

fn decode_numeric(op: u8) -> RResult<Instr> {
    use BinOp::*;
    use CvtOp::*;
    use RelOp::*;
    use TestOp::*;
    use UnOp::*;
    let i = match op {
        0x45 => Instr::Test(I32Eqz),
        0x46 => Instr::Rel(I32Eq),
        0x47 => Instr::Rel(I32Ne),
        0x48 => Instr::Rel(I32LtS),
        0x49 => Instr::Rel(I32LtU),
        0x4a => Instr::Rel(I32GtS),
        0x4b => Instr::Rel(I32GtU),
        0x4c => Instr::Rel(I32LeS),
        0x4d => Instr::Rel(I32LeU),
        0x4e => Instr::Rel(I32GeS),
        0x4f => Instr::Rel(I32GeU),
        0x50 => Instr::Test(I64Eqz),
        0x51 => Instr::Rel(I64Eq),
        0x52 => Instr::Rel(I64Ne),
        0x53 => Instr::Rel(I64LtS),
        0x54 => Instr::Rel(I64LtU),
        0x55 => Instr::Rel(I64GtS),
        0x56 => Instr::Rel(I64GtU),
        0x57 => Instr::Rel(I64LeS),
        0x58 => Instr::Rel(I64LeU),
        0x59 => Instr::Rel(I64GeS),
        0x5a => Instr::Rel(I64GeU),
        0x5b => Instr::Rel(F32Eq),
        0x5c => Instr::Rel(F32Ne),
        0x5d => Instr::Rel(F32Lt),
        0x5e => Instr::Rel(F32Gt),
        0x5f => Instr::Rel(F32Le),
        0x60 => Instr::Rel(F32Ge),
        0x61 => Instr::Rel(F64Eq),
        0x62 => Instr::Rel(F64Ne),
        0x63 => Instr::Rel(F64Lt),
        0x64 => Instr::Rel(F64Gt),
        0x65 => Instr::Rel(F64Le),
        0x66 => Instr::Rel(F64Ge),
        0x67 => Instr::Un(I32Clz),
        0x68 => Instr::Un(I32Ctz),
        0x69 => Instr::Un(I32Popcnt),
        0x6a => Instr::Bin(I32Add),
        0x6b => Instr::Bin(I32Sub),
        0x6c => Instr::Bin(I32Mul),
        0x6d => Instr::Bin(I32DivS),
        0x6e => Instr::Bin(I32DivU),
        0x6f => Instr::Bin(I32RemS),
        0x70 => Instr::Bin(I32RemU),
        0x71 => Instr::Bin(I32And),
        0x72 => Instr::Bin(I32Or),
        0x73 => Instr::Bin(I32Xor),
        0x74 => Instr::Bin(I32Shl),
        0x75 => Instr::Bin(I32ShrS),
        0x76 => Instr::Bin(I32ShrU),
        0x77 => Instr::Bin(I32Rotl),
        0x78 => Instr::Bin(I32Rotr),
        0x79 => Instr::Un(I64Clz),
        0x7a => Instr::Un(I64Ctz),
        0x7b => Instr::Un(I64Popcnt),
        0x7c => Instr::Bin(I64Add),
        0x7d => Instr::Bin(I64Sub),
        0x7e => Instr::Bin(I64Mul),
        0x7f => Instr::Bin(I64DivS),
        0x80 => Instr::Bin(I64DivU),
        0x81 => Instr::Bin(I64RemS),
        0x82 => Instr::Bin(I64RemU),
        0x83 => Instr::Bin(I64And),
        0x84 => Instr::Bin(I64Or),
        0x85 => Instr::Bin(I64Xor),
        0x86 => Instr::Bin(I64Shl),
        0x87 => Instr::Bin(I64ShrS),
        0x88 => Instr::Bin(I64ShrU),
        0x89 => Instr::Bin(I64Rotl),
        0x8a => Instr::Bin(I64Rotr),
        0x8b => Instr::Un(F32Abs),
        0x8c => Instr::Un(F32Neg),
        0x8d => Instr::Un(F32Ceil),
        0x8e => Instr::Un(F32Floor),
        0x8f => Instr::Un(F32Trunc),
        0x90 => Instr::Un(F32Nearest),
        0x91 => Instr::Un(F32Sqrt),
        0x92 => Instr::Bin(F32Add),
        0x93 => Instr::Bin(F32Sub),
        0x94 => Instr::Bin(F32Mul),
        0x95 => Instr::Bin(F32Div),
        0x96 => Instr::Bin(F32Min),
        0x97 => Instr::Bin(F32Max),
        0x98 => Instr::Bin(F32Copysign),
        0x99 => Instr::Un(F64Abs),
        0x9a => Instr::Un(F64Neg),
        0x9b => Instr::Un(F64Ceil),
        0x9c => Instr::Un(F64Floor),
        0x9d => Instr::Un(F64Trunc),
        0x9e => Instr::Un(F64Nearest),
        0x9f => Instr::Un(F64Sqrt),
        0xa0 => Instr::Bin(F64Add),
        0xa1 => Instr::Bin(F64Sub),
        0xa2 => Instr::Bin(F64Mul),
        0xa3 => Instr::Bin(F64Div),
        0xa4 => Instr::Bin(F64Min),
        0xa5 => Instr::Bin(F64Max),
        0xa6 => Instr::Bin(F64Copysign),
        0xa7 => Instr::Cvt(I32WrapI64),
        0xa8 => Instr::Cvt(I32TruncF32S),
        0xa9 => Instr::Cvt(I32TruncF32U),
        0xaa => Instr::Cvt(I32TruncF64S),
        0xab => Instr::Cvt(I32TruncF64U),
        0xac => Instr::Cvt(I64ExtendI32S),
        0xad => Instr::Cvt(I64ExtendI32U),
        0xae => Instr::Cvt(I64TruncF32S),
        0xaf => Instr::Cvt(I64TruncF32U),
        0xb0 => Instr::Cvt(I64TruncF64S),
        0xb1 => Instr::Cvt(I64TruncF64U),
        0xb2 => Instr::Cvt(F32ConvertI32S),
        0xb3 => Instr::Cvt(F32ConvertI32U),
        0xb4 => Instr::Cvt(F32ConvertI64S),
        0xb5 => Instr::Cvt(F32ConvertI64U),
        0xb6 => Instr::Cvt(F32DemoteF64),
        0xb7 => Instr::Cvt(F64ConvertI32S),
        0xb8 => Instr::Cvt(F64ConvertI32U),
        0xb9 => Instr::Cvt(F64ConvertI64S),
        0xba => Instr::Cvt(F64ConvertI64U),
        0xbb => Instr::Cvt(F64PromoteF32),
        0xbc => Instr::Cvt(I32ReinterpretF32),
        0xbd => Instr::Cvt(I64ReinterpretF64),
        0xbe => Instr::Cvt(F32ReinterpretI32),
        0xbf => Instr::Cvt(F64ReinterpretI64),
        0xc0 => Instr::Un(I32Extend8S),
        0xc1 => Instr::Un(I32Extend16S),
        0xc2 => Instr::Un(I64Extend8S),
        0xc3 => Instr::Un(I64Extend16S),
        0xc4 => Instr::Un(I64Extend32S),
        _ => return Err(format!("unsupported numeric opcode 0x{:02x}", op)),
    };
    Ok(i)
}

fn decode_fc(r: &mut Reader) -> RResult<Instr> {
    let sub = r.u32()?;
    Ok(match sub {
        0 => Instr::Cvt(CvtOp::I32TruncSatF32S),
        1 => Instr::Cvt(CvtOp::I32TruncSatF32U),
        2 => Instr::Cvt(CvtOp::I32TruncSatF64S),
        3 => Instr::Cvt(CvtOp::I32TruncSatF64U),
        4 => Instr::Cvt(CvtOp::I64TruncSatF32S),
        5 => Instr::Cvt(CvtOp::I64TruncSatF32U),
        6 => Instr::Cvt(CvtOp::I64TruncSatF64S),
        7 => Instr::Cvt(CvtOp::I64TruncSatF64U),
        8 => {
            let d = r.u32()?;
            let z = r.byte()?;
            if z != 0 {
                return Err("nonzero memory index in memory.init is out of scope".to_string());
            }
            Instr::MemoryInit(d)
        }
        9 => Instr::DataDrop(r.u32()?),
        10 => {
            let a = r.byte()?;
            let b = r.byte()?;
            if a != 0 || b != 0 {
                return Err("nonzero memory index in memory.copy is out of scope".to_string());
            }
            Instr::MemoryCopy
        }
        11 => {
            let z = r.byte()?;
            if z != 0 {
                return Err("nonzero memory index in memory.fill is out of scope".to_string());
            }
            Instr::MemoryFill
        }
        12 => {
            let e = r.u32()?;
            let t = r.u32()?;
            Instr::TableInit(e, t)
        }
        13 => Instr::ElemDrop(r.u32()?),
        14 => {
            let d = r.u32()?;
            let s = r.u32()?;
            Instr::TableCopy(d, s)
        }
        15 => Instr::TableGrow(r.u32()?),
        16 => Instr::TableSize(r.u32()?),
        17 => Instr::TableFill(r.u32()?),
        _ => {
            return Err(format!(
                "unsupported 0xfc instruction {} (bulk memory/reference types scope)",
                sub
            ))
        }
    })
}

fn simd_lane(idx: u8, max: u8) -> RResult<u8> {
    if idx >= max {
        Err(format!(
            "SIMD lane index {} out of range (max {})",
            idx,
            max - 1
        ))
    } else {
        Ok(idx)
    }
}

fn decode_simd(r: &mut Reader) -> RResult<Instr> {
    use SimdOp::*;
    let sub = r.u32()?;
    let ins = match sub {
        0 => Instr::V128Load(read_memarg(r)?),
        1 => Instr::V128LoadExt(SimdLoadOp::Load8x8S, read_memarg(r)?),
        2 => Instr::V128LoadExt(SimdLoadOp::Load8x8U, read_memarg(r)?),
        3 => Instr::V128LoadExt(SimdLoadOp::Load16x4S, read_memarg(r)?),
        4 => Instr::V128LoadExt(SimdLoadOp::Load16x4U, read_memarg(r)?),
        5 => Instr::V128LoadExt(SimdLoadOp::Load32x2S, read_memarg(r)?),
        6 => Instr::V128LoadExt(SimdLoadOp::Load32x2U, read_memarg(r)?),
        7 => Instr::V128LoadExt(SimdLoadOp::Load8Splat, read_memarg(r)?),
        8 => Instr::V128LoadExt(SimdLoadOp::Load16Splat, read_memarg(r)?),
        9 => Instr::V128LoadExt(SimdLoadOp::Load32Splat, read_memarg(r)?),
        10 => Instr::V128LoadExt(SimdLoadOp::Load64Splat, read_memarg(r)?),
        11 => Instr::V128Store(read_memarg(r)?),
        12 => {
            let b = r.bytes(16)?;
            let mut a = [0u8; 16];
            a.copy_from_slice(b);
            Instr::V128Const(a)
        }
        13 => {
            let b = r.bytes(16)?;
            let mut a = [0u8; 16];
            for (i, &x) in b.iter().enumerate() {
                if x >= 32 {
                    return Err(format!("i8x16.shuffle lane index {} out of range", x));
                }
                a[i] = x;
            }
            Instr::I8x16Shuffle(a)
        }
        14 => Instr::Simd(I8x16Swizzle),
        15 => Instr::Simd(I8x16Splat),
        16 => Instr::Simd(I16x8Splat),
        17 => Instr::Simd(I32x4Splat),
        18 => Instr::Simd(I64x2Splat),
        19 => Instr::Simd(F32x4Splat),
        20 => Instr::Simd(F64x2Splat),
        21 => Instr::SimdLane(SimdLaneOp::I8x16ExtractS, simd_lane(r.byte()?, 16)?),
        22 => Instr::SimdLane(SimdLaneOp::I8x16ExtractU, simd_lane(r.byte()?, 16)?),
        23 => Instr::SimdLane(SimdLaneOp::I8x16Replace, simd_lane(r.byte()?, 16)?),
        24 => Instr::SimdLane(SimdLaneOp::I16x8ExtractS, simd_lane(r.byte()?, 8)?),
        25 => Instr::SimdLane(SimdLaneOp::I16x8ExtractU, simd_lane(r.byte()?, 8)?),
        26 => Instr::SimdLane(SimdLaneOp::I16x8Replace, simd_lane(r.byte()?, 8)?),
        27 => Instr::SimdLane(SimdLaneOp::I32x4Extract, simd_lane(r.byte()?, 4)?),
        28 => Instr::SimdLane(SimdLaneOp::I32x4Replace, simd_lane(r.byte()?, 4)?),
        29 => Instr::SimdLane(SimdLaneOp::I64x2Extract, simd_lane(r.byte()?, 2)?),
        30 => Instr::SimdLane(SimdLaneOp::I64x2Replace, simd_lane(r.byte()?, 2)?),
        31 => Instr::SimdLane(SimdLaneOp::F32x4Extract, simd_lane(r.byte()?, 4)?),
        32 => Instr::SimdLane(SimdLaneOp::F32x4Replace, simd_lane(r.byte()?, 4)?),
        33 => Instr::SimdLane(SimdLaneOp::F64x2Extract, simd_lane(r.byte()?, 2)?),
        34 => Instr::SimdLane(SimdLaneOp::F64x2Replace, simd_lane(r.byte()?, 2)?),
        35 => Instr::Simd(I8x16Eq),
        36 => Instr::Simd(I8x16Ne),
        37 => Instr::Simd(I8x16LtS),
        38 => Instr::Simd(I8x16LtU),
        39 => Instr::Simd(I8x16GtS),
        40 => Instr::Simd(I8x16GtU),
        41 => Instr::Simd(I8x16LeS),
        42 => Instr::Simd(I8x16LeU),
        43 => Instr::Simd(I8x16GeS),
        44 => Instr::Simd(I8x16GeU),
        45 => Instr::Simd(I16x8Eq),
        46 => Instr::Simd(I16x8Ne),
        47 => Instr::Simd(I16x8LtS),
        48 => Instr::Simd(I16x8LtU),
        49 => Instr::Simd(I16x8GtS),
        50 => Instr::Simd(I16x8GtU),
        51 => Instr::Simd(I16x8LeS),
        52 => Instr::Simd(I16x8LeU),
        53 => Instr::Simd(I16x8GeS),
        54 => Instr::Simd(I16x8GeU),
        55 => Instr::Simd(I32x4Eq),
        56 => Instr::Simd(I32x4Ne),
        57 => Instr::Simd(I32x4LtS),
        58 => Instr::Simd(I32x4LtU),
        59 => Instr::Simd(I32x4GtS),
        60 => Instr::Simd(I32x4GtU),
        61 => Instr::Simd(I32x4LeS),
        62 => Instr::Simd(I32x4LeU),
        63 => Instr::Simd(I32x4GeS),
        64 => Instr::Simd(I32x4GeU),
        65 => Instr::Simd(F32x4Eq),
        66 => Instr::Simd(F32x4Ne),
        67 => Instr::Simd(F32x4Lt),
        68 => Instr::Simd(F32x4Gt),
        69 => Instr::Simd(F32x4Le),
        70 => Instr::Simd(F32x4Ge),
        71 => Instr::Simd(F64x2Eq),
        72 => Instr::Simd(F64x2Ne),
        73 => Instr::Simd(F64x2Lt),
        74 => Instr::Simd(F64x2Gt),
        75 => Instr::Simd(F64x2Le),
        76 => Instr::Simd(F64x2Ge),
        77 => Instr::Simd(Not),
        78 => Instr::Simd(And),
        79 => Instr::Simd(AndNot),
        80 => Instr::Simd(Or),
        81 => Instr::Simd(Xor),
        82 => Instr::Simd(Bitselect),
        83 => Instr::Simd(AnyTrue),
        84 => Instr::V128LoadLane(SimdMemLaneOp::Load8, read_memarg(r)?, simd_lane(r.byte()?, 16)?),
        85 => Instr::V128LoadLane(SimdMemLaneOp::Load16, read_memarg(r)?, simd_lane(r.byte()?, 8)?),
        86 => Instr::V128LoadLane(SimdMemLaneOp::Load32, read_memarg(r)?, simd_lane(r.byte()?, 4)?),
        87 => Instr::V128LoadLane(SimdMemLaneOp::Load64, read_memarg(r)?, simd_lane(r.byte()?, 2)?),
        88 => Instr::V128LoadLane(SimdMemLaneOp::Store8, read_memarg(r)?, simd_lane(r.byte()?, 16)?),
        89 => Instr::V128LoadLane(SimdMemLaneOp::Store16, read_memarg(r)?, simd_lane(r.byte()?, 8)?),
        90 => Instr::V128LoadLane(SimdMemLaneOp::Store32, read_memarg(r)?, simd_lane(r.byte()?, 4)?),
        91 => Instr::V128LoadLane(SimdMemLaneOp::Store64, read_memarg(r)?, simd_lane(r.byte()?, 2)?),
        92 => Instr::V128LoadExt(SimdLoadOp::Load32Zero, read_memarg(r)?),
        93 => Instr::V128LoadExt(SimdLoadOp::Load64Zero, read_memarg(r)?),
        94 => Instr::Simd(F32x4DemoteF64x2Zero),
        95 => Instr::Simd(F64x2PromoteLowF32x4),
        96 => Instr::Simd(I8x16Abs),
        97 => Instr::Simd(I8x16Neg),
        98 => Instr::Simd(I8x16Popcnt),
        99 => Instr::Simd(I8x16AllTrue),
        100 => Instr::Simd(I8x16Bitmask),
        101 => Instr::Simd(I8x16NarrowI16x8S),
        102 => Instr::Simd(I8x16NarrowI16x8U),
        103 => Instr::Simd(F32x4Ceil),
        104 => Instr::Simd(F32x4Floor),
        105 => Instr::Simd(F32x4Trunc),
        106 => Instr::Simd(F32x4Nearest),
        107 => Instr::Simd(I8x16Shl),
        108 => Instr::Simd(I8x16ShrS),
        109 => Instr::Simd(I8x16ShrU),
        110 => Instr::Simd(I8x16Add),
        111 => Instr::Simd(I8x16AddSatS),
        112 => Instr::Simd(I8x16AddSatU),
        113 => Instr::Simd(I8x16Sub),
        114 => Instr::Simd(I8x16SubSatS),
        115 => Instr::Simd(I8x16SubSatU),
        116 => Instr::Simd(F64x2Ceil),
        117 => Instr::Simd(F64x2Floor),
        118 => Instr::Simd(I8x16MinS),
        119 => Instr::Simd(I8x16MinU),
        120 => Instr::Simd(I8x16MaxS),
        121 => Instr::Simd(I8x16MaxU),
        122 => Instr::Simd(F64x2Trunc),
        123 => Instr::Simd(I8x16AvgrU),
        124 => Instr::Simd(I16x8ExtaddPairwiseI8x16S),
        125 => Instr::Simd(I16x8ExtaddPairwiseI8x16U),
        126 => Instr::Simd(I32x4ExtaddPairwiseI16x8S),
        127 => Instr::Simd(I32x4ExtaddPairwiseI16x8U),
        128 => Instr::Simd(I16x8Abs),
        129 => Instr::Simd(I16x8Neg),
        130 => Instr::Simd(I16x8Q15mulrSatS),
        131 => Instr::Simd(I16x8AllTrue),
        132 => Instr::Simd(I16x8Bitmask),
        133 => Instr::Simd(I16x8NarrowI32x4S),
        134 => Instr::Simd(I16x8NarrowI32x4U),
        135 => Instr::Simd(I16x8ExtendLowI8x16S),
        136 => Instr::Simd(I16x8ExtendHighI8x16S),
        137 => Instr::Simd(I16x8ExtendLowI8x16U),
        138 => Instr::Simd(I16x8ExtendHighI8x16U),
        139 => Instr::Simd(I16x8Shl),
        140 => Instr::Simd(I16x8ShrS),
        141 => Instr::Simd(I16x8ShrU),
        142 => Instr::Simd(I16x8Add),
        143 => Instr::Simd(I16x8AddSatS),
        144 => Instr::Simd(I16x8AddSatU),
        145 => Instr::Simd(I16x8Sub),
        146 => Instr::Simd(I16x8SubSatS),
        147 => Instr::Simd(I16x8SubSatU),
        148 => Instr::Simd(F64x2Nearest),
        149 => Instr::Simd(I16x8Mul),
        150 => Instr::Simd(I16x8MinS),
        151 => Instr::Simd(I16x8MinU),
        152 => Instr::Simd(I16x8MaxS),
        153 => Instr::Simd(I16x8MaxU),
        155 => Instr::Simd(I16x8AvgrU),
        156 => Instr::Simd(I16x8ExtmulLowI8x16S),
        157 => Instr::Simd(I16x8ExtmulHighI8x16S),
        158 => Instr::Simd(I16x8ExtmulLowI8x16U),
        159 => Instr::Simd(I16x8ExtmulHighI8x16U),
        160 => Instr::Simd(I32x4Abs),
        161 => Instr::Simd(I32x4Neg),
        163 => Instr::Simd(I32x4AllTrue),
        164 => Instr::Simd(I32x4Bitmask),
        167 => Instr::Simd(I32x4ExtendLowI16x8S),
        168 => Instr::Simd(I32x4ExtendHighI16x8S),
        169 => Instr::Simd(I32x4ExtendLowI16x8U),
        170 => Instr::Simd(I32x4ExtendHighI16x8U),
        171 => Instr::Simd(I32x4Shl),
        172 => Instr::Simd(I32x4ShrS),
        173 => Instr::Simd(I32x4ShrU),
        174 => Instr::Simd(I32x4Add),
        177 => Instr::Simd(I32x4Sub),
        181 => Instr::Simd(I32x4Mul),
        182 => Instr::Simd(I32x4MinS),
        183 => Instr::Simd(I32x4MinU),
        184 => Instr::Simd(I32x4MaxS),
        185 => Instr::Simd(I32x4MaxU),
        186 => Instr::Simd(I32x4DotI16x8S),
        188 => Instr::Simd(I32x4ExtmulLowI16x8S),
        189 => Instr::Simd(I32x4ExtmulHighI16x8S),
        190 => Instr::Simd(I32x4ExtmulLowI16x8U),
        191 => Instr::Simd(I32x4ExtmulHighI16x8U),
        192 => Instr::Simd(I64x2Abs),
        193 => Instr::Simd(I64x2Neg),
        195 => Instr::Simd(I64x2AllTrue),
        196 => Instr::Simd(I64x2Bitmask),
        199 => Instr::Simd(I64x2ExtendLowI32x4S),
        200 => Instr::Simd(I64x2ExtendHighI32x4S),
        201 => Instr::Simd(I64x2ExtendLowI32x4U),
        202 => Instr::Simd(I64x2ExtendHighI32x4U),
        203 => Instr::Simd(I64x2Shl),
        204 => Instr::Simd(I64x2ShrS),
        205 => Instr::Simd(I64x2ShrU),
        206 => Instr::Simd(I64x2Add),
        209 => Instr::Simd(I64x2Sub),
        213 => Instr::Simd(I64x2Mul),
        214 => Instr::Simd(I64x2Eq),
        215 => Instr::Simd(I64x2Ne),
        216 => Instr::Simd(I64x2LtS),
        217 => Instr::Simd(I64x2GtS),
        218 => Instr::Simd(I64x2LeS),
        219 => Instr::Simd(I64x2GeS),
        220 => Instr::Simd(I64x2ExtmulLowI32x4S),
        221 => Instr::Simd(I64x2ExtmulHighI32x4S),
        222 => Instr::Simd(I64x2ExtmulLowI32x4U),
        223 => Instr::Simd(I64x2ExtmulHighI32x4U),
        224 => Instr::Simd(F32x4Abs),
        225 => Instr::Simd(F32x4Neg),
        227 => Instr::Simd(F32x4Sqrt),
        228 => Instr::Simd(F32x4Add),
        229 => Instr::Simd(F32x4Sub),
        230 => Instr::Simd(F32x4Mul),
        231 => Instr::Simd(F32x4Div),
        232 => Instr::Simd(F32x4Min),
        233 => Instr::Simd(F32x4Max),
        234 => Instr::Simd(F32x4Pmin),
        235 => Instr::Simd(F32x4Pmax),
        236 => Instr::Simd(F64x2Abs),
        237 => Instr::Simd(F64x2Neg),
        239 => Instr::Simd(F64x2Sqrt),
        240 => Instr::Simd(F64x2Add),
        241 => Instr::Simd(F64x2Sub),
        242 => Instr::Simd(F64x2Mul),
        243 => Instr::Simd(F64x2Div),
        244 => Instr::Simd(F64x2Min),
        245 => Instr::Simd(F64x2Max),
        246 => Instr::Simd(F64x2Pmin),
        247 => Instr::Simd(F64x2Pmax),
        248 => Instr::Simd(I32x4TruncSatF32x4S),
        249 => Instr::Simd(I32x4TruncSatF32x4U),
        250 => Instr::Simd(F32x4ConvertI32x4S),
        251 => Instr::Simd(F32x4ConvertI32x4U),
        252 => Instr::Simd(I32x4TruncSatF64x2SZero),
        253 => Instr::Simd(I32x4TruncSatF64x2UZero),
        254 => Instr::Simd(F64x2ConvertLowI32x4S),
        255 => Instr::Simd(F64x2ConvertLowI32x4U),
        _ => {
            return Err(format!(
                "unsupported SIMD instruction {} (relaxed SIMD etc. are out of scope)",
                sub
            ))
        }
    };
    Ok(ins)
}

use crate::wasm::{SimdImmediate, ValType};

pub fn signature(op: u32) -> Option<(Vec<ValType>, Option<ValType>)> {
    use ValType::{F32, F64, I32, I64, V128};
    let unary = |input, output| Some((vec![input], Some(output)));
    let binary = || Some((vec![V128, V128], Some(V128)));
    match op {
        0x0c => Some((vec![], Some(V128))),
        0x0d | 0x0e => binary(),
        0x0f..=0x11 => unary(I32, V128),
        0x12 => unary(I64, V128),
        0x13 => unary(F32, V128),
        0x14 => unary(F64, V128),
        0x15 | 0x16 | 0x18 | 0x19 | 0x1b => unary(V128, I32),
        0x17 | 0x1a | 0x1c => Some((vec![V128, I32], Some(V128))),
        0x1d => unary(V128, I64),
        0x1e => Some((vec![V128, I64], Some(V128))),
        0x1f => unary(V128, F32),
        0x20 => Some((vec![V128, F32], Some(V128))),
        0x21 => unary(V128, F64),
        0x22 => Some((vec![V128, F64], Some(V128))),
        0x23..=0x4c => binary(),
        0x4d => unary(V128, V128),
        0x4e..=0x51 => binary(),
        0x52 => Some((vec![V128, V128, V128], Some(V128))),
        0x53 => unary(V128, I32),
        0x5e | 0x5f => unary(V128, V128),
        0x60..=0x62 => unary(V128, V128),
        0x63 | 0x64 => unary(V128, I32),
        0x65 | 0x66 => binary(),
        0x67..=0x6a => unary(V128, V128),
        0x6b..=0x6d => Some((vec![V128, I32], Some(V128))),
        0x6e..=0x7b => binary_or_unary(op),
        0x7c..=0x7f => unary(V128, V128),
        0x80 | 0x81 => unary(V128, V128),
        0x82 => binary(),
        0x83 | 0x84 => unary(V128, I32),
        0x85..=0x86 => binary(),
        0x87..=0x8a => unary(V128, V128),
        0x8b..=0x8d => Some((vec![V128, I32], Some(V128))),
        0x8e..=0x99 => binary_or_unary(op),
        0x9b..=0x9f => binary(),
        0xa0 | 0xa1 | 0xa3 | 0xa4 => unary(V128, if matches!(op, 0xa3 | 0xa4) { I32 } else { V128 }),
        0xa7..=0xaa => unary(V128, V128),
        0xab..=0xad => Some((vec![V128, I32], Some(V128))),
        0xae | 0xb1 | 0xb5..=0xba | 0xbc..=0xbf => binary(),
        0xc0..=0xc1 => unary(V128, V128),
        0xc3..=0xc4 => unary(V128, I32),
        0xc7..=0xca => unary(V128, V128),
        0xcb..=0xcd => Some((vec![V128, I32], Some(V128))),
        0xce | 0xd1 | 0xd5..=0xdf => binary(),
        0xe0 | 0xe1 | 0xe3 | 0xec | 0xed | 0xef => unary(V128, V128),
        0xe4..=0xeb | 0xf0..=0xf7 => binary(),
        0xf8..=0xff => unary(V128, V128),
        _ => None,
    }
}

fn binary_or_unary(op: u32) -> Option<(Vec<ValType>, Option<ValType>)> {
    use ValType::V128;
    match op {
        0x94 | 0x7a => Some((vec![V128], Some(V128))),
        0x8e..=0x93 | 0x95..=0x99 | 0x9b..=0x9f | 0x6e..=0x79 | 0x7b => {
            Some((vec![V128, V128], Some(V128)))
        }
        _ => None,
    }
}

pub fn memory_access(op: u32, immediate: &SimdImmediate) -> Option<(u32, u32)> {
    let (align, width) = match (op, immediate) {
        (0x00, SimdImmediate::MemArg { align, .. }) => (*align, 16),
        (0x01 | 0x02 | 0x03 | 0x04 | 0x05 | 0x06, SimdImmediate::MemArg { align, .. }) => (*align, 8),
        (0x07, SimdImmediate::MemArg { align, .. }) => (*align, 1),
        (0x08, SimdImmediate::MemArg { align, .. }) => (*align, 2),
        (0x09, SimdImmediate::MemArg { align, .. }) => (*align, 4),
        (0x0a, SimdImmediate::MemArg { align, .. }) => (*align, 8),
        (0x0b, SimdImmediate::MemArg { align, .. }) => (*align, 16),
        (0x54 | 0x58, SimdImmediate::MemLane { align, .. }) => (*align, 1),
        (0x55 | 0x59, SimdImmediate::MemLane { align, .. }) => (*align, 2),
        (0x56 | 0x5a, SimdImmediate::MemLane { align, .. }) => (*align, 4),
        (0x57 | 0x5b, SimdImmediate::MemLane { align, .. }) => (*align, 8),
        (0x5c, SimdImmediate::MemArg { align, .. }) => (*align, 4),
        (0x5d, SimdImmediate::MemArg { align, .. }) => (*align, 8),
        _ => return None,
    };
    Some((align, width))
}

pub fn lane_count(op: u32) -> u8 {
    match op {
        0x15..=0x17 | 0x54 | 0x58 => 16,
        0x18..=0x1a | 0x55 | 0x59 => 8,
        0x1b..=0x1c | 0x1f..=0x20 | 0x56 | 0x5a => 4,
        0x1d..=0x1e | 0x21..=0x22 | 0x57 | 0x5b => 2,
        _ => 0,
    }
}

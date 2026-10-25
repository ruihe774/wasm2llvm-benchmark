// Binary reader with the WebAssembly LEB128 rules.

pub struct Reader<'a> {
    pub buf: &'a [u8],
    pub pos: usize,
}

pub type RResult<T> = Result<T, String>;

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }

    pub fn eof(&self) -> bool {
        self.pos >= self.buf.len()
    }

    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    pub fn peek(&self) -> RResult<u8> {
        self.buf
            .get(self.pos)
            .copied()
            .ok_or_else(|| "unexpected end of input".to_string())
    }

    pub fn byte(&mut self) -> RResult<u8> {
        let b = self.peek()?;
        self.pos += 1;
        Ok(b)
    }

    pub fn bytes(&mut self, n: usize) -> RResult<&'a [u8]> {
        if self.remaining() < n {
            return Err("unexpected end of input".to_string());
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    /// Unsigned LEB128 of at most `bits` bits.
    pub fn uleb(&mut self, bits: u32) -> RResult<u64> {
        let max_bytes = ((bits + 6) / 7) as usize;
        let mut result: u128 = 0;
        let mut shift: u32 = 0;
        for i in 0..max_bytes {
            let b = self.byte()?;
            result |= ((b & 0x7f) as u128) << shift;
            shift += 7;
            if b & 0x80 == 0 {
                if result >> bits != 0 {
                    return Err(format!("integer too large for u{}", bits));
                }
                return Ok(result as u64);
            }
            if i + 1 == max_bytes {
                return Err(format!("integer representation too long for u{}", bits));
            }
        }
        unreachable!()
    }

    /// Signed LEB128 of at most `bits` bits. Returns the value sign-extended
    /// from `bits`.
    pub fn sleb(&mut self, bits: u32) -> RResult<i64> {
        let max_bytes = ((bits + 6) / 7) as usize;
        let mut result: i128 = 0;
        let mut shift: u32 = 0;
        for i in 0..max_bytes {
            let b = self.byte()?;
            result |= ((b & 0x7f) as i128) << shift;
            shift += 7;
            if b & 0x80 == 0 {
                // Sign extend from shift bits.
                if shift < 128 && (b & 0x40) != 0 {
                    result |= !0i128 << shift;
                }
                let min = -(1i128 << (bits - 1));
                let max = (1i128 << (bits - 1)) - 1;
                if result < min || result > max {
                    return Err(format!("integer too large for s{}", bits));
                }
                return Ok(result as i64);
            }
            if i + 1 == max_bytes {
                return Err(format!("integer representation too long for s{}", bits));
            }
        }
        unreachable!()
    }

    pub fn u32(&mut self) -> RResult<u32> {
        Ok(self.uleb(32)? as u32)
    }

    pub fn s32(&mut self) -> RResult<i32> {
        Ok(self.sleb(32)? as i32)
    }

    pub fn s33(&mut self) -> RResult<i64> {
        self.sleb(33)
    }

    pub fn s64(&mut self) -> RResult<i64> {
        self.sleb(64)
    }

    pub fn f32bits(&mut self) -> RResult<u32> {
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn f64bits(&mut self) -> RResult<u64> {
        let b = self.bytes(8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    /// A wasm name: byte vector that must be valid UTF-8.
    pub fn name(&mut self) -> RResult<Vec<u8>> {
        let n = self.u32()? as usize;
        // Guard against absurd lengths before allocating.
        if n > self.remaining() {
            return Err("name length exceeds input".to_string());
        }
        let b = self.bytes(n)?.to_vec();
        std::str::from_utf8(&b).map_err(|_| "malformed UTF-8 encoding".to_string())?;
        Ok(b)
    }
}

pub fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xf) as usize] as char);
    }
    s
}

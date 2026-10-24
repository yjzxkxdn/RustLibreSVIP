//! Little-endian primitive reader/writer for the `VocalinaStudioProjectFile`
//! construct model (`experimental/vsp/model.py`).
//!
//! Format idioms handled here:
//! * `Utf16LeString` — `PascalString(Int16ul, "utf-16-le")`: a u16 **byte**
//!   length followed by UTF-16LE bytes;
//! * `VocalinaSectionSize` — `ExprAdapter(Int32ul, encoder=+8, decoder=-8)`:
//!   the stored u32 equals the payload length **plus 8**;
//! * `SubSize` — the same idea with a u16 and an offset of 2;
//! * `PrefixedArray(Int16ul, ...)` — a u16 element count before the elements.

use libresvip_core::error::{LibreSvipError, Result};

pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn u8(&mut self) -> Result<u8> {
        self.bytes(1).map(|bytes| bytes[0])
    }

    pub fn u16(&mut self) -> Result<u16> {
        self.bytes(2).map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    pub fn i16(&mut self) -> Result<i16> {
        self.bytes(2).map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]))
    }

    pub fn u32(&mut self) -> Result<u32> {
        self.bytes(4)
            .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn f32(&mut self) -> Result<f32> {
        self.bytes(4)
            .map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn bytes(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(count)
            .filter(|&end| end <= self.data.len())
            .ok_or_else(|| {
                LibreSvipError::msg(format!(
                    "unexpected end of vsp data: need {count} bytes at offset {}",
                    self.pos
                ))
            })?;
        let slice = &self.data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    /// `Const(...)` field validation.
    pub fn expect_bytes(&mut self, expected: &[u8]) -> Result<()> {
        let actual = self.bytes(expected.len())?;
        if actual != expected {
            return Err(LibreSvipError::msg(format!(
                "vsp magic mismatch: expected {expected:?}, found {actual:?}"
            )));
        }
        Ok(())
    }

    pub fn expect_u16(&mut self, expected: u16) -> Result<()> {
        let actual = self.u16()?;
        if actual != expected {
            return Err(LibreSvipError::msg(format!(
                "vsp field mismatch: expected {expected}, found {actual}"
            )));
        }
        Ok(())
    }

    pub fn expect_u8(&mut self, expected: u8) -> Result<()> {
        let actual = self.u8()?;
        if actual != expected {
            return Err(LibreSvipError::msg(format!(
                "vsp field mismatch: expected {expected}, found {actual}"
            )));
        }
        Ok(())
    }

    /// `Utf16LeString`.
    pub fn utf16_string(&mut self) -> Result<String> {
        let byte_len = self.u16()? as usize;
        let bytes = self.bytes(byte_len)?;
        if bytes.len() % 2 != 0 {
            return Err(LibreSvipError::msg(
                "odd-length utf-16-le string in vsp data",
            ));
        }
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        String::from_utf16(&units)
            .map_err(|error| LibreSvipError::msg(format!("invalid utf-16-le string: {error}")))
    }

    /// `Prefixed(VocalinaSectionSize, ...)`: u32 length minus 8.
    pub fn section(&mut self) -> Result<Reader<'a>> {
        let stored = self.u32()? as usize;
        let len = stored
            .checked_sub(8)
            .ok_or_else(|| LibreSvipError::msg("negative vsp section size"))?;
        let bytes = self.bytes(len)?;
        Ok(Reader::new(bytes))
    }

    /// `Prefixed(SubSize, ...)`: u16 length minus 2.
    pub fn subsection(&mut self) -> Result<Reader<'a>> {
        let stored = self.u16()? as usize;
        let len = stored
            .checked_sub(2)
            .ok_or_else(|| LibreSvipError::msg("negative vsp sub-section size"))?;
        let bytes = self.bytes(len)?;
        Ok(Reader::new(bytes))
    }

    /// `PrefixedArray(Int16ul, ...)` element count.
    pub fn count(&mut self) -> Result<usize> {
        self.u16().map(|value| value as usize)
    }
}

pub struct Writer {
    buf: Vec<u8>,
}

impl Default for Writer {
    fn default() -> Self {
        Self::new()
    }
}

impl Writer {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    pub fn u8(&mut self, value: u8) {
        self.buf.push(value);
    }

    pub fn u16(&mut self, value: u16) {
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    pub fn i16(&mut self, value: i16) {
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    pub fn u32(&mut self, value: u32) {
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    pub fn f32(&mut self, value: f32) {
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    pub fn bytes(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    pub fn utf16_string(&mut self, value: &str) {
        let units: Vec<u16> = value.encode_utf16().collect();
        self.u16((units.len() * 2) as u16);
        for unit in units {
            self.u16(unit);
        }
    }

    /// Emits a `Prefixed(VocalinaSectionSize, payload)` field from an
    /// already-built payload.
    pub fn section(&mut self, payload: &[u8]) {
        self.u32(payload.len() as u32 + 8);
        self.bytes(payload);
    }

    /// Emits a `Prefixed(SubSize, payload)` field.
    pub fn subsection(&mut self, payload: &[u8]) {
        self.u16(payload.len() as u16 + 2);
        self.bytes(payload);
    }

    pub fn count(&mut self, count: usize) {
        self.u16(count as u16);
    }

    pub fn into_vec(self) -> Vec<u8> {
        self.buf
    }
}

/// `packed_to_linear` — expands the `{low, mid, high}` bar/beat/tick triple.
pub fn raw_packed_to_linear(raw: u32, ticks_per_beat: i64, beats_per_bar: i64) -> i64 {
    let low = i64::from(raw & 0xFFF);
    let mid = i64::from((raw >> 12) & 0xF);
    let high = i64::from((raw >> 16) & 0xFFFF);
    high * ticks_per_beat * beats_per_bar + mid * ticks_per_beat + low
}

/// `_linear_to_packed_raw`. Python's `//` and `%` floor toward negative
/// infinity, hence the euclidean operations.
pub fn linear_to_packed_raw(pos: i64, ticks_per_beat: i64, beats_per_bar: i64) -> u32 {
    let ticks_per_bar = ticks_per_beat * beats_per_bar;
    let bar = pos.div_euclid(ticks_per_bar);
    let remainder = pos.rem_euclid(ticks_per_bar);
    let beat = remainder.div_euclid(ticks_per_beat);
    let tick = remainder.rem_euclid(ticks_per_beat);
    (tick | (beat << 12) | (bar << 16)) as u32
}

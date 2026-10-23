//! Hand-written protobuf codec for `svip3.proto`.
//!
//! Python uses the generated `libresvip.plugins.svip3.svip3_pb` module. The
//! schema is small and stable, so the message types and their wire encoding are
//! implemented directly here instead of pulling in `prost-build` (the crate
//! list does not include it).
//!
//! Encoding follows proto3 rules exactly like the Python runtime: fields are
//! emitted in ascending field-number order and scalar fields holding their
//! default value are omitted. Repeated message fields emit one length-delimited
//! record per element (even when the element encodes to an empty body).

use libresvip_core::error::{LibreSvipError, Result};

// ---------------------------------------------------------------------------
// Wire helpers
// ---------------------------------------------------------------------------

fn invalid(message: impl Into<String>) -> LibreSvipError {
    LibreSvipError::Parse(message.into())
}

pub struct ProtoReader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> ProtoReader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn is_eof(&self) -> bool {
        self.pos >= self.buf.len()
    }

    pub fn varint(&mut self) -> Result<u64> {
        let mut result = 0u64;
        let mut shift = 0u32;
        loop {
            let byte = *self
                .buf
                .get(self.pos)
                .ok_or_else(|| invalid("unexpected end of protobuf message"))?;
            self.pos += 1;
            result |= u64::from(byte & 0x7F) << shift;
            if byte & 0x80 == 0 {
                return Ok(result);
            }
            shift += 7;
            if shift >= 64 {
                return Err(invalid("protobuf varint is too long"));
            }
        }
    }

    pub fn key(&mut self) -> Result<(u32, u8)> {
        let key = self.varint()?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        Ok((field, wire))
    }

    pub fn bytes(&mut self) -> Result<&'a [u8]> {
        let length = self.varint()? as usize;
        let end = self
            .pos
            .checked_add(length)
            .ok_or_else(|| invalid("protobuf length overflow"))?;
        let slice = self
            .buf
            .get(self.pos..end)
            .ok_or_else(|| invalid("unexpected end of protobuf message"))?;
        self.pos = end;
        Ok(slice)
    }

    pub fn float(&mut self) -> Result<f32> {
        let raw = self.bytes_fixed::<4>()?;
        Ok(f32::from_le_bytes(raw))
    }

    fn bytes_fixed<const N: usize>(&mut self) -> Result<[u8; N]> {
        let end = self
            .pos
            .checked_add(N)
            .ok_or_else(|| invalid("protobuf length overflow"))?;
        let slice = self
            .buf
            .get(self.pos..end)
            .ok_or_else(|| invalid("unexpected end of protobuf message"))?;
        self.pos = end;
        let mut out = [0u8; N];
        out.copy_from_slice(slice);
        Ok(out)
    }

    /// Skip an unknown field, keeping the reader in sync.
    pub fn skip(&mut self, wire: u8) -> Result<()> {
        match wire {
            0 => {
                self.varint()?;
            }
            1 => {
                self.bytes_fixed::<8>()?;
            }
            2 => {
                self.bytes()?;
            }
            5 => {
                self.bytes_fixed::<4>()?;
            }
            other => {
                return Err(invalid(format!("unsupported protobuf wire type {other}")));
            }
        }
        Ok(())
    }
}

fn write_varint(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn write_key(out: &mut Vec<u8>, field: u32, wire: u8) {
    write_varint(out, (u64::from(field) << 3) | u64::from(wire));
}

pub fn write_int(out: &mut Vec<u8>, field: u32, value: i64) {
    if value == 0 {
        return;
    }
    write_key(out, field, 0);
    write_varint(out, value as u64);
}

pub fn write_enum(out: &mut Vec<u8>, field: u32, value: i32) {
    write_int(out, field, i64::from(value));
}

fn zigzag(value: i32) -> u64 {
    ((value << 1) ^ (value >> 31)) as u32 as u64
}

pub fn write_sint(out: &mut Vec<u8>, field: u32, value: i32) {
    if value == 0 {
        return;
    }
    write_key(out, field, 0);
    write_varint(out, zigzag(value));
}

pub fn write_bool(out: &mut Vec<u8>, field: u32, value: bool) {
    if !value {
        return;
    }
    write_key(out, field, 0);
    write_varint(out, 1);
}

pub fn write_float(out: &mut Vec<u8>, field: u32, value: f32) {
    if value == 0.0 {
        return;
    }
    write_key(out, field, 5);
    out.extend_from_slice(&value.to_le_bytes());
}

pub fn write_str(out: &mut Vec<u8>, field: u32, value: &str) {
    if value.is_empty() {
        return;
    }
    write_key(out, field, 2);
    write_varint(out, value.len() as u64);
    out.extend_from_slice(value.as_bytes());
}

pub fn write_bytes(out: &mut Vec<u8>, field: u32, value: &[u8]) {
    if value.is_empty() {
        return;
    }
    write_key(out, field, 2);
    write_varint(out, value.len() as u64);
    out.extend_from_slice(value);
}

/// Writes a length-delimited message field unconditionally (used for repeated
/// fields, whose elements are always present).
pub fn write_message(out: &mut Vec<u8>, field: u32, body: &[u8]) {
    write_key(out, field, 2);
    write_varint(out, body.len() as u64);
    out.extend_from_slice(body);
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct Svip3SongTempo {
    pub pos: i32,
    pub tempo: i32,
}

impl Svip3SongTempo {
    fn decode(buf: &[u8]) -> Result<Self> {
        let mut message = Self::default();
        let mut reader = ProtoReader::new(buf);
        while !reader.is_eof() {
            let (field, wire) = reader.key()?;
            match (field, wire) {
                (1, 0) => message.pos = reader.varint()? as i32,
                (2, 0) => message.tempo = reader.varint()? as i32,
                _ => reader.skip(wire)?,
            }
        }
        Ok(message)
    }

    fn encode_into(&self, out: &mut Vec<u8>) {
        write_int(out, 1, i64::from(self.pos));
        write_int(out, 2, i64::from(self.tempo));
    }
}

#[derive(Clone, Debug, Default)]
pub struct Svip3BeatSize {
    pub numerator: i32,
    pub denominator: i32,
}

impl Svip3BeatSize {
    fn decode(buf: &[u8]) -> Result<Self> {
        let mut message = Self::default();
        let mut reader = ProtoReader::new(buf);
        while !reader.is_eof() {
            let (field, wire) = reader.key()?;
            match (field, wire) {
                (1, 0) => message.numerator = reader.varint()? as i32,
                (2, 0) => message.denominator = reader.varint()? as i32,
                _ => reader.skip(wire)?,
            }
        }
        Ok(message)
    }

    fn encode_into(&self, out: &mut Vec<u8>) {
        write_int(out, 1, i64::from(self.numerator));
        write_int(out, 2, i64::from(self.denominator));
    }
}

#[derive(Clone, Debug, Default)]
pub struct Svip3SongBeat {
    pub pos: i32,
    pub beat_size: Option<Svip3BeatSize>,
}

impl Svip3SongBeat {
    fn decode(buf: &[u8]) -> Result<Self> {
        let mut message = Self::default();
        let mut reader = ProtoReader::new(buf);
        while !reader.is_eof() {
            let (field, wire) = reader.key()?;
            match (field, wire) {
                (1, 0) => message.pos = reader.varint()? as i32,
                (2, 2) => message.beat_size = Some(Svip3BeatSize::decode(reader.bytes()?)?),
                _ => reader.skip(wire)?,
            }
        }
        Ok(message)
    }

    fn encode_into(&self, out: &mut Vec<u8>) {
        write_int(out, 1, i64::from(self.pos));
        if let Some(beat_size) = &self.beat_size {
            let mut body = Vec::new();
            beat_size.encode_into(&mut body);
            write_message(out, 2, &body);
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Svip3LineParamNode {
    pub pos: i32,
    pub value: f32,
}

impl Svip3LineParamNode {
    fn decode(buf: &[u8]) -> Result<Self> {
        let mut message = Self::default();
        let mut reader = ProtoReader::new(buf);
        while !reader.is_eof() {
            let (field, wire) = reader.key()?;
            match (field, wire) {
                (1, 0) => {
                    let raw = reader.varint()? as u32;
                    message.pos = ((raw >> 1) as i32) ^ -((raw & 1) as i32);
                }
                (2, 5) => message.value = reader.float()?,
                _ => reader.skip(wire)?,
            }
        }
        Ok(message)
    }

    fn encode_into(&self, out: &mut Vec<u8>) {
        write_sint(out, 1, self.pos);
        write_float(out, 2, self.value);
    }
}

#[derive(Clone, Debug, Default)]
pub struct Svip3Note {
    pub start_pos: i32,
    pub width_pos: i32,
    pub key_index: i32,
    pub lyric: String,
    pub pronouncing: String,
    pub consonant_len: i32,
    pub has_consonant: bool,
    pub sp_len: i32,
    pub sil_len: i32,
    pub length_validate_tag: i32,
}

impl Svip3Note {
    fn decode(buf: &[u8]) -> Result<Self> {
        let mut message = Self::default();
        let mut reader = ProtoReader::new(buf);
        while !reader.is_eof() {
            let (field, wire) = reader.key()?;
            match (field, wire) {
                (1, 0) => message.start_pos = reader.varint()? as i32,
                (2, 0) => message.width_pos = reader.varint()? as i32,
                (3, 0) => message.key_index = reader.varint()? as i32,
                (4, 2) => {
                    message.lyric =
                        String::from_utf8_lossy(reader.bytes()?).into_owned();
                }
                (5, 2) => {
                    message.pronouncing =
                        String::from_utf8_lossy(reader.bytes()?).into_owned();
                }
                (6, 0) => message.consonant_len = reader.varint()? as i32,
                (7, 0) => message.has_consonant = reader.varint()? != 0,
                (9, 0) => message.sp_len = reader.varint()? as i32,
                (10, 0) => message.sil_len = reader.varint()? as i32,
                (11, 0) => message.length_validate_tag = reader.varint()? as i32,
                _ => reader.skip(wire)?,
            }
        }
        Ok(message)
    }

    fn encode_into(&self, out: &mut Vec<u8>) {
        write_int(out, 1, i64::from(self.start_pos));
        write_int(out, 2, i64::from(self.width_pos));
        write_int(out, 3, i64::from(self.key_index));
        write_str(out, 4, &self.lyric);
        write_str(out, 5, &self.pronouncing);
        write_int(out, 6, i64::from(self.consonant_len));
        write_bool(out, 7, self.has_consonant);
        write_int(out, 9, i64::from(self.sp_len));
        write_int(out, 10, i64::from(self.sil_len));
        write_enum(out, 11, self.length_validate_tag);
    }
}

#[derive(Clone, Debug, Default)]
pub struct Svip3SingingPattern {
    pub name: String,
    pub real_pos: i32,
    pub real_dur: i32,
    pub play_pos: i32,
    pub play_dur: i32,
    pub is_mute: bool,
    pub note_list: Vec<Svip3Note>,
    pub edited_pitch_line: Vec<Svip3LineParamNode>,
}

impl Svip3SingingPattern {
    /// `pos` — `real_pos + play_pos` (added as a Python property in `model.py`).
    pub fn pos(&self) -> i32 {
        self.real_pos.wrapping_add(self.play_pos)
    }

    fn decode(buf: &[u8]) -> Result<Self> {
        let mut message = Self::default();
        let mut reader = ProtoReader::new(buf);
        while !reader.is_eof() {
            let (field, wire) = reader.key()?;
            match (field, wire) {
                (1, 2) => {
                    message.name = String::from_utf8_lossy(reader.bytes()?).into_owned();
                }
                (3, 0) => message.real_pos = reader.varint()? as i32,
                (4, 0) => message.real_dur = reader.varint()? as i32,
                (5, 0) => message.play_pos = reader.varint()? as i32,
                (6, 0) => message.play_dur = reader.varint()? as i32,
                (7, 0) => message.is_mute = reader.varint()? != 0,
                (8, 2) => message.note_list.push(Svip3Note::decode(reader.bytes()?)?),
                (9, 2) => message
                    .edited_pitch_line
                    .push(Svip3LineParamNode::decode(reader.bytes()?)?),
                _ => reader.skip(wire)?,
            }
        }
        Ok(message)
    }

    fn encode_into(&self, out: &mut Vec<u8>) {
        // `type` is left at its proto3 default by the Python generator, so it is
        // not written on the wire.
        write_str(out, 1, &self.name);
        write_int(out, 3, i64::from(self.real_pos));
        write_int(out, 4, i64::from(self.real_dur));
        write_int(out, 5, i64::from(self.play_pos));
        write_int(out, 6, i64::from(self.play_dur));
        write_bool(out, 7, self.is_mute);
        for note in &self.note_list {
            let mut body = Vec::new();
            note.encode_into(&mut body);
            write_message(out, 8, &body);
        }
        for node in &self.edited_pitch_line {
            let mut body = Vec::new();
            node.encode_into(&mut body);
            write_message(out, 9, &body);
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Svip3SingingTrack {
    pub volume: f32,
    pub pan: f32,
    pub mute: bool,
    pub name: String,
    pub solo: bool,
    pub color: String,
    pub pattern_list: Vec<Svip3SingingPattern>,
    pub ai_singer_id: String,
}

impl Svip3SingingTrack {
    pub fn decode(buf: &[u8]) -> Result<Self> {
        let mut message = Self::default();
        let mut reader = ProtoReader::new(buf);
        while !reader.is_eof() {
            let (field, wire) = reader.key()?;
            match (field, wire) {
                (1, 5) => message.volume = reader.float()?,
                (2, 5) => message.pan = reader.float()?,
                (3, 0) => message.mute = reader.varint()? != 0,
                (4, 2) => {
                    message.name = String::from_utf8_lossy(reader.bytes()?).into_owned();
                }
                (5, 0) => message.solo = reader.varint()? != 0,
                (6, 2) => {
                    message.color = String::from_utf8_lossy(reader.bytes()?).into_owned();
                }
                (8, 2) => message
                    .pattern_list
                    .push(Svip3SingingPattern::decode(reader.bytes()?)?),
                (9, 2) => {
                    message.ai_singer_id =
                        String::from_utf8_lossy(reader.bytes()?).into_owned();
                }
                _ => reader.skip(wire)?,
            }
        }
        Ok(message)
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        write_float(&mut out, 1, self.volume);
        write_float(&mut out, 2, self.pan);
        write_bool(&mut out, 3, self.mute);
        write_str(&mut out, 4, &self.name);
        write_bool(&mut out, 5, self.solo);
        write_str(&mut out, 6, &self.color);
        for pattern in &self.pattern_list {
            let mut body = Vec::new();
            pattern.encode_into(&mut body);
            write_message(&mut out, 8, &body);
        }
        write_str(&mut out, 9, &self.ai_singer_id);
        out
    }
}

#[derive(Clone, Debug, Default)]
pub struct Svip3AudioPattern {
    pub name: String,
    pub real_pos: i32,
    pub real_dur: i32,
    pub play_pos: i32,
    pub play_dur: i32,
    pub is_mute: bool,
    pub audio_file_path: String,
}

impl Svip3AudioPattern {
    fn decode(buf: &[u8]) -> Result<Self> {
        let mut message = Self::default();
        let mut reader = ProtoReader::new(buf);
        while !reader.is_eof() {
            let (field, wire) = reader.key()?;
            match (field, wire) {
                (1, 2) => {
                    message.name = String::from_utf8_lossy(reader.bytes()?).into_owned();
                }
                (3, 0) => message.real_pos = reader.varint()? as i32,
                (4, 0) => message.real_dur = reader.varint()? as i32,
                (5, 0) => message.play_pos = reader.varint()? as i32,
                (6, 0) => message.play_dur = reader.varint()? as i32,
                (7, 0) => message.is_mute = reader.varint()? != 0,
                (8, 2) => {
                    message.audio_file_path =
                        String::from_utf8_lossy(reader.bytes()?).into_owned();
                }
                _ => reader.skip(wire)?,
            }
        }
        Ok(message)
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        write_str(&mut out, 1, &self.name);
        write_int(&mut out, 3, i64::from(self.real_pos));
        write_int(&mut out, 4, i64::from(self.real_dur));
        write_int(&mut out, 5, i64::from(self.play_pos));
        write_int(&mut out, 6, i64::from(self.play_dur));
        write_bool(&mut out, 7, self.is_mute);
        write_str(&mut out, 8, &self.audio_file_path);
        out
    }
}

#[derive(Clone, Debug, Default)]
pub struct Svip3AudioTrack {
    pub volume: f32,
    pub pan: f32,
    pub mute: bool,
    pub name: String,
    pub solo: bool,
    pub color: String,
    pub pattern_list: Vec<Svip3AudioPattern>,
}

impl Svip3AudioTrack {
    pub fn decode(buf: &[u8]) -> Result<Self> {
        let mut message = Self::default();
        let mut reader = ProtoReader::new(buf);
        while !reader.is_eof() {
            let (field, wire) = reader.key()?;
            match (field, wire) {
                (1, 5) => message.volume = reader.float()?,
                (2, 5) => message.pan = reader.float()?,
                (3, 0) => message.mute = reader.varint()? != 0,
                (4, 2) => {
                    message.name = String::from_utf8_lossy(reader.bytes()?).into_owned();
                }
                (5, 0) => message.solo = reader.varint()? != 0,
                (6, 2) => {
                    message.color = String::from_utf8_lossy(reader.bytes()?).into_owned();
                }
                (8, 2) => message
                    .pattern_list
                    .push(Svip3AudioPattern::decode(reader.bytes()?)?),
                _ => reader.skip(wire)?,
            }
        }
        Ok(message)
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        write_float(&mut out, 1, self.volume);
        write_float(&mut out, 2, self.pan);
        write_bool(&mut out, 3, self.mute);
        write_str(&mut out, 4, &self.name);
        write_bool(&mut out, 5, self.solo);
        write_str(&mut out, 6, &self.color);
        for pattern in &self.pattern_list {
            write_message(&mut out, 8, &pattern.encode());
        }
        out
    }
}

#[derive(Clone, Debug, Default)]
pub struct ProtoAny {
    pub type_url: String,
    pub value: Vec<u8>,
}

impl ProtoAny {
    fn decode(buf: &[u8]) -> Result<Self> {
        let mut message = Self::default();
        let mut reader = ProtoReader::new(buf);
        while !reader.is_eof() {
            let (field, wire) = reader.key()?;
            match (field, wire) {
                (1, 2) => {
                    message.type_url = String::from_utf8_lossy(reader.bytes()?).into_owned();
                }
                (2, 2) => message.value = reader.bytes()?.to_vec(),
                _ => reader.skip(wire)?,
            }
        }
        Ok(message)
    }

    fn encode_into(&self, out: &mut Vec<u8>) {
        write_str(out, 1, &self.type_url);
        write_bytes(out, 2, &self.value);
    }
}

#[derive(Clone, Debug, Default)]
pub struct Svip3Project {
    pub duration: i32,
    pub tempo_list: Vec<Svip3SongTempo>,
    pub beat_list: Vec<Svip3SongBeat>,
    pub track_list: Vec<ProtoAny>,
}

impl Svip3Project {
    pub fn from_binary(buf: &[u8]) -> Result<Self> {
        let mut message = Self::default();
        let mut reader = ProtoReader::new(buf);
        while !reader.is_eof() {
            let (field, wire) = reader.key()?;
            match (field, wire) {
                (3, 0) => message.duration = reader.varint()? as i32,
                (4, 2) => message.tempo_list.push(Svip3SongTempo::decode(reader.bytes()?)?),
                (5, 2) => message.beat_list.push(Svip3SongBeat::decode(reader.bytes()?)?),
                (6, 2) => message.track_list.push(ProtoAny::decode(reader.bytes()?)?),
                _ => reader.skip(wire)?,
            }
        }
        Ok(message)
    }

    pub fn to_binary(&self) -> Vec<u8> {
        let mut out = Vec::new();
        write_int(&mut out, 3, i64::from(self.duration));
        for tempo in &self.tempo_list {
            let mut body = Vec::new();
            tempo.encode_into(&mut body);
            write_message(&mut out, 4, &body);
        }
        for beat in &self.beat_list {
            let mut body = Vec::new();
            beat.encode_into(&mut body);
            write_message(&mut out, 5, &body);
        }
        for track in &self.track_list {
            let mut body = Vec::new();
            track.encode_into(&mut body);
            write_message(&mut out, 6, &body);
        }
        out
    }
}

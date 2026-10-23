//! Binary (UMP clip) layout of the VX-beta project file - port of the
//! `construct` definitions in `libresvip.plugins.vxf.model`.

use libresvip_core::error::{LibreSvipError, Result};

const BASE_DCTPQ: i64 = 0x30;
const BASE_DELTA_TICKS: i64 = 0x40;
const BASE_START_PITCH: i64 = 0x4090;
const BASE_END_PITCH: i64 = 0x4080;

const CLIP_NAME_MAGIC: [u8; 2] = [0x01, 0x03];
const START_OF_CLIP: [u8; 4] = [0xf0, 0x20, 0x00, 0x00];
const END_OF_CLIP: [u8; 4] = [0xf0, 0x21, 0x00, 0x00];
const TEMPO_MAGIC: [u8; 4] = [0xd0, 0x10, 0x00, 0x00];
const TIME_SIGNATURE_MAGIC: [u8; 4] = [0xd0, 0x10, 0x00, 0x01];
const LYRIC_MAGIC: [u8; 2] = [0x01, 0x50];
const METADATA_MAGIC: [u8; 2] = [0x01, 0x51];
const TEMPO_CONSTANT: f64 = 1_705_032_600.0;

fn parse_error(message: impl Into<String>) -> LibreSvipError {
    LibreSvipError::Parse(message.into())
}

fn u32_of(value: i128, what: &str) -> Result<u32> {
    u32::try_from(value).map_err(|_| parse_error(format!("{what} out of range: {value}")))
}

fn write_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn write_u16(output: &mut Vec<u8>, value: i64, what: &str) -> Result<()> {
    let value = u16::try_from(value).map_err(|_| parse_error(format!("{what} out of range")))?;
    output.extend_from_slice(&value.to_be_bytes());
    Ok(())
}

fn pad_bytes(output: &mut Vec<u8>, bytes: &[u8], length: usize, what: &str) -> Result<()> {
    if bytes.len() > length {
        return Err(parse_error(format!(
            "{what} does not fit into {length} bytes: {:?}",
            String::from_utf8_lossy(bytes)
        )));
    }
    output.extend_from_slice(bytes);
    output.resize(output.len() + (length - bytes.len()), 0);
    Ok(())
}

fn pad_ascii(output: &mut Vec<u8>, text: &str, length: usize) -> Result<()> {
    if !text.is_ascii() {
        return Err(parse_error(format!("non-ASCII text: {text:?}")));
    }
    pad_bytes(output, text.as_bytes(), length, "lyric")
}

fn pad_utf8(output: &mut Vec<u8>, text: &str, length: usize) -> Result<()> {
    pad_bytes(output, text.as_bytes(), length, "text")
}

/// `int.bit_length() - 1`, the encoded time-signature denominator.
fn denominator_code(denominator: i64) -> Result<u8> {
    if denominator <= 0 {
        return Err(parse_error(format!(
            "invalid time signature denominator: {denominator}"
        )));
    }
    let bits = 64 - (denominator as u64).leading_zeros();
    u8::try_from(bits - 1).map_err(|_| parse_error("denominator too large"))
}

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum VxEvent {
    Tempo {
        time: i64,
        tempo: f64,
    },
    TimeSignature {
        time: i64,
        numerator: i64,
        denominator: i64,
    },
    Lyric {
        time: i64,
        seq_stat: u8,
        seq_num: Option<i64>,
        text: String,
    },
    Metadata {
        time: i64,
        seq_stat: u8,
        text: String,
    },
    NoteOn {
        time: i64,
        note: i64,
        velocity: i64,
        on_data: i64,
    },
    NoteOff {
        time: i64,
        note: i64,
        velocity: i64,
        off_data: i64,
    },
}

impl VxEvent {
    pub fn time(&self) -> i64 {
        match *self {
            VxEvent::Tempo { time, .. }
            | VxEvent::TimeSignature { time, .. }
            | VxEvent::Lyric { time, .. }
            | VxEvent::Metadata { time, .. }
            | VxEvent::NoteOn { time, .. }
            | VxEvent::NoteOff { time, .. } => time,
        }
    }

    pub fn time_mut(&mut self) -> &mut i64 {
        match self {
            VxEvent::Tempo { time, .. }
            | VxEvent::TimeSignature { time, .. }
            | VxEvent::Lyric { time, .. }
            | VxEvent::Metadata { time, .. }
            | VxEvent::NoteOn { time, .. }
            | VxEvent::NoteOff { time, .. } => time,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            VxEvent::Tempo { .. } => "set_tempo",
            VxEvent::TimeSignature { .. } => "time_signature",
            VxEvent::Lyric { .. } => "lyrics",
            VxEvent::Metadata { .. } => "metadata",
            VxEvent::NoteOn { .. } => "note_on",
            VxEvent::NoteOff { .. } => "note_off",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct VxTrack {
    pub title_parts: Vec<(u8, String)>,
    pub events: Vec<VxEvent>,
}

#[derive(Clone, Debug, Default)]
pub struct VxFile {
    pub ticks_per_beat: i64,
    pub tracks: Vec<VxTrack>,
}

// ---------------------------------------------------------------------------
// Reader
// ---------------------------------------------------------------------------

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            position: 0,
        }
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.position)
    }

    fn peek(&self, length: usize) -> Result<&'a [u8]> {
        self.bytes
            .get(self.position..self.position + length)
            .ok_or_else(|| parse_error("unexpected end of file"))
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8]> {
        let slice = self.peek(length)?;
        self.position += length;
        Ok(slice)
    }

    fn read_u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn read_u16(&mut self) -> Result<u16> {
        let bytes = self.take(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn read_u32(&mut self) -> Result<u32> {
        let bytes = self.take(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn expect(&mut self, expected: &[u8]) -> Result<()> {
        let actual = self.take(expected.len())?;
        if actual != expected {
            return Err(parse_error(format!(
                "expected {expected:02x?}, found {actual:02x?}"
            )));
        }
        Ok(())
    }

    fn read_padded(&mut self, length: usize) -> Result<&'a [u8]> {
        let bytes = self.take(length)?;
        let end = bytes
            .iter()
            .rposition(|byte| *byte != 0)
            .map(|index| index + 1)
            .unwrap_or(0);
        Ok(&bytes[..end])
    }

    fn read_ascii(&mut self, length: usize) -> Result<String> {
        let bytes = self.read_padded(length)?;
        if !bytes.is_ascii() {
            return Err(parse_error("non-ASCII lyric text"));
        }
        Ok(String::from_utf8_lossy(bytes).into_owned())
    }

    fn read_utf8(&mut self, length: usize) -> Result<String> {
        let bytes = self.read_padded(length)?;
        String::from_utf8(bytes.to_vec()).map_err(|error| parse_error(error.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

impl VxFile {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let mut reader = Reader::new(bytes);
        reader.expect(b"SMF2CLIP")?;
        let raw_ppqn = reader.read_u32()? as i64;
        let ticks_per_beat = raw_ppqn - (BASE_DCTPQ << 16);
        let mut tracks = Vec::new();
        while reader.remaining() > 0 {
            match parse_track(&mut reader) {
                Ok(track) => tracks.push(track),
                // `GreedyRange` rewinds and stops at the first failure, leaving
                // any trailing bytes unconsumed.
                Err(_) => break,
            }
        }
        Ok(Self {
            ticks_per_beat,
            tracks,
        })
    }
}

fn parse_track(reader: &mut Reader<'_>) -> Result<VxTrack> {
    let mut title_parts = Vec::new();
    loop {
        let checkpoint = reader.position;
        match parse_clip_name(reader) {
            Ok(part) => title_parts.push(part),
            Err(_) => {
                reader.position = checkpoint;
                break;
            }
        }
    }
    reader.expect(&START_OF_CLIP)?;
    reader.expect(&[0u8; 12])?;
    let mut events = Vec::new();
    loop {
        let checkpoint = reader.position;
        match parse_event(reader) {
            Ok(event) => events.push(event),
            Err(_) => {
                reader.position = checkpoint;
                break;
            }
        }
    }
    reader.expect(&END_OF_CLIP)?;
    reader.expect(&[0u8; 12])?;
    Ok(VxTrack {
        title_parts,
        events,
    })
}

fn parse_clip_name(reader: &mut Reader<'_>) -> Result<(u8, String)> {
    reader.expect(&[0xd0])?;
    let seq_stat = reader.read_u8()?;
    reader.expect(&CLIP_NAME_MAGIC)?;
    let name = reader.read_utf8(12)?;
    Ok((seq_stat, name))
}

fn parse_event(reader: &mut Reader<'_>) -> Result<VxEvent> {
    let header = reader.peek(8)?;
    let raw_time = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
    let time = raw_time as i64 - (BASE_DELTA_TICKS << 16);
    let magic = &header[4..8];
    if magic == TEMPO_MAGIC {
        reader.take(4)?;
        reader.expect(&TEMPO_MAGIC)?;
        let raw = reader.read_u32()?;
        reader.expect(&[0u8; 8])?;
        if raw == 0 {
            return Err(parse_error("zero tempo denominator"));
        }
        Ok(VxEvent::Tempo {
            time,
            tempo: TEMPO_CONSTANT / raw as f64,
        })
    } else if magic == TIME_SIGNATURE_MAGIC {
        reader.take(4)?;
        reader.expect(&TIME_SIGNATURE_MAGIC)?;
        let numerator = reader.read_u8()? as i64;
        let denominator_code = reader.read_u8()?;
        reader.expect(&[0x08, 0, 0, 0, 0, 0, 0, 0, 0, 0])?;
        if denominator_code >= 63 {
            return Err(parse_error("time signature denominator too large"));
        }
        Ok(VxEvent::TimeSignature {
            time,
            numerator,
            denominator: 1i64 << denominator_code,
        })
    } else if magic[0] == 0xd0 && magic[2..4] == LYRIC_MAGIC {
        reader.take(4)?;
        reader.expect(&[0xd0])?;
        let seq_stat = reader.read_u8()?;
        reader.expect(&LYRIC_MAGIC)?;
        let has_seq_num = (seq_stat as i64 - 16).div_euclid(64) <= 1;
        if has_seq_num {
            let seq_num = reader.read_u16()? as i64;
            let text = reader.read_ascii(10)?;
            Ok(VxEvent::Lyric {
                time,
                seq_stat,
                seq_num: Some(seq_num),
                text,
            })
        } else {
            let text = reader.read_ascii(12)?;
            Ok(VxEvent::Lyric {
                time,
                seq_stat,
                seq_num: None,
                text,
            })
        }
    } else if magic[0] == 0xd0 && magic[2..4] == METADATA_MAGIC {
        reader.take(4)?;
        reader.expect(&[0xd0])?;
        let seq_stat = reader.read_u8()?;
        reader.expect(&METADATA_MAGIC)?;
        let text = reader.read_utf8(12)?;
        Ok(VxEvent::Metadata {
            time,
            seq_stat,
            text,
        })
    } else {
        let payload = u32::from_be_bytes([header[4], header[5], header[6], header[7]]);
        let high = payload >> 16;
        if high == BASE_START_PITCH as u32 {
            reader.take(8)?;
            let note = (payload >> 8) & 0xFF;
            if note > 127 {
                return Err(parse_error("note number out of range"));
            }
            let velocity = reader.read_u16()? as i64;
            let on_data = reader.read_u16()? as i64;
            Ok(VxEvent::NoteOn {
                time,
                note: note as i64,
                velocity,
                on_data,
            })
        } else if high == BASE_END_PITCH as u32 {
            reader.take(8)?;
            let note = (payload >> 8) & 0xFF;
            if note > 127 {
                return Err(parse_error("note number out of range"));
            }
            let velocity = reader.read_u16()? as i64;
            let off_data = reader.read_u16()? as i64;
            Ok(VxEvent::NoteOff {
                time,
                note: note as i64,
                velocity,
                off_data,
            })
        } else {
            Err(parse_error("unknown event"))
        }
    }
}

// ---------------------------------------------------------------------------
// Building
// ---------------------------------------------------------------------------

impl VxFile {
    pub fn build(&self) -> Result<Vec<u8>> {
        let mut output = Vec::new();
        output.extend_from_slice(b"SMF2CLIP");
        let raw_ppqn = self.ticks_per_beat + (BASE_DCTPQ << 16);
        write_u32(&mut output, u32_of(i128::from(raw_ppqn), "ticks per beat")?);
        for track in &self.tracks {
            encode_track(track, &mut output)?;
        }
        Ok(output)
    }
}

fn write_delta(output: &mut Vec<u8>, time: i64) -> Result<()> {
    let raw = i128::from(time) + i128::from(BASE_DELTA_TICKS << 16);
    write_u32(output, u32_of(raw, "delta ticks")?);
    Ok(())
}

fn encode_track(track: &VxTrack, output: &mut Vec<u8>) -> Result<()> {
    for (seq_stat, name) in &track.title_parts {
        output.push(0xd0);
        output.push(*seq_stat);
        output.extend_from_slice(&CLIP_NAME_MAGIC);
        pad_utf8(output, name, 12)?;
    }
    output.extend_from_slice(&START_OF_CLIP);
    output.extend_from_slice(&[0u8; 12]);
    for event in &track.events {
        encode_event(event, output)?;
    }
    output.extend_from_slice(&END_OF_CLIP);
    output.extend_from_slice(&[0u8; 12]);
    Ok(())
}

fn encode_event(event: &VxEvent, output: &mut Vec<u8>) -> Result<()> {
    write_delta(output, event.time())?;
    match *event {
        VxEvent::Tempo { tempo, .. } => {
            output.extend_from_slice(&TEMPO_MAGIC);
            if tempo == 0.0 {
                return Err(parse_error("zero tempo"));
            }
            let raw = (TEMPO_CONSTANT / tempo) as i128;
            write_u32(output, u32_of(raw, "tempo")?);
            output.extend_from_slice(&[0u8; 8]);
        }
        VxEvent::TimeSignature {
            numerator,
            denominator,
            ..
        } => {
            output.extend_from_slice(&TIME_SIGNATURE_MAGIC);
            output.push(
                u8::try_from(numerator).map_err(|_| parse_error("numerator out of range"))?,
            );
            output.push(denominator_code(denominator)?);
            output.push(0x08);
            output.extend_from_slice(&[0u8; 9]);
        }
        VxEvent::Lyric {
            seq_stat,
            seq_num,
            ref text,
            ..
        } => {
            output.push(0xd0);
            output.push(seq_stat);
            output.extend_from_slice(&LYRIC_MAGIC);
            let has_seq_num = (seq_stat as i64 - 16).div_euclid(64) <= 1;
            if has_seq_num {
                let seq_num = seq_num.ok_or_else(|| parse_error("missing seq_num"))?;
                write_u16(output, seq_num, "seq_num")?;
                pad_ascii(output, text, 10)?;
            } else {
                pad_ascii(output, text, 12)?;
            }
        }
        VxEvent::Metadata {
            seq_stat, ref text, ..
        } => {
            output.push(0xd0);
            output.push(seq_stat);
            output.extend_from_slice(&METADATA_MAGIC);
            pad_utf8(output, text, 12)?;
        }
        VxEvent::NoteOn {
            note,
            velocity,
            on_data,
            ..
        } => {
            let raw = 1i128 + (i128::from(note) << 8) + (i128::from(BASE_START_PITCH) << 16);
            write_u32(output, u32_of(raw, "note")?);
            write_u16(output, velocity, "velocity")?;
            write_u16(output, on_data, "on_data")?;
        }
        VxEvent::NoteOff {
            note,
            velocity,
            off_data,
            ..
        } => {
            let raw = 1i128 + (i128::from(note) << 8) + (i128::from(BASE_END_PITCH) << 16);
            write_u32(output, u32_of(raw, "note")?);
            write_u16(output, velocity, "velocity")?;
            write_u16(output, off_data, "off_data")?;
        }
    }
    Ok(())
}

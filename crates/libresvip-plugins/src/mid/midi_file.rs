//! Standard MIDI file (SMF) reading and writing.
//!
//! Port of `libresvip.utils.binary.midi` (the `construct`-based
//! `MIDIFile`/`MIDITrack`/`MIDIMessage` definitions). Parsing keeps the raw
//! event structure the `mid` parser inspects; building mirrors the `construct`
//! builder byte for byte, including the `VarIntBE` encoder algorithm.
//!
//! Delta times: parsed events keep the *delta* time exactly like `construct`;
//! the parser converts them to cumulative ticks (`_convert_delta_to_cumulative`)
//! and the generator converts back before building.

use libresvip_core::error::{LibreSvipError, Result};

use super::pitch::{PITCH_MAX_VALUE, PITCH_MIN_VALUE};

pub const MIDI_HEADER_CHUNK: &[u8; 4] = b"MThd";
pub const MIDI_TRACK_CHUNK: &[u8; 4] = b"MTrk";
pub const MIDI_HEADER_DATA_LENGTH: usize = 6;

/// `bpm2tempo` from `libresvip.utils.binary.midi`.
pub fn bpm2tempo(bpm: f64) -> Result<u32> {
    if bpm == 0.0 {
        // Python would raise ZeroDivisionError.
        return Err(parse_error("cannot convert a BPM of 0 to a MIDI tempo"));
    }
    let tempo = (60.0 * 1_000_000.0 / bpm).round_ties_even();
    if !(0.0..=0x00FF_FFFF as f64).contains(&tempo) {
        return Err(parse_error(format!(
            "tempo out of range for BPM {bpm}: {tempo}"
        )));
    }
    Ok(tempo as u32)
}

/// `tempo2bpm` from `libresvip.utils.binary.midi`.
pub fn tempo2bpm(tempo: u32) -> f64 {
    60.0 * 1_000_000.0 / f64::from(tempo)
}

fn parse_error(message: impl Into<String>) -> LibreSvipError {
    LibreSvipError::Parse(message.into())
}

// ---------------------------------------------------------------------------
// Parsed model
// ---------------------------------------------------------------------------

/// The `detail.data` variants `construct` decodes for status `0xFF`.
#[derive(Clone, Debug, PartialEq)]
pub enum MetaEvent {
    SequenceNumber(u16),
    Text(Vec<u8>),
    Copyright(Vec<u8>),
    TrackName(Vec<u8>),
    InstrumentName(Vec<u8>),
    Lyrics(Vec<u8>),
    Marker(Vec<u8>),
    CuePoint(Vec<u8>),
    ProgramName(Vec<u8>),
    DeviceName(Vec<u8>),
    ChannelPrefix(Vec<u8>),
    MidiPort(Vec<u8>),
    EndOfTrack,
    SetTempo(u32),
    SmpteOffset {
        frame_rate: u8,
        hours: u8,
        minutes: u8,
        seconds: u8,
        frames: u8,
        sub_frames: u8,
    },
    TimeSignature {
        numerator: u8,
        denominator: u8,
        clocks_per_click: u8,
        notated_32nd_notes_per_quarter: u8,
    },
    KeySignature {
        key: i8,
        scale: u8,
    },
    SequencerSpecific(Vec<u8>),
    /// Present so unknown meta events do not abort parsing.
    Unknown {
        event_type: u8,
        data: Vec<u8>,
    },
}

/// The `detail.data` variants `construct` decodes for channel messages.
#[derive(Clone, Debug, PartialEq)]
pub enum ChannelEvent {
    NoteOff { note: u8, velocity: u8 },
    NoteOn { note: u8, velocity: u8 },
    PolyTouch { note: u8, value: u8 },
    ControlChange { control: u8, value: u8 },
    ProgramChange { program: u8 },
    AfterTouch { value: u8 },
    /// `((msb << 7) | lsb) + PITCH_MIN_VALUE`.
    PitchWheel { pitch: i32 },
}

#[derive(Clone, Debug, PartialEq)]
pub enum EventKind {
    Meta(MetaEvent),
    SysEx(Vec<u8>),
    EscapeSequence(Vec<u8>),
    Channel { channel: u8, data: ChannelEvent },
}

#[derive(Clone, Debug, PartialEq)]
pub struct MidiEvent {
    pub time: i64,
    pub kind: EventKind,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MidiFile {
    pub format: u16,
    pub ticks_per_beat: i16,
    pub tracks: Vec<Vec<MidiEvent>>,
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

struct Reader<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    fn is_empty(&self) -> bool {
        self.position >= self.data.len()
    }

    fn peek(&self) -> Result<u8> {
        self.data
            .get(self.position)
            .copied()
            .ok_or_else(|| parse_error("unexpected end of MIDI data"))
    }

    fn u8(&mut self) -> Result<u8> {
        let byte = self.peek()?;
        self.position += 1;
        Ok(byte)
    }

    fn bytes(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self
            .position
            .checked_add(count)
            .ok_or_else(|| parse_error("MIDI chunk length overflow"))?;
        let slice = self
            .data
            .get(self.position..end)
            .ok_or_else(|| parse_error("unexpected end of MIDI data"))?;
        self.position = end;
        Ok(slice)
    }

    fn u32(&mut self) -> Result<u32> {
        let bytes = self.bytes(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn varint(&mut self) -> Result<u32> {
        let mut value: u32 = 0;
        loop {
            let byte = self.u8()?;
            value = (value << 7) | u32::from(byte & 0x7F);
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
    }
}

/// `MIDIFileStruct._parse`.
pub fn parse(data: &[u8]) -> Result<MidiFile> {
    let mut reader = Reader::new(data);
    let magic = reader.bytes(4)?;
    if magic != MIDI_HEADER_CHUNK {
        return Err(parse_error(format!(
            "parsing expected {MIDI_HEADER_CHUNK:?} but parsed {magic:?}"
        )));
    }
    let header_length = reader.u32()? as usize;
    if header_length < MIDI_HEADER_DATA_LENGTH {
        return Err(parse_error(format!(
            "MIDI header length must be at least {MIDI_HEADER_DATA_LENGTH}"
        )));
    }
    let header = reader.bytes(header_length)?;
    let format = u16::from_be_bytes([header[0], header[1]]);
    let track_count = u16::from_be_bytes([header[2], header[3]]) as usize;
    let ticks_per_beat = i16::from_be_bytes([header[4], header[5]]);
    let mut tracks = Vec::with_capacity(track_count);
    while tracks.len() < track_count {
        let chunk_magic = reader.bytes(4)?;
        let chunk_length = reader.u32()? as usize;
        let chunk_data = reader.bytes(chunk_length)?;
        if chunk_magic == MIDI_TRACK_CHUNK {
            tracks.push(parse_track(chunk_data)?);
        }
    }
    Ok(MidiFile {
        format,
        ticks_per_beat,
        tracks,
    })
}

fn parse_track(data: &[u8]) -> Result<Vec<MidiEvent>> {
    let mut reader = Reader::new(data);
    let mut events = Vec::new();
    let mut last_status: Option<u8> = None;
    while !reader.is_empty() {
        let time = i64::from(reader.varint()?);
        let next = reader.peek()?;
        let status = if next & 0x80 != 0 {
            let status = reader.u8()?;
            last_status = Some(status);
            status
        } else {
            last_status.ok_or_else(|| parse_error("running status without a status byte"))?
        };
        let kind = match status {
            0xFF => {
                let event_type = reader.u8()?;
                let length = reader.varint()? as usize;
                let payload = reader.bytes(length)?;
                EventKind::Meta(parse_meta(event_type, payload)?)
            }
            0xF0 => {
                let length = reader.varint()? as usize;
                EventKind::SysEx(reader.bytes(length)?.to_vec())
            }
            0xF7 => {
                let length = reader.varint()? as usize;
                EventKind::EscapeSequence(reader.bytes(length)?.to_vec())
            }
            0x80..=0xEF => {
                let channel = status & 0x0F;
                let data = match status & 0xF0 {
                    0x80 => ChannelEvent::NoteOff {
                        note: reader.u8()?,
                        velocity: reader.u8()?,
                    },
                    0x90 => ChannelEvent::NoteOn {
                        note: reader.u8()?,
                        velocity: reader.u8()?,
                    },
                    0xA0 => ChannelEvent::PolyTouch {
                        note: reader.u8()?,
                        value: reader.u8()?,
                    },
                    0xB0 => ChannelEvent::ControlChange {
                        control: reader.u8()?,
                        value: reader.u8()?,
                    },
                    0xC0 => ChannelEvent::ProgramChange {
                        program: reader.u8()?,
                    },
                    0xD0 => ChannelEvent::AfterTouch {
                        value: reader.u8()?,
                    },
                    0xE0 => {
                        let lsb = reader.u8()?;
                        let msb = reader.u8()?;
                        ChannelEvent::PitchWheel {
                            pitch: (i32::from(msb) << 7 | i32::from(lsb))
                                + PITCH_MIN_VALUE as i32,
                        }
                    }
                    _ => unreachable!("status & 0xF0 is in 0x80..=0xE0"),
                };
                EventKind::Channel { channel, data }
            }
            other => {
                return Err(parse_error(format!(
                    "unsupported MIDI status byte 0x{other:02X}"
                )));
            }
        };
        events.push(MidiEvent { time, kind });
    }
    Ok(events)
}

fn parse_meta(event_type: u8, payload: &[u8]) -> Result<MetaEvent> {
    let require = |count: usize| -> Result<()> {
        if payload.len() < count {
            Err(parse_error(format!(
                "meta event 0x{event_type:02X} needs {count} bytes, got {}",
                payload.len()
            )))
        } else {
            Ok(())
        }
    };
    let data = payload.to_vec();
    Ok(match event_type {
        0x00 => {
            require(2)?;
            MetaEvent::SequenceNumber(u16::from_be_bytes([payload[0], payload[1]]))
        }
        0x01 => MetaEvent::Text(data),
        0x02 => MetaEvent::Copyright(data),
        0x03 => MetaEvent::TrackName(data),
        0x04 => MetaEvent::InstrumentName(data),
        0x05 => MetaEvent::Lyrics(data),
        0x06 => MetaEvent::Marker(data),
        0x07 => MetaEvent::CuePoint(data),
        0x08 => MetaEvent::ProgramName(data),
        0x09 => MetaEvent::DeviceName(data),
        0x20 => MetaEvent::ChannelPrefix(data),
        0x21 => MetaEvent::MidiPort(data),
        0x2F => MetaEvent::EndOfTrack,
        0x51 => {
            require(3)?;
            MetaEvent::SetTempo(
                (u32::from(payload[0]) << 16)
                    | (u32::from(payload[1]) << 8)
                    | u32::from(payload[2]),
            )
        }
        0x54 => {
            require(5)?;
            MetaEvent::SmpteOffset {
                frame_rate: (payload[0] >> 5) & 0x03,
                hours: payload[0] & 0x1F,
                minutes: payload[1],
                seconds: payload[2],
                frames: payload[3],
                sub_frames: payload[4],
            }
        }
        0x58 => {
            require(4)?;
            MetaEvent::TimeSignature {
                numerator: payload[0],
                denominator: 1u8.wrapping_shl(u32::from(payload[1])),
                clocks_per_click: payload[2],
                notated_32nd_notes_per_quarter: payload[3],
            }
        }
        0x59 => {
            require(2)?;
            MetaEvent::KeySignature {
                key: payload[0] as i8,
                scale: payload[1],
            }
        }
        0x7F => MetaEvent::SequencerSpecific(data),
        _ => MetaEvent::Unknown { event_type, data },
    })
}

// ---------------------------------------------------------------------------
// Building
// ---------------------------------------------------------------------------

/// One message as produced by `midi_generator` (status byte implied).
#[derive(Clone, Debug, PartialEq)]
pub enum OutEvent {
    SetTempo(u32),
    TimeSignature {
        numerator: u8,
        denominator: u8,
        clocks_per_click: u8,
        notated_32nd_notes_per_quarter: u8,
    },
    TrackName(Vec<u8>),
    Lyrics(Vec<u8>),
    EndOfTrack,
    NoteOn { note: u8, velocity: u8 },
    NoteOff { note: u8, velocity: u8 },
    ControlChange { control: u8, value: u8 },
    PitchWheel { pitch: i32 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct OutMessage {
    pub time: i64,
    pub event: OutEvent,
}

/// `MIDIFileStruct._build`.
pub fn build(format: u16, ticks_per_beat: i64, tracks: &[Vec<OutMessage>]) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    output.extend_from_slice(MIDI_HEADER_CHUNK);
    output.extend_from_slice(&(MIDI_HEADER_DATA_LENGTH as u32).to_be_bytes());
    output.extend_from_slice(&format.to_be_bytes());
    let track_count = u16::try_from(tracks.len())
        .map_err(|_| parse_error("too many MIDI tracks"))?;
    output.extend_from_slice(&track_count.to_be_bytes());
    let ticks = i16::try_from(ticks_per_beat)
        .map_err(|_| parse_error(format!("ticks per beat out of range: {ticks_per_beat}")))?;
    output.extend_from_slice(&ticks.to_be_bytes());
    for track in tracks {
        let mut data = Vec::new();
        for message in track {
            write_varint(&mut data, message.time)?;
            write_event(&mut data, &message.event)?;
        }
        output.extend_from_slice(MIDI_TRACK_CHUNK);
        let length = u32::try_from(data.len())
            .map_err(|_| parse_error("MIDI track chunk is too large"))?;
        output.extend_from_slice(&length.to_be_bytes());
        output.extend_from_slice(&data);
    }
    Ok(output)
}

/// `VarIntBE._build` — the musx algorithm used by Python, including its
/// three-byte limit for the leading groups.
fn write_varint(output: &mut Vec<u8>, value: i64) -> Result<()> {
    if value < 0 {
        return Err(parse_error(format!(
            "VarIntBE cannot build from negative number {value}"
        )));
    }
    let value = value as i128;
    let mut encoded = Vec::new();
    for shift in [21i128, 14, 7] {
        if value >= (1i128 << shift) {
            encoded.push((((value >> shift) & 0x7F) | 0x80) as u8);
        }
    }
    encoded.push((value & 0x7F) as u8);
    output.extend_from_slice(&encoded);
    Ok(())
}

fn write_event(output: &mut Vec<u8>, event: &OutEvent) -> Result<()> {
    match event {
        OutEvent::SetTempo(tempo) => {
            if *tempo > 0x00FF_FFFF {
                return Err(parse_error(format!("tempo out of range: {tempo}")));
            }
            output.push(0xFF);
            output.push(0x51);
            output.push(3);
            output.extend_from_slice(&[
                ((*tempo >> 16) & 0xFF) as u8,
                ((*tempo >> 8) & 0xFF) as u8,
                (*tempo & 0xFF) as u8,
            ]);
        }
        OutEvent::TimeSignature {
            numerator,
            denominator,
            clocks_per_click,
            notated_32nd_notes_per_quarter,
        } => {
            if *denominator == 0 {
                return Err(parse_error("time signature denominator must not be zero"));
            }
            // `denominator.bit_length() - 1`
            let exponent = 7 - denominator.leading_zeros();
            output.push(0xFF);
            output.push(0x58);
            output.push(4);
            output.push(*numerator);
            output.push(exponent as u8);
            output.push(*clocks_per_click);
            output.push(*notated_32nd_notes_per_quarter);
        }
        OutEvent::TrackName(name) => write_meta_bytes(output, 0x03, name)?,
        OutEvent::Lyrics(text) => write_meta_bytes(output, 0x05, text)?,
        OutEvent::EndOfTrack => {
            output.push(0xFF);
            output.push(0x2F);
            output.push(0);
        }
        OutEvent::NoteOn { note, velocity } => {
            output.push(0x90);
            output.push(*note);
            output.push(*velocity);
        }
        OutEvent::NoteOff { note, velocity } => {
            output.push(0x80);
            output.push(*note);
            output.push(*velocity);
        }
        OutEvent::ControlChange { control, value } => {
            output.push(0xB0);
            output.push(*control);
            output.push(*value);
        }
        OutEvent::PitchWheel { pitch } => {
            let pitch = *pitch;
            if pitch < PITCH_MIN_VALUE as i32 || pitch > PITCH_MAX_VALUE as i32 {
                return Err(parse_error(format!("pitch bend out of range: {pitch}")));
            }
            let value = pitch - PITCH_MIN_VALUE as i32;
            output.push(0xE0);
            output.push((value & 0x7F) as u8);
            output.push(((value >> 7) & 0x7F) as u8);
        }
    }
    Ok(())
}

fn write_meta_bytes(output: &mut Vec<u8>, event_type: u8, bytes: &[u8]) -> Result<()> {
    output.push(0xFF);
    output.push(event_type);
    let mut length = Vec::new();
    write_varint(&mut length, bytes.len() as i64)?;
    output.extend_from_slice(&length);
    output.extend_from_slice(bytes);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_matches_python_encoder() {
        for (value, expected) in [
            (0i64, vec![0x00]),
            (0x7F, vec![0x7F]),
            (0x80, vec![0x81, 0x00]),
            (480, vec![0x83, 0x60]),
            (240, vec![0x81, 0x70]),
            (0x3FFF, vec![0xFF, 0x7F]),
        ] {
            let mut encoded = Vec::new();
            write_varint(&mut encoded, value).unwrap();
            assert_eq!(encoded, expected, "value {value}");
        }
    }

    #[test]
    fn round_trip_header() {
        let tracks = vec![
            vec![OutMessage {
                time: 0,
                event: OutEvent::EndOfTrack,
            }],
            Vec::new(),
        ];
        let bytes = build(1, 480, &tracks).unwrap();
        let file = parse(&bytes).unwrap();
        assert_eq!(file.format, 1);
        assert_eq!(file.ticks_per_beat, 480);
        assert_eq!(file.tracks.len(), 2);
        assert_eq!(file.tracks[0][0].kind, EventKind::Meta(MetaEvent::EndOfTrack));
    }
}

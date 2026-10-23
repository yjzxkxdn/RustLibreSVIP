//! Port of `libresvip.utils.binary.midi` (the subset used by the `vsq`
//! plugin): a Standard MIDI File reader/writer that reproduces `construct`'s
//! wire format.
//!
//! The Python module exposes a `construct` descriptor; only meta events are
//! consumed by the plugin, but the reader skips channel/sysex messages with the
//! correct payload sizes so running status and mixed files keep working.

use libresvip_core::error::{LibreSvipError, Result};

pub const PITCH_MIN_VALUE: i64 = -8192;
pub const PITCH_MAX_VALUE: i64 = 8191;
pub const DEFAULT_PITCH_BEND_SENSITIVITY: i64 = 2;
pub const MAX_PITCH_BEND_SENSITIVITY: i64 = 24;

pub const MIDI_HEADER_CHUNK: &[u8] = b"MThd";
pub const MIDI_TRACK_CHUNK: &[u8] = b"MTrk";
pub const MIDI_HEADER_DATA_LENGTH: usize = 6;

/// `bpm2tempo` from mido.
pub fn bpm2tempo(bpm: f64) -> u32 {
    // Python `round` (ties to even) on a possibly fractional value.
    ((60.0 * 1_000_000.0) / bpm).round_ties_even() as u32
}

/// `tempo2bpm` from mido.
pub fn tempo2bpm(tempo: i64) -> f64 {
    (60.0 * 1_000_000.0) / tempo as f64
}

#[derive(Clone, Debug)]
pub enum MetaEvent {
    Text(Vec<u8>),
    TrackName(Vec<u8>),
    Copyright(Vec<u8>),
    InstrumentName(Vec<u8>),
    Lyrics(Vec<u8>),
    Marker(Vec<u8>),
    CuePoint(Vec<u8>),
    ProgramName(Vec<u8>),
    DeviceName(Vec<u8>),
    SetTempo(u32),
    TimeSignature {
        numerator: u8,
        denominator: i64,
        clocks_per_click: u8,
        notated_32nd_notes_per_quarter: u8,
    },
    EndOfTrack,
    SequenceNumber(u16),
    ChannelPrefix(Vec<u8>),
    MidiPort(Vec<u8>),
    SmpteOffset,
    KeySignature { key: i8, scale: u8 },
    SequencerSpecific(Vec<u8>),
}

#[derive(Clone, Debug)]
pub enum MidiEventKind {
    Meta(MetaEvent),
    Sysex(Vec<u8>),
    EscapeSequence(Vec<u8>),
    Channel {
        status: u8,
        channel: u8,
        data: Vec<u8>,
    },
}

#[derive(Clone, Debug)]
pub struct MidiEvent {
    /// Absolute (cumulative) tick position, filling `_convert_delta_to_cumulative`.
    pub time: i64,
    pub delta: i64,
    /// Raw status byte (0xFF for meta events).
    pub status: u8,
    pub kind: MidiEventKind,
}

#[derive(Clone, Debug)]
pub struct MidiTrack {
    pub events: Vec<MidiEvent>,
}

#[derive(Clone, Debug)]
pub struct MidiFile {
    pub midi_type: u16,
    pub ticks_per_beat: i64,
    pub tracks: Vec<MidiTrack>,
}

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.offset)
    }

    fn read_bytes(&mut self, count: usize) -> Result<&'a [u8]> {
        if self.remaining() < count {
            return Err(LibreSvipError::Parse(
                "unexpected end of MIDI stream".to_owned(),
            ));
        }
        let slice = &self.bytes[self.offset..self.offset + count];
        self.offset += count;
        Ok(slice)
    }

    fn read_u8(&mut self) -> Result<u8> {
        Ok(self.read_bytes(1)?[0])
    }

    fn read_u32(&mut self) -> Result<u32> {
        let bytes = self.read_bytes(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// `VarIntBE` (base-128, big endian).
    fn read_varint(&mut self) -> Result<i64> {
        let mut acc: Vec<u8> = Vec::new();
        loop {
            let byte = self.read_u8()?;
            acc.push(byte & 0x7F);
            if byte & 0x80 == 0 {
                break;
            }
        }
        let mut value = 0i64;
        for byte in acc {
            value = (value << 7) | i64::from(byte);
        }
        Ok(value)
    }
}

/// Parse a Standard MIDI File (`MIDIFileStruct._parse`).
pub fn parse_midi(bytes: &[u8]) -> Result<MidiFile> {
    let mut reader = Reader::new(bytes);
    let magic = reader.read_bytes(4)?;
    if magic != MIDI_HEADER_CHUNK {
        return Err(LibreSvipError::Parse(format!(
            "parsing expected {MIDI_HEADER_CHUNK:?} but parsed {magic:?}"
        )));
    }
    let header_length = reader.read_u32()? as usize;
    if header_length < MIDI_HEADER_DATA_LENGTH {
        return Err(LibreSvipError::Parse(format!(
            "MIDI header length must be at least {MIDI_HEADER_DATA_LENGTH}"
        )));
    }
    let header = reader.read_bytes(header_length)?;
    let midi_type = u16::from_be_bytes([header[0], header[1]]);
    if midi_type > 2 {
        return Err(LibreSvipError::Parse(format!(
            "unknown MIDI file type: {midi_type}"
        )));
    }
    let track_count = u16::from_be_bytes([header[2], header[3]]) as usize;
    let ticks_per_beat = i16::from_be_bytes([header[4], header[5]]) as i64;

    let mut tracks: Vec<MidiTrack> = Vec::new();
    while tracks.len() < track_count {
        if reader.remaining() < 8 {
            break;
        }
        let chunk_magic = reader.read_bytes(4)?;
        let chunk_length = reader.read_u32()? as usize;
        let chunk_data = reader.read_bytes(chunk_length)?;
        if chunk_magic == MIDI_TRACK_CHUNK {
            tracks.push(parse_track(chunk_data)?);
        }
    }

    Ok(MidiFile {
        midi_type,
        ticks_per_beat,
        tracks,
    })
}

fn parse_track(data: &[u8]) -> Result<MidiTrack> {
    let mut reader = Reader::new(data);
    let mut events: Vec<MidiEvent> = Vec::new();
    let mut last_status: Option<u8> = None;
    while reader.remaining() > 0 {
        let delta = reader.read_varint()?;
        let next = reader.read_u8()?;
        let status = if next & 0x80 != 0 {
            last_status = Some(next);
            next
        } else {
            // Running status: the peeked byte is actual data.
            reader.offset -= 1;
            last_status.ok_or_else(|| {
                LibreSvipError::Parse("running status without a previous status byte".to_owned())
            })?
        };
        let kind = if status == 0xFF {
            let event_type = reader.read_u8()?;
            let length = reader.read_varint()? as usize;
            let payload = reader.read_bytes(length)?;
            MidiEventKind::Meta(parse_meta(event_type, payload)?)
        } else if status == 0xF0 || status == 0xF7 {
            let length = reader.read_varint()? as usize;
            let payload = reader.read_bytes(length)?.to_vec();
            if status == 0xF0 {
                MidiEventKind::Sysex(payload)
            } else {
                MidiEventKind::EscapeSequence(payload)
            }
        } else if status & 0x80 != 0 {
            let data_length = match status & 0xF0 {
                0x80 | 0x90 | 0xA0 | 0xB0 | 0xE0 => 2,
                0xC0 | 0xD0 => 1,
                _ => {
                    return Err(LibreSvipError::Parse(format!(
                        "unsupported MIDI status byte: {status:#04x}"
                    )))
                }
            };
            let payload = reader.read_bytes(data_length)?.to_vec();
            MidiEventKind::Channel {
                status,
                channel: status & 0x0F,
                data: payload,
            }
        } else {
            return Err(LibreSvipError::Parse(format!(
                "invalid MIDI status byte: {status:#04x}"
            )));
        };
        let time = events
            .last()
            .map(|event: &MidiEvent| event.time)
            .unwrap_or(0)
            + delta;
        events.push(MidiEvent {
            time,
            delta,
            status,
            kind,
        });
    }
    Ok(MidiTrack { events })
}

fn parse_meta(event_type: u8, payload: &[u8]) -> Result<MetaEvent> {
    let owned = payload.to_vec();
    match event_type {
        0x00 => {
            if payload.len() < 2 {
                return Err(LibreSvipError::Parse(
                    "truncated sequence number event".to_owned(),
                ));
            }
            Ok(MetaEvent::SequenceNumber(u16::from_be_bytes([
                payload[0], payload[1],
            ])))
        }
        0x01 => Ok(MetaEvent::Text(owned)),
        0x02 => Ok(MetaEvent::Copyright(owned)),
        0x03 => Ok(MetaEvent::TrackName(owned)),
        0x04 => Ok(MetaEvent::InstrumentName(owned)),
        0x05 => Ok(MetaEvent::Lyrics(owned)),
        0x06 => Ok(MetaEvent::Marker(owned)),
        0x07 => Ok(MetaEvent::CuePoint(owned)),
        0x08 => Ok(MetaEvent::ProgramName(owned)),
        0x09 => Ok(MetaEvent::DeviceName(owned)),
        0x20 => Ok(MetaEvent::ChannelPrefix(owned)),
        0x21 => Ok(MetaEvent::MidiPort(owned)),
        0x2F => Ok(MetaEvent::EndOfTrack),
        0x51 => {
            if payload.len() < 3 {
                return Err(LibreSvipError::Parse(
                    "truncated set_tempo event".to_owned(),
                ));
            }
            Ok(MetaEvent::SetTempo(
                (u32::from(payload[0]) << 16)
                    | (u32::from(payload[1]) << 8)
                    | u32::from(payload[2]),
            ))
        }
        0x54 => Ok(MetaEvent::SmpteOffset),
        0x58 => {
            if payload.len() < 4 {
                return Err(LibreSvipError::Parse(
                    "truncated time_signature event".to_owned(),
                ));
            }
            Ok(MetaEvent::TimeSignature {
                numerator: payload[0],
                denominator: if payload[1] < 63 {
                    1i64 << payload[1]
                } else {
                    i64::MAX
                },
                clocks_per_click: payload[2],
                notated_32nd_notes_per_quarter: payload[3],
            })
        }
        0x59 => {
            if payload.len() < 2 {
                return Err(LibreSvipError::Parse(
                    "truncated key_signature event".to_owned(),
                ));
            }
            Ok(MetaEvent::KeySignature {
                key: payload[0] as i8,
                scale: payload[1],
            })
        }
        0x7F => Ok(MetaEvent::SequencerSpecific(owned)),
        other => Err(LibreSvipError::Parse(format!(
            "unsupported meta event type: {other:#04x}"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------

/// A message produced by the generators (cumulative time; `build` converts it
/// back to delta times).
#[derive(Clone, Debug)]
pub enum BuildEvent {
    TrackName(Vec<u8>),
    Text(Vec<u8>),
    SetTempo(u32),
    TimeSignature {
        numerator: u8,
        denominator: i64,
        clocks_per_click: u8,
        notated_32nd_notes_per_quarter: u8,
    },
    EndOfTrack,
}

#[derive(Clone, Debug)]
pub struct BuildMessage {
    pub time: i64,
    pub event: BuildEvent,
}

fn write_varint(out: &mut Vec<u8>, value: i64) -> Result<()> {
    if value < 0 {
        return Err(LibreSvipError::msg(format!(
            "VarIntBE cannot build from negative number {value}"
        )));
    }
    let mut bytes = Vec::new();
    let mut shift = 21;
    while shift > 0 {
        if value >= (1i64 << shift) {
            bytes.push((((value >> shift) & 0x7F) as u8) | 0x80);
        }
        shift -= 7;
    }
    bytes.push((value & 0x7F) as u8);
    out.extend_from_slice(&bytes);
    Ok(())
}

/// `MIDIFileStruct._build` + `_convert_cumulative_to_delta`.
pub fn build_midi(
    midi_type: u16,
    ticks_per_beat: i64,
    tracks: &[Vec<BuildMessage>],
) -> Result<Vec<u8>> {
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(MIDI_HEADER_CHUNK);
    out.extend_from_slice(&(MIDI_HEADER_DATA_LENGTH as u32).to_be_bytes());
    out.extend_from_slice(&midi_type.to_be_bytes());
    out.extend_from_slice(&(tracks.len() as u16).to_be_bytes());
    out.extend_from_slice(&(ticks_per_beat as i16).to_be_bytes());
    for track in tracks {
        let mut data: Vec<u8> = Vec::new();
        let mut previous = 0i64;
        for message in track {
            let delta = message.time - previous;
            previous = message.time;
            write_varint(&mut data, delta)?;
            match &message.event {
                BuildEvent::TrackName(bytes) => {
                    data.push(0xFF);
                    data.push(0x03);
                    write_varint(&mut data, bytes.len() as i64)?;
                    data.extend_from_slice(bytes);
                }
                BuildEvent::Text(bytes) => {
                    data.push(0xFF);
                    data.push(0x01);
                    write_varint(&mut data, bytes.len() as i64)?;
                    data.extend_from_slice(bytes);
                }
                BuildEvent::SetTempo(tempo) => {
                    data.push(0xFF);
                    data.push(0x51);
                    write_varint(&mut data, 3)?;
                    data.extend_from_slice(&[
                        ((tempo >> 16) & 0xFF) as u8,
                        ((tempo >> 8) & 0xFF) as u8,
                        (tempo & 0xFF) as u8,
                    ]);
                }
                BuildEvent::TimeSignature {
                    numerator,
                    denominator,
                    clocks_per_click,
                    notated_32nd_notes_per_quarter,
                } => {
                    data.push(0xFF);
                    data.push(0x58);
                    write_varint(&mut data, 4)?;
                    data.push(*numerator);
                    // `encoder=lambda obj, context: obj.bit_length() - 1`
                    data.push(
                        (63u32).saturating_sub((*denominator as u64).leading_zeros()) as u8,
                    );
                    data.push(*clocks_per_click);
                    data.push(*notated_32nd_notes_per_quarter);
                }
                BuildEvent::EndOfTrack => {
                    data.push(0xFF);
                    data.push(0x2F);
                    write_varint(&mut data, 0)?;
                }
            }
        }
        out.extend_from_slice(MIDI_TRACK_CHUNK);
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(&data);
    }
    Ok(out)
}

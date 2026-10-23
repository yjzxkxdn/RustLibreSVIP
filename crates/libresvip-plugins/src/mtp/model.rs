//! Muta project binary model 鈥?port of `libresvip.plugins.mtp.model`.
//!
//! Python uses `construct`; the byte layout (including the `\n` separators of
//! `muta_prefixed_array`) is reproduced explicitly here.

use libresvip_core::error::{LibreSvipError, Result};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MutaTempo {
    pub position: i32,
    pub bpm: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MutaTimeSignature {
    pub measure_position: i32,
    pub numerator: u32,
    pub denominator: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MutaAudioTrackData {
    pub start: u32,
    pub length: u32,
    pub file_path: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MutaNoteTiming {
    pub ori_pos: i32,
    pub mod_pos: i32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MutaNote {
    pub start: i32,
    pub length: u32,
    pub key: u32,
    pub lyric: [u16; 8],
    pub phoneme: String,
    pub tmg_data: [MutaNoteTiming; 5],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MutaPoint {
    pub time: i32,
    pub value: i32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MutaParams {
    pub unknown_param: Vec<MutaPoint>,
    pub pitch_range: Vec<MutaPoint>,
    pub volume_data: Vec<MutaPoint>,
    pub pitch_data: Vec<MutaPoint>,
    pub vibrato_amplitude_range: Vec<MutaPoint>,
    pub vibrato_amplitude_data: Vec<MutaPoint>,
    pub vibrato_frequency_range: Vec<MutaPoint>,
    pub vibrato_frequency_data: Vec<MutaPoint>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MutaSongTrackData {
    pub start: u32,
    pub length: u32,
    pub singer_name: Vec<u16>,
    pub speaker_id: u32,
    pub notes: Vec<MutaNote>,
    pub params: MutaParams,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MutaPhoneme {
    pub volume: u8,
    pub pos1: u16,
    pub length: u8,
    pub pos2: u16,
    pub pitch: u32,
    pub phoneme: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MutaText {
    pub text: Vec<u8>,
    pub volume: u8,
    pub speed: u8,
    pub tone: u8,
    pub quality: u8,
    pub normal: u8,
    pub happiness: u8,
    pub sadness: u8,
    pub reserved: u8,
    pub phonemes: Vec<MutaPhoneme>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MutaTalkTrackData {
    pub start: u32,
    pub length: u32,
    pub talker_name: Vec<u16>,
    pub speaker_id: u32,
    pub text: MutaText,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MutaTrackData {
    Talk(Vec<MutaTalkTrackData>),
    Song(Vec<MutaSongTrackData>),
    Audio(Vec<MutaAudioTrackData>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MutaTrack {
    pub track_type: u32,
    pub seq_count: u32,
    pub mute: u32,
    pub solo: u32,
    pub volume: u32,
    pub pan: u32,
    pub name: String,
    pub padding: [u8; 52],
    pub data: MutaTrackData,
}

impl Default for MutaTrack {
    fn default() -> Self {
        Self {
            track_type: 0,
            seq_count: 0,
            mute: 0,
            solo: 0,
            volume: 0,
            pan: 0,
            name: String::new(),
            padding: [0; 52],
            data: MutaTrackData::default(),
        }
    }
}

impl Default for MutaTrackData {
    fn default() -> Self {
        Self::Song(Vec::new())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MutaProject {
    pub file_version: u32,
    pub tempos: Vec<MutaTempo>,
    pub time_signatures: Vec<MutaTimeSignature>,
    pub tracks: Vec<MutaTrack>,
}

pub const TRACK_TYPE_TALK: u32 = 1;
pub const TRACK_TYPE_SONG: u32 = 2;
pub const TRACK_TYPE_AUDIO: u32 = 3;

fn parse_error(message: impl Into<String>) -> LibreSvipError {
    LibreSvipError::Parse(message.into())
}

struct Reader<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self
            .position
            .checked_add(count)
            .ok_or_else(|| parse_error("overflow"))?;
        if end > self.data.len() {
            return Err(parse_error("unexpected end of file"));
        }
        let slice = &self.data[self.position..end];
        self.position = end;
        Ok(slice)
    }

    fn line_break(&mut self) -> Result<()> {
        if self.take(1)?[0] != b'\n' {
            return Err(parse_error("expected a line break"));
        }
        Ok(())
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16> {
        let bytes: [u8; 2] = self.take(2)?.try_into().map_err(|_| parse_error("short u16"))?;
        Ok(u16::from_le_bytes(bytes))
    }

    fn u32(&mut self) -> Result<u32> {
        let bytes: [u8; 4] = self.take(4)?.try_into().map_err(|_| parse_error("short u32"))?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn i32(&mut self) -> Result<i32> {
        let bytes: [u8; 4] = self.take(4)?.try_into().map_err(|_| parse_error("short i32"))?;
        Ok(i32::from_le_bytes(bytes))
    }

    fn u16_array<const N: usize>(&mut self) -> Result<[u16; N]> {
        let mut values = [0u16; N];
        for value in values.iter_mut() {
            *value = self.u16()?;
        }
        Ok(values)
    }

    fn byte_string(&mut self) -> Result<Vec<u8>> {
        let length = self.u32()? as usize;
        Ok(self.take(length)?.to_vec())
    }

    fn pascal_string(&mut self) -> Result<String> {
        let bytes = self.byte_string()?;
        String::from_utf8(bytes)
            .map_err(|error| parse_error(format!("invalid utf-8 string: {error}")))
    }

    fn padded_utf16(&mut self, byte_length: usize) -> Result<String> {
        let mut bytes = self.take(byte_length)?.to_vec();
        // `NullStripped(..., pad=encodingunit("utf-16-le"))` strips whole
        // two-byte null units.
        while bytes.len() >= 2 && bytes[bytes.len() - 2..] == [0, 0] {
            bytes.truncate(bytes.len() - 2);
        }
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        Ok(String::from_utf16_lossy(&units))
    }

    fn prefixed_array<T>(
        &mut self,
        mut parse: impl FnMut(&mut Reader<'a>) -> Result<T>,
    ) -> Result<Vec<T>> {
        self.line_break()?;
        let count = self.u32()? as usize;
        let mut items = Vec::new();
        for _ in 0..count {
            self.line_break()?;
            items.push(parse(self)?);
        }
        Ok(items)
    }

    fn points(&mut self) -> Result<Vec<MutaPoint>> {
        self.prefixed_array(|reader| {
            Ok(MutaPoint {
                time: reader.i32()?,
                value: reader.i32()?,
            })
        })
    }

    fn params(&mut self) -> Result<MutaParams> {
        Ok(MutaParams {
            unknown_param: self.points()?,
            pitch_range: self.points()?,
            volume_data: self.points()?,
            pitch_data: self.points()?,
            vibrato_amplitude_range: self.points()?,
            vibrato_amplitude_data: self.points()?,
            vibrato_frequency_range: self.points()?,
            vibrato_frequency_data: self.points()?,
        })
    }

    fn note(&mut self) -> Result<MutaNote> {
        let start = self.i32()?;
        let length = self.u32()?;
        let key = self.u32()?;
        let lyric = self.u16_array::<8>()?;
        let phoneme = self.padded_utf16(16)?;
        let mut tmg_data = [MutaNoteTiming::default(); 5];
        for timing in tmg_data.iter_mut() {
            *timing = MutaNoteTiming {
                ori_pos: self.i32()?,
                mod_pos: self.i32()?,
            };
        }
        Ok(MutaNote {
            start,
            length,
            key,
            lyric,
            phoneme,
            tmg_data,
        })
    }

    fn song_track_data(&mut self) -> Result<MutaSongTrackData> {
        self.line_break()?;
        let start = self.u32()?;
        let length = self.u32()?;
        let singer_name = self.u16_array::<258>()?.to_vec();
        let speaker_id = self.u32()?;
        let notes = self.prefixed_array(|reader| reader.note())?;
        let params = self.params()?;
        Ok(MutaSongTrackData {
            start,
            length,
            singer_name,
            speaker_id,
            notes,
            params,
        })
    }

    fn audio_track_data(&mut self) -> Result<MutaAudioTrackData> {
        self.line_break()?;
        Ok(MutaAudioTrackData {
            start: self.u32()?,
            length: self.u32()?,
            file_path: self.pascal_string()?,
        })
    }

    fn phoneme(&mut self) -> Result<MutaPhoneme> {
        Ok(MutaPhoneme {
            volume: self.u8()?,
            pos1: self.u16()?,
            length: self.u8()?,
            pos2: self.u16()?,
            pitch: self.u32()?,
            phoneme: self.byte_string()?,
        })
    }

    fn text(&mut self) -> Result<MutaText> {
        let text = self.byte_string()?;
        self.line_break()?;
        let volume = self.u8()?;
        let speed = self.u8()?;
        let tone = self.u8()?;
        let quality = self.u8()?;
        let normal = self.u8()?;
        let happiness = self.u8()?;
        let sadness = self.u8()?;
        let reserved = self.u8()?;
        let phonemes = self.prefixed_array(|reader| reader.phoneme())?;
        Ok(MutaText {
            text,
            volume,
            speed,
            tone,
            quality,
            normal,
            happiness,
            sadness,
            reserved,
            phonemes,
        })
    }

    fn talk_track_data(&mut self) -> Result<MutaTalkTrackData> {
        self.line_break()?;
        let start = self.u32()?;
        let length = self.u32()?;
        let talker_name = self.u16_array::<258>()?.to_vec();
        let speaker_id = self.u32()?;
        self.line_break()?;
        let text = self.text()?;
        Ok(MutaTalkTrackData {
            start,
            length,
            talker_name,
            speaker_id,
            text,
        })
    }

    fn track(&mut self) -> Result<MutaTrack> {
        let track_type = self.u32()?;
        let seq_count = self.u32()?;
        self.line_break()?;
        let mute = self.u32()?;
        let solo = self.u32()?;
        let volume = self.u32()?;
        let pan = self.u32()?;
        let name = self.padded_utf16(12)?;
        let mut padding = [0u8; 52];
        padding.copy_from_slice(self.take(52)?);
        let data = match track_type {
            TRACK_TYPE_TALK => {
                let mut items = Vec::new();
                for _ in 0..seq_count {
                    items.push(self.talk_track_data()?);
                }
                MutaTrackData::Talk(items)
            }
            TRACK_TYPE_SONG => {
                let mut items = Vec::new();
                for _ in 0..seq_count {
                    items.push(self.song_track_data()?);
                }
                MutaTrackData::Song(items)
            }
            TRACK_TYPE_AUDIO => {
                let mut items = Vec::new();
                for _ in 0..seq_count {
                    items.push(self.audio_track_data()?);
                }
                MutaTrackData::Audio(items)
            }
            other => {
                return Err(parse_error(format!("unknown Muta track type {other}")));
            }
        };
        Ok(MutaTrack {
            track_type,
            seq_count,
            mute,
            solo,
            volume,
            pan,
            name,
            padding,
            data,
        })
    }
}

/// `muta_project_struct.parse`.
pub fn parse_project(data: &[u8]) -> Result<MutaProject> {
    let mut reader = Reader::new(data);
    let file_version = reader.u32()?;
    let tempos = reader.prefixed_array(|reader| {
        Ok(MutaTempo {
            position: reader.i32()?,
            bpm: reader.u32()?,
        })
    })?;
    let time_signatures = reader.prefixed_array(|reader| {
        Ok(MutaTimeSignature {
            measure_position: reader.i32()?,
            numerator: reader.u32()?,
            denominator: reader.u32()?,
        })
    })?;
    let tracks = reader.prefixed_array(|reader| reader.track())?;
    reader.line_break()?;
    Ok(MutaProject {
        file_version,
        tempos,
        time_signatures,
        tracks,
    })
}

// ---------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_byte_string(out: &mut Vec<u8>, bytes: &[u8]) {
    push_u32(out, bytes.len() as u32);
    out.extend_from_slice(bytes);
}

fn push_pascal_string(out: &mut Vec<u8>, value: &str) {
    push_byte_string(out, value.as_bytes());
}

fn push_padded_utf16(out: &mut Vec<u8>, value: &str, byte_length: usize) -> Result<()> {
    let units: Vec<u16> = value.encode_utf16().collect();
    let mut bytes = Vec::with_capacity(units.len() * 2);
    for unit in &units {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    if bytes.len() > byte_length {
        return Err(parse_error("string does not fit the fixed field"));
    }
    bytes.resize(byte_length, 0);
    out.extend_from_slice(&bytes);
    Ok(())
}

fn push_u16_array(out: &mut Vec<u8>, values: &[u16], count: usize) {
    for index in 0..count {
        push_u16(out, values.get(index).copied().unwrap_or(0));
    }
}

fn write_prefixed_array<T>(
    out: &mut Vec<u8>,
    items: &[T],
    mut write: impl FnMut(&mut Vec<u8>, &T) -> Result<()>,
) -> Result<()> {
    out.push(b'\n');
    push_u32(out, items.len() as u32);
    for item in items {
        out.push(b'\n');
        write(out, item)?;
    }
    Ok(())
}

fn write_points(out: &mut Vec<u8>, points: &[MutaPoint]) -> Result<()> {
    write_prefixed_array(out, points, |payload, point| {
        push_i32(payload, point.time);
        push_i32(payload, point.value);
        Ok(())
    })
}

fn write_params(out: &mut Vec<u8>, params: &MutaParams) -> Result<()> {
    write_points(out, &params.unknown_param)?;
    write_points(out, &params.pitch_range)?;
    write_points(out, &params.volume_data)?;
    write_points(out, &params.pitch_data)?;
    write_points(out, &params.vibrato_amplitude_range)?;
    write_points(out, &params.vibrato_amplitude_data)?;
    write_points(out, &params.vibrato_frequency_range)?;
    write_points(out, &params.vibrato_frequency_data)?;
    Ok(())
}

fn write_note(out: &mut Vec<u8>, note: &MutaNote) -> Result<()> {
    push_i32(out, note.start);
    push_u32(out, note.length);
    push_u32(out, note.key);
    push_u16_array(out, &note.lyric, 8);
    push_padded_utf16(out, &note.phoneme, 16)?;
    for timing in &note.tmg_data {
        push_i32(out, timing.ori_pos);
        push_i32(out, timing.mod_pos);
    }
    Ok(())
}

fn write_track_data(out: &mut Vec<u8>, track: &MutaTrack) -> Result<()> {
    match &track.data {
        MutaTrackData::Song(items) => {
            for item in items {
                out.push(b'\n');
                push_u32(out, item.start);
                push_u32(out, item.length);
                push_u16_array(out, &item.singer_name, 258);
                push_u32(out, item.speaker_id);
                write_prefixed_array(out, &item.notes, |payload, note| write_note(payload, note))?;
                write_params(out, &item.params)?;
            }
        }
        MutaTrackData::Audio(items) => {
            for item in items {
                out.push(b'\n');
                push_u32(out, item.start);
                push_u32(out, item.length);
                push_pascal_string(out, &item.file_path);
            }
        }
        MutaTrackData::Talk(items) => {
            for item in items {
                out.push(b'\n');
                push_u32(out, item.start);
                push_u32(out, item.length);
                push_u16_array(out, &item.talker_name, 258);
                push_u32(out, item.speaker_id);
                out.push(b'\n');
                push_byte_string(out, &item.text.text);
                out.push(b'\n');
                for byte in [
                    item.text.volume,
                    item.text.speed,
                    item.text.tone,
                    item.text.quality,
                    item.text.normal,
                    item.text.happiness,
                    item.text.sadness,
                    item.text.reserved,
                ] {
                    out.push(byte);
                }
                write_prefixed_array(out, &item.text.phonemes, |payload, phoneme| {
                    payload.push(phoneme.volume);
                    push_u16(payload, phoneme.pos1);
                    payload.push(phoneme.length);
                    push_u16(payload, phoneme.pos2);
                    push_u32(payload, phoneme.pitch);
                    push_byte_string(payload, &phoneme.phoneme);
                    Ok(())
                })?;
            }
        }
    }
    Ok(())
}

fn write_track(out: &mut Vec<u8>, track: &MutaTrack) -> Result<()> {
    push_u32(out, track.track_type);
    push_u32(out, track.seq_count);
    out.push(b'\n');
    push_u32(out, track.mute);
    push_u32(out, track.solo);
    push_u32(out, track.volume);
    push_u32(out, track.pan);
    push_padded_utf16(out, &track.name, 12)?;
    out.extend_from_slice(&track.padding);
    write_track_data(out, track)
}

/// `muta_project_struct.build`.
pub fn build_project(project: &MutaProject) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    push_u32(&mut out, project.file_version);
    write_prefixed_array(&mut out, &project.tempos, |payload, tempo| {
        push_i32(payload, tempo.position);
        push_u32(payload, tempo.bpm);
        Ok(())
    })?;
    write_prefixed_array(
        &mut out,
        &project.time_signatures,
        |payload, signature| {
            push_i32(payload, signature.measure_position);
            push_u32(payload, signature.numerator);
            push_u32(payload, signature.denominator);
            Ok(())
        },
    )?;
    write_prefixed_array(&mut out, &project.tracks, |payload, track| {
        write_track(payload, track)
    })?;
    out.push(b'\n');
    Ok(out)
}

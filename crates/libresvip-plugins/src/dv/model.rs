//! DeepVocal project binary model 鈥?port of `libresvip.plugins.dv.model`.
//!
//! The Python model is a `construct` / `construct_typed` tree; the byte layout is
//! reproduced explicitly here.

use libresvip_core::error::{LibreSvipError, Result};

#[derive(Clone, Debug, PartialEq)]
pub enum DvTrackData {
    Singing(DvSingingTrack),
    Audio(DvAudioTrack),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DvPoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DvTempo {
    pub position: i32,
    pub bpm: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DvTimeSignature {
    pub measure_position: i32,
    pub numerator: u32,
    pub denominator: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DvNoteParameter {
    pub amplitude_points: Vec<DvPoint>,
    pub frequency_points: Vec<DvPoint>,
    pub vibrato_points: Vec<DvPoint>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DvPhoneme {
    pub unknown_1: i8,
    pub consonant_rate: f32,
    pub vowel_modified: i8,
    pub medial: f32,
    pub rime: f32,
    pub ending: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DvNote {
    pub start: i32,
    pub length: u32,
    pub key: u32,
    pub vibrato: u32,
    pub phoneme: String,
    pub word: String,
    pub padding_1: u8,
    pub note_vibrato_data: DvNoteParameter,
    pub unknown: Vec<f32>,
    pub phonemes: Option<DvPhoneme>,
    pub ben_depth: Option<u32>,
    pub ben_length: Option<u32>,
    pub por_tail: Option<u32>,
    pub por_head: Option<u32>,
    pub timbre: Option<i32>,
    pub cross_lyric: Option<String>,
    pub cross_timbre: Option<i32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DvSegment {
    pub start: u32,
    pub length: u32,
    pub name: String,
    pub singer_name: String,
    pub notes: Vec<DvNote>,
    pub volume_data: Vec<DvPoint>,
    pub pitch_data: Vec<DvPoint>,
    pub breath_data: Vec<DvPoint>,
    pub ext3_data: Option<Vec<DvPoint>>,
    pub ext5_data: Option<Vec<DvPoint>>,
    pub ext6_data: Option<Vec<DvPoint>>,
    pub ext7_data: Option<Vec<DvPoint>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DvSingingTrack {
    pub name: String,
    pub mute: u8,
    pub solo: u8,
    pub volume: u32,
    pub balance: u32,
    pub segments: Vec<DvSegment>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DvAudioInfo {
    pub start: u32,
    pub length: u32,
    pub name: String,
    pub path: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DvAudioTrack {
    pub name: String,
    pub mute: u8,
    pub solo: u8,
    pub volume: u32,
    pub balance: u32,
    pub infos: Vec<DvAudioInfo>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DvInnerProject {
    pub features: [bool; 7],
    pub tempos: Vec<DvTempo>,
    pub time_signatures: Vec<DvTimeSignature>,
    pub tracks: Vec<DvTrackRecord>,
}

/// A `DvTrack` together with its (boxed) payload.
#[derive(Clone, Debug, PartialEq)]
pub struct DvTrackRecord {
    pub track_type: u32,
    pub track_data: DvTrackData,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DvProject {
    pub version: u32,
    pub inner_project: DvInnerProject,
}

impl DvTrackRecord {
    pub fn singing(track: DvSingingTrack) -> Self {
        Self {
            track_type: 0,
            track_data: DvTrackData::Singing(track),
        }
    }

    pub fn audio(track: DvAudioTrack) -> Self {
        Self {
            track_type: 1,
            track_data: DvTrackData::Audio(track),
        }
    }

    pub fn as_singing(&self) -> Option<&DvSingingTrack> {
        match &self.track_data {
            DvTrackData::Singing(track) => Some(track),
            DvTrackData::Audio(_) => None,
        }
    }

    pub fn as_audio(&self) -> Option<&DvAudioTrack> {
        match &self.track_data {
            DvTrackData::Audio(track) => Some(track),
            DvTrackData::Singing(_) => None,
        }
    }
}

const FEATURE_NAMES: [&[u8; 4]; 7] = [b"ext1", b"ext2", b"ext3", b"ext4", b"ext5", b"ext6", b"ext7"];

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

    fn remaining(&self) -> &'a [u8] {
        &self.data[self.position..]
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn i8(&mut self) -> Result<i8> {
        Ok(self.u8()? as i8)
    }

    fn u32(&mut self) -> Result<u32> {
        let bytes: [u8; 4] = self.take(4)?.try_into().map_err(|_| parse_error("short u32"))?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn i32(&mut self) -> Result<i32> {
        let bytes: [u8; 4] = self.take(4)?.try_into().map_err(|_| parse_error("short i32"))?;
        Ok(i32::from_le_bytes(bytes))
    }

    fn f32(&mut self) -> Result<f32> {
        let bytes: [u8; 4] = self.take(4)?.try_into().map_err(|_| parse_error("short f32"))?;
        Ok(f32::from_le_bytes(bytes))
    }

    fn string(&mut self) -> Result<String> {
        let length = self.u32()? as usize;
        let bytes = self.take(length)?;
        String::from_utf8(bytes.to_vec())
            .map_err(|error| parse_error(format!("invalid utf-8 string: {error}")))
    }

    /// `Prefixed(Int32ul, subcon)`: read a byte length, parse `subcon` over it.
    fn prefixed<T>(&mut self, parse: impl FnOnce(&mut Reader<'a>) -> Result<T>) -> Result<T> {
        let length = self.u32()? as usize;
        let bytes = self.take(length)?;
        let mut sub = Reader::new(bytes);
        let value = parse(&mut sub)?;
        Ok(value)
    }

    fn point(&mut self) -> Result<DvPoint> {
        Ok(DvPoint {
            x: self.i32()?,
            y: self.i32()?,
        })
    }

    fn param(&mut self) -> Result<Vec<DvPoint>> {
        self.prefixed(|sub| {
            let count = sub.u32()? as usize;
            let mut points = Vec::new();
            for _ in 0..count {
                points.push(sub.point()?);
            }
            Ok(points)
        })
    }

    fn note(&mut self, features: &[bool; 7]) -> Result<DvNote> {
        let start = self.i32()?;
        let length = self.u32()?;
        let key = self.u32()?;
        let vibrato = self.u32()?;
        let phoneme = self.string()?;
        let word = self.string()?;
        let padding_1 = self.u8()?;
        let note_vibrato_data = self.prefixed(|sub| {
            Ok(DvNoteParameter {
                amplitude_points: sub.param()?,
                frequency_points: sub.param()?,
                vibrato_points: sub.param()?,
            })
        })?;
        let unknown = self.prefixed(|sub| {
            let count = sub.u32()? as usize;
            let mut values = Vec::new();
            for _ in 0..count {
                values.push(sub.f32()?);
            }
            Ok(values)
        })?;
        let phonemes = if features[0] {
            Some(DvPhoneme {
                unknown_1: self.i8()?,
                consonant_rate: self.f32()?,
                vowel_modified: self.i8()?,
                medial: self.f32()?,
                rime: self.f32()?,
                ending: self.f32()?,
            })
        } else {
            None
        };
        let (ben_depth, ben_length, por_tail, por_head) = if features[1] {
            (
                Some(self.u32()?),
                Some(self.u32()?),
                Some(self.u32()?),
                Some(self.u32()?),
            )
        } else {
            (None, None, None, None)
        };
        let timbre = if features[3] { Some(self.i32()?) } else { None };
        let (cross_lyric, cross_timbre) = if features[6] {
            (Some(self.string()?), Some(self.i32()?))
        } else {
            (None, None)
        };
        Ok(DvNote {
            start,
            length,
            key,
            vibrato,
            phoneme,
            word,
            padding_1,
            note_vibrato_data,
            unknown,
            phonemes,
            ben_depth,
            ben_length,
            por_tail,
            por_head,
            timbre,
            cross_lyric,
            cross_timbre,
        })
    }

    fn segment(&mut self, features: &[bool; 7]) -> Result<DvSegment> {
        let start = self.u32()?;
        let length = self.u32()?;
        let name = self.string()?;
        let singer_name = self.string()?;
        let notes = self.prefixed(|sub| {
            let count = sub.u32()? as usize;
            let mut notes = Vec::new();
            for _ in 0..count {
                notes.push(sub.note(features)?);
            }
            Ok(notes)
        })?;
        let volume_data = self.param()?;
        let pitch_data = self.param()?;
        let breath_data = self.param()?;
        let ext3_data = if features[2] {
            Some(self.param()?)
        } else {
            None
        };
        let ext5_data = if features[4] {
            Some(self.param()?)
        } else {
            None
        };
        let ext6_data = if features[5] {
            Some(self.param()?)
        } else {
            None
        };
        let ext7_data = if features[6] {
            Some(self.param()?)
        } else {
            None
        };
        Ok(DvSegment {
            start,
            length,
            name,
            singer_name,
            notes,
            volume_data,
            pitch_data,
            breath_data,
            ext3_data,
            ext5_data,
            ext6_data,
            ext7_data,
        })
    }

    fn singing_track(&mut self, features: &[bool; 7]) -> Result<DvSingingTrack> {
        let name = self.string()?;
        let mute = self.u8()?;
        let solo = self.u8()?;
        let volume = self.u32()?;
        let balance = self.u32()?;
        let segments = self.prefixed(|sub| {
            let count = sub.u32()? as usize;
            let mut segments = Vec::new();
            for _ in 0..count {
                segments.push(sub.segment(features)?);
            }
            Ok(segments)
        })?;
        Ok(DvSingingTrack {
            name,
            mute,
            solo,
            volume,
            balance,
            segments,
        })
    }

    fn audio_track(&mut self) -> Result<DvAudioTrack> {
        let name = self.string()?;
        let mute = self.u8()?;
        let solo = self.u8()?;
        let volume = self.u32()?;
        let balance = self.u32()?;
        let infos = self.prefixed(|sub| {
            let count = sub.u32()? as usize;
            let mut infos = Vec::new();
            for _ in 0..count {
                infos.push(DvAudioInfo {
                    start: sub.u32()?,
                    length: sub.u32()?,
                    name: sub.string()?,
                    path: sub.string()?,
                });
            }
            Ok(infos)
        })?;
        Ok(DvAudioTrack {
            name,
            mute,
            solo,
            volume,
            balance,
            infos,
        })
    }
}

/// `dv_project_struct.parse`.
pub fn parse_project(data: &[u8]) -> Result<DvProject> {
    let mut reader = Reader::new(data);
    let magic = reader.take(8)?;
    if magic != b"SHARPKEY".as_slice() {
        return Err(parse_error("invalid DeepVocal file magic"));
    }
    let version = reader.u32()?;
    let inner_project = reader.prefixed(|sub| {
        let mut features = [false; 7];
        for (index, name) in FEATURE_NAMES.iter().enumerate() {
            if sub.remaining().starts_with(&name[..]) {
                sub.take(4)?;
                features[index] = true;
            }
        }
        let tempos = sub.prefixed(|inner| {
            let count = inner.u32()? as usize;
            let mut tempos = Vec::new();
            for _ in 0..count {
                tempos.push(DvTempo {
                    position: inner.i32()?,
                    bpm: inner.u32()?,
                });
            }
            Ok(tempos)
        })?;
        let time_signatures = sub.prefixed(|inner| {
            let count = inner.u32()? as usize;
            let mut signatures = Vec::new();
            for _ in 0..count {
                signatures.push(DvTimeSignature {
                    measure_position: inner.i32()?,
                    numerator: inner.u32()?,
                    denominator: inner.u32()?,
                });
            }
            Ok(signatures)
        })?;
        let track_count = sub.u32()? as usize;
        let mut tracks = Vec::new();
        for _ in 0..track_count {
            let track_type = sub.u32()?;
            let track_data = match track_type {
                0 => DvTrackData::Singing(sub.singing_track(&features)?),
                1 => DvTrackData::Audio(sub.audio_track()?),
                other => {
                    return Err(parse_error(format!("unknown DeepVocal track type {other}")));
                }
            };
            tracks.push(DvTrackRecord {
                track_type,
                track_data,
            });
        }
        Ok(DvInnerProject {
            features,
            tempos,
            time_signatures,
            tracks,
        })
    })?;
    Ok(DvProject {
        version,
        inner_project,
    })
}

// ---------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_string(out: &mut Vec<u8>, value: &str) {
    push_u32(out, value.len() as u32);
    out.extend_from_slice(value.as_bytes());
}

fn push_prefixed(out: &mut Vec<u8>, payload: &[u8]) {
    push_u32(out, payload.len() as u32);
    out.extend_from_slice(payload);
}

fn write_point(out: &mut Vec<u8>, point: &DvPoint) {
    push_i32(out, point.x);
    push_i32(out, point.y);
}

/// `DvParam`: `Prefixed(Int32ul, PrefixedArray(Int32ul, DataclassStruct(DvPoint)))`.
fn write_param(out: &mut Vec<u8>, points: &[DvPoint]) {
    let mut payload = Vec::with_capacity(4 + points.len() * 8);
    push_u32(&mut payload, points.len() as u32);
    for point in points {
        write_point(&mut payload, point);
    }
    push_prefixed(out, &payload);
}

/// `Prefixed(Int32ul, PrefixedArray(Int32ul, T))` for structs.
fn write_prefixed_array<T>(out: &mut Vec<u8>, items: &[T], write: impl Fn(&mut Vec<u8>, &T)) {
    let mut payload = Vec::new();
    push_u32(&mut payload, items.len() as u32);
    for item in items {
        write(&mut payload, item);
    }
    push_prefixed(out, &payload);
}

fn write_note_parameter(out: &mut Vec<u8>, parameter: &DvNoteParameter) {
    let mut payload = Vec::new();
    write_param(&mut payload, &parameter.amplitude_points);
    write_param(&mut payload, &parameter.frequency_points);
    write_param(&mut payload, &parameter.vibrato_points);
    push_prefixed(out, &payload);
}

fn write_note(out: &mut Vec<u8>, note: &DvNote, features: &[bool; 7]) {
    push_i32(out, note.start);
    push_u32(out, note.length);
    push_u32(out, note.key);
    push_u32(out, note.vibrato);
    push_string(out, &note.phoneme);
    push_string(out, &note.word);
    out.push(note.padding_1);
    write_note_parameter(out, &note.note_vibrato_data);
    let mut unknown = Vec::with_capacity(4 + note.unknown.len() * 4);
    push_u32(&mut unknown, note.unknown.len() as u32);
    for value in &note.unknown {
        push_f32(&mut unknown, *value);
    }
    push_prefixed(out, &unknown);
    if features[0] {
        let phoneme = note.phonemes.unwrap_or_default();
        out.push(phoneme.unknown_1 as u8);
        push_f32(out, phoneme.consonant_rate);
        out.push(phoneme.vowel_modified as u8);
        push_f32(out, phoneme.medial);
        push_f32(out, phoneme.rime);
        push_f32(out, phoneme.ending);
    }
    if features[1] {
        push_u32(out, note.ben_depth.unwrap_or(0));
        push_u32(out, note.ben_length.unwrap_or(0));
        push_u32(out, note.por_tail.unwrap_or(0));
        push_u32(out, note.por_head.unwrap_or(0));
    }
    if features[3] {
        push_i32(out, note.timbre.unwrap_or(0));
    }
    if features[6] {
        push_string(out, note.cross_lyric.as_deref().unwrap_or(""));
        push_i32(out, note.cross_timbre.unwrap_or(0));
    }
}

fn write_segment(out: &mut Vec<u8>, segment: &DvSegment, features: &[bool; 7]) {
    push_u32(out, segment.start);
    push_u32(out, segment.length);
    push_string(out, &segment.name);
    push_string(out, &segment.singer_name);
    write_prefixed_array(out, &segment.notes, |payload, note| {
        write_note(payload, note, features);
    });
    write_param(out, &segment.volume_data);
    write_param(out, &segment.pitch_data);
    write_param(out, &segment.breath_data);
    if features[2] {
        write_param(out, segment.ext3_data.as_deref().unwrap_or_default());
    }
    if features[4] {
        write_param(out, segment.ext5_data.as_deref().unwrap_or_default());
    }
    if features[5] {
        write_param(out, segment.ext6_data.as_deref().unwrap_or_default());
    }
    if features[6] {
        write_param(out, segment.ext7_data.as_deref().unwrap_or_default());
    }
}

fn write_singing_track(out: &mut Vec<u8>, track: &DvSingingTrack, features: &[bool; 7]) {
    push_string(out, &track.name);
    out.push(track.mute);
    out.push(track.solo);
    push_u32(out, track.volume);
    push_u32(out, track.balance);
    write_prefixed_array(out, &track.segments, |payload, segment| {
        write_segment(payload, segment, features);
    });
}

fn write_audio_track(out: &mut Vec<u8>, track: &DvAudioTrack) {
    push_string(out, &track.name);
    out.push(track.mute);
    out.push(track.solo);
    push_u32(out, track.volume);
    push_u32(out, track.balance);
    write_prefixed_array(out, &track.infos, |payload, info| {
        push_u32(payload, info.start);
        push_u32(payload, info.length);
        push_string(payload, &info.name);
        push_string(payload, &info.path);
    });
}

/// `dv_project_struct.build`.
pub fn build_project(project: &DvProject) -> Vec<u8> {
    let features = project.inner_project.features;
    let mut inner = Vec::new();
    for (index, name) in FEATURE_NAMES.iter().enumerate() {
        if features[index] {
            inner.extend_from_slice(&name[..]);
        }
    }
    write_prefixed_array(&mut inner, &project.inner_project.tempos, |payload, tempo| {
        push_i32(payload, tempo.position);
        push_u32(payload, tempo.bpm);
    });
    write_prefixed_array(
        &mut inner,
        &project.inner_project.time_signatures,
        |payload, signature| {
            push_i32(payload, signature.measure_position);
            push_u32(payload, signature.numerator);
            push_u32(payload, signature.denominator);
        },
    );
    push_u32(&mut inner, project.inner_project.tracks.len() as u32);
    for track in &project.inner_project.tracks {
        push_u32(&mut inner, track.track_type);
        match &track.track_data {
            DvTrackData::Singing(singing) => write_singing_track(&mut inner, singing, &features),
            DvTrackData::Audio(audio) => write_audio_track(&mut inner, audio),
        }
    }

    let mut out = Vec::new();
    out.extend_from_slice(b"SHARPKEY");
    push_u32(&mut out, project.version);
    push_prefixed(&mut out, &inner);
    out
}

//! Port of the legacy binary `PPSF` container (`legacy_model.py` +
//! `piapro_studio_legacy_parser.py`).
//!
//! The construct model validates every chunk; this reader fully decodes the
//! four chunks the hub model actually derives from (`CLPS`, `EVTS`, `PLGS`,
//! `TRKS`) and skips the rest by their declared size. Values are the same
//! little-endian primitives as the construct model; lengths that the model
//! prefixes with [`PpsfVarInt`] use the game's MSB-first 7-bit groups.

use std::collections::HashMap;

use libresvip_core::core::compat::py_round;
use libresvip_core::core::constants::{DEFAULT_BPM, TICKS_IN_BEAT};
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{
    Note, ParamCurve, Project, SingingTrack, SongTempo, TimeSignature,
};

use crate::vpr::controllers::{
    ControllerCurve, ControllerEvent, DEFAULT_PITCH_BEND_SENSITIVITY, MAX_PITCH_BEND_SENSITIVITY,
    PITCH_MAX_VALUE,
};
use crate::vpr::pitch::{PitchBendData, VocaloidPitchHandler};

pub const V3_PARAM_PIT_INDEX: usize = 6;
pub const V3_PARAM_PBS_INDEX: usize = 7;

/// `PpsfVarInt` — up to 5 bytes, high bit set means "continue", groups are
/// shifted in MSB-first.
pub fn write_varint(out: &mut Vec<u8>, value: u64) {
    if value <= 0x7F {
        out.push(value as u8);
    } else if value <= 0x3FFF {
        out.push(0x80 | (value >> 7) as u8);
        out.push((value & 0x7F) as u8);
    } else if value <= 0x1FFFFF {
        out.push(0x80 | (value >> 14) as u8);
        out.push(0x80 | ((value >> 7) & 0x7F) as u8);
        out.push((value & 0x7F) as u8);
    } else if value <= 0xFFFFFFF {
        out.push(0x80 | (value >> 21) as u8);
        out.push(0x80 | ((value >> 14) & 0x7F) as u8);
        out.push(0x80 | ((value >> 7) & 0x7F) as u8);
        out.push((value & 0x7F) as u8);
    } else {
        out.push(0x80 | (value >> 28) as u8);
        out.push(0x80 | ((value >> 21) & 0x7F) as u8);
        out.push(0x80 | ((value >> 14) & 0x7F) as u8);
        out.push(0x80 | ((value >> 7) & 0x7F) as u8);
        out.push((value & 0x7F) as u8);
    }
}

pub(crate) struct LegacyReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> LegacyReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn is_empty(&self) -> bool {
        self.pos >= self.data.len()
    }

    pub fn u8(&mut self) -> Result<u8> {
        self.bytes(1).map(|bytes| bytes[0])
    }

    pub fn u16(&mut self) -> Result<u16> {
        self.bytes(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn u32(&mut self) -> Result<u32> {
        self.bytes(4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn bytes(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(count)
            .filter(|&end| end <= self.data.len())
            .ok_or_else(|| {
                LibreSvipError::msg(format!(
                    "unexpected end of legacy ppsf data: need {count} bytes at offset {}",
                    self.pos
                ))
            })?;
        let slice = &self.data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    pub fn expect_bytes(&mut self, expected: &[u8]) -> Result<()> {
        let actual = self.bytes(expected.len())?;
        if actual != expected {
            return Err(LibreSvipError::msg(format!(
                "legacy ppsf magic mismatch: expected {expected:?}, found {actual:?}"
            )));
        }
        Ok(())
    }

    pub fn varint(&mut self) -> Result<u64> {
        let mut result: u64 = 0;
        for _ in 0..5 {
            let byte = self.u8()?;
            result = (result << 7) | u64::from(byte & 0x7F);
            if byte & 0x80 == 0 {
                return Ok(result);
            }
        }
        Err(LibreSvipError::msg("legacy ppsf varint exceeds 5 bytes"))
    }

    /// `PpsfVarIntString` — varint byte length + UTF-8.
    pub fn varint_string(&mut self) -> Result<String> {
        let len = self.varint()? as usize;
        let bytes = self.bytes(len)?;
        String::from_utf8(bytes.to_vec())
            .map_err(|error| LibreSvipError::msg(format!("invalid utf-8 string: {error}")))
    }

    /// `PascalString(Byte, "utf-8")` — the generator writes it, but no
    /// parser path needs to read one.
    #[allow(dead_code)]
    pub fn pstring8(&mut self) -> Result<String> {
        let len = self.u8()? as usize;
        let bytes = self.bytes(len)?;
        String::from_utf8(bytes.to_vec())
            .map_err(|error| LibreSvipError::msg(format!("invalid utf-8 string: {error}")))
    }

    pub fn varint_count(&mut self) -> Result<usize> {
        self.varint().map(|value| value as usize)
    }

    pub fn u32_varint_array(&mut self) -> Result<Vec<u32>> {
        let count = self.varint_count()?;
        let mut result = Vec::with_capacity(count.min(1024));
        for _ in 0..count {
            result.push(self.u32()?);
        }
        Ok(result)
    }

    pub fn u16_varint_array(&mut self) -> Result<Vec<u16>> {
        let count = self.varint_count()?;
        let mut result = Vec::with_capacity(count.min(1024));
        for _ in 0..count {
            result.push(self.u16()?);
        }
        Ok(result)
    }

    /// `Prefixed(PpsfVarIntCount, GreedyBytes)`.
    pub fn varint_prefixed(&mut self) -> Result<&'a [u8]> {
        let len = self.varint()? as usize;
        self.bytes(len)
    }
}

#[derive(Debug)]
pub(crate) enum LegacyEventData {
    Automation { tick: u32, value: u32 },
    V3Note { tick: u32, note: u8, duration: u32, lyric: String, phoneme: String },
    Other,
}

#[derive(Debug)]
pub(crate) struct LegacyEvent {
    pub data: LegacyEventData,
}

pub(crate) struct NoteClipRaw {
    pub offset: u32,
    pub event_indices: Vec<u32>,
}

pub(crate) struct AutomationClipRaw {
    pub index: u16,
    pub field_4: u16,
    pub event_indices: Vec<u32>,
    pub sub_event_indices: Vec<Vec<u32>>,
}

pub(crate) struct V3EventPluginRaw {
    pub index: u16,
    pub name: String,
    pub self_clip_idx: u16,
}

pub(crate) struct MusicParamPluginRaw {
    pub name: String,
    pub self_clip_idx: u16,
}

pub(crate) struct V3TrackRaw {
    pub v3_event_plugin_indices: Vec<u16>,
    pub clip_indices: Vec<u16>,
}

/// `_parse_version`.
fn parse_version(version_string: &str) -> [u64; 6] {
    let mut tuple = [0u64; 6];
    for (index, part) in version_string.split('.').enumerate().take(3) {
        tuple[index] = part.parse::<u64>().unwrap_or(0);
    }
    tuple
}

/// `_is_new_version`.
fn is_new_version(version: &[u64; 6], minor: u64, revision: u64) -> bool {
    version[0] > 0 || version[1] > minor || (version[1] == minor && version[2] > revision)
}

fn bad_magic(expected: &[u8; 4], actual: &[u8]) -> LibreSvipError {
    LibreSvipError::msg(format!(
        "legacy ppsf magic mismatch: expected {expected:?}, found {actual:?}"
    ))
}

/// One `PpsfClip` header shared by every clip array: magic + size + index,
/// then `size - 2` bytes of payload.
struct ClipHeader {
    magic: [u8; 4],
    payload: usize,
    index: u16,
}

fn read_clip_header(reader: &mut LegacyReader) -> Result<ClipHeader> {
    let magic: [u8; 4] = reader.bytes(4)?.try_into().unwrap();
    let size = reader.u32()?;
    let index = reader.u16()?;
    let payload = usize::try_from(size)
        .unwrap_or(usize::MAX)
        .checked_sub(2)
        .ok_or_else(|| LibreSvipError::msg("legacy ppsf clip size below 2"))?;
    Ok(ClipHeader {
        magic,
        payload,
        index,
    })
}

fn parse_v3_note_clip(reader: &mut LegacyReader, new_version: bool) -> Result<NoteClipRaw> {
    // PpsfNoteClipBase: iclip then the clip anchor fields.
    let _field_4 = reader.u16()?;
    let _field_6 = reader.u8()?;
    let event_indices = reader.u32_varint_array()?;
    let _name = reader.varint_string()?;
    let _flag = reader.u8()?;
    if new_version {
        reader.varint_prefixed()?; // iclip.extension
    }
    let offset = reader.u32()?;
    let _field_108 = reader.u32()?;
    let _song_end_anchor_a = reader.u32()?;
    let _field_116 = reader.u32()?;
    let _song_end_anchor_b = reader.u32()?;
    let _field_124 = reader.u8()?;
    let _field_128 = reader.u32()?;
    let _field_132 = reader.u32()?;
    Ok(NoteClipRaw {
        offset,
        event_indices,
    })
}

fn parse_automation_clip(reader: &mut LegacyReader) -> Result<AutomationClipRaw> {
    let field_4 = reader.u16()?;
    let _field_6 = reader.u8()?;
    let event_indices = reader.u32_varint_array()?;
    let sub_count = (field_4 as usize).checked_sub(1).ok_or_else(|| {
        LibreSvipError::msg("legacy ppsf automation clip field_4 below 1")
    })?;
    let mut sub_event_indices = Vec::with_capacity(sub_count);
    for _ in 0..sub_count {
        sub_event_indices.push(reader.u32_varint_array()?);
    }
    Ok(AutomationClipRaw {
        index: 0,
        field_4,
        event_indices,
        sub_event_indices,
    })
}

fn parse_vibrato_handle(reader: &mut LegacyReader) -> Result<()> {
    let _handle_id = reader.u16()?;
    reader.varint_prefixed()?; // depth_data
    let _flag = reader.u8()?;
    reader.varint_prefixed()?; // rate_data
    let _start_position = reader.u16()?;
    for _ in 0..2 {
        let count = reader.varint_count()?;
        for _ in 0..count {
            reader.u32()?; // value
            reader.u16()?; // position
        }
    }
    Ok(())
}

fn parse_event_payload(magic: u8, reader: &mut LegacyReader) -> Result<LegacyEventData> {
    let data = match magic {
        0x00 => {
            let tick = reader.u32()?;
            let value = reader.u32()?;
            LegacyEventData::Automation { tick, value }
        }
        0x08 => {
            let tick = reader.u32()?;
            let note = reader.u8()?;
            let duration = reader.u32()?;
            let _velocity = reader.u8()?;
            reader.bytes(6)?; // byte_25..byte_30
            let lyric = reader.varint_string()?;
            let _protected = reader.u8()?;
            let phoneme = reader.varint_string()?;
            parse_vibrato_handle(reader)?;
            let _trailing = reader.u16()?;
            LegacyEventData::V3Note {
                tick,
                note,
                duration,
                lyric,
                phoneme,
            }
        }
        _ => LegacyEventData::Other,
    };
    Ok(data)
}

/// Parses one whole legacy `PPSF` file, returning the pieces
/// `PiaproStudioLegacyParser.parse_project` consumes.
pub(crate) struct PpsfLegacyFile {
    pub note_clips: Vec<NoteClipRaw>,
    pub automation_clips: Vec<AutomationClipRaw>,
    pub events: Vec<LegacyEvent>,
    pub music_param_plugins: Vec<MusicParamPluginRaw>,
    pub v3_event_plugins: Vec<V3EventPluginRaw>,
    pub v3_tracks: Vec<V3TrackRaw>,
}

pub(crate) fn parse_legacy_file(content: &[u8]) -> Result<PpsfLegacyFile> {
    let mut reader = LegacyReader::new(content);
    reader.expect_bytes(b"PPSF")?;
    let total = reader.u32()? as usize;
    let mut body = LegacyReader::new(reader.bytes(total)?);

    let version_length = body.u16()? as usize;
    let version_string = String::from_utf8(body.bytes(version_length)?.to_vec())
        .map_err(|error| LibreSvipError::msg(format!("invalid version string: {error}")))?;
    let version = parse_version(&version_string);
    let new_version = is_new_version(&version, 9, 3);

    let mut file = PpsfLegacyFile {
        note_clips: Vec::new(),
        automation_clips: Vec::new(),
        events: Vec::new(),
        music_param_plugins: Vec::new(),
        v3_event_plugins: Vec::new(),
        v3_tracks: Vec::new(),
    };

    while !body.is_empty() {
        let magic: [u8; 4] = body.bytes(4)?.try_into().unwrap();
        let size = body.u32()? as usize;
        let mut chunk = LegacyReader::new(body.bytes(size)?);
        match &magic {
            b"CLPS" => parse_clips_chunk(&mut chunk, new_version, &mut file)?,
            b"EVTS" => file.events = parse_events_chunk(&mut chunk)?,
            b"PLGS" => parse_plugins_chunk(&mut chunk, &mut file)?,
            b"TRKS" => parse_tracks_chunk(&mut chunk, &mut file)?,
            _ => {}
        }
    }
    Ok(file)
}

/// `PpsfClips`: seven arrays of `PpsfClip`, of which the Vocaloid3 note clips
/// and the automation clips matter to the hub model.
fn parse_clips_chunk(
    reader: &mut LegacyReader,
    new_version: bool,
    file: &mut PpsfLegacyFile,
) -> Result<()> {
    // audio_clips, midi_note_clips, midi_sysex_clips, midi_meta_clips,
    // vsq_note_clips.
    for _ in 0..5 {
        let count = reader.varint_count()?;
        for _ in 0..count {
            let header = read_clip_header(reader)?;
            reader.bytes(header.payload)?;
        }
    }
    let count = reader.varint_count()?;
    file.note_clips = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        let header = read_clip_header(reader)?;
        if header.magic != *b"V3CL" {
            return Err(bad_magic(b"V3CL", &header.magic));
        }
        let mut payload = LegacyReader::new(reader.bytes(header.payload)?);
        file.note_clips.push(parse_v3_note_clip(&mut payload, new_version)?);
    }
    let count = reader.varint_count()?;
    file.automation_clips = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        let header = read_clip_header(reader)?;
        if header.magic != *b"AMCL" {
            return Err(bad_magic(b"AMCL", &header.magic));
        }
        let mut payload = LegacyReader::new(reader.bytes(header.payload)?);
        let mut clip = parse_automation_clip(&mut payload)?;
        clip.index = header.index;
        file.automation_clips.push(clip);
    }
    Ok(())
}

fn parse_events_chunk(reader: &mut LegacyReader) -> Result<Vec<LegacyEvent>> {
    let count = reader.varint_count()?;
    let mut events = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        let magic = reader.u8()?;
        match magic {
            0x00 | 0x01 | 0x03 | 0x04 | 0x05 | 0x08 | 0x0b => {}
            other => {
                return Err(LibreSvipError::msg(format!(
                    "unknown legacy ppsf event tag 0x{other:02x}"
                )))
            }
        }
        let size = reader.u16()? as usize;
        let mut payload = LegacyReader::new(reader.bytes(size)?);
        let data = parse_event_payload(magic, &mut payload)?;
        events.push(LegacyEvent { data });
    }
    Ok(events)
}

/// `PpsfEventControlPluginBody`.
fn parse_event_control_plugin_body(reader: &mut LegacyReader) -> Result<(String, u16)> {
    let name = reader.varint_string()?;
    reader.bytes(10)?; // reserved
    let _flag = reader.u16()?;
    let param_count = reader.u8()? as usize;
    for _ in 0..param_count {
        reader.u32()?;
        reader.u32()?;
    }
    let self_clip_idx = reader.u16()?;
    // vfxc_blob: varint-prefixed { magic "vfxc", u32 length, data }.
    let mut blob = reader.varint_prefixed_slice()?;
    blob.expect_bytes(b"vfxc")?;
    let data_len = blob.u32()? as usize;
    blob.bytes(data_len)?;
    Ok((name, self_clip_idx))
}

fn parse_plugins_chunk(reader: &mut LegacyReader, file: &mut PpsfLegacyFile) -> Result<()> {
    // audio, midi, vsq, vocaloid3, synth, vocaloid3 synth, music param,
    // then the three vst lists — only the event-control bodies matter here.
    for _ in 0..10 {
        let count = reader.varint_count()?;
        for _ in 0..count {
            let magic: [u8; 4] = reader.bytes(4)?.try_into().unwrap();
            let size = reader.u32()?;
            let index = reader.u16()?;
            let payload = usize::try_from(size)
                .unwrap_or(usize::MAX)
                .checked_sub(2)
                .ok_or_else(|| LibreSvipError::msg("legacy ppsf plugin size below 2"))?;
            let mut body = LegacyReader::new(reader.bytes(payload)?);
            match &magic {
                b"EAPG" | b"EMPG" | b"EVPG" | b"E3PG" | b"EMPP" => {
                    let (name, self_clip_idx) = parse_event_control_plugin_body(&mut body)?;
                    if magic == *b"EMPP" {
                        file.music_param_plugins.push(MusicParamPluginRaw {
                            name,
                            self_clip_idx,
                        });
                    } else if magic == *b"E3PG" {
                        file.v3_event_plugins.push(V3EventPluginRaw {
                            index,
                            name,
                            self_clip_idx,
                        });
                    }
                }
                b"ESPG" | b"V3CP" => {
                    let _name = body.varint_string()?;
                    let _required_tag = body.varint_string()?;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

/// `PpsfTrackBase`.
fn parse_track_base(reader: &mut LegacyReader) -> Result<Vec<u16>> {
    let _field_0 = reader.u16()?;
    let _flag_2 = reader.u8()?;
    let v3_event_plugin_indices = reader.u16_varint_array()?;
    let _audio_event_plugin_indices = reader.u16_varint_array()?;
    let _field_180 = reader.u16()?;
    let _synth_plugin_index = reader.u16()?;
    let _field_184 = reader.u32()?;
    let _byte_188 = reader.u8()?;
    let _byte_189 = reader.u8()?;
    let _byte_190 = reader.u8()?;
    let _source_track_indices = reader.u16_varint_array()?;
    let _byte_236 = reader.u8()?;
    let _byte_237 = reader.u8()?;
    let _ext_byte = reader.u8()?;
    Ok(v3_event_plugin_indices)
}

fn parse_tracks_chunk(reader: &mut LegacyReader, file: &mut PpsfLegacyFile) -> Result<()> {
    for _ in 0..10 {
        let magic: [u8; 4] = reader.bytes(4)?.try_into().unwrap();
        let size = reader.u32()? as usize;
        let mut list = LegacyReader::new(reader.bytes(size)?);
        if &magic != b"V3TS" {
            continue;
        }
        let count = list.varint_count()?;
        file.v3_tracks = Vec::with_capacity(count.min(64));
        for _ in 0..count {
            let track_magic: [u8; 4] = list.bytes(4)?.try_into().unwrap();
            let track_size = list.u32()?;
            let _index = list.u16()?;
            let payload = usize::try_from(track_size)
                .unwrap_or(usize::MAX)
                .checked_sub(2)
                .ok_or_else(|| LibreSvipError::msg("legacy ppsf track size below 2"))?;
            let mut body = LegacyReader::new(list.bytes(payload)?);
            if &track_magic != b"V3TK" {
                continue;
            }
            let v3_event_plugin_indices = parse_track_base(&mut body)?;
            let _name = body.varint_string()?;
            let clip_indices = body.u16_varint_array()?;
            file.v3_tracks.push(V3TrackRaw {
                v3_event_plugin_indices,
                clip_indices,
            });
        }
    }
    Ok(())
}

impl<'a> LegacyReader<'a> {
    /// `Prefixed(PpsfVarIntCount, GreedyBytes)` as a nested reader.
    fn varint_prefixed_slice(&mut self) -> Result<LegacyReader<'a>> {
        let len = self.varint()? as usize;
        Ok(LegacyReader::new(self.bytes(len)?))
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LegacyInputOptions {
    pub import_pitch: bool,
}

/// Port of `PiaproStudioLegacyParser.parse_project`.
pub(crate) fn parse_legacy_project(content: &[u8], options: LegacyInputOptions) -> Result<Project> {
    let file = parse_legacy_file(content)?;

    // Chunk order matters exactly as in Python: tempo/meter events are
    // resolved while walking `PLGS`, which references the `EVTS` chunk parsed
    // before it.
    let mut clip_offsets: Vec<i64> = Vec::new();
    let mut automation_id2event_indexes: HashMap<u16, Vec<u32>> = HashMap::new();
    let mut pit_indices_by_clip: HashMap<u16, Vec<u32>> = HashMap::new();
    let mut pbs_indices_by_clip: HashMap<u16, Vec<u32>> = HashMap::new();

    for clip in &file.note_clips {
        clip_offsets.push(i64::from(clip.offset));
    }
    for clip in &file.automation_clips {
        automation_id2event_indexes.insert(clip.index, clip.event_indices.clone());
        if clip.field_4 == 11 && clip.sub_event_indices.len() >= V3_PARAM_PBS_INDEX {
            pit_indices_by_clip.insert(clip.index, clip.sub_event_indices[V3_PARAM_PIT_INDEX - 1].clone());
            pbs_indices_by_clip.insert(clip.index, clip.sub_event_indices[V3_PARAM_PBS_INDEX - 1].clone());
        }
    }

    let mut tempo_events: Vec<(u32, u32)> = Vec::new();
    let mut meter_events: Vec<(u32, u32)> = Vec::new();
    let mut track_plugin_indices: HashMap<usize, Vec<u16>> = HashMap::new();
    let mut plugin_pit_clip: HashMap<u16, u16> = HashMap::new();

    let event_tick_value = |event: &LegacyEvent| -> Result<(u32, u32)> {
        match &event.data {
            LegacyEventData::Automation { tick, value } => Ok((*tick, *value)),
            _ => Err(LibreSvipError::msg(
                "expected an automation event in the tempo/meter clip",
            )),
        }
    };

    for plugin in &file.music_param_plugins {
        match plugin.name.as_str() {
            "Tempo" => {
                if let Some(indices) = automation_id2event_indexes.get(&plugin.self_clip_idx) {
                    for idx in indices {
                        let event = file
                            .events
                            .get(*idx as usize)
                            .ok_or_else(|| LibreSvipError::msg("event index out of range"))?;
                        tempo_events.push(event_tick_value(event)?);
                    }
                }
            }
            "Meter" => {
                if let Some(indices) = automation_id2event_indexes.get(&plugin.self_clip_idx) {
                    for idx in indices {
                        let event = file
                            .events
                            .get(*idx as usize)
                            .ok_or_else(|| LibreSvipError::msg("event index out of range"))?;
                        meter_events.push(event_tick_value(event)?);
                    }
                }
            }
            _ => {}
        }
    }
    for plugin in &file.v3_event_plugins {
        if plugin.name == "Vocaloid3 Parameter" {
            plugin_pit_clip.insert(plugin.index, plugin.self_clip_idx);
        }
    }

    let mut clips2track_indexes: HashMap<usize, usize> = HashMap::new();
    for (i, track) in file.v3_tracks.iter().enumerate() {
        for clip_index in &track.clip_indices {
            clips2track_indexes.insert(*clip_index as usize, i);
        }
        track_plugin_indices.insert(i, track.v3_event_plugin_indices.clone());
    }

    let mut pit_clip_by_track: HashMap<usize, u16> = HashMap::new();
    for (track_idx, plugin_list) in &track_plugin_indices {
        for plugin_idx in plugin_list {
            if let Some(clip_idx) = plugin_pit_clip.get(plugin_idx) {
                pit_clip_by_track.insert(*track_idx, *clip_idx);
                break;
            }
        }
    }

    let (tempos, time_signatures) =
        parse_tempos_and_time_signatures(&tempo_events, &meter_events);
    let synchronizer = TimeSynchronizer::from_project(&tempos);
    let first_bar_length = match time_signatures.first() {
        Some(first) => py_round(first.bar_length()),
        None => TICKS_IN_BEAT * 4,
    };

    let mut track_list: Vec<libresvip_core::model::base::Track> = Vec::new();
    let mut track_index2notes: Vec<(usize, Vec<Note>)> = Vec::new();
    // Iterate the note clips in file order: Python's dict insertion order is
    // what determines the track order of the hub model.
    for (clip_index, clip) in file.note_clips.iter().enumerate() {
        let track_index = match clips2track_indexes.get(&clip_index) {
            Some(index) => *index,
            None => continue, // Python would KeyError; only reachable on malformed files
        };
        for note_index in &clip.event_indices {
            let event = file
                .events
                .get(*note_index as usize)
                .ok_or_else(|| LibreSvipError::msg("event index out of range"))?;
            let LegacyEventData::V3Note {
                tick,
                note,
                duration,
                lyric,
                phoneme,
            } = &event.data
            else {
                return Err(LibreSvipError::msg(
                    "expected a Vocaloid3 note event in a note clip",
                ));
            };
            let offset = clip_offsets
                .get(clip_index)
                .copied()
                .ok_or_else(|| LibreSvipError::msg("clip index out of range"))?;
            let entry = match track_index2notes
                .iter_mut()
                .find(|(index, _)| *index == track_index)
            {
                Some((_, notes)) => notes,
                None => {
                    track_index2notes.push((track_index, Vec::new()));
                    &mut track_index2notes.last_mut().unwrap().1
                }
            };
            entry.push(Note {
                start_pos: offset + i64::from(*tick),
                length: i64::from(*duration),
                key_number: i64::from(*note),
                lyric: lyric.clone(),
                pronunciation: Some(phoneme.clone()),
                ..Default::default()
            });
        }
    }
    for (track_index, notes) in track_index2notes {
        let mut singing_track = SingingTrack {
            note_list: notes,
            ..Default::default()
        };
        let clip_idx = if options.import_pitch && !singing_track.note_list.is_empty() {
            pit_clip_by_track.get(&track_index).copied()
        } else {
            None
        };
        if let Some(clip_idx) = clip_idx {
            let pit_event_indices = pit_indices_by_clip.get(&clip_idx);
            let pbs_event_indices = pbs_indices_by_clip.get(&clip_idx);
            if let Some(pit_event_indices) =
                pit_event_indices.filter(|indices| !indices.is_empty())
            {
                let pitch = parse_pitch(
                    &file.events,
                    pit_event_indices,
                    pbs_event_indices.unwrap_or(&Vec::new()),
                    &singing_track.note_list,
                    &time_signatures,
                    first_bar_length,
                    &synchronizer,
                )?;
                if let Some(pitch) = pitch {
                    singing_track.edited_params.pitch = pitch;
                }
            }
        }
        track_list.push(libresvip_core::model::base::Track::Singing(singing_track));
    }

    Ok(Project {
        version: String::new(),
        song_tempo_list: tempos,
        time_signature_list: time_signatures,
        track_list,
        warnings: Vec::new(),
    })
}

fn parse_tempos_and_time_signatures(
    tempo_events: &[(u32, u32)],
    meter_events: &[(u32, u32)],
) -> (Vec<SongTempo>, Vec<TimeSignature>) {
    let tempos: Vec<SongTempo> = tempo_events
        .iter()
        .map(|(tick, value)| SongTempo {
            position: i64::from(*tick),
            bpm: f64::from(*value) / 10000.0,
        })
        .collect();
    let mut time_signatures: Vec<TimeSignature> = Vec::new();
    let mut prev_tick: i64 = 0;
    for (tick, value) in meter_events {
        let tick = i64::from(*tick);
        let denominator = value & 0xFF;
        let numerator = value >> 8;
        let (prev_bar_length, prev_bar_index) = match time_signatures.last() {
            Some(last) => (last.bar_length(), last.bar_index),
            None => ((TICKS_IN_BEAT * 4) as f64, 0),
        };
        let bar_index = prev_bar_index as f64
            + ((tick - prev_tick) as f64 / prev_bar_length).floor();
        time_signatures.push(TimeSignature {
            bar_index: bar_index as i64,
            numerator: numerator as i64,
            denominator: denominator as i64,
            ..Default::default()
        });
        prev_tick = tick;
    }
    let tempos = if tempos.is_empty() {
        vec![SongTempo::new(0, DEFAULT_BPM)]
    } else {
        tempos
    };
    let time_signatures = if time_signatures.is_empty() {
        vec![TimeSignature::default()]
    } else {
        time_signatures
    };
    (tempos, time_signatures)
}

/// `_parse_pitch`.
#[allow(clippy::too_many_arguments)]
fn parse_pitch(
    events: &[LegacyEvent],
    pit_event_indices: &[u32],
    pbs_event_indices: &[u32],
    notes: &[Note],
    time_signatures: &[TimeSignature],
    first_bar_length: i64,
    synchronizer: &TimeSynchronizer,
) -> Result<Option<ParamCurve>> {
    let mut pit_events = Vec::with_capacity(pit_event_indices.len());
    for idx in pit_event_indices {
        let event = events
            .get(*idx as usize)
            .ok_or_else(|| LibreSvipError::msg("event index out of range"))?;
        let LegacyEventData::Automation { tick, value } = &event.data else {
            return Err(LibreSvipError::msg(
                "expected an automation event in the pit clip",
            ));
        };
        let mut raw_value = i64::from(*value);
        if raw_value >= 0x8000_0000 {
            raw_value -= 0x1_0000_0000;
        }
        pit_events.push(ControllerEvent {
            pos: i64::from(*tick),
            value: raw_value,
        });
    }
    if pit_events.is_empty() {
        return Ok(None);
    }
    let mut pbs_events = Vec::with_capacity(pbs_event_indices.len());
    for idx in pbs_event_indices {
        let event = events
            .get(*idx as usize)
            .ok_or_else(|| LibreSvipError::msg("event index out of range"))?;
        let LegacyEventData::Automation { tick, value } = &event.data else {
            return Err(LibreSvipError::msg(
                "expected an automation event in the pbs clip",
            ));
        };
        pbs_events.push(ControllerEvent {
            pos: i64::from(*tick),
            value: i64::from(*value),
        });
    }

    let handler = VocaloidPitchHandler::new(
        synchronizer.clone(),
        notes,
        time_signatures,
        first_bar_length,
    );
    let pit = ControllerCurve::with_events(
        "pitch_bend",
        pit_events,
        0,
        -PITCH_MAX_VALUE - 1,
        PITCH_MAX_VALUE,
    );
    let pbs = ControllerCurve::with_events(
        "pitch_bend_sens",
        pbs_events,
        DEFAULT_PITCH_BEND_SENSITIVITY,
        1,
        MAX_PITCH_BEND_SENSITIVITY,
    );
    let pitch_data = PitchBendData { pit, pbs };
    handler.to_absolute_pitch(&[pitch_data], Some(&[0]))
}

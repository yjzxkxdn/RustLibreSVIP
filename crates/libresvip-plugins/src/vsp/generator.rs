//! Port of `experimental/vsp/vsp_generator.py`.
//!
//! The Python reference builds plain dicts and hands them to the construct
//! model for building; here the byte payloads are emitted directly, keeping
//! the same section/sub-section framing (see [`super::binary`]).

use std::cmp::max;

use libresvip_core::core::compat::py_round;
use libresvip_core::core::constants::TICKS_IN_BEAT;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::Result;
use libresvip_core::model::base::{Note, Project, SingingTrack, TimeSignature};

use crate::vpr::controllers::PITCH_MAX_VALUE;
use crate::vpr::pitch::VocaloidPitchHandler;

use super::binary::{linear_to_packed_raw, Writer};
use super::constants::*;

#[derive(Clone, Debug)]
pub struct OutputOptions {
    pub default_singer_name: String,
}

/// `EFFECT_DEFAULTS` keyed by `EFFECT_TYPES_ORDER` members.
fn effect_defaults(effect_type: u16) -> (u8, i64) {
    match effect_type {
        EFFECT_TYPE_PIT => (1, VSP_PIT_CENTER),
        6 | 17 | 12 | 13 | 14 | 7 => (1, 0),
        1 => (1, 168),
        11 => (1, 30),
        8 => (1, 110),
        15 | 16 => (0, 0),
        _ => unreachable!("not an EFFECT_TYPES_ORDER member"),
    }
}

/// `_vocaloid_pit_to_vsp`.
fn vocaloid_pit_to_vsp(pit: i64) -> i64 {
    let value = py_round(pit as f64 / PITCH_MAX_VALUE as f64 * VSP_PIT_RANGE as f64 + VSP_PIT_CENTER as f64);
    value.clamp(0, 127)
}

/// `_track_dict`.
fn build_track_entry(title: &str, singer_name: &str, volume: u8, pan: u8) -> Vec<u8> {
    let mut param_block = Writer::new();
    param_block.utf16_string("");
    for value in [168i16, 0, 0, 0, 240, 0, 0] {
        param_block.i16(value);
    }
    for value in [30i16, 0, 0, 0] {
        param_block.i16(value);
    }
    for _ in 0..3 {
        param_block.u16(14); // sub_46E220 magic
        param_block.f32(0.0);
        param_block.f32(0.0);
        param_block.f32(0.0);
    }
    param_block.u16(14); // sub_46E270 magic
    param_block.i16(0);
    param_block.i16(0);
    param_block.i16(0);
    param_block.f32(0.0);
    param_block.i16(0);
    param_block.i16(0); // param_17

    let mut entry = Writer::new();
    entry.utf16_string(title);
    entry.utf16_string(singer_name);
    entry.u8(volume);
    entry.u8(1); // const_1
    entry.subsection(&param_block.into_vec());
    entry.u8(pan);
    entry.u16(0); // field_20
    entry.u8(1); // field_12
    entry.into_vec()
}

/// `_note_entry_raw`; also returns `entry["packed_end"]["high"]` (the bar
/// number) which feeds `total_measures`.
fn build_note_entry(
    track_index: u16,
    note: &Note,
    ticks_per_beat: i64,
    beats_per_bar: i64,
) -> (Vec<u8>, i64) {
    let end_pos = note.start_pos + note.length;
    let lyric_char = note
        .lyric
        .chars()
        .next()
        .map(|character| character as u32 as u16)
        .unwrap_or(0);
    let packed_start = linear_to_packed_raw(note.start_pos, ticks_per_beat, beats_per_bar);
    let packed_end = linear_to_packed_raw(end_pos, ticks_per_beat, beats_per_bar);
    let end_bar = end_pos.div_euclid(ticks_per_beat * beats_per_bar);

    let mut params = Writer::new();
    params.u16(DEFAULT_VELOCITY);
    params.u16(0); // zero
    params.u8(0); // vibrato
    params.count(0); // intensity_envelope
    params.count(0); // vibrato_envelope_1
    params.count(0); // vibrato_envelope_0
    params.u16(0); // tail_word_0
    params.u8(0); // tail_byte_0
    params.u8(0); // tail_byte_1
    params.u8(0); // tail_byte_2
    params.u8(0); // intensity
    params.u16(0); // param_6
    params.u16(0); // param_17
    params.u16(0); // tail_word_1

    let mut entry = Writer::new();
    entry.u16(track_index);
    entry.u32(packed_start);
    entry.u32(packed_end);
    entry.u8(note.key_number as u8);
    entry.u16(lyric_char);
    entry.subsection(&params.into_vec());
    entry.utf16_string(&note.lyric);
    entry.u8(0); // flags
    entry.u8(0); // flags2
    (entry.into_vec(), end_bar)
}

/// `_make_effect_block`; entries are `(packed_value, bypass)` pairs.
fn build_effect_block(track_index: u16, effect_type: u16, flag: u8, entries: &[(u32, u32)]) -> Vec<u8> {
    let mut block = Writer::new();
    block.u16(track_index);
    block.u16(effect_type);
    block.count(entries.len());
    for &(packed_value, bypass) in entries {
        block.u32(packed_value);
        block.u32(bypass);
    }
    block.u8(flag);
    block.into_vec()
}

/// `_default_effect_blocks`.
fn default_effect_blocks(track_index: u16, pit_entries: Option<&[(u32, u32)]>) -> Vec<Vec<u8>> {
    let mut blocks = Vec::new();
    for &effect_type in &EFFECT_TYPES_ORDER {
        if effect_type == EFFECT_TYPE_PIT {
            if let Some(entries) = pit_entries {
                blocks.push(build_effect_block(track_index, effect_type, 1, entries));
                continue;
            }
        }
        let (flag, default_val) = effect_defaults(effect_type);
        if flag != 0 {
            blocks.push(build_effect_block(
                track_index,
                effect_type,
                flag,
                &[(0, default_val as u32)],
            ));
        } else {
            blocks.push(build_effect_block(track_index, effect_type, flag, &[]));
        }
    }
    blocks
}

pub struct VspGenerator {
    options: OutputOptions,
}

impl VspGenerator {
    pub fn new(options: OutputOptions) -> Self {
        Self { options }
    }

    pub fn generate_project(&self, project: &Project) -> Result<Vec<u8>> {
        let (numerator, denominator) = match project.time_signature_list.first() {
            Some(first) => (first.numerator, first.denominator),
            None => (4, 4),
        };
        let ticks_per_beat = TICKS_IN_WHOLE_NOTE / denominator;
        let beats_per_bar = numerator;
        let first_bar_length = match project.time_signature_list.first() {
            Some(first) => py_round(first.bar_length()),
            None => TICKS_IN_BEAT * 4,
        };
        let synchronizer = TimeSynchronizer::from_project(&project.song_tempo_list);

        let mut track_entries: Vec<Vec<u8>> = Vec::new();
        let mut note_entries: Vec<Vec<u8>> = Vec::new();
        let mut effect_blocks: Vec<Vec<u8>> = Vec::new();
        let mut max_bar: i64 = 0;
        let mut singing_track_idx: u16 = 0;

        for track in &project.track_list {
            let libresvip_core::model::base::Track::Singing(singing) = track else {
                continue;
            };
            let volume = py_round(singing.volume * 100.0).clamp(0, 100) as u8;
            let pan = py_round(singing.pan * 50.0 + 50.0).clamp(0, 100) as u8;
            let singer_name = if singing.ai_singer_name.is_empty() {
                self.options.default_singer_name.clone()
            } else {
                singing.ai_singer_name.clone()
            };
            track_entries.push(build_track_entry(&singing.title, &singer_name, volume, pan));
            for note in &singing.note_list {
                let (entry, end_bar) =
                    build_note_entry(singing_track_idx, note, ticks_per_beat, beats_per_bar);
                if end_bar > max_bar {
                    max_bar = end_bar;
                }
                note_entries.push(entry);
            }

            let pit_entries = if !singing.note_list.is_empty() {
                self.generate_pitch(
                    singing,
                    &project.time_signature_list,
                    first_bar_length,
                    &synchronizer,
                    ticks_per_beat,
                    beats_per_bar,
                )?
            } else {
                None
            };
            effect_blocks.extend(default_effect_blocks(singing_track_idx, pit_entries.as_deref()));
            singing_track_idx += 1;
        }
        effect_blocks.extend(default_effect_blocks(MASTER_TRACK_INDEX, None));
        effect_blocks.extend(default_effect_blocks(EFFECT_TRACK_INDEX, None));

        let mut out = Writer::new();

        let mut metadata = Writer::new();
        metadata.u32(0); // file_size, patched to the total length below
        metadata.u8(1); // is_vocalina2
        metadata.u8(11); // is_vocalina2_pro
        metadata.u8(0); // is_trial
        metadata.utf16_string("Imported");
        out.bytes(b"VSPF");
        out.section(&metadata.into_vec());

        let mut tempos = Writer::new();
        tempos.count(project.song_tempo_list.len());
        for tempo in &project.song_tempo_list {
            tempos.u32(tempo.position as u32);
            tempos.u16(py_round(tempo.bpm) as u16);
        }
        out.bytes(b"TMPO");
        out.section(&tempos.into_vec());

        let mut signatures = Writer::new();
        signatures.count(project.time_signature_list.len());
        for signature in &project.time_signature_list {
            signatures.u32(signature.bar_index as u32);
            signatures.u8(signature.numerator as u8);
            signatures.u8(signature.denominator as u8);
        }
        out.bytes(b"BEAT");
        out.section(&signatures.into_vec());

        let mut tracks = Writer::new();
        tracks.count(track_entries.len());
        for entry in &track_entries {
            tracks.subsection(entry);
        }
        tracks.u8(0); // is_muted
        tracks.subsection(&build_track_entry("Master", "", 100, 50));
        tracks.subsection(&build_track_entry("Effect", "", 100, 50));
        out.bytes(b"TRCK");
        out.section(&tracks.into_vec());

        let mut notes = Writer::new();
        notes.u16(max(max_bar + 4, DEFAULT_TOTAL_MEASURES) as u16);
        notes.count(note_entries.len());
        for entry in &note_entries {
            notes.subsection(entry);
        }
        out.bytes(b"NOTE");
        out.section(&notes.into_vec());

        let mut effects = Writer::new();
        effects.count(effect_blocks.len());
        for block in &effect_blocks {
            effects.subsection(block);
        }
        out.bytes(b"EFCT");
        out.section(&effects.into_vec());

        let mut vst_plugins = Writer::new();
        vst_plugins.count(0);
        out.bytes(b"VST_");
        out.section(&vst_plugins.into_vec());

        let mut bgm = Writer::new();
        bgm.u16(0); // block_size
        bgm.u32(0); // is_bgm_enabled
        bgm.u8(0); // unknown_byte_0
        bgm.u16(0); // unknown_word_2
        bgm.u32(0); // unknown_int_0
        bgm.u8(0); // unknown_byte_v6
        bgm.u16(0); // unknown_word_2b
        bgm.u32(0); // raw_data length
        out.bytes(b"BGM_");
        out.section(&bgm.into_vec());

        let mut grid = Writer::new();
        grid.u8(4);
        grid.u8(1);
        grid.u8(4);
        grid.u8(1);
        let mut playback = Writer::new();
        playback.u32(1); // playback_enabled
        playback.u32(0); // playback_start (packed)
        playback.u32(0); // playback_end (packed)
        playback.u16(0); // loop_start_tick
        playback.u16(0); // loop_end_tick
        playback.u16(0); // loop_count
        let mut volume = Writer::new();
        volume.u8(100); // volume_percent
        volume.u8(100); // master_volume
        let mut config = Writer::new();
        config.subsection(&grid.into_vec());
        config.subsection(&playback.into_vec());
        config.subsection(&volume.into_vec());
        out.bytes(b"CONF");
        out.section(&config.into_vec());

        // `vsp_converter.dump`: `struct.pack_into("<I", vsp_content, 8,
        // len(vsp_content))` — offset 8 is the metadata's `file_size` field.
        let mut bytes = out.into_vec();
        let total_len = bytes.len() as u32;
        bytes[8..12].copy_from_slice(&total_len.to_le_bytes());
        Ok(bytes)
    }

    /// `_generate_pitch`.
    #[allow(clippy::too_many_arguments)]
    fn generate_pitch(
        &self,
        track: &SingingTrack,
        time_signatures: &[TimeSignature],
        first_bar_length: i64,
        synchronizer: &TimeSynchronizer,
        ticks_per_beat: i64,
        beats_per_bar: i64,
    ) -> Result<Option<Vec<(u32, u32)>>> {
        if track.edited_params.pitch.points.is_empty() {
            return Ok(None);
        }
        let handler = VocaloidPitchHandler::new(
            synchronizer.clone(),
            &track.note_list,
            time_signatures,
            first_bar_length,
        );
        let result = handler.from_absolute_pitch(&track.edited_params.pitch)?;
        if result.is_empty() {
            return Ok(None);
        }
        let mut entries: Vec<(u32, u32)> = Vec::new();
        let mut prev_value: i64 = -1;
        for event in &result.pit.events {
            let vsp_value = vocaloid_pit_to_vsp(event.value);
            if vsp_value == prev_value {
                continue;
            }
            let packed_raw = linear_to_packed_raw(event.pos, ticks_per_beat, beats_per_bar);
            entries.push((packed_raw, vsp_value as u32));
            prev_value = vsp_value;
        }
        if entries.len() > MAX_EFFECT_ENTRIES {
            let step = entries.len() / MAX_EFFECT_ENTRIES + 1;
            let mut kept = vec![entries[0]];
            for index in 1..entries.len() - 1 {
                if index % step == 0 || entries[index].1 != entries[index - 1].1 {
                    kept.push(entries[index]);
                }
            }
            kept.push(entries[entries.len() - 1]);
            entries = kept;
        }
        Ok(if entries.is_empty() {
            None
        } else {
            Some(entries)
        })
    }
}

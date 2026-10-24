//! Port of `experimental/vsp/vsp_parser.py`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use libresvip_core::core::compat::py_round;
use libresvip_core::core::constants::{DEFAULT_BPM, TICKS_IN_BEAT};
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{IoContext, Result};
use libresvip_core::model::base::{
    InstrumentalTrack, Note, ParamCurve, Project, SingingTrack, SongTempo, TimeSignature, Track,
};

use crate::vpr::controllers::{
    ControllerCurve, ControllerEvent, DEFAULT_PITCH_BEND_SENSITIVITY, MAX_PITCH_BEND_SENSITIVITY,
    PITCH_MAX_VALUE,
};
use crate::vpr::pitch::{PitchBendData, VocaloidPitchHandler};

use super::binary::{raw_packed_to_linear, Reader};
use super::constants::*;

#[derive(Clone, Copy, Debug)]
pub struct InputOptions {
    pub import_instrumental_track: bool,
    pub import_pitch: bool,
    pub extract_audio: bool,
}

struct TrackEntryData {
    track_name: String,
    singer_name: String,
    volume: u8,
    pan: u8,
}

struct NoteEntryData {
    track_index: u16,
    packed_start: u32,
    packed_end: u32,
    note_number: u8,
    lyrics: String,
}

/// `_vsp_pit_to_vocaloid_pit`.
fn vsp_pit_to_vocaloid_pit(value: i64) -> i64 {
    py_round((value - VSP_PIT_CENTER) as f64 / VSP_PIT_RANGE as f64 * PITCH_MAX_VALUE as f64)
}

#[cfg(not(target_arch = "wasm32"))]
fn path_exists(path: &Path) -> bool {
    path.exists()
}

#[cfg(target_arch = "wasm32")]
fn path_exists(path: &Path) -> bool {
    libresvip_core::memory_fs::exists(path)
}

pub struct VspParser {
    options: InputOptions,
    path: PathBuf,
}

impl VspParser {
    pub fn new(options: InputOptions, path: &Path) -> Self {
        Self {
            options,
            path: path.to_owned(),
        }
    }

    pub fn parse_project(&self, data: &[u8]) -> Result<Project> {
        let mut reader = Reader::new(data);
        self.parse_metadata(&mut reader)?;
        let tempos = self.parse_tempos(&mut reader)?;
        let time_signatures = self.parse_time_signatures(&mut reader)?;
        let track_entries = self.parse_tracks(&mut reader)?;
        let note_entries = self.parse_notes(&mut reader)?;
        // `parse_project` derives the packing geometry from the first time
        // signature *before* interpreting the effect blocks.
        let (numerator, denominator) = match time_signatures.first() {
            Some(first) => (first.numerator, first.denominator),
            None => (DEFAULT_NUMERATOR, DEFAULT_DENOMINATOR),
        };
        let ticks_per_beat = TICKS_IN_WHOLE_NOTE / denominator;
        let beats_per_bar = numerator;
        let pit_by_track =
            self.extract_pit_curves(&mut reader, ticks_per_beat, beats_per_bar)?;
        self.parse_vst_plugins(&mut reader)?;
        let bgm_raw = self.parse_bgm_header(&mut reader)?;
        self.parse_config(&mut reader)?;

        let first_bar_length = match time_signatures.first() {
            Some(first) => py_round(first.bar_length()),
            None => TICKS_IN_BEAT * 4,
        };
        let synchronizer = TimeSynchronizer::from_project(&tempos);

        let mut track_list: Vec<Track> = Vec::new();
        for (track_idx, entry) in track_entries.iter().enumerate() {
            let track_notes: Vec<&NoteEntryData> = note_entries
                .iter()
                .filter(|note| note.track_index as usize == track_idx)
                .collect();
            let mut note_list = Vec::with_capacity(track_notes.len());
            for note in track_notes {
                let start_pos =
                    raw_packed_to_linear(note.packed_start, ticks_per_beat, beats_per_bar);
                let end_pos = raw_packed_to_linear(note.packed_end, ticks_per_beat, beats_per_bar);
                note_list.push(Note {
                    start_pos,
                    length: end_pos - start_pos,
                    key_number: i64::from(note.note_number),
                    lyric: note.lyrics.clone(),
                    ..Default::default()
                });
            }
            let mut singing_track = SingingTrack {
                title: entry.track_name.clone(),
                ai_singer_name: entry.singer_name.clone(),
                note_list,
                volume: f64::from(entry.volume) / 100.0,
                pan: (f64::from(entry.pan) - 50.0) / 50.0,
                ..Default::default()
            };
            if self.options.import_pitch && !singing_track.note_list.is_empty() {
                let pitch = self.parse_pitch(
                    track_idx as u16,
                    &singing_track.note_list,
                    &time_signatures,
                    first_bar_length,
                    &synchronizer,
                    &pit_by_track,
                )?;
                if let Some(pitch) = pitch {
                    singing_track.edited_params.pitch = pitch;
                }
            }
            track_list.push(Track::Singing(singing_track));
        }

        if self.options.import_instrumental_track {
            if let Some(bgm_track) = self.parse_bgm(&bgm_raw)? {
                track_list.push(Track::Instrumental(bgm_track));
            }
        }

        Ok(Project {
            version: String::new(),
            song_tempo_list: tempos,
            time_signature_list: time_signatures,
            track_list,
            warnings: Vec::new(),
        })
    }

    fn parse_metadata(&self, reader: &mut Reader) -> Result<()> {
        reader.expect_bytes(b"VSPF")?;
        let mut section = reader.section()?;
        let _file_size = section.u32()?;
        let _is_vocalina2 = section.u8()?;
        let _is_vocalina2_pro = section.u8()?;
        let _is_trial = section.u8()?;
        let _project_name = section.utf16_string()?;
        Ok(())
    }

    fn parse_tempos(&self, reader: &mut Reader) -> Result<Vec<SongTempo>> {
        reader.expect_bytes(b"TMPO")?;
        let mut section = reader.section()?;
        let count = section.count()?;
        let mut tempos = Vec::with_capacity(count);
        for _ in 0..count {
            let tick = section.u32()?;
            let tempo_value = section.u16()?;
            tempos.push(SongTempo {
                position: i64::from(tick),
                bpm: if tempo_value == 0 {
                    DEFAULT_BPM
                } else {
                    f64::from(tempo_value)
                },
            });
        }
        Ok(tempos)
    }

    fn parse_time_signatures(&self, reader: &mut Reader) -> Result<Vec<TimeSignature>> {
        reader.expect_bytes(b"BEAT")?;
        let mut section = reader.section()?;
        let count = section.count()?;
        let mut signatures = Vec::with_capacity(count);
        for _ in 0..count {
            let tick = section.u32()?;
            let numerator = section.u8()?;
            let denominator = section.u8()?;
            signatures.push(TimeSignature {
                bar_index: i64::from(tick),
                numerator: if numerator == 0 {
                    DEFAULT_NUMERATOR
                } else {
                    i64::from(numerator)
                },
                denominator: if denominator == 0 {
                    DEFAULT_DENOMINATOR
                } else {
                    i64::from(denominator)
                },
                ..Default::default()
            });
        }
        Ok(signatures)
    }

    fn parse_track_entry(&self, reader: &mut Reader) -> Result<TrackEntryData> {
        let mut entry = reader.subsection()?;
        let track_name = entry.utf16_string()?;
        let singer_name = entry.utf16_string()?;
        let volume = entry.u8()?;
        entry.expect_u8(1)?;
        let mut param_block = entry.subsection()?;
        let _wstring = param_block.utf16_string()?;
        param_block.bytes(7 * 2)?; // params_1_7
        param_block.bytes(4 * 2)?; // params_11_14
        for _ in 0..3 {
            param_block.expect_u16(14)?; // sub_46E220_1..3
            param_block.bytes(12)?;
        }
        param_block.expect_u16(14)?; // sub_46E270
        param_block.bytes(12)?;
        param_block.bytes(2)?; // param_17
        let pan = entry.u8()?;
        let _field_20 = entry.u16()?;
        let _field_12 = entry.u8()?;
        Ok(TrackEntryData {
            track_name,
            singer_name,
            volume,
            pan,
        })
    }

    fn parse_tracks(&self, reader: &mut Reader) -> Result<Vec<TrackEntryData>> {
        reader.expect_bytes(b"TRCK")?;
        let mut section = reader.section()?;
        let count = section.count()?;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            entries.push(self.parse_track_entry(&mut section)?);
        }
        let _is_muted = section.u8()?;
        self.parse_track_entry(&mut section)?;
        self.parse_track_entry(&mut section)?;
        Ok(entries)
    }

    fn parse_notes(&self, reader: &mut Reader) -> Result<Vec<NoteEntryData>> {
        reader.expect_bytes(b"NOTE")?;
        let mut section = reader.section()?;
        let _total_measures = section.u16()?;
        let count = section.count()?;
        let mut notes = Vec::with_capacity(count);
        for _ in 0..count {
            let mut note = section.subsection()?;
            let track_index = note.u16()?;
            let packed_start = note.u32()?;
            let packed_end = note.u32()?;
            let note_number = note.u8()?;
            let _lyric_char = note.u16()?;
            let _params = note.subsection()?;
            let lyrics = note.utf16_string()?;
            let _flags = note.u8()?;
            let _flags2 = note.u8()?;
            notes.push(NoteEntryData {
                track_index,
                packed_start,
                packed_end,
                note_number,
                lyrics,
            });
        }
        Ok(notes)
    }

    /// `_extract_pit_curves` — pitch-bend effect blocks per track.
    fn extract_pit_curves(
        &self,
        reader: &mut Reader,
        ticks_per_beat: i64,
        beats_per_bar: i64,
    ) -> Result<HashMap<u16, Vec<ControllerEvent>>> {
        reader.expect_bytes(b"EFCT")?;
        let mut section = reader.section()?;
        let count = section.count()?;
        let mut pit_by_track: HashMap<u16, Vec<ControllerEvent>> = HashMap::new();
        for _ in 0..count {
            let mut block = section.subsection()?;
            let track_index = block.u16()?;
            let effect_type = block.u16()?;
            let entry_count = block.count()?;
            let mut entries = Vec::with_capacity(entry_count);
            for _ in 0..entry_count {
                let packed_value = block.u32()?;
                let mut bypass = i64::from(block.u32()?);
                if bypass >= 0x8000_0000 {
                    bypass -= 0x1_0000_0000;
                }
                entries.push((packed_value, bypass));
            }
            let _flag = block.u8()?;
            if effect_type != EFFECT_TYPE_PIT || track_index > 0xFFF0 {
                continue;
            }
            let events = entries
                .into_iter()
                .map(|(packed_value, bypass)| ControllerEvent {
                    pos: raw_packed_to_linear(packed_value, ticks_per_beat, beats_per_bar),
                    value: vsp_pit_to_vocaloid_pit(bypass),
                })
                .collect();
            pit_by_track.insert(track_index, events);
        }
        Ok(pit_by_track)
    }

    /// `parse_vst_plugins` — the section must still be walked for validation.
    fn parse_vst_plugins(&self, reader: &mut Reader) -> Result<()> {
        reader.expect_bytes(b"VST_")?;
        let mut section = reader.section()?;
        let count = section.count()?;
        for _ in 0..count {
            let mut entry = section.subsection()?;
            let _track_index = entry.u16()?;
            let _plugin_index = entry.u16()?;
            let _plugin_name = entry.utf16_string()?;
            let _preset_name = entry.utf16_string()?;
            let _bypass = entry.u8()?;
            let _param_display_count = entry.u32()?;
            let param_count = entry.count()?;
            for _ in 0..param_count {
                entry.f32()?;
            }
        }
        Ok(())
    }

    /// Parses the BGM section and returns the embedded PCM bytes.
    fn parse_bgm_header(&self, reader: &mut Reader) -> Result<Vec<u8>> {
        reader.expect_bytes(b"BGM_")?;
        let mut section = reader.section()?;
        let _block_size = section.u16()?;
        let _is_bgm_enabled = section.u32()?;
        let _unknown_byte_0 = section.u8()?;
        let _unknown_word_2 = section.u16()?;
        let _unknown_int_0 = section.u32()?;
        let _unknown_byte_v6 = section.u8()?;
        let _unknown_word_2b = section.u16()?;
        let raw_len = section.u32()? as usize;
        Ok(section.bytes(raw_len)?.to_vec())
    }

    /// `_parse_bgm` — writes the extracted wav and appends the instrumental
    /// track if the audio is (now) available next to the project.
    fn parse_bgm(&self, raw_data: &[u8]) -> Result<Option<InstrumentalTrack>> {
        if raw_data.is_empty() || self.path.file_name().is_none() || raw_data[..64.min(raw_data.len())].iter().all(|&byte| byte == 0) {
            return Ok(None);
        }
        let audio_path = self.path.with_extension("wav");
        if self.options.extract_audio && !path_exists(&audio_path) {
            libresvip_core::io::write(&audio_path, build_bgm_wave(raw_data))
                .at_path(&audio_path)?;
        }
        if path_exists(&audio_path) {
            let title = self
                .path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default();
            return Ok(Some(InstrumentalTrack {
                title,
                audio_file_path: audio_path.to_string_lossy().into_owned(),
                ..Default::default()
            }));
        }
        Ok(None)
    }

    fn parse_config(&self, reader: &mut Reader) -> Result<()> {
        reader.expect_bytes(b"CONF")?;
        let mut section = reader.section()?;
        let mut grid = section.subsection()?;
        grid.bytes(4)?;
        let mut playback = section.subsection()?;
        playback.u32()?;
        playback.u32()?;
        playback.u32()?;
        playback.u16()?;
        playback.u16()?;
        playback.u16()?;
        let mut volume = section.subsection()?;
        volume.u8()?;
        volume.u8()?;
        Ok(())
    }

    /// `_parse_pitch`.
    #[allow(clippy::too_many_arguments)]
    fn parse_pitch(
        &self,
        track_idx: u16,
        note_list: &[Note],
        time_signatures: &[TimeSignature],
        first_bar_length: i64,
        synchronizer: &TimeSynchronizer,
        pit_by_track: &HashMap<u16, Vec<ControllerEvent>>,
    ) -> Result<Option<ParamCurve>> {
        let pit_events = match pit_by_track.get(&track_idx) {
            Some(events) if !events.is_empty() => events.clone(),
            _ => return Ok(None),
        };
        let handler = VocaloidPitchHandler::new(
            synchronizer.clone(),
            note_list,
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
            Vec::new(),
            DEFAULT_PITCH_BEND_SENSITIVITY,
            1,
            MAX_PITCH_BEND_SENSITIVITY,
        );
        let pitch_data = PitchBendData { pit, pbs };
        handler.to_absolute_pitch(&[pitch_data], Some(&[0]))
    }
}

/// The 44-byte PCM wave header Python's `wave` module writes for the BGM
/// extraction (44100 Hz, stereo, 16-bit).
fn build_bgm_wave(raw_data: &[u8]) -> Vec<u8> {
    let data_len = raw_data.len() as u32;
    let block_align = BGM_CHANNELS * BGM_SAMPLE_WIDTH;
    let byte_rate = BGM_SAMPLE_RATE * u32::from(block_align);
    let mut wav = Vec::with_capacity(44 + raw_data.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&BGM_CHANNELS.to_le_bytes());
    wav.extend_from_slice(&BGM_SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&(BGM_SAMPLE_WIDTH * 8).to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    wav.extend_from_slice(raw_data);
    wav
}

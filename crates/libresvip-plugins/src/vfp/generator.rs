//! Port of `libresvip.plugins.vfp.vox_factory_generator`.

use std::path::{Path, PathBuf};

use libresvip_core::core::constants::DEFAULT_PHONEME;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{Note, ParamCurve, Params, Project, Track, TICKS_IN_BEAT};
use libresvip_core::options::Options;

use super::audio::audio_track_info;
use super::model::{
    VoxAudioData, VoxClip, VoxNote, VoxProject, VoxTrack,
};

pub struct VOXFactoryGenerator<'a> {
    #[allow(dead_code)]
    options: &'a Options,
    prefix: String,
    pub audio_paths: indexmap::IndexMap<String, PathBuf>,
    synchronizer: Option<TimeSynchronizer>,
    first_bar_length: i64,
}

impl<'a> VOXFactoryGenerator<'a> {
    pub fn new(options: &'a Options) -> Self {
        Self {
            options,
            prefix: String::new(),
            audio_paths: indexmap::IndexMap::new(),
            synchronizer: None,
            first_bar_length: 0,
        }
    }

    pub fn generate_project(&mut self, project: &Project) -> Result<VoxProject> {
        self.prefix = random_prefix();
        let first = project
            .time_signature_list
            .first()
            .ok_or_else(|| LibreSvipError::msg("missing time signature"))?;
        self.first_bar_length = first.bar_length() as i64;
        self.synchronizer = Some(TimeSynchronizer::new(
            &project.song_tempo_list,
            0,
            false,
            60.0,
        ));
        let (track_bank, audio_data_bank) = self.generate_tracks(&project.track_list)?;
        let mut track_order: Vec<String> = track_bank.keys().cloned().collect();
        track_order.sort();
        Ok(VoxProject {
            version: "0.12.0".to_owned(),
            tempo: project
                .song_tempo_list
                .first()
                .map(|tempo| tempo.bpm)
                .unwrap_or(120.0),
            time_signature: project
                .time_signature_list
                .first()
                .map(|signature| vec![signature.numerator, signature.denominator])
                .unwrap_or_else(|| vec![4, 4]),
            track_bank,
            track_order,
            audio_data_bank,
            ..Default::default()
        })
    }

    fn generate_tracks(
        &mut self,
        tracks: &[Track],
    ) -> Result<(
        indexmap::IndexMap<String, VoxTrack>,
        indexmap::IndexMap<String, VoxAudioData>,
    )> {
        let mut track_bank = indexmap::IndexMap::new();
        let mut audio_data_bank = indexmap::IndexMap::new();
        for (i, track) in tracks.iter().enumerate() {
            match track {
                Track::Instrumental(instrumental) => {
                    let audio_path = Path::new(&instrumental.audio_file_path);
                    let Some(track_info) = audio_track_info(audio_path) else {
                        continue;
                    };
                    let extension = audio_path
                        .extension()
                        .map(|extension| format!(".{}", extension.to_string_lossy()))
                        .unwrap_or_default();
                    let source_audio_data_key = format!("{}-au{i}{extension}", self.prefix);
                    self.audio_paths
                        .insert(source_audio_data_key.clone(), audio_path.to_path_buf());
                    audio_data_bank.insert(
                        source_audio_data_key.clone(),
                        VoxAudioData {
                            sample_rate: track_info.sample_rate,
                            sample_length: (track_info.duration * track_info.sample_rate as f64)
                                as i64,
                            number_of_channels: track_info.channels,
                            ..Default::default()
                        },
                    );
                    let clip_key = format!("{}-cl0", self.prefix);
                    let clip = VoxClip {
                        name: audio_path
                            .file_stem()
                            .map(|stem| stem.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                        start_quarter: instrumental.offset as f64 / TICKS_IN_BEAT as f64,
                        offset_quarter: 0.0,
                        length: track_info.duration,
                        source_audio_data_key: Some(source_audio_data_key),
                        type_: "audio".to_owned(),
                        length_type: "time".to_owned(),
                        ..Default::default()
                    };
                    let mut clip_bank = indexmap::IndexMap::new();
                    clip_bank.insert(clip_key.clone(), clip);
                    track_bank.insert(
                        format!("{}-tr{i}", self.prefix),
                        VoxTrack {
                            clip_bank,
                            clip_order: vec![clip_key],
                            name: instrumental.title.clone(),
                            mute: instrumental.mute,
                            solo: instrumental.solo,
                            pan: instrumental.pan,
                            type_: "audio".to_owned(),
                            ..Default::default()
                        },
                    );
                }
                Track::Singing(singing) => {
                    let clip_bank = self.generate_notes(&singing.note_list, &singing.edited_params);
                    let mut clip_order: Vec<String> = clip_bank.keys().cloned().collect();
                    clip_order.sort();
                    track_bank.insert(
                        format!("{}-tr{i}", self.prefix),
                        VoxTrack {
                            clip_bank,
                            clip_order,
                            name: singing.title.clone(),
                            mute: singing.mute,
                            solo: singing.solo,
                            pan: singing.pan,
                            type_: "vocal".to_owned(),
                            ..Default::default()
                        },
                    );
                }
            }
        }
        Ok((track_bank, audio_data_bank))
    }

    fn generate_notes(
        &self,
        notes: &[Note],
        params: &Params,
    ) -> indexmap::IndexMap<String, VoxClip> {
        let mut note_bank = indexmap::IndexMap::new();
        let mut note_order = Vec::new();
        let max_ticks = notes.last().map(Note::end_pos).unwrap_or(0);
        let max_quarter = max_ticks as f64 / TICKS_IN_BEAT as f64;
        for (i, note) in notes.iter().enumerate() {
            let key = format!("{}-no{i}", self.prefix);
            note_bank.insert(key.clone(), self.generate_note(note, params));
            note_order.push(key);
        }
        let clip_count = (max_quarter / 32.0).ceil() as i64;
        let mut clip_bank = indexmap::IndexMap::new();
        for i in 0..clip_count {
            clip_bank.insert(
                format!("{}-cl{i}", self.prefix),
                VoxClip {
                    start_quarter: 32.0 * i as f64,
                    offset_quarter: 32.0 * i as f64,
                    length: 32.0,
                    note_bank: note_bank.clone(),
                    note_order: note_order.clone(),
                    ..Default::default()
                },
            );
        }
        clip_bank
    }

    fn generate_note(&self, note: &Note, params: &Params) -> VoxNote {
        let note_start_time = self
            .synchronizer
            .as_ref()
            .map(|synchronizer| synchronizer.get_actual_secs_from_ticks(note.start_pos))
            .unwrap_or(0.0);
        VoxNote {
            time: note_start_time,
            ticks: note.start_pos as f64,
            duration_ticks: note.length as f64,
            midi: note.key_number,
            name: note.lyric.clone(),
            syllable: match &note.pronunciation {
                Some(value) if !value.is_empty() => value.clone(),
                _ => DEFAULT_PHONEME.to_owned(),
            },
            pitch_bends: self.generate_note_pitch(note, &params.pitch),
            ..Default::default()
        }
    }

    fn generate_note_pitch(&self, note: &Note, pitch: &ParamCurve) -> Vec<f64> {
        let Some(synchronizer) = self.synchronizer.as_ref() else {
            return Vec::new();
        };
        let note_start_time = synchronizer.get_actual_secs_from_ticks(note.start_pos);
        let note_end_time = synchronizer.get_actual_secs_from_ticks(note.end_pos());
        let mut intervals: Vec<KeyInterval> = Vec::new();
        let mut prev_secs: Option<f64> = None;
        let mut prev_key: f64 = -1.0;
        for point in &pitch.points {
            if point.x - self.first_bar_length < note.start_pos {
                continue;
            } else if point.x - self.first_bar_length > note.end_pos() {
                break;
            }
            if point.y == -100 {
                prev_secs = None;
                prev_key = 0.0;
            } else {
                let secs = synchronizer
                    .get_actual_secs_from_ticks(point.x - self.first_bar_length);
                let key = point.y as f64 / 100.0;
                if let Some(previous_secs) = prev_secs {
                    intervals.push(KeyInterval {
                        start: previous_secs,
                        end: secs,
                        include_start: false,
                        include_end: true,
                        value: IntervalValue::Linear {
                            x0: previous_secs,
                            y0: prev_key - note.key_number as f64,
                            x1: secs,
                            y1: key - note.key_number as f64,
                        },
                    });
                } else {
                    intervals.push(KeyInterval {
                        start: secs,
                        end: secs,
                        include_start: true,
                        include_end: true,
                        value: IntervalValue::Constant(key - note.key_number as f64),
                    });
                }
                prev_secs = Some(secs);
                prev_key = key;
            }
        }
        let secs_step = 1024.0 / 44100.0;
        let mut pitch_bends = Vec::new();
        let mut n = 0usize;
        loop {
            let secs = note_start_time + n as f64 * secs_step;
            if !(secs < note_end_time) {
                break;
            }
            pitch_bends.push(interval_get(&intervals, secs).unwrap_or(0.0));
            n += 1;
        }
        if pitch_bends.iter().any(|value| *value != 0.0) {
            pitch_bends
        } else {
            Vec::new()
        }
    }
}

enum IntervalValue {
    Constant(f64),
    Linear {
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
    },
}

struct KeyInterval {
    start: f64,
    end: f64,
    include_start: bool,
    include_end: bool,
    value: IntervalValue,
}

fn interval_get(intervals: &[KeyInterval], secs: f64) -> Option<f64> {
    for interval in intervals.iter().rev() {
        let after_start = if interval.include_start {
            secs >= interval.start
        } else {
            secs > interval.start
        };
        let before_end = if interval.include_end {
            secs <= interval.end
        } else {
            secs < interval.end
        };
        if after_start && before_end {
            return Some(match &interval.value {
                IntervalValue::Constant(value) => *value,
                IntervalValue::Linear { x0, y0, x1, y1 } => {
                    let r = (secs - x0) / (x1 - x0);
                    y0 + (y1 - y0) * r
                }
            });
        }
    }
    None
}

/// `secrets.token_hex(5)` — ten random hex characters.
fn random_prefix() -> String {
    let uuid = libresvip_core::utils::text::uuid_str();
    uuid.chars()
        .filter(|character| *character != '-')
        .take(10)
        .collect()
}

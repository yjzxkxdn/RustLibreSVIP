//! Port of `libresvip.plugins.ds.utils.note_list_util`,
//! `utils.models.ds_note`, `utils.models.ds_project`, `ds_param_curve` and
//! `ds_param_node`.

use std::collections::HashMap;

use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::Note;

use super::lyric::get_symbol_removed_lyric;
use super::model::DsItem;
use super::music_math::{float_repr, midi2note, round_ndigits};
use super::param_utils::{encode_gender, encode_pitch, DsParamCurve};
use super::pinyin::{pinyin_series, split_pinyin};

/// `DsPhonemeItem`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DsPhonemeItem {
    pub phoneme: String,
    pub duration: Option<f64>,
    pub note_name: String,
}

impl DsPhonemeItem {
    fn new(phoneme: impl Into<String>, duration: Option<f64>, note_name: impl Into<String>) -> Self {
        Self {
            phoneme: phoneme.into(),
            duration,
            note_name: note_name.into(),
        }
    }
}

/// `DsPhoneme`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DsPhoneme {
    pub consonant: Option<DsPhonemeItem>,
    pub vowel: DsPhonemeItem,
}

/// `DsNote` (the `is_slur` property is a method here).
#[derive(Clone, Debug, PartialEq)]
pub struct DsNote {
    pub duration: f64,
    pub ds_phoneme: DsPhoneme,
    pub lyric: String,
    pub note_name: String,
}

impl DsNote {
    /// `DsNote.is_slur`.
    pub fn is_slur(&self) -> bool {
        self.lyric.contains('-')
    }
}

fn rest_note(duration: f64, vowel_duration: f64) -> DsNote {
    DsNote {
        duration,
        ds_phoneme: DsPhoneme {
            consonant: None,
            vowel: DsPhonemeItem::new("SP", Some(vowel_duration), "rest"),
        },
        lyric: "SP".to_owned(),
        note_name: "rest".to_owned(),
    }
}

fn aspiration_note(duration: f64, vowel_duration: f64) -> DsNote {
    DsNote {
        duration,
        ds_phoneme: DsPhoneme {
            consonant: None,
            vowel: DsPhonemeItem::new("AP", Some(vowel_duration), "rest"),
        },
        lyric: "AP".to_owned(),
        note_name: "rest".to_owned(),
    }
}

fn py_float(value: f64) -> String {
    float_repr(value)
}

fn py_opt_float(value: Option<f64>) -> String {
    value.map_or_else(|| "None".to_owned(), py_float)
}

/// `encode_notes`.
pub fn encode_notes(
    os_notes: &[Note],
    synchronizer: &TimeSynchronizer,
    trailing_space: f64,
    phoneme_table: &HashMap<String, String>,
) -> Result<Vec<DsNote>> {
    let lyrics: Vec<String> = os_notes.iter().map(|note| note.lyric.clone()).collect();
    let pinyin_list = pinyin_series(&lyrics)?;

    let mut ds_notes: Vec<DsNote> = Vec::new();
    let mut prev_end_in_ticks: i64 = 0;
    let mut prev_actual_end_in_secs = 0.0f64;
    let mut prev_phoneme = DsPhoneme::default();
    let min_asp_len = 0.1f64;
    let max_asp_len = 0.4f64;

    for (index, note) in os_notes.iter().enumerate() {
        let prev_end_in_secs = synchronizer.get_actual_secs_from_ticks(prev_end_in_ticks);
        let cur_start_in_ticks = note.start_pos;
        let cur_end_in_ticks = cur_start_in_ticks + note.length;
        let cur_start_in_secs = synchronizer.get_actual_secs_from_ticks(cur_start_in_ticks);
        let cur_end_in_secs = synchronizer.get_actual_secs_from_ticks(cur_end_in_ticks);
        let mut cur_actual_start_in_secs = cur_start_in_secs;
        let mut cur_actual_end_in_secs = cur_end_in_secs;
        if let Some(phones) = note.edited_phones.as_ref() {
            if phones.head_length_in_secs >= 0.0 {
                cur_actual_start_in_secs -= phones.head_length_in_secs;
            }
        }
        if index < os_notes.len() - 1 {
            if let Some(next_phones) = os_notes[index + 1].edited_phones.as_ref() {
                if next_phones.head_length_in_secs >= 0.0 {
                    let next_note = &os_notes[index + 1];
                    let next_start_in_secs =
                        synchronizer.get_actual_secs_from_ticks(next_note.start_pos);
                    let next_actual_start_in_secs =
                        next_start_in_secs - next_phones.head_length_in_secs;
                    if cur_end_in_secs > next_actual_start_in_secs {
                        cur_actual_end_in_secs -= cur_end_in_secs - next_actual_start_in_secs;
                    }
                }
            }
        }

        // Fill the gap between this note and the previous one.
        let gap = cur_actual_start_in_secs - prev_actual_end_in_secs;
        if gap > 0.0 {
            if gap < min_asp_len {
                let rest = rest_note(
                    round_ndigits(cur_start_in_secs - prev_end_in_secs, 6),
                    round_ndigits(cur_actual_start_in_secs - prev_actual_end_in_secs, 6),
                );
                prev_phoneme = rest.ds_phoneme.clone();
                ds_notes.push(rest);
            } else if gap < max_asp_len {
                let aspiration = aspiration_note(
                    round_ndigits(cur_start_in_secs - prev_end_in_secs, 6),
                    round_ndigits(cur_actual_start_in_secs - prev_actual_end_in_secs, 6),
                );
                prev_phoneme = aspiration.ds_phoneme.clone();
                ds_notes.push(aspiration);
            } else {
                let rest = rest_note(
                    round_ndigits(
                        cur_start_in_secs - prev_end_in_secs - max_asp_len,
                        6,
                    ),
                    round_ndigits(
                        cur_actual_start_in_secs - prev_actual_end_in_secs - max_asp_len,
                        6,
                    ),
                );
                ds_notes.push(rest);
                let aspiration = aspiration_note(
                    round_ndigits(max_asp_len, 6),
                    round_ndigits(max_asp_len, 6),
                );
                prev_phoneme = aspiration.ds_phoneme.clone();
                ds_notes.push(aspiration);
            }
        }

        // Convert the OpenSVIP note into a DiffSinger note.
        let mut ds_phoneme = DsPhoneme::default();
        if note.lyric.contains('-') {
            ds_phoneme.vowel = DsPhonemeItem::new(
                prev_phoneme.vowel.phoneme.clone(),
                Some(round_ndigits(
                    cur_actual_end_in_secs - cur_actual_start_in_secs,
                    6,
                )),
                midi2note(note.key_number as f64),
            );
        } else {
            let pronunciation = note
                .pronunciation
                .as_deref()
                .filter(|value| !value.is_empty());
            let pinyin = match pronunciation {
                Some(value) => value.to_owned(),
                None => pinyin_list.get(index).cloned().ok_or_else(|| {
                    LibreSvipError::msg("get_pinyin_series: the pinyin list ran out of items")
                })?,
            };
            let (consonant, vowel) = split_pinyin(phoneme_table, &pinyin)?;
            if !consonant.is_empty() {
                let consonant_note_name = if matches!(
                    prev_phoneme.vowel.phoneme.as_str(),
                    "SP" | "AP"
                ) {
                    midi2note(note.key_number as f64)
                } else {
                    prev_phoneme.vowel.note_name.clone()
                };
                ds_phoneme.consonant = Some(DsPhonemeItem::new(
                    consonant,
                    if cur_actual_start_in_secs == cur_start_in_secs {
                        None
                    } else {
                        Some(round_ndigits(
                            cur_start_in_secs - cur_actual_start_in_secs,
                            6,
                        ))
                    },
                    consonant_note_name,
                ));
                ds_phoneme.vowel = DsPhonemeItem::new(
                    vowel,
                    Some(round_ndigits(cur_actual_end_in_secs - cur_start_in_secs, 6)),
                    midi2note(note.key_number as f64),
                );
            } else {
                ds_phoneme.vowel = DsPhonemeItem::new(
                    vowel,
                    Some(round_ndigits(
                        cur_actual_end_in_secs - cur_actual_start_in_secs,
                        6,
                    )),
                    midi2note(note.key_number as f64),
                );
            }
        }
        let ds_note = DsNote {
            lyric: get_symbol_removed_lyric(&note.lyric),
            ds_phoneme,
            note_name: midi2note(note.key_number as f64),
            duration: round_ndigits(cur_end_in_secs - cur_start_in_secs, 6),
        };
        ds_notes.push(ds_note.clone());

        prev_end_in_ticks = cur_end_in_ticks;
        prev_actual_end_in_secs = cur_actual_end_in_secs;
        prev_phoneme = ds_note.ds_phoneme;
    }

    insert_end_rest_note(&mut ds_notes, trailing_space);
    Ok(ds_notes)
}

/// `insert_end_rest_note`.
fn insert_end_rest_note(ds_notes: &mut Vec<DsNote>, trailing_space: f64) {
    ds_notes.push(rest_note(trailing_space, trailing_space));
}

/// `DsProjectModel`.
#[derive(Clone, Debug, Default)]
pub struct DsProjectModel {
    pub note_list: Vec<DsNote>,
    pub pitch_param_curve: DsParamCurve,
    pub gender_param_curve: DsParamCurve,
}

impl DsProjectModel {
    /// `to_param_model`.
    pub fn to_param_model(&self) -> DsItem {
        let mut input_text = String::new();
        let mut phoneme_seq = String::new();
        let mut phoneme_counts: Vec<i64> = Vec::new();
        let mut input_note_seq = String::new();
        let mut input_duration = String::new();
        let mut input_slur = String::new();
        let mut is_slur_seq = String::new();
        let mut phoneme_dur_seq = String::new();
        let mut phoneme_dur_enabled = true;
        let note_count = self.note_list.len();
        for (i, cur_note) in self.note_list.iter().enumerate() {
            let is_slur = cur_note.is_slur();
            let ds_phoneme = &cur_note.ds_phoneme;
            let vowel = &ds_phoneme.vowel;
            input_text.push_str(&cur_note.lyric.replace('-', ""));
            if !is_slur {
                phoneme_counts.push(1);
            }
            if let Some(consonant) = ds_phoneme.consonant.as_ref() {
                phoneme_seq.push_str(&consonant.phoneme);
                phoneme_seq.push(' ');
                if let Some(duration) = consonant.duration {
                    phoneme_dur_seq.push_str(&py_float(duration));
                    phoneme_dur_seq.push(' ');
                } else if phoneme_dur_enabled {
                    phoneme_dur_enabled = false;
                }
                is_slur_seq.push_str("0 ");
                if let Some(last) = phoneme_counts.last_mut() {
                    *last += 1;
                }
            }
            if !is_slur {
                phoneme_seq.push_str(&vowel.phoneme);
                phoneme_dur_seq.push_str(&py_opt_float(vowel.duration));
            }
            input_note_seq.push_str(&vowel.note_name);
            input_duration.push_str(&py_float(cur_note.duration));
            is_slur_seq.push_str(if is_slur { "1" } else { "0" });
            input_slur.push_str(if is_slur { "1" } else { "0" });
            if i < note_count - 1 {
                if !is_slur {
                    input_text.push(' ');
                }
                input_note_seq.push(' ');
                input_duration.push(' ');
                input_slur.push(' ');
                is_slur_seq.push(' ');
                phoneme_seq.push(' ');
                phoneme_dur_seq.push(' ');
            }
        }
        if !phoneme_counts.is_empty() {
            phoneme_counts.rotate_left(1);
        }

        let pitch_points = self
            .pitch_param_curve
            .point_list
            .as_deref()
            .unwrap_or_default();
        let f0_seq = if pitch_points.is_empty() {
            None
        } else {
            Some(
                pitch_points
                    .iter()
                    .map(|point| format!("{:.1}", point.value).parse::<f64>().unwrap_or(0.0))
                    .collect::<Vec<f64>>(),
            )
        };

        let gender_points = self
            .gender_param_curve
            .point_list
            .as_deref()
            .unwrap_or_default();
        let gender = if gender_points.is_empty() {
            None
        } else {
            Some(
                gender_points
                    .iter()
                    .map(|point| format!("{:.1}", point.value).parse::<f64>().unwrap_or(0.0))
                    .collect::<Vec<f64>>(),
            )
        };

        let ph_dur = if phoneme_dur_enabled {
            Some(
                phoneme_dur_seq
                    .split_whitespace()
                    .map(|part| part.parse::<f64>().unwrap_or(0.0))
                    .collect::<Vec<f64>>(),
            )
        } else {
            None
        };

        DsItem {
            text: input_text
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<String>>(),
            ph_seq: phoneme_seq
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<String>>(),
            note_seq: input_note_seq
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<String>>(),
            note_dur: Some(
                input_duration
                    .split_whitespace()
                    .map(|part| part.parse::<f64>().unwrap_or(0.0))
                    .collect(),
            ),
            note_dur_seq: Some(
                phoneme_dur_seq
                    .split_whitespace()
                    .map(|part| part.parse::<f64>().unwrap_or(0.0))
                    .collect(),
            ),
            note_slur: Some(
                input_slur
                    .split_whitespace()
                    .map(|part| part.parse::<i64>().unwrap_or(0))
                    .collect(),
            ),
            note_glide: None,
            is_slur_seq: Some(
                is_slur_seq
                    .split_whitespace()
                    .map(|part| part.parse::<i64>().unwrap_or(0))
                    .collect(),
            ),
            ph_dur,
            ph_num: Some(phoneme_counts),
            f0_timestep: Some(self.pitch_param_curve.step_size),
            f0_seq,
            input_type: Some("phoneme".to_owned()),
            offset: 0.0,
            seed: None,
            spk_mix: None,
            spk_mix_timestep: None,
            gender,
            gender_timestep: Some(self.gender_param_curve.step_size),
            velocity: None,
            velocity_timestep: None,
        }
    }
}

/// `DiffSingerGenerator` (`diffsinger_generator.py`) minus the plugin plumbing.
pub struct DiffSingerGenerator<'a> {
    pub phoneme_table: &'a HashMap<String, String>,
    pub export_gender: bool,
    pub trailing_space: f64,
}

impl DiffSingerGenerator<'_> {
    /// `generate`.
    pub fn generate(&self, project: &libresvip_core::model::base::Project) -> Result<DsItem> {
        let synchronizer = TimeSynchronizer::from_project(&project.song_tempo_list);
        let Some(singing_track) = project.singing_tracks().find(|track| !track.note_list.is_empty())
        else {
            return Err(LibreSvipError::msg("No singing track found"));
        };
        let ds_notes = encode_notes(
            &singing_track.note_list,
            &synchronizer,
            self.trailing_space,
            self.phoneme_table,
        )?;
        let total_duration = {
            let seconds: f64 = ds_notes.iter().map(|note| note.duration).sum();
            (seconds * 1000.0).ceil() as i64
        };
        let mut ds_project = DsProjectModel {
            note_list: ds_notes,
            pitch_param_curve: encode_pitch(&singing_track.edited_params.pitch, total_duration, 0.005),
            gender_param_curve: DsParamCurve::default(),
        };
        if self.export_gender {
            ds_project.gender_param_curve = encode_gender(
                &singing_track.edited_params.gender,
                total_duration,
                0.005,
            )?;
        }
        Ok(ds_project.to_param_model())
    }

    /// `generate` for a specific track index (`-1` = first non-empty singing
    /// track, as `DiffSingerGenerator` selects it).
    pub fn generate_for_track(
        &self,
        project: &libresvip_core::model::base::Project,
        track_index: i64,
    ) -> Result<DsItem> {
        if track_index < 0 {
            return self.generate(project);
        }
        let track = project
            .track_list
            .get(track_index as usize)
            .ok_or_else(|| LibreSvipError::msg("list index out of range"))?;
        let Some(singing_track) = track.as_singing() else {
            return Err(LibreSvipError::msg("No singing track found"));
        };
        let synchronizer = TimeSynchronizer::from_project(&project.song_tempo_list);
        let ds_notes = encode_notes(
            &singing_track.note_list,
            &synchronizer,
            self.trailing_space,
            self.phoneme_table,
        )?;
        let total_duration = {
            let seconds: f64 = ds_notes.iter().map(|note| note.duration).sum();
            (seconds * 1000.0).ceil() as i64
        };
        let mut ds_project = DsProjectModel {
            note_list: ds_notes,
            pitch_param_curve: encode_pitch(&singing_track.edited_params.pitch, total_duration, 0.005),
            gender_param_curve: DsParamCurve::default(),
        };
        if self.export_gender {
            ds_project.gender_param_curve = encode_gender(
                &singing_track.edited_params.gender,
                total_duration,
                0.005,
            )?;
        }
        Ok(ds_project.to_param_model())
    }
}

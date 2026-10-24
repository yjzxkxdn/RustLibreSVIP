//! Port of `libresvip.plugins.ds.diffsinger_parser`.

use std::sync::OnceLock;

use regex::Regex;

use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{
    Note, ParamCurve, Params, Project, SingingTrack, SongTempo, TimeSignature, Track,
};
use libresvip_core::model::point::Point;

use super::model::DsItem;
use super::music_math::{hz2midi, note2midi, py_round};

fn cents_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[+-]\d+$").expect("valid cents pattern"))
}

/// `more_itertools.split_before`: every group starts at an item whose value is
/// `0`; leading items before the first `0` form the first group.
fn split_before(items: &[i64]) -> Vec<Vec<(usize, i64)>> {
    let mut groups: Vec<Vec<(usize, i64)>> = Vec::new();
    for (index, value) in items.iter().copied().enumerate() {
        if value == 0 || groups.is_empty() {
            groups.push(Vec::new());
        }
        if let Some(group) = groups.last_mut() {
            group.push((index, value));
        }
    }
    groups
}

pub struct DiffSingerParser {
    pub tempo: f64,
    pub import_pitch: bool,
}

impl DiffSingerParser {
    /// `parse_project`.
    pub fn parse_project(&self, ds_items: &[DsItem]) -> Result<Project> {
        let song_tempo_list = vec![SongTempo::new(0, self.tempo)];
        let synchronizer = TimeSynchronizer::from_project(&song_tempo_list);
        let note_list = self.parse_notes(ds_items, &synchronizer)?;
        let pitch = self.parse_pitch(ds_items, &synchronizer)?;
        Ok(Project {
            warnings: Vec::new(),
            version: String::new(),
            song_tempo_list,
            time_signature_list: vec![TimeSignature::new(0, 4, 4)],
            track_list: vec![Track::Singing(SingingTrack {
                note_list,
                edited_params: Params {
                    pitch,
                    ..Default::default()
                },
                ..Default::default()
            })],
        })
    }

    /// `parse_notes`.
    fn parse_notes(
        &self,
        ds_items: &[DsItem],
        synchronizer: &TimeSynchronizer,
    ) -> Result<Vec<Note>> {
        let mut all_notes: Vec<Note> = Vec::new();
        for ds_item in ds_items {
            let mut notes: Vec<Note> = Vec::new();
            let mut cur_secs = ds_item.offset;
            let mut prev_is_breath = false;
            let note_slur = ds_item.note_slur.as_deref().unwrap_or_default();
            let groups = split_before(note_slur);
            let note_dur = ds_item.note_dur.as_deref().unwrap_or_default();
            if !note_dur.is_empty() {
                for (lyric_index, slur_group) in groups.iter().enumerate() {
                    let text = ds_item.text.get(lyric_index).ok_or_else(|| {
                        LibreSvipError::msg("list index out of range")
                    })?;
                    for (note_index, is_slur) in slur_group {
                        let note_dur_value = *note_dur.get(*note_index).ok_or_else(|| {
                            LibreSvipError::msg("list index out of range")
                        })?;
                        let note = ds_item.note_seq.get(*note_index).ok_or_else(|| {
                            LibreSvipError::msg("list index out of range")
                        })?;
                        let cur_time = synchronizer.get_actual_ticks_from_secs(cur_secs);
                        let next_time =
                            synchronizer.get_actual_ticks_from_secs(cur_secs + note_dur_value);
                        if text == "SP" {
                            // Silence: no note.
                        } else if text == "AP" {
                            prev_is_breath = true;
                        } else {
                            let midi_key = note2midi(cents_re().replace(note, "").as_ref())?;
                            if *is_slur == 0 {
                                notes.push(Note {
                                    start_pos: cur_time as i64,
                                    length: next_time as i64 - cur_time as i64,
                                    key_number: midi_key,
                                    lyric: text.clone(),
                                    head_tag: if prev_is_breath {
                                        Some("V".to_owned())
                                    } else {
                                        None
                                    },
                                    ..Default::default()
                                });
                                prev_is_breath = false;
                            } else {
                                notes.push(Note {
                                    start_pos: cur_time as i64,
                                    length: next_time as i64 - cur_time as i64,
                                    key_number: midi_key,
                                    lyric: "-".to_owned(),
                                    ..Default::default()
                                });
                            }
                        }
                        cur_secs += note_dur_value;
                    }
                }
            }
            all_notes.extend(notes);
        }
        Ok(all_notes)
    }

    /// `parse_pitch`.
    fn parse_pitch(
        &self,
        ds_items: &[DsItem],
        synchronizer: &TimeSynchronizer,
    ) -> Result<ParamCurve> {
        let mut points = vec![Point::start_point()];
        if self.import_pitch {
            for ds_item in ds_items {
                let (Some(f0_timestep), Some(f0_seq)) =
                    (ds_item.f0_timestep, ds_item.f0_seq.as_ref())
                else {
                    continue;
                };
                let offset = ds_item.offset;
                points.push(Point::new(
                    py_round(synchronizer.get_actual_ticks_from_secs(offset)) + 1920,
                    -100,
                ));
                for (i, f0) in f0_seq.iter().enumerate() {
                    let ticks = synchronizer
                        .get_actual_ticks_from_secs(offset + f0_timestep * i as f64);
                    points.push(Point::new(
                        py_round(ticks) + 1920,
                        py_round(hz2midi(*f0)? * 100.0),
                    ));
                }
                let last_ticks = synchronizer
                    .get_actual_ticks_from_secs(offset + f0_timestep * (f0_seq.len() as f64 - 1.0));
                points.push(Point::new(py_round(last_ticks) + 1920, -100));
            }
        }
        points.push(Point::end_point());
        Ok(ParamCurve::new(points))
    }
}

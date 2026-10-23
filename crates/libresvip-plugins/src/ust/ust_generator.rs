//! Port of `libresvip.plugins.ust.ust_generator`.

use libresvip_core::model::base::{Project, SingingTrack, SongTempo, Track, DEFAULT_BPM};
use libresvip_core::{LibreSvipError, Result};

use super::model::{ListValue, Note, PitchBendType, Track as UtauTrack, UtauProject};
use super::pitch1::pitch_to_utau_mode1_track;
use super::pitch2::{bpm_for_note, pitch_to_utau_mode2_track};

/// Port of `USTGenerator`.
pub struct UstGenerator {
    pub version: f64,
    pub track_index: i64,
    pub encoding: String,
}

impl UstGenerator {
    pub fn generate_project(&self, project: &Project) -> Result<UtauProject> {
        let tempo_list: Vec<SongTempo> = if project.song_tempo_list.is_empty() {
            vec![SongTempo::default()]
        } else {
            project.song_tempo_list.clone()
        };
        let tempo = tempo_list
            .first()
            .map(|tempo| tempo.bpm)
            .unwrap_or(DEFAULT_BPM);
        let ust_tracks: Vec<UtauTrack>;
        if self.version < 2.0 {
            let first_singing_track: Option<&SingingTrack> = if self.track_index < 0 {
                project
                    .track_list
                    .iter()
                    .filter_map(Track::as_singing)
                    .find(|track| !track.note_list.is_empty())
            } else {
                let index = self.track_index as usize;
                match project.track_list.get(index) {
                    Some(track) => track.as_singing(),
                    None => {
                        return Err(LibreSvipError::msg(format!(
                            "list index out of range: {}",
                            self.track_index
                        )))
                    }
                }
            };
            let Some(first_singing_track) = first_singing_track else {
                return Err(LibreSvipError::NoTrack);
            };
            ust_tracks = vec![self.generate_track(first_singing_track, &tempo_list)?];
        } else {
            let singing_tracks: Vec<&SingingTrack> =
                project.track_list.iter().filter_map(Track::as_singing).collect();
            if singing_tracks.is_empty() {
                return Err(LibreSvipError::NoTrack);
            }
            let mut tracks = Vec::with_capacity(singing_tracks.len());
            for track in singing_tracks {
                tracks.push(self.generate_track(track, &tempo_list)?);
            }
            ust_tracks = tracks;
        }
        Ok(UtauProject {
            charset: Some(self.encoding.clone()),
            ust_version: Some(self.version),
            tempo: Some(tempo),
            track: ust_tracks,
            pitch_mode2: true,
            ..Default::default()
        })
    }

    fn generate_track(&self, track: &SingingTrack, tempo_list: &[SongTempo]) -> Result<UtauTrack> {
        let mode1_track_pitch_data = pitch_to_utau_mode1_track(&track.edited_params.pitch, &track.note_list);
        let mode2_track_pitch_data =
            pitch_to_utau_mode2_track(&track.edited_params.pitch, &track.note_list, tempo_list)?;
        let mut utau_notes: Vec<Note> = Vec::new();
        let mut prev_bpm = tempo_list.first().map(|tempo| tempo.bpm).unwrap_or(DEFAULT_BPM);
        let mut prev_end_pos: Option<i64> = None;
        let mut note_index: i64 = 0;
        for ((note, mode1_pitch), mode2_pitch) in track
            .note_list
            .iter()
            .zip(mode1_track_pitch_data.notes.iter())
            .zip(mode2_track_pitch_data.notes.iter())
        {
            let rest_length = note.start_pos - prev_end_pos.unwrap_or(0);
            if rest_length != 0 {
                utau_notes.push(Note {
                    note_type: format!("{note_index:04}"),
                    lyric: "R".to_owned(),
                    length: rest_length,
                    note_num: 60,
                    ..Default::default()
                });
                note_index += 1;
            }
            let cur_bpm = bpm_for_note(tempo_list, note);
            let mut utau_note = Note {
                note_type: format!("{note_index:04}"),
                lyric: note.lyric.clone(),
                length: note.length,
                note_num: note.key_number,
                pitch_bend_points: mode1_pitch
                    .as_ref()
                    .map(|data| data.pitch_points.clone())
                    .unwrap_or_default(),
                pitchbend_type: Some(PitchBendType::Five),
                pitchbend_start: Some(0.0),
                ..Default::default()
            };
            if let Some(mode2_pitch) = mode2_pitch {
                if let Some(start) = mode2_pitch.start {
                    utau_note.pbs = vec![ListValue::Float(start)];
                }
                if !mode2_pitch.widths.is_empty() {
                    utau_note.pbw = std::iter::once(ListValue::Int(1))
                        .chain(mode2_pitch.widths.iter().map(|value| ListValue::Float(*value)))
                        .collect();
                }
                if let Some(start_shift) = mode2_pitch.start_shift {
                    utau_note.pby = vec![ListValue::Float(start_shift)];
                }
                if !mode2_pitch.shifts.is_empty() {
                    utau_note.pby.extend(
                        mode2_pitch
                            .shifts
                            .iter()
                            .map(|value| ListValue::Float(*value)),
                    );
                }
                if !mode2_pitch.curve_types.is_empty() {
                    utau_note.pbm = mode2_pitch.curve_types.clone();
                }
                if let Some(vibrato_params) = mode2_pitch.vibrato_params.clone() {
                    utau_note.vbr = Some(vibrato_params);
                }
            }
            if cur_bpm != prev_bpm {
                utau_note.tempo = Some(cur_bpm);
                prev_bpm = cur_bpm;
            }
            utau_notes.push(utau_note);
            prev_end_pos = Some(note.end_pos());
            note_index += 1;
        }
        Ok(UtauTrack { notes: utau_notes })
    }
}

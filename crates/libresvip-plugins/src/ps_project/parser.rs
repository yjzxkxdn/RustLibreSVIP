//! Port of `libresvip.plugins.ps_project.pocket_singer_parser`.

use std::path::Path;

use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{
    InstrumentalTrack, Note, Phones, Project, SingingTrack, SongTempo, TimeSignature, Track,
};
use libresvip_core::model::point::Point;
use libresvip_core::options::Options;

use super::model::{PocketSingerNote, PocketSingerProject};
use crate::ppsf::util::py_round;

pub struct PocketSingerParser<'a> {
    options: &'a Options,
    path: &'a Path,
    project: PocketSingerProject,
    first_bar_length: i64,
    synchronizer: TimeSynchronizer,
}

impl<'a> PocketSingerParser<'a> {
    pub fn new(options: &'a Options, path: &'a Path, project: PocketSingerProject) -> Self {
        Self {
            options,
            path,
            project,
            first_bar_length: 0,
            synchronizer: TimeSynchronizer::from_project(&[SongTempo::default()]),
        }
    }

    pub fn parse_project(mut self) -> Result<Project> {
        let song_tempo_list = self.parse_tempos();
        self.synchronizer = TimeSynchronizer::from_project(&song_tempo_list);
        let time_signature_list = self.parse_time_signatures();
        let mut track_list: Vec<Track> = self
            .parse_singing_tracks()
            .into_iter()
            .map(Track::Singing)
            .collect();
        track_list.extend(
            self.parse_instrumental_tracks()?
                .into_iter()
                .map(Track::Instrumental),
        );
        Ok(Project {
            version: String::new(),
            song_tempo_list,
            time_signature_list,
            track_list,
        })
    }

    fn parse_tempos(&self) -> Vec<SongTempo> {
        vec![SongTempo::new(0, self.project.song_info.bpm)]
    }

    fn parse_time_signatures(&mut self) -> Vec<TimeSignature> {
        let time_signatures = vec![TimeSignature::new(
            0,
            self.project.song_info.beat_of_bar,
            4,
        )];
        self.first_bar_length = py_round(time_signatures[0].bar_length());
        time_signatures
    }

    fn parse_singing_tracks(&self) -> Vec<SingingTrack> {
        let mut singing_tracks = Vec::new();
        for track in &self.project.tracks {
            let (notes, mut pitch_points) = self.parse_notes(&track.notes);
            let mut singing_track = SingingTrack {
                mute: track.mute,
                solo: track.solo,
                pan: track.pan,
                ai_singer_name: track.role_info.name.clone(),
                note_list: notes,
                ..Default::default()
            };
            if self.options.bool("import_pitch", true) {
                pitch_points.insert(0, Point::start_point());
                pitch_points.push(Point::end_point());
                singing_track.edited_params.pitch.points = pitch_points;
            }
            singing_tracks.push(singing_track);
        }
        singing_tracks
    }

    fn parse_notes(&self, ps_notes: &[PocketSingerNote]) -> (Vec<Note>, Vec<Point>) {
        let mut notes = Vec::new();
        let mut pitch_points = Vec::new();
        for ps_note in ps_notes {
            let start_pos = self
                .synchronizer
                .get_actual_ticks_from_secs(ps_note.start_time) as i64;
            let end_pos = self
                .synchronizer
                .get_actual_ticks_from_secs(ps_note.end_time) as i64;
            let real_pitch = if self.project.version < 3 {
                ps_note.pitch - 12
            } else {
                ps_note.pitch
            };
            let mut note = Note {
                lyric: if ps_note.grapheme_index == 0 {
                    ps_note.grapheme.clone()
                } else {
                    "+".to_owned()
                },
                key_number: real_pitch,
                start_pos,
                length: end_pos - start_pos,
                ..Default::default()
            };
            if ps_note.br_note.is_some() {
                note.head_tag = Some("V".to_owned());
            }
            if let Some(consonant_time_head) = ps_note
                .consonant_time_head
                .as_ref()
                .and_then(|values| values.first())
            {
                note.edited_phones = Some(Phones {
                    head_length_in_secs: *consonant_time_head,
                    ..Default::default()
                });
            }
            if self.options.bool("import_pitch", true) {
                let pitch_bends: &[super::model::PocketSingerPitchBend] = if !ps_note
                    .pitch_bends
                    .is_empty()
                {
                    &ps_note.pitch_bends
                } else if let Some(user_pitch) = ps_note.user_pitch.as_deref() {
                    if !user_pitch.is_empty() {
                        user_pitch
                    } else {
                        &[]
                    }
                } else {
                    &[]
                };
                let mut note_pitch_points: Vec<Point> = pitch_bends
                    .iter()
                    .map(|pitch_bend| Point::new(
                        (self
                            .synchronizer
                            .get_actual_ticks_from_secs(pitch_bend.time)
                            + self.first_bar_length as f64) as i64,
                        ((pitch_bend.pitch + real_pitch as f64) * 100.0) as i64,
                    ))
                    .collect();
                if let Some(last) = note_pitch_points.last().copied() {
                    if last.x < end_pos + self.first_bar_length {
                        note_pitch_points.push(Point::new(
                            end_pos + self.first_bar_length,
                            last.y,
                        ));
                    }
                }
                if !note_pitch_points.is_empty() {
                    let first_x = note_pitch_points[0].x;
                    note_pitch_points.insert(0, Point::new(first_x, -100));
                    note_pitch_points
                        .push(Point::new(end_pos + self.first_bar_length, -100));
                    pitch_points.extend(note_pitch_points);
                }
            }
            notes.push(note);
        }
        (notes, pitch_points)
    }

    fn parse_instrumental_tracks(&self) -> Result<Vec<InstrumentalTrack>> {
        let mut instrumental_tracks = Vec::new();
        if self.options.bool("import_instrumental_track", true) {
            for bgm_track in &self.project.bgm_info.tracks {
                let audio_path = self.path.parent().unwrap_or(self.path).join(format!(
                    "{}.{}",
                    bgm_track.file_name, bgm_track.file_type
                ));
                if audio_path.exists() {
                    // `InstrumentalTrack.offset` is an int in pydantic, which
                    // rejects a fractional value instead of truncating it.
                    let offset = self
                        .synchronizer
                        .get_actual_ticks_from_secs(bgm_track.start_time);
                    if offset.fract() != 0.0 {
                        return Err(LibreSvipError::Validation(format!(
                            "offset: Input should be a valid integer, got {offset}"
                        )));
                    }
                    instrumental_tracks.push(InstrumentalTrack {
                        title: bgm_track.file_name.clone(),
                        mute: self.project.bgm_info.mute,
                        solo: self.project.bgm_info.solo,
                        audio_file_path: audio_path.to_string_lossy().into_owned(),
                        offset: offset as i64,
                        ..Default::default()
                    });
                }
            }
        }
        Ok(instrumental_tracks)
    }
}

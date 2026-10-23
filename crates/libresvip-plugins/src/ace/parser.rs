//! Port of `libresvip.plugins.ace.ace_parser`.

use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use libresvip_core::core::constants::DEFAULT_BPM;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::Result;
use libresvip_core::model::base::{
    InstrumentalTrack, Note, Phones, Project, SingingTrack, SongTempo, TimeSignature, Track,
};
use libresvip_core::model::point::Point;
use libresvip_core::options::Options;

use super::model::{AceBgmTrack, AceNote, AceProject, AceTrack};
use super::pyutil::{first_pinyin, is_close_abs, py_round};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BpmSource {
    Automatic,
    SongInfo,
    BgmInfo,
}

pub struct AceParser {
    import_strength: bool,
    import_pitch: bool,
    import_instrumental: bool,
    bpm_source: BpmSource,
    path: PathBuf,
    synchronizer: TimeSynchronizer,
    first_bar_length: i64,
    project_version: i64,
}

impl AceParser {
    pub fn new(options: &Options, path: &Path) -> Self {
        let bpm_source = match options.str("bpm_source", "automatic").as_str() {
            "song_info" => BpmSource::SongInfo,
            "bgm_info" => BpmSource::BgmInfo,
            _ => BpmSource::Automatic,
        };
        Self {
            import_strength: options.bool("import_strength", true),
            import_pitch: options.bool("import_pitch", true),
            import_instrumental: options.bool("import_instrumental_track", true),
            bpm_source,
            path: path.to_path_buf(),
            synchronizer: TimeSynchronizer::from_project(&[]),
            first_bar_length: 0,
            project_version: 2,
        }
    }

    pub fn parse_project(&mut self, ace_project: &AceProject) -> Result<Project> {
        self.project_version = ace_project.version;
        let bpm = self.select_bpm(ace_project);
        let tempos = vec![SongTempo {
            position: 0,
            bpm,
        }];
        let time_signature = TimeSignature {
            bar_index: 0,
            numerator: ace_project.song_info.beat_of_bar.max(1),
            denominator: 4,
        };
        self.first_bar_length = py_round(time_signature.bar_length());
        self.synchronizer = TimeSynchronizer::from_project(&tempos);
        let mut track_list = self.parse_singing_tracks(&ace_project.singing_tracks())?;
        track_list.extend(self.parse_instrumental_tracks(ace_project));
        Ok(Project {
            song_tempo_list: tempos,
            time_signature_list: vec![time_signature],
            track_list,
            ..Default::default()
        })
    }

    fn select_bpm(&self, ace_project: &AceProject) -> f64 {
        let song_bpm = ace_project.song_info.bpm;
        let bgm_bpm = ace_project
            .bgm_info
            .tracks
            .iter()
            .filter_map(|track| track.bpm)
            .find(|bpm| *bpm > 0.0);
        let fallback = || {
            if song_bpm > 0.0 {
                song_bpm
            } else {
                DEFAULT_BPM
            }
        };
        match self.bpm_source {
            BpmSource::SongInfo => fallback(),
            BpmSource::BgmInfo | BpmSource::Automatic => bgm_bpm.unwrap_or_else(fallback),
        }
    }

    fn parse_singing_tracks(&self, ace_tracks: &[AceTrack]) -> Result<Vec<Track>> {
        let mut tracks = Vec::new();
        for ace_track in ace_tracks {
            let (notes, mut pitch_points, mut strength_points) =
                self.parse_notes(&ace_track.notes, &ace_track.br_notes)?;
            let mut track = SingingTrack {
                title: ace_track.role_info.name.clone(),
                mute: ace_track.mute,
                solo: ace_track.solo,
                pan: ace_track.pan,
                volume: ace_track.singer_volume,
                ai_singer_name: ace_track.role_info.name.clone(),
                note_list: notes,
                ..Default::default()
            };
            if self.import_pitch && !pitch_points.is_empty() {
                pitch_points.sort_by_key(|point| point.x);
                pitch_points.insert(0, Point::start_point());
                pitch_points.push(Point::end_point());
                track.edited_params.pitch = libresvip_core::model::base::ParamCurve::new(pitch_points);
            }
            if self.import_strength && !strength_points.is_empty() {
                strength_points.sort_by_key(|point| point.x);
                strength_points.insert(0, Point::start_point_with(0));
                strength_points.push(Point::end_point_with(0));
                track.edited_params.strength =
                    libresvip_core::model::base::ParamCurve::new(strength_points);
            }
            tracks.push(Track::Singing(track));
        }
        Ok(tracks)
    }

    fn parse_notes(
        &self,
        ace_notes: &[AceNote],
        breath_notes: &[super::model::AceBreathNote],
    ) -> Result<(Vec<Note>, Vec<Point>, Vec<Point>)> {
        let mut notes = Vec::new();
        let mut pitch_points = Vec::new();
        let mut strength_points: IndexMap<i64, Point> = IndexMap::new();
        let breath_end_times: Vec<f64> = breath_notes.iter().map(|note| note.base.end_time).collect();
        let mut sorted_notes: Vec<&AceNote> = ace_notes.iter().collect();
        sorted_notes.sort_by(|left, right| {
            left.base
                .start_time
                .partial_cmp(&right.base.start_time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for ace_note in sorted_notes {
            let real_pitch = if self.project_version < 3 {
                ace_note.pitch - 12
            } else {
                ace_note.pitch
            };
            let start_pos = py_round(
                self.synchronizer
                    .get_actual_ticks_from_secs(ace_note.base.start_time),
            );
            let mut end_pos = py_round(
                self.synchronizer
                    .get_actual_ticks_from_secs(ace_note.base.end_time),
            );
            if end_pos <= start_pos {
                end_pos = start_pos + 1;
            }
            let lyric = Self::select_lyric(ace_note);
            let pinyin = ace_note.pinyin.trim().to_owned();
            let head_tag = if ace_note.br
                || breath_end_times
                    .iter()
                    .any(|end_time| is_close_abs(ace_note.base.start_time, *end_time))
            {
                Some("V".to_owned())
            } else {
                None
            };
            let edited_phones = ace_note.consonant_time_abs.map(|head_length| Phones {
                head_length_in_secs: head_length,
                mid_ratio_over_tail: -1.0,
            });
            let note = Note {
                lyric: lyric.clone(),
                pronunciation: if !pinyin.is_empty() && lyric != pinyin {
                    Some(pinyin.clone())
                } else {
                    None
                },
                key_number: real_pitch,
                start_pos,
                length: end_pos - start_pos,
                head_tag,
                edited_phones,
                ..Default::default()
            };
            let user_pitch = ace_note
                .user_pitch
                .as_ref()
                .filter(|pitch| !pitch.is_empty());
            let source_pitch = user_pitch.unwrap_or(&ace_note.pitch_bends);
            if self.import_pitch && !source_pitch.is_empty() {
                let mut note_pitch_points: Vec<Point> = source_pitch
                    .iter()
                    .filter(|bend| bend.time <= ace_note.base.end_time)
                    .map(|bend| {
                        Point::new(
                            py_round(
                                self.synchronizer.get_actual_ticks_from_secs(bend.time)
                                    + self.first_bar_length as f64,
                            ),
                            py_round((real_pitch as f64 + bend.pitch) * 100.0),
                        )
                    })
                    .collect();
                if !note_pitch_points.is_empty() {
                    note_pitch_points.sort_by_key(|point| point.x);
                    let note_end_x = end_pos + self.first_bar_length;
                    if note_pitch_points[note_pitch_points.len() - 1].x < note_end_x {
                        note_pitch_points.push(Point::new(
                            note_end_x,
                            note_pitch_points[note_pitch_points.len() - 1].y,
                        ));
                    }
                    note_pitch_points.insert(0, Point::new(note_pitch_points[0].x, -100));
                    note_pitch_points.push(Point::new(note_end_x, -100));
                    pitch_points.extend(note_pitch_points);
                }
            }
            if self.import_strength {
                if let Some(envelope) = &ace_note.base.energy_envolope {
                    if !envelope.is_empty() {
                        let mut note_strength_points: Vec<Point> = envelope
                            .iter()
                            .map(|point| {
                                Point::new(
                                    py_round(
                                        self.synchronizer
                                            .get_actual_ticks_from_secs(point.time)
                                            + self.first_bar_length as f64,
                                    ),
                                    (-1000i64).max(
                                        1000i64.min(py_round((point.envolope - 1.0) * 1000.0)),
                                    ),
                                )
                            })
                            .collect();
                        note_strength_points.sort_by_key(|point| point.x);
                        let first_x = note_strength_points[0].x;
                        let last_x = note_strength_points[note_strength_points.len() - 1].x;
                        strength_points
                            .entry(first_x - 1)
                            .or_insert(Point::new(first_x - 1, 0));
                        for point in note_strength_points {
                            strength_points.insert(point.x, point);
                        }
                        strength_points
                            .entry(last_x + 1)
                            .or_insert(Point::new(last_x + 1, 0));
                    }
                }
            }
            notes.push(note);
        }
        Ok((notes, pitch_points, strength_points.into_values().collect()))
    }

    fn select_lyric(note: &AceNote) -> String {
        let word = note.word.trim();
        let pinyin = note.pinyin.trim();
        if !word.is_empty() && word.chars().any(|c| ('\u{3400}'..='\u{9fff}').contains(&c)) && !pinyin.is_empty()
        {
            if let Some(default_pinyin) = first_pinyin(word) {
                if pinyin.to_lowercase() != default_pinyin.to_lowercase() {
                    return pinyin.to_owned();
                }
            }
        }
        if !word.is_empty()
            && (word.is_ascii()
                || word.chars().any(|c| {
                    ('\u{3400}'..='\u{9fff}').contains(&c)
                        || ('\u{3040}'..='\u{30ff}').contains(&c)
                        || ('\u{ac00}'..='\u{d7af}').contains(&c)
                }))
        {
            return word.to_owned();
        }
        if !pinyin.is_empty() {
            pinyin.to_owned()
        } else if !word.is_empty() {
            word.to_owned()
        } else {
            "la".to_owned()
        }
    }

    fn parse_instrumental_tracks(&self, ace_project: &AceProject) -> Vec<Track> {
        if !self.import_instrumental {
            return Vec::new();
        }
        let mut result = Vec::new();
        for bgm_track in &ace_project.bgm_info.tracks {
            let Some(audio_path) = self.find_audio_path(bgm_track) else {
                continue;
            };
            let position = if bgm_track.position != 0.0 {
                bgm_track.position
            } else {
                bgm_track.start_time
            };
            result.push(Track::Instrumental(InstrumentalTrack {
                title: bgm_track.file_name.clone(),
                mute: ace_project.bgm_info.mute,
                solo: ace_project.bgm_info.solo,
                volume: ace_project.bgm_info.bgm_volume,
                audio_file_path: audio_path.to_string_lossy().into_owned(),
                offset: py_round(self.synchronizer.get_actual_ticks_from_secs(position)),
                ..Default::default()
            }));
        }
        result
    }

    fn find_audio_path(&self, bgm_track: &AceBgmTrack) -> Option<PathBuf> {
        let suffix = bgm_track.file_type.trim_start_matches('.');
        let parent = self.path.parent()?;
        let candidate = parent.join(format!("{}.{}", bgm_track.file_name, suffix));
        if candidate.is_file() {
            return Some(candidate);
        }
        if !bgm_track.file_md5.is_empty() {
            let candidate = parent
                .parent()?
                .join("local")
                .join("assets_res")
                .join("bgms")
                .join(format!("{}.{}", bgm_track.file_md5, suffix));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    }
}

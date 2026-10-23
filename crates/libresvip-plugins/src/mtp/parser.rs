//! Port of `libresvip.plugins.mtp.muta_parser`.

use libresvip_core::core::compat::find_last_index;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{
    InstrumentalTrack, Note, ParamCurve, Project, SingingTrack, SongTempo, TimeSignature, Track,
};
use libresvip_core::model::point::Point;

use super::model::{
    MutaNote, MutaPoint, MutaProject, MutaTempo, MutaTimeSignature, MutaTrackData, TRACK_TYPE_AUDIO,
    TRACK_TYPE_SONG,
};

/// `skip_beat_list` (local copy; see the note in `dv::parser`).
fn skip_beat_list(beat_list: &[TimeSignature], skip_bars: i64) -> Vec<TimeSignature> {
    let mut result: Vec<TimeSignature> = beat_list
        .iter()
        .filter(|beat| beat.bar_index >= skip_bars)
        .map(|beat| TimeSignature::new(beat.bar_index - skip_bars, beat.numerator, beat.denominator))
        .collect();
    if result.is_empty() || result[0].bar_index > 0 {
        if beat_list.is_empty() {
            result.insert(0, TimeSignature::default());
        } else {
            let index = find_last_index(beat_list, |beat| beat.bar_index <= skip_bars);
            let mut beat = if index < 0 {
                beat_list[beat_list.len() - 1].clone()
            } else {
                beat_list[index as usize].clone()
            };
            beat.bar_index = 0;
            result.insert(0, beat);
        }
    }
    result
}

pub struct MutaParser {
    pub import_instrumental_track: bool,
    pub import_pitch: bool,
}

fn decode_name(values: &[u16]) -> String {
    String::from_utf16_lossy(values)
        .trim_end_matches('\0')
        .to_owned()
}

impl MutaParser {
    /// `parse_project`.
    pub fn parse_project(&self, muta_project: &MutaProject) -> Result<Project> {
        let time_signatures = Self::parse_time_signatures(&muta_project.time_signatures);
        if time_signatures.is_empty() {
            return Err(LibreSvipError::msg("list index out of range"));
        }
        let first_bar_length = time_signatures[0].bar_length() as i64;
        let tempos = Self::parse_tempos(&muta_project.tempos, first_bar_length);
        let mut singing_tracks = Vec::new();
        for muta_track in &muta_project.tracks {
            if muta_track.track_type != TRACK_TYPE_SONG {
                continue;
            }
            let MutaTrackData::Song(parts) = &muta_track.data else {
                continue;
            };
            for part in parts {
                let mut singing_track = SingingTrack {
                    title: muta_track.name.clone(),
                    ai_singer_name: decode_name(&part.singer_name),
                    mute: muta_track.mute != 0,
                    solo: muta_track.solo != 0,
                    note_list: Self::parse_notes(&part.notes, part.start),
                    ..Default::default()
                };
                if self.import_pitch {
                    if let Some(pitch) = self.parse_pitch(&part.params.pitch_data, part.start, first_bar_length)
                    {
                        singing_track.edited_params.pitch = pitch;
                    }
                }
                singing_tracks.push(Track::Singing(singing_track));
            }
        }
        let instrumental_tracks = self.parse_instrumental_tracks(muta_project);
        let mut track_list = singing_tracks;
        track_list.extend(instrumental_tracks);
        Ok(Project {
            version: String::new(),
            time_signature_list: time_signatures,
            song_tempo_list: tempos,
            track_list,
        })
    }

    /// `parse_time_signatures`.
    fn parse_time_signatures(muta_time_signatures: &[MutaTimeSignature]) -> Vec<TimeSignature> {
        let time_signatures: Vec<TimeSignature> = muta_time_signatures
            .iter()
            .map(|signature| {
                TimeSignature::new(
                    signature.measure_position as i64 + 1,
                    signature.numerator as i64,
                    signature.denominator as i64,
                )
            })
            .collect();
        skip_beat_list(&time_signatures, 0)
    }

    /// `parse_tempos`.
    fn parse_tempos(muta_tempos: &[MutaTempo], first_bar_length: i64) -> Vec<SongTempo> {
        muta_tempos
            .iter()
            .map(|tempo| {
                SongTempo::new(
                    tempo.position as i64 + first_bar_length,
                    tempo.bpm as f64 / 100.0,
                )
            })
            .collect()
    }

    /// `parse_pitch`.
    fn parse_pitch(
        &self,
        muta_pitch: &[MutaPoint],
        tick_offset: u32,
        first_bar_length: i64,
    ) -> Option<ParamCurve> {
        let mut pitch_points = vec![Point::start_point()];
        for muta_point in muta_pitch {
            pitch_points.push(Point::new(
                muta_point.time as i64 + tick_offset as i64 + first_bar_length,
                if 0 < muta_point.value && muta_point.value < 12900 {
                    muta_point.value as i64 + 1200
                } else {
                    -100
                },
            ));
        }
        pitch_points.push(Point::end_point());
        if pitch_points.len() > 2 {
            Some(ParamCurve::new(pitch_points))
        } else {
            None
        }
    }

    /// `parse_instrumental_tracks`.
    fn parse_instrumental_tracks(&self, muta_project: &MutaProject) -> Vec<Track> {
        let mut track_list = Vec::new();
        if !self.import_instrumental_track {
            return track_list;
        }
        for muta_track in &muta_project.tracks {
            if muta_track.track_type != TRACK_TYPE_AUDIO {
                continue;
            }
            let MutaTrackData::Audio(parts) = &muta_track.data else {
                continue;
            };
            for part in parts {
                track_list.push(Track::Instrumental(InstrumentalTrack {
                    title: muta_track.name.clone(),
                    mute: muta_track.mute != 0,
                    solo: muta_track.solo != 0,
                    offset: part.start as i64,
                    audio_file_path: part.file_path.trim_end_matches('\0').to_owned(),
                    ..Default::default()
                }));
            }
        }
        track_list
    }

    /// `parse_notes`.
    fn parse_notes(muta_notes: &[MutaNote], tick_offset: u32) -> Vec<Note> {
        muta_notes
            .iter()
            .map(|muta_note| Note {
                start_pos: muta_note.start as i64 + tick_offset as i64,
                length: muta_note.length as i64,
                key_number: 139 - muta_note.key as i64,
                lyric: decode_name(&muta_note.lyric),
                pronunciation: Some(muta_note.phoneme.clone()),
                ..Default::default()
            })
            .collect()
    }
}

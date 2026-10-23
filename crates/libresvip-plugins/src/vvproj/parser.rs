//! Port of `libresvip.plugins.vvproj.voicevox_parser`.

use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{Note, Project, SingingTrack, SongTempo, TimeSignature, Track};
use libresvip_core::model::point::Point;

use super::model::{VoiceVoxProject, VoiceVoxTrack};
use super::music::{hz2midi, py_round, PiecewiseIntervalDict};

#[derive(Clone, Debug)]
pub struct InputOptions {
    pub import_pitch: bool,
}

impl Default for InputOptions {
    fn default() -> Self {
        Self { import_pitch: true }
    }
}

pub fn parse_project(project: &VoiceVoxProject, options: &InputOptions) -> Result<Project> {
    let song = &project.song;
    let time_signature_list: Vec<TimeSignature> = song
        .time_signatures
        .iter()
        .map(|time_signature| {
            TimeSignature::new(
                time_signature.measure_number - 1,
                time_signature.beats,
                time_signature.beat_type,
            )
        })
        .collect();
    let Some(first_signature) = time_signature_list.first() else {
        return Err(LibreSvipError::Validation(
            "project has no time signatures".to_owned(),
        ));
    };
    let first_bar_length = first_signature.bar_length_with(song.tpqn) as i64;
    let song_tempo_list: Vec<SongTempo> = song
        .tempos
        .iter()
        .map(|tempo| SongTempo::new(tempo.position, tempo.bpm as f64))
        .collect();
    let time_synchronizer = TimeSynchronizer::from_project(&song_tempo_list);
    let mut track_list = Vec::new();
    for track_key in &song.track_order {
        let track = song.track(track_key).ok_or_else(|| {
            LibreSvipError::Validation(format!("track `{track_key}` not found in the project"))
        })?;
        let singing_track = parse_track(
            track,
            options,
            &time_synchronizer,
            first_bar_length,
        );
        track_list.push(Track::Singing(singing_track));
    }
    Ok(Project {
        version: String::new(),
        song_tempo_list,
        time_signature_list,
        track_list,
    })
}

fn parse_track(
    track: &VoiceVoxTrack,
    options: &InputOptions,
    time_synchronizer: &TimeSynchronizer,
    first_bar_length: i64,
) -> SingingTrack {
    let mut singing_track = SingingTrack {
        title: track.name.clone(),
        mute: track.mute,
        solo: track.solo,
        volume: track.gain,
        pan: track.pan,
        note_list: track
            .notes
            .iter()
            .map(|note| {
                Note::new(note.position, note.duration, note.note_number, note.lyric.clone())
            })
            .collect(),
        ..Default::default()
    };
    if options.import_pitch {
        singing_track.edited_params.pitch.points =
            parse_pitch(&track.pitch_edit_data, time_synchronizer, first_bar_length);
    }
    singing_track
}

fn parse_pitch(
    pitch_edit_data: &[f64],
    time_synchronizer: &TimeSynchronizer,
    first_bar_length: i64,
) -> Vec<Point> {
    let mut secs = 0.0f64;
    let secs_step = 4.0 / 375.0;
    let mut points = vec![Point::start_point()];
    for part in PiecewiseIntervalDict::split_when(pitch_edit_data, |x, y| {
        usize::from(x == -1.0) + usize::from(y == -1.0) == 1
    }) {
        if part.iter().all(|value| *value == -1.0) {
            secs += secs_step * part.len() as f64;
        } else {
            points.push(Point::new(
                time_synchronizer.get_actual_ticks_from_secs(secs) as i64 + first_bar_length,
                -100,
            ));
            for value in &part {
                points.push(Point::new(
                    time_synchronizer.get_actual_ticks_from_secs(secs) as i64 + first_bar_length,
                    py_round(hz2midi(*value) * 100.0),
                ));
                secs += secs_step;
            }
            points.push(Point::new(
                time_synchronizer.get_actual_ticks_from_secs(secs) as i64 + first_bar_length,
                -100,
            ));
        }
    }
    points.push(Point::end_point());
    points
}

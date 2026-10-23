//! Port of `libresvip.model.reset_time_axis`.

use crate::core::constants::DEFAULT_BPM;
use crate::core::time_sync::TimeSynchronizer;
use crate::model::base::{ParamCurve, Params, Project, SongTempo, TimeSignature, Track};
use crate::model::point::Point;
use crate::Result;

#[inline]
fn py_round(value: f64) -> i64 {
    value.round_ties_even() as i64
}

fn remap_curve(
    curve: &ParamCurve,
    func: impl Fn(i64) -> f64,
    ori_first_bar_ticks: i64,
    new_first_bar_ticks: i64,
    limit_func: Option<impl Fn(i64) -> bool>,
) -> ParamCurve {
    let points = curve
        .points
        .iter()
        .filter_map(|point| {
            let shifted = point.x - ori_first_bar_ticks;
            if limit_func.as_ref().map(|limit| limit(shifted)).unwrap_or(true) {
                Some(Point::new(
                    py_round(func(shifted)) + new_first_bar_ticks,
                    point.y,
                ))
            } else {
                None
            }
        })
        .collect();
    ParamCurve::new(points)
}

fn remap_params(
    params: &Params,
    func: &impl Fn(i64) -> f64,
    ori_first_bar_ticks: i64,
    new_first_bar_ticks: i64,
) -> Params {
    Params {
        pitch: remap_curve(
            &params.pitch,
            func,
            ori_first_bar_ticks,
            new_first_bar_ticks,
            None::<fn(i64) -> bool>,
        ),
        volume: remap_curve(
            &params.volume,
            func,
            ori_first_bar_ticks,
            new_first_bar_ticks,
            None::<fn(i64) -> bool>,
        ),
        breath: remap_curve(
            &params.breath,
            func,
            ori_first_bar_ticks,
            new_first_bar_ticks,
            None::<fn(i64) -> bool>,
        ),
        gender: remap_curve(
            &params.gender,
            func,
            ori_first_bar_ticks,
            new_first_bar_ticks,
            None::<fn(i64) -> bool>,
        ),
        strength: remap_curve(
            &params.strength,
            func,
            ori_first_bar_ticks,
            new_first_bar_ticks,
            None::<fn(i64) -> bool>,
        ),
    }
}

/// `reset_time_axis`: flatten a dynamic-tempo project onto a constant tempo.
pub fn reset_time_axis(project: &Project, tempo: f64) -> Result<Project> {
    let synchronizer = TimeSynchronizer::new(&project.song_tempo_list, 0, true, tempo);
    let new_time_signature = TimeSignature::new(0, 4, 4);
    let ori_first = project
        .time_signature_list
        .first()
        .map(|signature| py_round(signature.bar_length()))
        .unwrap_or(0);
    let new_first = py_round(new_time_signature.bar_length());
    let func = |ticks: i64| synchronizer.get_actual_ticks_from_ticks(ticks);

    let mut new_track_list = Vec::with_capacity(project.track_list.len());
    for track in &project.track_list {
        match track {
            Track::Instrumental(instrumental) => {
                let mut updated = instrumental.clone();
                updated.offset = py_round(synchronizer.get_actual_ticks_from_ticks(instrumental.offset));
                new_track_list.push(Track::Instrumental(updated));
            }
            Track::Singing(singing) => {
                let mut updated = singing.clone();
                updated.note_list = singing
                    .note_list
                    .iter()
                    .map(|note| {
                        let note_end = py_round(synchronizer.get_actual_ticks_from_ticks(note.end_pos()));
                        let note_start = py_round(synchronizer.get_actual_ticks_from_ticks(note.start_pos));
                        let mut new_note = note.clone();
                        new_note.start_pos = note_start;
                        new_note.length = note_end - note_start;
                        new_note
                    })
                    .collect();
                updated.edited_params = remap_params(&singing.edited_params, &func, ori_first, new_first);
                new_track_list.push(Track::Singing(updated));
            }
        }
    }

    Ok(Project {
        version: String::new(),
        song_tempo_list: vec![SongTempo::new(0, tempo)],
        time_signature_list: vec![new_time_signature],
        track_list: new_track_list,
    })
}

/// `zoom_project`: scale tempos, notes and curves by `factor`.
pub fn zoom_project(project: &Project, factor: f64) -> Result<Project> {
    // `more_itertools.unique_in_window(..., 2, key=str)`: drop adjacent
    // duplicates (the key is the full model repr).
    let mut time_signature_list: Vec<TimeSignature> = Vec::new();
    for signature in &project.time_signature_list {
        if time_signature_list.last() != Some(signature) {
            let mut updated = signature.clone();
            updated.bar_index = py_round(signature.bar_index as f64 * factor);
            time_signature_list.push(updated);
        }
    }
    if time_signature_list.is_empty() {
        return Err(crate::LibreSvipError::msg("time_signature_list must not be empty"));
    }
    let new_first = py_round(time_signature_list[0].bar_length());
    let ori_first = project
        .time_signature_list
        .first()
        .map(|signature| py_round(signature.bar_length()))
        .unwrap_or(0);
    let func = |ticks: i64| factor * ticks as f64;

    let mut track_list = Vec::with_capacity(project.track_list.len());
    for track in &project.track_list {
        match track {
            Track::Instrumental(instrumental) => {
                let mut updated = instrumental.clone();
                updated.offset = py_round(factor * instrumental.offset as f64);
                track_list.push(Track::Instrumental(updated));
            }
            Track::Singing(singing) => {
                let mut updated = singing.clone();
                updated.note_list = singing
                    .note_list
                    .iter()
                    .map(|note| {
                        let mut new_note = note.clone();
                        new_note.start_pos = py_round(factor * note.start_pos as f64);
                        new_note.length = py_round(factor * note.length as f64);
                        new_note
                    })
                    .collect();
                updated.edited_params = remap_params(&singing.edited_params, &func, ori_first, new_first);
                track_list.push(Track::Singing(updated));
            }
        }
    }

    let mut result = project.clone();
    result.song_tempo_list = project
        .song_tempo_list
        .iter()
        .map(|tempo| SongTempo::new(py_round(tempo.position as f64 * factor), tempo.bpm * factor))
        .collect();
    result.time_signature_list = time_signature_list;
    result.track_list = track_list;
    Ok(result)
}

/// `limit_bars`: keep only the first `max_bars` bars.
pub fn limit_bars(project: &Project, max_bars: i64) -> Result<Project> {
    let time_signature_list: Vec<TimeSignature> = project
        .time_signature_list
        .iter()
        .filter(|signature| signature.bar_index < max_bars)
        .cloned()
        .collect();
    if time_signature_list.is_empty() {
        return Err(crate::LibreSvipError::msg("time_signature_list must not be empty"));
    }
    let first_bar_length = py_round(time_signature_list[0].bar_length());
    let max_ticks = max_bars * first_bar_length;

    let mut result = project.clone();
    result.song_tempo_list = project
        .song_tempo_list
        .iter()
        .filter(|tempo| tempo.position < max_ticks)
        .cloned()
        .collect();
    result.time_signature_list = time_signature_list;
    result.track_list = project
        .track_list
        .iter()
        .map(|track| match track {
            Track::Instrumental(_) => track.clone(),
            Track::Singing(singing) => {
                let mut updated = singing.clone();
                updated.note_list = singing
                    .note_list
                    .iter()
                    .filter(|note| note.end_pos() < max_ticks)
                    .cloned()
                    .collect();
                updated.edited_params = Params {
                    pitch: remap_curve(
                        &singing.edited_params.pitch,
                        |value| value as f64,
                        first_bar_length,
                        first_bar_length,
                        Some(move |value: i64| max_ticks > value),
                    ),
                    volume: remap_curve(
                        &singing.edited_params.volume,
                        |value| value as f64,
                        first_bar_length,
                        first_bar_length,
                        Some(move |value: i64| max_ticks > value),
                    ),
                    breath: remap_curve(
                        &singing.edited_params.breath,
                        |value| value as f64,
                        first_bar_length,
                        first_bar_length,
                        Some(move |value: i64| max_ticks > value),
                    ),
                    gender: remap_curve(
                        &singing.edited_params.gender,
                        |value| value as f64,
                        first_bar_length,
                        first_bar_length,
                        Some(move |value: i64| max_ticks > value),
                    ),
                    strength: remap_curve(
                        &singing.edited_params.strength,
                        |value| value as f64,
                        first_bar_length,
                        first_bar_length,
                        Some(move |value: i64| max_ticks > value),
                    ),
                };
                Track::Singing(updated)
            }
        })
        .collect();
    Ok(result)
}

/// Convenience wrapper used by plugins: reset to the default tempo.
pub fn reset_time_axis_default(project: &Project) -> Result<Project> {
    reset_time_axis(project, DEFAULT_BPM)
}

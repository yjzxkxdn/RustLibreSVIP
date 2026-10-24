//! Port of `libresvip.plugins.ds.utils.project_util.split_into_segments`.

use libresvip_core::error::Result;
use libresvip_core::model::base::{
    Note, ParamCurve, Params, Project, SingingTrack, SongTempo, TimeSignature, Track,
};
use libresvip_core::model::point::Point;
use libresvip_core::model::reset_time_axis::reset_time_axis;

/// `reset_time_axis(project)` uses its own default tempo of `120.0`.
const DEFAULT_RESET_TEMPO: f64 = 120.0;

fn segment_points(points: &[Point], seg_note_start_pos: i64, last_end: i64, prepare_space: i64) -> Vec<Point> {
    points
        .iter()
        .filter(|point| 1920 <= point.x - 1920 && point.x - 1920 <= last_end + 50)
        .map(|point| Point::new(point.x - seg_note_start_pos + prepare_space, point.y))
        .collect()
}

fn build_segment(
    notes: &[Note],
    buffer: &[usize],
    pitch_points: Vec<Point>,
    gender_points: Vec<Point>,
    song_tempo_list: &[SongTempo],
) -> Project {
    let segment_notes = buffer.iter().map(|index| notes[*index].clone()).collect();
    Project {
        warnings: Vec::new(),
        version: String::new(),
        song_tempo_list: song_tempo_list.to_vec(),
        time_signature_list: vec![TimeSignature::new(0, 4, 4)],
        track_list: vec![Track::Singing(SingingTrack {
            note_list: segment_notes,
            edited_params: Params {
                pitch: ParamCurve::new(pitch_points),
                gender: ParamCurve::new(gender_points),
                ..Default::default()
            },
            ..Default::default()
        })],
    }
}

/// `split_into_segments`: returns `(offset_secs, segment_project, trailing_secs)`
/// tuples. An empty vector mirrors Python's generator returning no items.
pub fn split_into_segments(
    project: &Project,
    min_interval: i64,
    min_length: i64,
    track_index: i64,
) -> Result<Vec<(f64, Project, f64)>> {
    let track = if track_index < 0 {
        project
            .singing_tracks()
            .find(|track| !track.note_list.is_empty())
    } else {
        let track = project
            .track_list
            .get(track_index as usize)
            .ok_or_else(|| libresvip_core::error::LibreSvipError::msg("list index out of range"))?;
        track.as_singing()
    };
    let Some(source) = track else {
        return Ok(Vec::new());
    };
    if source.note_list.is_empty() {
        return Ok(Vec::new());
    }

    // Python keeps a reference to the *original* track (pitch/gender curves are
    // not remapped by `reset_time_axis`) while the segment project uses the
    // reset tempo list.
    let source = source.clone();
    let reset_project = reset_time_axis(project.clone(), DEFAULT_RESET_TEMPO)?;
    let mut notes = source.note_list.clone();
    let mut segments: Vec<(f64, Project, f64)> = Vec::new();

    let mut buffer: Vec<usize> = vec![0];
    let mut cur_seg_start = (notes[0].start_pos - 600).max((notes[0].start_pos as f64 * 0.8) as i64);
    let mut cur_seg_interval = notes[0].start_pos;

    for i in 1..notes.len() {
        let interval = notes[i].start_pos - notes[i - 1].end_pos();
        if interval >= min_interval
            && (notes[i].start_pos as f64 - interval as f64 * 0.8 - cur_seg_start as f64)
                >= min_length as f64
        {
            let prepare_space = 600.min((cur_seg_interval as f64 * 0.8) as i64);
            let trailing_space = 400.min((cur_seg_interval as f64 * 0.2) as i64);
            let seg_note_start_pos = notes[buffer[0]].start_pos;
            let last_end = notes[buffer[buffer.len() - 1]].end_pos();
            let pitch_points =
                segment_points(&source.edited_params.pitch.points, seg_note_start_pos, last_end, prepare_space);
            let gender_points =
                segment_points(&source.edited_params.gender.points, seg_note_start_pos, last_end, prepare_space);
            for index in &buffer {
                notes[*index].start_pos =
                    notes[*index].start_pos - seg_note_start_pos + prepare_space;
            }
            let segment = build_segment(
                &notes,
                &buffer,
                pitch_points,
                gender_points,
                &reset_project.song_tempo_list,
            );
            segments.push((
                (seg_note_start_pos - prepare_space) as f64 / 1000.0,
                segment,
                trailing_space as f64 / 1000.0,
            ));

            cur_seg_start = notes[i].start_pos - 600.min((interval as f64 * 0.8) as i64);
            cur_seg_interval = interval;
            buffer.clear();
        }
        buffer.push(i);
    }

    let prepare_space = 600.min((cur_seg_interval as f64 * 0.8) as i64);
    let seg_note_start_pos = notes[buffer[0]].start_pos;
    let last_end = notes[buffer[buffer.len() - 1]].end_pos();
    let pitch_points =
        segment_points(&source.edited_params.pitch.points, seg_note_start_pos, last_end, prepare_space);
    let gender_points =
        segment_points(&source.edited_params.gender.points, seg_note_start_pos, last_end, prepare_space);
    for index in &buffer {
        notes[*index].start_pos = notes[*index].start_pos - seg_note_start_pos + prepare_space;
    }
    let segment = build_segment(
        &notes,
        &buffer,
        pitch_points,
        gender_points,
        &reset_project.song_tempo_list,
    );
    segments.push((
        (seg_note_start_pos - prepare_space) as f64 / 1000.0,
        segment,
        0.5,
    ));

    Ok(segments)
}

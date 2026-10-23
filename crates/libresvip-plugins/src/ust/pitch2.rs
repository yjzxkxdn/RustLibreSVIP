//! Port of `libresvip.plugins.ust.pitch_mode2`.

use libresvip_core::core::constants::TICKS_IN_BEAT;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::model::base::{Note, ParamCurve, SongTempo, TimeSignature};
use libresvip_core::model::point::Point;
use libresvip_core::{LibreSvipError, Result};

use super::constants::{MODE2_PITCH_MAX_POINT_COUNT, MODE2_PITCH_SAMPLING_INTERVAL_TICK};
use super::interpolation::interpolate;
use super::math::{div_floor, round_int};
use super::model::{NoteVibrato, PitchBendMode};
use super::pitch::{PitchSimulator, Portamento, RelativePitchCurve};
use super::rdp::simplify_shape_to;
use super::vibrato::append_utau_note_vibrato;

/// `milli_sec_from_tick`.
pub fn milli_sec_from_tick(tick: i64, bpm: f64) -> f64 {
    tick as f64 / TICKS_IN_BEAT as f64 * 60.0 / bpm * 1000.0
}

/// `UtauMode2NotePitchData`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mode2NotePitchData {
    pub bpm: f64,
    pub start: Option<f64>,
    pub start_shift: Option<f64>,
    pub widths: Vec<f64>,
    pub shifts: Vec<f64>,
    pub curve_types: Vec<PitchBendMode>,
    pub vibrato_params: Option<NoteVibrato>,
}

/// `UtauMode2TrackPitchData`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mode2TrackPitchData {
    pub notes: Vec<Option<Mode2NotePitchData>>,
}

/// `NotePitchData`.
struct NotePitchData {
    points: Vec<Point>,
    offset: i64,
    bpm: f64,
}

/// `bpm_for_note`.
pub fn bpm_for_note(tempos: &[SongTempo], note: &Note) -> f64 {
    let mut index: isize = -1;
    for (offset, tempo) in tempos.iter().enumerate() {
        if tempo.position <= note.start_pos {
            index = offset as isize;
        }
    }
    if index < 0 {
        index = tempos.len() as isize - 1;
    }
    tempos.get(index.max(0) as usize).map(|tempo| tempo.bpm).unwrap_or(120.0)
}

/// `pitch_to_utau_mode2_track`.
pub fn pitch_to_utau_mode2_track(
    pitch: &ParamCurve,
    notes: &[Note],
    tempos: &[SongTempo],
) -> Result<Mode2TrackPitchData> {
    let absolute_pitch: Vec<Point> = pitch
        .points
        .iter()
        .filter(|point| point.y != -100)
        .map(|point| Point::new(point.x - 1920, point.y))
        .collect();

    let to_relative = |from: &[Point], key: i64| -> Vec<Point> {
        from.iter()
            .map(|point| {
                let y = if point.y != 0 { point.y } else { key * 100 };
                Point::new(point.x, y - key * 100)
            })
            .collect()
    };

    if notes.is_empty() {
        // Python indexes `notes[0]` unconditionally.
        return Err(LibreSvipError::msg("list index out of range"));
    }

    let mut dot_pit_data: Vec<NotePitchData> = Vec::new();
    let first_note = &notes[0];
    let first_points: Vec<Point> = absolute_pitch
        .iter()
        .filter(|point| point.x < first_note.end_pos())
        .copied()
        .collect();
    let min_negative = absolute_pitch
        .iter()
        .filter(|point| point.x < 0)
        .map(|point| point.x)
        .min()
        .unwrap_or(0);
    dot_pit_data.push(NotePitchData {
        points: to_relative(&first_points, first_note.key_number),
        offset: -min_negative,
        bpm: bpm_for_note(tempos, first_note),
    });
    for note in &notes[1..] {
        let points: Vec<Point> = absolute_pitch
            .iter()
            .filter(|point| note.start_pos <= point.x && point.x < note.end_pos())
            .copied()
            .collect();
        let first_x = absolute_pitch
            .iter()
            .find(|point| point.x >= note.start_pos)
            .map(|point| point.x)
            .unwrap_or(note.start_pos);
        dot_pit_data.push(NotePitchData {
            points: to_relative(&points, note.key_number),
            offset: first_x - note.start_pos,
            bpm: bpm_for_note(tempos, note),
        });
    }

    let simplified: Vec<NotePitchData> = dot_pit_data
        .into_iter()
        .map(|note| NotePitchData {
            points: simplify_shape_to(&note.points, MODE2_PITCH_MAX_POINT_COUNT),
            offset: note.offset,
            bpm: note.bpm,
        })
        .collect();

    let notes_data: Vec<Option<Mode2NotePitchData>> = simplified
        .into_iter()
        .map(|current| {
            if current.points.is_empty() {
                return None;
            }
            let mut widths = Vec::new();
            for pair in current.points.windows(2) {
                widths.push(milli_sec_from_tick(
                    pair[1].x - pair[0].x,
                    current.bpm,
                ));
            }
            let shifts: Vec<f64> = current
                .points
                .iter()
                .skip(1)
                .map(|point| point.y as f64 / 10.0)
                .collect();
            Some(Mode2NotePitchData {
                bpm: current.bpm,
                start: Some(milli_sec_from_tick(current.offset, current.bpm)),
                start_shift: Some(current.points[0].y as f64 / 10.0),
                widths,
                shifts,
                curve_types: vec![PitchBendMode::Empty; current.points.len() - 1],
                vibrato_params: None,
            })
        })
        .collect();
    Ok(Mode2TrackPitchData { notes: notes_data })
}

/// `pitch_from_utau_mode2_track`.
pub fn pitch_from_utau_mode2_track(
    pitch_data: &Mode2TrackPitchData,
    tick_time_transformer: &TimeSynchronizer,
    notes: &[Note],
    time_signatures: &[TimeSignature],
) -> Result<ParamCurve> {
    let mut pitch_points: Vec<Point> = Vec::new();
    let mut last_note: Option<&Note> = None;
    let mut pending_pitch_points: Vec<Point> = Vec::new();
    for (note, note_pitch) in notes.iter().zip(pitch_data.notes.iter()) {
        if let Some(note_pitch) = note_pitch {
            let mut points: Vec<Point> = Vec::new();
            let note_start_in_millis =
                tick_time_transformer.get_actual_secs_from_ticks(note.start_pos) * 1000.0;
            if let Some(start) = note_pitch.start {
                let mut pos_in_millis = note_start_in_millis + start;
                let tick_pos =
                    tick_time_transformer.get_actual_ticks_from_secs(pos_in_millis / 1000.0);
                let start_shift = round_int(
                    if let Some(last_note) = last_note {
                        if note.start_pos == last_note.end_pos() {
                            ((last_note.key_number - note.key_number) * 100) as f64
                        } else if let Some(start_shift) = note_pitch.start_shift {
                            start_shift * 10.0
                        } else {
                            0.0
                        }
                    } else if let Some(start_shift) = note_pitch.start_shift {
                        start_shift * 10.0
                    } else {
                        0.0
                    },
                );
                points.push(Point::new(round_int(tick_pos), start_shift));
                let count = note_pitch
                    .widths
                    .len()
                    .max(note_pitch.shifts.len())
                    .max(note_pitch.curve_types.len());
                for index in 0..count {
                    let Some(width) = note_pitch.widths.get(index).copied() else {
                        break;
                    };
                    let shift = note_pitch.shifts.get(index).copied().unwrap_or(0.0);
                    let curve_type = note_pitch
                        .curve_types
                        .get(index)
                        .copied()
                        .unwrap_or(PitchBendMode::Empty);
                    pos_in_millis += width;
                    let tick_pos =
                        tick_time_transformer.get_actual_ticks_from_secs(pos_in_millis / 1000.0);
                    let this_point =
                        Point::new(round_int(tick_pos), round_int(shift * 10.0));
                    let last_point = *points.last().unwrap_or(&Point::default());
                    if this_point.x != last_point.x && this_point.y != last_point.y {
                        let interpolated_point_list = interpolate(
                            last_point,
                            this_point,
                            curve_type,
                            MODE2_PITCH_SAMPLING_INTERVAL_TICK,
                        );
                        points.extend(interpolated_point_list.iter().skip(1).copied());
                    } else {
                        points.push(this_point);
                    }
                }
            }
            let limit = points.first().map(|point| point.x).unwrap_or(i64::MAX);
            pitch_points.extend(
                pending_pitch_points
                    .iter()
                    .copied()
                    .filter(|point| point.x < limit),
            );
            let mut shaped = fix_points_at_last_note(&points, note, last_note);
            shaped = append_start_point(&shaped, note);
            shaped = append_end_point(&shaped, note);
            let shaped = append_utau_note_vibrato(
                &shaped,
                note_pitch.vibrato_params.as_ref(),
                note,
                tick_time_transformer,
                MODE2_PITCH_SAMPLING_INTERVAL_TICK,
            );
            pending_pitch_points = shape(&shaped);
        }
        last_note = Some(note);
    }
    pitch_points.extend(pending_pitch_points);
    let pitch_simulator = PitchSimulator::new(
        tick_time_transformer,
        Portamento::no_portamento(),
        notes,
        time_signatures,
    )?;
    let curve = RelativePitchCurve {
        lower_bound: notes.first().map(|note| note.start_pos).unwrap_or(0) as f64,
        upper_bound: notes.last().map(|note| note.end_pos()).unwrap_or(0) as f64,
        ..Default::default()
    };
    Ok(curve.to_absolute(&pitch_points, tick_time_transformer, &pitch_simulator))
}

/// `fix_points_at_last_note`.
fn fix_points_at_last_note(
    pitch_data: &[Point],
    this_note: &Note,
    last_note: Option<&Note>,
) -> Vec<Point> {
    let Some(last_note) = last_note else {
        return pitch_data.to_vec();
    };
    if last_note.end_pos() != this_note.start_pos {
        return pitch_data.to_vec();
    }
    let mut fixed: Vec<Point> = pitch_data
        .iter()
        .map(|point| {
            if point.x < this_note.start_pos {
                Point::new(
                    point.x,
                    point.y + (this_note.key_number - last_note.key_number) * 100,
                )
            } else {
                *point
            }
        })
        .collect();
    let last_point = fixed.last().copied();
    if let Some(last_point) = last_point {
        if last_point.x < this_note.start_pos {
            fixed.push(Point::new(this_note.start_pos, 0));
        }
    }
    fixed
}

/// `append_start_point`.
fn append_start_point(pitch_data: &[Point], this_note: &Note) -> Vec<Point> {
    match pitch_data.first().copied() {
        None => vec![Point::new(this_note.start_pos, 0)],
        Some(first_point) if first_point.x > this_note.start_pos => {
            let mut result = Vec::with_capacity(pitch_data.len() + 1);
            result.push(Point::new(this_note.start_pos, first_point.y));
            result.extend_from_slice(pitch_data);
            result
        }
        Some(_) => pitch_data.to_vec(),
    }
}

/// `append_end_point`.
fn append_end_point(pitch_data: &[Point], this_note: &Note) -> Vec<Point> {
    match pitch_data.last().copied() {
        None => vec![Point::new(this_note.end_pos(), 0)],
        Some(last_point) if last_point.x < this_note.end_pos() => {
            let mut result = pitch_data.to_vec();
            result.push(Point::new(this_note.end_pos(), last_point.y));
            result
        }
        Some(_) => pitch_data.to_vec(),
    }
}

/// `shape`: sort by x, then average points that share an x.
fn shape(pitch_data: &[Point]) -> Vec<Point> {
    let mut sorted_data = pitch_data.to_vec();
    sorted_data.sort_by_key(|point| point.x);
    let mut shaped_data: Vec<Point> = Vec::new();
    for point in sorted_data {
        if let Some(last) = shaped_data.last_mut() {
            if last.x == point.x {
                *last = Point::new(last.x, div_floor(last.y + point.y, 2));
                continue;
            }
        }
        shaped_data.push(point);
    }
    shaped_data
}

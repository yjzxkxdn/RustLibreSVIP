//! Port of `libresvip.plugins.dv.deepvocal_pitch`.

use std::collections::HashMap;

use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::model::base::{Note, ParamCurve, SongTempo};
use libresvip_core::model::point::Point;

use super::constants::{
    BEND_DOWN_LENGTH_FIXED_SEC, BEND_LENGTH_MAX_SEC, BEND_LENGTH_MIN_SEC, BEND_VALUE_MAX,
    NOTE_KEY_SUM, PORTAMENTO_LENGTH_MAX_SEC, SAMPLING_INTERVAL_TICK,
};
use super::model::DvPoint;

/// `DvSegmentPitchRawData`.
#[derive(Clone, Debug, Default)]
pub struct DvSegmentPitchRawData {
    pub tick_offset: i64,
    pub data: Vec<DvPoint>,
}

/// `DvNoteWithPitch`.
#[derive(Clone, Debug)]
pub struct DvNoteWithPitch {
    pub note: Note,
    pub por_head: i64,
    pub por_tail: i64,
    pub ben_len: i64,
    pub ben_dep: i64,
    pub vibrato: Vec<DvPoint>,
}

use libresvip_core::core::compat::py_round;

fn py_floor(value: f64) -> f64 {
    value.floor()
}

/// `convert_note_key(int)`.
#[inline]
pub fn convert_note_key_int(key: i64) -> i64 {
    NOTE_KEY_SUM as i64 - key
}

/// `convert_note_key(float)`.
#[inline]
pub fn convert_note_key_float(key: f64) -> f64 {
    NOTE_KEY_SUM - key
}

// ---------------------------------------------------------------------------
// Interpolation (subset of `libresvip.utils.music_math`)
// ---------------------------------------------------------------------------

#[inline]
fn linear_at(x: f64, start: (f64, f64), end: (f64, f64)) -> f64 {
    let (x0, y0) = start;
    let (x1, y1) = end;
    let r = (x - x0) / (x1 - x0);
    y0 + (y1 - y0) * r
}

#[inline]
fn cosine_easing_in_out_at(x: f64, start: (f64, f64), end: (f64, f64)) -> f64 {
    let (x0, y0) = start;
    let (x1, y1) = end;
    let r = (x - x0) / (x1 - x0);
    y0 + (y1 - y0) * ((1.0 - (r * std::f64::consts::PI).cos()) / 2.0)
}

fn inner_interpolate(
    data: &[Point],
    interval: i64,
    mapping: impl Fn(f64, (f64, f64), (f64, f64)) -> f64,
) -> Vec<Point> {
    if data.is_empty() {
        return Vec::new();
    }
    let mut result = Vec::new();
    result.push(data[0]);
    for pair in data.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        let start_f = (start.x as f64, start.y as f64);
        let end_f = (end.x as f64, end.y as f64);
        if interval > 0 {
            let mut x = start.x + 1;
            while x < end.x {
                result.push(Point::new(x, py_round(mapping(x as f64, start_f, end_f))));
                x += interval;
            }
        }
    }
    result.push(data[data.len() - 1]);
    result
}

fn interpolate_linear(data: &[Point], interval: i64) -> Vec<Point> {
    inner_interpolate(data, interval, linear_at)
}

fn interpolate_cosine_ease_in_out(data: &[Point], interval: i64) -> Vec<Point> {
    inner_interpolate(data, interval, cosine_easing_in_out_at)
}

// ---------------------------------------------------------------------------
// Merging
// ---------------------------------------------------------------------------

/// `merge_points_from_segments`.
pub fn merge_points_from_segments(segments: &[DvSegmentPitchRawData]) -> Option<Vec<Point>> {
    let mut points = Vec::new();
    for segment in segments {
        for dv_point in &segment.data {
            let raw_tick = dv_point.x as i64;
            if raw_tick >= 0 {
                let tick = raw_tick + segment.tick_offset;
                if dv_point.y >= 0 {
                    let value = py_round(convert_note_key_float(dv_point.y as f64 / 100.0) * 100.0);
                    points.push(Point::new(tick, value));
                } else {
                    points.push(Point::new(tick, -100));
                }
            }
        }
    }
    if points.is_empty() {
        None
    } else {
        Some(points)
    }
}

/// `merge_same_tick_points` (`itertools.groupby` on `x`).
pub fn merge_same_tick_points(points: &[Point]) -> Option<Vec<Point>> {
    let mut merged = Vec::new();
    let mut index = 0usize;
    while index < points.len() {
        let tick = points[index].x;
        let start = index;
        while index < points.len() && points[index].x == tick {
            index += 1;
        }
        let group = &points[start..index];
        if group.len() > 1 {
            if group.iter().any(|point| point.y == -100) {
                merged.push(Point::new(tick, -100));
            } else {
                let sum: i64 = group.iter().map(|point| point.y).sum();
                merged.push(Point::new(tick, py_round(sum as f64 / group.len() as f64)));
            }
        } else {
            merged.push(group[0]);
        }
    }
    if merged.is_empty() {
        None
    } else {
        Some(merged)
    }
}

/// `merge_same_value_points` (`itertools.groupby` on `y`, first of each group).
pub fn merge_same_value_points(points: &[Point]) -> Option<Vec<Point>> {
    let mut merged: Vec<Point> = Vec::new();
    for point in points {
        if merged.last().map(|last| last.y) != Some(point.y) {
            merged.push(*point);
        }
    }
    if merged.is_empty() {
        None
    } else {
        Some(merged)
    }
}

/// `tick_half_start`.
fn tick_half_start(note: &Note) -> i64 {
    (note.start_pos + note.end_pos()) / 2
}

fn note_end_pos(note: &Note) -> i64 {
    note.end_pos()
}

/// `get_portamento`.
fn get_portamento(
    last_note: &DvNoteWithPitch,
    transformer: &TimeSynchronizer,
    this_note: &DvNoteWithPitch,
) -> Vec<Point> {
    let tail_length_sec = py_floor(PORTAMENTO_LENGTH_MAX_SEC * last_note.por_tail as f64 / 100.0);
    let start_sec =
        transformer.get_actual_secs_from_ticks(note_end_pos(&last_note.note)) - tail_length_sec;
    let start_tick = transformer
        .get_actual_ticks_from_secs(start_sec)
        .max(tick_half_start(&last_note.note) as f64);

    let head_length_sec = py_floor(PORTAMENTO_LENGTH_MAX_SEC * this_note.por_head as f64 / 100.0);
    let end_sec =
        transformer.get_actual_secs_from_ticks(this_note.note.start_pos) + head_length_sec;
    let end_tick = transformer
        .get_actual_ticks_from_secs(end_sec)
        .min((tick_half_start(&this_note.note) - 1) as f64);

    interpolate_cosine_ease_in_out(
        &[
            Point::new(py_round(start_tick), last_note.note.key_number * 100),
            Point::new(py_round(end_tick), this_note.note.key_number * 100),
        ],
        1,
    )
}

/// `get_base_pitch`.
fn get_base_pitch(
    notes: &[DvNoteWithPitch],
    transformer: &TimeSynchronizer,
) -> HashMap<i64, i64> {
    let mut result = HashMap::new();
    for index in 0..=notes.len() {
        let last_note = if index == 0 {
            None
        } else {
            notes.get(index - 1)
        };
        let this_note = notes.get(index);
        let mut portamento: Vec<Point> = Vec::new();
        if let (Some(last_note), Some(this_note)) = (last_note, this_note) {
            portamento = get_portamento(last_note, transformer, this_note);
            for point in &portamento {
                result.insert(point.x, point.y);
            }
        }
        if let Some(last_note) = last_note {
            let end = portamento
                .first()
                .map_or_else(|| note_end_pos(&last_note.note), |point| point.x);
            let mut tick = tick_half_start(&last_note.note);
            while tick < end {
                result.insert(tick, last_note.note.key_number * 100);
                tick += 1;
            }
        }
        if let Some(this_note) = this_note {
            let start = if last_note.is_none() {
                0
            } else {
                portamento
                    .last()
                    .map_or(this_note.note.start_pos, |point| point.x)
            };
            let mut tick = start;
            while tick < tick_half_start(&this_note.note) {
                result.insert(tick, this_note.note.key_number * 100);
                tick += 1;
            }
        }
    }
    result
}

/// `get_bend_pitch`.
fn get_bend_pitch(
    notes: &[DvNoteWithPitch],
    transformer: &TimeSynchronizer,
) -> HashMap<i64, i64> {
    let mut result: HashMap<i64, i64> = HashMap::new();
    for note in notes {
        let start_tick = note.note.start_pos;
        let start_sec = transformer.get_actual_secs_from_ticks(start_tick);
        let valley_sec = start_sec + BEND_DOWN_LENGTH_FIXED_SEC;
        let valley_tick = transformer
            .get_actual_ticks_from_secs(valley_sec)
            .min((note.note.start_pos + note.note.length.div_euclid(2) - 1) as f64);

        let length_sec = if note.ben_len <= 50 {
            BEND_LENGTH_MIN_SEC
        } else {
            py_floor(
                (BEND_LENGTH_MAX_SEC - BEND_LENGTH_MIN_SEC) * (note.ben_len - 50) as f64 / 50.0,
            ) + BEND_LENGTH_MIN_SEC
        };
        let end_sec = start_sec + length_sec;
        let end_tick = transformer
            .get_actual_ticks_from_secs(end_sec)
            .min((note.note.start_pos - 1) as f64);

        let valley_value = -BEND_VALUE_MAX * note.ben_dep as f64;
        let valley_point = Point::new(py_round(valley_tick), py_round(valley_value));

        let bend_down = interpolate_linear(&[Point::new(start_tick, 0), valley_point], 1);
        let bend_up_all = interpolate_cosine_ease_in_out(
            &[valley_point, Point::new(py_round(end_tick), 0)],
            1,
        );
        for point in bend_down {
            result.insert(point.x, point.y);
        }
        if bend_up_all.len() > 1 {
            for point in &bend_up_all[1..] {
                result.insert(point.x, point.y);
            }
        }
    }
    result
}

/// `get_vibrato_pitch`.
fn get_vibrato_pitch(
    notes: &[DvNoteWithPitch],
    transformer: &TimeSynchronizer,
) -> HashMap<i64, i64> {
    let mut result: HashMap<i64, i64> = HashMap::new();
    for note in notes {
        let start_tick = note.note.start_pos;
        let start_sec = transformer.get_actual_secs_from_ticks(start_tick);
        let mut vibrato_points: Vec<Point> = Vec::new();
        for vib_point in &note.vibrato {
            let tick = transformer.get_actual_ticks_from_secs(start_sec + vib_point.x as f64 / 1000.0);
            if (start_tick as f64) <= tick && tick < note_end_pos(&note.note) as f64 {
                vibrato_points.push(Point::new(py_round(tick), -(vib_point.y as i64)));
            }
        }
        for point in interpolate_linear(&vibrato_points, 1) {
            result.insert(point.x, point.y);
        }
    }
    result
}

/// `apply_default_pitch`.
pub fn apply_default_pitch(
    first_bar_length: i64,
    points: &[Point],
    notes: &[DvNoteWithPitch],
    tempos: &[SongTempo],
) -> Vec<Point> {
    if points.is_empty() || notes.is_empty() {
        return points.to_vec();
    }

    let time_synchronizer = TimeSynchronizer::from_project(tempos);

    let base = get_base_pitch(notes, &time_synchronizer);
    let bend_diff = get_bend_pitch(notes, &time_synchronizer);
    let vibrato_diff = get_vibrato_pitch(notes, &time_synchronizer);
    let pitch_at = |tick: i64| -> i64 {
        base.get(&tick).copied().unwrap_or(0)
            + bend_diff.get(&tick).copied().unwrap_or(0)
            + vibrato_diff.get(&tick).copied().unwrap_or(0)
    };

    let mut result: Vec<Point> = Vec::new();
    let mut last_point: Option<Point> = None;
    for point in points {
        if let Some(last) = last_point {
            if last.y == -100 {
                let mut tick = last.x + 1;
                while tick < point.x {
                    result.push(Point::new(tick + first_bar_length, pitch_at(tick)));
                    tick += SAMPLING_INTERVAL_TICK;
                }
                result.push(Point::new(last.x + first_bar_length, -100));
            }
        }
        if point.y != -100 {
            result.push(Point::new(point.x + first_bar_length, point.y));
        } else {
            result.push(Point::new(point.x + first_bar_length, -100));
        }
        last_point = Some(*point);
    }
    let last_note_end = notes[notes.len() - 1].note.end_pos();
    if result.is_empty() || result[0].x > last_note_end {
        let mut prefix = Vec::new();
        let mut tick = 0i64;
        while tick < last_note_end {
            prefix.push(Point::new(tick + first_bar_length, pitch_at(tick)));
            tick += SAMPLING_INTERVAL_TICK;
        }
        prefix.extend(result);
        result = prefix;
    }
    if !result.is_empty() {
        result.insert(0, Point::start_point());
        result.push(Point::end_point());
    }
    result
}

/// `pitch_from_dv_track`.
pub fn pitch_from_dv_track(
    first_bar_length: i64,
    segments: &[DvSegmentPitchRawData],
    notes: &[DvNoteWithPitch],
    tempos: &[SongTempo],
) -> Option<ParamCurve> {
    let merged_points = merge_points_from_segments(segments)?;
    let merged_points = merge_same_tick_points(&merged_points)?;
    let merged_points = merge_same_value_points(&merged_points)?;
    Some(ParamCurve::new(apply_default_pitch(
        first_bar_length,
        &merged_points,
        notes,
        tempos,
    )))
}

/// `generate_for_dv`.
pub fn generate_for_dv(
    first_bar_length: i64,
    pitch: &ParamCurve,
) -> Option<DvSegmentPitchRawData> {
    if pitch.points.is_empty() {
        return None;
    }
    let mut data = vec![DvPoint { x: -1, y: -1 }];
    let mut last_value: Option<i64> = None;
    for point in &pitch.points {
        if (last_value.is_none() && point.y != -100) || point.y == -100 {
            data.push(DvPoint {
                x: (point.x - first_bar_length) as i32,
                y: -1,
            });
        }
        if point.y != -100 {
            data.push(DvPoint {
                x: (point.x - first_bar_length) as i32,
                y: py_round(convert_note_key_float(point.y as f64 / 100.0) * 100.0) as i32,
            });
        }
        last_value = Some(point.y);
    }
    let last_x = if data.len() > 1 {
        data[data.len() - 1].x + 1
    } else {
        307201
    };
    data.push(DvPoint { x: last_x, y: -1 });
    Some(DvSegmentPitchRawData {
        tick_offset: 0,
        data,
    })
}

//! Port of `libresvip.plugins.svp.track_merge_utils`.

use libresvip_core::model::base::{Note, ParamCurve, Params, SingingTrack};
use libresvip_core::model::point::Point;

use crate::svp::math_utils::{find_index, find_last_index, py_round, RangeInterval};

fn py_slice_prefix(items: &[Point], index: isize) -> Vec<Point> {
    if index >= 0 {
        items[..items.len().min(index as usize)].to_vec()
    } else {
        let keep = items.len().saturating_sub(index.unsigned_abs());
        items[..keep].to_vec()
    }
}

fn py_slice_from(items: &[Point], index: isize) -> Vec<Point> {
    if index >= 0 {
        let start = items.len().min(index as usize);
        items[start..].to_vec()
    } else {
        let start = items.len().saturating_sub(index.unsigned_abs());
        items[start..].to_vec()
    }
}

/// `param_curve_override_with`.
pub fn param_curve_override_with(
    main_curve: &mut ParamCurve,
    override_curve: &ParamCurve,
    start: i64,
    end: i64,
    termination: i64,
) {
    let mut inserted_points: Vec<Point> = Vec::new();
    let main_points = &main_curve.points;
    let main_count = main_points.len();
    let main_left_index = find_last_index(main_points, |point| point.x <= start);
    let main_right_index = find_index(main_points, |point| point.x > end);
    let override_points = &override_curve.points;
    let override_count = override_points.len();
    let override_left_index = find_last_index(override_points, |point| point.x <= start);
    let override_right_index = find_index(override_points, |point| point.x > end);

    let main_left_defined = 0 <= main_left_index
        && main_left_index < main_count as isize - 1
        && main_points[main_left_index as usize].x != termination
        && main_points[main_left_index as usize + 1].x != termination;
    let main_right_defined = 0 < main_right_index
        && main_right_index <= main_count as isize - 1
        && main_points[main_right_index as usize - 1].x != termination
        && main_points[main_right_index as usize].x != termination;
    let override_left_defined = 0 <= override_left_index
        && override_left_index < override_count as isize - 1
        && override_points[override_left_index as usize].x != termination
        && override_points[override_left_index as usize + 1].x != termination;
    let override_right_defined = 0 < override_right_index
        && override_right_index <= override_count as isize - 1
        && override_points[override_right_index as usize - 1].x != termination
        && override_points[override_right_index as usize].x != termination;

    if main_left_defined {
        let left = &main_points[main_left_index as usize];
        let next = &main_points[main_left_index as usize + 1];
        let r = (start - left.x) as f64 / (next.x - left.x) as f64;
        inserted_points.push(Point::new(
            start,
            py_round(r * (next.y + left.y) as f64 * (1.0 - r)),
        ));
    }
    if main_left_defined ^ override_left_defined {
        inserted_points.push(Point::new(start, termination));
    }
    if override_left_defined {
        let left = &override_points[override_left_index as usize];
        let next = &override_points[override_left_index as usize + 1];
        let r = (start - left.x) as f64 / (next.x - left.x) as f64;
        inserted_points.push(Point::new(
            start,
            py_round(r * (next.y + left.y) as f64 * (1.0 - r)),
        ));
    }
    let mut index = override_left_index + 1;
    while index < override_right_index {
        inserted_points.push(override_points[index as usize]);
        index += 1;
    }
    if override_right_defined {
        let previous = &override_points[override_right_index as usize - 1];
        let right = &override_points[override_right_index as usize];
        let r = (end - previous.x) as f64 / (right.x - previous.x) as f64;
        inserted_points.push(Point::new(
            end,
            py_round(r * (right.y + previous.y) as f64 * (1.0 - r)),
        ));
    }
    if main_right_defined ^ override_right_defined {
        inserted_points.push(Point::new(end, termination));
    }
    if main_right_defined {
        let previous = &main_points[main_right_index as usize - 1];
        let right = &main_points[main_right_index as usize];
        let r = (end - previous.x) as f64 / (right.x - previous.x) as f64;
        inserted_points.push(Point::new(
            end,
            py_round(r * (right.y + previous.y) as f64 * (1.0 - r)),
        ));
    }

    // Main points outside (main_left_index, main_right_index) are dropped.
    let mut kept: Vec<Point> = Vec::new();
    for (i, point) in main_points.iter().enumerate() {
        if main_left_index < i as isize && (i as isize) < main_right_index {
            kept.push(*point);
        }
    }
    let mut result = py_slice_prefix(&kept, main_left_index);
    result.extend(inserted_points);
    result.extend(py_slice_from(&kept, main_left_index));
    main_curve.points = result;
}

/// `params_override_with`.
pub fn params_override_with(main_params: &mut Params, override_params: &Params, start: i64, end: i64) {
    param_curve_override_with(&mut main_params.pitch, &override_params.pitch, start, end, 0);
    param_curve_override_with(&mut main_params.volume, &override_params.volume, start, end, 0);
    param_curve_override_with(&mut main_params.breath, &override_params.breath, start, end, 0);
    param_curve_override_with(&mut main_params.gender, &override_params.gender, start, end, 0);
    param_curve_override_with(
        &mut main_params.strength,
        &override_params.strength,
        start,
        end,
        0,
    );
}

/// `track_override_with`.
pub fn track_override_with(
    track: &mut SingingTrack,
    note_list: &[Note],
    params: &Params,
    first_bar_tick: i64,
) {
    let main_note_list = track.note_list.clone();
    let mut interval = RangeInterval::new();
    let mut main_left_index: isize = -1;
    let mut main_right_index: isize = -1;
    let mut i = 0usize;
    while i < note_list.len() {
        main_left_index = if main_left_index < main_note_list.len() as isize - 1 {
            find_last_index(&main_note_list, |note| note.start_pos <= note_list[i].start_pos)
        } else {
            -1
        };
        let main_left_note = if main_left_index >= 0 {
            main_note_list.get(main_left_index as usize)
        } else {
            None
        };
        let start = match main_left_note {
            None => note_list[i].start_pos - 120,
            Some(note) if note.end_pos() < note_list[i].start_pos - 120 => {
                note_list[i].start_pos - 120
            }
            Some(note) => (note.end_pos() + note_list[i].start_pos).div_euclid(2),
        };
        while i < note_list.len() - 1 && note_list[i].end_pos() >= note_list[i + 1].start_pos {
            i += 1;
        }
        main_right_index = if main_right_index < main_note_list.len() as isize - 1 {
            if main_left_index >= 0 {
                let slice = &main_note_list[main_left_index as usize..];
                find_index(slice, |note| note.start_pos > note_list[i].start_pos)
            } else {
                // Python `main_note_list[-1:]` shortcut.
                let slice = &main_note_list[main_note_list.len().saturating_sub(1)..];
                find_index(slice, |note| note.start_pos > note_list[i].start_pos)
            }
        } else {
            -1
        };
        let main_right_note = if main_right_index >= 0 {
            main_note_list.get(main_right_index as usize)
        } else {
            None
        };
        let end = match main_right_note {
            None => note_list[i].end_pos() + 120,
            Some(note) if note.start_pos > note_list[i].end_pos() + 120 => {
                note_list[i].end_pos() + 120
            }
            Some(note) => (note.start_pos + note_list[i].end_pos()).div_euclid(2),
        };
        interval.union_with(&RangeInterval::from_pairs([(start, end)]));
        i += 1;
    }
    track.note_list.extend(note_list.iter().cloned());
    track.note_list.sort_by_key(|note| note.start_pos);
    for (start, end) in interval.shift(first_bar_tick).sub_ranges() {
        params_override_with(&mut track.edited_params, params, start, end);
    }
}

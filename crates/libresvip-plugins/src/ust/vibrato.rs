//! Port of `libresvip.plugins.ust.vibrato_param`.

use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::model::base::Note;
use libresvip_core::model::point::Point;

use super::math::{clamp, round_int};
use super::model::NoteVibrato;

/// `append_utau_note_vibrato`.
pub fn append_utau_note_vibrato(
    note_values: &[Point],
    vibrato_params: Option<&NoteVibrato>,
    note_start: &Note,
    tick_time_transformer: &TimeSynchronizer,
    sample_interval_tick: i64,
) -> Vec<Point> {
    let Some(vibrato_params) = vibrato_params else {
        return note_values.to_vec();
    };
    // x-axis: milliSec, y-axis: 100cents
    let note_length = tick_time_transformer
        .get_duration_secs_from_ticks(note_start.start_pos, note_start.end_pos())
        * 1000.0;
    let vibrato_length = note_length * vibrato_params.length / 100.0;
    let depth = vibrato_params.depth / 100.0;
    if vibrato_length <= 0.0 || vibrato_params.period == 0.0 || depth <= 0.0 {
        return note_values.to_vec();
    }
    let frequency = 1.0 / vibrato_params.period;
    let ease_in_length = note_length * vibrato_params.fade_in / 100.0;
    let ease_out_length = note_length * vibrato_params.fade_out / 100.0;
    let phase = vibrato_params.phase_shift / 100.0;
    let shift = vibrato_params.shift / 100.0;
    let start = note_length - vibrato_length;

    let vibrato = |t: f64| -> f64 {
        if t < start {
            return 0.0;
        }
        let ease_in_factor = if ease_in_length != 0.0 {
            clamp((t - start) / ease_in_length, 0.0, 1.0)
        } else {
            1.0
        };
        let ease_out_factor = if ease_out_length != 0.0 {
            clamp((note_length - t) / ease_out_length, 0.0, 1.0)
        } else {
            1.0
        };
        let x = std::f64::consts::TAU * (frequency * (t - start) - phase);
        depth * ease_in_factor * ease_out_factor * (x.sin() + shift) * 100.0
    };

    let note_start_in_millis =
        tick_time_transformer.get_actual_secs_from_ticks(note_start.start_pos) * 1000.0;
    // get approximate interval for interpolation
    let sample_interval_in_millis = tick_time_transformer.get_duration_secs_from_ticks(
        note_start.start_pos,
        note_start.start_pos + sample_interval_tick,
    ) * 1000.0;

    let mut interpolated_points: Vec<(f64, f64)> = Vec::new();
    for (index, point) in note_values.iter().enumerate() {
        let (x, y) = (point.x, point.y);
        let x_in_millis =
            tick_time_transformer.get_actual_secs_from_ticks(x) * 1000.0 - note_start_in_millis;
        interpolated_points.push((x_in_millis, y as f64 + vibrato(x_in_millis)));
        if index < note_values.len() - 1 {
            let next_x = note_values[index + 1].x;
            let mut pos = x_in_millis + sample_interval_in_millis;
            while pos
                < tick_time_transformer.get_actual_secs_from_ticks(next_x) * 1000.0
                    - note_start_in_millis
            {
                interpolated_points.push((pos, y as f64 + vibrato(pos)));
                pos += sample_interval_in_millis;
            }
        }
    }

    interpolated_points
        .into_iter()
        .map(|(x, y)| {
            Point::new(
                round_int(
                    tick_time_transformer.get_actual_ticks_from_secs(
                        (x + note_start_in_millis) / 1000.0,
                    ),
                ),
                round_int(y),
            )
        })
        .collect()
}

//! Port of `libresvip.plugins.acep.base_pitch_curve`.

use libresvip_core::core::time_sync::TimeSynchronizer;

use super::interval::IntervalDict;
use super::model::{AcepNote, AcepVibrato};
use super::music_math::{exact_sumprod, linear_interpolation, neumaier_sum};

#[derive(Clone, Debug, Default)]
pub struct NoteInSeconds {
    pub semitone: i64,
    pub start: f64,
    pub end: f64,
}

/// `_convolve` — a 119 tap raised-cosine smoothing kernel.
fn convolve_notes(note_list: &[NoteInSeconds]) -> Vec<f64> {
    let last = match note_list.last() {
        Some(last) => last,
        None => return Vec::new(),
    };
    let total_points = (1000.0 * (last.end + 0.12)).round_ties_even() as i64 + 1;
    let total_points = total_points.max(0) as usize;
    let mut init_values = vec![0.0f64; total_points];
    let mut note_index = 0usize;
    for (index, value) in init_values.iter_mut().enumerate() {
        *value = note_list[note_index].semitone as f64;
        if note_index < note_list.len() - 1 {
            let ts = 0.001 * index as f64;
            if ts >= 0.5 * (note_list[note_index].end + note_list[note_index + 1].start) {
                note_index += 1;
            }
        }
    }
    let mut kernel = [0.0f64; 119];
    for (index, value) in kernel.iter_mut().enumerate() {
        let ts = 0.001 * (index as f64 - 59.0);
        *value = (std::f64::consts::PI * ts / 0.12).cos();
    }
    let kernel_sum = neumaier_sum(&kernel);
    for value in kernel.iter_mut() {
        *value /= kernel_sum;
    }
    // `more_itertools.convolve(init_values, kernel)[59:-59]`: the full
    // convolution element `59 + t` is `sum_i kernel[i] * init[t + 59 - i]`, and
    // the terms are accumulated in exactly that (ascending `i`) order because
    // `math.sumprod` sums left to right.
    let mut result = vec![0.0f64; init_values.len()];
    let mut window = vec![0.0f64; kernel.len()];
    let half = (kernel.len() as i64 - 1) / 2;
    for (index, value) in result.iter_mut().enumerate() {
        for (tap, slot) in window.iter_mut().enumerate() {
            let position = half + index as i64 - tap as i64;
            *slot = if position >= 0 && (position as usize) < init_values.len() {
                init_values[position as usize]
            } else {
                0.0
            };
        }
        *value = exact_sumprod(&kernel, &window);
    }
    result
}

/// `acep_vibrato_value_curve`.
pub fn vibrato_value_curve(seconds: f64, vibrato_start: f64, vibrato: &AcepVibrato) -> f64 {
    (std::f64::consts::PI
        * (2.0 * (seconds - vibrato_start) * vibrato.frequency - vibrato.phase))
        .sin()
        * vibrato.amplitude
        * 0.5
}

#[derive(Clone, Debug)]
enum CurveValue {
    Vibrato {
        vibrato_start: f64,
        vibrato: AcepVibrato,
    },
    Linear {
        start: (f64, f64),
        end: (f64, f64),
    },
}

impl CurveValue {
    fn evaluate(&self, seconds: f64) -> f64 {
        match self {
            CurveValue::Vibrato {
                vibrato_start,
                vibrato,
            } => vibrato_value_curve(seconds, *vibrato_start, vibrato),
            CurveValue::Linear { start, end } => linear_interpolation(seconds, *start, *end),
        }
    }
}

/// Port of `libresvip.plugins.acep.base_pitch_curve.BasePitchCurve`.
#[derive(Clone, Debug, Default)]
pub struct BasePitchCurve {
    vibrato_value_dict: IntervalDict<CurveValue>,
    vibrato_coef_dict: IntervalDict<CurveValue>,
    values_in_semitone: Vec<f64>,
}

impl BasePitchCurve {
    pub fn new(notes: &[AcepNote], synchronizer: &TimeSynchronizer, tick_offset: i64) -> Self {
        let mut note_list: Vec<NoteInSeconds> = Vec::with_capacity(notes.len());
        let mut value_dict: IntervalDict<CurveValue> = IntervalDict::new();
        let mut coef_dict: IntervalDict<CurveValue> = IntervalDict::new();
        for note in notes {
            let note_end =
                synchronizer.get_actual_secs_from_ticks(note.pos + note.dur + tick_offset);
            let note_start = synchronizer.get_actual_secs_from_ticks(note.pos + tick_offset);
            note_list.push(NoteInSeconds {
                start: note_start,
                end: note_end,
                semitone: note.pitch,
            });
            let Some(vibrato) = note.vibrato.as_ref() else {
                continue;
            };
            let vibrato_start = synchronizer.get_actual_secs_from_ticks(
                (note.pos as f64 + vibrato.start_pos + tick_offset as f64) as i64,
            );
            let vibrato_duration = note_end - vibrato_start;
            value_dict.set(
                vibrato_start,
                true,
                note_end,
                true,
                CurveValue::Vibrato {
                    vibrato_start,
                    vibrato: vibrato.clone(),
                },
            );
            let attack_time = vibrato_start + vibrato.attack_ratio * vibrato_duration;
            let release_time = note_end - vibrato.release_ratio * vibrato_duration;
            if vibrato.release_ratio != 0.0 {
                coef_dict.set(
                    release_time,
                    false,
                    note_end,
                    true,
                    CurveValue::Linear {
                        start: (release_time, vibrato.release_level),
                        end: (note_end, 0.0),
                    },
                );
            }
            coef_dict.set(
                attack_time,
                true,
                release_time,
                true,
                CurveValue::Linear {
                    start: (attack_time, vibrato.attack_level),
                    end: (release_time, vibrato.release_level),
                },
            );
            if vibrato.attack_ratio != 0.0 {
                coef_dict.set(
                    vibrato_start,
                    true,
                    attack_time,
                    false,
                    CurveValue::Linear {
                        start: (vibrato_start, 0.0),
                        end: (attack_time, vibrato.attack_level),
                    },
                );
            }
        }
        let values_in_semitone = convolve_notes(&note_list);
        Self {
            vibrato_value_dict: value_dict,
            vibrato_coef_dict: coef_dict,
            values_in_semitone,
        }
    }

    /// `BasePitchCurve.semitone_value_at`.
    pub fn semitone_value_at(&self, seconds: f64) -> f64 {
        if self.values_in_semitone.is_empty() {
            return 0.0;
        }
        let position = 1000.0 * seconds.max(0.0);
        let left_index = position.floor();
        let lambda = position - left_index;
        let clipped_left_index =
            (left_index as i64).clamp(0, self.values_in_semitone.len() as i64 - 1) as usize;
        let clipped_right_index =
            (clipped_left_index as i64 + 1).clamp(0, self.values_in_semitone.len() as i64 - 1)
                as usize;
        let mut pitch_value = (1.0 - lambda) * self.values_in_semitone[clipped_left_index]
            + lambda * self.values_in_semitone[clipped_right_index];
        if let Some(vibrato_value) = self.vibrato_value_dict.get(seconds) {
            let coefficient = self
                .vibrato_coef_dict
                .get(seconds)
                .map(|value| value.evaluate(seconds))
                .unwrap_or(0.0);
            pitch_value += vibrato_value.evaluate(seconds) * coefficient;
        }
        pitch_value
    }
}

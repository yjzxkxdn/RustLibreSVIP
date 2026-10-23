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

/// Length of `_convolve`'s sample array: `round(1000 * (last.end + 0.12)) + 1`.
fn semitone_sample_count(note_list: &[NoteInSeconds]) -> usize {
    match note_list.last() {
        Some(last) => ((1000.0 * (last.end + 0.12)).round_ties_even() as i64 + 1).max(0) as usize,
        None => 0,
    }
}

/// `_convolve` — a 119 tap raised-cosine smoothing kernel.
///
/// Only the samples from `first_index` on are actually convolved: the samples
/// before it are never read by `semitone_value_at` (the caller passes the first
/// second it is going to query), and a convolution sample only looks at its
/// `±59` neighbours, so the values that *are* read stay bit-identical to the
/// reference implementation.
fn convolve_notes(note_list: &[NoteInSeconds], first_index: usize) -> Vec<f64> {
    if note_list.is_empty() {
        return Vec::new();
    }
    let total_points = semitone_sample_count(note_list);
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
    let first_index = first_index.min(init_values.len());
    let mut result = vec![0.0f64; init_values.len() - first_index];
    let mut window = vec![0.0f64; kernel.len()];
    let half = (kernel.len() as i64 - 1) / 2;
    for (offset, value) in result.iter_mut().enumerate() {
        let index = first_index + offset;
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
    /// `values_in_semitone` starting at sample `values_offset` (1 sample = 1 ms).
    values_in_semitone: Vec<f64>,
    values_offset: usize,
    /// Length of the reference `values_in_semitone` array.
    values_len: usize,
}

impl BasePitchCurve {
    /// `first_query_secs` is the earliest second `semitone_value_at` will ever
    /// be asked about; everything before it is left uncomputed (see
    /// [`convolve_notes`]). Pass `0.0` for the reference behaviour.
    pub fn new(
        notes: &[AcepNote],
        synchronizer: &TimeSynchronizer,
        tick_offset: i64,
        first_query_secs: f64,
    ) -> Self {
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
        // A convolution sample only reads its ±59 neighbours, so dropping the
        // samples before `first_query_secs - 60` keeps every value that
        // `semitone_value_at` can return unchanged.
        let values_len = semitone_sample_count(&note_list);
        let first_index = ((1000.0 * first_query_secs.max(0.0)).floor() as i64)
            .saturating_sub(60)
            .max(0) as usize;
        let first_index = first_index.min(values_len.saturating_sub(1));
        let values_in_semitone = convolve_notes(&note_list, first_index);
        Self {
            vibrato_value_dict: value_dict,
            vibrato_coef_dict: coef_dict,
            values_in_semitone,
            values_offset: first_index,
            values_len,
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
            (left_index as i64).clamp(0, self.values_len as i64 - 1) as usize;
        let clipped_right_index =
            (clipped_left_index as i64 + 1).clamp(0, self.values_len as i64 - 1) as usize;
        let mut pitch_value = (1.0 - lambda) * self.value_at(clipped_left_index)
            + lambda * self.value_at(clipped_right_index);
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

    /// Reads the reference `values_in_semitone[index]`; indices before
    /// `values_offset` are never queried, they fall back to the first stored
    /// sample.
    fn value_at(&self, index: usize) -> f64 {
        let local = index.saturating_sub(self.values_offset);
        self.values_in_semitone[local.min(self.values_in_semitone.len() - 1)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note_list() -> Vec<NoteInSeconds> {
        vec![
            NoteInSeconds {
                semitone: 55,
                start: 0.2,
                end: 0.5,
            },
            NoteInSeconds {
                semitone: 57,
                start: 0.5,
                end: 1.2,
            },
            NoteInSeconds {
                semitone: 53,
                start: 1.25,
                end: 2.0,
            },
        ]
    }

    /// Dropping the samples before `first_index` must not change any sample
    /// that `semitone_value_at` can return.
    #[test]
    fn windowed_convolve_matches_the_full_convolve() {
        let notes = note_list();
        let full = convolve_notes(&notes, 0);
        assert!(!full.is_empty());
        for first_index in [1usize, 59, 60, 300, 900, full.len() - 1] {
            let windowed = convolve_notes(&notes, first_index);
            assert_eq!(windowed.len(), full.len() - first_index);
            for (offset, value) in windowed.iter().enumerate() {
                let expected = full[first_index + offset];
                assert_eq!(
                    value.to_bits(),
                    expected.to_bits(),
                    "sample {} differs",
                    first_index + offset
                );
            }
        }
    }

    /// A windowed curve must answer exactly like the reference one over the
    /// range the caller quotes at construction time.
    #[test]
    fn windowed_curve_matches_the_reference_over_the_query_range() {
        use libresvip_core::model::base::SongTempo;

        let synchronizer =
            TimeSynchronizer::new(&[SongTempo::new(0, 120.0)], 0, false, 60.0);
        let notes: Vec<AcepNote> = [60i64, 62, 64]
            .iter()
            .enumerate()
            .map(|(index, pitch)| AcepNote {
                pos: index as i64 * 480,
                dur: 480,
                pitch: *pitch,
                ..Default::default()
            })
            .collect();
        let reference = BasePitchCurve::new(&notes, &synchronizer, 0, 0.0);
        assert!(reference.values_len > 1000);
        let query_start = 0.9;
        let windowed = BasePitchCurve::new(&notes, &synchronizer, 0, query_start);
        assert!(windowed.values_offset > 0);
        let mut seconds = query_start;
        while seconds < 1.4 {
            assert_eq!(
                windowed.semitone_value_at(seconds).to_bits(),
                reference.semitone_value_at(seconds).to_bits(),
                "value at {seconds} s differs"
            );
            seconds += 0.001;
        }
    }
}

//! Numeric helpers and a small `PiecewiseIntervalDict` replacement.
//!
//! The CCS pitch conversion (shared with the TechnoSpeech family) stores
//! constants or wave functions over closed-open intervals. Python builds them
//! with `portion.IntervalDict`, so overlapping writes split earlier intervals
//! and a lookup returns the value covering the queried point.

pub const TIME_UNIT_AS_TICKS_PER_BPM: f64 = 4.8 / 120.0;
pub const TEMP_VALUE_AS_NULL: f64 = -1.0;
pub const MIN_DATA_LENGTH: i64 = 500;

/// `repr(float)`.
pub fn float_repr(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-Infinity".to_owned()
        } else {
            "Infinity".to_owned()
        };
    }
    let rendered = format!("{value:?}");
    match rendered.find(['e', 'E']) {
        Some(position) => {
            let (mantissa, exponent) = rendered.split_at(position);
            let exponent = &exponent[1..];
            let (sign, digits) = match exponent.strip_prefix('-') {
                Some(rest) => ('-', rest),
                None => ('+', exponent.strip_prefix('+').unwrap_or(exponent)),
            };
            format!("{mantissa}e{sign}{digits:0>2}")
        }
        None => rendered,
    }
}

/// Python's `round(float)`.

pub use libresvip_core::core::compat::py_round;

/// `repr(float)` for `json.dumps`-style output, kept next to `float_repr`.
pub fn json_float(value: f64) -> String {
    float_repr(value)
}

#[derive(Clone, Debug, Default)]
pub struct ConstantIntervals {
    entries: Vec<(f64, f64, Option<f64>)>,
}

impl ConstantIntervals {
    pub fn insert(&mut self, lower: f64, upper: f64, value: Option<f64>) {
        if !(lower < upper) {
            return;
        }
        let mut next = Vec::with_capacity(self.entries.len() + 2);
        for (start, end, previous) in self.entries.drain(..) {
            if end <= lower || start >= upper {
                next.push((start, end, previous));
                continue;
            }
            if start < lower {
                next.push((start, lower, previous));
            }
            if upper < end {
                next.push((upper, end, previous));
            }
        }
        next.push((lower, upper, value));
        next.sort_by(|left, right| {
            left.0
                .partial_cmp(&right.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        self.entries = next;
    }

    pub fn get(&self, position: f64) -> Option<f64> {
        let index = self
            .entries
            .partition_point(|(start, _, _)| *start <= position);
        let (start, end, value) = self.entries.get(index.checked_sub(1)?)?;
        if position >= *start && position < *end {
            *value
        } else {
            None
        }
    }

    pub fn get_with_default(&self, position: f64, default: f64) -> f64 {
        self.get(position).unwrap_or(default)
    }
}

#[derive(Clone, Copy, Debug)]
enum WaveValue {
    Constant(f64),
    Wave {
        shift: f64,
        omega: f64,
        phase: f64,
    },
}

#[derive(Clone, Debug, Default)]
pub struct WaveIntervals {
    entries: Vec<(f64, f64, WaveValue)>,
}

fn vibrato_curve(value: f64, shift: f64, omega: f64, phase: f64) -> f64 {
    (omega * (value - shift) + phase).sin()
}

impl WaveIntervals {
    pub fn insert_constant(&mut self, lower: f64, upper: f64, value: f64) {
        self.insert(lower, upper, WaveValue::Constant(value));
    }

    pub fn insert_wave(&mut self, lower: f64, upper: f64, shift: f64, omega: f64, phase: f64) {
        self.insert(
            lower,
            upper,
            WaveValue::Wave {
                shift,
                omega,
                phase,
            },
        );
    }

    fn insert(&mut self, lower: f64, upper: f64, value: WaveValue) {
        if !(lower < upper) {
            return;
        }
        let mut next = Vec::with_capacity(self.entries.len() + 2);
        for (start, end, previous) in self.entries.drain(..) {
            if end <= lower || start >= upper {
                next.push((start, end, previous));
                continue;
            }
            if start < lower {
                next.push((start, lower, previous));
            }
            if upper < end {
                next.push((upper, end, previous));
            }
        }
        next.push((lower, upper, value));
        next.sort_by(|left, right| {
            left.0
                .partial_cmp(&right.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        self.entries = next;
    }

    pub fn get(&self, position: f64) -> Option<f64> {
        let index = self
            .entries
            .partition_point(|(start, _, _)| *start <= position);
        let (start, end, value) = self.entries.get(index.checked_sub(1)?)?;
        if position < *start || position >= *end {
            return None;
        }
        match value {
            WaveValue::Constant(value) => Some(*value),
            WaveValue::Wave {
                shift,
                omega,
                phase,
            } => Some(vibrato_curve(position, *shift, *omega, *phase)),
        }
    }
}

/// A local re-implementation of the parts of `TimeSynchronizer` that are
/// queried with non-integer tick positions (vibrato events use fractional
/// repeats).
#[derive(Clone, Debug)]
pub struct Sync {
    pub tempo_list: Vec<libresvip_core::model::base::SongTempo>,
    positions: Vec<i64>,
    cum_secs: Vec<f64>,
}

impl Sync {
    pub fn new(tempo_list: &[libresvip_core::model::base::SongTempo]) -> Self {
        let positions: Vec<i64> = tempo_list.iter().map(|tempo| tempo.position).collect();
        let mut cum_secs = vec![0.0f64; tempo_list.len()];
        for index in 1..tempo_list.len() {
            let delta = (positions[index] - positions[index - 1]) as f64;
            cum_secs[index] = cum_secs[index - 1] + delta / tempo_list[index - 1].bpm / 8.0;
        }
        Self {
            tempo_list: tempo_list.to_vec(),
            positions,
            cum_secs,
        }
    }

    pub fn secs_at(&self, ticks: f64) -> f64 {
        if self.positions.is_empty() {
            return 0.0;
        }
        let index = self
            .positions
            .partition_point(|position| (*position as f64) <= ticks)
            .saturating_sub(1);
        self.cum_secs[index]
            + (ticks - self.positions[index] as f64) / self.tempo_list[index].bpm / 8.0
    }

    pub fn duration(&self, start_ticks: f64, end_ticks: f64) -> f64 {
        self.secs_at(end_ticks) - self.secs_at(start_ticks)
    }
}

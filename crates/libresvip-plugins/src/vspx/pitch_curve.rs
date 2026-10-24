//! Port of `libresvip.plugins.vspx.vspx_interval_dict` together with the
//! `PiecewiseIntervalDict` behaviour of `libresvip.core.time_interval`.

use std::f64::consts::PI;

use libresvip_core::core::time_sync::TimeSynchronizer;

use super::model::{NoteTrack, Trill};

const INFINITY: f64 = f64::INFINITY;

// ---------------------------------------------------------------------------
// Interpolation functions
// ---------------------------------------------------------------------------

/// `vspx_sine_vibrato_interpolation`.
fn sine_vibrato_interpolation(seconds: f64, vibrato_start: f64, trill: &Trill) -> f64 {
    (PI * (2.0 * (seconds - vibrato_start) * trill.frequency.as_f64() + trill.phase.as_f64()))
        .sin()
        * trill.amplitude.as_f64()
}

/// `vspx_cosine_vibrato_coef_attack_interpolation`.
fn cosine_vibrato_coef_attack_interpolation(seconds: f64, vibrato_start: f64, por: f64) -> f64 {
    1.0 - (PI * (0.5 * (seconds - vibrato_start) / por)).cos()
}

/// `vspx_cosine_vibrato_coef_release_interpolation`.
fn cosine_vibrato_coef_release_interpolation(seconds: f64, vibrato_end: f64, por: f64) -> f64 {
    1.0 - (PI * (0.5 * (vibrato_end - seconds) / por)).cos()
}

// ---------------------------------------------------------------------------
// PiecewiseIntervalDict
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum Segment {
    Constant(f64),
    EaseInOut {
        start: (f64, f64),
        end: (f64, f64),
    },
    Vibrato {
        vibrato_start: f64,
        trill: Trill,
    },
    CoefAttack {
        start: f64,
        por: f64,
    },
    CoefRelease {
        end: f64,
        por: f64,
    },
}

impl Segment {
    fn evaluate(&self, x: f64) -> f64 {
        match self {
            Segment::Constant(value) => *value,
            Segment::EaseInOut { start, end } => {
                let (x0, y0) = *start;
                let (x1, y1) = *end;
                let ratio = (x - x0) / (x1 - x0);
                y0 + (y1 - y0) * (1.0 - (ratio * PI).cos()) / 2.0
            }
            Segment::Vibrato {
                vibrato_start,
                trill,
            } => sine_vibrato_interpolation(x, *vibrato_start, trill),
            Segment::CoefAttack { start, por } => {
                cosine_vibrato_coef_attack_interpolation(x, *start, *por)
            }
            Segment::CoefRelease { end, por } => {
                cosine_vibrato_coef_release_interpolation(x, *end, *por)
            }
        }
    }
}

/// `PiecewiseIntervalDict` — overlapping intervals are allowed and a lookup
/// returns the value of the first interval (ordered by its lower bound) that
/// contains the point. Storage/matching lives in the shared `IntervalDict`.
#[derive(Clone, Debug, Default)]
pub struct PiecewiseIntervalDict {
    dict: crate::interval_dict::IntervalDict<Segment>,
}

impl PiecewiseIntervalDict {
    fn set(
        &mut self,
        start: f64,
        end: f64,
        start_closed: bool,
        end_closed: bool,
        value: Segment,
    ) {
        self.dict
            .set(start, end, start_closed, end_closed, value);
    }

    fn set_closedopen(&mut self, start: f64, end: f64, value: Segment) {
        self.set(start, end, true, false, value);
    }

    fn set_closed(&mut self, start: f64, end: f64, value: Segment) {
        self.set(start, end, true, true, value);
    }

    fn set_openclosed(&mut self, start: f64, end: f64, value: Segment) {
        self.set(start, end, false, true, value);
    }

    fn get(&mut self, x: f64) -> Option<f64> {
        self.dict.get(x, |value, x| value.evaluate(x))
    }
}

// ---------------------------------------------------------------------------
// BasePitchCurve
// ---------------------------------------------------------------------------

/// `BasePitchCurve`.
pub struct BasePitchCurve {
    key_interval_dict: PiecewiseIntervalDict,
    vibrato_value_interval_dict: PiecewiseIntervalDict,
    vibrato_coef_interval_dict: PiecewiseIntervalDict,
    synchronizer: TimeSynchronizer,
}

impl BasePitchCurve {
    pub fn new(
        note_track: &NoteTrack,
        default_trill: Option<&Trill>,
        synchronizer: TimeSynchronizer,
    ) -> Result<Self, String> {
        let mut curve = Self {
            key_interval_dict: PiecewiseIntervalDict::default(),
            vibrato_value_interval_dict: PiecewiseIntervalDict::default(),
            vibrato_coef_interval_dict: PiecewiseIntervalDict::default(),
            synchronizer,
        };
        curve.build(note_track, default_trill)?;
        Ok(curve)
    }

    fn build(
        &mut self,
        note_track: &NoteTrack,
        default_trill: Option<&Trill>,
    ) -> Result<(), String> {
        let notes = &note_track.notes;
        if notes.is_empty() {
            return Ok(());
        }
        if notes.len() == 1 {
            let note = &notes[0];
            self.key_interval_dict.set_closedopen(
                0.0,
                INFINITY,
                Segment::Constant(note.key_number()? as f64),
            );
            if let Some(trill) = note.trill.as_ref().or(default_trill) {
                let vibrato_start_secs = self
                    .synchronizer
                    .get_actual_secs_from_ticks(note.pos)
                    + trill.pos.as_f64();
                let vibrato_end_secs = self
                    .synchronizer
                    .get_actual_secs_from_ticks(note.pos + note.duration);
                if vibrato_end_secs > vibrato_start_secs {
                    self.set_vspx_vibrato_curve(
                        vibrato_start_secs,
                        vibrato_end_secs,
                        trill,
                        Some(note_track.por.as_f64()),
                    );
                }
            }
            return Ok(());
        }

        for index in 0..notes.len() - 1 {
            let prev_note = &notes[index];
            let next_note = &notes[index + 1];
            let is_first = index == 0;
            let is_last = index + 1 == notes.len() - 1;
            let prev_start_secs = self
                .synchronizer
                .get_actual_secs_from_ticks(prev_note.pos);
            let prev_end_secs = self
                .synchronizer
                .get_actual_secs_from_ticks(prev_note.pos + prev_note.duration);
            let next_start_secs = self
                .synchronizer
                .get_actual_secs_from_ticks(next_note.pos);
            let next_end_secs = self
                .synchronizer
                .get_actual_secs_from_ticks(next_note.pos + next_note.duration);
            let prev_key = prev_note.key_number()? as f64;
            let next_key = next_note.key_number()? as f64;
            if is_first {
                self.key_interval_dict.set_closedopen(
                    0.0,
                    prev_start_secs,
                    Segment::Constant(prev_key),
                );
            }
            let middle_secs = (prev_end_secs + next_start_secs) / 2.0;
            self.key_interval_dict.set_closedopen(
                prev_start_secs,
                middle_secs,
                Segment::Constant(prev_key),
            );
            self.key_interval_dict.set_closedopen(
                middle_secs,
                next_end_secs,
                Segment::Constant(next_key),
            );
            if is_last {
                self.key_interval_dict.set_closedopen(
                    next_start_secs,
                    INFINITY,
                    Segment::Constant(next_key),
                );
                if let Some(trill) = next_note.trill.as_ref().or(default_trill) {
                    let vibrato_start_secs = next_start_secs + trill.pos.as_f64();
                    let vibrato_end_secs = next_end_secs;
                    if vibrato_end_secs > vibrato_start_secs {
                        self.set_vspx_vibrato_curve(
                            vibrato_start_secs,
                            vibrato_end_secs,
                            trill,
                            Some(note_track.por.as_f64()),
                        );
                    }
                }
            }
            let por = note_track.por.as_f64();
            if por > 0.0 {
                let por_start = middle_secs - por;
                let por_end = middle_secs + por;
                self.key_interval_dict.set_closedopen(
                    por_start,
                    por_end,
                    Segment::EaseInOut {
                        start: (por_start, prev_key),
                        end: (por_end, next_key),
                    },
                );
            }
            if let Some(trill) = prev_note.trill.as_ref().or(default_trill) {
                let vibrato_start_secs = prev_start_secs + trill.pos.as_f64();
                let vibrato_end_secs = (prev_end_secs + next_start_secs) / 2.0;
                if vibrato_end_secs > vibrato_start_secs {
                    self.set_vspx_vibrato_curve(
                        vibrato_start_secs,
                        vibrato_end_secs,
                        trill,
                        Some(note_track.por.as_f64()),
                    );
                }
            }
        }
        Ok(())
    }

    fn set_vspx_vibrato_curve(
        &mut self,
        start: f64,
        end: f64,
        trill: &Trill,
        por: Option<f64>,
    ) {
        self.vibrato_value_interval_dict.set_closed(
            start,
            end,
            Segment::Vibrato {
                vibrato_start: start,
                trill: *trill,
            },
        );
        let short = match por {
            None => true,
            Some(por) => (end - start) < por * 2.0,
        };
        if short {
            let middle = (start + end) / 2.0;
            let half = (end - start) / 2.0;
            self.vibrato_coef_interval_dict.set_closedopen(
                start,
                middle,
                Segment::CoefAttack { start, por: half },
            );
            self.vibrato_coef_interval_dict.set_closed(
                middle,
                end,
                Segment::CoefRelease { end, por: half },
            );
        } else if let Some(por) = por {
            if por != 0.0 {
                self.vibrato_coef_interval_dict.set_closedopen(
                    start,
                    start + por,
                    Segment::CoefAttack { start, por },
                );
                self.vibrato_coef_interval_dict
                    .set_closed(start + por, end - por, Segment::Constant(1.0));
                self.vibrato_coef_interval_dict.set_openclosed(
                    end - por,
                    end,
                    Segment::CoefRelease { end, por },
                );
            } else {
                self.vibrato_coef_interval_dict
                    .set_closed(start, end, Segment::Constant(0.0));
            }
        } else {
            self.vibrato_coef_interval_dict
                .set_closed(start, end, Segment::Constant(0.0));
        }
    }

    /// `BasePitchCurve.semitone_value_at`.
    pub fn semitone_value_at(&mut self, seconds: f64) -> Option<f64> {
        let pitch_value = self.key_interval_dict.get(seconds)?;
        if let Some(vibrato_value) = self.vibrato_value_interval_dict.get(seconds) {
            if let Some(coef) = self.vibrato_coef_interval_dict.get(seconds) {
                return Some(pitch_value + vibrato_value * coef);
            }
        }
        Some(pitch_value)
    }
}

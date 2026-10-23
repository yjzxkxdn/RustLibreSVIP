//! Port of `libresvip.plugins.ustx.util.BasePitchGenerator`.

use libresvip_core::Result;

use crate::ust::math::{div_floor, interpolate_shape};

use super::model::{PitchPoint, UNote, UstxProject, UVoicePart};
use super::time_axis::TimeAxis;

/// `BasePitchGenerator`.
pub struct BasePitchGenerator {
    pub pitch_start: i64,
    pub pitch_interval: i64,
    pub time_axis: TimeAxis,
}

impl BasePitchGenerator {
    pub fn new(project: &UstxProject) -> Result<Self> {
        let mut time_axis = TimeAxis::default();
        time_axis.build_segments(project)?;
        Ok(Self {
            pitch_start: 0,
            pitch_interval: 5,
            time_axis,
        })
    }

    /// `base_pitch` — one sample every `pitch_interval` ticks.
    pub fn base_pitch(&self, part: &UVoicePart) -> Vec<f64> {
        let Some(last_note) = part.notes.last() else {
            return Vec::new();
        };
        let mut pitches = vec![0.0f64; (last_note.end() / self.pitch_interval) as usize];
        let mut prev_note: Option<&UNote> = None;

        let mut index: usize = 0;
        for note in &part.notes {
            if index > 0
                && index < pitches.len()
                && self.pitch_start + index as i64 * self.pitch_interval < note.position
            {
                pitches[index] = pitches[index - 1];
                index += 1;
            }
            if index < pitches.len() {
                let pad = div_floor(note.end() - self.pitch_start, self.pitch_interval)
                    - index as i64;
                if pad >= 0 {
                    let pad = pad as usize;
                    // Python slice assignment: `pitches[index:index+pad+1] = ...`
                    let stop = (index + pad + 1).min(pitches.len());
                    pitches.splice(
                        index..stop,
                        std::iter::repeat(note.tone as f64 * 100.0).take(pad + 1),
                    );
                    index += pad + 1;
                }
            }
        }
        index = index.max(1);
        if index < pitches.len() {
            let value = pitches[index - 1];
            for slot in &mut pitches[index..] {
                *slot = value;
            }
        }

        for note in &part.notes {
            let vibrato = &note.vibrato;
            if vibrato.length <= 0.0 {
                continue;
            }
            let start_index =
                ((note.position - self.pitch_start) as f64 / self.pitch_interval as f64) as i64;
            let start_index = start_index.max(0) as usize;
            let end_index = std::cmp::min(
                pitches.len(),
                div_floor(note.end() - self.pitch_start, self.pitch_interval) as usize,
            );
            let n_period = vibrato.period
                / self
                    .time_axis
                    .ms_between_tick_pos(note.position as f64, note.end() as f64);
            for i in start_index..end_index {
                let n_pos = (self.pitch_start + i as i64 * self.pitch_interval - note.position)
                    as f64
                    / note.duration as f64;
                let point = vibrato.evaluate(n_pos, n_period, note);
                pitches[i] = point.1 * 100.0;
            }
        }

        for (note_index, note) in part.notes.iter().enumerate() {
            let mut pitch_points: Vec<PitchPoint> = Vec::new();
            for point in &note.pitch.data {
                let x = self.time_axis.ms_pos_to_tick_pos(
                    self.time_axis
                        .tick_pos_to_ms_pos((part.position + note.position) as f64)
                        + point.x,
                ) - part.position;
                let candidate = PitchPoint {
                    x: x as f64,
                    y: point.y * 10.0 + note.tone as f64 * 100.0,
                    shape: point.shape.clone(),
                };
                if pitch_points
                    .last()
                    .map(|last| last.x == candidate.x)
                    .unwrap_or(false)
                {
                    continue;
                }
                pitch_points.push(candidate);
            }
            if pitch_points.is_empty() {
                pitch_points.push(PitchPoint {
                    x: note.position as f64,
                    y: note.tone as f64 * 100.0,
                    shape: None,
                });
                pitch_points.push(PitchPoint {
                    x: note.end() as f64,
                    y: note.tone as f64 * 100.0,
                    shape: None,
                });
            }
            let first = pitch_points[0].clone();
            if note_index == 0 && first.x > self.pitch_start as f64 {
                pitch_points.insert(
                    0,
                    PitchPoint {
                        x: self.pitch_start as f64,
                        y: first.y,
                        shape: first.shape.clone(),
                    },
                );
            } else if first.x > note.position as f64 {
                pitch_points.insert(
                    0,
                    PitchPoint {
                        x: note.position as f64,
                        y: first.y,
                        shape: first.shape.clone(),
                    },
                );
            }
            let last = pitch_points.last().cloned().unwrap();
            if last.x < note.end() as f64 {
                pitch_points.push(PitchPoint {
                    x: note.end() as f64,
                    y: last.y,
                    shape: last.shape.clone(),
                });
            }
            let mut prev_point = pitch_points[0].clone();
            let mut index = {
                let offset = (prev_point.x - self.pitch_start as f64)
                    / self.pitch_interval as f64;
                if offset <= 0.0 {
                    0usize
                } else {
                    offset as usize
                }
            };
            for point in pitch_points.iter().skip(1) {
                let mut x =
                    self.pitch_start as f64 + index as f64 * self.pitch_interval as f64;
                while x <= point.x && index < pitches.len() {
                    let shape = prev_point
                        .shape
                        .as_deref()
                        .filter(|value| !value.is_empty())
                        .unwrap_or("lo");
                    let pitch = interpolate_shape(
                        x,
                        (prev_point.x, prev_point.y),
                        (point.x, point.y),
                        shape,
                    );
                    let base_pitch = match prev_note {
                        Some(prev) if x <= prev.end() as f64 => prev.tone as f64 * 100.0,
                        _ => note.tone as f64 * 100.0,
                    };
                    pitches[index] += pitch - base_pitch;
                    index += 1;
                    x += self.pitch_interval as f64;
                }
                prev_point = point.clone();
            }
            prev_note = Some(note);
        }
        pitches
    }
}

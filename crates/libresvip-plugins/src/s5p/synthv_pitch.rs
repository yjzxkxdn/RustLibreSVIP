//! Port of the parts of `libresvip.model.synthv_pitch` used by the s5p
//! plugin: the base (portamento), gaussian and vibrato layers plus the
//! simulator that sums them.
//!
//! The pitch-control layer is not used by s5p (the format only stores a
//! relative delta curve), so it is intentionally omitted here.

use std::cell::Cell;

use libresvip_core::core::time_sync::TimeSynchronizer;

use crate::s5p::math_utils::{bisect_left_f64, bisect_right_f64};

const MAX_BREAK: f64 = 0.01;
const PORTAMENTO_HALFWIDTH_RATIO: f64 = 0.3;
const HALFWIDTH_FLOOR_SECS: f64 = 1.0 / 256.0;
const SIGMOID_EDGE_RESIDUAL_CENTS: f64 = 1.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NoteStruct {
    pub key: i64,
    pub start: f64,
    pub end: f64,
    pub portamento_offset: f64,
    pub portamento_left: f64,
    pub portamento_right: f64,
    pub depth_left: f64,
    pub depth_right: f64,
    pub vibrato_start: f64,
    pub vibrato_left: f64,
    pub vibrato_right: f64,
    pub vibrato_depth: f64,
    pub vibrato_frequency: f64,
    pub vibrato_phase: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SigmoidNode {
    pub start: f64,
    pub end: f64,
    pub center: f64,
    pub key_left: i64,
    pub key_right: i64,
    h: f64,
    base: f64,
    k: f64,
}

impl SigmoidNode {
    pub fn new(
        center: f64,
        half_left: f64,
        half_right: f64,
        key_left: i64,
        key_right: i64,
    ) -> Self {
        let h = (key_right - key_left) as f64 * 100.0;
        let base = key_left as f64 * 100.0;
        let k = 3.0 / (half_left + half_right);
        let margin = ((h.abs() / SIGMOID_EDGE_RESIDUAL_CENTS - 1.0).max(1.0)).ln() / k;
        Self {
            start: center - margin,
            end: center + margin,
            center,
            key_left,
            key_right,
            h,
            base,
            k,
        }
    }

    pub fn value_at_secs(&self, secs: f64) -> f64 {
        let arg = self.k * (secs - self.center);
        if arg >= 10.0 {
            return self.base + self.h;
        }
        if arg <= -10.0 {
            return self.base;
        }
        self.base + self.h / (1.0 + (-arg).exp())
    }

    pub fn slope_at_secs(&self, secs: f64) -> f64 {
        let arg = self.k * (secs - self.center);
        if arg >= 10.0 || arg <= -10.0 {
            return 0.0;
        }
        let e = (-arg).exp();
        self.h * self.k * e / ((1.0 + e) * (1.0 + e))
    }
}

#[derive(Clone, Debug, Default)]
pub struct BaseLayerGenerator {
    pub note_list: Vec<NoteStruct>,
    pub sigmoid_nodes: Vec<SigmoidNode>,
    starts: Vec<f64>,
    ends: Vec<f64>,
    note_starts: Vec<f64>,
    note_ends: Vec<f64>,
    last_note_index: Cell<Option<usize>>,
}

impl BaseLayerGenerator {
    pub fn new(note_list: &[NoteStruct]) -> Self {
        let mut result = Self {
            note_list: note_list.to_vec(),
            ..Default::default()
        };
        if note_list.is_empty() {
            return result;
        }
        for pair in note_list.windows(2) {
            let current_note = &pair[0];
            let next_note = &pair[1];
            if current_note.key == next_note.key {
                continue;
            }
            if next_note.start - current_note.end > MAX_BREAK {
                continue;
            }
            let half_left = (next_note.portamento_left * PORTAMENTO_HALFWIDTH_RATIO)
                .max(HALFWIDTH_FLOOR_SECS);
            let half_right = (current_note.portamento_right * PORTAMENTO_HALFWIDTH_RATIO)
                .max(HALFWIDTH_FLOOR_SECS);
            let mid = (current_note.end + next_note.start) / 2.0 + next_note.portamento_offset;
            result.sigmoid_nodes.push(SigmoidNode::new(
                mid,
                half_left,
                half_right,
                current_note.key,
                next_note.key,
            ));
        }
        result.starts = result.sigmoid_nodes.iter().map(|node| node.start).collect();
        result.ends = result.sigmoid_nodes.iter().map(|node| node.end).collect();
        result.note_starts = result.note_list.iter().map(|note| note.start).collect();
        result.note_ends = result.note_list.iter().map(|note| note.end).collect();
        result
    }

    pub fn pitch_at_secs(&self, secs: f64) -> f64 {
        if self.note_list.is_empty() {
            return 0.0;
        }
        let left_index = bisect_right_f64(&self.ends, secs);
        let right_index = bisect_right_f64(&self.starts, secs);
        let query_count = right_index as isize - left_index as isize;
        if query_count <= 0 {
            if let Some(note_index) = self.find_note_index(secs) {
                return self.note_list[note_index].key as f64 * 100.0;
            }
            let nearest = self.nearest_note_index(secs);
            return self.note_list[nearest].key as f64 * 100.0;
        }
        if query_count == 1 {
            return self.sigmoid_nodes[left_index].value_at_secs(secs);
        }
        let first = &self.sigmoid_nodes[left_index];
        let last = &self.sigmoid_nodes[right_index - 1];
        let width = first.end - last.start;
        let bottom = first.value_at_secs(last.start);
        let top = last.value_at_secs(first.end);
        let diff1 = first.slope_at_secs(last.start);
        let diff2 = last.slope_at_secs(first.end);
        let x = secs - last.start;
        let a = (2.0 * (bottom - top) + (diff1 + diff2) * width) / width.powi(3);
        let b = (3.0 * (top - bottom) - 2.0 * diff1 * width - diff2 * width) / width.powi(2);
        a * x.powi(3) + b * x.powi(2) + diff1 * x + bottom
    }

    fn find_note_index(&self, secs: f64) -> Option<usize> {
        if let Some(last_index) = self.last_note_index.get() {
            let note = &self.note_list[last_index];
            if note.start <= secs && secs < note.end {
                return Some(last_index);
            }
            let next_index = last_index + 1;
            if next_index < self.note_list.len() {
                let next_note = &self.note_list[next_index];
                if next_note.start <= secs && secs < next_note.end {
                    self.last_note_index.set(Some(next_index));
                    return Some(next_index);
                }
            }
            if let Some(prev_index) = last_index.checked_sub(1) {
                let prev_note = &self.note_list[prev_index];
                if prev_note.start <= secs && secs < prev_note.end {
                    self.last_note_index.set(Some(prev_index));
                    return Some(prev_index);
                }
            }
        }
        let index = bisect_right_f64(&self.note_starts, secs).checked_sub(1);
        if let Some(index) = index {
            if secs < self.note_ends[index] {
                self.last_note_index.set(Some(index));
                return Some(index);
            }
        }
        self.last_note_index.set(None);
        None
    }

    fn nearest_note_index(&self, secs: f64) -> usize {
        let insert_index = bisect_left_f64(&self.note_starts, secs);
        let mut candidates: Vec<usize> = Vec::new();
        if insert_index < self.note_list.len() {
            candidates.push(insert_index);
        }
        if insert_index > 0 {
            candidates.push(insert_index - 1);
        }
        if candidates.is_empty() {
            return 0;
        }
        let mut best = candidates[0];
        let mut best_distance = f64::INFINITY;
        for index in candidates {
            let distance = (self.note_list[index].start - secs)
                .abs()
                .min((secs - self.note_list[index].end).abs());
            if distance < best_distance {
                best_distance = distance;
                best = index;
            }
        }
        best
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GaussianNode {
    pub origin: f64,
    pub sigma: f64,
    pub depth: f64,
    pub start: f64,
    pub end: f64,
}

impl GaussianNode {
    pub fn new(origin: f64, sigma: f64, depth: f64) -> Self {
        Self {
            origin,
            sigma,
            depth,
            start: origin - 5.0 * sigma,
            end: origin + 5.0 * sigma,
        }
    }

    pub fn value_at_secs(&self, secs: f64) -> f64 {
        self.depth * (-0.5 * ((secs - self.origin) / self.sigma).powi(2)).exp()
    }
}

fn portamento_sigma(portamento: f64) -> f64 {
    (portamento * PORTAMENTO_HALFWIDTH_RATIO).max(HALFWIDTH_FLOOR_SECS)
}

/// `_ActiveRangeMixin._range_matches`.
fn range_matches(
    starts: &[f64],
    ends: &[f64],
    secs: f64,
    left_index: usize,
    right_index: usize,
) -> bool {
    let left_boundary = if left_index > 0 {
        ends[left_index - 1]
    } else {
        f64::NEG_INFINITY
    };
    let right_boundary = if right_index > left_index {
        if left_index < ends.len() {
            ends[left_index]
        } else {
            f64::INFINITY
        }
    } else if right_index < starts.len() {
        starts[right_index]
    } else {
        f64::INFINITY
    };
    left_boundary < secs && secs < right_boundary
}

/// `_ActiveRangeMixin._active_range`.
fn active_range(
    starts: &[f64],
    ends: &[f64],
    last_range: &Cell<Option<(usize, usize)>>,
    secs: f64,
) -> (usize, usize) {
    if let Some((left_index, right_index)) = last_range.get() {
        if range_matches(starts, ends, secs, left_index, right_index) {
            return (left_index, right_index);
        }
    }
    let left_index = bisect_right_f64(ends, secs);
    let right_index = bisect_right_f64(starts, secs);
    last_range.set(Some((left_index, right_index)));
    (left_index, right_index)
}

#[derive(Clone, Debug, Default)]
pub struct GaussianLayerGenerator {
    pub gaussian_nodes: Vec<GaussianNode>,
    starts: Vec<f64>,
    ends: Vec<f64>,
    last_range: Cell<Option<(usize, usize)>>,
}

impl GaussianLayerGenerator {
    pub fn new(note_list: &[NoteStruct]) -> Self {
        let mut result = Self::default();
        if note_list.is_empty() {
            return result;
        }
        let first_note = &note_list[0];
        let sigma_first = portamento_sigma(first_note.portamento_left);
        result.gaussian_nodes.push(GaussianNode::new(
            first_note.start + 1.5 * sigma_first,
            sigma_first,
            -first_note.depth_left * 100.0,
        ));
        for pair in note_list.windows(2) {
            let current_note = &pair[0];
            let next_note = &pair[1];
            let sigma_l = portamento_sigma(current_note.portamento_right);
            let sigma_r = portamento_sigma(next_note.portamento_left);
            if next_note.start - current_note.end > MAX_BREAK {
                result.gaussian_nodes.push(GaussianNode::new(
                    current_note.end - 1.5 * sigma_l,
                    sigma_l,
                    -current_note.depth_right * 100.0,
                ));
                result.gaussian_nodes.push(GaussianNode::new(
                    next_note.start + 1.5 * sigma_r,
                    sigma_r,
                    -next_note.depth_left * 100.0,
                ));
                continue;
            }
            let center = (current_note.end + next_note.start) / 2.0 + next_note.portamento_offset;
            let mut depth_left = current_note.depth_right * 100.0;
            let mut depth_right = next_note.depth_left * 100.0;
            if next_note.key <= current_note.key {
                depth_right = -depth_right;
            } else {
                depth_left = -depth_left;
            }
            result.gaussian_nodes.push(GaussianNode::new(
                center - 1.5 * sigma_l,
                sigma_l,
                depth_left,
            ));
            result.gaussian_nodes.push(GaussianNode::new(
                center + 1.5 * sigma_r,
                sigma_r,
                depth_right,
            ));
        }
        let last_note = &note_list[note_list.len() - 1];
        let sigma_last = portamento_sigma(last_note.portamento_right);
        result.gaussian_nodes.push(GaussianNode::new(
            last_note.end - 1.5 * sigma_last,
            sigma_last,
            -last_note.depth_right * 100.0,
        ));
        result.starts = result
            .gaussian_nodes
            .iter()
            .map(|node| node.start)
            .collect();
        result.ends = result.gaussian_nodes.iter().map(|node| node.end).collect();
        result
    }

    pub fn pitch_diff_at_secs(&self, secs: f64) -> f64 {
        let (left_index, right_index) =
            active_range(&self.starts, &self.ends, &self.last_range, secs);
        let mut sum = 0.0;
        for index in left_index..right_index {
            sum += self.gaussian_nodes[index].value_at_secs(secs);
        }
        sum
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct VibratoNode {
    pub start: f64,
    pub end: f64,
    pub fade_left: f64,
    pub fade_right: f64,
    pub amplitude: f64,
    pub frequency: f64,
    pub phase: f64,
}

impl VibratoNode {
    pub fn value_at_secs(&self, secs: f64) -> f64 {
        let zoom = if self.end - self.start >= self.fade_left + self.fade_right {
            if self.fade_left > 0.0 && secs < self.start + self.fade_left {
                (secs - self.start) / self.fade_left
            } else if self.fade_right > 0.0 && secs > self.end - self.fade_right {
                (self.end - secs) / self.fade_right
            } else {
                1.0
            }
        } else {
            let fade_sum = self.fade_left + self.fade_right;
            let mid = if fade_sum <= 0.0 {
                self.start
            } else {
                (self.start * self.fade_right + self.end * self.fade_left) / fade_sum
            };
            if self.fade_left > 0.0 && secs < mid {
                (secs - self.start) / self.fade_left
            } else if self.fade_right > 0.0 {
                (self.end - secs) / self.fade_right
            } else {
                1.0
            }
        };
        self.amplitude
            * zoom
            * 100.0
            * (std::f64::consts::TAU * self.frequency * (secs - self.start) + self.phase).sin()
    }
}

#[derive(Clone, Debug, Default)]
pub struct VibratoLayerGenerator {
    pub vibrato_nodes: Vec<VibratoNode>,
    starts: Vec<f64>,
    ends: Vec<f64>,
    last_range: Cell<Option<(usize, usize)>>,
}

impl VibratoLayerGenerator {
    pub fn new(note_list: &[NoteStruct]) -> Self {
        let mut result = Self::default();
        for index in 0..note_list.len() {
            let start = note_list[index].start;
            let mut end = note_list[index].end;
            if end - start <= note_list[index].vibrato_start {
                continue;
            }
            if index < note_list.len() - 1 && note_list[index + 1].start - end < MAX_BREAK {
                let next = &note_list[index + 1];
                end += next
                    .portamento_offset
                    .max(0.0)
                    .min(next.vibrato_start.min(next.end - next.start));
            }
            if start >= end {
                continue;
            }
            result.vibrato_nodes.push(VibratoNode {
                start: start + note_list[index].vibrato_start,
                end,
                fade_left: note_list[index].vibrato_left,
                fade_right: note_list[index].vibrato_right,
                amplitude: note_list[index].vibrato_depth / 2.0,
                frequency: note_list[index].vibrato_frequency,
                phase: note_list[index].vibrato_phase,
            });
        }
        result.starts = result
            .vibrato_nodes
            .iter()
            .map(|node| node.start)
            .collect();
        result.ends = result
            .vibrato_nodes
            .iter()
            .map(|node| node.end)
            .collect();
        result
    }

    pub fn pitch_diff_at_secs(&self, secs: f64) -> f64 {
        let (left_index, right_index) =
            active_range(&self.starts, &self.ends, &self.last_range, secs);
        let mut sum = 0.0;
        for index in left_index..right_index {
            sum += self.vibrato_nodes[index].value_at_secs(secs);
        }
        sum
    }
}

#[derive(Clone, Debug)]
pub struct SynthVPitchSimulator {
    pub synchronizer: TimeSynchronizer,
    pub base_layer: BaseLayerGenerator,
    pub gaussian_layer: GaussianLayerGenerator,
    pub vibrato_layer: VibratoLayerGenerator,
}

impl SynthVPitchSimulator {
    pub fn new(synchronizer: TimeSynchronizer, note_list: &[NoteStruct]) -> Self {
        Self {
            synchronizer,
            base_layer: BaseLayerGenerator::new(note_list),
            gaussian_layer: GaussianLayerGenerator::new(note_list),
            vibrato_layer: VibratoLayerGenerator::new(note_list),
        }
    }

    pub fn pitch_at_ticks(&self, ticks: i64) -> f64 {
        self.pitch_at_secs(
            self.synchronizer.get_actual_secs_from_ticks(ticks),
            Some(ticks),
        )
    }

    pub fn pitch_at_secs(&self, secs: f64, _ticks: Option<i64>) -> f64 {
        if self.base_layer.note_list.is_empty() {
            return 0.0;
        }
        self.base_layer.pitch_at_secs(secs)
            + self.vibrato_layer.pitch_diff_at_secs(secs)
            + self.gaussian_layer.pitch_diff_at_secs(secs)
    }
}

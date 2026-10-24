//! Port of `libresvip.core.time_sync`.

use std::sync::Arc;

use crate::core::tick_counter::skip_tempo_list;
use crate::model::base::SongTempo;

/// Converts between project ticks and real seconds.
#[derive(Clone, Debug)]
pub struct TimeSynchronizer {
    pub is_absolute_time_code: bool,
    pub default_tempo: f64,
    /// Shared so the many `TimeSynchronizer` clones on the note/pitch paths
    /// copy a reference count instead of the whole tempo table.
    pub tempo_list: Arc<[SongTempo]>,
    positions: Vec<i64>,
    cum_secs: Vec<f64>,
    /// Prefix sums of `get_actual_ticks_from_ticks`, one per tempo segment:
    /// `cum_ticks[i]` is the remapped position of `positions[i]`. Built with
    /// the same left-to-right accumulation the per-call loop used, so results
    /// are bit-identical while the lookup becomes O(log n).
    cum_ticks: Vec<f64>,
}

impl TimeSynchronizer {
    /// `TimeSynchronizer(ori_tempo_list, skip_ticks, _is_absolute_time_code, _default_tempo)`.
    pub fn new(
        ori_tempo_list: &[SongTempo],
        skip_ticks: i64,
        is_absolute_time_code: bool,
        default_tempo: f64,
    ) -> Self {
        let tempo_list = if skip_ticks > 0 {
            skip_tempo_list(ori_tempo_list, skip_ticks)
        } else {
            ori_tempo_list.to_vec()
        };
        Self::new_owned(tempo_list, is_absolute_time_code, default_tempo)
    }

    /// Like [`TimeSynchronizer::new`] but takes ownership, so a caller that
    /// already holds the tempo list (or an `Arc` to it) avoids the copy.
    pub fn new_owned(
        tempo_list: Vec<SongTempo>,
        is_absolute_time_code: bool,
        default_tempo: f64,
    ) -> Self {
        Self::new_shared(Arc::from(tempo_list), is_absolute_time_code, default_tempo)
    }

    /// Like [`TimeSynchronizer::new_owned`] for an already shared tempo list.
    pub fn new_shared(
        tempo_list: Arc<[SongTempo]>,
        is_absolute_time_code: bool,
        default_tempo: f64,
    ) -> Self {
        let positions: Vec<i64> = tempo_list.iter().map(|tempo| tempo.position).collect();
        let mut cum_secs = vec![0.0f64; tempo_list.len()];
        for i in 1..tempo_list.len() {
            let dt = positions[i] - positions[i - 1];
            cum_secs[i] = cum_secs[i - 1] + dt as f64 / tempo_list[i - 1].bpm / 8.0;
        }
        let mut cum_ticks = vec![0.0f64; tempo_list.len()];
        for i in 1..tempo_list.len() {
            let dt = (positions[i] - positions[i - 1]) as f64;
            cum_ticks[i] = cum_ticks[i - 1] + dt * (default_tempo / tempo_list[i - 1].bpm);
        }
        Self {
            is_absolute_time_code,
            default_tempo,
            tempo_list,
            positions,
            cum_secs,
            cum_ticks,
        }
    }

    /// Convenience constructor mirroring the common CLI/plugin usage:
    /// `TimeSynchronizer(project.song_tempo_list)`.
    pub fn from_project(tempo_list: &[SongTempo]) -> Self {
        Self::new(tempo_list, 0, false, 60.0)
    }

    fn bpm_at(&self, index: usize) -> f64 {
        self.tempo_list[index].bpm
    }

    /// `bisect_right(positions, ticks) - 1`, clamped at zero.
    fn segment_index(&self, ticks: i64) -> usize {
        let idx = self.positions.partition_point(|&position| position <= ticks);
        idx.saturating_sub(1)
    }

    fn secs_at_tick(&self, ticks: i64) -> f64 {
        let idx = self.segment_index(ticks);
        self.cum_secs[idx] + (ticks - self.positions[idx]) as f64 / self.bpm_at(idx) / 8.0
    }

    fn ticks_at_secs(&self, secs: f64) -> f64 {
        let idx = self.cum_secs.partition_point(|&cum| cum <= secs);
        let idx = idx.saturating_sub(1);
        self.positions[idx] as f64
            + (secs - self.cum_secs[idx]) * self.bpm_at(idx) * 8.0
    }

    pub fn get_actual_ticks_from_ticks(&self, ticks: i64) -> f64 {
        if !self.is_absolute_time_code {
            return ticks as f64;
        }
        let idx = self.segment_index(ticks);
        self.cum_ticks[idx]
            + (ticks - self.positions[idx]) as f64 * self.default_tempo / self.bpm_at(idx)
    }

    pub fn get_duration_secs_from_ticks(&self, start_ticks: i64, end_ticks: i64) -> f64 {
        if self.is_absolute_time_code {
            return (self.get_actual_ticks_from_ticks(end_ticks)
                - self.get_actual_ticks_from_ticks(start_ticks))
                / self.default_tempo
                / 8.0;
        }
        self.secs_at_tick(end_ticks) - self.secs_at_tick(start_ticks)
    }

    pub fn get_actual_ticks_from_secs_offset(&self, start_ticks: i64, offset_secs: f64) -> f64 {
        if self.is_absolute_time_code {
            return self.get_actual_ticks_from_ticks(start_ticks)
                + offset_secs * self.default_tempo * 8.0;
        }
        let target_secs = self.secs_at_tick(start_ticks) + offset_secs;
        self.ticks_at_secs(target_secs)
    }

    pub fn get_actual_ticks_from_secs(&self, secs: f64) -> f64 {
        self.get_actual_ticks_from_secs_offset(0, secs)
    }

    pub fn get_actual_secs_from_ticks(&self, ticks: i64) -> f64 {
        self.get_duration_secs_from_ticks(0, ticks)
    }

    /// Batched variant: sorts once, then walks the tempo segments.
    pub fn get_actual_secs_from_ticks_batch(&self, ticks_list: &[i64]) -> Vec<f64> {
        if ticks_list.is_empty() {
            return Vec::new();
        }
        let mut indexed: Vec<(usize, i64)> = ticks_list
            .iter()
            .copied()
            .enumerate()
            .collect();
        indexed.sort_by_key(|&(_, ticks)| ticks);
        let mut results = vec![0.0f64; ticks_list.len()];
        let mut seg_idx = 0usize;
        let n = self.positions.len();
        for (orig_idx, ticks) in indexed {
            while seg_idx + 1 < n && self.positions[seg_idx + 1] <= ticks {
                seg_idx += 1;
            }
            results[orig_idx] = self.cum_secs[seg_idx]
                + (ticks - self.positions[seg_idx]) as f64 / self.bpm_at(seg_idx) / 8.0;
        }
        results
    }
}

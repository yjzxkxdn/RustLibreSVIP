//! Port of `libresvip.plugins.ustx.time_axis`.

use libresvip_core::{LibreSvipError, Result};

use super::model::UstxProject;

const MAXSIZE: i64 = i64::MAX;

/// `TimeSigSegment`.
#[derive(Clone, Debug)]
pub struct TimeSigSegment {
    pub bar_pos: i64,
    pub tick_pos: i64,
    pub beat_per_bar: i64,
    pub beat_unit: i64,
    pub ticks_per_bar: i64,
    #[allow(dead_code)]
    pub ticks_per_beat: i64,
    pub bar_end: i64,
    pub tick_end: i64,
}

/// `TempoSegment`.
#[derive(Clone, Debug)]
pub struct TempoSegment {
    pub tick_pos: i64,
    pub beat_per_bar: i64,
    pub beat_unit: i64,
    pub bpm: f64,
    pub ms_pos: f64,
    pub ms_per_tick: f64,
    pub ticks_per_ms: f64,
    pub tick_end: i64,
    pub ms_end: f64,
}

impl TempoSegment {
    pub fn ticks(&self) -> i64 {
        self.tick_end - self.tick_pos
    }
}

/// `TimeAxis`.
#[derive(Clone, Debug, Default)]
pub struct TimeAxis {
    pub time_sig_segments: Vec<TimeSigSegment>,
    pub tempo_segments: Vec<TempoSegment>,
}

impl TimeAxis {
    /// `build_segments`.
    pub fn build_segments(&mut self, project: &UstxProject) -> Result<()> {
        self.time_sig_segments.clear();
        for index in 0..project.time_signatures.len() {
            let timesig = &project.time_signatures[index];
            let pos_tick = if index > 0 {
                let last_bar_pos = project.time_signatures[index - 1].bar_position;
                let last = self
                    .time_sig_segments
                    .last()
                    .ok_or_else(|| LibreSvipError::msg("missing time signature segment"))?;
                last.tick_pos + last.ticks_per_bar * (timesig.bar_position - last_bar_pos)
            } else {
                if timesig.bar_position != 0 {
                    return Err(LibreSvipError::msg(format!(
                        "assertion failed: bar_position == 0 (got {})",
                        timesig.bar_position
                    )));
                }
                0
            };
            self.time_sig_segments.push(TimeSigSegment {
                bar_pos: timesig.bar_position,
                tick_pos: pos_tick,
                beat_per_bar: timesig.beat_per_bar,
                beat_unit: timesig.beat_unit,
                ticks_per_bar: project.resolution * 4 * timesig.beat_per_bar
                    / timesig.beat_unit,
                ticks_per_beat: project.resolution * 4 / timesig.beat_unit,
                bar_end: MAXSIZE,
                tick_end: MAXSIZE,
            });
        }
        for index in 0..self.time_sig_segments.len().saturating_sub(1) {
            self.time_sig_segments[index].bar_end = self.time_sig_segments[index + 1].bar_pos;
            self.time_sig_segments[index].tick_end = self.time_sig_segments[index + 1].tick_pos;
        }
        if self.time_sig_segments.is_empty() {
            self.time_sig_segments.push(TimeSigSegment {
                bar_pos: 0,
                tick_pos: 0,
                beat_per_bar: project.beat_per_bar,
                beat_unit: project.beat_unit,
                ticks_per_bar: project.resolution * 4 * project.beat_per_bar
                    / project.beat_unit,
                ticks_per_beat: project.resolution * 4 / project.beat_unit,
                bar_end: MAXSIZE,
                tick_end: MAXSIZE,
            });
        }

        self.tempo_segments.clear();
        for sigseg in &self.time_sig_segments {
            self.tempo_segments.push(TempoSegment {
                tick_pos: sigseg.tick_pos,
                beat_per_bar: sigseg.beat_per_bar,
                beat_unit: sigseg.beat_unit,
                bpm: 120.0,
                ms_pos: 0.0,
                ms_per_tick: 0.0,
                ticks_per_ms: 0.0,
                tick_end: MAXSIZE,
                ms_end: f64::INFINITY,
            });
        }
        for (index, tempo) in project.tempos.iter().enumerate() {
            if index == 0 && tempo.position != 0 {
                return Err(LibreSvipError::msg(format!(
                    "assertion failed: tempo.position == 0 (got {})",
                    tempo.position
                )));
            }
            let found = self
                .tempo_segments
                .iter()
                .position(|segment| segment.tick_pos >= tempo.position);
            match found {
                None => {
                    let last = self
                        .tempo_segments
                        .last()
                        .ok_or_else(|| LibreSvipError::msg("missing tempo segment"))?;
                    let (beat_per_bar, beat_unit) = (last.beat_per_bar, last.beat_unit);
                    self.tempo_segments.push(TempoSegment {
                        tick_pos: tempo.position,
                        beat_per_bar,
                        beat_unit,
                        bpm: tempo.bpm,
                        ms_pos: 0.0,
                        ms_per_tick: 0.0,
                        ticks_per_ms: 0.0,
                        tick_end: MAXSIZE,
                        ms_end: f64::INFINITY,
                    });
                }
                Some(index) => {
                    if self.tempo_segments[index].tick_pos == tempo.position {
                        self.tempo_segments[index].bpm = tempo.bpm;
                    } else {
                        // Python indexes `self.tempo_segments[index - 1]`, which
                        // wraps to the last segment when `index == 0`.
                        let previous_index = if index == 0 {
                            self.tempo_segments.len().saturating_sub(1)
                        } else {
                            index - 1
                        };
                        let (beat_per_bar, beat_unit) = {
                            let previous = self.tempo_segments.get(previous_index).ok_or_else(
                                || LibreSvipError::msg("missing preceding tempo segment"),
                            )?;
                            (previous.beat_per_bar, previous.beat_unit)
                        };
                        self.tempo_segments.insert(
                            index,
                            TempoSegment {
                                tick_pos: tempo.position,
                                beat_per_bar,
                                beat_unit,
                                bpm: tempo.bpm,
                                ms_pos: 0.0,
                                ms_per_tick: 0.0,
                                ticks_per_ms: 0.0,
                                tick_end: MAXSIZE,
                                ms_end: f64::INFINITY,
                            },
                        );
                    }
                }
            }
        }
        let count = self.tempo_segments.len();
        for index in 0..count.saturating_sub(1) {
            if self.tempo_segments[index + 1].bpm == 0.0 {
                self.tempo_segments[index + 1].bpm = self.tempo_segments[index].bpm;
            }
            self.tempo_segments[index].tick_end = self.tempo_segments[index + 1].tick_pos;
        }
        for index in 0..self.tempo_segments.len() {
            {
                let segment = &mut self.tempo_segments[index];
                segment.ms_per_tick = 60.0 * 1000.0 * segment.beat_per_bar as f64
                    / (segment.bpm * 4.0 * project.resolution as f64);
                segment.ticks_per_ms =
                    segment.bpm * 4.0 * project.resolution as f64
                        / (60.0 * 1000.0 * segment.beat_per_bar as f64);
            }
            if index > 0 {
                let (previous_ms_pos, previous_ticks, previous_ms_per_tick) = {
                    let previous = &self.tempo_segments[index - 1];
                    (previous.ms_pos, previous.ticks(), previous.ms_per_tick)
                };
                let ms_pos = previous_ms_pos + previous_ticks as f64 * previous_ms_per_tick;
                self.tempo_segments[index].ms_pos = ms_pos;
                self.tempo_segments[index - 1].ms_end = ms_pos;
            }
        }
        Ok(())
    }

    /// `tick_pos_to_ms_pos`.
    pub fn tick_pos_to_ms_pos(&self, tick: f64) -> f64 {
        let segment = self
            .tempo_segments
            .iter()
            .find(|segment| segment.tick_pos as f64 == tick || segment.tick_end as f64 > tick);
        match segment {
            Some(segment) => segment.ms_pos + segment.ms_per_tick * (tick - segment.tick_pos as f64),
            None => 0.0,
        }
    }

    /// `ms_pos_to_tick_pos`.
    pub fn ms_pos_to_tick_pos(&self, ms: f64) -> i64 {
        let segment = self
            .tempo_segments
            .iter()
            .find(|segment| segment.ms_pos == ms || segment.ms_end > ms);
        match segment {
            Some(segment) => crate::ust::math::round_int(
                segment.tick_pos as f64 + (ms - segment.ms_pos) * segment.ticks_per_ms,
            ),
            None => 0,
        }
    }

    /// `ms_between_tick_pos`.
    pub fn ms_between_tick_pos(&self, tick_pos: f64, tick_end: f64) -> f64 {
        self.tick_pos_to_ms_pos(tick_end) - self.tick_pos_to_ms_pos(tick_pos)
    }
}

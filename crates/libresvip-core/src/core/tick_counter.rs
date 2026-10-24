//! Port of `libresvip.core.tick_counter`.

use crate::core::compat::{ceil_i64, find_last_index};
use crate::model::base::{SongTempo, TimeSignature};

/// `skip_tempo_list`: drop tempos before `skip_ticks` and rebase positions.
pub fn skip_tempo_list(tempo_list: &[SongTempo], skip_ticks: i64) -> Vec<SongTempo> {
    let mut result: Vec<SongTempo> = tempo_list
        .iter()
        .filter(|tempo| tempo.position >= skip_ticks)
        .map(|tempo| SongTempo::new(tempo.position - skip_ticks, tempo.bpm))
        .collect();
    if result.first().map(|tempo| tempo.position).unwrap_or(0) > 0 || result.is_empty() {
        let index = find_last_index(tempo_list, |tempo| tempo.position <= skip_ticks);
        let element = match (index, tempo_list.is_empty()) {
            // `tempo_list[-1]` in Python
            (-1, false) => tempo_list.last(),
            // An empty list also lands here via the `index as usize` cast.
            (index, _) => tempo_list.get(index as usize),
        };
        let tempo = match element {
            Some(element) => {
                let mut found = element.clone();
                found.position = 0;
                found
            }
            None => SongTempo::default(),
        };
        result.insert(0, tempo);
    }
    result
}

/// `skip_beat_list`: drop time signatures before `skip_bars` and rebase.
pub fn skip_beat_list(beat_list: &[TimeSignature], skip_bars: i64) -> Vec<TimeSignature> {
    let mut result: Vec<TimeSignature> = beat_list
        .iter()
        .filter(|beat| beat.bar_index >= skip_bars)
        .map(|beat| {
            TimeSignature::new(beat.bar_index - skip_bars, beat.numerator, beat.denominator)
        })
        .collect();
    if result.first().map(|beat| beat.bar_index).unwrap_or(0) > 0 || result.is_empty() {
        let index = find_last_index(beat_list, |beat| beat.bar_index <= skip_bars);
        let element = match (index, beat_list.is_empty()) {
            (-1, false) => beat_list.last(),
            (index, _) => beat_list.get(index as usize),
        };
        let beat = match element {
            Some(element) => {
                let mut found = element.clone();
                found.bar_index = 0;
                found
            }
            None => TimeSignature::default(),
        };
        result.insert(0, beat);
    }
    result
}

/// `shift_tempo_list`: move all tempos except the first.
pub fn shift_tempo_list(tempo_list: &[SongTempo], shift_ticks: i64) -> Vec<SongTempo> {
    let mut result: Vec<SongTempo> = tempo_list.iter().take(1).cloned().collect();
    result.extend(
        tempo_list
            .iter()
            .skip(1)
            .map(|tempo| SongTempo::new(tempo.position + shift_ticks, tempo.bpm)),
    );
    result
}

/// `shift_beat_list`: move all time signatures except the first.
pub fn shift_beat_list(beat_list: &[TimeSignature], shift_bars: i64) -> Vec<TimeSignature> {
    let mut result: Vec<TimeSignature> = beat_list.iter().take(1).cloned().collect();
    result.extend(beat_list.iter().skip(1).map(|beat| {
        TimeSignature::new(
            beat.bar_index + shift_bars,
            beat.numerator,
            beat.denominator,
        )
    }));
    result
}

/// `calc_bar_index`.
pub fn calc_bar_index(tick: f64, base_tick: f64, beat: &TimeSignature) -> i64 {
    beat.bar_index + ceil_i64((tick - base_tick) / beat.bar_length())
}

/// `find_bar_index` — errors (`ValueError`) when `beat_list` is empty.
pub fn find_bar_index(beat_list: &[TimeSignature], ticks: i64) -> crate::Result<i64> {
    let Some(last) = beat_list.last() else {
        return Err(crate::LibreSvipError::msg("beat_list must not be empty"));
    };
    let mut tick = 0.0f64;
    for pair in beat_list.windows(2) {
        let beat = &pair[0];
        let next_beat = &pair[1];
        let next_tick = tick + beat.bar_length() * (next_beat.bar_index - beat.bar_index) as f64;
        if (ticks as f64) < next_tick {
            return Ok(calc_bar_index(ticks as f64, tick, beat));
        }
        tick = next_tick;
    }
    Ok(calc_bar_index(ticks as f64, tick, last))
}

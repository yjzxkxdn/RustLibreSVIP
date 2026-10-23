//! Port of `libresvip.plugins.mid.midi_parser`.

use std::collections::HashMap;

use libresvip_core::core::constants::{DEFAULT_PHONEME, TICKS_IN_BEAT};
use libresvip_core::core::tick_counter::find_bar_index;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::core::warning_types::show_warning;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{
    Note, ParamCurve, Params, Project, SingingTrack, SongTempo, TimeSignature, Track,
};
use libresvip_core::model::point::Point;

use super::codec::decode_ignore;
use super::midi_file::{ChannelEvent, EventKind, MetaEvent, MidiEvent, MidiFile, tempo2bpm};
use super::pitch::{PitchSimulator, Portamento, RelativePitchCurve};
use super::text::contains_latin;
use super::{InputOptions, MultiChannelOption};

/// `constants.VELOCITY_CONSTANT`.
const VELOCITY_CONSTANT: f64 = 1.75;
/// `libresvip.utils.binary.midi.EXPRESSION_CONSTANT`.
const EXPRESSION_CONSTANT: f64 = 2.0;

/// `libresvip.utils.music_math.ratio_to_db`.
fn ratio_to_db(ratio: f64) -> f64 {
    if ratio == 0.0 {
        return f64::NEG_INFINITY;
    }
    20.0 * ratio.log10()
}

/// `midi_parser.velocity_to_db_change`.
fn velocity_to_db_change(value: f64) -> f64 {
    ratio_to_db((value / 127.0).powf(VELOCITY_CONSTANT) + 1e-6)
}

/// `libresvip.utils.binary.midi.cc11_to_db_change`.
fn cc11_to_db_change(value: f64) -> f64 {
    ratio_to_db((value / 127.0).powf(EXPRESSION_CONSTANT) + 1e-6)
}

/// `round(value, 3)` (ties to even), used for the imported BPM.
fn round_ndigits(value: f64, digits: usize) -> f64 {
    if !value.is_finite() {
        return value;
    }
    format!("{value:.digits$}")
        .parse::<f64>()
        .unwrap_or(value)
}

fn round_to_i64(value: f64) -> i64 {
    value.round_ties_even() as i64
}

/// `_convert_delta_to_cumulative`, returning a converted copy of the tracks.
fn cumulative_tracks(midi_file: &MidiFile) -> Vec<Vec<MidiEvent>> {
    midi_file
        .tracks
        .iter()
        .map(|track| {
            let mut tick = 0i64;
            track
                .iter()
                .map(|event| {
                    tick += event.time;
                    MidiEvent {
                        time: tick,
                        kind: event.kind.clone(),
                    }
                })
                .collect()
        })
        .collect()
}

struct MidiParser<'a> {
    options: &'a InputOptions,
    ticks_per_beat: i16,
    first_bar_length: i64,
    synchronizer: TimeSynchronizer,
    selected_channels: Vec<u8>,
    time_signatures: Vec<TimeSignature>,
}

/// Port of `MidiParser.parse_project`.
pub fn parse_project(midi_file: &MidiFile, options: &InputOptions) -> Result<Project> {
    let mut selected_channels: Vec<u8> = Vec::new();
    match options.multi_channel {
        MultiChannelOption::First => selected_channels.push(0),
        MultiChannelOption::Split => {}
        MultiChannelOption::Custom => {
            for expression in options.channels.split(',') {
                if expression.is_empty() {
                    continue;
                }
                let (start, end) = match expression.split_once('-') {
                    Some((start, end)) => (start, Some(end)),
                    None => (expression, None),
                };
                if is_positive_digit_string(start) {
                    match end {
                        None => {
                            if let Ok(value) = start.parse::<i64>() {
                                selected_channels.push((value - 1) as u8);
                            }
                        }
                        Some(end) if end.is_empty() => {
                            if let Ok(value) = start.parse::<i64>() {
                                selected_channels.push((value - 1) as u8);
                            }
                        }
                        Some(end) => {
                            if is_positive_digit_string(end) {
                                if let (Ok(start), Ok(end)) =
                                    (start.parse::<i64>(), end.parse::<i64>())
                                {
                                    // Python: `range(int(start) - 1, int(end))`
                                    for channel in (start - 1)..end {
                                        selected_channels.push(channel as u8);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    let parser = MidiParser {
        options,
        ticks_per_beat: midi_file.ticks_per_beat,
        first_bar_length: 0,
        synchronizer: TimeSynchronizer::from_project(&[]),
        selected_channels,
        time_signatures: Vec::new(),
    };
    parser.parse(midi_file)
}

/// Python `str.isdigit()` restricted to ASCII plus the "no leading zero" rule.
fn is_positive_digit_string(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('0')
        && value.bytes().all(|byte| byte.is_ascii_digit())
}

impl MidiParser<'_> {
    fn tick_rate(&self) -> f64 {
        TICKS_IN_BEAT as f64 / f64::from(self.ticks_per_beat)
    }

    fn parse(mut self, midi_file: &MidiFile) -> Result<Project> {
        if midi_file.ticks_per_beat < 0 {
            return Err(LibreSvipError::msg(
                "MIDI file with SMPTE time division is not supported.",
            ));
        }
        self.ticks_per_beat = midi_file.ticks_per_beat;
        let tracks = cumulative_tracks(midi_file);
        if let Some(master_track) = tracks.first() {
            self.time_signatures = self.parse_time_signatures(master_track);
        }
        let song_tempo_list = self.parse_tempo(&tracks)?;
        self.synchronizer = TimeSynchronizer::from_project(&song_tempo_list);
        let track_list = self.parse_tracks(&tracks)?;
        Ok(Project {
            version: String::new(),
            song_tempo_list,
            time_signature_list: self.time_signatures,
            track_list,
        })
    }

    /// `parse_time_signatures`; also sets `first_bar_length`.
    fn parse_time_signatures(&mut self, master_track: &[MidiEvent]) -> Vec<TimeSignature> {
        let mut time_signature_changes: Vec<TimeSignature> = Vec::new();
        if self.options.import_time_signatures {
            let mut prev_ticks = 0i64;
            let mut measure = 0.0f64;
            for event in master_track {
                if let EventKind::Meta(MetaEvent::TimeSignature {
                    numerator,
                    denominator,
                    ..
                }) = &event.kind
                {
                    let tick = event.time;
                    let tick_in_full_note = match time_signature_changes.last() {
                        Some(previous) => round_to_i64(
                            previous.bar_length_with(i64::from(self.ticks_per_beat)),
                        ) as f64,
                        None => 4.0 * f64::from(self.ticks_per_beat),
                    };
                    measure += (tick - prev_ticks) as f64 / tick_in_full_note;
                    time_signature_changes.push(TimeSignature::new(
                        measure.floor() as i64,
                        i64::from(*numerator),
                        i64::from(*denominator),
                    ));
                    prev_ticks = tick;
                }
            }
        }
        if time_signature_changes.is_empty() || time_signature_changes[0].bar_index > 0 {
            time_signature_changes.insert(0, TimeSignature::new(0, 4, 4));
        }
        self.first_bar_length = round_to_i64(time_signature_changes[0].bar_length());
        time_signature_changes
    }

    /// `parse_tempo`.
    fn parse_tempo(&self, tracks: &[Vec<MidiEvent>]) -> Result<Vec<SongTempo>> {
        let mut tempos: Vec<SongTempo> = Vec::new();
        for track in tracks {
            for event in track {
                if let EventKind::Meta(MetaEvent::SetTempo(tempo)) = &event.kind {
                    let bpm = round_ndigits(tempo2bpm(*tempo), 3);
                    let tick = round_to_i64(event.time as f64 * self.tick_rate());
                    let last_tempo = tempos.last().map(|item| item.bpm);
                    if Some(bpm) != last_tempo {
                        tempos.push(SongTempo::new(tick, bpm));
                    }
                }
            }
        }
        if tempos.is_empty() {
            show_warning("No tempo labels found in the imported project.");
            tempos.push(SongTempo::new(0, self.options.default_bpm));
        } else {
            tempos.sort_by_key(|tempo| tempo.position);
        }
        Ok(tempos)
    }

    /// `parse_tracks` 0- `parse_track` for every MIDI track.
    fn parse_tracks(&self, midi_tracks: &[Vec<MidiEvent>]) -> Result<Vec<Track>> {
        let mut tracks = Vec::new();
        for (track_idx, track) in midi_tracks.iter().enumerate() {
            tracks.extend(self.parse_track(track_idx, track)?);
        }
        Ok(tracks)
    }

    /// `parse_track`.
    fn parse_track(&self, track_idx: usize, track: &[MidiEvent]) -> Result<Vec<Track>> {
        let mut tracks: Vec<Track> = Vec::new();
        // `more_itertools.bucket` — channels in order of first appearance.
        let mut buckets: Vec<(u8, Vec<&MidiEvent>)> = Vec::new();
        for event in track {
            if let EventKind::Channel { channel, .. } = &event.kind {
                match buckets.iter_mut().find(|(bucket, _)| bucket == channel) {
                    Some((_, events)) => events.push(event),
                    None => buckets.push((*channel, vec![event])),
                }
            }
        }
        let mut lyrics: HashMap<i64, String> = HashMap::new();
        if self.options.import_lyrics {
            for event in track {
                if let EventKind::Meta(MetaEvent::Lyrics(text)) = &event.kind {
                    lyrics.insert(
                        event.time,
                        decode_ignore(text, &self.options.lyric_encoding)?,
                    );
                }
            }
        }
        for (channel, events) in &buckets {
            if !self.selected_channels.is_empty() && !self.selected_channels.contains(channel) {
                continue;
            }
            let mut last_note_on: HashMap<u8, Vec<i64>> = HashMap::new();
            let mut pitchbend_range_changed: HashMap<i64, Vec<u8>> = HashMap::new();
            let mut notes: Vec<Note> = Vec::new();
            let mut rel_pitch_points: Vec<Point> = Vec::new();
            let mut expression = ParamCurve::default();
            let mut pitch_bend_sensitivity = super::pitch::DEFAULT_PITCH_BEND_SENSITIVITY;
            let mut volume_base = 0.0f64;
            for event in events {
                match &event.kind {
                    EventKind::Channel { data, .. } => match data {
                        ChannelEvent::NoteOn { note, velocity } if *velocity > 0 => {
                            rel_pitch_points.push(Point::new(
                                round_to_i64(event.time as f64 * self.tick_rate()),
                                0,
                            ));
                            last_note_on.entry(*note).or_default().push(event.time);
                        }
                        ChannelEvent::NoteOff { note, .. }
                        | ChannelEvent::NoteOn { note, velocity: 0 } => {
                            let key = *note;
                            if let Some(open_notes) = last_note_on.get(&key) {
                                let end_tick = event.time;
                                let mut notes_to_close: Vec<i64> = Vec::new();
                                let mut notes_to_keep: Vec<i64> = Vec::new();
                                for start_tick in open_notes {
                                    if *start_tick != end_tick {
                                        notes_to_close.push(*start_tick);
                                    } else {
                                        notes_to_keep.push(*start_tick);
                                    }
                                }
                                for start_tick in &notes_to_close {
                                    let mut new_note = Note::new(
                                        round_to_i64(*start_tick as f64 * self.tick_rate()),
                                        round_to_i64(
                                            (end_tick - *start_tick) as f64 * self.tick_rate(),
                                        ),
                                        i64::from(key),
                                        "",
                                    );
                                    let lyric = lyrics
                                        .get(start_tick)
                                        .cloned()
                                        .unwrap_or_else(|| DEFAULT_PHONEME.to_owned());
                                    if contains_latin(&lyric) {
                                        new_note.pronunciation = Some(lyric.clone());
                                    }
                                    new_note.lyric = lyric;
                                    notes.push(new_note);
                                }
                                if !notes_to_close.is_empty() && !notes_to_keep.is_empty() {
                                    last_note_on.insert(key, notes_to_keep);
                                } else {
                                    last_note_on.remove(&key);
                                }
                            }
                        }
                        ChannelEvent::PitchWheel { pitch } if self.options.import_pitch => {
                            let divisor = if *pitch > 0 {
                                f64::from(super::pitch::PITCH_MAX_VALUE as i32)
                            } else {
                                f64::from(super::pitch::PITCH_MAX_VALUE as i32 + 1)
                            };
                            rel_pitch_points.push(Point::new(
                                round_to_i64(event.time as f64 * self.tick_rate()),
                                round_to_i64(
                                    pitch_bend_sensitivity as f64 * f64::from(*pitch) / divisor,
                                ),
                            ));
                        }
                        ChannelEvent::ControlChange { control, value } => {
                            if self.options.import_pitch {
                                if *control == 6
                                    && pitchbend_range_changed
                                        .get(&event.time)
                                        .map(|values| values.len())
                                        .unwrap_or(0)
                                        >= 2
                                {
                                    pitch_bend_sensitivity = i64::from(*value);
                                } else if (*control == 101 && *value == 0)
                                    || (*control == 100 && *value == 0)
                                {
                                    pitchbend_range_changed
                                        .entry(event.time)
                                        .or_default()
                                        .push(*value);
                                }
                            }
                            if self.options.import_volume {
                                if *control == 11 && *value != 0 {
                                    expression.points.push(Point::new(
                                        round_to_i64(event.time as f64 * self.tick_rate()),
                                        round_to_i64(
                                            volume_base + cc11_to_db_change(f64::from(*value)),
                                        ),
                                    ));
                                } else if *control == 7 && *value != 0 {
                                    volume_base = velocity_to_db_change(f64::from(*value));
                                }
                            }
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
            if let Some(position) = overlapped_pos(&mut notes) {
                let bar_index = find_bar_index(&self.time_signatures, position)?;
                return Err(LibreSvipError::msg(format!(
                    "Notes overlapped near bar {bar_index}"
                )));
            }
            let mut edited_params = Params {
                volume: expression,
                ..Default::default()
            };
            if self.options.import_pitch {
                let pitch_simulator = PitchSimulator::new(
                    &self.synchronizer,
                    Portamento::no_portamento(),
                    &notes,
                    &self.time_signatures,
                )?;
                rel_pitch_points.sort_by_key(|point| point.x);
                edited_params.pitch =
                    RelativePitchCurve::new(self.first_bar_length)
                        .to_absolute(&rel_pitch_points, &pitch_simulator)?;
            }
            if !notes.is_empty() {
                tracks.push(Track::Singing(SingingTrack {
                    title: format!("Track {} ({})", track_idx + 1, channel),
                    note_list: notes,
                    edited_params,
                    ..Default::default()
                }));
            }
        }
        Ok(tracks)
    }
}

/// `note_overlap.overlapped_pos`; sorts `notes` by `start_pos` in place.
fn overlapped_pos(notes: &mut [Note]) -> Option<i64> {
    if notes.len() < 2 {
        return None;
    }
    notes.sort_by_key(|note| note.start_pos);
    let mut max_end: Option<i64> = None;
    for note in notes.iter() {
        if let Some(end) = max_end {
            if note.start_pos < end {
                return Some(note.start_pos);
            }
        }
        max_end = Some(match max_end {
            Some(end) => end.max(note.end_pos()),
            None => note.end_pos(),
        });
    }
    None
}

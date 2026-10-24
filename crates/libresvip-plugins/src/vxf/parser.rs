//! Port of `libresvip.plugins.vxf.vx_beta_parser`.

use std::collections::HashMap;

use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::core::warning_types::{show_warning_kind, WarningKind};
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{Note, Project, SingingTrack, SongTempo, TimeSignature, Track};
use libresvip_core::model::point::Point;

use super::binary::{VxEvent, VxFile, VxTrack};
use super::model::VxPitchData;
use super::music::{py_round, round_decimals};
use super::pyjson;

#[derive(Clone, Debug)]
pub struct InputOptions {
    pub import_pitch: bool,
    pub default_bpm: f64,
}

impl Default for InputOptions {
    fn default() -> Self {
        Self {
            import_pitch: true,
            default_bpm: libresvip_core::core::constants::DEFAULT_BPM,
        }
    }
}

/// Plugin-scoped `gettext`: the CLI merges the plugin catalog into the core
/// one, the converter itself only sees the core catalog.
fn translate(message: &str) -> String {
    use libresvip_core::core::config::read_settings;
    use libresvip_core::utils::translation::{core_catalog, plugin_catalog, TranslationChain};
    let lang = read_settings(|settings| settings.language.value()).to_owned();
    let mut chain = TranslationChain::new();
    chain.push(core_catalog(&lang));
    chain.push(plugin_catalog("vxf", &lang));
    chain.gettext(message)
}

pub struct VxBetaParser {
    options: InputOptions,
    ticks_per_beat: i64,
    first_bar_length: i64,
    synchronizer: TimeSynchronizer,
}

impl VxBetaParser {
    pub fn new(options: InputOptions) -> Self {
        Self {
            options,
            ticks_per_beat: libresvip_core::core::constants::TICKS_IN_BEAT,
            first_bar_length: 0,
            synchronizer: TimeSynchronizer::from_project(&[]),
        }
    }

    fn tick_rate(&self) -> f64 {
        libresvip_core::core::constants::TICKS_IN_BEAT as f64 / self.ticks_per_beat as f64
    }

    pub fn parse_project(&mut self, vx_project: &VxFile) -> Result<Project> {
        self.ticks_per_beat = vx_project.ticks_per_beat;
        if self.ticks_per_beat == 0 {
            return Err(LibreSvipError::Validation(
                "ticks per beat must not be zero".to_owned(),
            ));
        }
        let mut tracks = vx_project.tracks.clone();
        Self::convert_delta_to_cumulative(&mut tracks);
        let mut time_signature_list = Vec::new();
        if let Some(master_track) = tracks.first() {
            time_signature_list = self.parse_time_signatures(master_track);
        }
        let song_tempo_list = self.parse_tempos(&tracks);
        self.synchronizer = TimeSynchronizer::from_project(&song_tempo_list);
        let mut track_list = Vec::new();
        for track in &tracks {
            track_list.push(Track::Singing(self.parse_track(track)?));
        }
        Ok(Project {
            warnings: Vec::new(),
            version: String::new(),
            song_tempo_list,
            time_signature_list,
            track_list,
        })
    }

    fn convert_delta_to_cumulative(tracks: &mut [VxTrack]) {
        for track in tracks {
            let mut tick = 0i64;
            for event in &mut track.events {
                let time = event.time_mut();
                *time += tick;
                tick = *time;
            }
        }
    }

    fn parse_tempos(&self, tracks: &[VxTrack]) -> Vec<SongTempo> {
        let mut tempos: Vec<SongTempo> = Vec::new();
        for track in tracks {
            for event in &track.events {
                if let VxEvent::Tempo { time, tempo } = *event {
                    let tempo = round_decimals(tempo, 3);
                    let tick = py_round(time as f64 * self.tick_rate());
                    let last_tempo = tempos.last().map(|item| item.bpm);
                    if last_tempo != Some(tempo) {
                        tempos.push(SongTempo::new(tick, tempo));
                    }
                }
            }
        }
        if tempos.is_empty() {
            show_warning_kind(WarningKind::TempoDropped, translate(
                "No tempo labels found in the imported project.",
            ));
            tempos.push(SongTempo::new(0, self.options.default_bpm));
        } else {
            tempos.sort_by_key(|tempo| tempo.position);
        }
        tempos
    }

    fn parse_time_signatures(&mut self, master_track: &VxTrack) -> Vec<TimeSignature> {
        let mut time_signature_changes: Vec<TimeSignature> = Vec::new();
        let mut prev_ticks = 0i64;
        let mut measure = 0.0f64;
        for event in &master_track.events {
            if let VxEvent::TimeSignature {
                time,
                numerator,
                denominator,
            } = *event
            {
                let tick = time;
                let tick_in_full_note = match time_signature_changes.last() {
                    None => 4.0 * self.ticks_per_beat as f64,
                    Some(previous) => {
                        py_round(previous.bar_length_with(self.ticks_per_beat)) as f64
                    }
                };
                measure += (tick - prev_ticks) as f64 / tick_in_full_note;
                time_signature_changes.push(TimeSignature::new(
                    measure.floor() as i64,
                    numerator,
                    denominator,
                ));
                prev_ticks = tick;
            }
        }
        if time_signature_changes.is_empty()
            || time_signature_changes[0].bar_index > 0
        {
            time_signature_changes.insert(0, TimeSignature::new(0, 4, 4));
        }
        self.first_bar_length = py_round(time_signature_changes[0].bar_length());
        time_signature_changes
    }

    fn parse_track(&self, track: &VxTrack) -> Result<SingingTrack> {
        let mut lyrics: HashMap<usize, String> = HashMap::new();
        let mut prev_index: Option<usize> = None;
        for event in &track.events {
            if let VxEvent::Lyric {
                seq_stat,
                seq_num,
                text,
                ..
            } = event
            {
                let seq_stat = (*seq_stat as i64 - 16).div_euclid(64);
                let index = seq_num.unwrap_or(0) as usize;
                match seq_stat {
                    0 => {
                        lyrics.insert(index, text.clone());
                        prev_index = None;
                    }
                    1 => {
                        lyrics.insert(index, text.clone());
                        prev_index = Some(index);
                    }
                    2 => {
                        if let Some(index) = prev_index {
                            lyrics
                                .entry(index)
                                .or_insert_with(|| "l-aa".to_owned())
                                .push_str(text);
                        }
                    }
                    3 => {
                        if let Some(index) = prev_index {
                            lyrics
                                .entry(index)
                                .or_insert_with(|| "l-aa".to_owned())
                                .push_str(text);
                            prev_index = None;
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut last_note_on: Option<(i64, i64)> = None;
        let mut notes: Vec<Note> = Vec::new();
        for event in &track.events {
            match *event {
                VxEvent::NoteOn { time, note, .. } => {
                    last_note_on = Some((time, note));
                }
                VxEvent::NoteOff { time, note, .. } => {
                    if let Some((start, _)) = last_note_on {
                        let lyric = lyrics
                            .get(&notes.len())
                            .cloned()
                            .unwrap_or_else(|| "l-aa".to_owned());
                        notes.push(Note::new(
                            py_round(start as f64 * self.tick_rate()),
                            py_round((time - start) as f64 * self.tick_rate()),
                            note,
                            lyric,
                        ));
                        last_note_on = None;
                    }
                }
                _ => {}
            }
        }
        let mut singing_track = SingingTrack {
            title: track
                .title_parts
                .iter()
                .map(|(_, name)| name.as_str())
                .collect::<String>(),
            note_list: notes,
            ..Default::default()
        };
        let buffer: String = track
            .events
            .iter()
            .filter_map(|event| match event {
                VxEvent::Metadata { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        if self.options.import_pitch && !buffer.is_empty() {
            let value = pyjson::parse(&buffer)?;
            let pitch_data = VxPitchData::from_json(&value)?;
            let sequence = &pitch_data.time_based_pitch_sequence;
            let mut pitch_points = vec![Point::start_point()];
            let mut prev_pos: Option<i64> = None;
            for point in &sequence.pitch_sequence {
                if let Some(prev) = prev_pos {
                    if prev != 0
                        && point.position - prev > 1
                        && !pitch_points.is_empty()
                    {
                        let last_x = pitch_points.last().map(|item| item.x).unwrap_or(0);
                        pitch_points.push(Point::new(last_x, -100));
                    }
                }
                let tick = self
                    .synchronizer
                    .get_actual_ticks_from_secs(
                        point.position as f64 * sequence.time_frame_period_seconds,
                    )
                    as i64
                    + self.first_bar_length;
                if pitch_points.last().map(|item| item.y) == Some(-100) {
                    pitch_points.push(Point::new(tick, -100));
                }
                pitch_points.push(Point::new(tick, point.pitch as i64 + 6900));
                prev_pos = Some(point.position);
            }
            if pitch_points.len() > 1 {
                let last_x = pitch_points.last().map(|item| item.x).unwrap_or(0);
                pitch_points.push(Point::new(last_x, -100));
            }
            pitch_points.push(Point::end_point());
            singing_track.edited_params.pitch.points = pitch_points;
        }
        Ok(singing_track)
    }
}

//! Parser for Vocaloid 2 sequence files (`vsq_parser.py`).

use libresvip_core::core::constants::TICKS_IN_BEAT;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{
    Note, ParamCurve, Params, Project, SingingTrack, SongTempo, TimeSignature, Track,
};

use super::ini::Ini;
use super::midi::{parse_midi, tempo2bpm, MetaEvent, MidiEventKind, MidiTrack};
use crate::vsqx::vocaloid::{
    py_round, ControllerCurve, ControllerEvent, PitchBendData, VocaloidPitchHandler,
};

pub const BREATH_PATTERN: &str = r"br[1-5]";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BreathOption {
    Ignore,
    Keep,
}

/// `InputOptions` of the vsq plugin.
#[derive(Clone, Debug)]
pub struct InputOptions {
    pub import_pitch: bool,
    pub import_volume: bool,
    pub import_breath: bool,
    pub import_gender: bool,
    pub import_strength: bool,
    pub lyric_encoding: String,
    pub breath: BreathOption,
}

impl Default for InputOptions {
    fn default() -> Self {
        Self {
            import_pitch: true,
            import_volume: true,
            import_breath: true,
            import_gender: true,
            import_strength: true,
            lyric_encoding: "SHIFT_JIS".to_owned(),
            breath: BreathOption::Ignore,
        }
    }
}

/// Resolve a Python codec name to an `encoding_rs` encoding.
pub fn encoding_by_name(name: &str) -> Result<&'static encoding_rs::Encoding> {
    encoding_rs::Encoding::for_label(name.trim().as_bytes()).ok_or_else(|| {
        LibreSvipError::msg(format!("unknown encoding: {name}"))
    })
}

/// `bytes.decode(encoding, "replace")`.
pub fn decode_text(bytes: &[u8], encoding: &'static encoding_rs::Encoding) -> String {
    let (decoded, _, _) = encoding.decode(bytes);
    decoded.into_owned()
}

/// `str.encode(encoding, "replace")` — Python replaces unencodable characters
/// with `?`, while the Encoding Standard uses numeric character references.
pub fn encode_text(text: &str, encoding: &'static encoding_rs::Encoding) -> Vec<u8> {
    if encoding == encoding_rs::UTF_8 {
        return text.as_bytes().to_vec();
    }
    let mut out: Vec<u8> = Vec::with_capacity(text.len());
    let mut buffer = [0u8; 4];
    for character in text.chars() {
        let encoded_char = character.encode_utf8(&mut buffer);
        let (encoded, _, had_errors) = encoding.encode(encoded_char);
        if had_errors {
            out.push(b'?');
        } else {
            out.extend_from_slice(&encoded);
        }
    }
    out
}

fn read_ini_with_fallback(text: &str) -> Result<Ini> {
    match Ini::parse(text) {
        Ok(ini) => Ok(ini),
        Err(_) => {
            // `configparser` error paths retry after dropping the last line.
            let trimmed = match text.rfind('\n') {
                Some(index) => &text[..index],
                None => text,
            };
            Ini::parse(trimmed)
        }
    }
}

fn get_int(ini: &Ini, section: &str, option: &str) -> Result<i64> {
    let value = ini
        .get(section, option)
        .ok_or_else(|| LibreSvipError::Parse(format!("missing option {option} in [{section}]")))?;
    value.trim().parse::<i64>().map_err(|_| {
        LibreSvipError::Parse(format!(
            "invalid integer {value:?} for option {option} in [{section}]"
        ))
    })
}

pub struct VsqParser<'a> {
    options: &'a InputOptions,
    synchronizer: TimeSynchronizer,
    first_bar_length: i64,
    time_signatures: Vec<TimeSignature>,
}

/// `VsqParser.parse_project`.
pub fn parse_project(bytes: &[u8], options: &InputOptions) -> Result<Project> {
    let midi = parse_midi(bytes)?;
    let ticks_per_beat = midi.ticks_per_beat;
    let tick_rate = TICKS_IN_BEAT as f64 / ticks_per_beat as f64;

    let tracks_as_text = extract_vsq_text(&midi.tracks, options)?;
    if tracks_as_text.is_empty() {
        return Err(LibreSvipError::msg("no VSQ text events found"));
    }
    let measure_prefix = get_measure_prefix(&tracks_as_text[0])?;
    let master_track = midi
        .tracks
        .first()
        .ok_or_else(|| LibreSvipError::msg("MIDI file has no tracks"))?;
    let time_signatures = parse_time_signatures(master_track, ticks_per_beat)?;
    let first_bar_length = py_round(
        time_signatures
            .first()
            .map(|signature| signature.bar_length_with(ticks_per_beat))
            .unwrap_or(0.0),
    );
    let tick_prefix = first_bar_length * measure_prefix;
    let song_tempo_list = parse_tempo(master_track, tick_prefix, tick_rate)?;
    let synchronizer = TimeSynchronizer::from_project(&song_tempo_list);

    let parser = VsqParser {
        options,
        synchronizer,
        first_bar_length,
        time_signatures: time_signatures.clone(),
    };
    let mut track_list = Vec::with_capacity(tracks_as_text.len());
    for (index, text) in tracks_as_text.iter().enumerate() {
        track_list.push(parser.parse_track(text, index + 1, tick_prefix)?);
    }
    Ok(Project {
        version: String::new(),
        song_tempo_list,
        time_signature_list: time_signatures,
        track_list,
    })
}

fn extract_vsq_text(
    tracks: &[MidiTrack],
    options: &InputOptions,
) -> Result<Vec<String>> {
    let encoding = encoding_by_name(&options.lyric_encoding)?;
    let mut result = Vec::new();
    for track in tracks {
        let mut text = String::new();
        for event in &track.events {
            if let MidiEventKind::Meta(MetaEvent::Text(bytes)) = &event.kind {
                let decoded = decode_text(bytes, encoding);
                let without_prefix = decoded.strip_prefix("DM:").unwrap_or(&decoded);
                let payload = without_prefix.splitn(2, ':').nth(1).unwrap_or("");
                text.push_str(payload);
            }
        }
        if !text.is_empty() {
            result.push(text);
        }
    }
    Ok(result)
}

pub fn get_measure_prefix(text: &str) -> Result<i64> {
    let ini = read_ini_with_fallback(text)?;
    match ini.get("Master", "PreMeasure") {
        Some(value) => value.trim().parse::<i64>().map_err(|_| {
            LibreSvipError::msg(format!("invalid literal for int() with base 10: {value:?}"))
        }),
        None => Ok(1),
    }
}

fn parse_time_signatures(master_track: &MidiTrack, ticks_per_beat: i64) -> Result<Vec<TimeSignature>> {
    let mut time_signature_changes: Vec<TimeSignature> = Vec::new();
    let mut prev_ticks = 0i64;
    let mut measure = 0.0f64;
    for event in &master_track.events {
        if let MidiEventKind::Meta(MetaEvent::TimeSignature {
            numerator,
            denominator,
            ..
        }) = &event.kind
        {
            let tick_in_full_note = time_signature_changes
                .last()
                .map(|signature| signature.bar_length_with(ticks_per_beat))
                .unwrap_or(4.0 * ticks_per_beat as f64);
            let tick = event.time;
            measure += (tick - prev_ticks) as f64 / tick_in_full_note;
            time_signature_changes.push(TimeSignature {
                bar_index: (measure.floor() as i64).max(0),
                numerator: *numerator as i64,
                denominator: *denominator,
            });
            prev_ticks = tick;
        }
    }
    if time_signature_changes.is_empty() {
        time_signature_changes.push(TimeSignature::new(0, 4, 4));
    }
    Ok(time_signature_changes)
}

fn parse_tempo(
    master_track: &MidiTrack,
    tick_prefix: i64,
    tick_rate: f64,
) -> Result<Vec<SongTempo>> {
    let mut tempos: Vec<SongTempo> = Vec::new();
    for event in &master_track.events {
        if let MidiEventKind::Meta(MetaEvent::SetTempo(tempo)) = &event.kind {
            let bpm = (tempo2bpm(*tempo as i64) * 1000.0).round_ties_even() / 1000.0;
            let tick = py_round(event.time as f64 * tick_rate);
            if tick == 0 {
                tempos = vec![SongTempo::new(0, bpm)];
            } else {
                let last_tempo = tempos
                    .last()
                    .map(|tempo| tempo.bpm)
                    .ok_or_else(|| LibreSvipError::msg("tempo list is empty"))?;
                if bpm != last_tempo {
                    tempos.push(SongTempo::new(tick - tick_prefix, bpm));
                }
            }
        }
    }
    if tempos.is_empty() {
        tempos.push(SongTempo::default());
    }
    Ok(tempos)
}

impl VsqParser<'_> {
    fn parse_track(&self, text: &str, track_index: usize, tick_prefix: i64) -> Result<Track> {
        let ini = read_ini_with_fallback(text)?;
        let mut singer_name = String::new();
        if let Some(icon_handle) = ini.section_names().find_map(|section| {
            if section.starts_with("ID#") && ini.get(section, "type") == Some("Singer") {
                Some(ini.get(section, "iconhandle").map(str::to_owned))
            } else {
                None
            }
        }) {
            if let Some(icon_handle) = icon_handle {
                singer_name = ini
                    .get(&icon_handle, "ids")
                    .unwrap_or("")
                    .to_owned();
            }
        }
        let mut singing_track = SingingTrack::new(
            ini.get("Common", "Name")
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Track {track_index}")),
        );
        singing_track.ai_singer_name = singer_name;
        singing_track.note_list = if ini.has_section("EventList") {
            self.parse_notes(&ini, tick_prefix)?
        } else {
            Vec::new()
        };
        if self.options.import_pitch {
            if let Some(pitch) = self.parse_pitch(&ini, &singing_track.note_list, tick_prefix)? {
                singing_track.edited_params.pitch = pitch;
            }
        }
        self.parse_params(&ini, &mut singing_track.edited_params, tick_prefix);
        Ok(Track::Singing(singing_track))
    }

    fn extract_curve(&self, ini: &Ini, param_name: &str, tick_prefix: i64) -> Result<Option<ControllerCurve>> {
        let sections: &[&str] = match param_name {
            "pitch_bend" => &["PitchBendBPList"],
            "pitch_bend_sens" => &["PitchBendSensBPList"],
            "dynamics" => &["DynamicsBPList"],
            "gender" => &["GenderFactorBPList"],
            "breathiness" => &["BreathinessBPList", "EpRResidualBPList"],
            "brightness" => &["BrightnessBPList", "EpRESlopeBPList"],
            _ => &[],
        };
        let Some(section) = sections.iter().find(|section| ini.has_section(section)) else {
            return Ok(None);
        };
        let param_def = crate::vsqx::vocaloid::get_param_def(param_name);
        let mut events = Vec::new();
        for (key, value) in ini.items(section) {
            let Ok(pos) = key.trim().parse::<i64>() else {
                continue;
            };
            let Ok(value) = value.trim().parse::<i64>() else {
                continue;
            };
            events.push(ControllerEvent {
                pos: pos - tick_prefix,
                value,
            });
        }
        if events.is_empty() {
            return Ok(None);
        }
        Ok(Some(ControllerCurve::with_events(
            param_name,
            events,
            param_def.map(|definition| definition.default_value).unwrap_or(0),
            param_def.map(|definition| definition.min_value).unwrap_or(-127),
            param_def.map(|definition| definition.max_value).unwrap_or(127),
        )))
    }

    fn parse_pitch(
        &self,
        ini: &Ini,
        note_list: &[Note],
        tick_prefix: i64,
    ) -> Result<Option<ParamCurve>> {
        let pit_curve = self.extract_curve(ini, "pitch_bend", tick_prefix)?;
        let pbs_curve = self.extract_curve(ini, "pitch_bend_sens", tick_prefix)?;
        let Some(pit_curve) = pit_curve else {
            return Ok(None);
        };
        let pbs_curve = pbs_curve.unwrap_or_else(|| {
            ControllerCurve::with_events("pitch_bend_sens", Vec::new(), 2, 1, 24)
        });
        let pitch_data = PitchBendData {
            pit: pit_curve,
            pbs: pbs_curve,
        };
        let handler = VocaloidPitchHandler::new(
            self.synchronizer.clone(),
            note_list,
            &self.time_signatures,
            self.first_bar_length,
        );
        handler.to_absolute_pitch(&[pitch_data], None, None, None, None, None)
    }

    fn parse_params(&self, ini: &Ini, params: &mut Params, tick_prefix: i64) {
        if self.options.import_volume {
            if let Ok(Some(curve)) = self.extract_curve(ini, "dynamics", tick_prefix) {
                params.volume.points.extend(
                    crate::vsqx::vocaloid::convert_vocaloid_curve_to_param_points(
                        &curve,
                        self.first_bar_length,
                        false,
                    ),
                );
            }
        }
        if self.options.import_breath {
            if let Ok(Some(curve)) = self.extract_curve(ini, "breathiness", tick_prefix) {
                params.breath.points.extend(
                    crate::vsqx::vocaloid::convert_vocaloid_curve_to_param_points(
                        &curve,
                        self.first_bar_length,
                        false,
                    ),
                );
            }
        }
        if self.options.import_gender {
            if let Ok(Some(curve)) = self.extract_curve(ini, "gender", tick_prefix) {
                params.gender.points.extend(
                    crate::vsqx::vocaloid::convert_vocaloid_curve_to_param_points(
                        &curve,
                        self.first_bar_length,
                        true,
                    ),
                );
            }
        }
        if self.options.import_strength {
            if let Ok(Some(curve)) = self.extract_curve(ini, "brightness", tick_prefix) {
                params.strength.points.extend(
                    crate::vsqx::vocaloid::convert_vocaloid_curve_to_param_points(
                        &curve,
                        self.first_bar_length,
                        false,
                    ),
                );
            }
        }
    }

    fn parse_notes(&self, ini: &Ini, tick_prefix: i64) -> Result<Vec<Note>> {
        let breath_pattern = regex::Regex::new(&format!(r"\A{BREATH_PATTERN}\z"))
            .map_err(LibreSvipError::Regex)?;
        let mut notes = Vec::new();
        for (tick_str, event_key) in ini.items("EventList") {
            let tick = tick_str.trim().parse::<i64>().map_err(|_| {
                LibreSvipError::Parse(format!("invalid EventList tick: {tick_str:?}"))
            })? - tick_prefix;
            if !event_key.starts_with("ID#") {
                continue;
            }
            let note_section = event_key;
            let note_type = ini.get(note_section, "type").ok_or_else(|| {
                LibreSvipError::Parse(format!("missing section [{note_section}]"))
            })?;
            if note_type != "Anote" {
                continue;
            }
            let length = get_int(ini, note_section, "length")?;
            if length == 0 {
                continue;
            }
            let key = get_int(ini, note_section, "note#")?;
            if key == 0 {
                continue;
            }
            let lyric_handle = ini
                .get(note_section, "lyrichandle")
                .unwrap_or("")
                .to_owned();
            let l0 = ini.get(&lyric_handle, "L0").unwrap_or(",");
            let mut parts = l0.splitn(3, ',');
            let lyric_part = parts.next().unwrap_or("");
            let phoneme_part = parts.next().ok_or_else(|| {
                LibreSvipError::msg(format!(
                    "not enough values to unpack (expected 2) in [{lyric_handle}] L0"
                ))
            })?;
            let mut lyric_value = lyric_part.trim_matches('"').to_owned();
            let phoneme_value = phoneme_part.trim_matches('"');
            if breath_pattern.is_match(phoneme_value) {
                if self.options.breath == BreathOption::Ignore {
                    continue;
                }
                lyric_value = phoneme_part.trim_matches('"').to_owned();
            }
            notes.push(Note {
                start_pos: tick,
                length,
                key_number: key,
                lyric: lyric_value,
                pronunciation: None,
                ..Default::default()
            });
        }
        Ok(notes)
    }
}

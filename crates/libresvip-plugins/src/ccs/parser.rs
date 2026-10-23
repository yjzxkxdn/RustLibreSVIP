//! Port of `libresvip.plugins.ccs.cevio_parser`.

use std::collections::HashMap;

use libresvip_core::core::tick_counter::{skip_beat_list, skip_tempo_list};
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{
    InstrumentalTrack, Note, Project, SingingTrack, SongTempo, TimeSignature, Track,
};
use libresvip_core::options::Options;
use roxmltree::Node;

use super::techno_speech::{pitch_from_techno_speech_track, ParamEvent};

pub const TICK_RATE: f64 = 2.0;
pub const OCTAVE_OFFSET: i64 = -1;

fn child<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Option<Node<'a, 'input>> {
    node.children()
        .find(|candidate| candidate.is_element() && candidate.tag_name().name() == name)
}

fn children<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Vec<Node<'a, 'input>> {
    node.children()
        .filter(|candidate| candidate.is_element() && candidate.tag_name().name() == name)
        .collect()
}

fn attr<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attribute(name)
}

fn attr_i64(node: Node<'_, '_>, name: &str) -> Option<i64> {
    attr(node, name).and_then(|value| value.trim().parse().ok())
}

fn attr_f64(node: Node<'_, '_>, name: &str) -> Option<f64> {
    attr(node, name).and_then(|value| value.trim().parse().ok())
}

fn attr_bool(node: Node<'_, '_>, name: &str) -> bool {
    matches!(
        attr(node, name).map(|value| value.trim().to_ascii_lowercase()),
        Some(value) if value == "true" || value == "1"
    )
}

/// `// TICK_RATE` for a CeVIO clock value (Python floor-divides by 2.0).
fn clock_ticks(value: i64) -> f64 {
    (value as f64 / TICK_RATE).floor()
}

/// `XmlTime.duration` for a `HH:MM:SS[.fff]` attribute.
fn xml_time_duration(value: &str) -> Option<f64> {
    let mut parts = value.trim().split(':');
    let hour: f64 = parts.next()?.parse().ok()?;
    let minute: f64 = parts.next()?.parse().ok()?;
    let seconds: f64 = parts.next()?.parse().ok()?;
    Some(hour * 3600.0 + minute * 60.0 + seconds)
}

pub fn parse_project(text: &str, options: &Options) -> Result<Project> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let document = roxmltree::Document::parse(text)
        .map_err(|error| LibreSvipError::Parse(format!("invalid XML: {error}")))?;
    let root = document.root_element();
    if root.tag_name().name() != "Scenario" {
        return Err(LibreSvipError::InvalidFileType);
    }
    let mut singer_id2name: HashMap<Option<String>, String> = HashMap::new();
    if let Some(generation) = child(root, "Generation") {
        if let Some(svss) = child(generation, "SVSS") {
            if let Some(sound_sources) = child(svss, "SoundSources") {
                for source in children(sound_sources, "SoundSource") {
                    if let Some(name) = attr(source, "Name") {
                        singer_id2name
                            .insert(attr(source, "Id").map(str::to_owned), name.to_owned());
                    }
                }
            }
        }
    }
    let scene = child(root, "Sequence")
        .and_then(|sequence| child(sequence, "Scene"))
        .ok_or_else(|| LibreSvipError::Parse("missing Scene".to_owned()))?;
    let units: Vec<Node> = child(scene, "Units")
        .map(|units| children(units, "Unit"))
        .unwrap_or_default();
    let mut id2group: HashMap<Option<String>, Node> = HashMap::new();
    if let Some(groups) = child(scene, "Groups") {
        for group in children(groups, "Group") {
            id2group.insert(attr(group, "Id").map(str::to_owned), group);
        }
    }
    let singing_units: Vec<Node> = units
        .iter()
        .copied()
        .filter(|unit| attr(*unit, "Category") == Some("SingerSong"))
        .collect();
    let audio_units: Vec<Node> = units
        .iter()
        .copied()
        .filter(|unit| attr(*unit, "Category") == Some("OuterAudio"))
        .collect();

    let mut tracks: Vec<Track> = Vec::new();
    let mut tempos: Vec<SongTempo> = Vec::new();
    let mut time_signatures: Vec<TimeSignature> = Vec::new();
    for (index, unit) in singing_units.iter().enumerate() {
        let group = attr(*unit, "Group")
            .and_then(|group_id| id2group.get(&Some(group_id.to_owned())).copied());
        let track_name = group.and_then(|group| attr(group, "Name"));
        let (track, tempo_part, time_signature_part) = parse_singing_track(
            index,
            *unit,
            group,
            track_name,
            options,
            &singer_id2name,
        )?;
        tracks.push(Track::Singing(track));
        tempos.extend(tempo_part);
        time_signatures.extend(time_signature_part);
    }
    let tempos = merge_tempos(tempos);
    let time_signatures = merge_time_signatures(time_signatures);
    let time_synchronizer = TimeSynchronizer::from_project(&tempos);

    if options.bool("import_instrumental_track", true) {
        for (index, unit) in audio_units.iter().enumerate() {
            let group = attr(*unit, "Group")
                .and_then(|group_id| id2group.get(&Some(group_id.to_owned())).copied());
            let track_name = group.and_then(|group| attr(group, "Name"));
            let title = track_name
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Track {}", index + 1));
            let offset = match attr(*unit, "StartTime").and_then(xml_time_duration) {
                Some(duration) => {
                    time_synchronizer.get_actual_ticks_from_secs(duration) as i64
                }
                None => 0,
            };
            tracks.push(Track::Instrumental(InstrumentalTrack {
                title,
                audio_file_path: attr(*unit, "FilePath").unwrap_or_default().to_owned(),
                mute: group.map(|group| attr_bool(group, "IsMuted")).unwrap_or(false),
                solo: group.map(|group| attr_bool(group, "IsSolo")).unwrap_or(false),
                offset,
                ..Default::default()
            }));
        }
    }
    Ok(Project {
        track_list: tracks,
        time_signature_list: skip_beat_list(&time_signatures, 0),
        song_tempo_list: skip_tempo_list(&tempos, 0),
        ..Default::default()
    })
}

fn merge_tempos(tempos: Vec<SongTempo>) -> Vec<SongTempo> {
    let mut order: Vec<i64> = Vec::new();
    let mut map: HashMap<i64, SongTempo> = HashMap::new();
    for tempo in tempos {
        if !map.contains_key(&tempo.position) {
            order.push(tempo.position);
            map.insert(tempo.position, tempo.clone());
        }
    }
    if order.is_empty() {
        return vec![SongTempo::default()];
    }
    order
        .into_iter()
        .filter_map(|position| map.get(&position).cloned())
        .collect()
}

fn merge_time_signatures(time_signatures: Vec<TimeSignature>) -> Vec<TimeSignature> {
    let mut order: Vec<i64> = Vec::new();
    let mut map: HashMap<i64, TimeSignature> = HashMap::new();
    for time_signature in time_signatures {
        if !map.contains_key(&time_signature.bar_index) {
            order.push(time_signature.bar_index);
            map.insert(time_signature.bar_index, time_signature.clone());
        }
    }
    if order.is_empty() {
        return vec![TimeSignature::default()];
    }
    order
        .into_iter()
        .filter_map(|bar_index| map.get(&bar_index).cloned())
        .collect()
}

fn parse_singing_track(
    index: usize,
    unit: Node,
    group: Option<Node>,
    track_name: Option<&str>,
    options: &Options,
    singer_id2name: &HashMap<Option<String>, String>,
) -> Result<(SingingTrack, Vec<SongTempo>, Vec<TimeSignature>)> {
    let time_nodes: Vec<Node> = child(unit, "Song")
        .and_then(|song| child(song, "Beat"))
        .map(|beat| children(beat, "Time"))
        .unwrap_or_default();
    let mut prev_tick = 0.0f64;
    let mut time_signatures = vec![TimeSignature {
        bar_index: 0,
        numerator: 4,
        denominator: 4,
    }];
    for time_node in time_nodes {
        let tick = clock_ticks(attr_i64(time_node, "Clock").unwrap_or(0));
        let numerator = attr_i64(time_node, "Beats");
        let denominator = attr_i64(time_node, "BeatType");
        if let (Some(numerator), Some(denominator)) = (numerator, denominator) {
            let ticks_in_measure = time_signatures
                .last()
                .map(TimeSignature::bar_length)
                .unwrap_or(1920.0);
            let tick_diff = tick - prev_tick;
            let measure_diff = tick_diff / ticks_in_measure;
            let last = time_signatures.last().cloned().unwrap_or_default();
            time_signatures.push(TimeSignature {
                bar_index: (last.bar_index as f64 + measure_diff) as i64,
                numerator,
                denominator,
            });
            prev_tick = tick;
        }
    }
    for time_signature in &mut time_signatures {
        time_signature.bar_index -= 4;
    }
    let tick_prefix = time_signatures
        .first()
        .map(|signature| signature.bar_length() as i64)
        .unwrap_or(0);

    let tempo_nodes: Vec<Node> = child(unit, "Song")
        .and_then(|song| child(song, "Tempo"))
        .map(|tempo| children(tempo, "Sound"))
        .unwrap_or_default();
    let mut tempos = Vec::new();
    for tempo_node in tempo_nodes {
        if let (Some(clock), Some(tempo)) = (
            attr_i64(tempo_node, "Clock"),
            attr_f64(tempo_node, "Tempo"),
        ) {
            tempos.push(SongTempo {
                position: clock_ticks(clock) as i64,
                bpm: tempo,
            });
        }
    }

    let mut notes = Vec::new();
    let note_nodes: Vec<Node> = child(unit, "Song")
        .and_then(|song| child(song, "Score"))
        .map(|score| children(score, "Note"))
        .unwrap_or_default();
    for note_node in note_nodes {
        let tick_on = attr_i64(note_node, "Clock").map(clock_ticks);
        let duration = attr_i64(note_node, "Duration").map(clock_ticks);
        let pitch_step = attr_i64(note_node, "PitchStep");
        let pitch_octave = attr_i64(note_node, "PitchOctave").map(|octave| octave - OCTAVE_OFFSET);
        let key = match (pitch_step, pitch_octave) {
            (Some(step), Some(octave)) => Some(step + octave * 12),
            _ => None,
        };
        let lyric = attr(note_node, "Lyric").and_then(|lyric| {
            if lyric.is_empty() {
                None
            } else {
                Some(lyric)
            }
        });
        if let (Some(tick_on), Some(duration), Some(key), Some(lyric)) =
            (tick_on, duration, key, lyric)
        {
            let lyric = if lyric == "\u{30FC}" {
                "-".to_owned()
            } else {
                lyric.to_owned()
            };
            let phoneme = attr(note_node, "Phonetic").map(|phonetic| phonetic.replace(',', " "));
            notes.push(Note {
                key_number: key,
                lyric,
                start_pos: tick_on as i64,
                length: duration as i64,
                pronunciation: phoneme,
                ..Default::default()
            });
        }
    }

    let parameter = child(unit, "Song").and_then(|song| child(song, "Parameter"));
    let mut pitch = None;
    if options.bool("import_pitch", true) {
        if let Some(log_f0) = parameter.and_then(|parameter| child(parameter, "LogF0")) {
            let pitch_data: Vec<ParamEvent> = children(log_f0, "Data")
                .into_iter()
                .filter_map(parse_param_data)
                .collect();
            let vibrato_amplitude: Vec<ParamEvent> = parameter
                .and_then(|parameter| child(parameter, "VibAmp"))
                .map(|node| {
                    children(node, "Data")
                        .into_iter()
                        .filter_map(parse_param_data)
                        .collect()
                })
                .unwrap_or_default();
            let vibrato_frequency: Vec<ParamEvent> = parameter
                .and_then(|parameter| child(parameter, "VibFrq"))
                .map(|node| {
                    children(node, "Data")
                        .into_iter()
                        .filter_map(parse_param_data)
                        .collect()
                })
                .unwrap_or_default();
            pitch = pitch_from_techno_speech_track(
                &pitch_data,
                &tempos,
                tick_prefix,
                &vibrato_amplitude,
                &vibrato_frequency,
            );
        }
    }

    let title = track_name
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("Track {}", index + 1));
    let cast_id = attr(unit, "CastId").map(str::to_owned);
    let mut track = SingingTrack {
        ai_singer_name: singer_id2name.get(&cast_id).cloned().unwrap_or_default(),
        title,
        note_list: notes,
        mute: group.map(|group| attr_bool(group, "IsMuted")).unwrap_or(false),
        solo: group.map(|group| attr_bool(group, "IsSolo")).unwrap_or(false),
        ..Default::default()
    };
    if let Some(pitch) = pitch {
        track.edited_params.pitch = pitch;
    }
    Ok((track, tempos, time_signatures))
}

fn parse_param_data(node: Node) -> Option<ParamEvent> {
    let value: f64 = node.text()?.trim().parse().ok()?;
    let index = attr_i64(node, "Index").filter(|index| *index != 0);
    let repeat = attr_i64(node, "Repeat").filter(|repeat| *repeat != 0);
    Some(ParamEvent {
        idx: index,
        repeat,
        value,
    })
}

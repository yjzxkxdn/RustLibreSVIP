//! Port of `libresvip.plugins.musicxml.musicxml_parser`.
//!
//! The Python parser works on two levels: the xsdata `ScorePartwise` model for
//! the note stream and the raw ElementTree tree for tempo/dynamics placement.
//! Since the intermediate model only carries the handful of fields the parser
//! reads, this port works directly on the XML tree while reproducing both
//! walks (including their cursor and overlap quirks).

use std::collections::HashMap;

use libresvip_core::core::constants::DEFAULT_PHONEME;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{
    Note, ParamCurve, Params, Project, SingingTrack, SongTempo, TimeSignature, Track, TICKS_IN_BEAT,
};
use libresvip_core::model::point::Point;
use roxmltree::Node;

use super::music::{note2midi, parse_f64};
use super::options::InputOptions;

// ---------------------------------------------------------------------------
// XML helpers
// ---------------------------------------------------------------------------

fn children_named<'a, 'input>(
    node: Node<'a, 'input>,
    name: &str,
) -> Vec<Node<'a, 'input>> {
    node.children()
        .filter(|child| child.is_element() && child.tag_name().name() == name)
        .collect()
}

fn child<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Option<Node<'a, 'input>> {
    node.children()
        .find(|child| child.is_element() && child.tag_name().name() == name)
}

fn text_of(node: Node<'_, '_>) -> String {
    node.text().unwrap_or("").to_owned()
}

fn int_text(node: Node<'_, '_>) -> Option<i64> {
    node.text()
        .map(str::trim)
        .and_then(|text| text.parse::<i64>().ok())
}

fn float_text(node: Node<'_, '_>) -> Option<f64> {
    node.text().and_then(parse_f64)
}

fn attr_int(node: Node<'_, '_>, name: &str) -> Option<i64> {
    node.attribute(name).and_then(|text| text.trim().parse().ok())
}

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

pub fn parse_project(bytes: &[u8], path_display: &str, options: &InputOptions) -> Result<Project> {
    let xml_bytes = if bytes.starts_with(b"PK") {
        unpack_mxl(bytes)?
    } else {
        bytes.to_vec()
    };
    let head = xml_bytes
        .iter()
        .take(4096)
        .copied()
        .skip_while(|byte| matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c))
        .collect::<Vec<u8>>();
    if !contains_bytes(&head, b"<score-partwise") && !contains_bytes(&head, b"<score-timewise") {
        return Err(LibreSvipError::Message(format!(
            "{path_display}: not a MusicXML score (root must be score-partwise or score-timewise)"
        )));
    }
    let text = decode_xml(&xml_bytes);
    let parse_options = roxmltree::ParsingOptions {
        allow_dtd: true,
        nodes_limit: u32::MAX,
    };
    let document = roxmltree::Document::parse_with_options(&text, parse_options)
        .map_err(|error| LibreSvipError::Parse(format!("{path_display}: {error}")))?;
    let root = document.root_element();

    let parts: Vec<Node<'_, '_>> = children_named(root, "part");
    let master = parts
        .iter()
        .find(|part| !children_named(**part, "measure").is_empty())
        .copied()
        .ok_or_else(|| LibreSvipError::Message("no part with measures".to_owned()))?;

    let mut parser = MusicXmlParser::new(options, root);
    let (time_signatures, mut tempos, import_tick_rate, divisions) = parser.parse_master_track(master);

    let mut tracks: Vec<SingingTrack> = Vec::new();
    let mut track_fermatas: Vec<Vec<FermataInfo>> = Vec::new();
    let part_list = child(root, "part-list");
    for (index, part) in parts.iter().enumerate() {
        let score_part = part
            .attribute("id")
            .and_then(|id| score_part_by_id(part_list, id));
        let title = match score_part {
            Some(score_part) => child(score_part, "part-name")
                .map(text_of)
                .unwrap_or_default(),
            None => format!("Track {}", index + 1),
        };
        let (mut track, fermatas) =
            parser.parse_track(*part, title, import_tick_rate)?;
        if options.import_dynamics {
            if let Some(part_element) = find_part_element(root, part.attribute("id")) {
                parser.populate_volume(&mut track, part_element, divisions);
            }
        }
        tracks.push(track);
        track_fermatas.push(fermatas);
    }

    if options.apply_fermata_stretch {
        for (index, fermatas) in track_fermatas.iter().enumerate() {
            if fermatas.is_empty() {
                continue;
            }
            apply_fermata_stretch(&mut tracks[index], fermatas, index == 0, &mut tempos);
        }
    }

    let mut project = Project {
        version: String::new(),
        song_tempo_list: tempos,
        time_signature_list: time_signatures,
        track_list: tracks.into_iter().map(Track::Singing).collect(),
    };
    if project.time_signature_list.is_empty() {
        project.time_signature_list.push(TimeSignature::default());
    }
    if project.song_tempo_list.is_empty() {
        project.song_tempo_list.push(SongTempo::default());
    }
    Ok(project)
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// `int(Decimal(text) * Decimal(TICKS_IN_BEAT) / Decimal(divisions))` with
/// truncation toward zero. Exact for ordinary durations; falls back to `f64`
/// when the decimal does not fit in an `i128`.
fn decimal_ticks(text: Option<&str>, divisions: i64) -> i64 {
    let Some(text) = text else {
        return 0;
    };
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0;
    }
    let (negative, rest) = match trimmed.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, trimmed.strip_prefix('+').unwrap_or(trimmed)),
    };
    // Split into mantissa digits and a base-10 exponent.
    let (mantissa_text, exponent_text) = match rest.find(['e', 'E']) {
        Some(index) => (&rest[..index], Some(&rest[index + 1..])),
        None => (rest, None),
    };
    let exponent: i64 = exponent_text
        .and_then(|text| text.parse::<i64>().ok())
        .unwrap_or(0);
    let (integer, fraction) = match mantissa_text.find('.') {
        Some(index) => (&mantissa_text[..index], &mantissa_text[index + 1..]),
        None => (mantissa_text, ""),
    };
    let digits_text = format!("{integer}{fraction}");
    if digits_text.is_empty() || !digits_text.chars().all(|character| character.is_ascii_digit()) {
        return 0;
    }
    let scale = exponent.saturating_sub(fraction.len() as i64);
    if scale < -400 {
        // Vanishingly small values truncate to zero.
        return 0;
    }
    if scale > 400 {
        return fallback_decimal_ticks(trimmed, divisions);
    }
    let Ok(mut digits) = digits_text.parse::<i128>() else {
        return fallback_decimal_ticks(trimmed, divisions);
    };
    let mut denominator: i128 = i128::from(divisions.max(1));
    if scale >= 0 {
        for _ in 0..scale {
            let Some(value) = digits.checked_mul(10) else {
                return fallback_decimal_ticks(trimmed, divisions);
            };
            digits = value;
        }
    } else {
        for _ in 0..(-scale) {
            let Some(value) = denominator.checked_mul(10) else {
                return fallback_decimal_ticks(trimmed, divisions);
            };
            denominator = value;
        }
    }
    let Some(numerator) = digits.checked_mul(i128::from(TICKS_IN_BEAT)) else {
        return fallback_decimal_ticks(trimmed, divisions);
    };
    let result = numerator / denominator;
    let result = i64::try_from(result).unwrap_or(if result < 0 { i64::MIN } else { i64::MAX });
    if negative {
        -result
    } else {
        result
    }
}

fn fallback_decimal_ticks(text: &str, divisions: i64) -> i64 {
    let value = text.parse::<f64>().unwrap_or(0.0);
    (value * TICKS_IN_BEAT as f64 / divisions.max(1) as f64) as i64
}

fn decode_xml(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let (decoded, _, _) = encoding_rs::UTF_16LE.decode(bytes);
        return decoded.into_owned();
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let (decoded, _, _) = encoding_rs::UTF_16BE.decode(bytes);
        return decoded.into_owned();
    }
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    // Honour a BOM-less XML declaration (`<?xml ... encoding="..."?>`), like
    // ElementTree does.
    if let Some(encoding) = declared_encoding(bytes) {
        if !encoding.eq_ignore_ascii_case("utf-8") {
            if let Some(encoding) = encoding_rs::Encoding::for_label(encoding.as_bytes()) {
                let (decoded, _, _) = encoding.decode(bytes);
                return decoded.into_owned();
            }
        }
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => String::from_utf8_lossy(bytes).into_owned(),
    }
}

fn declared_encoding(bytes: &[u8]) -> Option<String> {
    let prefix = bytes.get(..256)?;
    let text = String::from_utf8_lossy(prefix);
    let declaration = text.strip_prefix("<?xml")?;
    let end = declaration.find("?>")?;
    let declaration = &declaration[..end];
    let index = declaration.find("encoding")?;
    let rest = declaration[index + "encoding".len()..].trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &rest[1..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_owned())
}

fn unpack_mxl(bytes: &[u8]) -> Result<Vec<u8>> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|error| LibreSvipError::Parse(format!("invalid .mxl archive: {error}")))?;
    let container_bytes = read_zip_entry(&mut archive, "META-INF/container.xml")?;
    let container_text = decode_xml(&container_bytes);
    let document = roxmltree::Document::parse(&container_text)
        .map_err(|error| LibreSvipError::Parse(format!("invalid container.xml: {error}")))?;
    let full_path = first_rootfile_path(document.root_element())
        .ok_or_else(|| LibreSvipError::Parse("container.xml has no rootfile".to_owned()))?;
    read_zip_entry(&mut archive, &full_path)
}

fn read_zip_entry<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
) -> Result<Vec<u8>> {
    let mut entry = archive
        .by_name(name)
        .map_err(|error| LibreSvipError::Parse(format!("missing zip entry {name}: {error}")))?;
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut entry, &mut bytes)
        .map_err(|error| LibreSvipError::Parse(format!("cannot read zip entry {name}: {error}")))?;
    Ok(bytes)
}

fn first_rootfile_path(node: Node<'_, '_>) -> Option<String> {
    if node.is_element() && node.tag_name().name() == "rootfile" {
        if let Some(path) = node.attribute("full-path") {
            return Some(path.to_owned());
        }
    }
    node.children()
        .filter(|child| child.is_element())
        .find_map(first_rootfile_path)
}

fn find_part_element<'a, 'input>(
    root: Node<'a, 'input>,
    part_id: Option<&str>,
) -> Option<Node<'a, 'input>> {
    root.descendants().find(|node| {
        node.is_element()
            && node.tag_name().name() == "part"
            && part_id.map_or(true, |id| node.attribute("id") == Some(id))
    })
}

fn score_part_by_id<'a, 'input>(
    part_list: Option<Node<'a, 'input>>,
    id: &str,
) -> Option<Node<'a, 'input>> {
    let part_list = part_list?;
    children_named(part_list, "score-part")
        .into_iter()
        .find(|score_part| score_part.attribute("id") == Some(id))
}

// ---------------------------------------------------------------------------
// Raw event walk (tempo and dynamic placement)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum EventKind {
    Note,
    Direction,
    Sound,
}

fn walk_part_events<'a, 'input>(
    part: Node<'a, 'input>,
    divisions: i64,
    mut visitor: impl FnMut(EventKind, i64, Node<'a, 'input>),
) {
    let mut measure_start = 0i64;
    let mut current_time_signature = TimeSignature::default();
    let ticks = |text: Option<&str>| -> i64 { decimal_ticks(text, divisions) };
    for measure in children_named(part, "measure") {
        let mut cursor = 0i64;
        let mut prev_note_start = 0i64;
        if let Some(attributes) = child(measure, "attributes") {
            if let Some(time) = child(attributes, "time") {
                if let (Some(beats), Some(beat_type)) =
                    (child(time, "beats"), child(time, "beat-type"))
                {
                    if beats.text().is_some() && beat_type.text().is_some() {
                        if let (Some(numerator), Some(denominator)) =
                            (int_text(beats), int_text(beat_type))
                        {
                            current_time_signature =
                                TimeSignature::new(0, numerator, denominator);
                        }
                    }
                }
            }
        }
        for element in measure.children().filter(|node| node.is_element()) {
            match element.tag_name().name() {
                "note" => {
                    let is_chord = child(element, "chord").is_some();
                    let is_grace = child(element, "grace").is_some();
                    let note_start = if is_chord { prev_note_start } else { cursor };
                    visitor(EventKind::Note, measure_start + note_start, element);
                    if !is_chord && !is_grace {
                        prev_note_start = note_start;
                        cursor += ticks(child(element, "duration").and_then(|node| node.text()));
                    }
                }
                "backup" => {
                    cursor -= ticks(child(element, "duration").and_then(|node| node.text()));
                    prev_note_start = cursor;
                }
                "forward" => {
                    cursor += ticks(child(element, "duration").and_then(|node| node.text()));
                    prev_note_start = cursor;
                }
                "direction" => {
                    let offset = ticks(child(element, "offset").and_then(|node| node.text()));
                    visitor(EventKind::Direction, measure_start + cursor + offset, element);
                }
                "sound" => {
                    visitor(EventKind::Sound, measure_start + cursor, element);
                }
                _ => {}
            }
        }
        measure_start += current_time_signature.bar_length().round_ties_even() as i64;
    }
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct FermataInfo {
    note_index: usize,
    shape: String,
}

pub struct MusicXmlParser<'a> {
    options: &'a InputOptions,
    root: Node<'a, 'a>,
}

impl<'a> MusicXmlParser<'a> {
    pub fn new(options: &'a InputOptions, root: Node<'a, 'a>) -> Self {
        Self { options, root }
    }

    fn parse_master_track(
        &mut self,
        part_node: Node<'a, 'a>,
    ) -> (Vec<TimeSignature>, Vec<SongTempo>, f64, i64) {
        let measure_nodes = children_named(part_node, "measure");
        let Some(first_measure) = measure_nodes.first() else {
            return (
                Vec::new(),
                Vec::new(),
                0.0,
                0,
            );
        };
        let first_attributes = child(*first_measure, "attributes");
        let divisions = first_attributes
            .and_then(|attributes| child(attributes, "divisions"))
            .and_then(int_text)
            .unwrap_or(0);
        let import_tick_rate = TICKS_IN_BEAT as f64 / divisions as f64;

        let mut time_signatures: Vec<TimeSignature> = Vec::new();
        for (index, measure) in measure_nodes.iter().enumerate() {
            let Some(attributes) = child(*measure, "attributes") else {
                continue;
            };
            let times = children_named(attributes, "time");
            let Some(time_node) = times.first() else {
                continue;
            };
            let bar_index = attr_int(*time_node, "number").unwrap_or(index as i64);
            let numerator = children_named(*time_node, "beats")
                .first()
                .and_then(|node| int_text(*node));
            let denominator = children_named(*time_node, "beat-type")
                .first()
                .and_then(|node| int_text(*node));
            if let (Some(numerator), Some(denominator)) = (numerator, denominator) {
                time_signatures.push(TimeSignature::new(bar_index, numerator, denominator));
            }
        }

        let mut tempos: Vec<SongTempo> = Vec::new();
        if self.options.import_tempo {
            if let Some(part_element) = find_part_element(self.root, part_node.attribute("id")) {
                walk_part_events(part_element, divisions, |kind, tick, node| match kind {
                    EventKind::Direction => {
                        if let Some(sound) = child(node, "sound") {
                            if let Some(tempo) = sound.attribute("tempo") {
                                if let Ok(bpm) = tempo.trim().parse::<f64>() {
                                    tempos.push(SongTempo::new(tick, bpm));
                                }
                            }
                        }
                    }
                    EventKind::Sound => {
                        if let Some(tempo) = node.attribute("tempo") {
                            if let Ok(bpm) = tempo.trim().parse::<f64>() {
                                tempos.push(SongTempo::new(tick, bpm));
                            }
                        }
                    }
                    EventKind::Note => {}
                });
            } else {
                // Legacy fallback: typed walk, using the final time signature.
                let default_signature = TimeSignature::default();
                let last_signature = time_signatures.last().unwrap_or(&default_signature);
                let ticks_in_measure = last_signature.bar_length().round_ties_even() as i64;
                let mut tick_position = 0i64;
                for measure in &measure_nodes {
                    for sound in children_named(*measure, "sound") {
                        if let Some(tempo) = sound.attribute("tempo") {
                            if let Ok(bpm) = tempo.trim().parse::<f64>() {
                                tempos.push(SongTempo::new(tick_position, bpm));
                            }
                        }
                    }
                    tick_position += ticks_in_measure;
                }
            }
        }
        tempos.sort_by_key(|tempo| tempo.position);
        let mut deduped: Vec<SongTempo> = Vec::new();
        for tempo in tempos {
            match deduped.last_mut() {
                Some(last) if last.position == tempo.position => *last = tempo,
                _ => deduped.push(tempo),
            }
        }
        (time_signatures, deduped, import_tick_rate, divisions)
    }

    fn parse_track(
        &mut self,
        part_node: Node<'a, 'a>,
        title: String,
        import_tick_rate: f64,
    ) -> Result<(SingingTrack, Vec<FermataInfo>)> {
        let mut notes: Vec<Note> = Vec::new();
        let mut fermatas: Vec<FermataInfo> = Vec::new();
        let mut is_inside_note = false;
        let mut tick_position = 0i64;
        let mut previous_tick_position = 0i64;
        let mut incomplete_lyric_note_index: Option<usize> = None;
        let mut is_lyric_extension_active = false;

        for measure in children_named(part_node, "measure") {
            let content: Vec<Node<'a, 'a>> = measure
                .children()
                .filter(|node| node.is_element())
                .filter(|node| matches!(node.tag_name().name(), "note" | "backup" | "forward"))
                .collect();
            for note_node in content {
                match note_node.tag_name().name() {
                    "backup" => {
                        let duration = child(note_node, "duration")
                            .and_then(float_text)
                            .ok_or_else(|| {
                                LibreSvipError::Parse("Duration not found".to_owned())
                            })?;
                        tick_position -= (duration * import_tick_rate) as i64;
                        previous_tick_position = tick_position;
                        continue;
                    }
                    "forward" => {
                        let duration = child(note_node, "duration")
                            .and_then(float_text)
                            .ok_or_else(|| {
                                LibreSvipError::Parse("Duration not found".to_owned())
                            })?;
                        tick_position += (duration * import_tick_rate) as i64;
                        previous_tick_position = tick_position;
                        continue;
                    }
                    _ => {}
                }

                let duration = child(note_node, "duration")
                    .and_then(float_text)
                    .map(|value| (value * import_tick_rate) as i64);
                let Some(duration) = duration.filter(|duration| *duration != 0) else {
                    if child(note_node, "grace").is_some() {
                        continue;
                    }
                    return Err(LibreSvipError::Parse("Duration not found".to_owned()));
                };

                if child(note_node, "rest").is_some() {
                    is_lyric_extension_active = false;
                    tick_position += duration;
                    continue;
                }

                let pitch_node = child(note_node, "pitch").ok_or_else(|| {
                    LibreSvipError::Parse("Duration not found".to_owned())
                })?;
                let step_node = child(pitch_node, "step")
                    .ok_or_else(|| LibreSvipError::Parse("missing pitch step".to_owned()))?;
                let step = text_of(step_node)
                    .trim()
                    .chars()
                    .next()
                    .map(|character| character.to_ascii_uppercase())
                    .ok_or_else(|| LibreSvipError::Parse("missing pitch step".to_owned()))?;
                let octave = child(pitch_node, "octave")
                    .and_then(int_text)
                    .ok_or_else(|| LibreSvipError::Parse("missing pitch octave".to_owned()))?;
                let alter = child(pitch_node, "alter")
                    .and_then(float_text)
                    .map(|value| {
                        if value == 0.0 {
                            0i64
                        } else {
                            value.trunc() as i64
                        }
                    })
                    .unwrap_or(0);
                let key = note2midi(step, octave, alter)
                    .ok_or_else(|| LibreSvipError::Parse("Invalid note format".to_owned()))?;

                let lyric_nodes = children_named(note_node, "lyric");
                let lyric_node = lyric_nodes.first().copied();
                let texts = lyric_node
                    .map(|lyric| children_named(lyric, "text"))
                    .unwrap_or_default();
                let has_lyric_text = !texts.is_empty();
                if let Some(lyric_node) = lyric_node {
                    let extends = children_named(lyric_node, "extend");
                    if !extends.is_empty() {
                        for extend in extends {
                            is_lyric_extension_active =
                                extend.attribute("type") != Some("stop");
                        }
                    } else if has_lyric_text {
                        is_lyric_extension_active = false;
                    }
                }
                let lyric = if !has_lyric_text && is_lyric_extension_active {
                    "+~".to_owned()
                } else if has_slur_continue_or_stop(note_node) {
                    "-".to_owned()
                } else if has_lyric_text {
                    text_of(texts[0])
                } else {
                    DEFAULT_PHONEME.to_owned()
                };

                if child(note_node, "chord").is_some() {
                    tick_position = previous_tick_position;
                }

                if !is_inside_note {
                    let mut note = Note::new(tick_position, duration, key, lyric.clone());
                    let note_index = notes.len();
                    notes.push(note.clone());
                    if let Some(lyric_node) = lyric_node {
                        let syllabic = children_named(lyric_node, "syllabic")
                            .first()
                            .map(|node| text_of(*node).trim().to_owned())
                            .unwrap_or_else(|| "single".to_owned());
                        match syllabic.as_str() {
                            "begin" => incomplete_lyric_note_index = Some(note_index),
                            "end" => {
                                if let Some(incomplete) = incomplete_lyric_note_index {
                                    notes[incomplete].lyric.push_str(&lyric);
                                    incomplete_lyric_note_index = None;
                                    note.lyric = "+".to_owned();
                                    notes[note_index] = note.clone();
                                }
                            }
                            "middle" => {
                                if let Some(incomplete) = incomplete_lyric_note_index {
                                    notes[incomplete].lyric.push_str(&lyric);
                                    note.lyric = "+".to_owned();
                                    notes[note_index] = note.clone();
                                }
                            }
                            _ => {}
                        }
                    }
                    if let Some(shape) = first_fermata_shape(note_node)? {
                        fermatas.push(FermataInfo {
                            note_index,
                            shape,
                        });
                    }
                } else if let Some(last) = notes.last_mut() {
                    last.length += duration;
                }

                previous_tick_position = tick_position;
                tick_position += duration;

                if let Some(tie) = child(note_node, "tie") {
                    match tie.attribute("type") {
                        Some("start") => is_inside_note = true,
                        Some("stop") => is_inside_note = false,
                        _ => {}
                    }
                }
            }
        }

        notes.sort_by_key(|note| (note.start_pos, note.key_number));
        let mut track = SingingTrack::new(title);
        track.note_list = notes;
        Ok((track, fermatas))
    }

    fn populate_volume(&mut self, track: &mut SingingTrack, part_element: Node<'a, 'a>, divisions: i64) {
        let mut points: Vec<Point> = Vec::new();
        let mut open_wedges: HashMap<i64, (i64, String, i64)> = HashMap::new();
        let mut last_volume = 0i64;

        walk_part_events(part_element, divisions, |kind, tick, node| {
            if kind != EventKind::Direction {
                return;
            }
            for direction_type in children_named(node, "direction-type") {
                for dynamics in children_named(direction_type, "dynamics") {
                    for label in dynamics.children().filter(Node::is_element) {
                        let tag = label.tag_name().name();
                        if let Some(velocity) = dynamic_velocity(tag) {
                            last_volume = dyn_label_to_volume(velocity);
                            points.push(Point::new(tick, last_volume));
                            break;
                        }
                    }
                }
                for wedge in children_named(direction_type, "wedge") {
                    let number = wedge
                        .attribute("number")
                        .and_then(|value| value.parse::<i64>().ok())
                        .unwrap_or(1);
                    match wedge.attribute("type") {
                        Some("crescendo") | Some("diminuendo") => {
                            open_wedges.insert(
                                number,
                                (tick, wedge.attribute("type").unwrap_or("").to_owned(), last_volume),
                            );
                            points.push(Point::new(tick, last_volume));
                        }
                        Some("stop") => {
                            if open_wedges.remove(&number).is_some() {
                                if wedge.attribute("niente") == Some("yes") {
                                    let value = dyn_label_to_volume(
                                        dynamic_velocity("ppppp").unwrap_or(12),
                                    );
                                    points.push(Point::new(tick, value));
                                    last_volume = value;
                                } else {
                                    points.push(Point::new(tick, last_volume));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            if let Some(sound) = child(node, "sound") {
                if let Some(dynamics) = sound.attribute("dynamics") {
                    if let Ok(value) = dynamics.trim().parse::<f64>() {
                        let velocity = (value * 127.0 / 100.0).round_ties_even() as i64;
                        last_volume = velocity_to_volume(velocity);
                        points.push(Point::new(tick, last_volume));
                    }
                }
            }
        });

        if points.is_empty() {
            return;
        }
        points.sort_by_key(|point| point.x);
        let mut deduped: Vec<Point> = Vec::new();
        for point in points {
            match deduped.last_mut() {
                Some(last) if last.x == point.x => *last = point,
                _ => deduped.push(point),
            }
        }
        track.edited_params = Params {
            volume: ParamCurve::new(deduped),
            ..Default::default()
        };
    }
}

fn has_slur_continue_or_stop(note_node: Node<'_, '_>) -> bool {
    for notation in children_named(note_node, "notations") {
        for slur in children_named(notation, "slur") {
            if matches!(slur.attribute("type"), Some("continue") | Some("stop")) {
                return true;
            }
        }
    }
    false
}

fn first_fermata_shape(note_node: Node<'_, '_>) -> Result<Option<String>> {
    for notation in children_named(note_node, "notations") {
        for fermata in children_named(notation, "fermata") {
            let value = fermata.text().map(str::trim).unwrap_or("");
            let shape = if value.is_empty() { "normal" } else { value };
            if !is_fermata_shape(shape) {
                return Err(LibreSvipError::Validation(format!(
                    "invalid fermata shape: {shape}"
                )));
            }
            return Ok(Some(shape.to_owned()));
        }
    }
    Ok(None)
}

fn is_fermata_shape(value: &str) -> bool {
    matches!(
        value,
        "normal"
            | "angled"
            | "square"
            | "double-angled"
            | "double-square"
            | "double-dot"
            | "half-curve"
            | "curlew"
    )
}

// ---------------------------------------------------------------------------
// Dynamics / fermata constants
// ---------------------------------------------------------------------------

/// `DYNAMIC_TO_VELOCITY` (`plugins/musicxml/dynamics.py`).
fn dynamic_velocity(label: &str) -> Option<i64> {
    Some(match label {
        "ffffff" => 127,
        "fffff" => 126,
        "ffff" => 124,
        "fff" => 120,
        "ff" => 112,
        "f" => 96,
        "mf" => 80,
        "mp" => 64,
        "n" => 64,
        "p" => 49,
        "pp" => 36,
        "ppp" => 24,
        "pppp" => 16,
        "ppppp" => 12,
        "pppppp" => 8,
        "sf" => 112,
        "sfz" => 112,
        "sffz" => 120,
        "fz" => 112,
        "rf" => 96,
        "rfz" => 96,
        "fp" => 96,
        "pf" => 80,
        "sfp" => 112,
        "sfpp" => 112,
        "sfzp" => 112,
        _ => return None,
    })
}

/// `round(cc11_to_db_change(velocity))`.
fn velocity_to_volume(velocity: i64) -> i64 {
    let ratio = velocity as f64 / 127.0;
    let db = 20.0 * (ratio.powf(2.0) + 1e-6).log10();
    db.round_ties_even() as i64
}

fn dyn_label_to_volume(velocity: i64) -> i64 {
    velocity_to_volume(velocity)
}

fn fermata_factor(shape: &str) -> f64 {
    match shape {
        "" | "normal" => 1.5,
        "angled" | "short" => 1.25,
        "square" | "long" => 2.0,
        "double-square" | "double-dot" | "very-long" => 3.0,
        "half-curve" | "curlew" | "double-angled" => 1.75,
        _ => 1.5,
    }
}

fn apply_fermata_stretch(
    track: &mut SingingTrack,
    fermatas: &[FermataInfo],
    tempo_track: bool,
    tempos: &mut [SongTempo],
) {    let mut index_to_shape: HashMap<usize, String> = HashMap::new();
    for fermata in fermatas {
        index_to_shape.insert(fermata.note_index, fermata.shape.clone());
    }
    let mut thresholds: Vec<(i64, i64)> = Vec::new();
    let mut shift = 0i64;
    for (index, note) in track.note_list.iter_mut().enumerate() {
        note.start_pos += shift;
        if let Some(shape) = index_to_shape.get(&index) {
            let shape = if shape.is_empty() { "normal" } else { shape };
            let factor = fermata_factor(shape);
            let extra = (note.length as f64 * (factor - 1.0)).round_ties_even() as i64;
            if extra > 0 {
                let threshold = note.start_pos + note.length;
                note.length += extra;
                thresholds.push((threshold, extra));
                shift += extra;
            }
        }
    }
    if thresholds.is_empty() {
        return;
    }
    let delta = |x: i64| -> i64 {
        thresholds
            .iter()
            .filter(|(threshold, _)| x > *threshold)
            .map(|(_, extra)| *extra)
            .sum()
    };
    if !track.edited_params.volume.is_empty() {
        let existing = std::mem::take(&mut track.edited_params.volume);
        let shifted = existing
            .points
            .iter()
            .map(|point| Point::new(point.x + delta(point.x), point.y))
            .collect();
        track.edited_params.volume = ParamCurve::new(shifted);
    }
    if tempo_track {
        for tempo in tempos.iter_mut() {
            tempo.position += delta(tempo.position);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_ticks_match_python() {
        // `int(Decimal(duration) * (Decimal(480) / Decimal(divisions)))`.
        for (divisions, duration, expected) in [
            (7, "7", 480),
            (7, "25", 1714),
            (7, "1.5", 102),
            (7, "125", 8571),
            (7, "3.333", 228),
            (7, "1e3", 68571),
            (7, "-25", -1714),
            (7, "0.001", 0),
            (1000, "7", 3),
            (1000, "25", 12),
            (1000, "1.5", 0),
            (1000, "125", 60),
            (1000, "3.333", 1),
            (1000, "1e3", 480),
            (1000, "-25", -12),
            (960, "125", 62),
            (960, "1e3", 500),
            (480, "7", 7),
            (480, "1.5", 1),
            (480, "3.333", 3),
            (480, "1e3", 1000),
        ] {
            assert_eq!(
                decimal_ticks(Some(duration), divisions),
                expected,
                "divisions={divisions} duration={duration}"
            );
        }
    }

    #[test]
    fn fermatas_shift_later_notes_and_tempos() {
        let mut track = SingingTrack::new("t");
        track.note_list = vec![
            Note::new(0, 480, 60, "a"),
            Note::new(480, 480, 62, "b"),
            Note::new(960, 480, 64, "c"),
        ];
        let fermatas = vec![FermataInfo {
            note_index: 0,
            shape: "normal".to_owned(),
        }];
        let mut tempos = vec![SongTempo::new(0, 120.0), SongTempo::new(960, 90.0)];
        apply_fermata_stretch(&mut track, &fermatas, true, &mut tempos);
        assert_eq!(track.note_list[0].length, 720);
        assert_eq!(track.note_list[1].start_pos, 720);
        assert_eq!(track.note_list[2].start_pos, 1200);
        assert_eq!(tempos[0].position, 0);
        assert_eq!(tempos[1].position, 1200);
    }
}

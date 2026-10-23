//! Hand-written port of the tatsu grammar in `libresvip.plugins.ust.model` plus
//! the `UstWalker`.
//!
//! The grammar is a PEG: alternatives are tried in order and the first match is
//! committed to, even when it leaves part of the line unconsumed (which then
//! turns into a parse error). The parser below mirrors that behaviour instead
//! of being lenient.

use libresvip_core::{LibreSvipError, Result};

use super::model::{
    Envelope, ListValue, Note, PitchBendMode, PitchBendType, TimeSignature, Track, UtauProject,
    MAX_ACCEPTED_BPM,
};

fn parse_error(message: impl Into<String>) -> LibreSvipError {
    LibreSvipError::Parse(message.into())
}

// ---------------------------------------------------------------------------
// Cursor
// ---------------------------------------------------------------------------

struct Cursor<'a> {
    text: &'a str,
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, pos: 0 }
    }

    fn rest(&self) -> &'a str {
        &self.text[self.pos..]
    }

    fn starts_with(&self, prefix: &str) -> bool {
        self.rest().starts_with(prefix)
    }

    fn eat(&mut self, prefix: &str) -> bool {
        if self.starts_with(prefix) {
            self.pos += prefix.len();
            true
        } else {
            false
        }
    }

    /// `/\r?\n/`.
    fn eat_newline(&mut self) -> bool {
        self.eat("\r\n") || self.eat("\n")
    }

    /// `value = /[^\r\n]*/` — the rest of the current line, not consumed.
    fn line(&self) -> &'a str {
        let rest = self.rest();
        match rest.find(['\r', '\n']) {
            Some(index) => &rest[..index],
            None => rest,
        }
    }
}

// ---------------------------------------------------------------------------
// Regex-like scanners (byte oriented, patterns are ASCII only)
// ---------------------------------------------------------------------------

/// `float = /[-]?(?:(?:\d+(?:\.\d*)?)|(?:\.\d+))(?:[eE][-+]?\d+)?/` — returns
/// the matched length.
fn scan_float(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut index = 0usize;
    if index < bytes.len() && bytes[index] == b'-' {
        index += 1;
    }
    let mut integer_digits = 0usize;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
        integer_digits += 1;
    }
    if integer_digits > 0 {
        if index < bytes.len() && bytes[index] == b'.' {
            index += 1;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
        }
    } else if index < bytes.len() && bytes[index] == b'.' {
        index += 1;
        let mut fraction_digits = 0usize;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
            fraction_digits += 1;
        }
        if fraction_digits == 0 {
            return None;
        }
    } else {
        return None;
    }
    if index < bytes.len() && (bytes[index] == b'e' || bytes[index] == b'E') {
        let saved = index;
        index += 1;
        if index < bytes.len() && (bytes[index] == b'+' || bytes[index] == b'-') {
            index += 1;
        }
        let mut exponent_digits = 0usize;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
            exponent_digits += 1;
        }
        if exponent_digits == 0 {
            index = saved;
        }
    }
    Some(index)
}

/// `int = /[-]?[0-9]+/` — returns the matched length.
fn scan_int(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut index = 0usize;
    if index < bytes.len() && bytes[index] == b'-' {
        index += 1;
    }
    let start = index;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    if index == start {
        None
    } else {
        Some(index)
    }
}

fn parse_f64(text: &str) -> Result<f64> {
    text.parse::<f64>()
        .map_err(|_| parse_error(format!("invalid float: {text:?}")))
}

fn parse_i64(text: &str) -> Result<i64> {
    text.parse::<i64>()
        .map_err(|_| parse_error(format!("invalid integer: {text:?}")))
}

/// A committed alternative must leave only spaces on the line (`{" "}*`).
fn commit<'a>(line: &'a str, length: usize) -> Result<&'a str> {
    let (value, tail) = line.split_at(length);
    if tail.trim_end_matches(' ').is_empty() {
        Ok(value)
    } else {
        Err(parse_error(format!(
            "unexpected trailing characters after {value:?}"
        )))
    }
}

/// `key:"..." "="` — the part of the line after the equals sign.
fn after_equals<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.strip_prefix(key).and_then(|rest| rest.strip_prefix('='))
}

// ---------------------------------------------------------------------------
// Setting section
// ---------------------------------------------------------------------------

enum Setting {
    UstVersion(f64),
    Tempo(String),
    TimeSignatures(Vec<TimeSignature>),
    Tracks(i64),
    Project(String),
    VoiceDir(String),
    OutFile(String),
    CacheDir(String),
    Tool1(String),
    Tool2(String),
    Mode2(bool),
    Autoren(bool),
    MapFirst(bool),
    Flags(String),
    Charset(String),
}

/// `ust_time_signature = "(" numerator:int "/" denominator:int "/" bar_index:int ")"`
fn parse_time_signature(text: &str) -> Option<(TimeSignature, usize)> {
    let mut index = 0usize;
    if !text[index..].starts_with('(') {
        return None;
    }
    index += 1;
    let numerator_len = scan_int(&text[index..])?;
    let numerator = parse_i64(&text[index..index + numerator_len]).ok()?;
    index += numerator_len;
    if !text[index..].starts_with('/') {
        return None;
    }
    index += 1;
    let denominator_len = scan_int(&text[index..])?;
    let denominator = parse_i64(&text[index..index + denominator_len]).ok()?;
    index += denominator_len;
    if !text[index..].starts_with('/') {
        return None;
    }
    index += 1;
    let bar_index_len = scan_int(&text[index..])?;
    let bar_index = parse_i64(&text[index..index + bar_index_len]).ok()?;
    index += bar_index_len;
    if !text[index..].starts_with(')') {
        return None;
    }
    index += 1;
    Some((
        TimeSignature {
            numerator,
            denominator,
            bar_index,
        },
        index,
    ))
}

fn parse_time_signatures(text: &str) -> Option<(Vec<TimeSignature>, usize)> {
    let (first, mut consumed) = parse_time_signature(text)?;
    let mut values = vec![first];
    loop {
        let saved = consumed;
        if !text[consumed..].starts_with(',') {
            break;
        }
        consumed += 1;
        match parse_time_signature(&text[consumed..]) {
            Some((value, length)) => {
                values.push(value);
                consumed += length;
            }
            None => {
                consumed = saved;
                break;
            }
        }
    }
    // `[","]`
    if text[consumed..].starts_with(',') {
        consumed += 1;
    }
    Some((values, consumed))
}

fn parse_setting_line(line: &str) -> Result<Option<Setting>> {
    // `(key:"UstVersion" "=" value:float)`
    if let Some(rest) = after_equals(line, "UstVersion") {
        if let Some(length) = scan_float(rest) {
            let value = parse_f64(commit(rest, length)?)?;
            return Ok(Some(Setting::UstVersion(value)));
        }
    }
    // `(key:"Tempo" "=" value:value)`
    if let Some(rest) = after_equals(line, "Tempo") {
        return Ok(Some(Setting::Tempo(rest.to_owned())));
    }
    // `(key:"TimeSignatures" "=" value:(...))`
    if let Some(rest) = after_equals(line, "TimeSignatures") {
        if let Some((values, length)) = parse_time_signatures(rest) {
            commit(rest, length)?;
            return Ok(Some(Setting::TimeSignatures(values)));
        }
    }
    // `(key:"Tracks" "=" value:int)`
    if let Some(rest) = after_equals(line, "Tracks") {
        if let Some(length) = scan_int(rest) {
            let value = parse_i64(commit(rest, length)?)?;
            return Ok(Some(Setting::Tracks(value)));
        }
    }
    // `(key:?"Project(Name)?" "=" value:value)`
    for key in ["Project", "ProjectName"] {
        if let Some(rest) = after_equals(line, key) {
            return Ok(Some(Setting::Project(rest.to_owned())));
        }
    }
    for (key, build) in [
        ("VoiceDir", Setting::VoiceDir as fn(String) -> Setting),
        ("OutFile", Setting::OutFile),
        ("CacheDir", Setting::CacheDir),
        ("Tool1", Setting::Tool1),
        ("Tool2", Setting::Tool2),
    ] {
        if let Some(rest) = after_equals(line, key) {
            return Ok(Some(build(rest.to_owned())));
        }
    }
    for (key, build) in [
        ("Mode2", Setting::Mode2 as fn(bool) -> Setting),
        ("Autoren", Setting::Autoren),
        ("MapFirst", Setting::MapFirst),
    ] {
        if let Some(rest) = after_equals(line, key) {
            if let Some((value, length)) = parse_bool(rest) {
                commit(rest, length)?;
                return Ok(Some(build(value)));
            }
        }
    }
    if let Some(rest) = after_equals(line, "Flags") {
        return Ok(Some(Setting::Flags(rest.to_owned())));
    }
    if let Some(rest) = after_equals(line, "Charset") {
        return Ok(Some(Setting::Charset(rest.to_owned())));
    }
    Ok(None)
}

/// `bool = "1" | "0" | "True" | "False"`.
///
/// tatsu's `::bool` coercion is Python's `bool()` applied to the matched
/// *string*, which is truthy for every alternative (`"0"` included), so all
/// four forms yield `True`. The pydantic models therefore always see `True`.
fn parse_bool(text: &str) -> Option<(bool, usize)> {
    for token in ["1", "0", "True", "False"] {
        if text.starts_with(token) {
            return Some((true, token.len()));
        }
    }
    None
}

/// `tempo2bpm`: `float(tempo.replace(",", "."))`.
fn tempo2bpm(text: &str) -> Result<f64> {
    let normalized = text.replace(',', ".");
    normalized
        .parse::<f64>()
        .map_err(|_| parse_error(format!("invalid tempo: {text:?}")))
}

fn apply_setting(project: &mut UtauProject, setting: Setting) -> Result<()> {
    match setting {
        Setting::UstVersion(value) => project.ust_version = Some(value),
        Setting::Tempo(text) => {
            let value = tempo2bpm(&text)?;
            if value < MAX_ACCEPTED_BPM {
                project.tempo = Some(value);
            }
        }
        Setting::TimeSignatures(values) => project.time_signatures = values,
        Setting::Tracks(value) => project.track_count = value,
        Setting::Project(value) => project.project_name = value,
        Setting::VoiceDir(value) => project.voice_dir = Some(value),
        Setting::OutFile(value) => project.out_file = Some(value),
        Setting::CacheDir(value) => project.cache_dir = Some(value),
        Setting::Tool1(value) => project.tool1 = Some(value),
        Setting::Tool2(value) => project.tool2 = Some(value),
        Setting::Mode2(value) => project.pitch_mode2 = value,
        Setting::Autoren(value) => project.autoren = Some(value),
        Setting::MapFirst(value) => project.map_first = Some(value),
        Setting::Flags(value) => project.flags = Some(value),
        Setting::Charset(value) => project.charset = Some(value),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Note attributes
// ---------------------------------------------------------------------------

enum Attr {
    Length(f64),
    Duration(f64),
    Lyric(String),
    NoteNum(i64),
    Delta(i64),
    PreUtterance(String),
    VoiceOverlap(f64),
    Intensity(f64),
    Modulation(f64),
    StartPoint(f64),
    Envelope(Envelope),
    Tempo(String),
    Velocity(f64),
    Label(String),
    Flags(String),
    PitchBendType(PitchBendType),
    PitchBendStart(f64),
    PitchBendPoints(Vec<i64>),
    Pbs(Vec<ListValue>),
    Pbw(Vec<ListValue>),
    Pby(Vec<ListValue>),
    Pbm(Vec<PitchBendMode>),
    StpTrim(f64),
    Layer(i64),
    AtPreutterance(f64),
    AtOverlap(f64),
    AtStartPoint(f64),
    SampleFilename(String),
    Alias(String),
    CacheLocation(String),
}

/// `optional_float = float | "null" | ()` — `Empty` stands for the empty string
/// and for `"null"`.
fn parse_optional_float(text: &str) -> Option<(ListValue, usize)> {
    if let Some(length) = scan_float(text) {
        let value = text[..length].parse::<f64>().ok()?;
        return Some((ListValue::Float(value), length));
    }
    if text.starts_with("null") {
        return Some((ListValue::Empty, 4));
    }
    Some((ListValue::Empty, 0))
}

/// `optional_float {sep? optional_float}*`.
fn parse_optional_float_list(
    text: &str,
    separators: &[char],
    require_separator: bool,
) -> Option<(Vec<ListValue>, usize)> {
    let (first, mut consumed) = parse_optional_float(text)?;
    let mut values = vec![first];
    loop {
        let saved = consumed;
        if let Some(character) = text[consumed..].chars().next() {
            if separators.contains(&character) {
                consumed += character.len_utf8();
            } else if require_separator {
                break;
            }
        } else if require_separator {
            break;
        }
        let (value, length) = parse_optional_float(&text[consumed..])?;
        if length == 0 && consumed == saved {
            // A zero-width repetition makes tatsu stop instead of looping.
            consumed = saved;
            break;
        }
        values.push(value);
        consumed += length;
    }
    Some((values, consumed))
}

/// `utau_pitch_bend_mode = "s" | "r" | "j" | "null" | ()`.
fn parse_pitch_bend_mode(text: &str) -> (PitchBendMode, usize) {
    if text.starts_with('s') {
        (PitchBendMode::S, 1)
    } else if text.starts_with('r') {
        (PitchBendMode::R, 1)
    } else if text.starts_with('j') {
        (PitchBendMode::J, 1)
    } else if text.starts_with("null") {
        (PitchBendMode::Empty, 4)
    } else {
        (PitchBendMode::Empty, 0)
    }
}

fn parse_pitch_bend_modes(text: &str) -> (Vec<PitchBendMode>, usize) {
    let (first, mut consumed) = parse_pitch_bend_mode(text);
    let mut values = vec![first];
    loop {
        if !text[consumed..].starts_with(',') {
            break;
        }
        consumed += 1;
        let (value, length) = parse_pitch_bend_mode(&text[consumed..]);
        values.push(value);
        consumed += length;
    }
    (values, consumed)
}

/// `ust_envelope`.
fn parse_envelope(text: &str) -> Option<(Envelope, usize)> {
    let mut index = 0usize;
    let mut values = [0.0f64; 7];
    for (offset, slot) in values.iter_mut().enumerate() {
        let length = scan_float(&text[index..])?;
        *slot = text[index..index + length].parse::<f64>().ok()?;
        index += length;
        if offset < 6 {
            if !text[index..].starts_with(',') {
                return None;
            }
            index += 1;
        }
    }
    let mut envelope = Envelope {
        p1: values[0],
        p2: values[1],
        p3: values[2],
        v1: values[3],
        v2: values[4],
        v3: values[5],
        v4: values[6],
        p4: None,
        p5: None,
        v5: None,
        other_points: None,
    };
    if text[index..].starts_with(",%,") {
        index += 3;
        if let Some(length) = scan_float(&text[index..]) {
            envelope.p4 = text[index..index + length].parse::<f64>().ok();
            index += length;
        }
        if text[index..].starts_with(',') {
            let saved = index;
            index += 1;
            if let Some(length) = scan_float(&text[index..]) {
                envelope.p5 = text[index..index + length].parse::<f64>().ok();
                index += length;
                if text[index..].starts_with(',') {
                    let saved_inner = index;
                    index += 1;
                    if let Some(length) = scan_float(&text[index..]) {
                        envelope.v5 = text[index..index + length].parse::<f64>().ok();
                        index += length;
                    } else {
                        index = saved_inner;
                    }
                }
            } else {
                index = saved;
            }
        }
    } else if text[index..].starts_with(",,") {
        index += 2;
        let length = scan_float(&text[index..])?;
        envelope.p4 = text[index..index + length].parse::<f64>().ok();
        index += length;
    } else if text[index..].starts_with(",%") {
        index += 2;
    } else {
        let mut points = Vec::new();
        while text[index..].starts_with(',') {
            let saved = index;
            index += 1;
            if let Some(length) = scan_float(&text[index..]) {
                points.push(text[index..index + length].parse::<f64>().ok()?);
                index += length;
            } else {
                index = saved;
                break;
            }
        }
        envelope.other_points = Some(points);
    }
    Some((envelope, index))
}

/// Parse one `ust_note_attr` line. `Ok(None)` means the generic catch-all
/// matched (the attribute is unknown and ignored).
fn parse_note_attr(line: &str) -> Result<Option<Attr>> {
    if let Some(rest) = after_equals(line, "Length") {
        if let Some(length) = scan_float(rest) {
            return Ok(Some(Attr::Length(parse_f64(commit(rest, length)?)?)));
        }
    }
    if let Some(rest) = after_equals(line, "Duration") {
        if let Some(length) = scan_float(rest) {
            return Ok(Some(Attr::Duration(parse_f64(commit(rest, length)?)?)));
        }
    }
    if let Some(rest) = after_equals(line, "Lyric") {
        return Ok(Some(Attr::Lyric(rest.to_owned())));
    }
    if let Some(rest) = after_equals(line, "NoteNum") {
        if let Some(length) = scan_int(rest) {
            return Ok(Some(Attr::NoteNum(parse_i64(commit(rest, length)?)?)));
        }
    }
    if let Some(rest) = after_equals(line, "Delta") {
        if let Some(length) = scan_int(rest) {
            return Ok(Some(Attr::Delta(parse_i64(commit(rest, length)?)?)));
        }
    }
    if let Some(rest) = after_equals(line, "PreUtterance") {
        return Ok(Some(Attr::PreUtterance(rest.to_owned())));
    }
    if let Some(rest) = after_equals(line, "VoiceOverlap") {
        if let Some(length) = scan_float(rest) {
            return Ok(Some(Attr::VoiceOverlap(parse_f64(commit(rest, length)?)?)));
        }
    }
    if let Some(rest) = after_equals(line, "Intensity") {
        if let Some(length) = scan_float(rest) {
            return Ok(Some(Attr::Intensity(parse_f64(commit(rest, length)?)?)));
        }
    }
    for key in ["Modulation", "Moduration"] {
        if let Some(rest) = after_equals(line, key) {
            if let Some(length) = scan_float(rest) {
                return Ok(Some(Attr::Modulation(parse_f64(commit(rest, length)?)?)));
            }
        }
    }
    if let Some(rest) = after_equals(line, "StartPoint") {
        if let Some(length) = scan_float(rest) {
            return Ok(Some(Attr::StartPoint(parse_f64(commit(rest, length)?)?)));
        }
    }
    if let Some(rest) = after_equals(line, "Envelope") {
        if let Some((envelope, length)) = parse_envelope(rest) {
            commit(rest, length)?;
            return Ok(Some(Attr::Envelope(envelope)));
        }
    }
    if let Some(rest) = after_equals(line, "Tempo") {
        return Ok(Some(Attr::Tempo(rest.to_owned())));
    }
    if let Some(rest) = after_equals(line, "Velocity") {
        if let Some(length) = scan_float(rest) {
            return Ok(Some(Attr::Velocity(parse_f64(commit(rest, length)?)?)));
        }
    }
    if let Some(rest) = after_equals(line, "Label") {
        return Ok(Some(Attr::Label(rest.to_owned())));
    }
    if let Some(rest) = after_equals(line, "Flags") {
        return Ok(Some(Attr::Flags(rest.to_owned())));
    }
    if let Some(rest) = after_equals(line, "PBType") {
        if rest.starts_with('5') {
            commit(rest, 1)?;
            return Ok(Some(Attr::PitchBendType(PitchBendType::Five)));
        }
        if rest.starts_with("OldData") {
            commit(rest, 7)?;
            return Ok(Some(Attr::PitchBendType(PitchBendType::OldData)));
        }
    }
    if let Some(rest) = after_equals(line, "PBStart") {
        if let Some(length) = scan_float(rest) {
            return Ok(Some(Attr::PitchBendStart(parse_f64(commit(rest, length)?)?)));
        }
    }
    for key in ["Piches", "Pitches", "PitchBend"] {
        if let Some(rest) = after_equals(line, key) {
            if let Some(length) = scan_int(rest) {
                // `value:(int {"," int}*)`
                let mut consumed = length;
                let mut values = vec![parse_i64(&rest[..length])?];
                loop {
                    let saved = consumed;
                    if !rest[consumed..].starts_with(',') {
                        break;
                    }
                    consumed += 1;
                    match scan_int(&rest[consumed..]) {
                        Some(next) => {
                            values.push(parse_i64(&rest[consumed..consumed + next])?);
                            consumed += next;
                        }
                        None => {
                            consumed = saved;
                            break;
                        }
                    }
                }
                commit(rest, consumed)?;
                return Ok(Some(Attr::PitchBendPoints(values)));
            }
        }
    }
    if let Some(rest) = after_equals(line, "PBS") {
        if let Some((values, length)) = parse_optional_float_list(rest, &[';', ','], false) {
            commit(rest, length)?;
            return Ok(Some(Attr::Pbs(values)));
        }
    }
    for (key, build) in [
        ("PBW", Attr::Pbw as fn(Vec<ListValue>) -> Attr),
        ("PBY", Attr::Pby),
    ] {
        if let Some(rest) = after_equals(line, key) {
            if let Some((values, length)) = parse_optional_float_list(rest, &[','], true) {
                commit(rest, length)?;
                return Ok(Some(build(values)));
            }
        }
    }
    if let Some(rest) = after_equals(line, "PBM") {
        let (values, length) = parse_pitch_bend_modes(rest);
        commit(rest, length)?;
        return Ok(Some(Attr::Pbm(values)));
    }
    // `VBR` is parsed by Python but stored under the wrong key (`vibrato`
    // instead of `vbr`), so pydantic drops it and the field stays `None`.
    if let Some(rest) = after_equals(line, "VBR") {
        if let Some((_, length)) = parse_optional_float_list(rest, &[','], true) {
            commit(rest, length)?;
            return Ok(None);
        }
    }
    if let Some(rest) = after_equals(line, "stptrim") {
        if let Some(length) = scan_float(rest) {
            return Ok(Some(Attr::StpTrim(parse_f64(commit(rest, length)?)?)));
        }
    }
    if let Some(rest) = after_equals(line, "layer") {
        if let Some(length) = scan_int(rest) {
            return Ok(Some(Attr::Layer(parse_i64(commit(rest, length)?)?)));
        }
    }
    if let Some(rest) = after_equals(line, "@preuttr") {
        if let Some(length) = scan_float(rest) {
            return Ok(Some(Attr::AtPreutterance(parse_f64(commit(rest, length)?)?)));
        }
    }
    if let Some(rest) = after_equals(line, "@overlap") {
        if let Some(length) = scan_float(rest) {
            return Ok(Some(Attr::AtOverlap(parse_f64(commit(rest, length)?)?)));
        }
    }
    if let Some(rest) = after_equals(line, "@stpoint") {
        if let Some(length) = scan_float(rest) {
            return Ok(Some(Attr::AtStartPoint(parse_f64(commit(rest, length)?)?)));
        }
    }
    if let Some(rest) = after_equals(line, "@filename") {
        return Ok(Some(Attr::SampleFilename(rest.to_owned())));
    }
    if let Some(rest) = after_equals(line, "@alias") {
        return Ok(Some(Attr::Alias(rest.to_owned())));
    }
    if let Some(rest) = after_equals(line, "@cache") {
        return Ok(Some(Attr::CacheLocation(rest.to_owned())));
    }
    // `(key:/\$?[^=\r\n]+/ "=" value:value)` and the catch-all both end up
    // ignoring the attribute.
    Ok(None)
}

// ---------------------------------------------------------------------------
// Notes and tracks
// ---------------------------------------------------------------------------

/// `ust_note_head = "[#" note_type:?"(\d+|PREV|NEXT|INSERT|DELETE)" "]"`.
fn note_head(text: &str) -> Option<(&str, usize)> {
    let rest = text.strip_prefix("[#")?;
    let bytes = rest.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    if index == 0 {
        for token in ["PREV", "NEXT", "INSERT", "DELETE"] {
            if rest.starts_with(token) {
                index = token.len();
                break;
            }
        }
        if index == 0 {
            return None;
        }
    }
    let note_type = &rest[..index];
    let after = &rest[index..];
    let closing = after.strip_prefix(']')?;
    let total = text.len() - closing.len();
    Some((note_type, total))
}

fn is_note_head(text: &str) -> bool {
    note_head(text).is_some()
}

fn parse_note(cursor: &mut Cursor<'_>) -> Result<Option<Note>> {
    let Some((note_type, head_length)) = note_head(cursor.rest()) else {
        return Ok(None);
    };
    let mut note = Note {
        note_type: note_type.to_owned(),
        ..Default::default()
    };
    cursor.pos += head_length;
    let mut has_length = false;
    let mut has_lyric = false;
    let mut has_note_num = false;
    loop {
        let saved = cursor.pos;
        if !cursor.eat_newline() {
            cursor.pos = saved;
            break;
        }
        if is_note_head(cursor.rest()) || cursor.starts_with("[#TRACKEND]") {
            cursor.pos = saved;
            break;
        }
        let line = cursor.line();
        cursor.pos += line.len();
        if let Some(attr) = parse_note_attr(line)? {
            match attr {
                Attr::Length(value) => {
                    note.length = float_to_int(value)?;
                    has_length = true;
                }
                Attr::Duration(value) => note.duration = Some(float_to_int(value)?),
                Attr::Lyric(value) => {
                    note.lyric = value;
                    has_lyric = true;
                }
                Attr::NoteNum(value) => {
                    note.note_num = value;
                    has_note_num = true;
                }
                Attr::Delta(value) => note.delta = Some(value),
                Attr::PreUtterance(value) => note.pre_utterance = Some(value),
                Attr::VoiceOverlap(value) => note.voice_overlap = Some(value),
                Attr::Intensity(value) => note.intensity = Some(value),
                Attr::Modulation(value) => note.modulation = Some(value),
                Attr::StartPoint(value) => note.start_point = Some(value),
                Attr::Envelope(value) => note.envelope = Some(value),
                Attr::Tempo(value) => note.tempo = Some(tempo2bpm(&value)?),
                Attr::Velocity(value) => note.velocity = Some(value),
                Attr::Label(value) => note.label = Some(value),
                Attr::Flags(value) => note.flags = Some(value),
                Attr::PitchBendType(value) => note.pitchbend_type = Some(value),
                Attr::PitchBendStart(value) => note.pitchbend_start = Some(value),
                Attr::PitchBendPoints(values) => note.pitch_bend_points = values,
                Attr::Pbs(values) => note.pbs = values,
                Attr::Pbw(values) => note.pbw = values,
                Attr::Pby(values) => note.pby = values,
                Attr::Pbm(values) => note.pbm = values,
                Attr::StpTrim(value) => note.stp_trim = Some(value),
                Attr::Layer(value) => note.layer = Some(value),
                Attr::AtPreutterance(value) => note.at_preutterance = Some(value),
                Attr::AtOverlap(value) => note.at_overlap = Some(value),
                Attr::AtStartPoint(value) => note.at_start_point = Some(value),
                Attr::SampleFilename(value) => note.sample_filename = Some(value),
                Attr::Alias(value) => note.alias = Some(value),
                Attr::CacheLocation(value) => note.cache_location = Some(value),
            }
        }
    }
    if !has_length {
        return Err(LibreSvipError::Validation(
            "UTAUNote: missing field `length`".to_owned(),
        ));
    }
    if !has_lyric {
        return Err(LibreSvipError::Validation(
            "UTAUNote: missing field `lyric`".to_owned(),
        ));
    }
    if !has_note_num {
        return Err(LibreSvipError::Validation(
            "UTAUNote: missing field `note_num`".to_owned(),
        ));
    }
    Ok(Some(note))
}

/// pydantic's lax `float -> int` conversion: the value must be integral.
fn float_to_int(value: f64) -> Result<i64> {
    if !value.is_finite() || value.fract() != 0.0 {
        return Err(LibreSvipError::Validation(format!(
            "input should be a valid integer, got {value}"
        )));
    }
    Ok(value as i64)
}

fn parse_track(cursor: &mut Cursor<'_>) -> Result<Option<Track>> {
    let mut notes: Vec<Note> = Vec::new();
    loop {
        let saved = cursor.pos;
        if !cursor.eat_newline() {
            cursor.pos = saved;
            break;
        }
        match parse_note(cursor)? {
            Some(note) => notes.push(note),
            None => {
                cursor.pos = saved;
                break;
            }
        }
    }
    let saved = cursor.pos;
    if cursor.eat_newline() && !cursor.eat("[#TRACKEND]") {
        cursor.pos = saved;
    }
    if notes.is_empty() {
        return Ok(None);
    }
    Ok(Some(Track { notes }))
}

// ---------------------------------------------------------------------------
// Project
// ---------------------------------------------------------------------------

/// `ust_header` — returns the version and the optional `Charset` value.
fn parse_header(cursor: &mut Cursor<'_>) -> Option<(f64, Option<String>)> {
    if !cursor.eat("[#VERSION]") {
        return None;
    }
    if !cursor.eat_newline() {
        return None;
    }
    if !cursor.eat("UST Version") {
        return None;
    }
    cursor.eat("=");
    let length = scan_float(cursor.rest())?;
    let version = cursor.rest()[..length].parse::<f64>().ok()?;
    cursor.pos += length;
    let saved = cursor.pos;
    let mut charset = None;
    if cursor.eat_newline() {
        if cursor.eat("Charset") && cursor.eat("=") {
            let value = cursor.line();
            cursor.pos += value.len();
            charset = Some(value.to_owned());
        } else {
            cursor.pos = saved;
        }
    } else {
        cursor.pos = saved;
    }
    Some((version, charset))
}

/// Parse a UST document into the intermediate [`UtauProject`] model.
pub fn parse_ust(text: &str) -> Result<UtauProject> {
    let mut cursor = Cursor::new(text);
    let mut project = UtauProject::default();
    if cursor.starts_with("[#VERSION]") {
        match parse_header(&mut cursor) {
            Some((version, charset)) => {
                project.ust_version = Some(version);
                project.charset = charset;
            }
            None => return Err(parse_error("invalid UST header")),
        }
    }

    // `[newline] "[#SETTING]"`
    cursor.eat_newline();
    if !cursor.eat("[#SETTING]") {
        return Err(parse_error("expected [#SETTING]"));
    }
    loop {
        let saved = cursor.pos;
        if !cursor.eat_newline() {
            cursor.pos = saved;
            break;
        }
        let line = cursor.line();
        match parse_setting_line(line)? {
            Some(setting) => {
                cursor.pos += line.len();
                apply_setting(&mut project, setting)?;
            }
            None => {
                cursor.pos = saved;
                break;
            }
        }
    }

    loop {
        let saved = cursor.pos;
        if let Some(track) = parse_track(&mut cursor)? {
            project.track.push(track);
        }
        if cursor.pos == saved {
            break;
        }
    }

    while cursor.eat_newline() {}
    if cursor.pos != text.len() {
        return Err(parse_error(format!(
            "unexpected content at byte {}: {:?}",
            cursor.pos,
            cursor.rest().lines().next().unwrap_or("")
        )));
    }
    Ok(project)
}

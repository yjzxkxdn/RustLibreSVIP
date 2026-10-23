//! Port of `libresvip.plugins.tlpx.model` (the current TuneLab project model).

use libresvip_core::error::{LibreSvipError, Result};

use super::pyjson::Json;

fn validation(message: impl Into<String>) -> LibreSvipError {
    LibreSvipError::Validation(message.into())
}

fn field<'a>(value: &'a Json, key: &str) -> Option<&'a Json> {
    value.get(key)
}

fn require<'a>(value: &'a Json, key: &str) -> Result<&'a Json> {
    field(value, key).ok_or_else(|| validation(format!("missing field `{key}`")))
}

fn f64_of(value: &Json, key: &str) -> Result<f64> {
    value
        .as_f64()
        .ok_or_else(|| validation(format!("field `{key}` must be a number")))
}

fn i64_of(value: &Json, key: &str) -> Result<i64> {
    value
        .as_i64()
        .ok_or_else(|| validation(format!("field `{key}` must be an integer")))
}

fn str_of(value: &Json, key: &str) -> Result<String> {
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| validation(format!("field `{key}` must be a string")))
}

fn bool_of(value: &Json, key: &str) -> Result<bool> {
    value
        .as_bool()
        .ok_or_else(|| validation(format!("field `{key}` must be a boolean")))
}

fn require_f64(value: &Json, key: &str) -> Result<f64> {
    f64_of(require(value, key)?, key)
}

fn require_i64(value: &Json, key: &str) -> Result<i64> {
    i64_of(require(value, key)?, key)
}

fn require_str(value: &Json, key: &str) -> Result<String> {
    str_of(require(value, key)?, key)
}

fn require_bool(value: &Json, key: &str) -> Result<bool> {
    bool_of(require(value, key)?, key)
}

fn opt_i64(value: &Json, key: &str, default: i64) -> Result<i64> {
    match field(value, key) {
        None | Some(Json::Null) => Ok(default),
        Some(item) => i64_of(item, key),
    }
}

fn opt_str(value: &Json, key: &str, default: &str) -> Result<String> {
    match field(value, key) {
        None | Some(Json::Null) => Ok(default.to_owned()),
        Some(item) => str_of(item, key),
    }
}

fn opt_bool(value: &Json, key: &str, default: bool) -> Result<bool> {
    match field(value, key) {
        None | Some(Json::Null) => Ok(default),
        Some(item) => bool_of(item, key),
    }
}

fn opt_f64(value: &Json, key: &str, default: f64) -> Result<f64> {
    match field(value, key) {
        None | Some(Json::Null) => Ok(default),
        Some(item) => f64_of(item, key),
    }
}

fn opt_f64_opt(value: &Json, key: &str) -> Result<Option<f64>> {
    match field(value, key) {
        None | Some(Json::Null) => Ok(None),
        Some(item) => Ok(Some(f64_of(item, key)?)),
    }
}

fn opt_string(value: &Json, key: &str) -> Result<Option<String>> {
    match field(value, key) {
        None | Some(Json::Null) => Ok(None),
        Some(item) => Ok(Some(str_of(item, key)?)),
    }
}

fn opt_object(value: &Json, key: &str) -> Json {
    match field(value, key) {
        Some(item @ Json::Object(_)) => item.clone(),
        _ => Json::empty_object(),
    }
}

fn array_of<'a>(value: &'a Json, key: &str) -> Result<&'a [Json]> {
    match field(value, key) {
        None | Some(Json::Null) => Ok(&[]),
        Some(Json::Array(items)) => Ok(items),
        Some(_) => Err(validation(format!("field `{key}` must be an array"))),
    }
}

fn push_field(entries: &mut Vec<(String, Json)>, key: &str, value: Option<Json>) {
    if let Some(value) = value {
        entries.push((key.to_owned(), value));
    }
}

// ---------------------------------------------------------------------------
// Points / tempo / time signature
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TuneLabPoint {
    pub pos: f64,
    pub value: f64,
}

impl TuneLabPoint {
    pub fn new(pos: f64, value: f64) -> Self {
        Self { pos, value }
    }
}

pub fn points_to_json(points: &[TuneLabPoint]) -> Json {
    let mut items = Vec::with_capacity(points.len() * 2);
    for point in points {
        items.push(Json::Float(point.pos));
        items.push(Json::Float(point.value));
    }
    Json::Array(items)
}

pub fn points_from_json(value: &Json) -> Result<Vec<TuneLabPoint>> {
    let items = value
        .as_array()
        .ok_or_else(|| validation("points must be an array"))?;
    if items.is_empty() {
        return Ok(Vec::new());
    }
    let nested = items
        .iter()
        .any(|item| matches!(item, Json::Array(_) | Json::Object(_)));
    if nested {
        let mut points = Vec::with_capacity(items.len());
        for item in items {
            match item {
                Json::Array(pair) => {
                    if pair.len() < 2 {
                        return Err(validation("point tuple must have two items"));
                    }
                    points.push(TuneLabPoint::new(
                        f64_of(&pair[0], "pos")?,
                        f64_of(&pair[1], "value")?,
                    ));
                }
                Json::Object(_) => points.push(TuneLabPoint::new(
                    require_f64(item, "pos")?,
                    require_f64(item, "value")?,
                )),
                _ => return Err(validation("invalid point")),
            }
        }
        Ok(points)
    } else {
        if items.len() % 2 != 0 {
            return Err(validation("points array must have an even length"));
        }
        let mut points = Vec::with_capacity(items.len() / 2);
        for pair in items.chunks(2) {
            points.push(TuneLabPoint::new(
                f64_of(&pair[0], "pos")?,
                f64_of(&pair[1], "value")?,
            ));
        }
        Ok(points)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TuneLabTempo {
    pub pos: f64,
    pub bpm: f64,
}

impl TuneLabTempo {
    pub fn new(pos: f64, bpm: f64) -> Self {
        Self { pos, bpm }
    }

    fn to_json(self) -> Json {
        Json::object([
            ("pos", Json::Float(self.pos)),
            ("bpm", Json::Float(self.bpm)),
        ])
    }

    fn from_json(value: &Json) -> Result<Self> {
        Ok(Self {
            pos: require_f64(value, "pos")?,
            bpm: require_f64(value, "bpm")?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TuneLabTimeSignature {
    pub numerator: i64,
    pub denominator: i64,
    pub bar_index: i64,
}

impl TuneLabTimeSignature {
    pub fn new(numerator: i64, denominator: i64, bar_index: i64) -> Self {
        Self {
            numerator,
            denominator,
            bar_index,
        }
    }

    fn to_json(self) -> Json {
        Json::object([
            ("numerator", Json::Int(self.numerator)),
            ("denominator", Json::Int(self.denominator)),
            ("barIndex", Json::Int(self.bar_index)),
        ])
    }

    fn from_json(value: &Json) -> Result<Self> {
        Ok(Self {
            numerator: require_i64(value, "numerator")?,
            denominator: require_i64(value, "denominator")?,
            bar_index: require_i64(value, "barIndex")?,
        })
    }
}

// ---------------------------------------------------------------------------
// Voice / automation / phoneme / note / vibrato
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct TuneLabVoice {
    pub type_: String,
    pub id_: String,
    pub kind: Option<String>,
}

impl TuneLabVoice {
    fn to_json(&self) -> Json {
        let mut entries = vec![
            ("type".to_owned(), Json::Str(self.type_.clone())),
            ("id".to_owned(), Json::Str(self.id_.clone())),
        ];
        push_field(
            &mut entries,
            "kind",
            self.kind.clone().map(Json::Str),
        );
        Json::Object(entries)
    }

    fn from_json(value: &Json) -> Result<Self> {
        Ok(Self {
            type_: opt_str(value, "type", "")?,
            id_: opt_str(value, "id", "")?,
            kind: opt_string(value, "kind")?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct TuneLabAutomation {
    pub default: f64,
    pub values: Vec<TuneLabPoint>,
}

impl TuneLabAutomation {
    fn to_json(&self) -> Json {
        Json::object([
            ("default", Json::Float(self.default)),
            ("values", points_to_json(&self.values)),
        ])
    }

    fn from_json(value: &Json) -> Result<Self> {
        Ok(Self {
            default: require_f64(value, "default")?,
            values: match field(value, "values") {
                None | Some(Json::Null) => Vec::new(),
                Some(values) => points_from_json(values)?,
            },
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct TuneLabPhoneme {
    pub start_time: Option<f64>,
    pub end_time: Option<f64>,
    pub symbol: String,
    pub duration: Option<f64>,
    pub stretch_weight: Option<f64>,
    pub properties: Option<Json>,
}

impl TuneLabPhoneme {
    fn to_json(&self) -> Json {
        let mut entries: Vec<(String, Json)> = Vec::new();
        push_field(
            &mut entries,
            "startTime",
            self.start_time.map(Json::Float),
        );
        push_field(&mut entries, "endTime", self.end_time.map(Json::Float));
        entries.push(("symbol".to_owned(), Json::Str(self.symbol.clone())));
        push_field(&mut entries, "duration", self.duration.map(Json::Float));
        push_field(
            &mut entries,
            "stretchWeight",
            self.stretch_weight.map(Json::Float),
        );
        push_field(&mut entries, "properties", self.properties.clone());
        Json::Object(entries)
    }

    fn from_json(value: &Json) -> Result<Self> {
        Ok(Self {
            start_time: opt_f64_opt(value, "startTime")?,
            end_time: opt_f64_opt(value, "endTime")?,
            symbol: require_str(value, "symbol")?,
            duration: opt_f64_opt(value, "duration")?,
            stretch_weight: opt_f64_opt(value, "stretchWeight")?,
            properties: field(value, "properties").cloned(),
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct TuneLabNote {
    pub pos: f64,
    pub dur: f64,
    pub pitch: i64,
    pub lyric: String,
    pub pronunciation: Option<String>,
    pub phonemes: Vec<TuneLabPhoneme>,
    pub body_offset: Option<f64>,
    pub leading_phonemes: Option<Vec<TuneLabPhoneme>>,
    pub body_phonemes: Option<Vec<TuneLabPhoneme>>,
    pub properties: Json,
}

impl TuneLabNote {
    pub fn new(pos: f64, dur: f64, pitch: i64, lyric: impl Into<String>) -> Self {
        Self {
            pos,
            dur,
            pitch,
            lyric: lyric.into(),
            pronunciation: None,
            phonemes: Vec::new(),
            body_offset: None,
            leading_phonemes: None,
            body_phonemes: None,
            properties: Json::empty_object(),
        }
    }

    fn to_json(&self) -> Json {
        let mut entries: Vec<(String, Json)> = vec![
            ("pos".to_owned(), Json::Float(self.pos)),
            ("dur".to_owned(), Json::Float(self.dur)),
            ("pitch".to_owned(), Json::Int(self.pitch)),
            ("lyric".to_owned(), Json::Str(self.lyric.clone())),
        ];
        push_field(
            &mut entries,
            "pronunciation",
            self.pronunciation.clone().map(Json::Str),
        );
        entries.push((
            "phonemes".to_owned(),
            Json::Array(self.phonemes.iter().map(TuneLabPhoneme::to_json).collect()),
        ));
        push_field(
            &mut entries,
            "bodyOffset",
            self.body_offset.map(Json::Float),
        );
        push_field(
            &mut entries,
            "leadingPhonemes",
            self.leading_phonemes.as_ref().map(|list| {
                Json::Array(list.iter().map(TuneLabPhoneme::to_json).collect())
            }),
        );
        push_field(
            &mut entries,
            "bodyPhonemes",
            self.body_phonemes.as_ref().map(|list| {
                Json::Array(list.iter().map(TuneLabPhoneme::to_json).collect())
            }),
        );
        entries.push(("properties".to_owned(), self.properties.clone()));
        Json::Object(entries)
    }

    fn from_json(value: &Json) -> Result<Self> {
        let mut phonemes = Vec::new();
        for item in array_of(value, "phonemes")? {
            phonemes.push(TuneLabPhoneme::from_json(item)?);
        }
        let list_of = |key: &str| -> Result<Option<Vec<TuneLabPhoneme>>> {
            match field(value, key) {
                None | Some(Json::Null) => Ok(None),
                Some(Json::Array(items)) => {
                    let mut list = Vec::new();
                    for item in items {
                        list.push(TuneLabPhoneme::from_json(item)?);
                    }
                    Ok(Some(list))
                }
                Some(_) => Err(validation(format!("field `{key}` must be an array"))),
            }
        };
        Ok(Self {
            pos: require_f64(value, "pos")?,
            dur: require_f64(value, "dur")?,
            pitch: require_i64(value, "pitch")?,
            lyric: require_str(value, "lyric")?,
            pronunciation: opt_string(value, "pronunciation")?,
            phonemes,
            body_offset: opt_f64_opt(value, "bodyOffset")?,
            leading_phonemes: list_of("leadingPhonemes")?,
            body_phonemes: list_of("bodyPhonemes")?,
            properties: opt_object(value, "properties"),
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct TuneLabVibrato {
    pub pos: f64,
    pub dur: f64,
    pub frequency: f64,
    pub amplitude: f64,
    pub phase: f64,
    pub attack: f64,
    pub release: f64,
    pub affected_automations: Vec<(String, f64)>,
}

impl TuneLabVibrato {
    fn to_json(&self) -> Json {
        Json::object([
            ("pos", Json::Float(self.pos)),
            ("dur", Json::Float(self.dur)),
            ("frequency", Json::Float(self.frequency)),
            ("amplitude", Json::Float(self.amplitude)),
            ("phase", Json::Float(self.phase)),
            ("attack", Json::Float(self.attack)),
            ("release", Json::Float(self.release)),
            (
                "affectedAutomations",
                Json::Object(
                    self.affected_automations
                        .iter()
                        .map(|(key, value)| (key.clone(), Json::Float(*value)))
                        .collect(),
                ),
            ),
        ])
    }

    fn from_json(value: &Json) -> Result<Self> {
        let mut affected_automations = Vec::new();
        if let Some(Json::Object(entries)) = field(value, "affectedAutomations") {
            for (key, item) in entries {
                affected_automations.push((key.clone(), f64_of(item, "affectedAutomations")?));
            }
        }
        Ok(Self {
            pos: require_f64(value, "pos")?,
            dur: require_f64(value, "dur")?,
            frequency: require_f64(value, "frequency")?,
            amplitude: require_f64(value, "amplitude")?,
            phase: require_f64(value, "phase")?,
            attack: require_f64(value, "attack")?,
            release: require_f64(value, "release")?,
            affected_automations,
        })
    }
}

// ---------------------------------------------------------------------------
// Parts / tracks / project
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct TuneLabBasePart {
    pub name: String,
    pub pos: f64,
    pub start_offset: f64,
    pub end_offset: f64,
}

impl TuneLabBasePart {
    /// `TuneLabBasePart.dur`.
    pub fn dur(&self) -> f64 {
        self.end_offset - self.start_offset
    }

    fn from_json(value: &Json) -> Result<Self> {
        let start_offset = match field(value, "startOffset") {
            Some(item) => f64_of(item, "startOffset")?,
            None if field(value, "dur").is_some() => 0.0,
            None => return Err(validation("missing field `startOffset`")),
        };
        let end_offset = match field(value, "endOffset") {
            Some(item) => f64_of(item, "endOffset")?,
            None => match field(value, "dur") {
                Some(item) => f64_of(item, "dur")?,
                None => return Err(validation("missing field `endOffset`")),
            },
        };
        Ok(Self {
            name: require_str(value, "name")?,
            pos: require_f64(value, "pos")?,
            start_offset,
            end_offset,
        })
    }
}

#[derive(Clone, Debug)]
pub struct TuneLabMidiPart {
    pub base: TuneLabBasePart,
    pub gain: Option<f64>,
    pub voice: TuneLabVoice,
    pub properties: Json,
    pub notes: Vec<TuneLabNote>,
    pub automations: Vec<(String, TuneLabAutomation)>,
    pub pitch: Vec<Vec<TuneLabPoint>>,
    pub vibratos: Vec<TuneLabVibrato>,
}

impl TuneLabMidiPart {
    pub fn new(name: impl Into<String>, pos: f64, start_offset: f64, end_offset: f64) -> Self {
        Self {
            base: TuneLabBasePart {
                name: name.into(),
                pos,
                start_offset,
                end_offset,
            },
            gain: Some(0.0),
            voice: TuneLabVoice::default(),
            properties: Json::empty_object(),
            notes: Vec::new(),
            automations: Vec::new(),
            pitch: Vec::new(),
            vibratos: Vec::new(),
        }
    }

    fn to_json(&self) -> Json {
        let mut entries: Vec<(String, Json)> = vec![
            ("name".to_owned(), Json::Str(self.base.name.clone())),
            ("pos".to_owned(), Json::Float(self.base.pos)),
            ("startOffset".to_owned(), Json::Float(self.base.start_offset)),
            ("endOffset".to_owned(), Json::Float(self.base.end_offset)),
            ("type".to_owned(), Json::Str("midi".to_owned())),
        ];
        push_field(&mut entries, "gain", self.gain.map(Json::Float));
        entries.push(("voice".to_owned(), self.voice.to_json()));
        entries.push(("properties".to_owned(), self.properties.clone()));
        entries.push((
            "notes".to_owned(),
            Json::Array(self.notes.iter().map(TuneLabNote::to_json).collect()),
        ));
        entries.push((
            "automations".to_owned(),
            Json::Object(
                self.automations
                    .iter()
                    .map(|(key, value)| (key.clone(), value.to_json()))
                    .collect(),
            ),
        ));
        entries.push((
            "pitch".to_owned(),
            Json::Array(
                self.pitch
                    .iter()
                    .map(|points| points_to_json(points))
                    .collect(),
            ),
        ));
        entries.push((
            "vibratos".to_owned(),
            Json::Array(self.vibratos.iter().map(TuneLabVibrato::to_json).collect()),
        ));
        Json::Object(entries)
    }

    fn from_json(value: &Json) -> Result<Self> {
        let gain = match field(value, "gain") {
            None => Some(0.0),
            Some(Json::Null) => None,
            Some(item) => Some(f64_of(item, "gain")?),
        };
        let voice = match field(value, "voice") {
            None | Some(Json::Null) => TuneLabVoice::default(),
            Some(item) => TuneLabVoice::from_json(item)?,
        };
        let mut notes = Vec::new();
        for item in array_of(value, "notes")? {
            notes.push(TuneLabNote::from_json(item)?);
        }
        let mut automations = Vec::new();
        if let Some(Json::Object(entries)) = field(value, "automations") {
            for (key, item) in entries {
                automations.push((key.clone(), TuneLabAutomation::from_json(item)?));
            }
        }
        let mut pitch = Vec::new();
        for item in array_of(value, "pitch")? {
            pitch.push(points_from_json(item)?);
        }
        let mut vibratos = Vec::new();
        for item in array_of(value, "vibratos")? {
            vibratos.push(TuneLabVibrato::from_json(item)?);
        }
        Ok(Self {
            base: TuneLabBasePart::from_json(value)?,
            gain,
            voice,
            properties: opt_object(value, "properties"),
            notes,
            automations,
            pitch,
            vibratos,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct TuneLabAudioPart {
    pub base: TuneLabBasePart,
    pub path: String,
}

impl TuneLabAudioPart {
    pub fn new(
        name: impl Into<String>,
        pos: f64,
        start_offset: f64,
        end_offset: f64,
        path: impl Into<String>,
    ) -> Self {
        Self {
            base: TuneLabBasePart {
                name: name.into(),
                pos,
                start_offset,
                end_offset,
            },
            path: path.into(),
        }
    }

    fn to_json(&self) -> Json {
        Json::object([
            ("name", Json::Str(self.base.name.clone())),
            ("pos", Json::Float(self.base.pos)),
            ("startOffset", Json::Float(self.base.start_offset)),
            ("endOffset", Json::Float(self.base.end_offset)),
            ("type", Json::Str("audio".to_owned())),
            ("path", Json::Str(self.path.clone())),
        ])
    }

    fn from_json(value: &Json) -> Result<Self> {
        Ok(Self {
            base: TuneLabBasePart::from_json(value)?,
            path: require_str(value, "path")?,
        })
    }
}

#[derive(Clone, Debug)]
pub enum TuneLabPart {
    Midi(TuneLabMidiPart),
    Audio(TuneLabAudioPart),
}

impl TuneLabPart {
    pub fn base(&self) -> &TuneLabBasePart {
        match self {
            TuneLabPart::Midi(part) => &part.base,
            TuneLabPart::Audio(part) => &part.base,
        }
    }

    pub fn to_json(&self) -> Json {
        match self {
            TuneLabPart::Midi(part) => part.to_json(),
            TuneLabPart::Audio(part) => part.to_json(),
        }
    }

    pub fn from_json(value: &Json) -> Result<Self> {
        let kind = opt_str(value, "type", "midi")?;
        if kind == "audio" {
            Ok(TuneLabPart::Audio(TuneLabAudioPart::from_json(value)?))
        } else {
            Ok(TuneLabPart::Midi(TuneLabMidiPart::from_json(value)?))
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct TuneLabTrack {
    pub name: String,
    pub gain: f64,
    pub pan: f64,
    pub mute: bool,
    pub solo: bool,
    pub color: String,
    pub as_refer: Option<bool>,
    pub parts: Vec<TuneLabPart>,
    pub export_enabled: bool,
    pub export_channels: i64,
}

impl TuneLabTrack {
    pub fn new(
        name: impl Into<String>,
        gain: f64,
        pan: f64,
        mute: bool,
        solo: bool,
        parts: Vec<TuneLabPart>,
    ) -> Self {
        Self {
            name: name.into(),
            gain,
            pan,
            mute,
            solo,
            color: String::new(),
            as_refer: Some(false),
            parts,
            export_enabled: false,
            export_channels: 1,
        }
    }

    fn to_json(&self) -> Json {
        Json::object([
            ("name", Json::Str(self.name.clone())),
            ("gain", Json::Float(self.gain)),
            ("pan", Json::Float(self.pan)),
            ("mute", Json::Bool(self.mute)),
            ("solo", Json::Bool(self.solo)),
            ("color", Json::Str(self.color.clone())),
            (
                "asRefer",
                match self.as_refer {
                    Some(value) => Json::Bool(value),
                    None => Json::Null,
                },
            ),
            (
                "parts",
                Json::Array(self.parts.iter().map(TuneLabPart::to_json).collect()),
            ),
            ("exportEnabled", Json::Bool(self.export_enabled)),
            ("exportChannels", Json::Int(self.export_channels)),
        ])
    }

    fn from_json(value: &Json) -> Result<Self> {
        let as_refer = match field(value, "asRefer") {
            None => Some(false),
            Some(Json::Null) => None,
            Some(item) => Some(bool_of(item, "asRefer")?),
        };
        let mut parts = Vec::new();
        for item in array_of(value, "parts")? {
            parts.push(TuneLabPart::from_json(item)?);
        }
        Ok(Self {
            name: require_str(value, "name")?,
            gain: require_f64(value, "gain")?,
            pan: require_f64(value, "pan")?,
            mute: require_bool(value, "mute")?,
            solo: require_bool(value, "solo")?,
            color: opt_str(value, "color", "")?,
            as_refer,
            parts,
            export_enabled: opt_bool(value, "exportEnabled", false)?,
            export_channels: opt_i64(value, "exportChannels", 1)?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct TuneLabEditorInfo {
    pub playhead_pos: f64,
}

impl Default for TuneLabEditorInfo {
    fn default() -> Self {
        Self { playhead_pos: 0.0 }
    }
}

impl TuneLabEditorInfo {
    fn to_json(&self) -> Json {
        Json::object([("playheadPos", Json::Float(self.playhead_pos))])
    }

    fn from_json(value: &Json) -> Result<Self> {
        Ok(Self {
            playhead_pos: opt_f64(value, "playheadPos", 0.0)?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct TuneLabExportConfig {
    pub export_path: String,
    pub file_name: String,
    pub sample_rate: i64,
    pub bit_depth: i64,
    pub format: Option<String>,
    pub bitrate: Option<i64>,
    pub master_export_channels: i64,
    pub master_export_enabled: bool,
}

impl Default for TuneLabExportConfig {
    fn default() -> Self {
        Self {
            export_path: String::new(),
            file_name: String::new(),
            sample_rate: 44100,
            bit_depth: 16,
            format: None,
            bitrate: None,
            master_export_channels: 2,
            master_export_enabled: true,
        }
    }
}

impl TuneLabExportConfig {
    fn to_json(&self) -> Json {
        let mut entries: Vec<(String, Json)> = vec![
            ("exportPath".to_owned(), Json::Str(self.export_path.clone())),
            ("fileName".to_owned(), Json::Str(self.file_name.clone())),
            ("sampleRate".to_owned(), Json::Int(self.sample_rate)),
            ("bitDepth".to_owned(), Json::Int(self.bit_depth)),
        ];
        push_field(&mut entries, "format", self.format.clone().map(Json::Str));
        push_field(&mut entries, "bitrate", self.bitrate.map(Json::Int));
        entries.push((
            "masterExportChannels".to_owned(),
            Json::Int(self.master_export_channels),
        ));
        entries.push((
            "masterExportEnabled".to_owned(),
            Json::Bool(self.master_export_enabled),
        ));
        Json::Object(entries)
    }

    fn from_json(value: &Json) -> Result<Self> {
        Ok(Self {
            export_path: opt_str(value, "exportPath", "")?,
            file_name: opt_str(value, "fileName", "")?,
            sample_rate: opt_i64(value, "sampleRate", 44100)?,
            bit_depth: opt_i64(value, "bitDepth", 16)?,
            format: opt_string(value, "format")?,
            bitrate: match field(value, "bitrate") {
                None | Some(Json::Null) => None,
                Some(item) => Some(i64_of(item, "bitrate")?),
            },
            master_export_channels: opt_i64(value, "masterExportChannels", 2)?,
            master_export_enabled: opt_bool(value, "masterExportEnabled", true)?,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct TuneLabProject {
    pub version: i64,
    pub tempos: Vec<TuneLabTempo>,
    pub time_signatures: Vec<TuneLabTimeSignature>,
    pub tracks: Vec<TuneLabTrack>,
    pub editor_info: TuneLabEditorInfo,
    pub export_config: TuneLabExportConfig,
    /// `model_config = ConfigDict(extra="allow")` keeps unknown top-level keys;
    /// pydantic emits them after the declared fields.
    pub extras: Vec<(String, Json)>,
}

/// `TuneLabProject.serialize_project` for `version == 0`: replace the start/end
/// offsets with a single duration, appended at the end of the object (the order
/// `dict.pop` + assignment produces in Python).
fn legacy_part(part: &mut Json) {
    if let Json::Object(entries) = part {
        let start = entries
            .iter()
            .position(|(key, _)| key == "startOffset")
            .map(|index| entries.remove(index).1);
        let end = entries
            .iter()
            .position(|(key, _)| key == "endOffset")
            .map(|index| entries.remove(index).1);
        if let (Some(start), Some(end)) = (start, end) {
            let duration = end.as_f64().unwrap_or(0.0) - start.as_f64().unwrap_or(0.0);
            entries.push(("dur".to_owned(), Json::Float(duration)));
        }
    }
}

impl TuneLabProject {
    pub fn to_json(&self) -> Json {
        let tracks: Vec<Json> = self
            .tracks
            .iter()
            .map(|track| {
                let mut value = TuneLabTrack::to_json(track);
                if self.version == 0 {
                    if let Json::Object(entries) = &mut value {
                        if let Some((_, Json::Array(parts))) =
                            entries.iter_mut().find(|(key, _)| key == "parts")
                        {
                            for part in parts {
                                legacy_part(part);
                            }
                        }
                    }
                }
                value
            })
            .collect();
        let mut entries: Vec<(String, Json)> = vec![
            ("version".to_owned(), Json::Int(self.version)),
            (
                "tempos".to_owned(),
                Json::Array(self.tempos.iter().map(|tempo| tempo.to_json()).collect()),
            ),
            (
                "timeSignatures".to_owned(),
                Json::Array(
                    self.time_signatures
                        .iter()
                        .map(|signature| signature.to_json())
                        .collect(),
                ),
            ),
            ("tracks".to_owned(), Json::Array(tracks)),
            ("editorInfo".to_owned(), self.editor_info.to_json()),
            ("exportConfig".to_owned(), self.export_config.to_json()),
        ];
        for (key, value) in &self.extras {
            if !value.is_null() {
                entries.push((key.clone(), value.clone()));
            }
        }
        Json::Object(entries)
    }

    pub fn from_json(value: &Json) -> Result<Self> {
        let mut tempos = Vec::new();
        for item in array_of(value, "tempos")? {
            tempos.push(TuneLabTempo::from_json(item)?);
        }
        let mut time_signatures = Vec::new();
        for item in array_of(value, "timeSignatures")? {
            time_signatures.push(TuneLabTimeSignature::from_json(item)?);
        }
        let mut tracks = Vec::new();
        for item in array_of(value, "tracks")? {
            tracks.push(TuneLabTrack::from_json(item)?);
        }
        let editor_info = match field(value, "editorInfo") {
            None | Some(Json::Null) => TuneLabEditorInfo::default(),
            Some(item) => TuneLabEditorInfo::from_json(item)?,
        };
        let export_config = match field(value, "exportConfig") {
            None | Some(Json::Null) => TuneLabExportConfig::default(),
            Some(item) => TuneLabExportConfig::from_json(item)?,
        };
        let known = [
            "version",
            "tempos",
            "timeSignatures",
            "tracks",
            "editorInfo",
            "exportConfig",
        ];
        let mut extras = Vec::new();
        if let Json::Object(entries) = value {
            for (key, item) in entries {
                if !known.contains(&key.as_str()) {
                    extras.push((key.clone(), item.clone()));
                }
            }
        }
        Ok(Self {
            version: opt_i64(value, "version", 0)?,
            tempos,
            time_signatures,
            tracks,
            editor_info,
            export_config,
            extras,
        })
    }
}

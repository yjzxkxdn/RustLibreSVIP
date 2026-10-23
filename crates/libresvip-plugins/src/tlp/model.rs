//! Port of `libresvip.plugins.tlp.model` (the TuneLab legacy project model).

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

fn float_json(value: f64) -> Json {
    Json::Float(value)
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
        Json::object([("pos", float_json(self.pos)), ("bpm", float_json(self.bpm))])
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
}

impl TuneLabVoice {
    fn to_json(&self) -> Json {
        Json::object([
            ("type", Json::Str(self.type_.clone())),
            ("id", Json::Str(self.id_.clone())),
        ])
    }

    fn from_json(value: &Json) -> Result<Self> {
        Ok(Self {
            type_: opt_str(value, "type", "")?,
            id_: opt_str(value, "id", "")?,
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
    pub start_time: f64,
    pub end_time: f64,
    pub symbol: String,
}

impl TuneLabPhoneme {
    fn to_json(&self) -> Json {
        Json::object([
            ("startTime", Json::Float(self.start_time)),
            ("endTime", Json::Float(self.end_time)),
            ("symbol", Json::Str(self.symbol.clone())),
        ])
    }

    fn from_json(value: &Json) -> Result<Self> {
        Ok(Self {
            start_time: require_f64(value, "startTime")?,
            end_time: require_f64(value, "endTime")?,
            symbol: require_str(value, "symbol")?,
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
            properties: Json::empty_object(),
        }
    }

    fn to_json(&self) -> Json {
        Json::object([
            ("pos", Json::Float(self.pos)),
            ("dur", Json::Float(self.dur)),
            ("pitch", Json::Int(self.pitch)),
            ("lyric", Json::Str(self.lyric.clone())),
            (
                "pronunciation",
                match &self.pronunciation {
                    Some(value) => Json::Str(value.clone()),
                    None => Json::Null,
                },
            ),
            (
                "phonemes",
                Json::Array(self.phonemes.iter().map(TuneLabPhoneme::to_json).collect()),
            ),
            ("properties", self.properties.clone()),
        ])
    }

    fn from_json(value: &Json) -> Result<Self> {
        let pronunciation = match field(value, "pronunciation") {
            None | Some(Json::Null) => None,
            Some(item) => Some(str_of(item, "pronunciation")?),
        };
        let mut phonemes = Vec::new();
        for item in array_of(value, "phonemes")? {
            phonemes.push(TuneLabPhoneme::from_json(item)?);
        }
        Ok(Self {
            pos: require_f64(value, "pos")?,
            dur: require_f64(value, "dur")?,
            pitch: require_i64(value, "pitch")?,
            lyric: require_str(value, "lyric")?,
            pronunciation,
            phonemes,
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
    pub dur: f64,
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
    pub fn new(name: impl Into<String>, pos: f64, dur: f64) -> Self {
        Self {
            base: TuneLabBasePart {
                name: name.into(),
                pos,
                dur,
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
        Json::object([
            ("name", Json::Str(self.base.name.clone())),
            ("pos", Json::Float(self.base.pos)),
            ("dur", Json::Float(self.base.dur)),
            ("type", Json::Str("midi".to_owned())),
            (
                "gain",
                match self.gain {
                    Some(value) => Json::Float(value),
                    None => Json::Null,
                },
            ),
            ("voice", self.voice.to_json()),
            ("properties", self.properties.clone()),
            (
                "notes",
                Json::Array(self.notes.iter().map(TuneLabNote::to_json).collect()),
            ),
            (
                "automations",
                Json::Object(
                    self.automations
                        .iter()
                        .map(|(key, value)| (key.clone(), value.to_json()))
                        .collect(),
                ),
            ),
            (
                "pitch",
                Json::Array(
                    self.pitch
                        .iter()
                        .map(|points| points_to_json(points))
                        .collect(),
                ),
            ),
            (
                "vibratos",
                Json::Array(self.vibratos.iter().map(TuneLabVibrato::to_json).collect()),
            ),
        ])
    }

    fn from_json(value: &Json) -> Result<Self> {
        let base = TuneLabBasePart {
            name: require_str(value, "name")?,
            pos: require_f64(value, "pos")?,
            dur: require_f64(value, "dur")?,
        };
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
            base,
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
    pub fn new(name: impl Into<String>, pos: f64, dur: f64, path: impl Into<String>) -> Self {
        Self {
            base: TuneLabBasePart {
                name: name.into(),
                pos,
                dur,
            },
            path: path.into(),
        }
    }

    fn to_json(&self) -> Json {
        Json::object([
            ("name", Json::Str(self.base.name.clone())),
            ("pos", Json::Float(self.base.pos)),
            ("dur", Json::Float(self.base.dur)),
            ("type", Json::Str("audio".to_owned())),
            ("path", Json::Str(self.path.clone())),
        ])
    }

    fn from_json(value: &Json) -> Result<Self> {
        Ok(Self {
            base: TuneLabBasePart {
                name: require_str(value, "name")?,
                pos: require_f64(value, "pos")?,
                dur: require_f64(value, "dur")?,
            },
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
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct TuneLabProject {
    pub version: i64,
    pub tempos: Vec<TuneLabTempo>,
    pub time_signatures: Vec<TuneLabTimeSignature>,
    pub tracks: Vec<TuneLabTrack>,
}

impl TuneLabProject {
    pub fn to_json(&self) -> Json {
        Json::object([
            ("version", Json::Int(self.version)),
            (
                "tempos",
                Json::Array(self.tempos.iter().map(|tempo| tempo.to_json()).collect()),
            ),
            (
                "timeSignatures",
                Json::Array(
                    self.time_signatures
                        .iter()
                        .map(|signature| signature.to_json())
                        .collect(),
                ),
            ),
            (
                "tracks",
                Json::Array(self.tracks.iter().map(TuneLabTrack::to_json).collect()),
            ),
        ])
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
        Ok(Self {
            version: opt_i64(value, "version", 0)?,
            tempos,
            time_signatures,
            tracks,
        })
    }
}

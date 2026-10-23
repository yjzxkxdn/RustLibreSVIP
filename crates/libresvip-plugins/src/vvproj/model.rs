//! Port of `libresvip.plugins.vvproj.model` (the VOICEVOX project model).

use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::utils::text::uuid_str;

use super::pyjson::Json;

fn validation(message: impl Into<String>) -> LibreSvipError {
    LibreSvipError::Validation(message.into())
}

fn field<'a>(value: &'a Json, key: &str) -> Option<&'a Json> {
    value.get(key)
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

fn opt_i64(value: &Json, key: &str, default: i64) -> Result<i64> {
    match field(value, key) {
        None | Some(Json::Null) => Ok(default),
        Some(item) => i64_of(item, key),
    }
}

fn opt_f64(value: &Json, key: &str, default: f64) -> Result<f64> {
    match field(value, key) {
        None | Some(Json::Null) => Ok(default),
        Some(item) => f64_of(item, key),
    }
}

fn opt_bool(value: &Json, key: &str, default: bool) -> Result<bool> {
    match field(value, key) {
        None | Some(Json::Null) => Ok(default),
        Some(item) => item
            .as_bool()
            .ok_or_else(|| validation(format!("field `{key}` must be a boolean"))),
    }
}

fn opt_str(value: &Json, key: &str, default: &str) -> Result<String> {
    match field(value, key) {
        None | Some(Json::Null) => Ok(default.to_owned()),
        Some(item) => str_of(item, key),
    }
}

fn array_of<'a>(value: &'a Json, key: &str) -> Result<&'a [Json]> {
    match field(value, key) {
        None | Some(Json::Null) => Ok(&[]),
        Some(Json::Array(items)) => Ok(items),
        Some(_) => Err(validation(format!("field `{key}` must be an array"))),
    }
}

// ---------------------------------------------------------------------------
// Singers / voices
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct VoiceVoxSinger {
    pub engine_id: String,
    pub style_id: i64,
}

impl Default for VoiceVoxSinger {
    fn default() -> Self {
        Self {
            engine_id: "074fc39e-678b-4c13-8916-ffca8d505d1d".to_owned(),
            style_id: 3002,
        }
    }
}

impl VoiceVoxSinger {
    fn to_json(&self) -> Json {
        Json::object([
            ("engineId", Json::Str(self.engine_id.clone())),
            ("styleId", Json::Int(self.style_id)),
        ])
    }

    fn from_json(value: &Json) -> Result<Self> {
        let default = Self::default();
        Ok(Self {
            engine_id: opt_str(value, "engineId", &default.engine_id)?,
            style_id: opt_i64(value, "styleId", default.style_id)?,
        })
    }
}

// ---------------------------------------------------------------------------
// Song data
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct VoiceVoxNote {
    pub id: String,
    pub position: i64,
    pub duration: i64,
    pub note_number: i64,
    pub lyric: String,
}

impl VoiceVoxNote {
    pub fn new(position: i64, duration: i64, note_number: i64, lyric: impl Into<String>) -> Self {
        Self {
            id: uuid_str(),
            position,
            duration,
            note_number,
            lyric: lyric.into(),
        }
    }

    fn to_json(&self) -> Json {
        Json::object([
            ("id", Json::Str(self.id.clone())),
            ("position", Json::Int(self.position)),
            ("duration", Json::Int(self.duration)),
            ("noteNumber", Json::Int(self.note_number)),
            ("lyric", Json::Str(self.lyric.clone())),
        ])
    }

    fn from_json(value: &Json) -> Result<Self> {
        Ok(Self {
            id: opt_str(value, "id", "")?,
            position: opt_i64(value, "position", 0)?,
            duration: opt_i64(value, "duration", 0)?,
            note_number: opt_i64(value, "noteNumber", 0)?,
            lyric: opt_str(value, "lyric", "")?,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct VoiceVoxTrack {
    pub name: String,
    pub singer: VoiceVoxSinger,
    pub key_range_adjustment: i64,
    pub volume_range_adjustment: i64,
    pub notes: Vec<VoiceVoxNote>,
    pub pitch_edit_data: Vec<f64>,
    pub solo: bool,
    pub mute: bool,
    pub gain: f64,
    pub pan: f64,
}

impl VoiceVoxTrack {
    fn to_json(&self) -> Json {
        Json::object([
            ("name", Json::Str(self.name.clone())),
            ("singer", self.singer.to_json()),
            ("keyRangeAdjustment", Json::Int(self.key_range_adjustment)),
            (
                "volumeRangeAdjustment",
                Json::Int(self.volume_range_adjustment),
            ),
            (
                "notes",
                Json::Array(self.notes.iter().map(VoiceVoxNote::to_json).collect()),
            ),
            (
                "pitchEditData",
                Json::Array(self.pitch_edit_data.iter().copied().map(Json::Float).collect()),
            ),
            ("solo", Json::Bool(self.solo)),
            ("mute", Json::Bool(self.mute)),
            ("gain", Json::Float(self.gain)),
            ("pan", Json::Float(self.pan)),
        ])
    }

    pub fn from_json(value: &Json) -> Result<Self> {
        let singer = match field(value, "singer") {
            None | Some(Json::Null) => VoiceVoxSinger::default(),
            Some(item) => VoiceVoxSinger::from_json(item)?,
        };
        let mut notes = Vec::new();
        for item in array_of(value, "notes")? {
            notes.push(VoiceVoxNote::from_json(item)?);
        }
        let mut pitch_edit_data = Vec::new();
        for item in array_of(value, "pitchEditData")? {
            pitch_edit_data.push(f64_of(item, "pitchEditData")?);
        }
        Ok(Self {
            name: opt_str(value, "name", "")?,
            singer,
            key_range_adjustment: opt_i64(value, "keyRangeAdjustment", 0)?,
            volume_range_adjustment: opt_i64(value, "volumeRangeAdjustment", 0)?,
            notes,
            pitch_edit_data,
            solo: opt_bool(value, "solo", false)?,
            mute: opt_bool(value, "mute", false)?,
            gain: opt_f64(value, "gain", 1.0)?,
            pan: opt_f64(value, "pan", 0.0)?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct VoiceVoxTempo {
    pub position: i64,
    pub bpm: i64,
}

impl VoiceVoxTempo {
    pub fn new(position: i64, bpm: i64) -> Self {
        Self { position, bpm }
    }

    fn to_json(&self) -> Json {
        Json::object([
            ("position", Json::Int(self.position)),
            ("bpm", Json::Int(self.bpm)),
        ])
    }

    fn from_json(value: &Json) -> Result<Self> {
        Ok(Self {
            position: opt_i64(value, "position", 0)?,
            bpm: opt_i64(value, "bpm", 0)?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct VoiceVoxTimeSignature {
    pub measure_number: i64,
    pub beats: i64,
    pub beat_type: i64,
}

impl VoiceVoxTimeSignature {
    pub fn new(measure_number: i64, beats: i64, beat_type: i64) -> Self {
        Self {
            measure_number,
            beats,
            beat_type,
        }
    }

    fn to_json(&self) -> Json {
        Json::object([
            ("measureNumber", Json::Int(self.measure_number)),
            ("beats", Json::Int(self.beats)),
            ("beatType", Json::Int(self.beat_type)),
        ])
    }

    fn from_json(value: &Json) -> Result<Self> {
        Ok(Self {
            measure_number: opt_i64(value, "measureNumber", 0)?,
            beats: opt_i64(value, "beats", 0)?,
            beat_type: opt_i64(value, "beatType", 0)?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct VoiceVoxSong {
    pub tpqn: i64,
    pub tempos: Vec<VoiceVoxTempo>,
    pub time_signatures: Vec<VoiceVoxTimeSignature>,
    pub tracks: Vec<(String, VoiceVoxTrack)>,
    pub track_order: Vec<String>,
}

impl Default for VoiceVoxSong {
    fn default() -> Self {
        Self {
            tpqn: libresvip_core::core::constants::TICKS_IN_BEAT,
            tempos: Vec::new(),
            time_signatures: Vec::new(),
            tracks: Vec::new(),
            track_order: Vec::new(),
        }
    }
}

impl VoiceVoxSong {
    pub fn track(&self, key: &str) -> Option<&VoiceVoxTrack> {
        self.tracks
            .iter()
            .find(|(track_key, _)| track_key == key)
            .map(|(_, track)| track)
    }

    pub fn to_json(&self) -> Json {
        Json::object([
            ("tpqn", Json::Int(self.tpqn)),
            (
                "tempos",
                Json::Array(self.tempos.iter().map(VoiceVoxTempo::to_json).collect()),
            ),
            (
                "timeSignatures",
                Json::Array(
                    self.time_signatures
                        .iter()
                        .map(VoiceVoxTimeSignature::to_json)
                        .collect(),
                ),
            ),
            (
                "tracks",
                Json::Object(
                    self.tracks
                        .iter()
                        .map(|(key, track)| (key.clone(), track.to_json()))
                        .collect(),
                ),
            ),
            (
                "trackOrder",
                Json::Array(self.track_order.iter().cloned().map(Json::Str).collect()),
            ),
        ])
    }

    fn from_json(value: &Json) -> Result<Self> {
        let mut tempos = Vec::new();
        for item in array_of(value, "tempos")? {
            tempos.push(VoiceVoxTempo::from_json(item)?);
        }
        let mut time_signatures = Vec::new();
        for item in array_of(value, "timeSignatures")? {
            time_signatures.push(VoiceVoxTimeSignature::from_json(item)?);
        }
        let mut tracks = Vec::new();
        if let Some(Json::Object(entries)) = field(value, "tracks") {
            for (key, item) in entries {
                tracks.push((key.clone(), VoiceVoxTrack::from_json(item)?));
            }
        }
        let mut track_order = Vec::new();
        for item in array_of(value, "trackOrder")? {
            track_order.push(str_of(item, "trackOrder")?);
        }
        Ok(Self {
            tpqn: opt_i64(value, "tpqn", libresvip_core::core::constants::TICKS_IN_BEAT)?,
            tempos,
            time_signatures,
            tracks,
            track_order,
        })
    }
}

#[derive(Clone, Debug)]
pub struct VoiceVoxProject {
    pub app_version: String,
    pub song: VoiceVoxSong,
}

impl Default for VoiceVoxProject {
    fn default() -> Self {
        Self {
            app_version: "0.21.1".to_owned(),
            song: VoiceVoxSong::default(),
        }
    }
}

impl VoiceVoxProject {
    pub fn to_json(&self) -> Json {
        Json::object([
            ("appVersion", Json::Str(self.app_version.clone())),
            (
                "talk",
                Json::object([
                    ("audioKeys", Json::Array(Vec::new())),
                    ("audioItems", Json::empty_object()),
                ]),
            ),
            ("song", self.song.to_json()),
        ])
    }

    pub fn from_json(value: &Json) -> Result<Self> {
        let default = Self::default();
        let song = match field(value, "song") {
            None | Some(Json::Null) => VoiceVoxSong::default(),
            Some(item) => VoiceVoxSong::from_json(item)?,
        };
        Ok(Self {
            app_version: opt_str(value, "appVersion", &default.app_version)?,
            song,
        })
    }
}

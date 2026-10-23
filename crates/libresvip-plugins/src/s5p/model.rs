//! Pydantic-compatible model of a Synthesizer V Editor (`.s5p`) project.
//!
//! Field order, aliases and defaults mirror `libresvip/plugins/s5p/model.py`.
//! The converter serialises with `by_alias=True, exclude_none=True`, so every
//! optional field is skipped when it is `None`.

use serde::de::Error as DeError;
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Ticks per second of the S5P format.
pub const TICK_RATE: i64 = 1_470_000;

/// `libresvip.core.constants.DEFAULT_PHONEME`.
pub const DEFAULT_PHONEME: &str = "la";
/// `libresvip.core.constants.DEFAULT_BPM`.
pub const DEFAULT_BPM: f64 = 120.0;

fn default_bpm() -> f64 {
    DEFAULT_BPM
}

fn default_color() -> String {
    "15e879".to_owned()
}

/// A relative pitch-curve point (`offset`, `value`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct S5pPoint {
    pub offset: f64,
    pub value: f64,
}

impl S5pPoint {
    pub fn new(offset: f64, value: f64) -> Self {
        Self { offset, value }
    }
}

/// `S5pPoints`: a flat `[offset, value, ...]` JSON array of points.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct S5pPoints(pub Vec<S5pPoint>);

impl S5pPoints {
    pub fn root(&self) -> &[S5pPoint] {
        &self.0
    }

    pub fn from_points(points: Vec<S5pPoint>) -> Self {
        Self(points)
    }
}

impl Serialize for S5pPoints {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(self.0.len() * 2))?;
        for point in &self.0 {
            seq.serialize_element(&point.offset)?;
            seq.serialize_element(&point.value)?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for S5pPoints {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let flat = Vec::<f64>::deserialize(deserializer)?;
        if flat.len() % 2 != 0 {
            return Err(D::Error::custom(
                "s5p point array must contain an even number of values",
            ));
        }
        Ok(Self(
            flat.chunks_exact(2)
                .map(|pair| S5pPoint::new(pair[0], pair[1]))
                .collect(),
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct S5pMeterItem {
    pub measure: i64,
    #[serde(rename = "beatPerMeasure", alias = "beat_per_measure")]
    pub beat_per_measure: i64,
    #[serde(rename = "beatGranularity", alias = "beat_granularity")]
    pub beat_granularity: i64,
}

impl Default for S5pMeterItem {
    fn default() -> Self {
        Self {
            measure: 0,
            beat_per_measure: 4,
            beat_granularity: 4,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct S5pTempoItem {
    pub position: i64,
    #[serde(
        rename = "beatPerMinute",
        alias = "beat_per_minute",
        default = "default_bpm"
    )]
    pub beat_per_minute: f64,
}

impl Default for S5pTempoItem {
    fn default() -> Self {
        Self {
            position: 0,
            beat_per_minute: DEFAULT_BPM,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct S5pDbDefaults {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lyric: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub breathiness: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gender: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tension: Option<f64>,
    #[serde(rename = "dF0Vbr", alias = "d_f0_vbr")]
    pub d_f0_vbr: f64,
    #[serde(rename = "pF0Vbr", alias = "p_f0_vbr")]
    pub p_f0_vbr: f64,
    #[serde(rename = "tF0VbrLeft", alias = "t_f0_vbr_left")]
    pub t_f0_vbr_left: f64,
    #[serde(rename = "tF0VbrRight", alias = "t_f0_vbr_right")]
    pub t_f0_vbr_right: f64,
    #[serde(rename = "tF0VbrStart", alias = "t_f0_vbr_start")]
    pub t_f0_vbr_start: f64,
    #[serde(rename = "fF0Vbr", alias = "f_f0_vbr")]
    pub f_f0_vbr: f64,
    #[serde(rename = "tF0Left", alias = "t_f0_left")]
    pub t_f0_left: f64,
    #[serde(rename = "tF0Right", alias = "t_f0_right")]
    pub t_f0_right: f64,
    #[serde(rename = "dF0Left", alias = "d_f0_left")]
    pub d_f0_left: f64,
    #[serde(rename = "dF0Right", alias = "d_f0_right")]
    pub d_f0_right: f64,
    #[serde(rename = "dF0Jitter", alias = "d_f0_jitter")]
    pub d_f0_jitter: f64,
}

impl Default for S5pDbDefaults {
    fn default() -> Self {
        Self {
            lyric: Some(DEFAULT_PHONEME.to_owned()),
            breathiness: Some(0.0),
            gender: Some(0.0),
            tension: Some(0.0),
            d_f0_vbr: 0.025,
            p_f0_vbr: 0.0,
            t_f0_vbr_left: 0.15,
            t_f0_vbr_right: 0.15,
            t_f0_vbr_start: 0.25,
            f_f0_vbr: 5.5,
            t_f0_left: 0.0,
            t_f0_right: 0.0,
            d_f0_left: 0.0,
            d_f0_right: 0.0,
            d_f0_jitter: 1.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct S5pNote {
    pub lyric: String,
    pub onset: i64,
    pub duration: i64,
    #[serde(default)]
    pub comment: String,
    pub pitch: i64,
    #[serde(
        rename = "dF0Vbr",
        alias = "d_f0_vbr",
        skip_serializing_if = "Option::is_none"
    )]
    pub d_f0_vbr: Option<f64>,
    #[serde(
        rename = "pF0Vbr",
        alias = "p_f0_vbr",
        skip_serializing_if = "Option::is_none"
    )]
    pub p_f0_vbr: Option<f64>,
    #[serde(
        rename = "tF0VbrLeft",
        alias = "t_f0_vbr_left",
        skip_serializing_if = "Option::is_none"
    )]
    pub t_f0_vbr_left: Option<f64>,
    #[serde(
        rename = "tF0VbrRight",
        alias = "t_f0_vbr_right",
        skip_serializing_if = "Option::is_none"
    )]
    pub t_f0_vbr_right: Option<f64>,
    #[serde(
        rename = "tF0VbrStart",
        alias = "t_f0_vbr_start",
        skip_serializing_if = "Option::is_none"
    )]
    pub t_f0_vbr_start: Option<f64>,
    #[serde(
        rename = "fF0Vbr",
        alias = "f_f0_vbr",
        skip_serializing_if = "Option::is_none"
    )]
    pub f_f0_vbr: Option<f64>,
    #[serde(
        rename = "tF0Left",
        alias = "t_f0_left",
        skip_serializing_if = "Option::is_none"
    )]
    pub t_f0_left: Option<f64>,
    #[serde(
        rename = "tF0Right",
        alias = "t_f0_right",
        skip_serializing_if = "Option::is_none"
    )]
    pub t_f0_right: Option<f64>,
    #[serde(
        rename = "dF0Left",
        alias = "d_f0_left",
        skip_serializing_if = "Option::is_none"
    )]
    pub d_f0_left: Option<f64>,
    #[serde(
        rename = "dF0Right",
        alias = "d_f0_right",
        skip_serializing_if = "Option::is_none"
    )]
    pub d_f0_right: Option<f64>,
    #[serde(
        rename = "dF0Jitter",
        alias = "d_f0_jitter",
        skip_serializing_if = "Option::is_none"
    )]
    pub d_f0_jitter: Option<f64>,
    #[serde(
        rename = "tF0Offset",
        alias = "t_f0_offset",
        skip_serializing_if = "Option::is_none"
    )]
    pub t_f0_offset: Option<f64>,
    #[serde(
        rename = "tNoteOffset",
        alias = "t_note_offset",
        skip_serializing_if = "Option::is_none"
    )]
    pub t_note_offset: Option<f64>,
    #[serde(
        rename = "tSylOnset",
        alias = "t_syl_onset",
        skip_serializing_if = "Option::is_none"
    )]
    pub t_syl_onset: Option<f64>,
    #[serde(
        rename = "tSylCoda",
        alias = "t_syl_coda",
        skip_serializing_if = "Option::is_none"
    )]
    pub t_syl_coda: Option<f64>,
    #[serde(
        rename = "wSylNucleus",
        alias = "w_syl_nucleus",
        skip_serializing_if = "Option::is_none"
    )]
    pub w_syl_nucleus: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sublib: Option<String>,
}

impl Default for S5pNote {
    fn default() -> Self {
        Self {
            lyric: String::new(),
            onset: 0,
            duration: 0,
            comment: String::new(),
            pitch: 0,
            d_f0_vbr: None,
            p_f0_vbr: None,
            t_f0_vbr_left: None,
            t_f0_vbr_right: None,
            t_f0_vbr_start: None,
            f_f0_vbr: None,
            t_f0_left: None,
            t_f0_right: None,
            d_f0_left: None,
            d_f0_right: None,
            d_f0_jitter: None,
            t_f0_offset: None,
            t_note_offset: None,
            t_syl_onset: None,
            t_syl_coda: None,
            w_syl_nucleus: None,
            sublib: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct S5pTrackMixer {
    #[serde(rename = "gainDecibel", alias = "gain_decibel")]
    pub gain_decibel: f64,
    pub pan: f64,
    pub muted: bool,
    pub solo: bool,
    #[serde(rename = "engineOn", alias = "engine_on")]
    pub engine_on: bool,
    pub display: bool,
}

impl Default for S5pTrackMixer {
    fn default() -> Self {
        Self {
            gain_decibel: 0.0,
            pan: 0.0,
            muted: false,
            solo: false,
            engine_on: true,
            display: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct S5pParameters {
    pub interval: i64,
    #[serde(rename = "pitchDelta", alias = "pitch_delta")]
    pub pitch_delta: S5pPoints,
    #[serde(rename = "vibratoEnv", alias = "vibrato_env")]
    pub vibrato_env: S5pPoints,
    pub loudness: S5pPoints,
    pub tension: S5pPoints,
    pub breathiness: S5pPoints,
    pub voicing: S5pPoints,
    pub gender: S5pPoints,
}

impl Default for S5pParameters {
    fn default() -> Self {
        Self {
            interval: 5_512_500,
            pitch_delta: S5pPoints::default(),
            vibrato_env: S5pPoints::default(),
            loudness: S5pPoints::default(),
            tension: S5pPoints::default(),
            breathiness: S5pPoints::default(),
            voicing: S5pPoints::default(),
            gender: S5pPoints::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct S5pTrack {
    pub name: String,
    #[serde(rename = "dbName", alias = "db_name", default)]
    pub db_name: String,
    #[serde(default = "default_color")]
    pub color: String,
    #[serde(rename = "displayOrder", alias = "display_order", default)]
    pub display_order: i64,
    #[serde(rename = "dbDefaults", alias = "db_defaults", default)]
    pub db_defaults: S5pDbDefaults,
    #[serde(default)]
    pub notes: Vec<Option<S5pNote>>,
    #[serde(
        rename = "gsEvents",
        alias = "gs_events",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub gs_events: Option<()>,
    #[serde(default)]
    pub mixer: S5pTrackMixer,
    #[serde(default)]
    pub parameters: S5pParameters,
}

impl Default for S5pTrack {
    fn default() -> Self {
        Self {
            name: String::new(),
            db_name: String::new(),
            color: default_color(),
            display_order: 0,
            db_defaults: S5pDbDefaults::default(),
            notes: Vec::new(),
            gs_events: None,
            mixer: S5pTrackMixer::default(),
            parameters: S5pParameters::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct S5pInstrumental {
    pub filename: String,
    pub offset: f64,
}

impl Default for S5pInstrumental {
    fn default() -> Self {
        Self {
            filename: String::new(),
            offset: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct S5pMixer {
    #[serde(rename = "gainInstrumentalDecibel", alias = "gain_instrumental_decibel")]
    pub gain_instrumental_decibel: f64,
    #[serde(
        rename = "gainVocalMasterDecibel",
        alias = "gain_vocal_master_decibel"
    )]
    pub gain_vocal_master_decibel: f64,
    #[serde(rename = "instrumentalMuted", alias = "instrumental_muted")]
    pub instrumental_muted: bool,
    #[serde(rename = "vocalMasterMuted", alias = "vocal_master_muted")]
    pub vocal_master_muted: bool,
}

impl Default for S5pMixer {
    fn default() -> Self {
        Self {
            gain_instrumental_decibel: 0.0,
            gain_vocal_master_decibel: 0.0,
            instrumental_muted: false,
            vocal_master_muted: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct S5pProject {
    pub version: i64,
    pub meter: Vec<S5pMeterItem>,
    pub tempo: Vec<S5pTempoItem>,
    pub tracks: Vec<S5pTrack>,
    pub instrumental: S5pInstrumental,
    pub mixer: S5pMixer,
}

impl Default for S5pProject {
    fn default() -> Self {
        Self {
            version: 7,
            meter: Vec::new(),
            tempo: Vec::new(),
            tracks: Vec::new(),
            instrumental: S5pInstrumental::default(),
            mixer: S5pMixer::default(),
        }
    }
}

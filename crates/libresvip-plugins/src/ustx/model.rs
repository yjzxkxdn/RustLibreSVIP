//! Port of `libresvip.plugins.ustx.model` (the YAML project model).
//!
//! Field order mirrors the pydantic declaration order, and `None` fields are
//! skipped, matching `model_dump(by_alias=True, exclude_none=True)`.

use serde::{Deserialize, Serialize};

/// `str | float` — `USTXProject.ustx_version`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VersionValue {
    Str(String),
    Float(f64),
}

impl Default for VersionValue {
    fn default() -> Self {
        Self::Str("0.6".to_owned())
    }
}

fn default_shape() -> Option<String> {
    Some("io".to_owned())
}

fn is_zero(value: &i64) -> bool {
    *value == 0
}

/// `PitchPoint`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PitchPoint {
    pub x: f64,
    pub y: f64,
    #[serde(default = "default_shape", skip_serializing_if = "Option::is_none")]
    pub shape: Option<String>,
}

impl Default for PitchPoint {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            shape: default_shape(),
        }
    }
}

/// `UTempo`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UTempo {
    #[serde(default)]
    pub position: i64,
    #[serde(default = "default_bpm")]
    pub bpm: f64,
}

fn default_bpm() -> f64 {
    libresvip_core::model::base::DEFAULT_BPM
}

impl Default for UTempo {
    fn default() -> Self {
        Self {
            position: 0,
            bpm: default_bpm(),
        }
    }
}

/// `UTimeSignature`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UTimeSignature {
    #[serde(default)]
    pub bar_position: i64,
    #[serde(default = "default_beat_per_bar")]
    pub beat_per_bar: i64,
    #[serde(default = "default_beat_unit")]
    pub beat_unit: i64,
}

fn default_beat_per_bar() -> i64 {
    4
}

fn default_beat_unit() -> i64 {
    4
}

impl Default for UTimeSignature {
    fn default() -> Self {
        Self {
            bar_position: 0,
            beat_per_bar: 4,
            beat_unit: 4,
        }
    }
}

/// `UCurve`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UCurve {
    #[serde(default)]
    pub xs: Vec<i64>,
    #[serde(default)]
    pub ys: Vec<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abbr: Option<String>,
}

impl UCurve {
    /// `is_empty`.
    pub fn is_empty(&self) -> bool {
        self.xs.is_empty() || self.ys.iter().all(|value| *value == 0)
    }

    /// `sample` — `bisect_left` + linear interpolation, rounded.
    pub fn sample(&self, x: i64) -> i64 {
        let index = self.xs.partition_point(|value| *value < x);
        if index < self.xs.len() && self.xs[index] == x {
            return self.ys[index];
        } else if index > 0 && index < self.xs.len() {
            let start = (self.xs[index - 1] as f64, self.ys[index - 1] as f64);
            let end = (self.xs[index] as f64, self.ys[index] as f64);
            return crate::ust::math::round_int(crate::ust::math::linear(x as f64, start, end));
        }
        0
    }
}

/// One run of consecutive values of a masked curve, on the 5-tick grid.
///
/// USTX 0.10 (`OpenUtau.Core/Ustx/UMaskedCurve.cs`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UMaskedRun {
    /// Part-relative tick of the first value.
    #[serde(default)]
    pub x: i64,
    /// `float32` in OpenUtau; `f64` here so the sampler needs no casts.
    #[serde(default)]
    pub ys: Vec<f64>,
}

/// A curve that has no value over some stretches, unlike [`UCurve`], which
/// always has at least its default. Only expression graphs read it.
///
/// USTX 0.10 (`OpenUtau.Core/Ustx/UMaskedCurve.cs`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UMaskedCurve {
    #[serde(default)]
    pub abbr: String,
    #[serde(default, deserialize_with = "vec_or_default")]
    pub runs: Vec<UMaskedRun>,
}

impl UMaskedCurve {
    /// `IsEmpty`.
    pub fn is_empty(&self) -> bool {
        self.runs.iter().all(|run| run.ys.is_empty())
    }

    /// `TrySample` — the value at a part-relative tick, linear between grid
    /// steps, or `None` when no run covers it.
    pub fn try_sample(&self, tick: f64, interval: i64) -> Option<f64> {
        let interval = interval.max(1) as f64;
        // The last run starting at or before the tick; runs are sorted and do
        // not overlap.
        let found = self
            .runs
            .partition_point(|run| run.x as f64 <= tick)
            .checked_sub(1)?;
        let run = &self.runs[found];
        if run.ys.is_empty() {
            return None;
        }
        let end = run.x as f64 + (run.ys.len() as f64 - 1.0) * interval;
        if tick > end {
            return None;
        }
        let position = (tick - run.x as f64) / interval;
        let index = (position.floor() as i64).clamp(0, run.ys.len() as i64 - 1) as usize;
        if index + 1 >= run.ys.len() {
            return Some(run.ys[index]);
        }
        let fraction = position - index as f64;
        Some(run.ys[index] + fraction * (run.ys[index + 1] - run.ys[index]))
    }
}

/// `null` and a missing key both mean "empty list" for the collections below:
/// OpenUtau writes `voice_parts: null` for a project without parts, and pydantic
/// accepted that.
fn vec_or_default<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}

/// `UVibrato`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UVibrato {
    #[serde(default = "default_vibrato_length")]
    pub length: f64,
    #[serde(default = "default_vibrato_period")]
    pub period: f64,
    #[serde(default = "default_vibrato_depth")]
    pub depth: f64,
    #[serde(rename = "in", alias = "in_value", default = "default_vibrato_in")]
    pub in_value: f64,
    #[serde(default = "default_vibrato_in")]
    pub out: f64,
    #[serde(default)]
    pub shift: f64,
    #[serde(default)]
    pub drift: f64,
    #[serde(default)]
    pub vol_link: f64,
}

fn default_vibrato_length() -> f64 {
    75.0
}

fn default_vibrato_period() -> f64 {
    175.0
}

fn default_vibrato_depth() -> f64 {
    25.0
}

fn default_vibrato_in() -> f64 {
    10.0
}

impl Default for UVibrato {
    fn default() -> Self {
        Self {
            length: 75.0,
            period: 175.0,
            depth: 25.0,
            in_value: 10.0,
            out: 10.0,
            shift: 0.0,
            drift: 0.0,
            vol_link: 0.0,
        }
    }
}

impl UVibrato {
    /// `normalized_start`.
    pub fn normalized_start(&self) -> f64 {
        1.0 - self.length / 100.0
    }

    /// `evaluate` — returns `(position, tone)`; only the pitch part is used by
    /// the base pitch generator.
    pub fn evaluate(&self, n_pos: f64, n_period: f64, note: &UNote) -> (f64, f64) {
        let n_start = self.normalized_start();
        let n_in = self.length / 100.0 * self.in_value / 100.0;
        let n_in_pos = n_start + n_in;
        let n_out = self.length / 100.0 * self.out / 100.0;
        let n_out_pos = 1.0 - n_out;
        let t = (n_pos - n_start) / n_period + self.shift / 100.0;
        let mut y = (std::f64::consts::TAU * t).sin() * self.depth;
        if n_pos < n_start {
            y = 0.0;
        } else if n_pos < n_in_pos {
            y *= (n_pos - n_start) / n_in;
        } else if n_pos > n_out_pos {
            y *= (1.0 - n_pos) / n_out;
        }
        (
            note.position as f64 + note.duration as f64 * n_pos,
            (note.pitch_cents() + y) / 100.0,
        )
    }
}

/// `UPitch`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UPitch {
    #[serde(default)]
    pub data: Vec<PitchPoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snap_first: Option<bool>,
}

/// `UExpression`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UExpression {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<i64>,
    pub abbr: String,
    pub value: f64,
}

/// `UPhonemeOverride`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UPhonemeOverride {
    pub index: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phoneme: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preutter_delta: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlap_delta: Option<f64>,
}

/// `UNote`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UNote {
    pub position: i64,
    pub duration: i64,
    pub tone: i64,
    /// Fine tuning of the note in cents (USTX 0.10). `AdjustedTone` in OpenUtau
    /// is `tone + tuning / 100`. Kept out of the output when zero so files we
    /// write stay byte-identical to the 0.6 ones.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub tuning: i64,
    pub lyric: String,
    #[serde(default)]
    pub pitch: UPitch,
    #[serde(default)]
    pub vibrato: UVibrato,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note_expressions: Option<Vec<UExpression>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phoneme_expressions: Option<Vec<UExpression>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phoneme_overrides: Option<Vec<UPhonemeOverride>>,
}

impl UNote {
    /// `end`.
    pub fn end(&self) -> i64 {
        self.position + self.duration
    }

    /// The note's pitch in cents, `AdjustedTone * 100` in OpenUtau terms:
    /// `tone` plus the USTX 0.10 fine tuning, which is already in cents.
    pub fn pitch_cents(&self) -> f64 {
        self.tone as f64 * 100.0 + self.tuning as f64
    }
}

/// `URendererSettings`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct URendererSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renderer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resampler: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wavtool: Option<String>,
}

/// `UMixFX`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UMixFX {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub eq_preset: String,
    #[serde(default)]
    pub comp_preset: String,
    #[serde(default)]
    pub reverb_preset: String,
    #[serde(default)]
    pub eq_low_db: f64,
    #[serde(default)]
    pub eq_mid_freq: f64,
    #[serde(default)]
    pub eq_mid_db: f64,
    #[serde(default)]
    pub eq_high_db: f64,
    #[serde(default)]
    pub comp_threshold_db: f64,
    #[serde(default)]
    pub comp_ratio: f64,
    #[serde(default)]
    pub comp_makeup_db: f64,
    #[serde(default)]
    pub reverb_size: f64,
    #[serde(default)]
    pub reverb_damp: f64,
    #[serde(default)]
    pub reverb_wet: f64,
    #[serde(default)]
    pub reverb_pre_delay_ms: f64,
}

/// `UTrack`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UTrack {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track_expressions: Option<Vec<serde_yaml::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_color_names: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub singer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phonemizer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renderer_settings: Option<URendererSettings>,
    #[serde(default)]
    pub mute: bool,
    #[serde(default)]
    pub solo: bool,
    #[serde(default)]
    pub volume: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pan: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mix_fx: Option<UMixFX>,
}

impl Default for UTrack {
    fn default() -> Self {
        Self {
            track_name: None,
            track_color: None,
            track_expressions: Some(Vec::new()),
            voice_color_names: None,
            singer: None,
            phonemizer: None,
            renderer_settings: None,
            mute: false,
            solo: false,
            volume: 0.0,
            pan: None,
            mix_fx: None,
        }
    }
}

/// `UVoicePart`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UVoicePart {
    pub name: String,
    #[serde(default)]
    pub position: i64,
    pub track_no: i64,
    #[serde(default)]
    pub comment: Option<String>,
    #[serde(default, deserialize_with = "vec_or_default")]
    pub notes: Vec<UNote>,
    #[serde(default, deserialize_with = "vec_or_default")]
    pub curves: Vec<UCurve>,
    /// USTX 0.10. Only expression graphs read these, but dropping them loses the
    /// pitch a 1.571+ project actually renders. Not written back: we still emit
    /// `ustx_version: 0.6`, and a 0.6 reader must not see 0.10 keys.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "vec_or_default"
    )]
    pub masked_curves: Vec<UMaskedCurve>,
    /// `computed_field duration` — recomputed when the part is generated.
    #[serde(default)]
    pub duration: i64,
}

impl UVoicePart {
    /// `duration` computed field.
    pub fn computed_duration(&self) -> i64 {
        match self.notes.last() {
            Some(note) => (note.end() - self.position).max(480),
            None => 480.max(0),
        }
    }
}

/// A YAML number that remembers whether it was an int or a float.
///
/// pydantic keeps the literal defaults of `float` fields (`0` for
/// `skip_ms`/`trim_ms`) as ints, so they are emitted as `0`, not `0.0`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Number {
    Int(i64),
    Float(f64),
}

impl Default for Number {
    fn default() -> Self {
        Self::Int(0)
    }
}

/// `UWavePart`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UWavePart {
    pub name: String,
    #[serde(default)]
    pub position: i64,
    pub track_no: i64,
    #[serde(default)]
    pub comment: Option<String>,
    #[serde(default)]
    pub relative_path: String,
    #[serde(default)]
    pub file_duration_ms: Number,
    #[serde(default)]
    pub skip_ms: Number,
    #[serde(default)]
    pub trim_ms: Number,
}

/// `USTXProject`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UstxProject {
    #[serde(default = "default_name")]
    pub name: String,
    #[serde(default)]
    pub comment: String,
    #[serde(default = "default_output_dir")]
    pub output_dir: String,
    #[serde(default = "default_cache_dir")]
    pub cache_dir: String,
    #[serde(default)]
    pub ustx_version: VersionValue,
    #[serde(default = "default_bpm")]
    pub bpm: f64,
    #[serde(default = "default_beat_per_bar")]
    pub beat_per_bar: i64,
    #[serde(default = "default_beat_unit")]
    pub beat_unit: i64,
    #[serde(default = "default_resolution")]
    pub resolution: i64,
    #[serde(default, deserialize_with = "vec_or_default")]
    pub time_signatures: Vec<UTimeSignature>,
    #[serde(default, deserialize_with = "vec_or_default")]
    pub tempos: Vec<UTempo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expressions: Option<serde_yaml::Value>,
    #[serde(default, deserialize_with = "vec_or_default")]
    pub tracks: Vec<UTrack>,
    #[serde(default, deserialize_with = "vec_or_default")]
    pub voice_parts: Vec<UVoicePart>,
    #[serde(default, deserialize_with = "vec_or_default")]
    pub wave_parts: Vec<UWavePart>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exp_selectors: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exp_primary: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exp_secondary: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<i64>,
}

fn default_name() -> String {
    "New Project".to_owned()
}

fn default_output_dir() -> String {
    "Vocal".to_owned()
}

fn default_cache_dir() -> String {
    "UCache".to_owned()
}

fn default_resolution() -> i64 {
    libresvip_core::model::base::TICKS_IN_BEAT
}

impl Default for UstxProject {
    fn default() -> Self {
        Self {
            name: default_name(),
            comment: String::new(),
            output_dir: default_output_dir(),
            cache_dir: default_cache_dir(),
            ustx_version: VersionValue::default(),
            bpm: default_bpm(),
            beat_per_bar: 4,
            beat_unit: 4,
            resolution: default_resolution(),
            time_signatures: Vec::new(),
            tempos: Vec::new(),
            expressions: None,
            tracks: Vec::new(),
            voice_parts: Vec::new(),
            wave_parts: Vec::new(),
            exp_selectors: None,
            exp_primary: Some(0),
            exp_secondary: Some(1),
            key: Some(0),
        }
    }
}

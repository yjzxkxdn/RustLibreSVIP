//! Port of `libresvip.plugins.svp.model`.
//!
//! The pydantic models serialise with `by_alias=True, exclude_none=True`, so
//! every `Option` field is skipped when it is `None`; all other fields keep
//! their declaration order.

use std::collections::HashSet;

use indexmap::IndexMap;
use serde::de::Error as DeError;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use libresvip_core::utils::text::uuid_str;

use crate::svp::constants::{
    DEFAULT_PITCH_DEPTH, DEFAULT_PITCH_PORTAMENTO, DEFAULT_PITCH_TRANSITION, DEFAULT_VIBRATO_DEPTH,
    DEFAULT_VIBRATO_FADE, DEFAULT_VIBRATO_FREQUENCY, DEFAULT_VIBRATO_JITTER,
    DEFAULT_VIBRATO_PHASE, DEFAULT_VIBRATO_START,
};
use crate::svp::math_utils::{position_to_ticks, py_round, RangeInterval};

/// pydantic's lax `float | None` coercion: Synthesizer V writes
/// `evenSyllableDuration` as a boolean, which pydantic accepts as `1.0`/`0.0`.
pub(crate) fn pydantic_float_or_none<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<f64>, D::Error> {
    struct V;
    impl<'de> serde::de::Visitor<'de> for V {
        type Value = Option<f64>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a float, a boolean or null")
        }

        fn visit_none<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_some<D: Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> std::result::Result<Self::Value, D::Error> {
            deserializer.deserialize_any(LaxFloatVisitor)
        }
    }
    struct LaxFloatVisitor;
    impl<'de> serde::de::Visitor<'de> for LaxFloatVisitor {
        type Value = Option<f64>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a float or a boolean")
        }

        fn visit_f64<E: serde::de::Error>(self, value: f64) -> std::result::Result<Self::Value, E> {
            Ok(Some(value))
        }

        fn visit_i64<E: serde::de::Error>(self, value: i64) -> std::result::Result<Self::Value, E> {
            Ok(Some(value as f64))
        }

        fn visit_u64<E: serde::de::Error>(self, value: u64) -> std::result::Result<Self::Value, E> {
            Ok(Some(value as f64))
        }

        fn visit_bool<E: serde::de::Error>(self, value: bool) -> std::result::Result<Self::Value, E> {
            Ok(Some(if value { 1.0 } else { 0.0 }))
        }
    }
    deserializer.deserialize_option(V)
}

// ---------------------------------------------------------------------------
// Points and parameter curves
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SVPoint {
    pub offset: i64,
    pub value: f64,
}

impl SVPoint {
    pub fn new(offset: i64, value: f64) -> Self {
        Self { offset, value }
    }
}

struct FlatPoints<'a>(&'a [SVPoint]);

impl Serialize for FlatPoints<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(self.0.len() * 2))?;
        for point in self.0 {
            // Python's field serializer is annotated `-> list[float]`, so
            // pydantic coerces the integer offsets to floats as well.
            seq.serialize_element(&(point.offset as f64))?;
            seq.serialize_element(&point.value)?;
        }
        seq.end()
    }
}

fn value_as_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|value| value as i64)),
        _ => None,
    }
}

fn value_as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        _ => None,
    }
}

pub fn parse_flat_points(value: &Value) -> Vec<SVPoint> {
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    let mut points = Vec::with_capacity(items.len() / 2);
    let mut index = 0;
    while index + 1 < items.len() {
        let offset = value_as_i64(&items[index]).unwrap_or(0);
        let point_value = value_as_f64(&items[index + 1]).unwrap_or(0.0);
        points.push(SVPoint::new(offset, point_value));
        index += 2;
    }
    points
}

/// `SVParamCurve`: `{"points": [offset, value, ...], "mode": "linear"}`.
#[derive(Clone, Debug, PartialEq)]
pub struct SVParamCurve {
    pub points: Vec<SVPoint>,
    pub mode: String,
}

impl Default for SVParamCurve {
    fn default() -> Self {
        Self {
            points: Vec::new(),
            mode: "linear".to_owned(),
        }
    }
}

impl SVParamCurve {
    pub fn new(points: Vec<SVPoint>, mode: impl Into<String>) -> Self {
        Self {
            points,
            mode: mode.into(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// `SVParamCurve.__add__` — shift every point offset.
    pub fn add_offset(&self, offset: i64) -> SVParamCurve {
        let mut curve = self.clone();
        for point in &mut curve.points {
            point.offset += offset;
        }
        curve
    }

    /// `SVParamCurve.edited_range`.
    pub fn edited_range(&self, default_value: f64) -> RangeInterval {
        const TOLERANCE: f64 = 1e-6;
        let points: Vec<SVPoint> = self
            .points
            .iter()
            .map(|point| SVPoint::new(position_to_ticks(point.offset), point.value))
            .collect();
        let mut interval = RangeInterval::new();
        if points.is_empty() {
            return interval;
        }
        let mut ranges: Vec<(i64, i64)> = Vec::new();
        if points.len() == 1 {
            if (points[0].value - default_value).abs() < TOLERANCE {
                return interval;
            }
            ranges.push((0, i64::MAX / 2));
            return RangeInterval::from_pairs(ranges);
        }
        if (points[0].value - default_value).abs() > TOLERANCE {
            ranges.push((0, points[0].offset));
        }
        let mut start = points[0].offset;
        let mut end = points[0].offset;
        for index in 1..points.len() {
            if (points[index - 1].value - default_value).abs() < TOLERANCE
                && (points[index].value - default_value).abs() < TOLERANCE
            {
                if start < end {
                    ranges.push((start, end));
                }
                start = points[index].offset;
            } else {
                end = points[index].offset;
            }
        }
        if start < end {
            ranges.push((start, end));
        }
        if (points[points.len() - 1].value - default_value).abs() > TOLERANCE {
            ranges.push((points[points.len() - 1].offset, i64::MAX / 2));
        }
        interval = RangeInterval::from_pairs(ranges);
        interval
    }
}

impl Serialize for SVParamCurve {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("points", &FlatPoints(&self.points))?;
        map.serialize_entry("mode", &self.mode)?;
        map.end()
    }
}

impl<'de> Deserialize<'de> for SVParamCurve {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let mut curve = SVParamCurve::default();
        match &value {
            Value::Object(map) => {
                if let Some(mode) = map.get("mode").and_then(Value::as_str) {
                    curve.mode = mode.to_owned();
                }
                if let Some(points) = map.get("points") {
                    curve.points = parse_flat_points(points);
                }
            }
            Value::Array(_) => curve.points = parse_flat_points(&value),
            _ => {}
        }
        Ok(curve)
    }
}

/// Base points structure used by `SVCurvePitchControl`.
fn serialize_points<S: Serializer>(
    points: &[SVPoint],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    FlatPoints(points).serialize(serializer)
}

// ---------------------------------------------------------------------------
// Shared note/voice attributes
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SVBaseAttributes {
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
        rename = "tF0VbrStart",
        alias = "t_f0_vbr_start",
        skip_serializing_if = "Option::is_none"
    )]
    pub t_f0_vbr_start: Option<f64>,
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
        rename = "dF0Vbr",
        alias = "d_f0_vbr",
        skip_serializing_if = "Option::is_none"
    )]
    pub d_f0_vbr: Option<f64>,
    #[serde(
        rename = "fF0Vbr",
        alias = "f_f0_vbr",
        skip_serializing_if = "Option::is_none"
    )]
    pub f_f0_vbr: Option<f64>,
    #[serde(
        rename = "dF0VbrMod",
        alias = "d_f0_vbr_mod",
        skip_serializing_if = "Option::is_none"
    )]
    pub d_f0_vbr_mod: Option<f64>,
    #[serde(
        rename = "paramLoudness",
        alias = "param_loudness",
        skip_serializing_if = "Option::is_none"
    )]
    pub param_loudness: Option<f64>,
    #[serde(
        rename = "paramTension",
        alias = "param_tension",
        skip_serializing_if = "Option::is_none"
    )]
    pub param_tension: Option<f64>,
    #[serde(
        rename = "paramBreathiness",
        alias = "param_breathiness",
        skip_serializing_if = "Option::is_none"
    )]
    pub param_breathiness: Option<f64>,
    #[serde(
        rename = "paramGender",
        alias = "param_gender",
        skip_serializing_if = "Option::is_none"
    )]
    pub param_gender: Option<f64>,
    #[serde(
        rename = "paramToneShift",
        alias = "param_tone_shift",
        skip_serializing_if = "Option::is_none"
    )]
    pub param_tone_shift: Option<f64>,
    #[serde(
        rename = "improviseAttackRelease",
        alias = "improvise_attack_release",
        skip_serializing_if = "Option::is_none"
    )]
    pub improvise_attack_release: Option<bool>,
    #[serde(
        rename = "languageOverride",
        alias = "language_override",
        skip_serializing_if = "Option::is_none"
    )]
    pub language_override: Option<String>,
    #[serde(
        rename = "phonesetOverride",
        alias = "phoneset_override",
        skip_serializing_if = "Option::is_none"
    )]
    pub phoneset_override: Option<String>,
    #[serde(
        rename = "transposeCents",
        alias = "transpose_cents",
        skip_serializing_if = "Option::is_none"
    )]
    pub transpose_cents: Option<f64>,
    #[serde(
        rename = "transposeSemitones",
        alias = "transpose_semitones",
        skip_serializing_if = "Option::is_none"
    )]
    pub transpose_semitones: Option<f64>,
    #[serde(
        rename = "choirNumStems",
        alias = "choir_num_stems",
        skip_serializing_if = "Option::is_none"
    )]
    pub choir_num_stems: Option<i64>,
    #[serde(
        rename = "choirPartName",
        alias = "choir_part_name",
        skip_serializing_if = "Option::is_none"
    )]
    pub choir_part_name: Option<String>,
    #[serde(
        rename = "consonantStrength",
        alias = "consonant_strength",
        skip_serializing_if = "Option::is_none"
    )]
    pub consonant_strength: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SVPhonemeAttribute {
    #[serde(
        rename = "leftOffset",
        alias = "left_offset",
        skip_serializing_if = "Option::is_none"
    )]
    pub left_offset: Option<f64>,
    #[serde(rename = "strength")]
    pub strength: f64,
    #[serde(rename = "alt")]
    pub alt: i64,
    #[serde(rename = "position", skip_serializing_if = "Option::is_none")]
    pub position: Option<f64>,
    #[serde(rename = "activity", skip_serializing_if = "Option::is_none")]
    pub activity: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SVNoteAttributes {
    #[serde(flatten)]
    pub base: SVBaseAttributes,
    #[serde(
        rename = "tF0Offset",
        alias = "t_f0_offset",
        skip_serializing_if = "Option::is_none"
    )]
    pub t_f0_offset: Option<f64>,
    #[serde(
        rename = "pF0Vbr",
        alias = "p_f0_vbr",
        skip_serializing_if = "Option::is_none"
    )]
    pub p_f0_vbr: Option<f64>,
    #[serde(
        rename = "dF0Jitter",
        alias = "d_f0_jitter",
        skip_serializing_if = "Option::is_none"
    )]
    pub d_f0_jitter: Option<f64>,
    #[serde(
        rename = "tNoteOffset",
        alias = "t_note_offset",
        skip_serializing_if = "Option::is_none"
    )]
    pub t_note_offset: Option<f64>,
    #[serde(rename = "dur", skip_serializing_if = "Option::is_none")]
    pub dur: Option<Vec<f64>>,
    #[serde(rename = "alt", skip_serializing_if = "Option::is_none")]
    pub alt: Option<Vec<i64>>,
    #[serde(
        rename = "exprGroup",
        alias = "expr_group",
        skip_serializing_if = "Option::is_none"
    )]
    pub expr_group: Option<String>,
    #[serde(
        rename = "expValueX",
        alias = "exp_value_x",
        skip_serializing_if = "Option::is_none"
    )]
    pub exp_value_x: Option<f64>,
    #[serde(
        rename = "expValueY",
        alias = "exp_value_y",
        skip_serializing_if = "Option::is_none"
    )]
    pub exp_value_y: Option<f64>,
    #[serde(rename = "strength", skip_serializing_if = "Option::is_none")]
    pub strength: Option<Vec<f64>>,
    #[serde(rename = "rTone", skip_serializing_if = "Option::is_none")]
    pub r_tone: Option<f64>,
    #[serde(
        rename = "rIntonation",
        alias = "r_intonation",
        skip_serializing_if = "Option::is_none"
    )]
    pub r_intonation: Option<f64>,
    #[serde(
        rename = "evenSyllableDuration",
        alias = "even_syllable_duration",
        skip_serializing_if = "Option::is_none",
        deserialize_with = "pydantic_float_or_none"
    )]
    pub even_syllable_duration: Option<f64>,
    #[serde(rename = "phonemes", skip_serializing_if = "Option::is_none")]
    pub phonemes: Option<Vec<SVPhonemeAttribute>>,
    #[serde(rename = "muted", skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
}

impl SVNoteAttributes {
    pub fn transition_offset(&self) -> f64 {
        self.t_f0_offset.unwrap_or(DEFAULT_PITCH_TRANSITION)
    }

    pub fn portamento_left(&self) -> f64 {
        self.base.t_f0_left.unwrap_or(DEFAULT_PITCH_PORTAMENTO)
    }

    pub fn portamento_right(&self) -> f64 {
        self.base.t_f0_right.unwrap_or(DEFAULT_PITCH_PORTAMENTO)
    }

    pub fn depth_left(&self) -> f64 {
        self.base.d_f0_left.unwrap_or(DEFAULT_PITCH_DEPTH)
    }

    pub fn depth_right(&self) -> f64 {
        self.base.d_f0_right.unwrap_or(DEFAULT_PITCH_DEPTH)
    }

    pub fn vibrato_start(&self) -> f64 {
        self.base.t_f0_vbr_start.unwrap_or(DEFAULT_VIBRATO_START)
    }

    pub fn vibrato_left(&self) -> f64 {
        self.base.t_f0_vbr_left.unwrap_or(DEFAULT_VIBRATO_FADE)
    }

    pub fn vibrato_right(&self) -> f64 {
        self.base.t_f0_vbr_right.unwrap_or(DEFAULT_VIBRATO_FADE)
    }

    pub fn vibrato_depth(&self) -> f64 {
        self.base.d_f0_vbr.unwrap_or(DEFAULT_VIBRATO_DEPTH)
    }

    pub fn vibrato_frequency(&self) -> f64 {
        self.base.f_f0_vbr.unwrap_or(DEFAULT_VIBRATO_FREQUENCY)
    }

    pub fn vibrato_phase(&self) -> f64 {
        self.p_f0_vbr.unwrap_or(DEFAULT_VIBRATO_PHASE)
    }

    pub fn vibrato_jitter(&self) -> f64 {
        self.d_f0_jitter.unwrap_or(DEFAULT_VIBRATO_JITTER)
    }

    /// `set_phone_duration`, including the padding quirk.
    pub fn set_phone_duration(&mut self, index: usize, duration: f64) {
        match &mut self.dur {
            None => {
                self.dur = Some(vec![1.0; index + 1]);
            }
            Some(dur) => {
                if dur.len() <= index {
                    let missing = index - dur.len() + 1;
                    dur.extend(std::iter::repeat(1.0).take(missing));
                }
            }
        }
        if let Some(dur) = &mut self.dur {
            if let Some(slot) = dur.get_mut(index) {
                *slot = duration;
            }
        }
    }

    /// `SVNoteAttributes.pitch_edited`.
    pub fn pitch_edited(
        &self,
        regard_default_vibrato_as_unedited: bool,
        consider_instant_pitch_mode: bool,
    ) -> bool {
        let mut transition_edited = [
            self.t_f0_offset,
            self.base.t_f0_left,
            self.base.t_f0_right,
            self.base.d_f0_left,
            self.base.d_f0_right,
        ]
        .iter()
        .any(Option::is_some);
        if consider_instant_pitch_mode {
            const TOLERANCE: f64 = 1e-6;
            transition_edited &= [
                (self.portamento_left() - DEFAULT_PITCH_PORTAMENTO).abs(),
                (self.portamento_right() - DEFAULT_PITCH_PORTAMENTO).abs(),
                (self.depth_left() - DEFAULT_PITCH_DEPTH).abs(),
                (self.depth_right() - DEFAULT_PITCH_DEPTH).abs(),
            ]
            .iter()
            .any(|value| *value >= TOLERANCE);
        }
        let mut vibrato_edited = self.vibrato_depth() != 0.0;
        if regard_default_vibrato_as_unedited {
            vibrato_edited &= self.base.d_f0_vbr.is_some()
                || self.base.f_f0_vbr.is_some()
                || self.p_f0_vbr.is_some()
                || self.base.t_f0_vbr_left.is_some()
                || self.base.t_f0_vbr_right.is_some()
                || self.base.t_f0_vbr_start.is_some();
        }
        transition_edited || vibrato_edited
    }

    /// Fill fields that are `None` from `other` (the `merge_attributes` quirk:
    /// only keys absent from the original dictionary are overwritten).
    pub fn merge_from(&mut self, other: &SVNoteAttributes) {
        macro_rules! fill {
            ($dst:expr, $src:expr, $($field:ident),* $(,)?) => {
                $(
                    if $dst.$field.is_none() {
                        $dst.$field = $src.$field.clone();
                    }
                )*
            };
        }
        fill!(
            self.base,
            other.base,
            t_f0_left,
            t_f0_right,
            d_f0_left,
            d_f0_right,
            t_f0_vbr_start,
            t_f0_vbr_left,
            t_f0_vbr_right,
            d_f0_vbr,
            f_f0_vbr,
            d_f0_vbr_mod,
            param_loudness,
            param_tension,
            param_breathiness,
            param_gender,
            param_tone_shift,
            improvise_attack_release,
            language_override,
            phoneset_override,
            transpose_cents,
            transpose_semitones,
            choir_num_stems,
            choir_part_name,
            consonant_strength,
        );
        fill!(
            self,
            other,
            t_f0_offset,
            p_f0_vbr,
            d_f0_jitter,
            t_note_offset,
            dur,
            alt,
            expr_group,
            exp_value_x,
            exp_value_y,
            strength,
            r_tone,
            r_intonation,
            even_syllable_duration,
            phonemes,
            muted,
        );
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SVVoiceDe {
    #[serde(flatten)]
    pub base: SVBaseAttributes,
    #[serde(
        rename = "vocalModeInherited",
        alias = "vocal_mode_inherited",
        skip_serializing_if = "Option::is_none"
    )]
    pub vocal_mode_inherited: Option<bool>,
    #[serde(
        rename = "vocalModePreset",
        alias = "vocal_mode_preset",
        skip_serializing_if = "Option::is_none"
    )]
    pub vocal_mode_preset: Option<String>,
    #[serde(
        rename = "vocalModeParams",
        alias = "vocal_mode_params",
        skip_serializing_if = "Option::is_none"
    )]
    pub vocal_mode_params: Option<IndexMap<String, Value>>,
    #[serde(
        rename = "renderMode",
        alias = "render_mode",
        skip_serializing_if = "Option::is_none"
    )]
    pub render_mode: Option<String>,
    #[serde(
        rename = "choirSeatingSeparation",
        alias = "choir_seating_separation",
        skip_serializing_if = "Option::is_none"
    )]
    pub choir_seating_separation: Option<f64>,
}

impl Default for SVVoiceDe {
    fn default() -> Self {
        Self {
            base: SVBaseAttributes {
                t_f0_left: Some(0.0),
                t_f0_right: Some(0.0),
                d_f0_left: Some(0.0),
                d_f0_right: Some(0.0),
                t_f0_vbr_start: Some(0.25),
                t_f0_vbr_left: Some(0.2),
                t_f0_vbr_right: Some(0.2),
                d_f0_vbr: Some(1.0),
                f_f0_vbr: Some(5.5),
                ..Default::default()
            },
            vocal_mode_inherited: Some(true),
            vocal_mode_preset: Some(String::new()),
            vocal_mode_params: None,
            render_mode: None,
            choir_seating_separation: None,
        }
    }
}

/// `SVVoice` keeps the pydantic defaults *and* which keys were present in the
/// input file, because `to_attributes()` uses `exclude_unset=True`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SVVoice {
    #[serde(flatten)]
    pub base: SVBaseAttributes,
    #[serde(
        rename = "vocalModeInherited",
        alias = "vocal_mode_inherited",
        skip_serializing_if = "Option::is_none"
    )]
    pub vocal_mode_inherited: Option<bool>,
    #[serde(
        rename = "vocalModePreset",
        alias = "vocal_mode_preset",
        skip_serializing_if = "Option::is_none"
    )]
    pub vocal_mode_preset: Option<String>,
    #[serde(
        rename = "vocalModeParams",
        alias = "vocal_mode_params",
        skip_serializing_if = "Option::is_none"
    )]
    pub vocal_mode_params: Option<IndexMap<String, Value>>,
    #[serde(
        rename = "renderMode",
        alias = "render_mode",
        skip_serializing_if = "Option::is_none"
    )]
    pub render_mode: Option<String>,
    #[serde(
        rename = "choirSeatingSeparation",
        alias = "choir_seating_separation",
        skip_serializing_if = "Option::is_none"
    )]
    pub choir_seating_separation: Option<f64>,
    #[serde(skip)]
    pub explicit: HashSet<String>,
}

impl Default for SVVoice {
    fn default() -> Self {
        let raw = SVVoiceDe::default();
        Self {
            base: raw.base,
            vocal_mode_inherited: raw.vocal_mode_inherited,
            vocal_mode_preset: raw.vocal_mode_preset,
            vocal_mode_params: raw.vocal_mode_params,
            render_mode: raw.render_mode,
            choir_seating_separation: raw.choir_seating_separation,
            explicit: HashSet::new(),
        }
    }
}

impl<'de> Deserialize<'de> for SVVoice {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let explicit = value
            .as_object()
            .map(|map| map.keys().cloned().collect::<HashSet<_>>())
            .unwrap_or_default();
        let raw: SVVoiceDe = serde_json::from_value(value).map_err(D::Error::custom)?;
        Ok(Self {
            base: raw.base,
            vocal_mode_inherited: raw.vocal_mode_inherited,
            vocal_mode_preset: raw.vocal_mode_preset,
            vocal_mode_params: raw.vocal_mode_params,
            render_mode: raw.render_mode,
            choir_seating_separation: raw.choir_seating_separation,
            explicit,
        })
    }
}

impl SVVoice {
    /// `SVVoice.to_attributes` with `exclude_none`/`exclude_unset`.
    pub fn to_attributes(&self) -> SVNoteAttributes {
        let mut attributes = SVNoteAttributes::default();
        macro_rules! copy {
            ($field:ident, $alias:literal, $snake:literal) => {
                if self.explicit.contains($alias) || self.explicit.contains($snake) {
                    attributes.base.$field = self.base.$field.clone();
                }
            };
        }
        copy!(t_f0_left, "tF0Left", "t_f0_left");
        copy!(t_f0_right, "tF0Right", "t_f0_right");
        copy!(d_f0_left, "dF0Left", "d_f0_left");
        copy!(d_f0_right, "dF0Right", "d_f0_right");
        copy!(t_f0_vbr_start, "tF0VbrStart", "t_f0_vbr_start");
        copy!(t_f0_vbr_left, "tF0VbrLeft", "t_f0_vbr_left");
        copy!(t_f0_vbr_right, "tF0VbrRight", "t_f0_vbr_right");
        copy!(d_f0_vbr, "dF0Vbr", "d_f0_vbr");
        copy!(f_f0_vbr, "fF0Vbr", "f_f0_vbr");
        copy!(d_f0_vbr_mod, "dF0VbrMod", "d_f0_vbr_mod");
        copy!(param_loudness, "paramLoudness", "param_loudness");
        copy!(param_tension, "paramTension", "param_tension");
        copy!(param_breathiness, "paramBreathiness", "param_breathiness");
        copy!(param_gender, "paramGender", "param_gender");
        copy!(param_tone_shift, "paramToneShift", "param_tone_shift");
        copy!(language_override, "languageOverride", "language_override");
        copy!(phoneset_override, "phonesetOverride", "phoneset_override");
        copy!(transpose_cents, "transposeCents", "transpose_cents");
        copy!(transpose_semitones, "transposeSemitones", "transpose_semitones");
        copy!(choir_num_stems, "choirNumStems", "choir_num_stems");
        copy!(choir_part_name, "choirPartName", "choir_part_name");
        copy!(consonant_strength, "consonantStrength", "consonant_strength");
        // `improvise_attack_release` is explicitly excluded by Python.
        attributes
    }
}

// ---------------------------------------------------------------------------
// Notes
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SVParamTake {
    #[serde(rename = "id", alias = "id_value")]
    pub id_value: i64,
    #[serde(
        rename = "seedDuration",
        alias = "seed_duration",
        skip_serializing_if = "Option::is_none"
    )]
    pub seed_duration: Option<i64>,
    #[serde(
        rename = "seedPitch",
        alias = "seed_pitch",
        skip_serializing_if = "Option::is_none"
    )]
    pub seed_pitch: Option<i64>,
    #[serde(
        rename = "seedTimbre",
        alias = "seed_timbre",
        skip_serializing_if = "Option::is_none"
    )]
    pub seed_timbre: Option<i64>,
    #[serde(rename = "expr", skip_serializing_if = "Option::is_none")]
    pub expr: Option<f64>,
    #[serde(rename = "enh", skip_serializing_if = "Option::is_none")]
    pub enh: Option<f64>,
    #[serde(rename = "liked")]
    pub liked: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SVParamTakes {
    #[serde(rename = "activeTakeId", alias = "active_take_id")]
    pub active_take_id: i64,
    #[serde(rename = "takes")]
    pub takes: Vec<SVParamTake>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SVNote {
    #[serde(rename = "onset")]
    pub onset: i64,
    #[serde(rename = "duration")]
    pub duration: i64,
    #[serde(rename = "lyrics")]
    pub lyrics: String,
    #[serde(rename = "phonemes")]
    pub phonemes: String,
    #[serde(rename = "pitch")]
    pub pitch: i64,
    #[serde(rename = "detune", skip_serializing_if = "Option::is_none")]
    pub detune: Option<i64>,
    #[serde(rename = "accent", skip_serializing_if = "Option::is_none")]
    pub accent: Option<String>,
    #[serde(rename = "attributes")]
    pub attributes: SVNoteAttributes,
    #[serde(
        rename = "systemAttributes",
        alias = "system_attributes",
        skip_serializing_if = "Option::is_none"
    )]
    pub system_attributes: Option<SVNoteAttributes>,
    #[serde(rename = "takes", skip_serializing_if = "Option::is_none")]
    pub takes: Option<SVParamTakes>,
    #[serde(
        rename = "pitchTakes",
        alias = "pitch_takes",
        skip_serializing_if = "Option::is_none"
    )]
    pub pitch_takes: Option<SVParamTakes>,
    #[serde(
        rename = "timbreTakes",
        alias = "timbre_takes",
        skip_serializing_if = "Option::is_none"
    )]
    pub timbre_takes: Option<SVParamTakes>,
    #[serde(
        rename = "musicalType",
        alias = "musical_type",
        skip_serializing_if = "Option::is_none"
    )]
    pub musical_type: Option<String>,
    #[serde(
        rename = "instantMode",
        alias = "instant_mode",
        skip_serializing_if = "Option::is_none"
    )]
    pub instant_mode: Option<bool>,
    #[serde(rename = "uuid")]
    pub uuid: String,
}

impl Default for SVNote {
    fn default() -> Self {
        Self {
            onset: 0,
            duration: 0,
            lyrics: String::new(),
            phonemes: String::new(),
            pitch: 0,
            detune: None,
            accent: None,
            attributes: SVNoteAttributes::default(),
            system_attributes: None,
            takes: None,
            pitch_takes: None,
            timbre_takes: None,
            musical_type: Some("singing".to_owned()),
            instant_mode: None,
            uuid: uuid_str(),
        }
    }
}

impl SVNote {
    pub fn merge_attributes(&mut self, attributes: &SVNoteAttributes) {
        self.attributes.merge_from(attributes);
    }

    /// Apply the pydantic `_merge_system_attribute` model validator.
    pub fn merge_system_attributes(&mut self) {
        if let Some(system) = self.system_attributes.take() {
            self.merge_attributes(&system);
        }
    }

    pub fn pitch_edited(
        &self,
        regard_default_vibrato_as_unedited: bool,
        consider_instant_pitch_mode: bool,
    ) -> bool {
        self.attributes.pitch_edited(
            regard_default_vibrato_as_unedited,
            consider_instant_pitch_mode && self.instant_mode != Some(false),
        )
    }

    pub fn add_offset(&self, offset: i64) -> SVNote {
        let mut note = self.clone();
        note.onset += offset;
        note
    }

    pub fn xor_pitch(&self, pitch_offset: i64) -> SVNote {
        let mut note = self.clone();
        note.pitch += pitch_offset;
        note
    }

    /// `SVNote.normalize_lyric`.
    pub fn normalize_lyric(lyric: &str) -> String {
        lyric
            .chars()
            .filter(|character| !is_blacklisted(*character))
            .collect::<String>()
            .trim()
            .to_owned()
    }

    /// `SVNote.normalize_phoneme`.
    pub fn normalize_phoneme(note: &libresvip_core::model::base::Note) -> String {
        if let Some(pronunciation) = &note.pronunciation {
            if !pronunciation.is_empty() {
                return pronunciation.clone();
            }
        }
        if let Some(hanzi) = first_chinese_run(&note.lyric) {
            return hanzi;
        }
        let valid_chars = Self::normalize_lyric(&note.lyric);
        if !valid_chars.is_empty() {
            return valid_chars.trim_start_matches('.').to_owned();
        }
        libresvip_core::core::constants::DEFAULT_PHONEME.to_owned()
    }
}

/// `symbols_blacklist` from the Python model (`Blacklist(match_substrings=True)`).
fn is_blacklisted(character: char) -> bool {
    matches!(
        character,
        '(' | ')'
            | '['
            | ']'
            | '{'
            | '}'
            | '（'
            | '）'
            | '<'
            | '>'
            | '《'
            | '》'
            | '―'
            | '—'
            | '*'
            | '×'
            | '!'
            | '！'
            | '?'
            | '？'
            | ':'
            | '：'
            | '·'
            | '•'
            | '。'
            | ','
            | '，'
            | ';'
            | '；'
            | '^'
            | '`'
            | '"'
            | '‘'
            | '’'
            | '“'
            | '”'
            | '='
            | '、'
            | '_'
            | '$'
            | '%'
            | '~'
            | '@'
            | '#'
            | '…'
            | '&'
            | '￥'
    )
}

/// First run of Hanzi characters (`re.search(rf"[{zhon.hanzi.characters}]+")`).
fn first_chinese_run(text: &str) -> Option<String> {
    let mut run = String::new();
    for character in text.chars() {
        if libresvip_core::core::lyric_phoneme::chinese::is_chinese_char(character) {
            run.push(character);
        } else if !run.is_empty() {
            return Some(run);
        }
    }
    if run.is_empty() {
        None
    } else {
        Some(run)
    }
}

// ---------------------------------------------------------------------------
// Groups / parameters / pitch controls
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SVParameters {
    #[serde(rename = "breathiness")]
    pub breathiness: SVParamCurve,
    #[serde(rename = "gender")]
    pub gender: SVParamCurve,
    #[serde(rename = "loudness")]
    pub loudness: SVParamCurve,
    #[serde(rename = "pitchDelta", alias = "pitch_delta")]
    pub pitch_delta: SVParamCurve,
    #[serde(rename = "tension")]
    pub tension: SVParamCurve,
    #[serde(rename = "vibratoEnv", alias = "vibrato_env")]
    pub vibrato_env: SVParamCurve,
    #[serde(rename = "voicing")]
    pub voicing: SVParamCurve,
    #[serde(rename = "toneShift", alias = "tone_shift")]
    pub tone_shift: SVParamCurve,
    #[serde(rename = "mouthOpening", alias = "mouth_opening")]
    pub mouth_opening: SVParamCurve,
}

impl SVParameters {
    pub fn add_offset(&self, offset: i64) -> SVParameters {
        SVParameters {
            breathiness: self.breathiness.add_offset(offset),
            gender: self.gender.add_offset(offset),
            loudness: self.loudness.add_offset(offset),
            pitch_delta: self.pitch_delta.add_offset(offset),
            tension: self.tension.add_offset(offset),
            vibrato_env: self.vibrato_env.add_offset(offset),
            voicing: self.voicing.add_offset(offset),
            tone_shift: self.tone_shift.add_offset(offset),
            mouth_opening: self.mouth_opening.add_offset(offset),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SVPointPitchControl {
    #[serde(rename = "pos")]
    pub pos: i64,
    #[serde(rename = "pitch")]
    pub pitch: f64,
    #[serde(rename = "id", alias = "id_value")]
    pub id_value: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SVCurvePitchControl {
    pub pos: i64,
    pub pitch: f64,
    pub id_value: String,
    pub points: Vec<SVPoint>,
}

impl Serialize for SVCurvePitchControl {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Pydantic puts the inherited `SVBasePoints.points` field first here.
        let mut map = serializer.serialize_map(Some(4))?;
        map.serialize_entry("points", &FlatPoints(&self.points))?;
        map.serialize_entry("pos", &self.pos)?;
        map.serialize_entry("pitch", &self.pitch)?;
        map.serialize_entry("id", &self.id_value)?;
        map.end()
    }
}

impl<'de> Deserialize<'de> for SVCurvePitchControl {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let map = value
            .as_object()
            .ok_or_else(|| D::Error::custom("pitch control must be an object"))?;
        let pos = map.get("pos").and_then(value_as_i64).unwrap_or(0);
        let pitch = map.get("pitch").and_then(value_as_f64).unwrap_or(0.0);
        let id_value = map
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let points = map
            .get("points")
            .map(parse_flat_points)
            .unwrap_or_default();
        Ok(Self {
            pos,
            pitch,
            id_value,
            points,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SVPitchControl {
    Point(SVPointPitchControl),
    Curve(SVCurvePitchControl),
}

impl SVPitchControl {
    pub fn type_name(&self) -> &'static str {
        match self {
            SVPitchControl::Point(_) => "point",
            SVPitchControl::Curve(_) => "curve",
        }
    }

    pub fn pos(&self) -> i64 {
        match self {
            SVPitchControl::Point(control) => control.pos,
            SVPitchControl::Curve(control) => control.pos,
        }
    }

    pub fn pitch(&self) -> f64 {
        match self {
            SVPitchControl::Point(control) => control.pitch,
            SVPitchControl::Curve(control) => control.pitch,
        }
    }

    pub fn points(&self) -> &[SVPoint] {
        match self {
            SVPitchControl::Point(_) => &[],
            SVPitchControl::Curve(control) => &control.points,
        }
    }
}

impl Serialize for SVPitchControl {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            SVPitchControl::Point(control) => {
                let mut map = serializer.serialize_map(Some(4))?;
                map.serialize_entry("pos", &control.pos)?;
                map.serialize_entry("pitch", &control.pitch)?;
                map.serialize_entry("id", &control.id_value)?;
                map.serialize_entry("type", "point")?;
                map.end()
            }
            SVPitchControl::Curve(control) => {
                let mut map = serializer.serialize_map(Some(5))?;
                map.serialize_entry("points", &FlatPoints(&control.points))?;
                map.serialize_entry("pos", &control.pos)?;
                map.serialize_entry("pitch", &control.pitch)?;
                map.serialize_entry("id", &control.id_value)?;
                map.serialize_entry("type", "curve")?;
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for SVPitchControl {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("curve");
        match kind {
            "point" => serde_json::from_value(value)
                .map(SVPitchControl::Point)
                .map_err(D::Error::custom),
            "curve" => serde_json::from_value(value)
                .map(SVPitchControl::Curve)
                .map_err(D::Error::custom),
            other => Err(D::Error::custom(format!("unknown pitch control type: {other}"))),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SVGroup {
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "notes")]
    pub notes: Vec<SVNote>,
    #[serde(rename = "parameters")]
    pub parameters: SVParameters,
    #[serde(rename = "uuid")]
    pub uuid: String,
    #[serde(rename = "vocalModes", alias = "vocal_modes")]
    pub vocal_modes: IndexMap<String, SVParamCurve>,
    #[serde(rename = "pitchControls", alias = "pitch_controls")]
    pub pitch_controls: Vec<SVPitchControl>,
    #[serde(
        rename = "musicalScale",
        alias = "musical_scale",
        skip_serializing_if = "Option::is_none"
    )]
    pub musical_scale: Option<IndexMap<String, String>>,
    #[serde(
        rename = "timestampLMR",
        alias = "timestamp_lmr",
        skip_serializing_if = "Option::is_none"
    )]
    pub timestamp_lmr: Option<f64>,
    #[serde(
        rename = "timestampLRSR",
        alias = "timestamp_lrsr",
        skip_serializing_if = "Option::is_none"
    )]
    pub timestamp_lrsr: Option<f64>,
}

impl Default for SVGroup {
    fn default() -> Self {
        Self {
            name: "main".to_owned(),
            notes: Vec::new(),
            parameters: SVParameters::default(),
            uuid: uuid_str(),
            vocal_modes: IndexMap::new(),
            pitch_controls: Vec::new(),
            musical_scale: None,
            timestamp_lmr: None,
            timestamp_lrsr: None,
        }
    }
}

impl SVGroup {
    /// `SVGroup.validate_notes` (json mode): drop notes before position 0.
    pub fn drop_negative_notes(&mut self) {
        self.notes.retain(|note| note.onset >= 0);
        for note in &mut self.notes {
            note.merge_system_attributes();
        }
    }

    /// `SVGroup.overlapped_with`.
    pub fn overlapped_with(&self, other: &SVGroup) -> bool {
        for note in &self.notes {
            for other_note in &other.notes {
                let x = note.onset - (other_note.onset + other_note.duration);
                let y = (note.onset + note.duration) - other_note.onset;
                if x * y == 0 {
                    continue;
                }
                if (x < 0) ^ (y > 0) {
                    return true;
                }
            }
        }
        false
    }

    pub fn add_offset(&self, offset: i64) -> SVGroup {
        let mut group = self.clone();
        group.notes = self
            .notes
            .iter()
            .filter(|note| note.onset + offset >= 0)
            .map(|note| note.add_offset(offset))
            .collect();
        group.parameters = self.parameters.add_offset(offset);
        group
    }

    pub fn xor_pitch(&self, pitch_offset: i64) -> SVGroup {
        let mut group = self.clone();
        group.notes = self
            .notes
            .iter()
            .map(|note| note.xor_pitch(pitch_offset))
            .collect();
        group
    }
}

// ---------------------------------------------------------------------------
// Tracks / project
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SVDatabase {
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "language")]
    pub language: String,
    #[serde(rename = "phoneset")]
    pub phoneset: String,
    #[serde(rename = "version", skip_serializing_if = "Option::is_none")]
    pub version: Option<Value>,
    #[serde(
        rename = "languageOverride",
        alias = "language_override",
        skip_serializing_if = "Option::is_none"
    )]
    pub language_override: Option<String>,
    #[serde(
        rename = "phonesetOverride",
        alias = "phoneset_override",
        skip_serializing_if = "Option::is_none"
    )]
    pub phoneset_override: Option<String>,
    #[serde(rename = "backendType", alias = "backend_type")]
    pub backend_type: String,
}

impl Default for SVDatabase {
    fn default() -> Self {
        Self {
            name: String::new(),
            language: "mandarin".to_owned(),
            phoneset: "xsampa".to_owned(),
            version: None,
            language_override: None,
            phoneset_override: None,
            backend_type: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SVAudio {
    #[serde(rename = "filename")]
    pub filename: String,
    #[serde(rename = "duration")]
    pub duration: f64,
    #[serde(rename = "bpm", skip_serializing_if = "Option::is_none")]
    pub bpm: Option<f64>,
    #[serde(
        rename = "alternativeBPMs",
        alias = "alternative_bpms",
        skip_serializing_if = "Option::is_none"
    )]
    pub alternative_bpms: Option<Vec<f64>>,
    #[serde(
        rename = "beatLocations",
        alias = "beat_locations",
        skip_serializing_if = "Option::is_none"
    )]
    pub beat_locations: Option<Vec<f64>>,
}

impl Default for SVAudio {
    fn default() -> Self {
        Self {
            filename: String::new(),
            duration: 0.0,
            bpm: None,
            alternative_bpms: None,
            beat_locations: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SVRef {
    #[serde(rename = "audio", skip_serializing_if = "Option::is_none")]
    pub audio: Option<SVAudio>,
    #[serde(rename = "mute", skip_serializing_if = "Option::is_none")]
    pub mute: Option<bool>,
    #[serde(
        rename = "blickAbsoluteBegin",
        alias = "blick_absolute_begin",
        skip_serializing_if = "Option::is_none"
    )]
    pub blick_absolute_begin: Option<i64>,
    #[serde(
        rename = "blickAbsoluteEnd",
        alias = "blick_absolute_end",
        skip_serializing_if = "Option::is_none"
    )]
    pub blick_absolute_end: Option<i64>,
    #[serde(rename = "blickOffset", alias = "blick_offset")]
    pub blick_offset: i64,
    #[serde(rename = "pitchOffset", alias = "pitch_offset")]
    pub pitch_offset: i64,
    #[serde(rename = "takes", skip_serializing_if = "Option::is_none")]
    pub takes: Option<SVParamTakes>,
    #[serde(
        rename = "pitchTakes",
        alias = "pitch_takes",
        skip_serializing_if = "Option::is_none"
    )]
    pub pitch_takes: Option<SVParamTakes>,
    #[serde(
        rename = "timbreTakes",
        alias = "timbre_takes",
        skip_serializing_if = "Option::is_none"
    )]
    pub timbre_takes: Option<SVParamTakes>,
    #[serde(rename = "database")]
    pub database: SVDatabase,
    #[serde(rename = "dictionary")]
    pub dictionary: String,
    #[serde(rename = "uuid")]
    pub uuid: String,
    #[serde(rename = "voice")]
    pub voice: SVVoice,
    #[serde(
        rename = "voicePresetName",
        alias = "voice_preset_name",
        skip_serializing_if = "Option::is_none"
    )]
    pub voice_preset_name: Option<String>,
    #[serde(rename = "groupID", alias = "group_id")]
    pub group_id: String,
    #[serde(rename = "isInstrumental", alias = "is_instrumental")]
    pub is_instrumental: bool,
    #[serde(rename = "systemPitchDelta", alias = "system_pitch_delta")]
    pub system_pitch_delta: SVParamCurve,
    #[serde(
        rename = "timestampLMR",
        alias = "timestamp_lmr",
        skip_serializing_if = "Option::is_none"
    )]
    pub timestamp_lmr: Option<f64>,
    #[serde(
        rename = "timestampLRSR",
        alias = "timestamp_lrsr",
        skip_serializing_if = "Option::is_none"
    )]
    pub timestamp_lrsr: Option<f64>,
}

impl Default for SVRef {
    fn default() -> Self {
        Self {
            audio: None,
            mute: Some(false),
            blick_absolute_begin: Some(0),
            blick_absolute_end: Some(-1),
            blick_offset: 0,
            pitch_offset: 0,
            takes: None,
            pitch_takes: None,
            timbre_takes: None,
            database: SVDatabase::default(),
            dictionary: String::new(),
            uuid: uuid_str(),
            voice: SVVoice::default(),
            voice_preset_name: None,
            group_id: uuid_str(),
            is_instrumental: false,
            system_pitch_delta: SVParamCurve::default(),
            timestamp_lmr: None,
            timestamp_lrsr: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SVMixer {
    #[serde(rename = "pan")]
    pub pan: f64,
    #[serde(rename = "mute")]
    pub mute: bool,
    #[serde(rename = "solo")]
    pub solo: bool,
    #[serde(rename = "display")]
    pub display: bool,
    #[serde(rename = "gainDecibel", alias = "gain_decibel")]
    pub gain_decibel: f64,
    #[serde(
        rename = "fxPresetName",
        alias = "fx_preset_name",
        skip_serializing_if = "Option::is_none"
    )]
    pub fx_preset_name: Option<String>,
    #[serde(
        rename = "fxParams",
        alias = "fx_params",
        skip_serializing_if = "Option::is_none"
    )]
    pub fx_params: Option<IndexMap<String, Value>>,
}

impl Default for SVMixer {
    fn default() -> Self {
        Self {
            pan: 0.0,
            mute: false,
            solo: false,
            display: true,
            gain_decibel: 0.0,
            fx_preset_name: None,
            fx_params: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SVTrack {
    #[serde(rename = "dispColor", alias = "disp_color")]
    pub disp_color: String,
    #[serde(rename = "dispOrder", alias = "disp_order")]
    pub disp_order: i64,
    #[serde(rename = "groups")]
    pub groups: Vec<SVRef>,
    #[serde(rename = "mainGroup", alias = "main_group")]
    pub main_group: SVGroup,
    #[serde(rename = "mainRef", alias = "main_ref")]
    pub main_ref: SVRef,
    #[serde(rename = "mixer")]
    pub mixer: SVMixer,
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "renderEnabled", alias = "render_enabled")]
    pub render_enabled: bool,
}

impl Default for SVTrack {
    fn default() -> Self {
        Self {
            disp_color: "ff7db235".to_owned(),
            disp_order: 0,
            groups: Vec::new(),
            main_group: SVGroup::default(),
            main_ref: SVRef::default(),
            mixer: SVMixer::default(),
            name: "Track 1".to_owned(),
            render_enabled: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SVMeter {
    #[serde(rename = "index")]
    pub index: i64,
    #[serde(rename = "denominator")]
    pub denominator: i64,
    #[serde(rename = "numerator")]
    pub numerator: i64,
}

impl Default for SVMeter {
    fn default() -> Self {
        Self {
            index: 0,
            denominator: 4,
            numerator: 4,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SVTempo {
    #[serde(rename = "bpm")]
    pub bpm: f64,
    #[serde(rename = "position")]
    pub position: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SVTime {
    #[serde(rename = "meter")]
    pub meter: Vec<SVMeter>,
    #[serde(rename = "tempo")]
    pub tempo: Vec<SVTempo>,
    #[serde(
        rename = "startTimeSeconds",
        alias = "start_time_seconds",
        skip_serializing_if = "Option::is_none"
    )]
    pub start_time_seconds: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SVRenderConfig {
    #[serde(rename = "aspirationFormat", alias = "aspiration_format")]
    pub aspiration_format: String,
    #[serde(rename = "bitDepth", alias = "bit_depth")]
    pub bit_depth: i64,
    #[serde(rename = "destination")]
    pub destination: String,
    #[serde(rename = "filename")]
    pub filename: String,
    #[serde(rename = "numChannels", alias = "num_channels")]
    pub num_channels: i64,
    #[serde(rename = "sampleRate", alias = "sample_rate")]
    pub sample_rate: i64,
    #[serde(rename = "exportMixDown", alias = "export_mix_down")]
    pub export_mix_down: bool,
    #[serde(
        rename = "exportPitch",
        alias = "export_pitch",
        skip_serializing_if = "Option::is_none"
    )]
    pub export_pitch: Option<bool>,
    #[serde(
        rename = "bypassPan",
        alias = "bypass_pan",
        skip_serializing_if = "Option::is_none"
    )]
    pub bypass_pan: Option<bool>,
    #[serde(
        rename = "bypassGain",
        alias = "bypass_gain",
        skip_serializing_if = "Option::is_none"
    )]
    pub bypass_gain: Option<bool>,
    #[serde(
        rename = "bypassEffects",
        alias = "bypass_effects",
        skip_serializing_if = "Option::is_none"
    )]
    pub bypass_effects: Option<bool>,
}

impl Default for SVRenderConfig {
    fn default() -> Self {
        Self {
            aspiration_format: "noAspiration".to_owned(),
            bit_depth: 16,
            destination: "./".to_owned(),
            filename: "untitled".to_owned(),
            num_channels: 1,
            sample_rate: 44100,
            export_mix_down: true,
            export_pitch: Some(false),
            bypass_pan: Some(false),
            bypass_gain: Some(false),
            bypass_effects: Some(false),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SVProject {
    #[serde(rename = "library")]
    pub library: Vec<SVGroup>,
    #[serde(rename = "renderConfig", alias = "render_config")]
    pub render_config: SVRenderConfig,
    #[serde(
        rename = "instantModeEnabled",
        alias = "instant_mode_enabled",
        skip_serializing_if = "Option::is_none"
    )]
    pub instant_mode_enabled: Option<bool>,
    #[serde(rename = "time")]
    pub time_sig: SVTime,
    #[serde(rename = "tracks")]
    pub tracks: Vec<SVTrack>,
    #[serde(rename = "version")]
    pub version: i64,
    #[serde(rename = "uuid")]
    pub uuid: String,
    #[serde(
        rename = "projectMixer",
        alias = "project_mixer",
        skip_serializing_if = "Option::is_none"
    )]
    pub project_mixer: Option<IndexMap<String, Value>>,
}

impl Default for SVProject {
    fn default() -> Self {
        Self {
            library: Vec::new(),
            render_config: SVRenderConfig::default(),
            instant_mode_enabled: None,
            time_sig: SVTime::default(),
            tracks: Vec::new(),
            version: 100,
            uuid: uuid_str(),
            project_mixer: None,
        }
    }
}

impl SVProject {
    /// Apply the pydantic validators that run on JSON load.
    pub fn post_validate(&mut self) {
        for group in &mut self.library {
            group.drop_negative_notes();
        }
        for track in &mut self.tracks {
            track.main_group.drop_negative_notes();
        }
    }
}

/// Unused helper kept to document the `serialize_points` shape.
#[allow(dead_code)]
fn _serialize_points<S: Serializer>(
    points: &[SVPoint],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serialize_points(points, serializer)
}

/// `SVPoint` itself is never serialised directly by the SVP models (points are
/// flattened), but the generator uses this helper for its internal lists.
pub fn round_curve_value(value: f64) -> f64 {
    py_round(value) as f64
}

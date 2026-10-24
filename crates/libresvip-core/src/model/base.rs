//! Port of `libresvip.model.base`.
//!
//! Every struct keeps the exact field order and the pydantic alias of the
//! Python model, because `Project::model_dump(by_alias=True)` output order is
//! part of the observable behaviour (the JSON plugin writes it verbatim).

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::error::{LibreSvipError, Result};
use crate::model::point::{END_POINT_X, Point};
use crate::model::serde_util::{lenient_bool, lenient_f64, lenient_i64};

/// Default tempo (BPM) used when a format does not provide one.
pub const DEFAULT_BPM: f64 = 120.0;
/// Ticks per quarter note used by the internal model.
pub const TICKS_IN_BEAT: i64 = 480;

// ---------------------------------------------------------------------------
// Tempo / time signature
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SongTempo {
    #[serde(
        rename = "Position",
        alias = "position",
        deserialize_with = "lenient_i64"
    )]
    pub position: i64,
    #[serde(rename = "BPM", alias = "bpm", deserialize_with = "lenient_f64")]
    pub bpm: f64,
}

impl Default for SongTempo {
    fn default() -> Self {
        Self {
            position: 0,
            bpm: DEFAULT_BPM,
        }
    }
}

impl SongTempo {
    pub fn new(position: i64, bpm: f64) -> Self {
        Self { position, bpm }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TimeSignature {
    #[serde(
        rename = "BarIndex",
        alias = "bar_index",
        deserialize_with = "lenient_i64"
    )]
    pub bar_index: i64,
    #[serde(
        rename = "Numerator",
        alias = "numerator",
        deserialize_with = "lenient_i64"
    )]
    pub numerator: i64,
    #[serde(
        rename = "Denominator",
        alias = "denominator",
        deserialize_with = "lenient_i64"
    )]
    pub denominator: i64,
}

impl Default for TimeSignature {
    fn default() -> Self {
        Self {
            bar_index: 0,
            numerator: 4,
            denominator: 4,
        }
    }
}

impl TimeSignature {
    pub fn new(bar_index: i64, numerator: i64, denominator: i64) -> Self {
        Self {
            bar_index,
            numerator,
            denominator,
        }
    }

    /// Length of one bar in ticks.
    pub fn bar_length(&self) -> f64 {
        self.bar_length_with(TICKS_IN_BEAT)
    }

    pub fn bar_length_with(&self, ticks_in_beat: i64) -> f64 {
        ((ticks_in_beat * 4) as f64) * self.numerator as f64 / self.denominator as f64
    }
}

// ---------------------------------------------------------------------------
// Parameter curves
// ---------------------------------------------------------------------------

/// A parameter curve: `{"PointList": [[x, y], ...], "TotalPointsCount": n}`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParamCurve {
    pub points: Vec<Point>,
}

impl ParamCurve {
    pub fn new(points: Vec<Point>) -> Self {
        Self { points }
    }

    pub fn total_points_count(&self) -> usize {
        self.points.len()
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Python's `round()` rounds half to even.
    #[inline]
    fn round_half_even(value: f64) -> i64 {
        value.round_ties_even() as i64
    }

    /// Port of `ParamCurve.reduce_sample_rate`.
    pub fn reduce_sample_rate(&self, interval: i64, interrupt_value: i64) -> ParamCurve {
        if interval <= 0 || self.points.len() <= 1 {
            return self.clone();
        }
        let points = &self.points;
        let mut result: Vec<Point> = Vec::new();
        let mut i = 0usize;
        while i < points.len() {
            let point = points[i];
            if point.y == interrupt_value {
                result.push(point);
                i += 1;
                continue;
            }
            let run_start = i;
            while i < points.len() && points[i].y != interrupt_value {
                i += 1;
            }
            let mut run: &[Point] = &points[run_start..i];
            if run_start > 0 && points[run_start - 1].y == interrupt_value {
                result.push(run[0]);
                run = &run[1..];
            }
            if run.is_empty() {
                continue;
            }
            if run.len() == 1 {
                result.push(run[0]);
                continue;
            }
            let tail_point = run[run.len() - 1];
            let window_points = &run[..run.len() - 1];
            let mut j = 0usize;
            while j < window_points.len() {
                let bucket_start = window_points[j].x;
                let mut bucket = vec![window_points[j]];
                j += 1;
                while j < window_points.len() && window_points[j].x < bucket_start + interval {
                    bucket.push(window_points[j]);
                    j += 1;
                }
                let count = bucket.len() as f64;
                let sum_x: i64 = bucket.iter().map(|p| p.x).sum();
                let sum_y: i64 = bucket.iter().map(|p| p.y).sum();
                result.push(Point::new(
                    Self::round_half_even(sum_x as f64 / count),
                    Self::round_half_even(sum_y as f64 / count),
                ));
            }
            result.push(tail_point);
        }
        ParamCurve::new(result)
    }

    /// Port of `ParamCurve.split_into_segments`, including its quirks.
    pub fn split_into_segments(&self, interrupt_value: i64) -> Vec<Vec<Point>> {
        let end_point_x = END_POINT_X;
        let points = &self.points;
        let mut segments: Vec<Vec<Point>> = Vec::new();
        if points.is_empty() {
            return segments;
        } else if points.len() == 1 {
            let point = points[0];
            if (0..end_point_x).contains(&point.x) && point.y != interrupt_value {
                segments.push(vec![point]);
            }
            return segments;
        }
        let mut buffer: Vec<Point> = Vec::new();
        if interrupt_value != 0 {
            for point in points {
                if point.x >= 0 && point.y < end_point_x {
                    if point.y != interrupt_value {
                        buffer.push(*point);
                    } else if !buffer.is_empty() {
                        segments.push(buffer.clone());
                        buffer.clear();
                    }
                }
            }
        } else {
            for i in 0..points.len() - 1 {
                let current_point = points[i];
                let next_point = points[i + 1];
                if current_point.y != interrupt_value {
                    buffer.push(current_point);
                } else if next_point.y != interrupt_value {
                    if current_point.x >= 0 && (i == 0 || points[i - 1].y != interrupt_value) {
                        buffer.push(current_point);
                    }
                } else if !buffer.is_empty() {
                    segments.push(buffer.clone());
                    buffer.clear();
                }
            }
            let last_point = points[points.len() - 1];
            if last_point.x < end_point_x
                && (last_point.y != interrupt_value || points[points.len() - 2].y != interrupt_value)
            {
                buffer.push(last_point);
            }
        }
        if !buffer.is_empty() {
            segments.push(buffer);
        }
        segments
    }
}

impl Serialize for ParamCurve {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("ParamCurve", 2)?;
        state.serialize_field("PointList", &self.points)?;
        state.serialize_field("TotalPointsCount", &self.points.len())?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for ParamCurve {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct ParamCurveVisitor;

        impl<'de> serde::de::Visitor<'de> for ParamCurveVisitor {
            type Value = ParamCurve;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a ParamCurve object or an array of points")
            }

            fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
                Ok(ParamCurve::default())
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut points = Vec::with_capacity(seq.size_hint().unwrap_or(0));
                while let Some(point) = seq.next_element::<Point>()? {
                    points.push(point);
                }
                Ok(ParamCurve { points })
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut points: Option<Vec<Point>> = None;
                while let Some(key) = map.next_key::<std::borrow::Cow<'de, str>>()? {
                    match &*key {
                        "PointList" | "points" => {
                            // A duplicated key keeps the last occurrence, like
                            // the previous `Value`-based implementation.
                            let parsed = map
                                .next_value::<Vec<Point>>()
                                .map_err(|e| serde::de::Error::custom(format!("invalid PointList: {e}")));
                            points = Some(parsed?);
                        }
                        _ => {
                            map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ParamCurve {
                    points: points.unwrap_or_default(),
                })
            }
        }

        deserializer.deserialize_any(ParamCurveVisitor)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Params {
    #[serde(rename = "Pitch", alias = "pitch")]
    pub pitch: ParamCurve,
    #[serde(rename = "Volume", alias = "volume")]
    pub volume: ParamCurve,
    #[serde(rename = "Breath", alias = "breath")]
    pub breath: ParamCurve,
    #[serde(rename = "Gender", alias = "gender")]
    pub gender: ParamCurve,
    #[serde(rename = "Strength", alias = "strength")]
    pub strength: ParamCurve,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VibratoParam {
    #[serde(
        rename = "StartPercent",
        alias = "start_percent",
        deserialize_with = "lenient_f64"
    )]
    pub start_percent: f64,
    #[serde(
        rename = "EndPercent",
        alias = "end_percent",
        deserialize_with = "lenient_f64"
    )]
    pub end_percent: f64,
    #[serde(
        rename = "IsAntiPhase",
        alias = "is_anti_phase",
        deserialize_with = "lenient_bool"
    )]
    pub is_anti_phase: bool,
    #[serde(rename = "Amplitude", alias = "amplitude")]
    pub amplitude: ParamCurve,
    #[serde(rename = "Frequency", alias = "frequency")]
    pub frequency: ParamCurve,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Phones {
    #[serde(
        rename = "HeadLengthInSecs",
        alias = "head_length_in_secs",
        deserialize_with = "lenient_f64"
    )]
    pub head_length_in_secs: f64,
    #[serde(
        rename = "MidRatioOverTail",
        alias = "mid_ratio_over_tail",
        deserialize_with = "lenient_f64"
    )]
    pub mid_ratio_over_tail: f64,
}

impl Default for Phones {
    fn default() -> Self {
        Self {
            head_length_in_secs: -1.0,
            mid_ratio_over_tail: -1.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Notes
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Note {
    #[serde(
        rename = "StartPos",
        alias = "start_pos",
        deserialize_with = "lenient_i64"
    )]
    pub start_pos: i64,
    #[serde(rename = "Length", alias = "length", deserialize_with = "lenient_i64")]
    pub length: i64,
    #[serde(
        rename = "KeyNumber",
        alias = "key_number",
        deserialize_with = "lenient_i64"
    )]
    pub key_number: i64,
    #[serde(rename = "HeadTag", alias = "head_tag")]
    pub head_tag: Option<String>,
    #[serde(rename = "Lyric", alias = "lyric")]
    pub lyric: String,
    #[serde(rename = "Pronunciation", alias = "pronunciation")]
    pub pronunciation: Option<String>,
    #[serde(rename = "EditedPhones", alias = "edited_phones")]
    pub edited_phones: Option<Phones>,
    #[serde(rename = "Vibrato", alias = "vibrato")]
    pub vibrato: Option<VibratoParam>,
}

impl Default for Note {
    fn default() -> Self {
        Self {
            start_pos: 0,
            length: 0,
            key_number: 0,
            head_tag: None,
            lyric: String::new(),
            pronunciation: None,
            edited_phones: None,
            vibrato: None,
        }
    }
}

impl Note {
    pub fn new(start_pos: i64, length: i64, key_number: i64, lyric: impl Into<String>) -> Self {
        Self {
            start_pos,
            length,
            key_number,
            lyric: lyric.into(),
            ..Default::default()
        }
    }

    #[inline]
    pub fn end_pos(&self) -> i64 {
        self.start_pos + self.length
    }
}

// ---------------------------------------------------------------------------
// Tracks
// ---------------------------------------------------------------------------

pub const TRACK_TYPE_SINGING: &str = "Singing";
pub const TRACK_TYPE_INSTRUMENTAL: &str = "Instrumental";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SingingTrack {
    #[serde(rename = "Title", alias = "title")]
    pub title: String,
    #[serde(rename = "Mute", alias = "mute", deserialize_with = "lenient_bool")]
    pub mute: bool,
    #[serde(rename = "Solo", alias = "solo", deserialize_with = "lenient_bool")]
    pub solo: bool,
    #[serde(rename = "Volume", alias = "volume", deserialize_with = "lenient_f64")]
    pub volume: f64,
    #[serde(rename = "Pan", alias = "pan", deserialize_with = "lenient_f64")]
    pub pan: f64,
    #[serde(rename = "Type", alias = "type_", default = "singing_type")]
    pub type_: String,
    #[serde(rename = "AISingerName", alias = "ai_singer_name")]
    pub ai_singer_name: String,
    #[serde(rename = "ReverbPreset", alias = "reverb_preset")]
    pub reverb_preset: String,
    #[serde(rename = "NoteList", alias = "note_list")]
    pub note_list: Vec<Note>,
    #[serde(rename = "EditedParams", alias = "edited_params")]
    pub edited_params: Params,
}

fn singing_type() -> String {
    TRACK_TYPE_SINGING.to_owned()
}

fn instrumental_type() -> String {
    TRACK_TYPE_INSTRUMENTAL.to_owned()
}

impl Default for SingingTrack {
    fn default() -> Self {
        Self {
            title: String::new(),
            mute: false,
            solo: false,
            volume: 1.0,
            pan: 0.0,
            type_: singing_type(),
            ai_singer_name: String::new(),
            reverb_preset: String::new(),
            note_list: Vec::new(),
            edited_params: Params::default(),
        }
    }
}

impl SingingTrack {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Default::default()
        }
    }

    /// Highest end position over all notes (`0` for an empty track).
    pub fn notes_end_pos(&self) -> i64 {
        self.note_list
            .iter()
            .map(Note::end_pos)
            .max()
            .unwrap_or(0)
    }

    pub fn first_note(&self) -> Option<&Note> {
        self.note_list.first()
    }

    pub fn last_note(&self) -> Option<&Note> {
        self.note_list.last()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InstrumentalTrack {
    #[serde(rename = "Title", alias = "title")]
    pub title: String,
    #[serde(rename = "Mute", alias = "mute", deserialize_with = "lenient_bool")]
    pub mute: bool,
    #[serde(rename = "Solo", alias = "solo", deserialize_with = "lenient_bool")]
    pub solo: bool,
    #[serde(rename = "Volume", alias = "volume", deserialize_with = "lenient_f64")]
    pub volume: f64,
    #[serde(rename = "Pan", alias = "pan", deserialize_with = "lenient_f64")]
    pub pan: f64,
    #[serde(rename = "Type", alias = "type_", default = "instrumental_type")]
    pub type_: String,
    #[serde(rename = "AudioFilePath", alias = "audio_file_path")]
    pub audio_file_path: String,
    #[serde(rename = "Offset", alias = "offset", deserialize_with = "lenient_i64")]
    pub offset: i64,
}

impl Default for InstrumentalTrack {
    fn default() -> Self {
        Self {
            title: String::new(),
            mute: false,
            solo: false,
            volume: 1.0,
            pan: 0.0,
            type_: instrumental_type(),
            audio_file_path: String::new(),
            offset: 0,
        }
    }
}

impl InstrumentalTrack {
    pub fn new(title: impl Into<String>, audio_file_path: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            audio_file_path: audio_file_path.into(),
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Track {
    Singing(SingingTrack),
    Instrumental(InstrumentalTrack),
}

impl Track {
    pub fn singing(title: impl Into<String>) -> Self {
        Track::Singing(SingingTrack::new(title))
    }

    pub fn instrumental(title: impl Into<String>, audio_file_path: impl Into<String>) -> Self {
        Track::Instrumental(InstrumentalTrack::new(title, audio_file_path))
    }

    pub fn is_singing(&self) -> bool {
        matches!(self, Track::Singing(_))
    }

    pub fn as_singing(&self) -> Option<&SingingTrack> {
        match self {
            Track::Singing(track) => Some(track),
            Track::Instrumental(_) => None,
        }
    }

    pub fn as_singing_mut(&mut self) -> Option<&mut SingingTrack> {
        match self {
            Track::Singing(track) => Some(track),
            Track::Instrumental(_) => None,
        }
    }

    pub fn as_instrumental(&self) -> Option<&InstrumentalTrack> {
        match self {
            Track::Instrumental(track) => Some(track),
            Track::Singing(_) => None,
        }
    }

    pub fn as_instrumental_mut(&mut self) -> Option<&mut InstrumentalTrack> {
        match self {
            Track::Instrumental(track) => Some(track),
            Track::Singing(_) => None,
        }
    }

    pub fn title(&self) -> &str {
        match self {
            Track::Singing(track) => &track.title,
            Track::Instrumental(track) => &track.title,
        }
    }

    pub fn set_title(&mut self, value: impl Into<String>) {
        let value = value.into();
        match self {
            Track::Singing(track) => track.title = value,
            Track::Instrumental(track) => track.title = value,
        }
    }

    pub fn mute(&self) -> bool {
        match self {
            Track::Singing(track) => track.mute,
            Track::Instrumental(track) => track.mute,
        }
    }

    pub fn solo(&self) -> bool {
        match self {
            Track::Singing(track) => track.solo,
            Track::Instrumental(track) => track.solo,
        }
    }

    pub fn volume(&self) -> f64 {
        match self {
            Track::Singing(track) => track.volume,
            Track::Instrumental(track) => track.volume,
        }
    }

    pub fn pan(&self) -> f64 {
        match self {
            Track::Singing(track) => track.pan,
            Track::Instrumental(track) => track.pan,
        }
    }
}

impl From<SingingTrack> for Track {
    fn from(track: SingingTrack) -> Self {
        Track::Singing(track)
    }
}

impl From<InstrumentalTrack> for Track {
    fn from(track: InstrumentalTrack) -> Self {
        Track::Instrumental(track)
    }
}

impl Serialize for Track {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        match self {
            Track::Singing(track) => {
                let mut map = serializer.serialize_map(Some(10))?;
                map.serialize_entry("Title", &track.title)?;
                map.serialize_entry("Mute", &track.mute)?;
                map.serialize_entry("Solo", &track.solo)?;
                map.serialize_entry("Volume", &track.volume)?;
                map.serialize_entry("Pan", &track.pan)?;
                map.serialize_entry("Type", TRACK_TYPE_SINGING)?;
                map.serialize_entry("AISingerName", &track.ai_singer_name)?;
                map.serialize_entry("ReverbPreset", &track.reverb_preset)?;
                map.serialize_entry("NoteList", &track.note_list)?;
                map.serialize_entry("EditedParams", &track.edited_params)?;
                map.end()
            }
            Track::Instrumental(track) => {
                let mut map = serializer.serialize_map(Some(8))?;
                map.serialize_entry("Title", &track.title)?;
                map.serialize_entry("Mute", &track.mute)?;
                map.serialize_entry("Solo", &track.solo)?;
                map.serialize_entry("Volume", &track.volume)?;
                map.serialize_entry("Pan", &track.pan)?;
                map.serialize_entry("Type", TRACK_TYPE_INSTRUMENTAL)?;
                map.serialize_entry("AudioFilePath", &track.audio_file_path)?;
                map.serialize_entry("Offset", &track.offset)?;
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for Track {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct TrackVisitor;

        // Seeds applying the same lenient coercions as the derived structs.
        struct LenientBool;
        struct LenientF64;
        struct LenientI64;

        impl<'de> serde::de::DeserializeSeed<'de> for LenientBool {
            type Value = bool;
            fn deserialize<D: Deserializer<'de>>(
                self,
                deserializer: D,
            ) -> std::result::Result<Self::Value, D::Error> {
                lenient_bool(deserializer)
            }
        }
        impl<'de> serde::de::DeserializeSeed<'de> for LenientF64 {
            type Value = f64;
            fn deserialize<D: Deserializer<'de>>(
                self,
                deserializer: D,
            ) -> std::result::Result<Self::Value, D::Error> {
                lenient_f64(deserializer)
            }
        }
        impl<'de> serde::de::DeserializeSeed<'de> for LenientI64 {
            type Value = i64;
            fn deserialize<D: Deserializer<'de>>(
                self,
                deserializer: D,
            ) -> std::result::Result<Self::Value, D::Error> {
                lenient_i64(deserializer)
            }
        }

        impl<'de> serde::de::Visitor<'de> for TrackVisitor {
            type Value = Track;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a track object")
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Track, A::Error> {
                let mut title: Option<String> = None;
                let mut mute: Option<bool> = None;
                let mut solo: Option<bool> = None;
                let mut volume: Option<f64> = None;
                let mut pan: Option<f64> = None;
                let mut type_value: Option<Value> = None;
                // Singing fields.
                let mut ai_singer_name: Option<String> = None;
                let mut reverb_preset: Option<String> = None;
                let mut note_list: Option<Vec<Note>> = None;
                let mut edited_params: Option<Params> = None;
                // Instrumental fields.
                let mut audio_file_path: Option<String> = None;
                let mut offset: Option<i64> = None;
                // Duplicated keys keep the last occurrence, matching the
                // previous `Value`-based implementation.
                while let Some(key) = map.next_key::<std::borrow::Cow<'de, str>>()? {
                    match &*key {
                        "Title" | "title" => title = Some(map.next_value()?),
                        "Mute" | "mute" => mute = Some(map.next_value_seed(LenientBool)?),
                        "Solo" | "solo" => solo = Some(map.next_value_seed(LenientBool)?),
                        "Volume" | "volume" => volume = Some(map.next_value_seed(LenientF64)?),
                        "Pan" | "pan" => pan = Some(map.next_value_seed(LenientF64)?),
                        "Type" | "type_" => type_value = Some(map.next_value()?),
                        "AISingerName" | "ai_singer_name" => {
                            ai_singer_name = Some(map.next_value()?)
                        }
                        "ReverbPreset" | "reverb_preset" => {
                            reverb_preset = Some(map.next_value()?)
                        }
                        "NoteList" | "note_list" => note_list = Some(map.next_value()?),
                        "EditedParams" | "edited_params" => {
                            edited_params = Some(map.next_value()?)
                        }
                        "AudioFilePath" | "audio_file_path" => {
                            audio_file_path = Some(map.next_value()?)
                        }
                        "Offset" | "offset" => offset = Some(map.next_value_seed(LenientI64)?),
                        _ => {
                            map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                let type_value = type_value.ok_or_else(|| serde::de::Error::missing_field("Type"))?;
                let type_name = type_value
                    .as_str()
                    .ok_or_else(|| serde::de::Error::custom("Type must be a string"))?;
                match type_name {
                    TRACK_TYPE_SINGING => Ok(Track::Singing(SingingTrack {
                        title: title.unwrap_or_default(),
                        mute: mute.unwrap_or(false),
                        solo: solo.unwrap_or(false),
                        volume: volume.unwrap_or(1.0),
                        pan: pan.unwrap_or_default(),
                        type_: type_name.to_owned(),
                        ai_singer_name: ai_singer_name.unwrap_or_default(),
                        reverb_preset: reverb_preset.unwrap_or_default(),
                        note_list: note_list.unwrap_or_default(),
                        edited_params: edited_params.unwrap_or_default(),
                    })),
                    TRACK_TYPE_INSTRUMENTAL => Ok(Track::Instrumental(InstrumentalTrack {
                        title: title.unwrap_or_default(),
                        mute: mute.unwrap_or(false),
                        solo: solo.unwrap_or(false),
                        volume: volume.unwrap_or(1.0),
                        pan: pan.unwrap_or_default(),
                        type_: type_name.to_owned(),
                        audio_file_path: audio_file_path.unwrap_or_default(),
                        offset: offset.unwrap_or_default(),
                    })),
                    other => Err(serde::de::Error::custom(format!(
                        "unknown track type: {other}"
                    ))),
                }
            }
        }

        deserializer.deserialize_map(TrackVisitor)
    }
}

// ---------------------------------------------------------------------------
// Project
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    #[serde(rename = "Version", alias = "version")]
    pub version: String,
    #[serde(rename = "SongTempoList", alias = "song_tempo_list")]
    pub song_tempo_list: Vec<SongTempo>,
    #[serde(rename = "TimeSignatureList", alias = "time_signature_list")]
    pub time_signature_list: Vec<TimeSignature>,
    #[serde(rename = "TrackList", alias = "track_list")]
    pub track_list: Vec<Track>,
}

impl Project {
    /// Port of `Project.merge_projects`.
    pub fn merge_projects(projects: Vec<Project>) -> Result<Project> {
        if projects.len() <= 1 {
            return Err(LibreSvipError::NoProjectsToMerge);
        }
        let sample_project = &projects[0];
        let compatible = projects[1..].iter().all(|project| {
            project.time_signature_list == sample_project.time_signature_list
                && project.song_tempo_list == sample_project.song_tempo_list
        });
        if !compatible {
            return Err(LibreSvipError::IncompatibleProjects);
        }
        let mut merged = sample_project.clone();
        merged.track_list = projects
            .into_iter()
            .flat_map(|project| project.track_list)
            .collect();
        Ok(merged)
    }

    /// Port of `Project.split_tracks`.
    pub fn split_tracks(&self, max_track_count: usize) -> Result<Vec<Project>> {
        if !self.track_list.iter().any(Track::is_singing) {
            return Err(LibreSvipError::NoSingingTracks);
        }
        let singing_tracks: Vec<Track> = self
            .track_list
            .iter()
            .filter(|track| track.is_singing())
            .cloned()
            .collect();
        let chunk_size = max_track_count.max(1);
        let mut result = Vec::new();
        for chunk in singing_tracks.chunks(chunk_size) {
            let mut project = self.clone();
            project.track_list = chunk.to_vec();
            result.push(project);
        }
        Ok(result)
    }

    pub fn singing_tracks(&self) -> impl Iterator<Item = &SingingTrack> {
        self.track_list.iter().filter_map(Track::as_singing)
    }

    pub fn singing_tracks_mut(&mut self) -> impl Iterator<Item = &mut SingingTrack> {
        self.track_list.iter_mut().filter_map(Track::as_singing_mut)
    }

    pub fn first_singing_track(&self) -> Option<&SingingTrack> {
        self.singing_tracks().next()
    }

    pub fn first_singing_track_mut(&mut self) -> Option<&mut SingingTrack> {
        self.singing_tracks_mut().next()
    }

    pub fn first_tempo(&self) -> SongTempo {
        self.song_tempo_list
            .first()
            .cloned()
            .unwrap_or_default()
    }

    pub fn first_time_signature(&self) -> TimeSignature {
        self.time_signature_list
            .first()
            .cloned()
            .unwrap_or_default()
    }

    /// Latest end position across every singing track.
    pub fn all_notes_end_pos(&self) -> i64 {
        self.singing_tracks()
            .map(SingingTrack::notes_end_pos)
            .max()
            .unwrap_or(0)
    }

    /// First note start position across every singing track, if any.
    pub fn first_note_start_pos(&self) -> Option<i64> {
        self.singing_tracks()
            .filter_map(|track| track.note_list.first().map(|note| note.start_pos))
            .min()
    }
}

// ---------------------------------------------------------------------------
// Helper: build a ParamCurve from (x, y) pairs, the way plugins do.
// ---------------------------------------------------------------------------

pub fn points_from_pairs<I>(pairs: I) -> Vec<Point>
where
    I: IntoIterator<Item = (i64, i64)>,
{
    pairs.into_iter().map(Point::from).collect()
}

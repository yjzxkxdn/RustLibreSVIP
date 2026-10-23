//! Port of `libresvip.plugins.acep.model`.
//!
//! Every struct keeps the pydantic declaration order, because
//! `AcepProject.model_dump(mode="json", by_alias=True)` is serialised verbatim
//! into the `.acep` file (field order is observable output).
//!
//! pydantic's `BaseModel` is configured with `populate_by_name=True`, so the
//! deserialisers accept both the camelCase alias and the python attribute name
//! where the two differ.

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

use super::interval::RangeInterval;
use super::music_math::{exact_sum, HermiteInterpolator};
use super::singers::{DEFAULT_SEED, DEFAULT_SINGER, DEFAULT_SINGER_ID};

/// `dict[str, Any]` fields (preserve insertion order like python dicts).
pub type JsonMap = Map<String, Value>;

/// The five languages whose lyrics carry a `#<span>` suffix.
pub const LATIN_LYRICS_LANGUAGES: [&str; 5] = ["ENG", "SPA", "POR", "FRA", "ITA"];

pub const LYRICS_LANGUAGE_CHINESE: &str = "CHN";
pub const LYRICS_LANGUAGE_JAPANESE: &str = "JPN";
pub const LYRICS_LANGUAGE_KOREAN: &str = "KOR";

fn default_true() -> Option<bool> {
    Some(true)
}

// ---------------------------------------------------------------------------
// Parameter curves
// ---------------------------------------------------------------------------

/// One `AcepParamCurve`.
///
/// `points` is stored in its flattened wire form (pydantic keeps a list of
/// `AcepAnchorPoint` tuples but serialises `chain.from_iterable`).
#[derive(Clone, Debug, PartialEq)]
pub struct AcepParamCurve {
    pub curve_type: String,
    pub offset: i64,
    pub values: Vec<Option<f64>>,
    pub points: Option<Vec<f64>>,
    pub points_vuv: Option<Vec<f64>>,
}

impl Default for AcepParamCurve {
    fn default() -> Self {
        Self {
            curve_type: "data".to_owned(),
            offset: 0,
            values: Vec::new(),
            points: None,
            points_vuv: None,
        }
    }
}

impl AcepParamCurve {
    pub fn new(offset: i64, values: Vec<Option<f64>>) -> Self {
        Self {
            offset,
            values,
            ..Default::default()
        }
    }

    /// `AcepParamCurve.points2values` — `model_validator(mode="after")`.
    pub fn apply_points(&mut self) {
        if self.curve_type != "anchor" {
            return;
        }
        let Some(points) = self.points.as_ref() else {
            return;
        };
        if points.len() < 2 {
            return;
        }
        let anchors: Vec<(f64, f64)> = points
            .chunks_exact(2)
            .map(|pair| (pair[0], pair[1]))
            .collect();
        if anchors.is_empty() {
            return;
        }
        let first = anchors[0].0;
        let last = anchors[anchors.len() - 1].0;
        let offset = first.floor() as i64;
        let end = last.ceil() as i64;
        let xs: Vec<f64> = (offset..=end).map(|tick| tick as f64).collect();
        let interpolator = HermiteInterpolator::new(anchors);
        self.offset = offset;
        self.values = interpolator.interpolate(&xs).into_iter().map(Some).collect();
    }

    /// `AcepParamCurve.transform`.
    pub fn transform(&self, value_transform: impl Fn(f64) -> f64) -> Self {
        let mut result = self.clone();
        result.values = self
            .values
            .iter()
            .map(|value| value.map(&value_transform))
            .collect();
        result
    }
}

impl Serialize for AcepParamCurve {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("AcepParamCurve", 5)?;
        state.serialize_field("type", &self.curve_type)?;
        state.serialize_field("offset", &self.offset)?;
        state.serialize_field("values", &self.values)?;
        state.serialize_field("points", &self.points)?;
        state.serialize_field("pointsVUV", &self.points_vuv)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for AcepParamCurve {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(default)]
        struct Raw {
            #[serde(rename = "type")]
            curve_type: String,
            offset: i64,
            values: Vec<Option<f64>>,
            points: Option<Vec<f64>>,
            #[serde(rename = "pointsVUV")]
            points_vuv: Option<Vec<f64>>,
        }

        impl Default for Raw {
            fn default() -> Self {
                Self {
                    curve_type: "data".to_owned(),
                    offset: 0,
                    values: Vec::new(),
                    points: None,
                    points_vuv: None,
                }
            }
        }

        let raw = Raw::deserialize(deserializer)?;
        let mut curve = AcepParamCurve {
            curve_type: raw.curve_type,
            offset: raw.offset,
            values: raw.values,
            points: raw.points,
            points_vuv: raw.points_vuv,
        };
        curve.apply_points();
        Ok(curve)
    }
}

/// `AcepParamCurveList` — a bare JSON array of curves.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AcepParamCurveList(pub Vec<AcepParamCurve>);

impl Serialize for AcepParamCurveList {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AcepParamCurveList {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        Ok(Self(Vec::<AcepParamCurve>::deserialize(deserializer)?))
    }
}

impl AcepParamCurveList {
    pub fn new() -> Self {
        Self::default()
    }

    /// `AcepParamCurveList.plus`.
    pub fn plus(
        &self,
        others: Option<&AcepParamCurveList>,
        default_value: f64,
        transform: impl Fn(f64) -> f64,
    ) -> AcepParamCurveList {
        let Some(others) = others else {
            return self.clone();
        };
        if others.0.is_empty() && self.0.is_empty() {
            return self.clone();
        }
        let mut ranges: Vec<(i64, i64)> = Vec::with_capacity(self.0.len() + others.0.len());
        for curve in self.0.iter().chain(others.0.iter()) {
            ranges.push((curve.offset, curve.offset + curve.values.len() as i64));
        }
        let result_ranges = RangeInterval::new(&ranges).sub_ranges();
        let mut result = AcepParamCurveList::default();
        for (start, end) in result_ranges {
            let mut values = vec![0.0f64; (end - start) as usize];
            for curve in self.0.iter() {
                if start <= curve.offset && curve.offset < end {
                    let mut index = (curve.offset - start) as usize;
                    for value in curve.values.iter() {
                        if index >= values.len() {
                            break;
                        }
                        if let Some(value) = value {
                            values[index] = *value;
                        }
                        index += 1;
                    }
                }
            }
            for curve in others.0.iter() {
                if start <= curve.offset && curve.offset < end {
                    let mut index = (curve.offset - start) as usize;
                    for value in curve.values.iter() {
                        if index >= values.len() {
                            break;
                        }
                        if values[index] == 0.0 {
                            values[index] = default_value;
                        }
                        if let Some(value) = value {
                            values[index] += transform(*value);
                        }
                        index += 1;
                    }
                }
            }
            result.0.push(AcepParamCurve::new(
                start,
                values.into_iter().map(Some).collect(),
            ));
        }
        result
    }

    /// `AcepParamCurveList.exclude`.
    pub fn exclude(&self, predicate: impl Fn(f64) -> bool) -> AcepParamCurveList {
        let mut result = AcepParamCurveList::default();
        for curve in self.0.iter() {
            let mut buffer: Vec<Option<f64>> = Vec::new();
            let mut pos = curve.offset;
            for value in curve.values.iter() {
                pos += 1;
                let matches = value.map(&predicate).unwrap_or(false);
                if matches {
                    if !buffer.is_empty() {
                        result.0.push(AcepParamCurve::new(
                            pos - buffer.len() as i64,
                            std::mem::take(&mut buffer),
                        ));
                    }
                } else {
                    buffer.push(*value);
                }
            }
            if !buffer.is_empty() {
                result.0.push(AcepParamCurve::new(
                    pos - buffer.len() as i64,
                    std::mem::take(&mut buffer),
                ));
            }
        }
        result
    }

    /// `AcepParamCurveList.z_score_normalize`.
    ///
    /// `statistics.mean` / `statistics.stdev` sum with `math.fsum`, so the
    /// exact (correctly rounded) sums are used here as well.
    pub fn z_score_normalize(&self, d: f64, b: f64) -> AcepParamCurveList {
        if self.0.is_empty() {
            return self.clone();
        }
        let points: Vec<f64> = self
            .0
            .iter()
            .flat_map(|curve| curve.values.iter().flatten().copied())
            .collect();
        if points.is_empty() {
            return self.clone();
        }
        let count = points.len() as f64;
        let mean = exact_sum(points.iter().copied()) / count;
        let sigma = if points.len() > 1 {
            let squared: Vec<f64> = points.iter().map(|p| (p - mean) * (p - mean)).collect();
            (exact_sum(squared) / (count - 1.0)).sqrt()
        } else {
            0.0
        };
        AcepParamCurveList(
            self.0
                .iter()
                .map(|curve| curve.transform(|x| (x - mean) / sigma * d + b))
                .collect(),
        )
    }

    /// `AcepParamCurveList.minmax_normalize`.
    pub fn minmax_normalize(&self, r: f64, b: f64) -> AcepParamCurveList {
        if self.0.is_empty() {
            return self.clone();
        }
        let mut min_value = f64::INFINITY;
        let mut max_value = f64::NEG_INFINITY;
        let mut any = false;
        for value in self.0.iter().flat_map(|curve| curve.values.iter().flatten()) {
            any = true;
            if *value < min_value {
                min_value = *value;
            }
            if *value > max_value {
                max_value = *value;
            }
        }
        if !any {
            min_value = 0.0;
            max_value = 0.0;
        }
        if (max_value - min_value).abs() > 1e-3 {
            AcepParamCurveList(
                self.0
                    .iter()
                    .map(|curve| {
                        curve.transform(|x| {
                            r * (2.0 * (x - min_value) / (max_value - min_value) - 1.0) + b
                        })
                    })
                    .collect(),
            )
        } else {
            AcepParamCurveList(
                self.0
                    .iter()
                    .map(|curve| curve.transform(|_| 0.0))
                    .collect(),
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Notes
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AcepVibrato {
    pub start: f64,
    pub amplitude: f64,
    pub frequency: f64,
    #[serde(rename = "attackLen")]
    pub attack_len: f64,
    #[serde(rename = "releaseLen")]
    pub release_len: f64,
    #[serde(rename = "releaseVol")]
    pub release_vol: f64,
    pub phase: f64,
    #[serde(rename = "startPos")]
    pub start_pos: f64,
    #[serde(rename = "releaseLevel")]
    pub release_level: f64,
    #[serde(rename = "releaseRatio")]
    pub release_ratio: f64,
    #[serde(rename = "attackLevel")]
    pub attack_level: f64,
    #[serde(rename = "attackRatio")]
    pub attack_ratio: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AcepNote {
    pub pos: i64,
    pub dur: i64,
    pub pitch: i64,
    pub uuid: Option<String>,
    pub language: String,
    pub lyric: String,
    pub pronunciation: Option<String>,
    #[serde(rename = "freezedDefaultSyllable")]
    pub freezed_default_syllable: Option<String>,
    #[serde(rename = "newLine")]
    pub new_line: bool,
    #[serde(rename = "consonantLen")]
    pub consonant_len: Option<i64>,
    #[serde(rename = "headConsonants")]
    pub head_consonants: Option<Vec<f64>>,
    #[serde(rename = "tailConsonants")]
    pub tail_consonants: Option<Vec<f64>>,
    pub syllable: String,
    #[serde(rename = "brLen")]
    pub br_len: f64,
    pub vibrato: Option<AcepVibrato>,
    #[serde(rename = "extraInfo")]
    pub extra_info: JsonMap,
}

impl Default for AcepNote {
    fn default() -> Self {
        Self {
            pos: 0,
            dur: 0,
            pitch: 0,
            uuid: None,
            language: LYRICS_LANGUAGE_CHINESE.to_owned(),
            lyric: String::new(),
            pronunciation: None,
            freezed_default_syllable: None,
            new_line: false,
            consonant_len: None,
            head_consonants: Some(Vec::new()),
            tail_consonants: Some(Vec::new()),
            syllable: String::new(),
            br_len: 0.0,
            vibrato: None,
            extra_info: JsonMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Patterns
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AcepPattern {
    pub name: String,
    pub pos: f64,
    pub dur: f64,
    pub uuid: Option<String>,
    #[serde(rename = "clipPos")]
    pub clip_pos: f64,
    #[serde(rename = "clipDur")]
    pub clip_dur: f64,
    pub enabled: Option<bool>,
    pub color: Option<String>,
    #[serde(rename = "extraInfo")]
    pub extra_info: JsonMap,
}

impl Default for AcepPattern {
    fn default() -> Self {
        Self {
            name: String::new(),
            pos: 0.0,
            dur: 0.0,
            uuid: None,
            clip_pos: 0.0,
            clip_dur: 0.0,
            enabled: default_true(),
            color: None,
            extra_info: JsonMap::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AcepAnalysedBeat {
    #[serde(rename = "beatTimes")]
    pub beat_times: Vec<f64>,
    pub length: f64,
    pub offset: i64,
    pub scales: Vec<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AcepAudioFadeShape {
    #[serde(rename = "offsetX")]
    pub offset_x: f64,
    #[serde(rename = "offsetY")]
    pub offset_y: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AcepAudioFadeEffect {
    pub length: f64,
    pub crossfade: Option<bool>,
    pub shape: AcepAudioFadeShape,
}

impl Default for AcepAudioFadeEffect {
    fn default() -> Self {
        Self {
            length: 0.0,
            crossfade: Some(false),
            shape: AcepAudioFadeShape::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AcepAudioPattern {
    #[serde(flatten)]
    pub base: AcepPattern,
    pub path: String,
    pub gain: Option<f64>,
    #[serde(rename = "analyzedBeat")]
    pub analyzed_beat: Option<AcepAnalysedBeat>,
    #[serde(rename = "timeUnit")]
    pub time_unit: Option<String>,
    #[serde(rename = "fadeIn")]
    pub fade_in: Option<AcepAudioFadeEffect>,
    #[serde(rename = "fadeOut")]
    pub fade_out: Option<AcepAudioFadeEffect>,
}

impl Default for AcepAudioPattern {
    fn default() -> Self {
        Self {
            base: AcepPattern::default(),
            path: String::new(),
            gain: None,
            analyzed_beat: None,
            time_unit: Some("sec".to_owned()),
            fade_in: None,
            fade_out: None,
        }
    }
}

/// `AcepParams`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AcepParams {
    #[serde(rename = "pitchDelta")]
    pub pitch_delta: AcepParamCurveList,
    pub energy: AcepParamCurveList,
    pub breathiness: AcepParamCurveList,
    pub tension: AcepParamCurveList,
    pub falsetto: AcepParamCurveList,
    pub gender: AcepParamCurveList,
    #[serde(rename = "realEnergy")]
    pub real_energy: AcepParamCurveList,
    #[serde(rename = "realBreathiness")]
    pub real_breathiness: AcepParamCurveList,
    #[serde(rename = "realTension")]
    pub real_tension: AcepParamCurveList,
    #[serde(rename = "realFalsetto")]
    pub real_falsetto: AcepParamCurveList,
    pub vuv: Option<AcepParamCurveList>,
}

impl Default for AcepParams {
    fn default() -> Self {
        Self {
            pitch_delta: AcepParamCurveList::new(),
            energy: AcepParamCurveList::new(),
            breathiness: AcepParamCurveList::new(),
            tension: AcepParamCurveList::new(),
            falsetto: AcepParamCurveList::new(),
            gender: AcepParamCurveList::new(),
            real_energy: AcepParamCurveList::new(),
            real_breathiness: AcepParamCurveList::new(),
            real_tension: AcepParamCurveList::new(),
            real_falsetto: AcepParamCurveList::new(),
            vuv: Some(AcepParamCurveList::new()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AcepVocalPattern {
    #[serde(flatten)]
    pub base: AcepPattern,
    pub language: String,
    #[serde(rename = "extendLyrics")]
    pub extend_lyrics: String,
    pub notes: Vec<AcepNote>,
    #[serde(rename = "timeUnit")]
    pub time_unit: Option<String>,
    pub parameters: AcepParams,
    #[serde(rename = "fadeIn")]
    pub fade_in: Option<JsonMap>,
    #[serde(rename = "fadeOut")]
    pub fade_out: Option<JsonMap>,
    #[serde(rename = "vocalControls")]
    pub vocal_controls: Option<JsonMap>,
}

impl Default for AcepVocalPattern {
    fn default() -> Self {
        Self {
            base: AcepPattern::default(),
            language: LYRICS_LANGUAGE_CHINESE.to_owned(),
            extend_lyrics: String::new(),
            notes: Vec::new(),
            time_unit: Some("tick".to_owned()),
            parameters: AcepParams::default(),
            fade_in: None,
            fade_out: None,
            vocal_controls: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AcepInstrumentPattern {
    #[serde(flatten)]
    pub base: AcepPattern,
    pub notes: Vec<AcepNote>,
    #[serde(rename = "timeUnit")]
    pub time_unit: Option<String>,
}

impl Default for AcepInstrumentPattern {
    fn default() -> Self {
        Self {
            base: AcepPattern::default(),
            notes: Vec::new(),
            time_unit: Some("tick".to_owned()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AcepChord {
    pub addeds: Vec<String>,
    pub bass: i64,
    pub dur: i64,
    pub root: i64,
    #[serde(rename = "type")]
    pub type_: String,
}

impl Default for AcepChord {
    fn default() -> Self {
        Self {
            addeds: Vec::new(),
            bass: 0,
            dur: 0,
            root: -1,
            type_: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AcepChordPattern {
    #[serde(flatten)]
    pub base: AcepPattern,
    pub chords: Vec<AcepChord>,
    #[serde(rename = "timeUnit")]
    pub time_unit: Option<String>,
}

impl Default for AcepChordPattern {
    fn default() -> Self {
        Self {
            base: AcepPattern::default(),
            chords: Vec::new(),
            time_unit: Some("tick".to_owned()),
        }
    }
}

// ---------------------------------------------------------------------------
// Tracks
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AcepTrackProperties {
    pub gain: f64,
    #[serde(rename = "externalFxChain")]
    pub external_fx_chain: Vec<Value>,
    pub name: String,
    pub color: String,
    pub pan: f64,
    pub mute: bool,
    pub solo: bool,
    pub record: bool,
    pub channel: Option<i64>,
    pub listen: Option<bool>,
    pub uuid: Option<String>,
    #[serde(rename = "extraInfo")]
    pub extra_info: JsonMap,
    #[serde(rename = "builtInFx")]
    pub built_in_fx: JsonMap,
    #[serde(rename = "inputSource")]
    pub input_source: Option<JsonMap>,
}

impl Default for AcepTrackProperties {
    fn default() -> Self {
        Self {
            gain: 0.0,
            external_fx_chain: Vec::new(),
            name: String::new(),
            color: "#91bcdc".to_owned(),
            pan: 0.0,
            mute: false,
            solo: false,
            record: false,
            channel: Some(0),
            listen: Some(false),
            uuid: None,
            extra_info: JsonMap::new(),
            built_in_fx: JsonMap::new(),
            input_source: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AcepEmptyTrack {
    #[serde(flatten)]
    pub base: AcepTrackProperties,
    #[serde(rename = "type")]
    pub type_: String,
}

impl Default for AcepEmptyTrack {
    fn default() -> Self {
        Self {
            base: AcepTrackProperties::default(),
            type_: "empty".to_owned(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AcepAudioTrack {
    #[serde(flatten)]
    pub base: AcepTrackProperties,
    #[serde(rename = "type")]
    pub type_: String,
    pub patterns: Vec<AcepAudioPattern>,
    #[serde(rename = "inputChannelIndex")]
    pub input_channel_index: Option<i64>,
}

impl Default for AcepAudioTrack {
    fn default() -> Self {
        Self {
            base: AcepTrackProperties::default(),
            type_: "audio".to_owned(),
            patterns: Vec::new(),
            input_channel_index: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AcepInstrumentTrack {
    #[serde(flatten)]
    pub base: AcepTrackProperties,
    #[serde(rename = "type")]
    pub type_: String,
    pub patterns: Vec<AcepInstrumentPattern>,
    pub instruments: Vec<JsonMap>,
    #[serde(rename = "ensembleInfo")]
    pub ensemble_info: JsonMap,
}

impl Default for AcepInstrumentTrack {
    fn default() -> Self {
        Self {
            base: AcepTrackProperties::default(),
            type_: "instrument".to_owned(),
            patterns: Vec::new(),
            instruments: Vec::new(),
            ensemble_info: JsonMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AcepChordTrack {
    #[serde(flatten)]
    pub base: AcepTrackProperties,
    #[serde(rename = "type")]
    pub type_: String,
    pub patterns: Vec<AcepChordPattern>,
}

impl Default for AcepChordTrack {
    fn default() -> Self {
        Self {
            base: AcepTrackProperties::default(),
            type_: "chord".to_owned(),
            patterns: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AcepSeedComposition {
    pub code: i64,
    pub lock: bool,
    pub style: f64,
    pub timbre: f64,
}

impl Default for AcepSeedComposition {
    fn default() -> Self {
        Self {
            code: DEFAULT_SEED,
            lock: true,
            style: 1.0,
            timbre: 1.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AcepCustomSinger {
    pub composition: Vec<AcepSeedComposition>,
    pub state: String,
    pub name: String,
    #[serde(rename = "id")]
    pub singer_id: Option<i64>,
    pub head: Option<i64>,
    pub router: Option<i64>,
    pub group: Option<String>,
}

impl Default for AcepCustomSinger {
    fn default() -> Self {
        Self {
            composition: Vec::new(),
            state: "Unmixed".to_owned(),
            name: DEFAULT_SINGER.to_owned(),
            singer_id: Some(DEFAULT_SINGER_ID),
            head: Some(-1),
            router: Some(1),
            group: Some(String::new()),
        }
    }
}

/// `int | AcepCustomSinger | None`.
#[derive(Clone, Debug, PartialEq)]
pub enum AcepSingerRef {
    Id(i64),
    Custom(AcepCustomSinger),
}

impl Serialize for AcepSingerRef {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            AcepSingerRef::Id(value) => value.serialize(serializer),
            AcepSingerRef::Custom(value) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for AcepSingerRef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        match value {
            Value::Number(number) => number
                .as_i64()
                .map(AcepSingerRef::Id)
                .ok_or_else(|| D::Error::custom("singer id must be an integer")),
            Value::Object(_) => serde_json::from_value(value)
                .map(AcepSingerRef::Custom)
                .map_err(|error| D::Error::custom(format!("invalid singer: {error}"))),
            other => Err(D::Error::custom(format!(
                "unexpected singer value: {other}"
            ))),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AcepSingerConfig {
    pub singer: AcepCustomSinger,
    pub gain: f64,
    pub mute: bool,
    #[serde(rename = "randomSeed")]
    pub random_seed: i64,
}

impl Default for AcepSingerConfig {
    fn default() -> Self {
        Self {
            singer: AcepCustomSinger::default(),
            gain: 0.0,
            mute: false,
            random_seed: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AcepVocalTrack {
    #[serde(flatten)]
    pub base: AcepTrackProperties,
    #[serde(rename = "type")]
    pub type_: String,
    pub singer: Option<AcepSingerRef>,
    pub language: String,
    pub patterns: Vec<AcepVocalPattern>,
    #[serde(rename = "choirInfo")]
    pub choir_info: JsonMap,
    #[serde(rename = "choirConfig")]
    pub choir_config: JsonMap,
    #[serde(rename = "roomEffect")]
    pub room_effect: Option<JsonMap>,
    pub singers: Vec<AcepSingerConfig>,
    #[serde(rename = "recordMode")]
    pub record_mode: Option<String>,
    #[serde(rename = "soundSourceMetadata")]
    pub sound_source_metadata: Option<JsonMap>,
}

impl Default for AcepVocalTrack {
    fn default() -> Self {
        Self {
            base: AcepTrackProperties::default(),
            type_: "sing".to_owned(),
            singer: None,
            language: LYRICS_LANGUAGE_CHINESE.to_owned(),
            patterns: Vec::new(),
            choir_info: JsonMap::new(),
            choir_config: JsonMap::new(),
            room_effect: None,
            singers: Vec::new(),
            record_mode: None,
            sound_source_metadata: None,
        }
    }
}

impl AcepVocalTrack {
    /// `AcepVocalTrack.migrate_singer_attr` — `model_validator(mode="after")`.
    pub fn migrate_singer_attr(&mut self) {
        if let Some(singer) = &self.singer {
            let config = match singer {
                AcepSingerRef::Id(id) => AcepSingerConfig {
                    singer: AcepCustomSinger {
                        singer_id: Some(*id),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                AcepSingerRef::Custom(custom) => AcepSingerConfig {
                    singer: custom.clone(),
                    ..Default::default()
                },
            };
            self.singers.push(config);
        }
    }

    /// `AcepVocalTrack.__len__` — the pattern extent in ticks.
    pub fn length(&self) -> i64 {
        let Some(last) = self.patterns.last() else {
            return 0;
        };
        (last.base.pos + last.base.clip_dur - last.base.clip_pos) as i64
    }
}

/// `AcepTrack = Annotated[Audio | Empty | Vocal | Chord | Instrument, Field(discriminator="type_")]`.
#[derive(Clone, Debug, PartialEq)]
pub enum AcepTrack {
    Audio(AcepAudioTrack),
    Empty(AcepEmptyTrack),
    Vocal(AcepVocalTrack),
    Chord(AcepChordTrack),
    Instrument(AcepInstrumentTrack),
}

impl AcepTrack {
    pub fn properties(&self) -> &AcepTrackProperties {
        match self {
            AcepTrack::Audio(track) => &track.base,
            AcepTrack::Empty(track) => &track.base,
            AcepTrack::Vocal(track) => &track.base,
            AcepTrack::Chord(track) => &track.base,
            AcepTrack::Instrument(track) => &track.base,
        }
    }

    pub fn properties_mut(&mut self) -> &mut AcepTrackProperties {
        match self {
            AcepTrack::Audio(track) => &mut track.base,
            AcepTrack::Empty(track) => &mut track.base,
            AcepTrack::Vocal(track) => &mut track.base,
            AcepTrack::Chord(track) => &mut track.base,
            AcepTrack::Instrument(track) => &mut track.base,
        }
    }

    /// `isinstance(track, AcepVocalTrack)`.
    pub fn as_vocal(&self) -> Option<&AcepVocalTrack> {
        match self {
            AcepTrack::Vocal(track) => Some(track),
            _ => None,
        }
    }

    /// `isinstance(track, AcepAudioTrack)`.
    pub fn as_audio(&self) -> Option<&AcepAudioTrack> {
        match self {
            AcepTrack::Audio(track) => Some(track),
            _ => None,
        }
    }
}

impl Serialize for AcepTrack {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            AcepTrack::Audio(track) => track.serialize(serializer),
            AcepTrack::Empty(track) => track.serialize(serializer),
            AcepTrack::Vocal(track) => track.serialize(serializer),
            AcepTrack::Chord(track) => track.serialize(serializer),
            AcepTrack::Instrument(track) => track.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for AcepTrack {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| D::Error::missing_field("type"))?
            .to_owned();
        match kind.as_str() {
            "audio" => serde_json::from_value(value)
                .map(AcepTrack::Audio)
                .map_err(|error| D::Error::custom(format!("invalid audio track: {error}"))),
            "empty" => serde_json::from_value(value)
                .map(AcepTrack::Empty)
                .map_err(|error| D::Error::custom(format!("invalid empty track: {error}"))),
            "sing" => serde_json::from_value(value)
                .map(|mut track: AcepVocalTrack| {
                    track.migrate_singer_attr();
                    AcepTrack::Vocal(track)
                })
                .map_err(|error| D::Error::custom(format!("invalid vocal track: {error}"))),
            "chord" => serde_json::from_value(value)
                .map(AcepTrack::Chord)
                .map_err(|error| D::Error::custom(format!("invalid chord track: {error}"))),
            "instrument" => serde_json::from_value(value)
                .map(AcepTrack::Instrument)
                .map_err(|error| D::Error::custom(format!("invalid instrument track: {error}"))),
            other => Err(D::Error::custom(format!("unknown track type: {other}"))),
        }
    }
}

// ---------------------------------------------------------------------------
// Project
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AcepTempo {
    pub bpm: f64,
    pub position: i64,
    #[serde(rename = "isLerp")]
    pub is_lerp: Option<bool>,
    pub bend: Option<f64>,
}

impl Default for AcepTempo {
    fn default() -> Self {
        Self {
            bpm: 0.0,
            position: 0,
            is_lerp: Some(false),
            bend: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AcepTimeSignature {
    #[serde(rename = "barPos")]
    pub bar_pos: i64,
    pub numerator: i64,
    pub denominator: i64,
}

impl Default for AcepTimeSignature {
    fn default() -> Self {
        Self {
            bar_pos: 0,
            numerator: 4,
            denominator: 4,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AcepLoop {
    pub active: bool,
    pub end: i64,
    pub start: i64,
    pub valid: bool,
}

impl Default for AcepLoop {
    fn default() -> Self {
        Self {
            active: false,
            end: 0,
            start: 0,
            valid: false,
        }
    }
}

/// `bool | AcepLoop | None` (the `loop` field).
#[derive(Clone, Debug, PartialEq)]
pub enum LoopValue {
    Bool(bool),
    Loop(AcepLoop),
}

impl Serialize for LoopValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            LoopValue::Bool(value) => value.serialize(serializer),
            LoopValue::Loop(value) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for LoopValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        match value {
            Value::Bool(value) => Ok(LoopValue::Bool(value)),
            Value::Object(_) => serde_json::from_value(value)
                .map(LoopValue::Loop)
                .map_err(|error| D::Error::custom(format!("invalid loop: {error}"))),
            other => Err(D::Error::custom(format!("unexpected loop value: {other}"))),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AcepMaster {
    pub gain: f64,
    #[serde(rename = "externalFxChain")]
    pub external_fx_chain: Vec<Value>,
}

impl Default for AcepMaster {
    fn default() -> Self {
        Self {
            gain: 0.0,
            external_fx_chain: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AcepProject {
    pub beats_per_bar: i64,
    pub chors_track: Option<JsonMap>,
    pub color_index: i64,
    pub pattern_individual_color_index: Option<i64>,
    pub debug_info: JsonMap,
    pub duration: i64,
    pub extra_info: JsonMap,
    pub master: AcepMaster,
    pub piano_cells: i64,
    pub tempo_brush_on: Option<bool>,
    pub tempos: Vec<AcepTempo>,
    pub max_bpm: Option<f64>,
    pub min_bpm: Option<f64>,
    pub track_cells: i64,
    pub tracks: Vec<AcepTrack>,
    pub loop_: Option<LoopValue>,
    pub loop_start: Option<i64>,
    pub loop_end: Option<i64>,
    pub version: i64,
    pub version_revision: Option<i64>,
    pub merged_pattern_index: i64,
    pub record_pattern_index: i64,
    pub singer_library_id: Option<String>,
    pub time_signatures: Vec<AcepTimeSignature>,
    pub track_control_panel_w: Option<i64>,
    pub svc_results: Vec<JsonMap>,
    pub piano_display_config: Option<JsonMap>,
    pub tempo_track_height: Option<i64>,
    pub video_track: Option<JsonMap>,
    pub video_tracks: Vec<JsonMap>,
    pub marker_tracks: Vec<JsonMap>,
    pub canvas: Option<JsonMap>,
}

impl Default for AcepProject {
    /// `AcepProject()` — the constructor migration appends the default time
    /// signature because `time_signatures` starts out empty.
    fn default() -> Self {
        let beats_per_bar = 4;
        Self {
            beats_per_bar,
            chors_track: None,
            color_index: 0,
            pattern_individual_color_index: Some(0),
            debug_info: JsonMap::new(),
            duration: 0,
            extra_info: JsonMap::new(),
            master: AcepMaster::default(),
            piano_cells: 2_147_483_646,
            tempo_brush_on: Some(false),
            tempos: Vec::new(),
            max_bpm: None,
            min_bpm: None,
            track_cells: 2_147_483_646,
            tracks: Vec::new(),
            loop_: Some(LoopValue::Bool(false)),
            loop_start: Some(0),
            loop_end: Some(7680),
            version: 9,
            version_revision: Some(0),
            merged_pattern_index: 0,
            record_pattern_index: 0,
            singer_library_id: Some("1200593006".to_owned()),
            time_signatures: vec![AcepTimeSignature {
                bar_pos: 0,
                numerator: beats_per_bar,
                denominator: 4,
            }],
            track_control_panel_w: Some(0),
            svc_results: Vec::new(),
            piano_display_config: None,
            tempo_track_height: Some(0),
            video_track: None,
            video_tracks: Vec::new(),
            marker_tracks: Vec::new(),
            canvas: None,
        }
    }
}

impl AcepProject {
    /// `AcepProject.migrate_time_signatures` — `model_validator(mode="after")`.
    pub fn migrate_time_signatures(&mut self) {
        if self.time_signatures.is_empty() {
            self.time_signatures.push(AcepTimeSignature {
                bar_pos: 0,
                numerator: self.beats_per_bar,
                denominator: 4,
            });
        }
        if let Some(video_track) = self.video_track.take() {
            self.video_tracks.push(video_track);
        }
    }
}

impl Serialize for AcepProject {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("AcepProject", 32)?;
        state.serialize_field("beatsPerBar", &self.beats_per_bar)?;
        state.serialize_field("chordTrack", &self.chors_track)?;
        state.serialize_field("colorIndex", &self.color_index)?;
        state.serialize_field(
            "patternIndividualColorIndex",
            &self.pattern_individual_color_index,
        )?;
        state.serialize_field("debugInfo", &self.debug_info)?;
        state.serialize_field("duration", &self.duration)?;
        state.serialize_field("extraInfo", &self.extra_info)?;
        state.serialize_field("master", &self.master)?;
        state.serialize_field("pianoCells", &self.piano_cells)?;
        state.serialize_field("tempoBrushOn", &self.tempo_brush_on)?;
        state.serialize_field("tempos", &self.tempos)?;
        state.serialize_field("maxBpm", &self.max_bpm)?;
        state.serialize_field("minBpm", &self.min_bpm)?;
        state.serialize_field("trackCells", &self.track_cells)?;
        state.serialize_field("tracks", &self.tracks)?;
        state.serialize_field("loop", &self.loop_)?;
        state.serialize_field("loopStart", &self.loop_start)?;
        state.serialize_field("loopEnd", &self.loop_end)?;
        state.serialize_field("version", &self.version)?;
        state.serialize_field("versionRevision", &self.version_revision)?;
        state.serialize_field("mergedPatternIndex", &self.merged_pattern_index)?;
        state.serialize_field("recordPatternIndex", &self.record_pattern_index)?;
        state.serialize_field("singer_library_id", &self.singer_library_id)?;
        state.serialize_field("timeSignatures", &self.time_signatures)?;
        state.serialize_field("trackControlPanelW", &self.track_control_panel_w)?;
        state.serialize_field("svcResults", &self.svc_results)?;
        state.serialize_field("pianoDisplayConfig", &self.piano_display_config)?;
        state.serialize_field("tempoTrackHeight", &self.tempo_track_height)?;
        state.serialize_field("videoTrack", &self.video_track)?;
        state.serialize_field("videoTracks", &self.video_tracks)?;
        state.serialize_field("markerTracks", &self.marker_tracks)?;
        state.serialize_field("canvas", &self.canvas)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for AcepProject {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(default)]
        struct Raw {
            #[serde(rename = "beatsPerBar")]
            beats_per_bar: i64,
            #[serde(rename = "chordTrack")]
            chors_track: Option<JsonMap>,
            #[serde(rename = "colorIndex")]
            color_index: i64,
            #[serde(rename = "patternIndividualColorIndex")]
            pattern_individual_color_index: Option<i64>,
            #[serde(rename = "debugInfo")]
            debug_info: JsonMap,
            duration: i64,
            #[serde(rename = "extraInfo")]
            extra_info: JsonMap,
            master: AcepMaster,
            #[serde(rename = "pianoCells")]
            piano_cells: i64,
            #[serde(rename = "tempoBrushOn")]
            tempo_brush_on: Option<bool>,
            tempos: Vec<AcepTempo>,
            #[serde(rename = "maxBpm")]
            max_bpm: Option<f64>,
            #[serde(rename = "minBpm")]
            min_bpm: Option<f64>,
            #[serde(rename = "trackCells")]
            track_cells: i64,
            tracks: Vec<AcepTrack>,
            #[serde(rename = "loop")]
            loop_: Option<LoopValue>,
            #[serde(rename = "loopStart")]
            loop_start: Option<i64>,
            #[serde(rename = "loopEnd")]
            loop_end: Option<i64>,
            version: i64,
            #[serde(rename = "versionRevision")]
            version_revision: Option<i64>,
            #[serde(rename = "mergedPatternIndex")]
            merged_pattern_index: i64,
            #[serde(rename = "recordPatternIndex")]
            record_pattern_index: i64,
            #[serde(rename = "singer_library_id")]
            singer_library_id: Option<String>,
            #[serde(rename = "timeSignatures")]
            time_signatures: Vec<AcepTimeSignature>,
            #[serde(rename = "trackControlPanelW")]
            track_control_panel_w: Option<i64>,
            #[serde(rename = "svcResults")]
            svc_results: Vec<JsonMap>,
            #[serde(rename = "pianoDisplayConfig")]
            piano_display_config: Option<JsonMap>,
            #[serde(rename = "tempoTrackHeight")]
            tempo_track_height: Option<i64>,
            #[serde(rename = "videoTrack")]
            video_track: Option<JsonMap>,
            #[serde(rename = "videoTracks")]
            video_tracks: Vec<JsonMap>,
            #[serde(rename = "markerTracks")]
            marker_tracks: Vec<JsonMap>,
            canvas: Option<JsonMap>,
        }

        impl Default for Raw {
            fn default() -> Self {
                let project = AcepProject::default();
                Self {
                    beats_per_bar: project.beats_per_bar,
                    chors_track: None,
                    color_index: project.color_index,
                    pattern_individual_color_index: project.pattern_individual_color_index,
                    debug_info: JsonMap::new(),
                    duration: 0,
                    extra_info: JsonMap::new(),
                    master: AcepMaster::default(),
                    piano_cells: project.piano_cells,
                    tempo_brush_on: project.tempo_brush_on,
                    tempos: Vec::new(),
                    max_bpm: None,
                    min_bpm: None,
                    track_cells: project.track_cells,
                    tracks: Vec::new(),
                    loop_: project.loop_,
                    loop_start: project.loop_start,
                    loop_end: project.loop_end,
                    version: project.version,
                    version_revision: project.version_revision,
                    merged_pattern_index: 0,
                    record_pattern_index: 0,
                    singer_library_id: project.singer_library_id,
                    // The load validator appends a default only when empty.
                    time_signatures: Vec::new(),
                    track_control_panel_w: project.track_control_panel_w,
                    svc_results: Vec::new(),
                    piano_display_config: None,
                    tempo_track_height: project.tempo_track_height,
                    video_track: None,
                    video_tracks: Vec::new(),
                    marker_tracks: Vec::new(),
                    canvas: None,
                }
            }
        }

        let raw = Raw::deserialize(deserializer)?;
        let mut project = AcepProject {
            beats_per_bar: raw.beats_per_bar,
            chors_track: raw.chors_track,
            color_index: raw.color_index,
            pattern_individual_color_index: raw.pattern_individual_color_index,
            debug_info: raw.debug_info,
            duration: raw.duration,
            extra_info: raw.extra_info,
            master: raw.master,
            piano_cells: raw.piano_cells,
            tempo_brush_on: raw.tempo_brush_on,
            tempos: raw.tempos,
            max_bpm: raw.max_bpm,
            min_bpm: raw.min_bpm,
            track_cells: raw.track_cells,
            tracks: raw.tracks,
            loop_: raw.loop_,
            loop_start: raw.loop_start,
            loop_end: raw.loop_end,
            version: raw.version,
            version_revision: raw.version_revision,
            merged_pattern_index: raw.merged_pattern_index,
            record_pattern_index: raw.record_pattern_index,
            singer_library_id: raw.singer_library_id,
            time_signatures: raw.time_signatures,
            track_control_panel_w: raw.track_control_panel_w,
            svc_results: raw.svc_results,
            piano_display_config: raw.piano_display_config,
            tempo_track_height: raw.tempo_track_height,
            video_track: raw.video_track,
            video_tracks: raw.video_tracks,
            marker_tracks: raw.marker_tracks,
            canvas: raw.canvas,
        };
        project.migrate_time_signatures();
        Ok(project)
    }
}

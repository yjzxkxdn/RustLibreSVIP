//! Port of `libresvip.deprecated.gj.model` (Gejigeji v2.0 JSON project).

use serde::{Deserialize, Serialize};

fn default_gjgj_version() -> Option<i64> {
    Some(2)
}

fn default_key_mode() -> Option<i64> {
    Some(1)
}

fn default_key_type() -> Option<i64> {
    Some(0)
}

fn default_volume() -> Option<f64> {
    Some(1.0)
}

fn default_left_volume() -> Option<f64> {
    Some(1.0)
}

fn default_right_volume() -> Option<f64> {
    Some(1.0)
}

fn default_false() -> Option<bool> {
    Some(false)
}

fn default_no1_key_name() -> Option<String> {
    Some("C".to_owned())
}

fn default_eq_after_mix() -> Option<String> {
    Some("".to_owned())
}

fn default_project_type() -> Option<i64> {
    Some(0)
}

fn default_denominator() -> Option<i64> {
    Some(4)
}

fn default_syn_mode() -> Option<i64> {
    Some(0)
}

fn default_singer_name() -> String {
    String::new()
}

fn default_singer_name_opt() -> Option<String> {
    Some(String::new())
}

fn default_singer_age() -> Option<i64> {
    Some(100)
}

fn default_singer_color() -> Option<i64> {
    Some(0)
}

fn default_eq_program() -> Option<String> {
    Some("无".to_owned())
}

fn default_sort_index() -> Option<i64> {
    Some(0)
}

fn default_velocity() -> Option<i64> {
    Some(127)
}

fn default_track_type() -> Option<i64> {
    Some(0)
}

/// `GjgjBeatStyle` — pydantic serializes int-valued enums as bare numbers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GjgjBeatStyle {
    None,
    Sp,
    Sil,
}

impl Serialize for GjgjBeatStyle {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(self.tag())
    }
}

impl<'de> Deserialize<'de> for GjgjBeatStyle {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl serde::de::Visitor<'_> for V {
            type Value = GjgjBeatStyle;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a beat style tag")
            }
            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<GjgjBeatStyle, E> {
                Ok(match value {
                    0 => GjgjBeatStyle::None,
                    1 => GjgjBeatStyle::Sp,
                    2 => GjgjBeatStyle::Sil,
                    other => return Err(E::custom(format!("unknown beat style: {other}"))),
                })
            }
            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<GjgjBeatStyle, E> {
                self.visit_i64(value as i64)
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<GjgjBeatStyle, E> {
                self.visit_i64(value.parse().map_err(E::custom)?)
            }
        }
        deserializer.deserialize_any(V)
    }
}

impl GjgjBeatStyle {
    /// Numeric tag, for both directions.
    pub const fn tag(self) -> i64 {
        match self {
            GjgjBeatStyle::None => 0,
            GjgjBeatStyle::Sp => 1,
            GjgjBeatStyle::Sil => 2,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct GjgjBeatItems {
    #[serde(rename = "ID")]
    pub id_value: Option<i64>,
    #[serde(rename = "Lyric")]
    pub lyric: String,
    #[serde(rename = "Pinyin")]
    pub pinyin: Option<String>,
    #[serde(rename = "StartTick")]
    pub start_tick: i64,
    #[serde(rename = "Duration")]
    pub duration: i64,
    #[serde(rename = "Track")]
    pub track: Option<i64>,
    #[serde(rename = "PreTime", default)]
    pub pre_time: f64,
    #[serde(rename = "PostTime", default)]
    pub post_time: f64,
    #[serde(rename = "Style")]
    pub style: Option<GjgjBeatStyle>,
    #[serde(rename = "Velocity", default = "default_velocity")]
    pub velocity: Option<i64>,
}

impl Default for GjgjBeatItems {
    fn default() -> Self {
        Self {
            id_value: None,
            lyric: String::new(),
            pinyin: None,
            start_tick: 0,
            duration: 0,
            track: None,
            pre_time: 0.0,
            post_time: 0.0,
            style: None,
            velocity: Some(127),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct GjgjKeyboard {
    #[serde(rename = "KeyMode", default = "default_key_mode")]
    pub key_mode: Option<i64>,
    #[serde(rename = "KeyType", default = "default_key_type")]
    pub key_type: Option<i64>,
}

impl Default for GjgjKeyboard {
    fn default() -> Self {
        Self {
            key_mode: Some(1),
            key_type: Some(0),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct GjgjTrackVolume {
    #[serde(rename = "Volume", default = "default_volume")]
    pub volume: Option<f64>,
    #[serde(rename = "LeftVolume", default = "default_left_volume")]
    pub left_volume: Option<f64>,
    #[serde(rename = "RightVolume", default = "default_right_volume")]
    pub right_volume: Option<f64>,
    #[serde(rename = "Mute", default = "default_false")]
    pub mute: Option<bool>,
}

impl Default for GjgjTrackVolume {
    fn default() -> Self {
        Self {
            volume: Some(1.0),
            left_volume: Some(1.0),
            right_volume: Some(1.0),
            mute: Some(false),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct GjgjProjectSetting {
    #[serde(rename = "No1KeyName", default = "default_no1_key_name")]
    pub no1_key_name: Option<String>,
    #[serde(rename = "EQAfterMix", default = "default_eq_after_mix")]
    pub eq_after_mix: Option<String>,
    #[serde(rename = "ProjectType", default = "default_project_type")]
    pub project_type: Option<i64>,
    #[serde(rename = "Denominator", default = "default_denominator")]
    pub denominator: Option<i64>,
    #[serde(rename = "SynMode", default = "default_syn_mode")]
    pub syn_mode: Option<i64>,
}

impl Default for GjgjProjectSetting {
    fn default() -> Self {
        Self {
            no1_key_name: Some("C".to_owned()),
            eq_after_mix: Some(String::new()),
            project_type: Some(0),
            denominator: Some(4),
            syn_mode: Some(0),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct GjgjSingerInfo {
    #[serde(rename = "Name", default = "default_singer_name")]
    pub name: String,
    #[serde(rename = "DisplayName", default = "default_singer_name")]
    pub display_name: String,
    #[serde(rename = "TemplateID", default = "default_singer_name_opt")]
    pub template_id: Option<String>,
    #[serde(rename = "Sex", default = "default_singer_name_opt")]
    pub sex: Option<String>,
    #[serde(rename = "Age", default = "default_singer_age")]
    pub age: Option<i64>,
    #[serde(rename = "Color", default = "default_singer_color")]
    pub color: Option<i64>,
    #[serde(rename = "UserID", default = "default_singer_name_opt")]
    pub user_id: Option<String>,
    #[serde(rename = "UpPitch", default)]
    pub up_pitch: Option<i64>,
    #[serde(rename = "LowPitch", default)]
    pub low_pitch: Option<i64>,
    #[serde(rename = "RecommandUpPitch", default)]
    pub recommend_up_pitch: Option<i64>,
    #[serde(rename = "RecommandLowPitch", default)]
    pub recommend_low_pitch: Option<i64>,
    #[serde(rename = "image", default)]
    pub image: Option<String>,
    #[serde(rename = "image_url", alias = "imageUrl", default)]
    pub image_url: Option<String>,
    #[serde(rename = "full_avatar", alias = "fullAvatar", default)]
    pub full_avatar: Option<String>,
    #[serde(rename = "synthetize_file", default = "default_singer_name_opt")]
    pub synthesize_file: Option<String>,
}

impl Default for GjgjSingerInfo {
    fn default() -> Self {
        Self {
            name: String::new(),
            display_name: String::new(),
            template_id: Some(String::new()),
            sex: Some(String::new()),
            age: Some(100),
            color: Some(0),
            user_id: Some(String::new()),
            up_pitch: Some(0),
            low_pitch: Some(0),
            recommend_up_pitch: Some(0),
            recommend_low_pitch: Some(0),
            image: Some(String::new()),
            image_url: Some(String::new()),
            full_avatar: Some(String::new()),
            synthesize_file: Some(String::new()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct GjgjTempos {
    #[serde(rename = "Time")]
    pub time: i64,
    #[serde(rename = "MicrosecondsPerQuarterNote")]
    pub microseconds_per_quarter_note: i64,
}

impl Default for GjgjTempos {
    fn default() -> Self {
        Self {
            time: 0,
            microseconds_per_quarter_note: 0,
        }
    }
}

fn default_time() -> i64 {
    0
}

fn default_numerator() -> i64 {
    4
}

fn default_denominator_ts() -> i64 {
    4
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct GjgjTimeSignature {
    #[serde(rename = "Time", default = "default_time")]
    pub time: i64,
    #[serde(rename = "Numerator", default = "default_numerator")]
    pub numerator: i64,
    #[serde(rename = "Denominator", default = "default_denominator_ts")]
    pub denominator: i64,
}

impl Default for GjgjTimeSignature {
    fn default() -> Self {
        Self {
            time: 0,
            numerator: 4,
            denominator: 4,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GjgjPoint {
    #[serde(rename = "X", default)]
    pub x: f64,
    #[serde(rename = "Y", default)]
    pub y: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GjgjTone {
    #[serde(rename = "Modifys", default)]
    pub modifies: Vec<GjgjPoint>,
    #[serde(rename = "ModifyRanges", default)]
    pub modify_ranges: Vec<GjgjPoint>,
}

fn default_ticks_per_quarter_note() -> i64 {
    libresvip_core::core::constants::TICKS_IN_BEAT
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct GjgjTempoMap {
    #[serde(rename = "TicksPerQuarterNote", default = "default_ticks_per_quarter_note")]
    pub ticks_per_quarter_note: i64,
    #[serde(rename = "Tempos", default)]
    pub tempos: Vec<GjgjTempos>,
    #[serde(rename = "TimeSignature", default)]
    pub time_signature: Vec<GjgjTimeSignature>,
}

impl Default for GjgjTempoMap {
    fn default() -> Self {
        Self {
            ticks_per_quarter_note: default_ticks_per_quarter_note(),
            tempos: Vec::new(),
            time_signature: Vec::new(),
        }
    }
}

fn default_volume_map_time() -> i64 {
    0
}

fn default_volume_map_volume() -> f64 {
    0.0
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct GjgjVolumeMap {
    #[serde(rename = "Time", default = "default_volume_map_time")]
    pub time: i64,
    #[serde(rename = "Volume", default = "default_volume_map_volume")]
    pub volume: f64,
}

impl Default for GjgjVolumeMap {
    fn default() -> Self {
        Self {
            time: 0,
            volume: 0.0,
        }
    }
}

fn default_tone() -> GjgjTone {
    GjgjTone::default()
}

fn default_singer_info_field() -> GjgjSingerInfo {
    GjgjSingerInfo::default()
}

fn default_keyboard() -> Option<GjgjKeyboard> {
    Some(GjgjKeyboard::default())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct GjgjSingingTrack {
    #[serde(rename = "ID")]
    pub id_value: Option<String>,
    #[serde(rename = "Type", default = "default_track_type")]
    pub type_: Option<i64>,
    #[serde(rename = "Name")]
    pub name: Option<String>,
    #[serde(rename = "BeatItems", default)]
    pub beat_items: Vec<GjgjBeatItems>,
    #[serde(rename = "Tone", default = "default_tone")]
    pub tone: GjgjTone,
    #[serde(rename = "VolumeMap", default)]
    pub volume_map: Vec<GjgjVolumeMap>,
    #[serde(rename = "SingerInfo", default = "default_singer_info_field")]
    pub singer_info: GjgjSingerInfo,
    #[serde(rename = "Keyboard", default = "default_keyboard")]
    pub keyboard: Option<GjgjKeyboard>,
    #[serde(rename = "MasterVolume")]
    pub master_volume: Option<GjgjTrackVolume>,
    #[serde(rename = "EQProgram", default = "default_eq_program")]
    pub eq_program: Option<String>,
    #[serde(rename = "SortIndex", default = "default_sort_index")]
    pub sort_index: Option<i64>,
}

impl Default for GjgjSingingTrack {
    fn default() -> Self {
        Self {
            id_value: None,
            type_: Some(0),
            name: None,
            beat_items: Vec::new(),
            tone: GjgjTone::default(),
            volume_map: Vec::new(),
            singer_info: GjgjSingerInfo::default(),
            keyboard: Some(GjgjKeyboard::default()),
            master_volume: None,
            eq_program: Some("无".to_owned()),
            sort_index: Some(0),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GjgjInstrumentalTrack {
    #[serde(rename = "ID")]
    pub id_value: Option<String>,
    #[serde(rename = "Type")]
    pub type_: Option<i64>,
    #[serde(rename = "Path")]
    pub path: Option<String>,
    #[serde(rename = "Offset")]
    pub offset: i64,
    #[serde(rename = "MasterVolume")]
    pub master_volume: Option<GjgjTrackVolume>,
    #[serde(rename = "EQProgram")]
    pub eq_program: Option<String>,
    #[serde(rename = "SortIndex")]
    pub sort_index: Option<i64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GjgjMidiControl {
    #[serde(rename = "ControlNum")]
    pub control_num: Option<i64>,
    #[serde(rename = "VolumeMap", default)]
    pub volume_map: Vec<GjgjVolumeMap>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GjgjMidiTrack {
    #[serde(rename = "ID")]
    pub id_value: Option<String>,
    #[serde(rename = "Type")]
    pub type_: Option<i64>,
    #[serde(rename = "SortIndex")]
    pub sort_index: Option<i64>,
    #[serde(rename = "Bank")]
    pub bank: Option<i64>,
    #[serde(rename = "Program")]
    pub program: Option<i64>,
    #[serde(rename = "Channel")]
    pub channel: Option<i64>,
    #[serde(rename = "BeatItems", default)]
    pub beat_items: Vec<GjgjBeatItems>,
    #[serde(rename = "ControlMap", default)]
    pub control_map: Vec<GjgjMidiControl>,
}

fn default_project_setting() -> Option<GjgjProjectSetting> {
    Some(GjgjProjectSetting::default())
}

fn default_tempo_map() -> GjgjTempoMap {
    GjgjTempoMap::default()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct GjgjProject {
    #[serde(rename = "gjgjVersion", default = "default_gjgj_version")]
    pub gjgj_version: Option<i64>,
    #[serde(rename = "ProjectSetting", default = "default_project_setting")]
    pub project_setting: Option<GjgjProjectSetting>,
    #[serde(rename = "Accompaniments", default)]
    pub accompaniments: Vec<GjgjInstrumentalTrack>,
    #[serde(rename = "Tracks", default)]
    pub tracks: Vec<GjgjSingingTrack>,
    #[serde(rename = "MIDITracks", default)]
    pub midi_tracks: Vec<GjgjMidiTrack>,
    #[serde(rename = "TempoMap", default = "default_tempo_map")]
    pub tempo_map: GjgjTempoMap,
}

impl Default for GjgjProject {
    fn default() -> Self {
        Self {
            gjgj_version: Some(2),
            project_setting: Some(GjgjProjectSetting::default()),
            accompaniments: Vec::new(),
            tracks: Vec::new(),
            midi_tracks: Vec::new(),
            tempo_map: GjgjTempoMap::default(),
        }
    }
}


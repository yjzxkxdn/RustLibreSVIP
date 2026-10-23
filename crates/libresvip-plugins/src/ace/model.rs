//! Pydantic-compatible models of the ACE Virtual Singer raw JSON format.

use serde::{Deserialize, Deserializer, Serialize};

fn default_true() -> bool {
    true
}

fn default_two() -> i64 {
    2
}

fn default_br() -> String {
    "br".to_owned()
}

fn default_record_type() -> String {
    "create".to_owned()
}

fn default_version() -> String {
    "3.2.5".to_owned()
}

fn default_scale() -> Option<Vec<i64>> {
    None
}

fn null_to_empty_vec<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<AceTrack>, D::Error> {
    let value = Option::<Vec<AceTrack>>::deserialize(deserializer)?;
    Ok(value.unwrap_or_default())
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AceEnvelopePoint {
    pub time: f64,
    pub envolope: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AcePitchBend {
    pub time: f64,
    pub pitch: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AceTimedItem {
    pub start_time: f64,
    pub end_time: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub energy_envolope: Option<Vec<AceEnvelopePoint>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub air_envolope: Option<Vec<AceEnvelopePoint>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tension_envolope: Option<Vec<AceEnvelopePoint>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub falsetto_envolope: Option<Vec<AceEnvelopePoint>>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AceBreathNote {
    #[serde(flatten)]
    pub base: AceTimedItem,
    #[serde(default = "default_br")]
    pub pinyin: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AceNote {
    #[serde(flatten)]
    pub base: AceTimedItem,
    pub pitch: i64,
    pub word: String,
    pub pinyin: String,
    #[serde(rename = "pitchBends")]
    pub pitch_bends: Vec<AcePitchBend>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_pitch: Option<Vec<AcePitchBend>>,
    pub br: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consonant_time_abs: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<Vec<i64>>,
}

impl Default for AceNote {
    fn default() -> Self {
        Self {
            base: AceTimedItem::default(),
            pitch: 0,
            word: String::new(),
            pinyin: String::new(),
            pitch_bends: Vec::new(),
            user_pitch: None,
            br: false,
            config: None,
            consonant_time_abs: None,
            key: None,
            scale: None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AceRoleInfo {
    pub name: String,
    pub role_id: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AceTrack {
    #[serde(rename = "AI_SVS_mode", default = "default_true")]
    pub ai_svs_mode: bool,
    pub br_notes: Vec<AceBreathNote>,
    pub front: bool,
    pub lyric: String,
    #[serde(rename = "unReadLyric")]
    pub unread_lyric: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mix_info: Option<String>,
    pub mute: bool,
    pub notes: Vec<AceNote>,
    pub pan: f64,
    pub role_info: AceRoleInfo,
    pub singer_volume: f64,
    pub solo: bool,
    pub sound_effect: i64,
}

impl Default for AceTrack {
    fn default() -> Self {
        Self {
            ai_svs_mode: true,
            br_notes: Vec::new(),
            front: false,
            lyric: String::new(),
            unread_lyric: String::new(),
            mix_info: None,
            mute: false,
            notes: Vec::new(),
            pan: 0.0,
            role_info: AceRoleInfo::default(),
            singer_volume: 1.0,
            solo: false,
            sound_effect: 0,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AceBgmTrack {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bpm: Option<f64>,
    pub end_time: f64,
    pub file_md5: String,
    pub file_name: String,
    pub file_type: String,
    pub position: f64,
    pub start_time: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AceBgmInfo {
    pub tracks: Vec<AceBgmTrack>,
    pub bgm_volume: f64,
    pub mute: bool,
    pub solo: bool,
}

impl Default for AceBgmInfo {
    fn default() -> Self {
        Self {
            tracks: Vec::new(),
            bgm_volume: 1.0,
            mute: false,
            solo: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AceSetting {
    pub bgm_volume: f64,
    pub sampler_loop: bool,
    pub singer_volume: f64,
}

impl Default for AceSetting {
    fn default() -> Self {
        Self {
            bgm_volume: 1.0,
            sampler_loop: false,
            singer_volume: 1.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AceSongInfo {
    pub author: String,
    pub beat_of_bar: i64,
    pub bpm: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_file: Option<String>,
    pub duration: f64,
    pub first_beat_offset: f64,
    pub key: String,
    pub lyric: String,
    pub name: String,
    #[serde(rename = "operateScale", default = "default_scale")]
    pub operate_scale: Option<Vec<i64>>,
    #[serde(rename = "pinyinLyric", skip_serializing_if = "Option::is_none")]
    pub pinyin_lyric: Option<String>,
    #[serde(rename = "readLyric", skip_serializing_if = "Option::is_none")]
    pub read_lyric: Option<String>,
    #[serde(rename = "readPinyinLyric", skip_serializing_if = "Option::is_none")]
    pub read_pinyin_lyric: Option<String>,
    pub scale: Vec<i64>,
    pub segment_of_beat: i64,
    pub song_id: i64,
    pub start: f64,
    #[serde(rename = "unReadLyric")]
    pub unread_lyric: String,
    #[serde(rename = "unReadPinyinLyric", skip_serializing_if = "Option::is_none")]
    pub unread_pinyin_lyric: Option<String>,
    pub user_id: i64,
}

impl Default for AceSongInfo {
    fn default() -> Self {
        Self {
            author: String::new(),
            beat_of_bar: 4,
            bpm: 120.0,
            cover_file: None,
            duration: 0.0,
            first_beat_offset: 0.0,
            key: "C".to_owned(),
            lyric: String::new(),
            name: "New Project".to_owned(),
            operate_scale: None,
            pinyin_lyric: None,
            read_lyric: None,
            read_pinyin_lyric: None,
            scale: Vec::new(),
            segment_of_beat: 4,
            song_id: 0,
            start: 0.0,
            unread_lyric: String::new(),
            unread_pinyin_lyric: None,
            user_id: 0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AceDebugInfo {
    pub device: String,
    pub os: String,
    pub platform: String,
    #[serde(rename = "recordType", default = "default_record_type")]
    pub record_type: String,
    #[serde(default = "default_version")]
    pub version: String,
}

impl Default for AceDebugInfo {
    fn default() -> Self {
        Self {
            device: String::new(),
            os: String::new(),
            platform: "android".to_owned(),
            record_type: default_record_type(),
            version: default_version(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AceProject {
    pub bgm_info: AceBgmInfo,
    pub br_notes: Vec<AceBreathNote>,
    #[serde(
        rename = "debug_info",
        alias = "debugInfo",
        skip_serializing_if = "Option::is_none"
    )]
    pub debug_info: Option<AceDebugInfo>,
    pub notes: Vec<AceNote>,
    pub role_info: AceRoleInfo,
    pub setting: AceSetting,
    pub song_info: AceSongInfo,
    #[serde(default, deserialize_with = "null_to_empty_vec")]
    pub tracks: Vec<AceTrack>,
    #[serde(default = "default_two")]
    pub version: i64,
}

impl Default for AceProject {
    fn default() -> Self {
        Self {
            bgm_info: AceBgmInfo::default(),
            br_notes: Vec::new(),
            debug_info: None,
            notes: Vec::new(),
            role_info: AceRoleInfo::default(),
            setting: AceSetting::default(),
            song_info: AceSongInfo::default(),
            tracks: Vec::new(),
            version: 2,
        }
    }
}

impl AceProject {
    /// `AceProject.mark_legacy_project_version`.
    pub fn normalize(&mut self) {
        if !self.notes.is_empty() {
            self.version = 1;
        }
    }

    /// `AceProject.singing_tracks`.
    pub fn singing_tracks(&self) -> Vec<AceTrack> {
        if !self.tracks.is_empty() {
            return self.tracks.clone();
        }
        if !self.notes.is_empty() {
            return vec![AceTrack {
                br_notes: self.br_notes.clone(),
                notes: self.notes.clone(),
                role_info: self.role_info.clone(),
                singer_volume: self.setting.singer_volume,
                ..Default::default()
            }];
        }
        Vec::new()
    }
}

//! Pydantic-compatible model of PocketSinger's `ps_structure.ps` / `config.json`.
//!
//! Field order and aliases mirror `libresvip.plugins.ps_project.model`;
//! serialisation matches `model_dump_json(by_alias=True, exclude_none=True)`
//! (compact separators, `None` fields omitted).

use serde::{Deserialize, Serialize};

fn skip_none<T>(value: &Option<T>) -> bool {
    value.is_none()
}

fn default_language() -> String {
    "ch".to_owned()
}

fn default_true() -> bool {
    true
}

fn default_one() -> f64 {
    1.0
}

fn default_grapheme_count() -> i64 {
    1
}

fn default_debug_version() -> String {
    "1.6.2".to_owned()
}

fn default_record_type() -> String {
    "create".to_owned()
}

fn default_os() -> String {
    "16".to_owned()
}

fn default_device() -> String {
    "iPad8,3".to_owned()
}

fn default_platform() -> String {
    "iOS".to_owned()
}

fn default_project_version() -> i64 {
    3
}

fn default_note_type() -> String {
    "phone".to_owned()
}

// ---------------------------------------------------------------------------
// Curve helpers
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PocketSingerPitchBend {
    pub pitch: f64,
    pub time: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PocketSingerEnvolopeItem {
    pub envolope: f64,
    pub time: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PocketSingerSinWave {
    pub release_len: f64,
    pub freq: f64,
    pub attack_vol: f64,
    pub attack_len: f64,
    pub amp: f64,
    pub start_pos: f64,
    pub release_vol: f64,
}

// ---------------------------------------------------------------------------
// Notes
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PocketSingerBrNote {
    pub start_time: f64,
    pub end_time: f64,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub identifier: Option<i64>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub energy_envolope: Option<Vec<PocketSingerEnvolopeItem>>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub air_envolope: Option<Vec<PocketSingerEnvolopeItem>>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub tension_envolope: Option<Vec<PocketSingerEnvolopeItem>>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub falsetto_envolope: Option<Vec<PocketSingerEnvolopeItem>>,
    #[serde(rename = "type", default = "default_br_type")]
    pub type_: String,
}

fn default_br_type() -> String {
    "br".to_owned()
}

impl Default for PocketSingerBrNote {
    fn default() -> Self {
        Self {
            start_time: 0.0,
            end_time: 0.0,
            identifier: None,
            energy_envolope: None,
            air_envolope: None,
            tension_envolope: None,
            falsetto_envolope: None,
            type_: default_br_type(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PocketSingerNote {
    pub start_time: f64,
    pub end_time: f64,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub identifier: Option<i64>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub energy_envolope: Option<Vec<PocketSingerEnvolopeItem>>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub air_envolope: Option<Vec<PocketSingerEnvolopeItem>>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub tension_envolope: Option<Vec<PocketSingerEnvolopeItem>>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub falsetto_envolope: Option<Vec<PocketSingerEnvolopeItem>>,
    #[serde(rename = "pitchBends", default)]
    pub pitch_bends: Vec<PocketSingerPitchBend>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub consonant_time_head: Option<Vec<f64>>,
    #[serde(default)]
    pub phone: Vec<String>,
    #[serde(default = "default_grapheme_count")]
    pub grapheme_count: i64,
    #[serde(default)]
    pub grapheme_index: i64,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(rename = "grapheme", alias = "word")]
    pub grapheme: String,
    pub pitch: i64,
    #[serde(rename = "type", default = "default_note_type")]
    pub type_: String,
    #[serde(default)]
    pub br: bool,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub sin_wave: Option<PocketSingerSinWave>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub user_pitch: Option<Vec<PocketSingerPitchBend>>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub consonant_time_tail: Option<Vec<f64>>,
    #[serde(rename = "brNote", default, skip_serializing_if = "skip_none")]
    pub br_note: Option<PocketSingerBrNote>,
    #[serde(rename = "userPhone", default, skip_serializing_if = "skip_none")]
    pub user_phone: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub valid: Option<bool>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub scale: Option<Vec<i64>>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub config: Option<String>,
    #[serde(rename = "isEditPhone", default, skip_serializing_if = "skip_none")]
    pub is_edit_phone: Option<bool>,
}

impl Default for PocketSingerNote {
    fn default() -> Self {
        Self {
            start_time: 0.0,
            end_time: 0.0,
            identifier: None,
            energy_envolope: None,
            air_envolope: None,
            tension_envolope: None,
            falsetto_envolope: None,
            pitch_bends: Vec::new(),
            consonant_time_head: None,
            phone: Vec::new(),
            grapheme_count: default_grapheme_count(),
            grapheme_index: 0,
            language: default_language(),
            grapheme: String::new(),
            pitch: 0,
            type_: default_note_type(),
            br: false,
            sin_wave: None,
            user_pitch: None,
            consonant_time_tail: None,
            br_note: None,
            user_phone: None,
            valid: None,
            key: None,
            scale: None,
            config: None,
            is_edit_phone: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Tracks / song info
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PocketSingerRoleInfo {
    pub name: String,
    pub role_id: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PocketSingerTrack {
    pub sound_effect: i64,
    pub pan: f64,
    #[serde(rename = "AI_SVS_mode", default = "default_true")]
    pub ai_svs_mode: bool,
    pub lyric: String,
    pub solo: bool,
    pub notes: Vec<PocketSingerNote>,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(skip_serializing_if = "skip_none")]
    pub mix_info: Option<String>,
    pub br_notes: Vec<PocketSingerBrNote>,
    pub role_info: PocketSingerRoleInfo,
    pub mute: bool,
    pub front: bool,
    #[serde(default = "default_one")]
    pub singer_volume: f64,
}

impl Default for PocketSingerTrack {
    fn default() -> Self {
        Self {
            sound_effect: 0,
            pan: 0.0,
            ai_svs_mode: true,
            lyric: String::new(),
            solo: false,
            notes: Vec::new(),
            language: default_language(),
            mix_info: None,
            br_notes: Vec::new(),
            role_info: PocketSingerRoleInfo::default(),
            mute: false,
            front: false,
            singer_volume: 1.0,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PocketSingerBgmTrack {
    pub end_time: f64,
    pub file_md5: String,
    pub file_name: String,
    pub file_type: String,
    pub position: f64,
    pub start_time: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PocketSingerBgmInfo {
    pub tracks: Vec<PocketSingerBgmTrack>,
    pub mute: bool,
    #[serde(default = "default_one")]
    pub bgm_volume: f64,
    pub solo: bool,
}

impl Default for PocketSingerBgmInfo {
    fn default() -> Self {
        Self {
            tracks: Vec::new(),
            mute: false,
            bgm_volume: 1.0,
            solo: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PocketSingerDebugInfo {
    #[serde(default = "default_debug_version")]
    pub version: String,
    #[serde(rename = "recordType", default = "default_record_type")]
    pub record_type: String,
    #[serde(default = "default_os")]
    pub os: String,
    #[serde(default = "default_device")]
    pub device: String,
    #[serde(rename = "buildVersion", skip_serializing_if = "skip_none")]
    pub build_version: Option<String>,
    #[serde(default = "default_platform")]
    pub platform: String,
    #[serde(rename = "userLanguage", skip_serializing_if = "skip_none")]
    pub user_language: Option<String>,
}

impl Default for PocketSingerDebugInfo {
    fn default() -> Self {
        Self {
            version: default_debug_version(),
            record_type: default_record_type(),
            os: default_os(),
            device: default_device(),
            build_version: None,
            platform: default_platform(),
            user_language: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PocketSingerSongInfo {
    pub start: i64,
    pub first_beat_offset: f64,
    pub scale: Vec<i64>,
    pub key: String,
    pub segment_of_beat: i64,
    #[serde(rename = "operateScale")]
    pub operate_scale: Vec<i64>,
    pub bpm: f64,
    pub duration: f64,
    pub beat_of_bar: i64,
    pub name: String,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub origin_duration: Option<f64>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub origin_start: Option<i64>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub tuner: Option<String>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub song_id: Option<i64>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub user_id: Option<i64>,
}

impl Default for PocketSingerSongInfo {
    fn default() -> Self {
        Self {
            start: 0,
            first_beat_offset: 0.0,
            scale: Vec::new(),
            key: String::new(),
            segment_of_beat: 0,
            operate_scale: Vec::new(),
            bpm: 120.0,
            duration: 0.0,
            beat_of_bar: 4,
            name: String::new(),
            origin_duration: None,
            origin_start: None,
            author: None,
            tuner: None,
            song_id: None,
            user_id: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PocketSingerProject {
    #[serde(default, skip_serializing_if = "skip_none")]
    pub notes: Option<Vec<PocketSingerNote>>,
    #[serde(default)]
    pub tracks: Vec<PocketSingerTrack>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub bpm: Option<f64>,
    #[serde(default)]
    pub bgm_info: PocketSingerBgmInfo,
    #[serde(default)]
    pub debug_info: PocketSingerDebugInfo,
    #[serde(default = "default_project_version")]
    pub version: i64,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub timestamp: Option<i64>,
    pub song_info: PocketSingerSongInfo,
}

impl Default for PocketSingerProject {
    fn default() -> Self {
        Self {
            notes: None,
            tracks: Vec::new(),
            bpm: None,
            bgm_info: PocketSingerBgmInfo::default(),
            debug_info: PocketSingerDebugInfo::default(),
            version: default_project_version(),
            timestamp: None,
            song_info: PocketSingerSongInfo::default(),
        }
    }
}

impl PocketSingerProject {
    /// `PocketSingerProject.model_validate_json` including the
    /// `migrate_notes` model validator.
    pub fn from_json(text: &[u8]) -> Result<Self, serde_json::Error> {
        let mut project: Self = serde_json::from_slice(text)?;
        project.migrate_notes();
        Ok(project)
    }

    fn migrate_notes(&mut self) {
        if let Some(notes) = self.notes.take() {
            self.version = 1;
            self.tracks.push(PocketSingerTrack {
                notes,
                ..Default::default()
            });
        }
    }

    /// `model_dump_json(by_alias=True, exclude_none=True)`.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

// ---------------------------------------------------------------------------
// Container metadata (`config.json`)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PocketSingerBgmMetadata {
    #[serde(rename = "barBeatCount")]
    pub bar_beat_count: i64,
    #[serde(rename = "beatSegmentCount")]
    pub beat_segment_count: i64,
    #[serde(rename = "bgmDescription")]
    pub bgm_description: String,
    pub bpm: f64,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "fileMD5")]
    pub file_md5: String,
    #[serde(rename = "fileName")]
    pub file_name: String,
    #[serde(rename = "fileType")]
    pub file_type: String,
    #[serde(rename = "firstBeatOffset")]
    pub first_beat_offset: f64,
}

impl Default for PocketSingerBgmMetadata {
    fn default() -> Self {
        Self {
            bar_beat_count: 0,
            beat_segment_count: 0,
            bgm_description: String::new(),
            bpm: 120.0,
            display_name: String::new(),
            file_md5: String::new(),
            file_name: String::new(),
            file_type: String::new(),
            first_beat_offset: 0.0,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PocketSingerMetadata {
    #[serde(rename = "aceFileName")]
    pub ace_file_name: String,
    #[serde(
        rename = "backgroundImageName",
        default,
        skip_serializing_if = "skip_none"
    )]
    pub background_image_name: Option<String>,
    #[serde(rename = "bgmInfo", default, skip_serializing_if = "skip_none")]
    pub bgm_info: Option<PocketSingerBgmMetadata>,
}

impl PocketSingerMetadata {
    pub fn from_json(text: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(text)
    }

    /// `model_dump_json(exclude_none=True, by_alias=True)`.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

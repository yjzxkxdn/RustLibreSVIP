//! Pydantic-compatible model of `ppsf.json` (Piapro Studio NT 3.x).
//!
//! Field order, aliases and defaults mirror `libresvip.plugins.ppsf.model`.
//! Serialisation uses pydantic's `model_dump(by_alias=True, exclude_none=True,
//! mode="json")` semantics: `None` fields are omitted, everything else is
//! emitted in declaration order.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::util::uuid_v4;

fn is_none<T>(value: &Option<T>) -> bool {
    value.is_none()
}

fn some_false() -> Option<bool> {
    Some(false)
}

fn some_true() -> Option<bool> {
    Some(true)
}

fn some_zero() -> Option<i64> {
    Some(0)
}

fn some_default_2() -> Option<i64> {
    Some(2)
}

fn some_180() -> Option<i64> {
    Some(180)
}

fn some_minus_120() -> Option<i64> {
    Some(-120)
}

fn default_envelope() -> Option<PpsfEnvelope> {
    Some(PpsfEnvelope::default())
}

// ---------------------------------------------------------------------------
// Curve points / parameters
// ---------------------------------------------------------------------------

/// `PpsfCurveType` (IntEnum): `BORDER = 0`, `NORMAL = 1`.
#[derive(Clone, Debug, Default)]
pub struct PpsfCurveType;

impl PpsfCurveType {
    pub const BORDER: i64 = 0;
    pub const NORMAL: i64 = 1;
}

/// `PpsfLanguage` (IntEnum): `JAPANESE = 0`, `ENGLISH = 1`, `SIMPLIFIED_CHINESE = 4`.
pub const PPSF_LANGUAGE_JAPANESE: i64 = 0;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfCurvePointSeq {
    #[serde(
        rename = "border-type",
        skip_serializing_if = "is_none",
        default
    )]
    pub border_type: Option<i64>,
    #[serde(rename = "note-index", skip_serializing_if = "is_none", default)]
    pub note_index: Option<i64>,
    #[serde(rename = "region-index")]
    pub region_index: i64,
    #[serde(
        rename = "edited-by-user",
        skip_serializing_if = "is_none",
        default = "some_false"
    )]
    pub edited_by_user: Option<bool>,
    #[serde(
        rename = "seg-array-id",
        skip_serializing_if = "is_none",
        default
    )]
    pub seg_array_id: Option<i64>,
    #[serde(rename = "abs-value", skip_serializing_if = "is_none", default)]
    pub abs_value: Option<i64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfCurvePoint {
    #[serde(
        rename = "plugin-descriptor",
        skip_serializing_if = "is_none",
        default
    )]
    pub plugin_descriptor: Option<String>,
    #[serde(default)]
    pub sequence: Vec<PpsfCurvePointSeq>,
    #[serde(rename = "sub-track-category")]
    pub sub_track_category: i64,
    #[serde(rename = "sub-track-id")]
    pub sub_track_id: i64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfParamPoint {
    #[serde(rename = "curve_type")]
    pub curve_type: i64,
    #[serde(rename = "pos")]
    pub pos: i64,
    #[serde(rename = "value")]
    pub value: i64,
    #[serde(rename = "region-index", skip_serializing_if = "is_none")]
    pub region_index: Option<i64>,
    #[serde(rename = "border-type", skip_serializing_if = "is_none")]
    pub border_type: Option<i64>,
    #[serde(rename = "note-index", skip_serializing_if = "is_none")]
    pub note_index: Option<i64>,
    #[serde(rename = "edited-by-user", skip_serializing_if = "is_none")]
    pub edited_by_user: Option<bool>,
    #[serde(rename = "seg-array-id", skip_serializing_if = "is_none")]
    pub seg_array_id: Option<i64>,
}

impl Default for PpsfParamPoint {
    fn default() -> Self {
        Self {
            curve_type: PpsfCurveType::NORMAL,
            pos: 0,
            value: 0,
            region_index: None,
            border_type: None,
            note_index: None,
            edited_by_user: None,
            seg_array_id: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Notes / syllables / regions
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfSyllable {
    #[serde(rename = "footer-text")]
    pub footer_text: String,
    #[serde(rename = "header-text")]
    pub header_text: String,
    #[serde(rename = "is-list-end")]
    pub is_list_end: bool,
    #[serde(rename = "is-list-top")]
    pub is_list_top: bool,
    #[serde(rename = "is-word-end")]
    pub is_word_end: bool,
    #[serde(rename = "is-word-top")]
    pub is_word_top: bool,
    #[serde(rename = "lyric-text")]
    pub lyric_text: String,
    #[serde(rename = "symbols-text")]
    pub symbols_text: String,
}

impl Default for PpsfSyllable {
    fn default() -> Self {
        Self {
            footer_text: String::new(),
            header_text: String::new(),
            is_list_end: true,
            is_list_top: true,
            is_word_end: true,
            is_word_top: true,
            lyric_text: String::new(),
            symbols_text: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfNote {
    pub language: i64,
    #[serde(rename = "region-index")]
    pub region_index: i64,
    pub syllables: Vec<PpsfSyllable>,
    #[serde(skip_serializing_if = "is_none")]
    pub event_index: Option<i64>,
    #[serde(skip_serializing_if = "is_none")]
    pub length: Option<i64>,
    #[serde(skip_serializing_if = "is_none", default = "some_false")]
    pub muted: Option<bool>,
    #[serde(skip_serializing_if = "is_none", default = "some_zero")]
    pub vibrato_preset_id: Option<i64>,
    #[serde(skip_serializing_if = "is_none", default = "some_zero")]
    pub voice_color_id: Option<i64>,
    #[serde(skip_serializing_if = "is_none", default = "some_zero")]
    pub voice_release_id: Option<i64>,
    #[serde(skip_serializing_if = "is_none", default = "some_default_2")]
    pub note_env_preset_id: Option<i64>,
    #[serde(skip_serializing_if = "is_none", default = "some_zero")]
    pub note_gain_value: Option<i64>,
    #[serde(skip_serializing_if = "is_none", default = "some_zero")]
    pub note_param_edited_stats: Option<i64>,
    #[serde(skip_serializing_if = "is_none", default = "some_180")]
    pub portamento_length: Option<i64>,
    #[serde(skip_serializing_if = "is_none", default = "some_minus_120")]
    pub portamento_offset: Option<i64>,
    #[serde(skip_serializing_if = "is_none", default = "some_zero")]
    pub vibrato_depth: Option<i64>,
    #[serde(skip_serializing_if = "is_none", default = "some_zero")]
    pub vibrato_rate: Option<i64>,
    #[serde(skip_serializing_if = "is_none", default = "some_false")]
    pub auto_ai_consonant_length_enabled: Option<bool>,
    #[serde(skip_serializing_if = "is_none", default = "some_true")]
    pub auto_ai_f0_enabled: Option<bool>,
    #[serde(skip_serializing_if = "is_none")]
    pub auto_ai_vibrato_blend_amount: Option<i64>,
    #[serde(skip_serializing_if = "is_none")]
    pub auto_ai_vibrato_blend_type: Option<i64>,
    #[serde(skip_serializing_if = "is_none")]
    pub auto_ai_vibrato_default_type: Option<i64>,
    #[serde(skip_serializing_if = "is_none", default = "some_zero")]
    pub auto_ai_vibrato_depth: Option<i64>,
    #[serde(skip_serializing_if = "is_none", default = "some_false")]
    pub auto_ai_vibrato_enabled: Option<bool>,
    #[serde(skip_serializing_if = "is_none")]
    pub auto_ai_vibrato_offset: Option<f64>,
    #[serde(skip_serializing_if = "is_none", default = "some_false")]
    pub auto_ai_vibrato_offset_by_user: Option<bool>,
}

impl Default for PpsfNote {
    fn default() -> Self {
        Self {
            language: PPSF_LANGUAGE_JAPANESE,
            region_index: 0,
            syllables: Vec::new(),
            event_index: None,
            length: None,
            muted: some_false(),
            vibrato_preset_id: some_zero(),
            voice_color_id: some_zero(),
            voice_release_id: some_zero(),
            note_env_preset_id: some_default_2(),
            note_gain_value: some_zero(),
            note_param_edited_stats: some_zero(),
            portamento_length: some_180(),
            portamento_offset: some_minus_120(),
            vibrato_depth: some_zero(),
            vibrato_rate: some_zero(),
            auto_ai_consonant_length_enabled: some_false(),
            auto_ai_f0_enabled: some_true(),
            auto_ai_vibrato_blend_amount: None,
            auto_ai_vibrato_blend_type: None,
            auto_ai_vibrato_default_type: None,
            auto_ai_vibrato_depth: some_zero(),
            auto_ai_vibrato_enabled: some_false(),
            auto_ai_vibrato_offset: None,
            auto_ai_vibrato_offset_by_user: some_false(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfRegion {
    #[serde(rename = "auto-expand-left", skip_serializing_if = "is_none")]
    pub auto_expand_left: Option<bool>,
    #[serde(rename = "auto-expand-right", skip_serializing_if = "is_none")]
    pub auto_expand_right: Option<bool>,
    pub length: i64,
    pub muted: bool,
    pub name: String,
    pub position: i64,
    #[serde(rename = "z-order")]
    pub z_order: i64,
    #[serde(rename = "audio-event-index", skip_serializing_if = "is_none")]
    pub audio_event_index: Option<i64>,
}

impl Default for PpsfRegion {
    fn default() -> Self {
        Self {
            auto_expand_left: None,
            auto_expand_right: None,
            length: 0,
            muted: false,
            name: String::new(),
            position: 0,
            z_order: 0,
            audio_event_index: None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfSubTrack {
    pub height: i64,
    #[serde(rename = "plugin-descriptor", skip_serializing_if = "is_none")]
    pub plugin_descriptor: Option<String>,
    #[serde(rename = "sub-track-category")]
    pub sub_track_category: i64,
    #[serde(rename = "sub-track-id")]
    pub sub_track_id: i64,
}

// ---------------------------------------------------------------------------
// Event tracks / editor
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfEventTrack {
    #[serde(rename = "curve-points")]
    pub curve_points: Vec<PpsfCurvePoint>,
    #[serde(rename = "fsm-effects", skip_serializing_if = "is_none")]
    pub fsm_effects: Option<Vec<Value>>,
    pub height: i64,
    pub index: i64,
    #[serde(rename = "mute-solo")]
    pub mute_solo: i64,
    pub notes: Vec<PpsfNote>,
    #[serde(rename = "nt-envelope-preset-id", skip_serializing_if = "is_none")]
    pub nt_envelope_preset_id: Option<i64>,
    pub regions: Vec<PpsfRegion>,
    #[serde(rename = "sub-tracks")]
    pub sub_tracks: Vec<PpsfSubTrack>,
    #[serde(rename = "total-height")]
    pub total_height: i64,
    #[serde(rename = "track-type")]
    pub track_type: i64,
    #[serde(rename = "vertical-scale")]
    pub vertical_scale: f64,
    #[serde(rename = "vertical-scroll")]
    pub vertical_scroll: i64,
}

impl Default for PpsfEventTrack {
    fn default() -> Self {
        Self {
            curve_points: Vec::new(),
            fsm_effects: Some(Vec::new()),
            height: 64,
            index: 0,
            mute_solo: 0,
            notes: Vec::new(),
            nt_envelope_preset_id: None,
            regions: Vec::new(),
            sub_tracks: Vec::new(),
            total_height: 64,
            track_type: 0,
            vertical_scale: 1.0,
            vertical_scroll: 0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfTempoTrack {
    pub height: i64,
    #[serde(rename = "vertical-scale")]
    pub vertical_scale: f64,
    #[serde(rename = "vertical-scroll")]
    pub vertical_scroll: i64,
}

impl Default for PpsfTempoTrack {
    fn default() -> Self {
        Self {
            height: 0,
            vertical_scale: 2.0,
            vertical_scroll: 0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfTrackEditor {
    #[serde(rename = "event-tracks")]
    pub event_tracks: Vec<PpsfEventTrack>,
    #[serde(rename = "header-width")]
    pub header_width: i64,
    pub height: i64,
    #[serde(rename = "horizontal-scale")]
    pub horizontal_scale: f64,
    #[serde(rename = "horizontal-scroll")]
    pub horizontal_scroll: i64,
    #[serde(rename = "tempo-track")]
    pub tempo_track: PpsfTempoTrack,
    #[serde(rename = "user-markers", skip_serializing_if = "is_none")]
    pub user_markers: Option<Vec<Value>>,
    pub width: i64,
    pub x: i64,
    pub y: i64,
}

impl Default for PpsfTrackEditor {
    fn default() -> Self {
        Self {
            event_tracks: Vec::new(),
            header_width: 264,
            height: 720,
            horizontal_scale: 0.08,
            horizontal_scroll: 0,
            tempo_track: PpsfTempoTrack::default(),
            user_markers: Some(Vec::new()),
            width: 1024,
            x: 100,
            y: 100,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfLoopPoint {
    pub begin: i64,
    #[serde(
        rename = "enabled",
        alias = "enable",
        skip_serializing_if = "is_none",
        default = "some_false"
    )]
    pub enabled: Option<bool>,
    pub end: i64,
}

impl Default for PpsfLoopPoint {
    fn default() -> Self {
        Self {
            begin: 0,
            enabled: some_false(),
            end: 7680,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfMetronome {
    #[serde(
        rename = "enabled",
        alias = "enable",
        skip_serializing_if = "is_none",
        default = "some_false"
    )]
    pub enabled: Option<bool>,
    pub wav: String,
}

impl Default for PpsfMetronome {
    fn default() -> Self {
        Self {
            enabled: some_false(),
            wav: "null".to_owned(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfGuiSettings {
    #[serde(rename = "loop_point", skip_serializing_if = "is_none")]
    pub loop_point: Option<PpsfLoopPoint>,
    #[serde(skip_serializing_if = "is_none")]
    pub metronome: Option<PpsfMetronome>,
    #[serde(rename = "ambient-enabled", skip_serializing_if = "is_none")]
    pub ambient_enabled: Option<bool>,
    #[serde(rename = "file-fullpath")]
    pub file_fullpath: String,
    #[serde(rename = "playback-position")]
    pub playback_position: i64,
    #[serde(rename = "project-length")]
    pub project_length: i64,
    #[serde(rename = "track-editor")]
    pub track_editor: PpsfTrackEditor,
    #[serde(rename = "auto-connect-interval-msec", skip_serializing_if = "is_none")]
    pub auto_connect_interval_msec: Option<i64>,
    #[serde(rename = "is-saved-by-nt2", skip_serializing_if = "is_none")]
    pub is_saved_by_nt2: Option<bool>,
}

impl Default for PpsfGuiSettings {
    fn default() -> Self {
        Self {
            loop_point: Some(PpsfLoopPoint::default()),
            metronome: Some(PpsfMetronome::default()),
            ambient_enabled: None,
            file_fullpath: String::new(),
            playback_position: 0,
            project_length: 0,
            track_editor: PpsfTrackEditor::default(),
            auto_connect_interval_msec: None,
            is_saved_by_nt2: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Sequences / envelopes / mixer
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfBaseSequence {
    pub constant: i64,
    pub name: String,
    pub sequence: Vec<PpsfParamPoint>,
    pub use_sequence: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfSeqParam {
    #[serde(rename = "base-sequence", skip_serializing_if = "is_none")]
    pub base_sequence: Option<PpsfBaseSequence>,
    #[serde(skip_serializing_if = "is_none")]
    pub layers: Option<Vec<Value>>,
}

impl Default for PpsfSeqParam {
    fn default() -> Self {
        Self {
            base_sequence: None,
            layers: Some(Vec::new()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfEnvelope {
    pub length: i64,
    pub offset: i64,
    pub points: Vec<PpsfParamPoint>,
    pub use_length: bool,
}

impl Default for PpsfEnvelope {
    fn default() -> Self {
        Self {
            length: 0,
            offset: 0,
            points: Vec::new(),
            use_length: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfMixer {
    pub gain: PpsfSeqParam,
    pub mixer_type: String,
    pub panpot: PpsfSeqParam,
}

impl Default for PpsfMixer {
    fn default() -> Self {
        let gain = PpsfSeqParam {
            base_sequence: Some(PpsfBaseSequence {
                constant: 750000,
                name: "Gain".to_owned(),
                sequence: Vec::new(),
                use_sequence: false,
            }),
            layers: Some(Vec::new()),
        };
        let panpot = PpsfSeqParam {
            base_sequence: Some(PpsfBaseSequence {
                constant: 0,
                name: "Panpot".to_owned(),
                sequence: Vec::new(),
                use_sequence: false,
            }),
            layers: Some(Vec::new()),
        };
        Self {
            gain,
            mixer_type: "stereo".to_owned(),
            panpot,
        }
    }
}

// ---------------------------------------------------------------------------
// Tracks
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfSinger {
    pub character_name: String,
    pub do_extrapolation: bool,
    pub frame_shift: f64,
    pub gender: i64,
    pub language_id: i64,
    pub library_id: String,
    pub name: String,
    pub singer_name: String,
    pub stationaly_type: String,
    pub synthesis_version: String,
    pub tail_silent: f64,
    pub vprm_morph_mode: String,
}

impl Default for PpsfSinger {
    fn default() -> Self {
        Self {
            character_name: "miku".to_owned(),
            do_extrapolation: false,
            frame_shift: 0.001,
            gender: 0,
            language_id: PPSF_LANGUAGE_JAPANESE,
            library_id: uuid_v4(),
            name: String::new(),
            singer_name: "HATSUNE MIKU NT Original".to_owned(),
            stationaly_type: "ParamedNDR".to_owned(),
            synthesis_version: "V20200915_H".to_owned(),
            tail_silent: 0.05,
            vprm_morph_mode: "Linear".to_owned(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfDvlTrackEvent {
    #[serde(skip_serializing_if = "is_none", default = "some_false")]
    pub adjust_speed: Option<bool>,
    pub attack_speed_rate: i64,
    pub consonant_rate: i64,
    pub consonant_speed_rate: i64,
    #[serde(
        rename = "enabled",
        alias = "enable",
        skip_serializing_if = "is_none",
        default = "some_true"
    )]
    pub enabled: Option<bool>,
    pub length: i64,
    pub lyric: String,
    pub note_number: i64,
    #[serde(skip_serializing_if = "is_none", default = "default_envelope")]
    pub note_off_pit_envelope: Option<PpsfEnvelope>,
    #[serde(skip_serializing_if = "is_none", default = "default_envelope")]
    pub note_on_pit_envelope: Option<PpsfEnvelope>,
    #[serde(skip_serializing_if = "is_none", default = "default_envelope")]
    pub portamento_envelope: Option<PpsfEnvelope>,
    #[serde(skip_serializing_if = "is_none")]
    pub vib_depth: Option<PpsfEnvelope>,
    #[serde(skip_serializing_if = "is_none")]
    pub vib_rate: Option<PpsfEnvelope>,
    #[serde(skip_serializing_if = "is_none")]
    pub vib_setting_id: Option<i64>,
    pub portamento_type: i64,
    pub pos: i64,
    pub protected: bool,
    pub release_speed_rate: i64,
    pub symbols: String,
    #[serde(skip_serializing_if = "is_none", default = "some_false")]
    pub vcl_like_note_off: Option<bool>,
    #[serde(skip_serializing_if = "is_none", default = "some_false")]
    pub is_consonant_length_by_user: Option<bool>,
}

impl PpsfDvlTrackEvent {
    pub fn end_pos(&self) -> i64 {
        self.pos + self.length
    }
}

impl Default for PpsfDvlTrackEvent {
    fn default() -> Self {
        Self {
            adjust_speed: some_false(),
            attack_speed_rate: 800_000,
            consonant_rate: 950_000,
            consonant_speed_rate: 1_000_000,
            enabled: some_true(),
            length: 0,
            lyric: String::new(),
            note_number: 0,
            note_off_pit_envelope: default_envelope(),
            note_on_pit_envelope: default_envelope(),
            portamento_envelope: default_envelope(),
            vib_depth: None,
            vib_rate: None,
            vib_setting_id: None,
            portamento_type: 5,
            pos: 0,
            protected: false,
            release_speed_rate: 1_400_000,
            symbols: String::new(),
            vcl_like_note_off: some_false(),
            is_consonant_length_by_user: some_false(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfDvlTrackItem {
    #[serde(
        rename = "enabled",
        alias = "enable",
        skip_serializing_if = "is_none",
        default = "some_true"
    )]
    pub enabled: Option<bool>,
    pub events: Vec<PpsfDvlTrackEvent>,
    pub mixer: PpsfMixer,
    pub name: String,
    pub parameters: Vec<PpsfSeqParam>,
    pub plugin_output_bus_index: i64,
    pub singer: PpsfSinger,
}

impl Default for PpsfDvlTrackItem {
    fn default() -> Self {
        Self {
            enabled: some_true(),
            events: Vec::new(),
            mixer: PpsfMixer::default(),
            name: "HATSUNE MIKU NT Original".to_owned(),
            parameters: Vec::new(),
            plugin_output_bus_index: 0,
            singer: PpsfSinger::default(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfFileAudioData {
    pub file_path: String,
    pub tempo: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfAudioTrackEvent {
    pub file_audio_data: PpsfFileAudioData,
    pub playback_offset_sample: i64,
    pub tick_length: i64,
    pub tick_pos: i64,
    #[serde(
        rename = "enabled",
        alias = "enable",
        skip_serializing_if = "is_none",
        default = "some_true"
    )]
    pub enabled: Option<bool>,
}

impl Default for PpsfAudioTrackEvent {
    fn default() -> Self {
        Self {
            file_audio_data: PpsfFileAudioData::default(),
            playback_offset_sample: 0,
            tick_length: 0,
            tick_pos: 0,
            enabled: some_true(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfAudioTrackItem {
    pub block_size: i64,
    #[serde(
        rename = "enabled",
        alias = "enable",
        skip_serializing_if = "is_none",
        default = "some_true"
    )]
    pub enabled: Option<bool>,
    pub events: Vec<PpsfAudioTrackEvent>,
    pub input_channel: i64,
    pub mixer: PpsfMixer,
    pub name: String,
    pub output_channel: i64,
    pub sampling_rate: i64,
}

impl Default for PpsfAudioTrackItem {
    fn default() -> Self {
        Self {
            block_size: 279,
            enabled: some_true(),
            events: Vec::new(),
            input_channel: 0,
            mixer: PpsfMixer::default(),
            name: String::new(),
            output_channel: 2,
            sampling_rate: 48_000,
        }
    }
}

// ---------------------------------------------------------------------------
// Meter / tempo
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfBaseMeter {
    pub denomi: i64,
    pub nume: i64,
}

impl Default for PpsfBaseMeter {
    fn default() -> Self {
        Self {
            denomi: 4,
            nume: 4,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfMeter {
    pub denomi: i64,
    pub nume: i64,
    pub measure: i64,
}

impl Default for PpsfMeter {
    fn default() -> Self {
        Self {
            denomi: 4,
            nume: 4,
            measure: 0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfMeters {
    #[serde(rename = "const")]
    pub const_meter: PpsfBaseMeter,
    pub sequence: Vec<PpsfMeter>,
    pub use_sequence: bool,
}

impl Default for PpsfMeters {
    fn default() -> Self {
        Self {
            const_meter: PpsfBaseMeter::default(),
            sequence: Vec::new(),
            use_sequence: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfTempo {
    pub curve_type: i64,
    pub tick: i64,
    pub value: i64,
}

impl Default for PpsfTempo {
    fn default() -> Self {
        Self {
            curve_type: PpsfCurveType::NORMAL,
            tick: 0,
            value: 0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfTempos {
    #[serde(rename = "const")]
    pub const_tempo: i64,
    pub sequence: Vec<PpsfTempo>,
    pub use_sequence: bool,
}

impl Default for PpsfTempos {
    fn default() -> Self {
        Self {
            const_tempo: 1_200_000,
            sequence: Vec::new(),
            use_sequence: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Project root
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfInnerProject {
    pub audio_track: Vec<PpsfAudioTrackItem>,
    pub block_size: i64,
    #[serde(skip_serializing_if = "is_none")]
    pub loop_point: Option<PpsfLoopPoint>,
    pub meter: PpsfMeters,
    #[serde(skip_serializing_if = "is_none")]
    pub metronome: Option<PpsfMetronome>,
    pub name: String,
    pub sampling_rate: i64,
    pub singer_table: Vec<Value>,
    pub tempo: PpsfTempos,
    pub vocaloid_track: Vec<Value>,
    #[serde(skip_serializing_if = "is_none")]
    pub dvl_track: Option<Vec<PpsfDvlTrackItem>>,
}

impl Default for PpsfInnerProject {
    fn default() -> Self {
        Self {
            audio_track: Vec::new(),
            block_size: 279,
            loop_point: None,
            meter: PpsfMeters::default(),
            metronome: None,
            name: String::new(),
            sampling_rate: 48_000,
            singer_table: Vec::new(),
            tempo: PpsfTempos::default(),
            vocaloid_track: Vec::new(),
            dvl_track: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfRoot {
    pub app_ver: String,
    pub gui_settings: PpsfGuiSettings,
    pub ppsf_ver: String,
    pub project: PpsfInnerProject,
}

impl Default for PpsfRoot {
    fn default() -> Self {
        Self {
            app_ver: "3.0.0.0".to_owned(),
            gui_settings: PpsfGuiSettings::default(),
            ppsf_ver: "3.2".to_owned(),
            project: PpsfInnerProject::default(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PpsfProject {
    pub ppsf: PpsfRoot,
}

impl PpsfProject {
    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }

    /// `json.dumps(model_dump(by_alias=True, exclude_none=True, mode="json"),
    /// ensure_ascii=False, separators=(", ", ": "))`.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let mut buffer = Vec::new();
        {
            let formatter = PythonSeparators;
            let mut serializer = serde_json::Serializer::with_formatter(&mut buffer, formatter);
            self.serialize(&mut serializer)?;
        }
        Ok(String::from_utf8_lossy(&buffer).into_owned())
    }
}

/// `separators=(", ", ": ")` - arrays and objects stay on a single line, but
/// every separator keeps one trailing space like `json.dumps` with those
/// separators.
struct PythonSeparators;

impl serde_json::ser::Formatter for PythonSeparators {
    fn begin_array_value<W>(&mut self, writer: &mut W, first: bool) -> std::io::Result<()>
    where
        W: ?Sized + std::io::Write,
    {
        if first {
            Ok(())
        } else {
            writer.write_all(b", ")
        }
    }

    fn begin_object_key<W>(&mut self, writer: &mut W, first: bool) -> std::io::Result<()>
    where
        W: ?Sized + std::io::Write,
    {
        if first {
            Ok(())
        } else {
            writer.write_all(b", ")
        }
    }

    fn begin_object_value<W>(&mut self, writer: &mut W) -> std::io::Result<()>
    where
        W: ?Sized + std::io::Write,
    {
        writer.write_all(b": ")
    }
}

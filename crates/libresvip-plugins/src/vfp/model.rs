//! VOX Factory project model (`vfp`), port of `libresvip.plugins.vfp.model`.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

fn default_duration() -> f64 {
    0.25
}

fn default_velocity() -> i64 {
    1
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VoxVibratoParam {
    pub start: f64,
    pub phase: f64,
    #[serde(default = "default_frequency")]
    pub frequency: f64,
    pub amplitude: f64,
    #[serde(rename = "fadeIn", alias = "fade_in")]
    pub fade_in: f64,
    #[serde(rename = "fadeOut", alias = "fade_out")]
    pub fade_out: f64,
}

fn default_frequency() -> f64 {
    5.0
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VoxGrowlParam {
    pub start: f64,
    pub duration: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VoxCorrectionParam {
    pub modulation: f64,
    pub center: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VoxNote {
    pub time: f64,
    pub midi: i64,
    pub name: String,
    pub syllable: String,
    pub ticks: f64,
    #[serde(rename = "durationTicks", alias = "duration_ticks")]
    pub duration_ticks: f64,
    #[serde(default = "default_duration")]
    pub duration: f64,
    #[serde(default = "default_velocity")]
    pub velocity: i64,
    #[serde(rename = "noteType", alias = "note_type")]
    pub note_type: Option<String>,
    #[serde(rename = "vibratoDepth", alias = "vibrato_depth")]
    pub vibrato_depth: Option<f64>,
    #[serde(rename = "preBend", alias = "pre_bend")]
    pub pre_bend: Option<f64>,
    #[serde(rename = "postBend", alias = "post_bend")]
    pub post_bend: Option<f64>,
    #[serde(rename = "harmonicRatio", alias = "harmonic_ratio")]
    pub harmonic_ratio: Option<f64>,
    #[serde(rename = "pitchBends", alias = "pitch_bends")]
    pub pitch_bends: Vec<f64>,
    #[serde(rename = "vibratoParam", alias = "vibrato_param")]
    pub vibrato_param: Option<VoxVibratoParam>,
    #[serde(rename = "growlParam", alias = "growl_param")]
    pub growl_param: Option<VoxGrowlParam>,
    #[serde(rename = "correctionParam", alias = "correction_param")]
    pub correction_param: Option<VoxCorrectionParam>,
}

impl Default for VoxNote {
    fn default() -> Self {
        Self {
            time: 0.0,
            midi: 0,
            name: String::new(),
            syllable: String::new(),
            ticks: 0.0,
            duration_ticks: 0.0,
            duration: default_duration(),
            velocity: default_velocity(),
            note_type: None,
            vibrato_depth: None,
            pre_bend: None,
            post_bend: None,
            harmonic_ratio: None,
            pitch_bends: Vec::new(),
            vibrato_param: None,
            growl_param: None,
            correction_param: None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VoxMetadata {
    pub style: String,
    pub accent: String,
    pub transpose: i64,
    #[serde(rename = "harmonicRatio", alias = "harmonic_ratio")]
    pub harmonic_ratio: f64,
    #[serde(rename = "pitchDetection", alias = "pitch_detection")]
    pub pitch_detection: Option<String>,
    pub instrument: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VoxClip {
    pub name: String,
    #[serde(rename = "startQuarter", alias = "start_quarter")]
    pub start_quarter: f64,
    #[serde(rename = "offsetQuarter", alias = "offset_quarter")]
    pub offset_quarter: f64,
    pub length: f64,
    #[serde(rename = "useSource", alias = "use_source", default = "default_true")]
    pub use_source: bool,
    #[serde(rename = "audioDataKey", alias = "audio_data_key")]
    pub audio_data_key: Option<String>,
    #[serde(rename = "audioDataOrder", alias = "audio_data_order")]
    pub audio_data_order: Vec<String>,
    #[serde(rename = "audioDataQuarter", alias = "audio_data_quarter")]
    pub audio_data_quarter: f64,
    #[serde(rename = "noteBank", alias = "note_bank")]
    pub note_bank: IndexMap<String, VoxNote>,
    #[serde(rename = "noteOrder", alias = "note_order")]
    pub note_order: Vec<String>,
    #[serde(rename = "pitchPointBank", alias = "pitch_point_bank")]
    pub pitch_point_bank: IndexMap<String, serde_json::Value>,
    #[serde(rename = "pitchPointOrder", alias = "pitch_point_order")]
    pub pitch_point_order: Vec<String>,
    #[serde(rename = "nextNoteIndex", alias = "next_note_index")]
    pub next_note_index: i64,
    #[serde(rename = "pinnedAudioDataOrder", alias = "pinned_audio_data_order")]
    pub pinned_audio_data_order: Vec<String>,
    pub metadata: Option<VoxMetadata>,
    #[serde(rename = "sourceAudioDataKey", alias = "source_audio_data_key")]
    pub source_audio_data_key: Option<String>,
    #[serde(rename = "processKey", alias = "process_key")]
    pub process_key: Option<String>,
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(rename = "lengthType", alias = "length_type")]
    pub length_type: String,
}

impl Default for VoxClip {
    fn default() -> Self {
        Self {
            name: String::new(),
            start_quarter: 0.0,
            offset_quarter: 0.0,
            length: 0.0,
            use_source: true,
            audio_data_key: None,
            audio_data_order: Vec::new(),
            audio_data_quarter: 0.0,
            note_bank: IndexMap::new(),
            note_order: Vec::new(),
            pitch_point_bank: IndexMap::new(),
            pitch_point_order: Vec::new(),
            next_note_index: 0,
            pinned_audio_data_order: Vec::new(),
            metadata: None,
            source_audio_data_key: None,
            process_key: None,
            type_: "vocal".to_owned(),
            length_type: "quarter".to_owned(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VoxAudioViewProperty {
    pub view: String,
    pub colormap: String,
    pub window: String,
    #[serde(rename = "windowSize", alias = "window_size")]
    pub window_size: i64,
    #[serde(rename = "hopSize", alias = "hop_size")]
    pub hop_size: i64,
    #[serde(rename = "fMin", alias = "f_min")]
    pub f_min: f64,
    #[serde(rename = "fMax", alias = "f_max")]
    pub f_max: Option<f64>,
    #[serde(rename = "levelMin", alias = "level_min")]
    pub level_min: Option<f64>,
    #[serde(rename = "levelMax", alias = "level_max")]
    pub level_max: Option<f64>,
    #[serde(rename = "levelScale", alias = "level_scale")]
    pub level_scale: String,
    #[serde(rename = "numBins", alias = "num_bins")]
    pub num_bins: i64,
    #[serde(rename = "binsPerOctave", alias = "bins_per_octave")]
    pub bins_per_octave: i64,
}

impl Default for VoxAudioViewProperty {
    fn default() -> Self {
        Self {
            view: "waveform".to_owned(),
            colormap: "Heated Metal".to_owned(),
            window: "Hann".to_owned(),
            window_size: 1024,
            hop_size: 256,
            f_min: 27.5,
            f_max: None,
            level_min: None,
            level_max: None,
            level_scale: "dB".to_owned(),
            num_bins: 230,
            bins_per_octave: 24,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VoxTrack {
    pub name: String,
    pub instrument: Option<String>,
    pub h: i64,
    pub color: String,
    pub volume: f64,
    pub pan: f64,
    pub solo: bool,
    pub mute: bool,
    pub arm: bool,
    #[serde(rename = "clipOrder", alias = "clip_order")]
    pub clip_order: Vec<String>,
    #[serde(rename = "deviceBank", alias = "device_bank")]
    pub device_bank: IndexMap<String, serde_json::Value>,
    #[serde(rename = "deviceOrder", alias = "device_order")]
    pub device_order: Vec<String>,
    #[serde(rename = "devicePreset", alias = "device_preset")]
    pub device_preset: Option<String>,
    #[serde(rename = "audioViewProperty", alias = "audio_view_property")]
    pub audio_view_property: VoxAudioViewProperty,
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(rename = "clipBank", alias = "clip_bank")]
    pub clip_bank: IndexMap<String, VoxClip>,
}

impl Default for VoxTrack {
    fn default() -> Self {
        Self {
            name: String::new(),
            instrument: None,
            h: 3,
            color: "#7878f1".to_owned(),
            volume: 1.0,
            pan: 0.0,
            solo: false,
            mute: false,
            arm: false,
            clip_order: Vec::new(),
            device_bank: IndexMap::new(),
            device_order: Vec::new(),
            device_preset: None,
            audio_view_property: VoxAudioViewProperty::default(),
            type_: "vocal".to_owned(),
            clip_bank: IndexMap::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VoxSelectedClipBankItem {
    #[serde(rename = "trackKey", alias = "track_key")]
    pub track_key: String,
    #[serde(rename = "clipKey", alias = "clip_key")]
    pub clip_key: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VoxSelectedNoteBankItem {
    #[serde(rename = "trackKey", alias = "track_key")]
    pub track_key: String,
    #[serde(rename = "clipKey", alias = "clip_key")]
    pub clip_key: String,
    #[serde(rename = "noteKey", alias = "note_key")]
    pub note_key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VoxAudioData {
    #[serde(rename = "sampleRate", alias = "sample_rate")]
    pub sample_rate: i64,
    #[serde(rename = "numberOfChannels", alias = "number_of_channels")]
    pub number_of_channels: i64,
    #[serde(rename = "sampleLength", alias = "sample_length")]
    pub sample_length: i64,
    pub metadata: Option<VoxMetadata>,
    #[serde(rename = "pitchData", alias = "pitch_data")]
    pub pitch_data: Vec<f64>,
}

impl Default for VoxAudioData {
    fn default() -> Self {
        Self {
            sample_rate: 0,
            number_of_channels: 0,
            sample_length: 0,
            metadata: None,
            pitch_data: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VoxProject {
    pub version: String,
    pub tempo: f64,
    #[serde(rename = "timeSignature", alias = "time_signature")]
    pub time_signature: Vec<i64>,
    #[serde(rename = "loop")]
    pub loop_: Option<Vec<f64>>,
    #[serde(rename = "projectName", alias = "project_name")]
    pub project_name: String,
    #[serde(rename = "trackBank", alias = "track_bank")]
    pub track_bank: IndexMap<String, VoxTrack>,
    #[serde(rename = "trackOrder", alias = "track_order")]
    pub track_order: Vec<String>,
    #[serde(rename = "selectedTrackBank", alias = "selected_track_bank")]
    pub selected_track_bank: Vec<String>,
    #[serde(rename = "selectedClipBank", alias = "selected_clip_bank")]
    pub selected_clip_bank: Vec<VoxSelectedClipBankItem>,
    #[serde(rename = "selectedNoteBank", alias = "selected_note_bank")]
    pub selected_note_bank: Vec<VoxSelectedNoteBankItem>,
    #[serde(rename = "audioDataBank", alias = "audio_data_bank")]
    pub audio_data_bank: IndexMap<String, VoxAudioData>,
}

impl Default for VoxProject {
    fn default() -> Self {
        Self {
            version: "0.12.0".to_owned(),
            tempo: 120.0,
            time_signature: Vec::new(),
            loop_: None,
            project_name: "Untitled Project".to_owned(),
            track_bank: IndexMap::new(),
            track_order: Vec::new(),
            selected_track_bank: Vec::new(),
            selected_clip_bank: Vec::new(),
            selected_note_bank: Vec::new(),
            audio_data_bank: IndexMap::new(),
        }
    }
}

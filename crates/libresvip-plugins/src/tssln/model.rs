//! Typed view of the parts of the VoiSona project model used by the converter
//! (port of `libresvip.plugins.tssln.model`).
//!
//! Only the fields the parser and generator actually touch are materialised;
//! unknown attributes and child nodes are ignored, exactly like pydantic's
//! default `extra="ignore"` configuration.

use libresvip_core::error::{LibreSvipError, Result};

use super::value_tree::{
    attr_f64, attr_i64, attr_str, attr_string_or, required_f64, required_i64, required_string,
    Node, Variant,
};

/// Default `VersionOfAppFileSaved` of a `VoiSonaStateInformation`.
pub const DEFAULT_STATE_VERSION: &str = "1.8.0.17";
/// Default `VersionOfAppFileSaved` of a `VoiSonaProject`.
pub const DEFAULT_PROJECT_VERSION: &str = "1.8.0.17";

pub const TRACK_TYPE_SINGING: i64 = 0;
pub const TRACK_TYPE_AUDIO: i64 = 2;

#[derive(Clone, Debug, Default)]
pub struct VoiSonaProject {
    pub tracks: Vec<VoiSonaTrack>,
    pub version_of_app_file_saved: String,
}

#[derive(Clone, Debug, Default)]
pub struct VoiSonaTrack {
    pub track: Vec<VoiSonaTrackItem>,
}

#[derive(Clone, Debug)]
pub enum VoiSonaTrackItem {
    Audio(VoiSonaAudioTrackItem),
    Singing(VoiSonaSingingTrackItem),
}

impl VoiSonaTrackItem {
    pub fn name(&self) -> &str {
        match self {
            Self::Audio(item) => &item.base.name,
            Self::Singing(item) => &item.base.name,
        }
    }
}

#[derive(Clone, Debug)]
pub struct VoiSonaBaseTrackItem {
    pub name: String,
    pub state: i64,
    pub volume: f64,
    pub pan: f64,
}

impl VoiSonaBaseTrackItem {
    fn from_node(node: &Node, struct_name: &str) -> Result<Self> {
        Ok(Self {
            name: required_string(node, "Name", struct_name)?,
            state: attr_i64(node, "State").unwrap_or(0),
            volume: attr_f64(node, "Volume").unwrap_or(0.0),
            pan: attr_f64(node, "Pan").unwrap_or(0.0),
        })
    }
}

#[derive(Clone, Debug)]
pub struct VoiSonaAudioEventItem {
    pub path: String,
    pub offset: f64,
}

impl VoiSonaAudioEventItem {
    fn from_node(node: &Node) -> Result<Self> {
        Ok(Self {
            path: required_string(node, "Path", "VoiSonaAudioEventItem")?,
            offset: required_f64(node, "Offset", "VoiSonaAudioEventItem")?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct VoiSonaAudioTrackItem {
    pub base: VoiSonaBaseTrackItem,
    pub audio_event: Option<Vec<VoiSonaAudioEventItem>>,
}

impl VoiSonaAudioTrackItem {
    fn from_node(node: &Node) -> Result<Self> {
        let base = VoiSonaBaseTrackItem::from_node(node, "VoiSonaAudioTrackItem")?;
        let events = node
            .children_named_vec("AudioEvent")
            .into_iter()
            .map(VoiSonaAudioEventItem::from_node)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            base,
            audio_event: if events.is_empty() {
                None
            } else {
                Some(events)
            },
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct VoiSonaPluginData {
    pub state_information: VoiSonaStateInformation,
}

#[derive(Clone, Debug)]
pub struct VoiSonaSingingTrackItem {
    pub base: VoiSonaBaseTrackItem,
    pub plugin_data: VoiSonaPluginData,
}

impl VoiSonaSingingTrackItem {
    fn from_node(node: &Node) -> Result<Self> {
        let base = VoiSonaBaseTrackItem::from_node(node, "VoiSonaSingingTrackItem")?;
        let plugin_data = match node.attr("PluginData") {
            Some(Variant::Binary(bytes)) => {
                let state_node = super::value_tree::parse(bytes)?;
                // `build_tree_dict` nests the decoded node under its own name;
                // VoiSonaPluginData only accepts `StateInformation`, so any
                // other node name silently falls back to the defaults.
                if state_node.name == "StateInformation" {
                    VoiSonaPluginData {
                        state_information: VoiSonaStateInformation::from_node(&state_node)?,
                    }
                } else {
                    VoiSonaPluginData::default()
                }
            }
            Some(_) => {
                return Err(LibreSvipError::Validation(
                    "PluginData must be binary".to_owned(),
                ))
            }
            None => VoiSonaPluginData::default(),
        };
        Ok(Self { base, plugin_data })
    }
}

#[derive(Clone, Debug)]
pub struct VoiSonaVoiceInformation {
    pub character_name: String,
}

impl VoiSonaVoiceInformation {
    fn from_node(node: &Node) -> Result<Self> {
        Ok(Self {
            character_name: required_string(node, "CharacterName", "VoiSonaVoiceInformation")?,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct VoiSonaStateInformation {
    pub voice_information: Option<Vec<VoiSonaVoiceInformation>>,
    pub song: Option<Vec<VoiSonaSongItem>>,
    pub parameter: Option<Vec<VoiSonaParametersItem>>,
}

impl VoiSonaStateInformation {
    pub fn from_node(node: &Node) -> Result<Self> {
        let voice_information = Some(
            node.children_named_vec("VoiceInformation")
                .into_iter()
                .map(VoiSonaVoiceInformation::from_node)
                .collect::<Result<Vec<_>>>()?,
        )
        .filter(|items| !items.is_empty());
        let song = Some(
            node.children_named_vec("Song")
                .into_iter()
                .map(VoiSonaSongItem::from_node)
                .collect::<Result<Vec<_>>>()?,
        )
        .filter(|items| !items.is_empty());
        let parameter = Some(
            node.children_named_vec("Parameter")
                .into_iter()
                .map(VoiSonaParametersItem::from_node)
                .collect::<Result<Vec<_>>>()?,
        )
        .filter(|items| !items.is_empty());
        Ok(Self {
            voice_information,
            song,
            parameter,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct VoiSonaSongItem {
    pub tempo: Vec<VoiSonaTempoItem>,
    pub beat: Vec<VoiSonaBeatItem>,
    pub score: Vec<VoiSonaScoreItem>,
}

impl VoiSonaSongItem {
    fn from_node(node: &Node) -> Result<Self> {
        Ok(Self {
            tempo: node
                .children_named_vec("Tempo")
                .into_iter()
                .map(VoiSonaTempoItem::from_node)
                .collect::<Result<Vec<_>>>()?,
            beat: node
                .children_named_vec("Beat")
                .into_iter()
                .map(VoiSonaBeatItem::from_node)
                .collect::<Result<Vec<_>>>()?,
            score: node
                .children_named_vec("Score")
                .into_iter()
                .map(VoiSonaScoreItem::from_node)
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct VoiSonaTempoItem {
    pub sound: Vec<VoiSonaSoundItem>,
}

impl VoiSonaTempoItem {
    fn from_node(node: &Node) -> Result<Self> {
        Ok(Self {
            sound: node
                .children_named_vec("Sound")
                .into_iter()
                .map(VoiSonaSoundItem::from_node)
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct VoiSonaSoundItem {
    pub clock: i64,
    pub tempo: f64,
}

impl VoiSonaSoundItem {
    fn from_node(node: &Node) -> Result<Self> {
        Ok(Self {
            clock: required_i64(node, "Clock", "VoiSonaSoundItem")?,
            tempo: required_f64(node, "Tempo", "VoiSonaSoundItem")?,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct VoiSonaBeatItem {
    pub time: Vec<VoiSonaTimeItem>,
}

impl VoiSonaBeatItem {
    fn from_node(node: &Node) -> Result<Self> {
        Ok(Self {
            time: node
                .children_named_vec("Time")
                .into_iter()
                .map(VoiSonaTimeItem::from_node)
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct VoiSonaTimeItem {
    pub clock: i64,
    pub beats: i64,
    pub beat_type: i64,
}

impl VoiSonaTimeItem {
    fn from_node(node: &Node) -> Result<Self> {
        Ok(Self {
            clock: required_i64(node, "Clock", "VoiSonaTimeItem")?,
            beats: required_i64(node, "Beats", "VoiSonaTimeItem")?,
            beat_type: required_i64(node, "BeatType", "VoiSonaTimeItem")?,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct VoiSonaScoreItem {
    pub note: Vec<VoiSonaNoteItem>,
}

impl VoiSonaScoreItem {
    fn from_node(node: &Node) -> Result<Self> {
        Ok(Self {
            note: node
                .children_named_vec("Note")
                .into_iter()
                .map(VoiSonaNoteItem::from_node)
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct VoiSonaNoteItem {
    pub clock: i64,
    pub duration: i64,
    pub pitch_step: i64,
    pub pitch_octave: i64,
    pub lyric: String,
    pub phoneme: Option<String>,
}

impl VoiSonaNoteItem {
    fn from_node(node: &Node) -> Result<Self> {
        Ok(Self {
            clock: required_i64(node, "Clock", "VoiSonaNoteItem")?,
            duration: required_i64(node, "Duration", "VoiSonaNoteItem")?,
            pitch_step: required_i64(node, "PitchStep", "VoiSonaNoteItem")?,
            pitch_octave: required_i64(node, "PitchOctave", "VoiSonaNoteItem")?,
            lyric: required_string(node, "Lyric", "VoiSonaNoteItem")?,
            phoneme: attr_str(node, "Phoneme"),
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct VoiSonaParametersItem {
    pub log_f0: Option<Vec<VoiSonaParameterItem>>,
    pub vib_amp: Option<Vec<VoiSonaParameterItem>>,
    pub vib_frq: Option<Vec<VoiSonaParameterItem>>,
}

impl VoiSonaParametersItem {
    fn from_node(node: &Node) -> Result<Self> {
        let curve_list = |name: &str| -> Result<Option<Vec<VoiSonaParameterItem>>> {
            Ok(Some(
                node.children_named_vec(name)
                    .into_iter()
                    .map(VoiSonaParameterItem::from_node)
                    .collect::<Result<Vec<_>>>()?,
            )
            .filter(|items| !items.is_empty()))
        };
        Ok(Self {
            log_f0: curve_list("LogF0")?,
            vib_amp: curve_list("VibAmp")?,
            vib_frq: curve_list("VibFrq")?,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct VoiSonaParameterItem {
    pub length: i64,
    pub data: Vec<VoiSonaPointData>,
}

impl VoiSonaParameterItem {
    fn from_node(node: &Node) -> Result<Self> {
        Ok(Self {
            length: required_i64(node, "Length", "VoiSonaParameterItem")?,
            data: node
                .children_named_vec("Data")
                .into_iter()
                .map(VoiSonaPointData::from_node)
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct VoiSonaPointData {
    pub index: Option<i64>,
    pub repeat: Option<i64>,
    pub value: f64,
}

impl VoiSonaPointData {
    fn from_node(node: &Node) -> Result<Self> {
        Ok(Self {
            index: attr_i64(node, "Index"),
            repeat: attr_i64(node, "Repeat"),
            value: required_f64(node, "Value", "VoiSonaPointData")?,
        })
    }
}

// ---------------------------------------------------------------------------
// Conversion from the value tree
// ---------------------------------------------------------------------------

/// `VoiSonaProject.model_validate(tree_dict["TSSolution"])`.
pub fn project_from_node(root: &Node) -> Result<VoiSonaProject> {
    if root.name != "TSSolution" {
        return Err(LibreSvipError::Validation(format!(
            "unexpected project root: {}",
            root.name
        )));
    }
    let tracks = root
        .children_named_vec("Tracks")
        .into_iter()
        .map(|node| {
            Ok(VoiSonaTrack {
                track: node
                    .children_named_vec("Track")
                    .into_iter()
                    .map(track_item_from_node)
                    .collect::<Result<Vec<_>>>()?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(VoiSonaProject {
        tracks,
        version_of_app_file_saved: attr_string_or(
            root,
            "VersionOfAppFileSaved",
            DEFAULT_PROJECT_VERSION,
        ),
    })
}

fn track_item_from_node(node: &Node) -> Result<VoiSonaTrackItem> {
    // `VoiSonaTrackItem` is a discriminated union on `Type`; pydantic reports a
    // missing tag instead of using the field default.
    let track_type = attr_i64(node, "Type").ok_or_else(|| {
        LibreSvipError::Validation("VoiSonaTrackItem: missing union tag Type".to_owned())
    })?;
    match track_type {
        TRACK_TYPE_SINGING => Ok(VoiSonaTrackItem::Singing(
            VoiSonaSingingTrackItem::from_node(node)?,
        )),
        TRACK_TYPE_AUDIO => Ok(VoiSonaTrackItem::Audio(VoiSonaAudioTrackItem::from_node(
            node,
        )?)),
        other => Err(LibreSvipError::Validation(format!(
            "unknown track type: {other}"
        ))),
    }
}

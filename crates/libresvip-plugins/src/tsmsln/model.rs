//! Typed view of the parts of the VoiSona Mobile project model used by the
//! converter (port of `libresvip.plugins.tsmsln.model`).

use libresvip_core::error::{LibreSvipError, Result};

use super::value_tree::{
    attr_i64, attr_str, attr_string_or, required_f64, required_i64, required_string, Node,
    Variant,
};

/// Default `VersionOfAppFileSaved` of a mobile state information.
pub const DEFAULT_STATE_VERSION: &str = "1.12.1.0";
/// Default `VersionOfAppFileSaved` of a mobile project.
pub const DEFAULT_PROJECT_VERSION: &str = "1.12.1.0";

#[derive(Clone, Debug, Default)]
pub struct VoiSonaMobileProject {
    pub mobile_singer: Vec<VoiSonaMobileSinger>,
    pub version_of_app_file_saved: String,
}

#[derive(Clone, Debug, Default)]
pub struct VoiSonaMobileSinger {
    pub mobile_singer_data: VoiSonaMobileSingerData,
}

#[derive(Clone, Debug, Default)]
pub struct VoiSonaMobileSingerData {
    pub state_information: VoiSonaMobileStateInformation,
}

#[derive(Clone, Debug, Default)]
pub struct VoiSonaMobileStateInformation {
    pub song: Option<Vec<VoiSonaMobileSongItem>>,
    pub parameter: Option<Vec<VoiSonaMobileParametersItem>>,
}

impl VoiSonaMobileStateInformation {
    pub fn from_node(node: &Node) -> Result<Self> {
        let song = Some(
            node.children_named_vec("Song")
                .into_iter()
                .map(VoiSonaMobileSongItem::from_node)
                .collect::<Result<Vec<_>>>()?,
        )
        .filter(|items| !items.is_empty());
        let parameter = Some(
            node.children_named_vec("Parameter")
                .into_iter()
                .map(VoiSonaMobileParametersItem::from_node)
                .collect::<Result<Vec<_>>>()?,
        )
        .filter(|items| !items.is_empty());
        Ok(Self { song, parameter })
    }
}

#[derive(Clone, Debug, Default)]
pub struct VoiSonaMobileSongItem {
    pub tempo: Vec<VoiSonaMobileTempoItem>,
    pub beat: Vec<VoiSonaMobileBeatItem>,
    pub score: Vec<VoiSonaMobileScoreItem>,
}

impl VoiSonaMobileSongItem {
    fn from_node(node: &Node) -> Result<Self> {
        Ok(Self {
            tempo: node
                .children_named_vec("Tempo")
                .into_iter()
                .map(VoiSonaMobileTempoItem::from_node)
                .collect::<Result<Vec<_>>>()?,
            beat: node
                .children_named_vec("Beat")
                .into_iter()
                .map(VoiSonaMobileBeatItem::from_node)
                .collect::<Result<Vec<_>>>()?,
            score: node
                .children_named_vec("Score")
                .into_iter()
                .map(VoiSonaMobileScoreItem::from_node)
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct VoiSonaMobileTempoItem {
    pub sound: Vec<VoiSonaMobileSoundItem>,
}

impl VoiSonaMobileTempoItem {
    fn from_node(node: &Node) -> Result<Self> {
        Ok(Self {
            sound: node
                .children_named_vec("Sound")
                .into_iter()
                .map(VoiSonaMobileSoundItem::from_node)
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct VoiSonaMobileSoundItem {
    pub clock: i64,
    pub tempo: f64,
}

impl VoiSonaMobileSoundItem {
    fn from_node(node: &Node) -> Result<Self> {
        Ok(Self {
            clock: required_i64(node, "Clock", "VoiSonaMobileSoundItem")?,
            tempo: required_f64(node, "Tempo", "VoiSonaMobileSoundItem")?,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct VoiSonaMobileBeatItem {
    pub time: Vec<VoiSonaMobileTimeItem>,
}

impl VoiSonaMobileBeatItem {
    fn from_node(node: &Node) -> Result<Self> {
        Ok(Self {
            time: node
                .children_named_vec("Time")
                .into_iter()
                .map(VoiSonaMobileTimeItem::from_node)
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct VoiSonaMobileTimeItem {
    pub clock: i64,
    pub beats: i64,
    pub beat_type: i64,
}

impl VoiSonaMobileTimeItem {
    fn from_node(node: &Node) -> Result<Self> {
        Ok(Self {
            clock: required_i64(node, "Clock", "VoiSonaMobileTimeItem")?,
            beats: required_i64(node, "Beats", "VoiSonaMobileTimeItem")?,
            beat_type: required_i64(node, "BeatType", "VoiSonaMobileTimeItem")?,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct VoiSonaMobileScoreItem {
    pub note: Vec<VoiSonaMobileNoteItem>,
}

impl VoiSonaMobileScoreItem {
    fn from_node(node: &Node) -> Result<Self> {
        Ok(Self {
            note: node
                .children_named_vec("Note")
                .into_iter()
                .map(VoiSonaMobileNoteItem::from_node)
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct VoiSonaMobileNoteItem {
    pub clock: i64,
    pub duration: i64,
    pub pitch_step: i64,
    pub pitch_octave: i64,
    pub lyric: String,
    pub phoneme: Option<String>,
}

impl VoiSonaMobileNoteItem {
    fn from_node(node: &Node) -> Result<Self> {
        Ok(Self {
            clock: required_i64(node, "Clock", "VoiSonaMobileNoteItem")?,
            duration: required_i64(node, "Duration", "VoiSonaMobileNoteItem")?,
            pitch_step: required_i64(node, "PitchStep", "VoiSonaMobileNoteItem")?,
            pitch_octave: required_i64(node, "PitchOctave", "VoiSonaMobileNoteItem")?,
            lyric: required_string(node, "Lyric", "VoiSonaMobileNoteItem")?,
            phoneme: attr_str(node, "Phoneme"),
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct VoiSonaMobileParametersItem {
    pub log_f0: Option<Vec<VoiSonaMobileParameterItem>>,
    pub vib_amp: Option<Vec<VoiSonaMobileParameterItem>>,
    pub vib_frq: Option<Vec<VoiSonaMobileParameterItem>>,
}

impl VoiSonaMobileParametersItem {
    fn from_node(node: &Node) -> Result<Self> {
        let curve_list = |name: &str| -> Result<Option<Vec<VoiSonaMobileParameterItem>>> {
            Ok(Some(
                node.children_named_vec(name)
                    .into_iter()
                    .map(VoiSonaMobileParameterItem::from_node)
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
pub struct VoiSonaMobileParameterItem {
    pub length: i64,
    pub data: Vec<VoiSonaMobilePointData>,
}

impl VoiSonaMobileParameterItem {
    fn from_node(node: &Node) -> Result<Self> {
        Ok(Self {
            length: required_i64(node, "Length", "VoiSonaMobileParameterItem")?,
            data: node
                .children_named_vec("Data")
                .into_iter()
                .map(VoiSonaMobilePointData::from_node)
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct VoiSonaMobilePointData {
    pub index: Option<i64>,
    pub repeat: Option<i64>,
    pub value: f64,
}

impl VoiSonaMobilePointData {
    fn from_node(node: &Node) -> Result<Self> {
        Ok(Self {
            index: attr_i64(node, "Index"),
            repeat: attr_i64(node, "Repeat"),
            value: required_f64(node, "Value", "VoiSonaMobilePointData")?,
        })
    }
}

/// `VoiSonaMobileProject.model_validate(tree_dict["MobileSongEditor"])`.
pub fn project_from_node(root: &Node) -> Result<VoiSonaMobileProject> {
    if root.name != "MobileSongEditor" {
        return Err(LibreSvipError::Validation(format!(
            "unexpected project root: {}",
            root.name
        )));
    }
    let mobile_singer = root
        .children_named_vec("MobileSinger")
        .into_iter()
        .map(|node| {
            let mobile_singer_data = match node.attr("ModileSingerData") {
                Some(Variant::Binary(bytes)) => {
                    let state_node = super::value_tree::parse(bytes)?;
                    // Only a node literally named `StateInformation` is accepted
                    // by the pydantic model; anything else is ignored.
                    if state_node.name == "StateInformation" {
                        VoiSonaMobileSingerData {
                            state_information: VoiSonaMobileStateInformation::from_node(
                                &state_node,
                            )?,
                        }
                    } else {
                        VoiSonaMobileSingerData::default()
                    }
                }
                Some(_) => {
                    return Err(LibreSvipError::Validation(
                        "ModileSingerData must be binary".to_owned(),
                    ))
                }
                None => VoiSonaMobileSingerData::default(),
            };
            Ok(VoiSonaMobileSinger { mobile_singer_data })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(VoiSonaMobileProject {
        mobile_singer,
        version_of_app_file_saved: attr_string_or(
            root,
            "VersionOfAppFileSaved",
            DEFAULT_PROJECT_VERSION,
        ),
    })
}

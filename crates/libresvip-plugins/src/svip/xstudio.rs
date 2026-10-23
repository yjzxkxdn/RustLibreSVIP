//! Port of `libresvip.plugins.svip.msnrbf.xstudio_models`.
//!
//! Python reflects over the dataclasses in `xstudio_models.py` (`inspect.getdoc`
//! for the CLR class name, `dataclasses.fields` + `metadata["order"]` for the
//! member order, `metadata["alias"]` for the .NET member name). Rust has no
//! reflection, so the same information is spelled out here as tables and the
//! model itself is a generic class/field tree ([`XsClass`]) that both the NRBF
//! reader and the generator/writer use.

use libresvip_core::error::{LibreSvipError, Result};

/// `LIBRARY_NAME_SINGING_TOOL_MODEL`.
pub const LIBRARY_NAME_SINGING_TOOL_MODEL: &str =
    "SingingTool.Model, Version=1.0.0.0, Culture=neutral, PublicKeyToken=null";
/// `LIBRARY_NAME_SINGING_TOOL_LIBRARY`.
pub const LIBRARY_NAME_SINGING_TOOL_LIBRARY: &str =
    "SingingTool.Library, Version=1.0.0.0, Culture=neutral, PublicKeyToken=null";

/// `VALUE_LIST_VERSION_SONG_TEMPO`.
pub const VALUE_LIST_VERSION_SONG_TEMPO: i64 = 2;
/// `VALUE_LIST_VERSION_SONG_BEAT`.
pub const VALUE_LIST_VERSION_SONG_BEAT: i64 = 2;
/// `VALUE_LIST_VERSION_SONG_ITRACK`.
pub const VALUE_LIST_VERSION_SONG_ITRACK: i64 = 4;
/// `VALUE_LIST_VERSION_SONG_NOTE`.
pub const VALUE_LIST_VERSION_SONG_NOTE: i64 = 1054;

/// `MIN_NOTE_DURATION`.
pub const MIN_NOTE_DURATION: f64 = 0.045;
/// `MAX_NOTE_DURATION`.
pub const MAX_NOTE_DURATION: f64 = 20.0;

/// Names of the X Studio model classes (`inspect.getdoc` of each dataclass).
pub mod class {
    pub const APP_MODEL: &str = "SingingTool.Model.AppModel";
    pub const SONG_TEMPO: &str = "SingingTool.Model.SingingGeneralConcept.SongTempo";
    pub const SONG_BEAT: &str = "SingingTool.Model.SingingGeneralConcept.SongBeat";
    pub const BEAT_SIZE: &str = "SingingTool.Model.SingingGeneralConcept.BeatSize";
    pub const ITRACK: &str = "SingingTool.Model.ITrack";
    pub const SINGING_TRACK: &str = "SingingTool.Model.SingingTrack";
    pub const INSTRUMENT_TRACK: &str = "SingingTool.Model.InstrumentTrack";
    pub const NOTE: &str = "SingingTool.Model.Note";
    pub const NOTE_HEAD_TAG: &str = "SingingTool.Model.NoteHeadTag";
    pub const NOTE_PHONE_INFO: &str = "SingingTool.Model.NotePhoneInfo";
    pub const VIBRATO_STYLE: &str = "SingingTool.Model.VibratoStyle";
    pub const VIBRATO_PERCENT_INFO: &str = "SingingTool.Model.VibratoPercentInfo";
    pub const LINE_PARAM: &str = "SingingTool.Model.Line.LineParam";
    pub const REVERB_PRESET: &str = "SingingTool.Library.Audio.ReverbPreset";
    pub const SERIAL_ITEM_LIST: &str = "SingingTool.Library.SerialOverlapableItemList";
    pub const GENERIC_LIST: &str = "System.Collections.Generic.List";
}

/// `to_backing_field`.
pub fn backing_field(key: &str) -> String {
    format!("<{key}>k__BackingField")
}

pub fn xs_error(message: impl Into<String>) -> LibreSvipError {
    LibreSvipError::Parse(message.into())
}

/// A value of the generic X Studio model tree.
#[derive(Clone, Debug, PartialEq)]
pub enum XsValue {
    Str(String),
    Bool(bool),
    /// `int` / `enum.IntEnum` members.
    Int(i64),
    /// `float` members written as `Single`.
    F32(f32),
    /// `float` members written as `Double`.
    F64(f64),
    Bytes(Vec<u8>),
    /// `None` — serialised as an `ObjectNull` record.
    Null,
    Class(Box<XsClass>),
    List(Vec<XsValue>),
}

impl XsValue {
    pub fn as_class(&self) -> Result<&XsClass> {
        match self {
            XsValue::Class(class) => Ok(class),
            other => Err(xs_error(format!(
                "expected a class value, found {}",
                other.kind()
            ))),
        }
    }

    pub fn as_list(&self) -> Result<&[XsValue]> {
        match self {
            XsValue::List(items) => Ok(items),
            other => Err(xs_error(format!(
                "expected a list value, found {}",
                other.kind()
            ))),
        }
    }

    pub fn as_str(&self) -> Result<&str> {
        match self {
            XsValue::Str(value) => Ok(value),
            other => Err(xs_error(format!(
                "expected a string value, found {}",
                other.kind()
            ))),
        }
    }

    /// Lenient numeric access (the file stores `Int16`/`Int32`/`UInt32`…).
    pub fn as_i64(&self) -> Result<i64> {
        match self {
            XsValue::Int(value) => Ok(*value),
            XsValue::Bool(value) => Ok(i64::from(*value)),
            XsValue::F32(value) => Ok(f64::from(*value) as i64),
            XsValue::F64(value) => Ok(*value as i64),
            XsValue::Bytes(bytes) => {
                let array: [u8; 8] = bytes
                    .get(..8)
                    .and_then(|slice| slice.try_into().ok())
                    .ok_or_else(|| xs_error("integer value has the wrong size"))?;
                Ok(i64::from_le_bytes(array))
            }
            other => Err(xs_error(format!(
                "expected an integer value, found {}",
                other.kind()
            ))),
        }
    }

    pub fn as_f64(&self) -> Result<f64> {
        match self {
            XsValue::F64(value) => Ok(*value),
            XsValue::F32(value) => Ok(f64::from(*value)),
            XsValue::Int(value) => Ok(*value as f64),
            XsValue::Bool(value) => Ok(f64::from(u8::from(*value))),
            other => Err(xs_error(format!(
                "expected a float value, found {}",
                other.kind()
            ))),
        }
    }

    pub fn as_bool(&self) -> Result<bool> {
        match self {
            XsValue::Bool(value) => Ok(*value),
            XsValue::Int(value) => Ok(*value != 0),
            other => Err(xs_error(format!(
                "expected a boolean value, found {}",
                other.kind()
            ))),
        }
    }

    pub fn as_bytes(&self) -> Result<&[u8]> {
        match self {
            XsValue::Bytes(bytes) => Ok(bytes),
            other => Err(xs_error(format!(
                "expected a byte array, found {}",
                other.kind()
            ))),
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, XsValue::Null)
    }

    pub fn kind(&self) -> &'static str {
        match self {
            XsValue::Str(_) => "string",
            XsValue::Bool(_) => "boolean",
            XsValue::Int(_) => "integer",
            XsValue::F32(_) | XsValue::F64(_) => "float",
            XsValue::Bytes(_) => "bytes",
            XsValue::Null => "null",
            XsValue::Class(_) => "class",
            XsValue::List(_) => "list",
        }
    }
}

/// The declared .NET type of a model field, as far as the writer needs it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeclType {
    /// `str`.
    Str,
    /// `bool`.
    Bool,
    /// `int` / `enum.IntEnum` (written as `Int32`).
    Int,
    /// `float` written as `Single`.
    F32,
    /// `float` written as `Double` (`pan`, `sample_rate`, `volume`).
    F64,
    /// `bytes` (written as a primitive `Byte` array).
    Bytes,
    /// `list[T]` / `XSBuf[T]` / `XSBufList[T]`; `declared` is the element class
    /// name used in the member type information (`T[]`).
    List { declared: &'static str },
    /// A dataclass field; `generic` is the generic argument when the class name
    /// is a `…List` generic (`List<T>` / `SerialOverlapableItemList<T>`).
    Class {
        name: &'static str,
        generic: Option<&'static str>,
    },
    /// `SerialItemList`1+_buf`: shares the object id of the previous field.
    SharedClass {
        name: &'static str,
        generic: Option<&'static str>,
    },
    /// A dataclass whose type has a `value` class attribute (`XSNoteHeadTag`,
    /// `XSReverbPreset`): the writer emits it inline with a negative object id.
    InlineClass { name: &'static str },
    /// Only used by reader built trees (never serialised).
    Unknown,
}

/// One member of a model class.
#[derive(Clone, Debug, PartialEq)]
pub struct XsField {
    /// Python dataclass field name (e.g. `note_list`).
    pub name: &'static str,
    /// .NET member name (`metadata["alias"]`).
    pub alias: &'static str,
    pub decl: DeclType,
    pub value: XsValue,
}

impl XsField {
    pub fn new(
        name: &'static str,
        alias: &'static str,
        decl: DeclType,
        value: XsValue,
    ) -> Self {
        Self {
            name,
            alias,
            decl,
            value,
        }
    }
}

/// A model object: a CLR class name plus its ordered members.
#[derive(Clone, Debug, PartialEq)]
pub struct XsClass {
    pub name: &'static str,
    pub fields: Vec<XsField>,
}

impl XsClass {
    pub fn new(name: &'static str, fields: Vec<XsField>) -> Self {
        Self { name, fields }
    }

    pub fn field(&self, name: &str) -> Result<&XsValue> {
        self.fields
            .iter()
            .find(|field| field.name == name)
            .map(|field| &field.value)
            .ok_or_else(|| xs_error(format!("{} has no member {}", self.name, name)))
    }

    pub fn as_class(&self, name: &str) -> Result<&XsClass> {
        self.field(name)?.as_class()
    }

    pub fn opt_class(&self, name: &str) -> Result<Option<&XsClass>> {
        match self.field(name)? {
            XsValue::Null => Ok(None),
            value => Ok(Some(value.as_class()?)),
        }
    }

    /// Like [`XsClass::opt_class`], but a member the record does not carry at
    /// all is treated as `None` (Python's dataclass defaults for optional
    /// members, e.g. `edited_power_line` in SVIP 6 projects).
    pub fn opt_class_or_missing(&self, name: &str) -> Result<Option<&XsClass>> {
        match self.maybe(name) {
            None | Some(XsValue::Null) => Ok(None),
            Some(value) => Ok(Some(value.as_class()?)),
        }
    }

    /// The member value, if the record carries it.
    pub fn maybe(&self, name: &str) -> Option<&XsValue> {
        self.fields
            .iter()
            .find(|field| field.name == name)
            .map(|field| &field.value)
    }

    pub fn as_list(&self, name: &str) -> Result<&[XsValue]> {
        self.field(name)?.as_list()
    }

    /// Members of a list field, each required to be a class object.
    pub fn class_list(&self, name: &str) -> Result<Vec<&XsClass>> {
        self.as_list(name)?
            .iter()
            .map(XsValue::as_class)
            .collect()
    }

    pub fn string(&self, name: &str) -> Result<&str> {
        self.field(name)?.as_str()
    }

    pub fn string_or_empty(&self, name: &str) -> Result<String> {
        match self.field(name)? {
            XsValue::Null => Ok(String::new()),
            value => Ok(value.as_str()?.to_owned()),
        }
    }

    pub fn i64(&self, name: &str) -> Result<i64> {
        self.field(name)?.as_i64()
    }

    pub fn i32(&self, name: &str) -> Result<i32> {
        let value = self.i64(name)?;
        i32::try_from(value)
            .map_err(|_| xs_error(format!("{}::{} does not fit into Int32", self.name, name)))
    }

    pub fn f64(&self, name: &str) -> Result<f64> {
        self.field(name)?.as_f64()
    }

    pub fn f32(&self, name: &str) -> Result<f32> {
        Ok(self.f64(name)? as f32)
    }

    pub fn bool(&self, name: &str) -> Result<bool> {
        self.field(name)?.as_bool()
    }

    pub fn bytes(&self, name: &str) -> Result<&[u8]> {
        self.field(name)?.as_bytes()
    }
}

// ---------------------------------------------------------------------------
// Alias tables (Python: `metadata["alias"]` + `alias2key`)
// ---------------------------------------------------------------------------

/// `(alias, python field name)` pairs of one model class, in member order.
pub type AliasTable = &'static [(&'static str, &'static str)];

const APP_MODEL_FIELDS: AliasTable = &[
    ("<ProjectFilePath>k__BackingField", "project_file_path"),
    ("_tempoList", "tempo_list"),
    ("_beatList", "beat_list"),
    ("_trackList", "track_list"),
    ("_quantize", "quantize"),
    ("_isTriplet", "is_triplet"),
    ("_isNumerialKeyName", "is_numerical_key_name"),
    ("_firstNumerialKeyNameAtIndex", "first_numerical_key_name_at_index"),
    ("<ActualProjectFilePath>k__BackingField", "actual_project_file_path"),
];

const SONG_TEMPO_FIELDS: AliasTable = &[
    ("_pos", "pos"),
    ("_tempo", "tempo"),
    ("<Overlaped>k__BackingField", "overlapped"),
];

const SONG_BEAT_FIELDS: AliasTable = &[
    ("_barIndex", "bar_index"),
    ("_beatSize", "beat_size"),
    ("<Overlaped>k__BackingField", "overlapped"),
];

const BEAT_SIZE_FIELDS: AliasTable = &[("_x", "x"), ("_y", "y")];

const SERIAL_ITEM_LIST_FIELDS: AliasTable =
    &[("_buf", "buf"), ("SerialItemList`1+_buf", "buf_1")];

const GENERIC_LIST_FIELDS: AliasTable =
    &[("_items", "items"), ("_size", "size"), ("_version", "version")];

const NOTE_FIELDS: AliasTable = &[
    ("_startPos", "start_pos"),
    ("_widthPos", "width_pos"),
    ("_keyIndex", "key_index"),
    ("_lyric", "lyric"),
    ("_pronouncing", "pronouncing"),
    ("_headTag", "head_tag"),
    ("<Overlaped>k__BackingField", "overlapped"),
    ("<NotePhoneInfo>k__BackingField", "note_phone_info"),
    ("<VibratoPercent>k__BackingField", "vibrato_percent"),
    ("<Vibrato>k__BackingField", "vibrato"),
    ("<VibratoPercentInfo>k__BackingField", "vibrato_percent_info"),
];

const NOTE_HEAD_TAG_FIELDS: AliasTable = &[("value__", "value")];

const NOTE_PHONE_INFO_FIELDS: AliasTable = &[
    ("<HeadPhoneTimeInSec>k__BackingField", "head_phone_time_in_sec"),
    (
        "<MidPartOverTailPartRatio>k__BackingField",
        "mid_part_over_tail_part_ratio",
    ),
];

const VIBRATO_STYLE_FIELDS: AliasTable = &[
    ("_ampLine", "amp_line"),
    ("_freqLine", "freq_line"),
    ("<IsAntiPhase>k__BackingField", "is_anti_phase"),
];

const VIBRATO_PERCENT_INFO_FIELDS: AliasTable = &[
    ("_startPercent", "start_percent"),
    ("_endPercent", "end_percent"),
];

const LINE_PARAM_FIELDS: AliasTable = &[("LineParam", "line_param")];

const REVERB_PRESET_FIELDS: AliasTable = &[("value__", "value")];

const SINGING_TRACK_FIELDS: AliasTable = &[
    ("_noteList", "note_list"),
    ("_needRefreshBaseMetadataFlag", "need_refresh_base_metadata_flag"),
    ("_editedPitchLine", "edited_pitch_line"),
    ("_editedVolumeLine", "edited_volume_line"),
    ("_editedBreathLine", "edited_breath_line"),
    ("_editedGenderLine", "edited_gender_line"),
    ("_editedPowerLine", "edited_power_line"),
    ("_reverbPreset", "reverb_preset"),
    ("_volume", "volume"),
    ("_pan", "pan"),
    ("_name", "name"),
    ("_mute", "mute"),
    ("_solo", "solo"),
    ("<AISingerId>k__BackingField", "ai_singer_id"),
];

const INSTRUMENT_TRACK_FIELDS: AliasTable = &[
    ("_volume", "volume"),
    ("_pan", "pan"),
    ("_name", "name"),
    ("_mute", "mute"),
    ("_solo", "solo"),
    ("<SampleRate>k__BackingField", "sample_rate"),
    ("<SampleCount>k__BackingField", "sample_count"),
    ("<ChannelCount>k__BackingField", "channel_count"),
    ("<OffsetInPos>k__BackingField", "offset_in_pos"),
    ("<InstrumentFilePath>k__BackingField", "instrument_file_path"),
];

/// Every class `fullname2classes` knows about.
static CLASS_TABLE: &[(&str, AliasTable)] = &[
    (class::APP_MODEL, APP_MODEL_FIELDS),
    (class::SONG_TEMPO, SONG_TEMPO_FIELDS),
    (class::SONG_BEAT, SONG_BEAT_FIELDS),
    (class::BEAT_SIZE, BEAT_SIZE_FIELDS),
    (class::ITRACK, &[]),
    (class::SINGING_TRACK, SINGING_TRACK_FIELDS),
    (class::INSTRUMENT_TRACK, INSTRUMENT_TRACK_FIELDS),
    (class::NOTE, NOTE_FIELDS),
    (class::NOTE_HEAD_TAG, NOTE_HEAD_TAG_FIELDS),
    (class::NOTE_PHONE_INFO, NOTE_PHONE_INFO_FIELDS),
    (class::VIBRATO_STYLE, VIBRATO_STYLE_FIELDS),
    (class::VIBRATO_PERCENT_INFO, VIBRATO_PERCENT_INFO_FIELDS),
    (class::LINE_PARAM, LINE_PARAM_FIELDS),
    (class::REVERB_PRESET, REVERB_PRESET_FIELDS),
    (class::SERIAL_ITEM_LIST, SERIAL_ITEM_LIST_FIELDS),
    (class::GENERIC_LIST, GENERIC_LIST_FIELDS),
];

/// `fullname2classes[class_name]` lookup; `class_name` must already be stripped
/// of its `` `1[[...]] `` suffix (Python: `full_name.split("`1", 1)[0]`).
pub fn alias_table(class_name: &str) -> Option<AliasTable> {
    CLASS_TABLE
        .iter()
        .find(|(name, _)| *name == class_name)
        .map(|(_, table)| *table)
}

/// The `'static` class name of a known model class.
pub fn static_class_name(class_name: &str) -> Option<&'static str> {
    CLASS_TABLE
        .iter()
        .find(|(name, _)| *name == class_name)
        .map(|(name, _)| *name)
}

/// `alias2key.get(name, name)` — unknown member names are passed through, and
/// the model constructor then rejects them.
pub fn field_name_for_alias(class_name: &str, alias: &str) -> Option<&'static str> {
    alias_table(class_name)?
        .iter()
        .find(|(candidate, _)| *candidate == alias)
        .map(|(_, name)| *name)
}

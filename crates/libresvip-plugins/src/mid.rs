//! MIDI plugin — port of `libresvip.plugins.mid`.
//!
//! * [`midi_file`] ports the `construct` based SMF parser/builder from
//!   `libresvip.utils.binary.midi` (including Python's `VarIntBE` encoder).
//! * [`parser`] ports `midi_parser.py` (channels, lyrics, tempo map, time
//!   signatures and the relative pitch curve).
//! * [`generator`] ports `midi_generator.py` (master track, per-track note and
//!   lyric events, pitch bend/RPN data).
//! * [`pitch`] ports the `PitchSimulator` / `RelativePitchCurve` helpers the
//!   Python plugins rely on; the core crate does not provide them yet.

use std::path::Path;

use libresvip_core::error::{IoContext, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{
    enable_pitch_importation, enable_volume_importation, EnumVariant, OptionFieldDef, Options,
};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};
use libresvip_core::utils::text::supported_charset_names;

use crate::plugin_infos;

pub mod codec;
pub mod generator;
pub mod midi_file;
pub mod parser;
pub mod pitch;
pub mod text;

pub struct MidiPlugin;

pub static PLUGIN: MidiPlugin = MidiPlugin;

#[allow(dead_code)]
fn _info() -> &'static PluginInfo {
    &plugin_infos::INFO_MID
}

const MULTI_CHANNEL_VARIANTS: &[EnumVariant] = &[
    EnumVariant {
        name: "FIRST",
        value: "first",
        title: "Import first channel only",
    },
    EnumVariant {
        name: "SPLIT",
        value: "split",
        title: "Split into tracks",
    },
    EnumVariant {
        name: "CUSTOM",
        value: "custom",
        title: "Custom import range",
    },
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MultiChannelOption {
    First,
    Split,
    Custom,
}

/// `mid/options.py::InputOptions`.
pub struct InputOptions {
    pub import_pitch: bool,
    pub import_volume: bool,
    pub import_lyrics: bool,
    pub lyric_encoding: String,
    pub import_time_signatures: bool,
    pub multi_channel: MultiChannelOption,
    pub channels: String,
    pub default_bpm: f64,
}

impl InputOptions {
    fn from_options(options: &Options) -> Self {
        let multi_channel = match options.str("multi_channel", "first").as_str() {
            "split" => MultiChannelOption::Split,
            "custom" => MultiChannelOption::Custom,
            _ => MultiChannelOption::First,
        };
        Self {
            import_pitch: options.bool("import_pitch", true),
            import_volume: options.bool("import_volume", true),
            import_lyrics: options.bool("import_lyrics", true),
            lyric_encoding: options.str("lyric_encoding", "utf-8"),
            import_time_signatures: options.bool("import_time_signatures", true),
            multi_channel,
            channels: options.str("channels", "1"),
            default_bpm: options.float("default_bpm", libresvip_core::model::base::DEFAULT_BPM),
        }
    }
}

/// `mid/options.py::OutputOptions`.
pub struct OutputOptions {
    pub export_lyrics: bool,
    pub remove_symbols: bool,
    pub compatible_lyric: bool,
    pub lyric_encoding: String,
    /// Declared by the Python options but never used by `midi_generator`.
    #[allow(dead_code)]
    pub transpose: i64,
    pub ticks_per_beat: i64,
}

impl OutputOptions {
    fn from_options(options: &Options) -> Self {
        Self {
            export_lyrics: options.bool("export_lyrics", true),
            remove_symbols: options.bool("remove_symbols", true),
            compatible_lyric: options.bool("compatible_lyric", false),
            lyric_encoding: options.str("lyric_encoding", "utf-8"),
            transpose: options.int("transpose", 0),
            ticks_per_beat: options.int("ticks_per_beat", 480),
        }
    }
}

impl SvsPlugin for MidiPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_MID
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_MID_VERSION
    }

    fn input_options(&self) -> Vec<OptionFieldDef> {
        vec![
            // pydantic collects mixin fields in reverse MRO order: the volume
            // mixin precedes the pitch mixin even though `InputOptions` lists
            // `EnablePitchImportationMixin` first.
            enable_volume_importation(),
            enable_pitch_importation(),
            OptionFieldDef::bool("import_lyrics", "Import lyrics", true),
            OptionFieldDef::string("lyric_encoding", "Lyric text encoding", "utf-8")
                .with_description(
                    "Unless the lyrics are garbled, this option should not be changed.",
                )
                .with_choices(supported_charset_names()),
            OptionFieldDef::bool("import_time_signatures", "Import time signatures", true)
                .with_description("If this option is unset, the time signature is set to 4/4."),
            OptionFieldDef::enumerated(
                "multi_channel",
                "Multi-channel processing method",
                "MultiChannelOption",
                MULTI_CHANNEL_VARIANTS,
                "FIRST",
            ),
            OptionFieldDef::string("channels", "Channels to import", "1").with_description(
                "Specify which channels to import notes from. Enter channel numbers and/or \
                 channel ranges (separated by commas), e.g. 1,3,5-12. Range: 1-16. Only valid \
                 when the option \"Custom import range\" is selected in \"Multi-channel \
                 processing method\".",
            ),
            OptionFieldDef::float(
                "default_bpm",
                "Default BPM",
                libresvip_core::model::base::DEFAULT_BPM,
            )
            .with_description("Used when no BPM information is found in the MIDI file."),
        ]
    }

    fn output_options(&self) -> Vec<OptionFieldDef> {
        vec![
            OptionFieldDef::bool("export_lyrics", "Export lyrics", true),
            OptionFieldDef::bool("remove_symbols", "Remove symbols from lyrics", true)
                .with_description(
                    "Remove commas, periods, question marks and exclamation marks in Chinese \
                     and English to prevent lyric import failure in some vocal synthesizers.",
                ),
            OptionFieldDef::bool("compatible_lyric", "Lyric compatibility mode", false)
                .with_description(
                    "Convert all Chinese lyrics to pinyin to prevent garbled characters in \
                     MIDI files with Chinese lyrics from being imported by vocal synthesizers \
                     that do not support Chinese lyrics.",
                ),
            OptionFieldDef::string("lyric_encoding", "Lyric text encoding", "utf-8")
                .with_description(
                    "Unless the lyrics are garbled, this option should not be changed.",
                )
                .with_choices(supported_charset_names()),
            OptionFieldDef::int("transpose", "Transpose", 0),
            OptionFieldDef::int(
                "ticks_per_beat",
                "Ticks per beat",
                libresvip_core::model::base::TICKS_IN_BEAT,
            )
            .with_description(
                "Also known as parts per quarter, ticks per quarter note, the number of pulses \
                 per quarter note. This setting should not be changed unless you know what it is.",
            ),
        ]
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let bytes = std::fs::read(path).at_path(path)?;
        let midi_file = midi_file::parse(&bytes)?;
        parser::parse_project(&midi_file, &InputOptions::from_options(options))
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let output_options = OutputOptions::from_options(options);
        let generated = generator::generate_project(project, &output_options)?;
        let bytes = midi_file::build(
            generated.format,
            generated.ticks_per_beat,
            &generated.tracks,
        )?;
        std::fs::write(path, &bytes).at_path(path)?;
        Ok(())
    }
}

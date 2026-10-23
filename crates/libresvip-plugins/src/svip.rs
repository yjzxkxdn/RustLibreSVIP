//! `svip` plugin — port of `libresvip.plugins.svip` (X Studio / BinSvip).
//!
//! `.svip` is a .NET `BinaryFormatter` (MS-NRBF) stream: a Pascal `magic`
//! (`SVIP`), a Pascal version (`6.0.0` / `7.0.0`) and the remoting record stream
//! of a `SingingTool.Model.AppModel` object graph. The `msnrbf` submodule models
//! the record stream, `xstudio` the CLR model, and `parser` / `generator` the
//! conversion between it and the internal project model.

pub mod generator;
pub mod models;
pub mod msnrbf;
pub mod parser;
pub mod xstudio;

use std::path::Path;

use libresvip_core::error::{IoContext, LibreSvipError, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{
    enable_breath_importation, enable_gender_importation, enable_instrumental_track_importation,
    enable_pitch_importation, enable_strength_importation, enable_volume_importation, EnumVariant,
    OptionFieldDef, Options,
};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

pub struct SvipPlugin;

pub static PLUGIN: SvipPlugin = SvipPlugin;

/// `BinarySvipVersion` — the member names, the serialised values and the
/// titles of the `_value_` annotation model.
static VERSION_VARIANTS: &[EnumVariant] = &[
    EnumVariant {
        name: "AUTO",
        value: "auto",
        title: "Auto detect",
    },
    EnumVariant {
        name: "SVIP7_0_0",
        value: "7.0.0",
        title: "SVIP 7.0.0",
    },
    EnumVariant {
        name: "SVIP6_0_0",
        value: "6.0.0",
        title: "SVIP 6.0.0",
    },
    EnumVariant {
        name: "COMPAT",
        value: "0.0.0",
        title: "Max compatibility (read only)",
    },
];

const SINGER_DESCRIPTION: &str = "Please enter the singer's name in Chinese. If the specified singer does not exist, the default singer set in X Studio will be used. If you want to specify the conversion relationship between singer ID and name, or add a singer that has an ID but has not been publicly released, please modify singers.json in the plugin directory.";
const TEMPO_DESCRIPTION: &str = "The allowed range of tempo in X Studio is 20 ~ 300. When the tempo is out of range, the absolute timeline will be used to ensure the alignment of notes. Please set this value to an integer multiple or integer fraction of the tempo in the source project file as much as possible; as long as it is within a reasonable range, the value of this option will not affect the alignment effect.";
const VERSION_DESCRIPTION: &str = "This option only controls the header version information of the output project file.\nChoosing an older project file version will not affect the integrity of the data, but using a lower version of X Studio to open, edit and save a higher version project file may cause data loss.";

impl SvsPlugin for SvipPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_SVIP
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_SVIP_VERSION
    }

    /// `InputOptions(EnableBreathImportationMixin, …)` — pydantic collects the
    /// mixin fields in reverse MRO order.
    fn input_options(&self) -> Vec<OptionFieldDef> {
        vec![
            enable_volume_importation(),
            enable_strength_importation(),
            enable_pitch_importation(),
            enable_instrumental_track_importation(),
            enable_gender_importation(),
            enable_breath_importation(),
        ]
    }

    /// `OutputOptions` of the plugin.
    fn output_options(&self) -> Vec<OptionFieldDef> {
        vec![
            OptionFieldDef::string("singer", "Default singer", "陈水若")
                .with_description(SINGER_DESCRIPTION),
            OptionFieldDef::int("tempo", "Default tempo", 60).with_description(TEMPO_DESCRIPTION),
            OptionFieldDef::enumerated(
                "version",
                "Specify the version of the generated .svip file",
                "BinarySvipVersion",
                VERSION_VARIANTS,
                "AUTO",
            )
            .with_description(VERSION_DESCRIPTION),
        ]
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let bytes = std::fs::read(path).at_path(path)?;
        let stream = msnrbf::reader::read(&bytes)?;
        let model = msnrbf::reader::build_app_model(&stream)?;
        let input_options = parser::InputOptions {
            import_volume: options.bool("import_volume", true),
            import_strength: options.bool("import_strength", true),
            import_pitch: options.bool("import_pitch", true),
            import_instrumental_track: options.bool("import_instrumental_track", true),
            import_gender: options.bool("import_gender", true),
            import_breath: options.bool("import_breath", true),
        };
        parser::BinarySvipParser::new(input_options)
            .parse_project(stream.full_version(), &model)
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let singer = options.str("singer", "陈水若");
        let tempo = options.int("tempo", 60);
        let version_option = options.str("version", "auto");
        // `SvipConverter.dump` rewrites `project.version` before generating.
        let effective_version = match version_option.as_str() {
            "7.0.0" => "SVIP7.0.0".to_owned(),
            "auto" => {
                if project.version == "SVIP0.0.0" {
                    "SVIP6.0.0".to_owned()
                } else {
                    project.version.clone()
                }
            }
            "6.0.0" => "SVIP6.0.0".to_owned(),
            "0.0.0" => "SVIP0.0.0".to_owned(),
            _ => return Err(LibreSvipError::msg("Unexpected enum value")),
        };
        let file_version = generator::project_version(&effective_version).to_owned();
        let output_options = generator::OutputOptions {
            singer,
            tempo,
            version: version_option,
        };
        let model = generator::BinarySvipGenerator::new(output_options)
            .generate_project(project)?;
        let bytes = msnrbf::writer::write(&file_version, &model)?;
        std::fs::write(path, bytes).at_path(path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libresvip_core::model::base::{Note, SingingTrack, SongTempo, TimeSignature, Track};

    fn sample_project() -> Project {
        Project {
            version: "SVIP6.0.0".to_owned(),
            song_tempo_list: vec![SongTempo::new(0, 120.0)],
            time_signature_list: vec![TimeSignature::new(0, 4, 4)],
            track_list: vec![Track::Singing(SingingTrack {
                title: "Track".to_owned(),
                note_list: vec![
                    Note::new(0, 480, 60, "啦"),
                    Note::new(480, 240, 64, "la"),
                ],
                ..SingingTrack::default()
            })],
        }
    }

    fn input_options() -> parser::InputOptions {
        parser::InputOptions {
            import_volume: true,
            import_strength: true,
            import_pitch: true,
            import_instrumental_track: true,
            import_gender: true,
            import_breath: true,
        }
    }

    fn dump_project(project: &Project) -> Vec<u8> {
        let mut generator = generator::BinarySvipGenerator::new(generator::OutputOptions {
            singer: "陈水若".to_owned(),
            tempo: 60,
            version: "auto".to_owned(),
        });
        let model = generator.generate_project(project).expect("generate");
        msnrbf::writer::write("SVIP6.0.0", &model).expect("write")
    }

    #[test]
    fn round_trips_through_the_record_stream() {
        let project = sample_project();
        let bytes = dump_project(&project);

        let stream = msnrbf::reader::read(&bytes).expect("parse");
        assert_eq!(stream.magic, "SVIP");
        assert_eq!(stream.version, "6.0.0");
        let model = msnrbf::reader::build_app_model(&stream).expect("build");
        let reparsed = parser::BinarySvipParser::new(input_options())
            .parse_project(stream.full_version(), &model)
            .expect("parse project");

        assert_eq!(reparsed.version, "SVIP6.0.0");
        assert_eq!(reparsed.song_tempo_list, project.song_tempo_list);
        assert_eq!(reparsed.time_signature_list, project.time_signature_list);
        let track = reparsed.track_list[0].as_singing().expect("singing");
        assert_eq!(track.title, "Track");
        assert_eq!(track.ai_singer_name, "陈水若");
        assert_eq!(track.note_list.len(), 2);
        // Non-Chinese lyrics are replaced by the default Chinese lyric.
        assert_eq!(track.note_list[0].lyric, "啦");
        assert_eq!(track.note_list[1].lyric, "啦");
        assert_eq!(track.note_list[1].start_pos, 480);

        // Re-encoding the reparsed project is deterministic. (The first dump
        // and the second are not byte-identical: like Python, an empty pitch
        // curve becomes a `-100` baseline on the way back in.)
        let second_bytes = dump_project(&reparsed);
        assert_eq!(dump_project(&reparsed), second_bytes);
        let second_stream = msnrbf::reader::read(&second_bytes).expect("parse again");
        let second_model = msnrbf::reader::build_app_model(&second_stream).expect("build again");
        let second = parser::BinarySvipParser::new(input_options())
            .parse_project(second_stream.full_version(), &second_model)
            .expect("parse project again");
        assert_eq!(second, reparsed);
    }

    #[test]
    fn malformed_input_is_an_error() {
        assert!(msnrbf::reader::read(b"").is_err());
        assert!(msnrbf::reader::read(b"\x03bad").is_err());
        let mut bytes = dump_project(&sample_project());
        bytes.truncate(bytes.len() / 2);
        assert!(msnrbf::reader::read(&bytes).is_err());
    }
}

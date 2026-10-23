//! DiffSinger parameters file (`ds`) — port of `libresvip.plugins.ds`.
//!
//! The file is a JSON array of parameter objects; every sequence field is
//! written as a space separated string.

mod lyric;
mod model;
mod music_math;
mod note_list;
mod param_utils;
mod parser;
mod pinyin;
mod project_util;

use std::path::Path;

use libresvip_core::error::{IoContext, LibreSvipError, Result};
use libresvip_core::model::base::Project;
use libresvip_core::model::reset_time_axis::reset_time_axis;
use libresvip_core::options::{
    enable_pitch_importation, select_single_track, static_tempo, OptionFieldDef, OptionValue,
    Options,
};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

pub struct DiffSingerPlugin;

pub static PLUGIN: DiffSingerPlugin = DiffSingerPlugin;

/// `reset_time_axis(project)` — the DS generator always uses the default tempo.
const DEFAULT_RESET_TEMPO: f64 = 120.0;

impl SvsPlugin for DiffSingerPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_DS
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_DS_VERSION
    }

    /// `InputOptions(EnablePitchImportationMixin, StaticTempoMixin, BaseModel)`:
    /// pydantic collects the mixin fields in reverse declaration order.
    fn input_options(&self) -> Vec<OptionFieldDef> {
        vec![static_tempo(), enable_pitch_importation()]
    }

    /// `OutputOptions(SelectSingleTrackMixin, BaseModel)` plus the generator's
    /// own fields.
    fn output_options(&self) -> Vec<OptionFieldDef> {
        // `split_threshold: float = Field(default=5, ...)`: pydantic keeps the
        // *integer* default (`plugin detail` prints `5`), so the value is stored
        // as an `Int` while the field type stays `float`.
        let mut split_threshold =
            OptionFieldDef::float("split_threshold", "Split threshold (in seconds)", 5.0)
                .with_description(
                    "This option controls the segmentation strategy during conversion. When the \
                     value of this option is negative, no segmentation is performed; when the \
                     value of this option is 0, segmentation is performed at the threshold where \
                     the interval between all notes reaches the threshold; when the value of this \
                     option is positive, the minimum length of each segment can be controlled on \
                     the basis of segmentation. Setting a reasonable segmentation strategy can \
                     reduce the memory usage during synthesis while maximizing the utilization of \
                     performance and improving the synthesis effect.",
                );
        split_threshold.default = OptionValue::Int(5);
        vec![
            select_single_track(),
            OptionFieldDef::string("dict_name", "Dictionary Name", "opencpop-extension"),
            split_threshold,
            OptionFieldDef::int("min_interval", "Minimum interval (in milliseconds)", 400)
                .with_description(
                    "This option controls the minimum interval between notes. It is recommended \
                     to set it to a value greater than 300 milliseconds.",
                ),
            OptionFieldDef::int("seed", "Seed", -1).with_description(
                "A fixed random seed can get a stable and reproducible synthesis effect. This \
                 option takes effect when the non-negative value is set.",
            ),
            OptionFieldDef::bool("export_gender", "Export gender parameter", false),
            OptionFieldDef::int("indent", "Indentation", 2).with_description(
                "The number of spaces used for indentation. When the value is negative, no \
                 formatting is performed.",
            ),
        ]
    }

    fn load(&self, path: &Path, options: &Options) -> Result<Project> {
        let bytes = std::fs::read(path).at_path(path)?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|error| LibreSvipError::Parse(format!("invalid utf-8: {error}")))?;
        let ds_project = model::parse_project(text)?;
        parser::DiffSingerParser {
            tempo: options.float("tempo", 120.0),
            import_pitch: options.bool("import_pitch", true),
        }
        .parse_project(&ds_project)
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let dict_name = options.str("dict_name", "opencpop-extension");
        let split_threshold = options.float("split_threshold", 5.0);
        let min_interval = options.int("min_interval", 400);
        let track_index = options.int("track_index", -1);
        let seed = options.int("seed", -1);
        let export_gender = options.bool("export_gender", false);
        let indent = options.int("indent", 2);

        let mut series = Vec::new();
        if split_threshold >= 0.0 {
            let min_length = (split_threshold * 1000.0) as i64;
            let segments =
                project_util::split_into_segments(project, min_interval, min_length, track_index)?;
            // `DiffSingerGenerator.generate` loads the dictionary; a project with
            // no segment never reaches it (Python would not read the file either).
            if !segments.is_empty() {
                let phoneme_table = pinyin::load_phoneme_table(&dict_name)?;
                for (offset, segment, trailing_space) in segments {
                    let generator = note_list::DiffSingerGenerator {
                        phoneme_table: &phoneme_table,
                        export_gender,
                        trailing_space,
                    };
                    let mut item = generator.generate_for_track(&segment, track_index)?;
                    item.offset = offset;
                    if seed >= 0 {
                        item.seed = Some(seed);
                    }
                    series.push(item);
                }
            }
        } else {
            let reset_project = reset_time_axis(project, DEFAULT_RESET_TEMPO)?;
            let phoneme_table = pinyin::load_phoneme_table(&dict_name)?;
            let generator = note_list::DiffSingerGenerator {
                phoneme_table: &phoneme_table,
                export_gender,
                trailing_space: 0.5,
            };
            let mut item = generator.generate_for_track(&reset_project, track_index)?;
            if seed >= 0 {
                item.seed = Some(seed);
            }
            series.push(item);
        }

        let text = model::dump_project(&series, Some(indent));
        std::fs::write(path, text.as_bytes()).at_path(path)?;
        Ok(())
    }
}

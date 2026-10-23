//! SVG graphic generator — port of `libresvip.plugins.svg`.
//!
//! Write-only: renders the first (or a selected) singing track as an SVG
//! picture of note rectangles, lyric/pronunciation text, optional grid and the
//! interior segments of the pitch curve.

use std::path::Path;

use libresvip_core::error::{IoContext, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{EnumVariant, OptionFieldDef, Options};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

mod color;
mod generator;

pub struct SvgPlugin;

pub static PLUGIN: SvgPlugin = SvgPlugin;

pub static TEXT_POSITION_VARIANTS: &[EnumVariant] = &[
    EnumVariant {
        name: "UPPER",
        value: "upper",
        title: "Above the note",
    },
    EnumVariant {
        name: "INNER",
        value: "inner",
        title: "Inside the note",
    },
    EnumVariant {
        name: "LOWER",
        value: "lower",
        title: "Below the note",
    },
    EnumVariant {
        name: "NONE",
        value: "none",
        title: "Don't show",
    },
];

pub static TEXT_ALIGN_VARIANTS: &[EnumVariant] = &[
    EnumVariant {
        name: "START",
        value: "start",
        title: "Align to left",
    },
    EnumVariant {
        name: "MIDDLE",
        value: "middle",
        title: "Align to middle",
    },
    EnumVariant {
        name: "END",
        value: "end",
        title: "Align to right",
    },
];

const COLOR_DESCRIPTION: &str =
    "CSS color value, e.g. #FF0000, #66CCFF, rgba(255,0,0,0.5), etc.";

fn output_options() -> Vec<OptionFieldDef> {
    vec![
        libresvip_core::options::select_single_track(),
        OptionFieldDef::int("pixel_per_beat", "Pixels per beat", 48)
            .with_description("Unit: pixel."),
        OptionFieldDef::int("note_height", "Note height", 24).with_description("Unit: pixel."),
        OptionFieldDef::int("note_round", "Note radius", 4).with_description("Unit: pixel."),
        OptionFieldDef::opaque("note_fill_color", "Note fill color", "#CCFFCC", "Color")
            .with_description(COLOR_DESCRIPTION),
        OptionFieldDef::opaque("note_stroke_color", "Note stroke color", "#006600", "Color")
            .with_description(COLOR_DESCRIPTION),
        OptionFieldDef::int("note_stroke_width", "Note stroke width", 1)
            .with_description("Unit: pixel."),
        OptionFieldDef::opaque("pitch_stroke_color", "Pitch stroke color", "#99aa99", "Color")
            .with_description(COLOR_DESCRIPTION),
        OptionFieldDef::int("pitch_stroke_width", "Pitch stroke width", 2)
            .with_description("Unit: pixel."),
        OptionFieldDef::enumerated(
            "lyric_position",
            "Lyric position",
            "TextPositionOption",
            TEXT_POSITION_VARIANTS,
            "LOWER",
        ),
        OptionFieldDef::enumerated(
            "pronounciation_position",
            "Pronounciation position",
            "TextPositionOption",
            TEXT_POSITION_VARIANTS,
            "INNER",
        ),
        OptionFieldDef::enumerated(
            "text_align",
            "Text align",
            "TextAlignOption",
            TEXT_ALIGN_VARIANTS,
            "START",
        ),
        OptionFieldDef::opaque("inner_text_color", "Inner text color", "#000000", "Color")
            .with_description(COLOR_DESCRIPTION),
        OptionFieldDef::opaque(
            "side_text_color",
            "The color of the text on the top and bottom of the note",
            "#000000",
            "Color",
        )
        .with_description(COLOR_DESCRIPTION),
        OptionFieldDef::bool("show_grid", "Show grid lines", false),
        OptionFieldDef::opaque("grid_color", "Grid line color", "#CCCCCC", "Color")
            .with_description(COLOR_DESCRIPTION),
        OptionFieldDef::int("grid_stroke_width", "Grid line width", 1)
            .with_description("Unit: pixel."),
    ]
}

impl SvsPlugin for SvgPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_SVG
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_SVG_VERSION
    }

    fn readable(&self) -> bool {
        false
    }

    fn writable(&self) -> bool {
        true
    }

    fn output_options(&self) -> Vec<OptionFieldDef> {
        output_options()
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let drawing = generator::generate_project(project, options)?;
        libresvip_core::io::write(path, drawing.as_bytes()).at_path(path)?;
        Ok(())
    }
}

//! Project middlewares for the Rust port of LibreSVIP.
//!
//! Mirrors `libresvip.middlewares`: each middleware is a `Middleware` plugin
//! with its own option schema. Order matches the Python loader (alphabetical by
//! package name).

use libresvip_core::core::config;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{Project, Track};
use libresvip_core::model::reset_time_axis::zoom_project;
use libresvip_core::options::{
    EnumVariant, OptionFieldDef, OptionKind, OptionValue, Options,
};
use libresvip_core::plugin::{Middleware, PluginInfo};

const fn info(
    id: &'static str,
    name: &'static str,
    author: &'static str,
    website: &'static str,
    description: &'static str,
) -> PluginInfo {
    PluginInfo {
        id,
        name,
        author,
        website,
        file_format: "",
        description,
        suffixes: &[],
        icon_base64: None,
    }
}

pub static INFO_PITCH_SHIFT: PluginInfo = info(
    "pitch_shift",
    "Pitch Shift",
    "SoulMelody",
    "https://space.bilibili.com/175862486",
    "",
);
pub static INFO_PROJECT_ZOOM: PluginInfo = info(
    "project_zoom",
    "Project Zoom",
    "Sder Colin",
    "https://sdercolin.github.io/utaformatix3/",
    "Zoom in/out project",
);
pub static INFO_PRONOUNCIATION_CONVERSION: PluginInfo = info(
    "pronounciation_conversion",
    "Pronounciation Conversion",
    "SoulMelody",
    "https://space.bilibili.com/175862486",
    "For Chinese and Japanese Lyrics",
);
pub static INFO_REMOVE_LYRIC_SYMBOLS: PluginInfo = info(
    "remove_lyric_symbols",
    "Remove numbers and punctuation",
    "SoulMelody",
    "https://space.bilibili.com/175862486",
    "Remove Unicode decimal digits and punctuation from note lyrics while preserving structural lyric markers.",
);
pub static INFO_REMOVE_SHORT_SILENCES: PluginInfo = info(
    "remove_short_silences",
    "Remove Short Silences",
    "SoulMelody",
    "https://space.bilibili.com/175862486",
    "Fill short rests",
);
pub static INFO_REPLACE_LYRIC: PluginInfo = info(
    "replace_lyric",
    "Replace lyrics",
    "SoulMelody",
    "https://space.bilibili.com/175862486",
    "Replace lyrics using preset rules. You can customize the lyrics replacement rules in the program configuration page",
);

// ---------------------------------------------------------------------------
// pitch_shift
// ---------------------------------------------------------------------------

pub struct PitchShiftMiddleware;

impl Middleware for PitchShiftMiddleware {
    fn info(&self) -> &'static PluginInfo {
        &INFO_PITCH_SHIFT
    }

    fn process_options(&self) -> Vec<OptionFieldDef> {
        vec![OptionFieldDef::int("key", "Key transition of pitch", 0)
            .with_description("Pitch shift in semitones.")]
    }

    fn process(&self, project: Project, options: &Options) -> Result<Project> {
        let key = options.int("key", 0);
        if key == 0 {
            return Ok(project);
        }
        let shift = key * 100;
        let mut project = project;
        for track in &mut project.track_list {
            if let Track::Singing(singing) = track {
                for note in &mut singing.note_list {
                    note.key_number += key;
                }
                let points = singing
                    .edited_params
                    .pitch
                    .points
                    .iter()
                    .map(|point| {
                        if point.y == -100 {
                            *point
                        } else {
                            libresvip_core::model::point::Point::new(point.x, point.y + shift)
                        }
                    })
                    .collect();
                singing.edited_params.pitch =
                    libresvip_core::model::base::ParamCurve::new(points);
            }
        }
        Ok(project)
    }
}

// ---------------------------------------------------------------------------
// project_zoom
// ---------------------------------------------------------------------------

pub static PROJECT_ZOOM_OPTIONS: &[EnumVariant] = &[
    EnumVariant {
        name: "NONE",
        value: "1/1",
        title: "None",
    },
    EnumVariant {
        name: "DOUBLE",
        value: "2/1",
        title: "2",
    },
    EnumVariant {
        name: "HALF",
        value: "1/2",
        title: "1/2",
    },
    EnumVariant {
        name: "FIVE_THIRDS",
        value: "5/3",
        title: "5/3",
    },
    EnumVariant {
        name: "THREE_HALVES",
        value: "3/2",
        title: "3/2",
    },
    EnumVariant {
        name: "SIX_FIFTHS",
        value: "6/5",
        title: "6/5",
    },
    EnumVariant {
        name: "FOUR_FIFTHS",
        value: "4/5",
        title: "4/5",
    },
    EnumVariant {
        name: "THREE_FIFTHS",
        value: "3/5",
        title: "3/5",
    },
    EnumVariant {
        name: "THREE_QUARTER",
        value: "3/4",
        title: "3/4",
    },
];

pub struct ProjectZoomMiddleware;

impl Middleware for ProjectZoomMiddleware {
    fn info(&self) -> &'static PluginInfo {
        &INFO_PROJECT_ZOOM
    }

    fn process_options(&self) -> Vec<OptionFieldDef> {
        vec![OptionFieldDef::enumerated(
            "factor",
            "Zoom factor",
            "ProjectZoomOptions",
            PROJECT_ZOOM_OPTIONS,
            "NONE",
        )
        .with_description(
            "Change Bpm and notes in parallel so that the actual singing speed is kept. For example, with factor 2, 60 bpm becomes 120 bpm and all notes become twice the length",
        )]
    }

    fn process(&self, project: Project, options: &Options) -> Result<Project> {
        let factor_text = options.str("factor", "1/1");
        let factor = parse_fraction(&factor_text).ok_or_else(|| {
            LibreSvipError::msg(format!("invalid zoom factor: {factor_text}"))
        })?;
        if factor == 1.0 {
            return Ok(project);
        }
        zoom_project(&project, factor)
    }
}

fn parse_fraction(text: &str) -> Option<f64> {
    match text.split_once('/') {
        Some((numerator, denominator)) => {
            let numerator: f64 = numerator.trim().parse().ok()?;
            let denominator: f64 = denominator.trim().parse().ok()?;
            if denominator == 0.0 {
                return None;
            }
            Some(numerator / denominator)
        }
        None => text.trim().parse().ok(),
    }
}

// ---------------------------------------------------------------------------
// pronounciation_conversion
// ---------------------------------------------------------------------------

pub static PRONOUNCIATION_OPTIONS: &[EnumVariant] = &[
    EnumVariant {
        name: "NONE",
        value: "Do nothing",
        title: "None",
    },
    EnumVariant {
        name: "HANZI2PINYIN",
        value: "Hanzi->Pinyin",
        title: "Hanzi to Pinyin",
    },
    EnumVariant {
        name: "HANZI2JYUTPING",
        value: "Hanzi->Jyutping",
        title: "Hanzi to Jyutping",
    },
    EnumVariant {
        name: "HANGUL2ROMANIZATION",
        value: "Hangul->Romanization",
        title: "Hangul to Romanization",
    },
    EnumVariant {
        name: "KANA2ROMAJI",
        value: "Kana->Romaji",
        title: "Kana to Romaji",
    },
    EnumVariant {
        name: "TO_HIRAGANA",
        value: "->Hiragana",
        title: "To Hiragana",
    },
    EnumVariant {
        name: "TO_KATAKANA",
        value: "->Katakana",
        title: "To Katakana",
    },
];

pub struct PronounciationMiddleware;

impl Middleware for PronounciationMiddleware {
    fn info(&self) -> &'static PluginInfo {
        &INFO_PRONOUNCIATION_CONVERSION
    }

    fn process_options(&self) -> Vec<OptionFieldDef> {
        vec![OptionFieldDef::enumerated(
            "mode",
            "Pronounciation conversion mode",
            "PronounciationConversionOptions",
            PRONOUNCIATION_OPTIONS,
            "NONE",
        )
        .with_description("Convert lyrics to the specified pronounciation format")]
    }

    fn process(&self, project: Project, options: &Options) -> Result<Project> {
        let mode = options.str("mode", "Do nothing");
        if mode == "Do nothing" {
            return Ok(project);
        }
        let mut project = project;
        for track in &mut project.track_list {
            if let Track::Singing(singing) = track {
                for note in &mut singing.note_list {
                    if is_legato_char(&note.lyric) {
                        note.lyric = "-".to_owned();
                    } else {
                        note.lyric = convert_lyric(&mode, &note.lyric)?;
                    }
                }
            }
        }
        Ok(project)
    }
}

fn is_legato_char(lyric: &str) -> bool {
    libresvip_core::core::lyric_phoneme::japanese::is_legato_char(lyric)
}

fn convert_lyric(mode: &str, lyric: &str) -> Result<String> {
    use libresvip_core::core::lyric_phoneme::{chinese, japanese};
    match mode {
        "->Hiragana" => Ok(japanese::to_hiragana(lyric)),
        "->Katakana" => Ok(japanese::to_katakana(lyric)),
        "Kana->Romaji" => japanese::to_romaji(lyric).ok_or_else(|| {
            LibreSvipError::msg("Kana->Romaji conversion is not implemented yet")
        }),
        "Hanzi->Pinyin" => Ok(chinese::get_pinyin_series(&[lyric])?.join(" ")),
        "Hanzi->Jyutping" => Err(LibreSvipError::msg(
            "Hanzi->Jyutping conversion is not implemented yet",
        )),
        "Hangul->Romanization" => Err(LibreSvipError::msg(
            "Hangul->Romanization conversion is not implemented yet",
        )),
        other => Err(LibreSvipError::msg(format!(
            "unknown pronounciation conversion mode: {other}"
        ))),
    }
}

// ---------------------------------------------------------------------------
// remove_lyric_symbols
// ---------------------------------------------------------------------------

pub struct RemoveLyricSymbolsMiddleware;

const STRUCTURAL_LYRICS: [&str; 4] = ["-", "+", "+~", "+*"];

fn remove_numbers_and_punctuation(lyric: &str) -> String {
    if STRUCTURAL_LYRICS.contains(&lyric) {
        return lyric.to_owned();
    }
    lyric
        .chars()
        .filter(|character| {
            let category = unicode_general_category::get_general_category(*character);
            use unicode_general_category::GeneralCategory::*;
            category != DecimalNumber
                && !matches!(
                    category,
                    ConnectorPunctuation
                        | DashPunctuation
                        | OpenPunctuation
                        | ClosePunctuation
                        | InitialPunctuation
                        | FinalPunctuation
                        | OtherPunctuation
                )
        })
        .collect()
}

impl Middleware for RemoveLyricSymbolsMiddleware {
    fn info(&self) -> &'static PluginInfo {
        &INFO_REMOVE_LYRIC_SYMBOLS
    }

    fn process_options(&self) -> Vec<OptionFieldDef> {
        Vec::new()
    }

    fn process(&self, project: Project, _options: &Options) -> Result<Project> {
        let mut project = project;
        for track in &mut project.track_list {
            if let Track::Singing(singing) = track {
                for note in &mut singing.note_list {
                    note.lyric = remove_numbers_and_punctuation(&note.lyric);
                }
            }
        }
        Ok(project)
    }
}

// ---------------------------------------------------------------------------
// remove_short_silences
// ---------------------------------------------------------------------------

pub static NOTE_LENGTH_OPTIONS: &[EnumVariant] = &[
    EnumVariant {
        name: "ZERO",
        value: "0/1",
        title: "Zero length note",
    },
    EnumVariant {
        name: "EIGHTH",
        value: "1/8",
        title: "1/8 note",
    },
    EnumVariant {
        name: "SIXTEENTH",
        value: "1/16",
        title: "1/16 note",
    },
    EnumVariant {
        name: "THIRTY_SECOND",
        value: "1/32",
        title: "1/32 note",
    },
    EnumVariant {
        name: "SIXTY_FOURTH",
        value: "1/64",
        title: "1/64 note",
    },
    EnumVariant {
        name: "ONE_HUNDRED_AND_TWENTY_EIGHTH",
        value: "1/128",
        title: "1/128 note",
    },
];

pub struct RemoveShortSilencesMiddleware;

impl Middleware for RemoveShortSilencesMiddleware {
    fn info(&self) -> &'static PluginInfo {
        &INFO_REMOVE_SHORT_SILENCES
    }

    fn process_options(&self) -> Vec<OptionFieldDef> {
        vec![OptionFieldDef::enumerated(
            "fill_threshold",
            "Max length to be processed (exclusive)",
            "NoteLengthOption",
            NOTE_LENGTH_OPTIONS,
            "ZERO",
        )
        .with_description(
            "Extend note to fill the short silences between it and its next note",
        )]
    }

    fn process(&self, project: Project, options: &Options) -> Result<Project> {
        let threshold_text = options.str("fill_threshold", "0/1");
        let Some(signature) = project.time_signature_list.first() else {
            return Err(LibreSvipError::msg("time_signature_list is empty"));
        };
        let first_bar_tick = signature.bar_length().round_ties_even() as i64;
        let min_silence =
            parse_fraction(&threshold_text).unwrap_or(0.0);
        if min_silence == 0.0 {
            return Ok(project);
        }
        let silence_threshold = first_bar_tick as f64 * min_silence;
        let mut project = project;
        for track in &mut project.track_list {
            if let Track::Singing(singing) = track {
                for index in 0..singing.note_list.len().saturating_sub(1) {
                    let (before, after) = singing.note_list.split_at_mut(index + 1);
                    let prev_note = &mut before[index];
                    let note = &after[0];
                    let silence = note.start_pos - prev_note.end_pos();
                    if silence > 0 && (silence as f64) < silence_threshold {
                        prev_note.length = note.start_pos - prev_note.start_pos;
                    }
                }
            }
        }
        Ok(project)
    }
}

// ---------------------------------------------------------------------------
// replace_lyric
// ---------------------------------------------------------------------------

pub struct ReplaceLyricsMiddleware;

impl Middleware for ReplaceLyricsMiddleware {
    fn info(&self) -> &'static PluginInfo {
        &INFO_REPLACE_LYRIC
    }

    fn process_options(&self) -> Vec<OptionFieldDef> {
        vec![OptionFieldDef::string(
            "lyric_replacement_preset_name",
            "Name of lyric replacement preset",
            "default",
        )]
    }

    fn process(&self, project: Project, options: &Options) -> Result<Project> {
        let preset = options.str("lyric_replacement_preset_name", "default");
        let rules = config::read_settings(|settings| {
            settings.lyric_replace_rules.get(&preset).cloned()
        });
        let Some(rules) = rules else {
            return Ok(project);
        };
        if rules.is_empty() {
            return Ok(project);
        }
        // Faithful port of the Python bug: `replace_lyric(replacement, lyric)`
        // swaps the arguments and therefore raises TypeError for any non-empty
        // rule set.
        Err(LibreSvipError::msg(
            "TypeError: replace() takes at least 2 arguments (1 given)",
        ))
    }
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

pub static PITCH_SHIFT: PitchShiftMiddleware = PitchShiftMiddleware;
pub static PROJECT_ZOOM: ProjectZoomMiddleware = ProjectZoomMiddleware;
pub static PRONOUNCIATION_CONVERSION: PronounciationMiddleware = PronounciationMiddleware;
pub static REMOVE_LYRIC_SYMBOLS: RemoveLyricSymbolsMiddleware = RemoveLyricSymbolsMiddleware;
pub static REMOVE_SHORT_SILENCES: RemoveShortSilencesMiddleware = RemoveShortSilencesMiddleware;
pub static REPLACE_LYRIC: ReplaceLyricsMiddleware = ReplaceLyricsMiddleware;

/// All middlewares, in loader order.
pub fn all_middlewares() -> Vec<&'static dyn Middleware> {
    vec![
        &PITCH_SHIFT as &'static dyn Middleware,
        &PROJECT_ZOOM as &'static dyn Middleware,
        &PRONOUNCIATION_CONVERSION as &'static dyn Middleware,
        &REMOVE_LYRIC_SYMBOLS as &'static dyn Middleware,
        &REMOVE_SHORT_SILENCES as &'static dyn Middleware,
        &REPLACE_LYRIC as &'static dyn Middleware,
    ]
}

/// Middleware identifiers, used for translations.
pub fn middleware_ids() -> Vec<&'static str> {
    all_middlewares()
        .iter()
        .map(|middleware| middleware.info().id)
        .collect()
}

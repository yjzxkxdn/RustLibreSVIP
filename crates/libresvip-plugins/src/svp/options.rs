//! Port of `libresvip.plugins.svp.options`.

use libresvip_core::options::{
    enable_breath_importation, enable_gender_importation, enable_instrumental_track_importation,
    enable_pitch_importation, enable_strength_importation, enable_volume_importation, EnumVariant,
    OptionFieldDef, OptionKind, OptionValue, Options,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PitchOption {
    Full,
    Vibrato,
    Plain,
}

impl PitchOption {
    pub fn from_value(value: &str) -> Self {
        match value {
            "full" => PitchOption::Full,
            "vibrato" => PitchOption::Vibrato,
            _ => PitchOption::Plain,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BreathOption {
    Ignore,
    Keep,
    Convert,
}

impl BreathOption {
    pub fn from_value(value: &str) -> Self {
        match value {
            "ignore" => BreathOption::Ignore,
            "keep" => BreathOption::Keep,
            _ => BreathOption::Convert,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupOption {
    Split,
    Merge,
}

impl GroupOption {
    pub fn from_value(value: &str) -> Self {
        match value {
            "merge" => GroupOption::Merge,
            _ => GroupOption::Split,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VibratoOption {
    None,
    Always,
    Hybrid,
}

impl VibratoOption {
    pub fn from_value(value: &str) -> Self {
        match value {
            "always" => VibratoOption::Always,
            "hybrid" => VibratoOption::Hybrid,
            _ => VibratoOption::None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LanguageOption {
    Mandarin,
    Cantonese,
    Japanese,
    English,
    Spanish,
    Korean,
    German,
    French,
    Portuguese,
    Italian,
}

impl LanguageOption {
    pub fn from_value(value: &str) -> Self {
        match value {
            "cantonese" => LanguageOption::Cantonese,
            "japanese" => LanguageOption::Japanese,
            "english" => LanguageOption::English,
            "spanish" => LanguageOption::Spanish,
            "korean" => LanguageOption::Korean,
            "german" => LanguageOption::German,
            "french" => LanguageOption::French,
            "portuguese" => LanguageOption::Portuguese,
            "italian" => LanguageOption::Italian,
            _ => LanguageOption::Mandarin,
        }
    }

    pub fn value(self) -> &'static str {
        match self {
            LanguageOption::Mandarin => "mandarin",
            LanguageOption::Cantonese => "cantonese",
            LanguageOption::Japanese => "japanese",
            LanguageOption::English => "english",
            LanguageOption::Spanish => "spanish",
            LanguageOption::Korean => "korean",
            LanguageOption::German => "german",
            LanguageOption::French => "french",
            LanguageOption::Portuguese => "portuguese",
            LanguageOption::Italian => "italian",
        }
    }

    /// `synthv_language_presets[self.options.language_override.value]`.
    pub fn preset(self) -> (&'static str, &'static str) {
        match self {
            LanguageOption::English => ("english", "arpabet"),
            LanguageOption::Japanese => ("japanese", "romaji"),
            other => (other.value(), "xsampa"),
        }
    }
}

pub const VERSION_BELOW_1_9_0: i64 = 100;
pub const VERSION_BETWEEN_1_10_0_AND_1_11_2: i64 = 135;
pub const VERSION_ABOVE_2_0_0: i64 = 182;

#[derive(Clone, Debug)]
pub struct InputOptions {
    pub import_volume: bool,
    pub import_strength: bool,
    pub import_pitch: bool,
    pub import_instrumental_track: bool,
    pub import_gender: bool,
    pub import_breath: bool,
    pub instant: bool,
    pub pitch: PitchOption,
    pub breath: BreathOption,
    pub group: GroupOption,
}

impl Default for InputOptions {
    fn default() -> Self {
        Self {
            import_volume: true,
            import_strength: true,
            import_pitch: true,
            import_instrumental_track: true,
            import_gender: true,
            import_breath: true,
            instant: true,
            pitch: PitchOption::Plain,
            breath: BreathOption::Convert,
            group: GroupOption::Split,
        }
    }
}

impl InputOptions {
    pub fn from_options(options: &Options) -> Self {
        Self {
            import_volume: options.bool("import_volume", true),
            import_strength: options.bool("import_strength", true),
            import_pitch: options.bool("import_pitch", true),
            import_instrumental_track: options.bool("import_instrumental_track", true),
            import_gender: options.bool("import_gender", true),
            import_breath: options.bool("import_breath", true),
            instant: options.bool("instant", true),
            pitch: PitchOption::from_value(&options.str("pitch", "plain")),
            breath: BreathOption::from_value(&options.str("breath", "convert")),
            group: GroupOption::from_value(&options.str("group", "split")),
        }
    }
}

#[derive(Clone, Debug)]
pub struct OutputOptions {
    pub version_compatibility: i64,
    pub vibrato: VibratoOption,
    pub down_sample: i64,
    pub language_override: LanguageOption,
}

impl Default for OutputOptions {
    fn default() -> Self {
        Self {
            version_compatibility: VERSION_BELOW_1_9_0,
            vibrato: VibratoOption::None,
            down_sample: 20,
            language_override: LanguageOption::Mandarin,
        }
    }
}

impl OutputOptions {
    pub fn from_options(options: &Options) -> Self {
        Self {
            version_compatibility: options.int("version_compatibility", VERSION_BELOW_1_9_0),
            vibrato: VibratoOption::from_value(&options.str("vibrato", "none")),
            down_sample: options.int("down_sample", 20),
            language_override: LanguageOption::from_value(
                &options.str("language_override", "mandarin"),
            ),
        }
    }
}

pub const PITCH_VARIANTS: &[EnumVariant] = &[
    EnumVariant {
        name: "FULL",
        value: "full",
        title: "Full pitch curve",
    },
    EnumVariant {
        name: "VIBRATO",
        value: "vibrato",
        title: "Edited part only (vibrato mode)",
    },
    EnumVariant {
        name: "PLAIN",
        value: "plain",
        title: "Edited part only (plain mode)",
    },
];

pub const BREATH_VARIANTS: &[EnumVariant] = &[
    EnumVariant {
        name: "IGNORE",
        value: "ignore",
        title: "Ignore all breath notes",
    },
    EnumVariant {
        name: "KEEP",
        value: "keep",
        title: "Keep as normal notes",
    },
    EnumVariant {
        name: "CONVERT",
        value: "convert",
        title: "Convert to breath mark",
    },
];

pub const GROUP_VARIANTS: &[EnumVariant] = &[
    EnumVariant {
        name: "SPLIT",
        value: "split",
        title: "Split all to tracks",
    },
    EnumVariant {
        name: "MERGE",
        value: "merge",
        title: "Keep original position",
    },
];

pub const VIBRATO_VARIANTS: &[EnumVariant] = &[
    EnumVariant {
        name: "NONE",
        value: "none",
        title: "All removed",
    },
    EnumVariant {
        name: "ALWAYS",
        value: "always",
        title: "All kept",
    },
    EnumVariant {
        name: "HYBRID",
        value: "hybrid",
        title: "Hybrid mode",
    },
];

pub const LANGUAGE_VARIANTS: &[EnumVariant] = &[
    EnumVariant {
        name: "MANDARIN",
        value: "mandarin",
        title: "Mandarin",
    },
    EnumVariant {
        name: "CANTONESE",
        value: "cantonese",
        title: "Cantonese",
    },
    EnumVariant {
        name: "JAPANESE",
        value: "japanese",
        title: "Japanese",
    },
    EnumVariant {
        name: "ENGLISH",
        value: "english",
        title: "English",
    },
    EnumVariant {
        name: "SPANISH",
        value: "spanish",
        title: "Spanish",
    },
    EnumVariant {
        name: "KOREAN",
        value: "korean",
        title: "Korean",
    },
    EnumVariant {
        name: "GERMAN",
        value: "german",
        title: "German",
    },
    EnumVariant {
        name: "FRENCH",
        value: "french",
        title: "French",
    },
    EnumVariant {
        name: "PORTUGUESE",
        value: "portuguese",
        title: "Portuguese",
    },
    EnumVariant {
        name: "ITALIAN",
        value: "italian",
        title: "Italian",
    },
];

pub const VERSION_VARIANTS: &[EnumVariant] = &[
    EnumVariant {
        name: "BELOW_1_9_0",
        value: "100",
        title: "Compatible with Synthesizer V Studio 1.9.0 and below",
    },
    EnumVariant {
        name: "BETWEEN_1_10_0_AND_1_11_2",
        value: "135",
        title: "Compatible with Synthesizer V Studio 1.10.0 to 1.11.2",
    },
    EnumVariant {
        name: "ABOVE_2_0_0",
        value: "182",
        title: "Compatible with Synthesizer V Studio 2.0 and up",
    },
];

pub fn input_schema() -> Vec<OptionFieldDef> {
    vec![
        enable_volume_importation(),
        enable_strength_importation(),
        enable_pitch_importation(),
        enable_instrumental_track_importation(),
        enable_gender_importation(),
        enable_breath_importation(),
        OptionFieldDef::bool(
            "instant",
            "Always follow instant pitch mode",
            true,
        )
        .with_description(
            "When this option is turned off, the default pitch curve will always be imported regardless of the project setting. If you have tuned the pitch curve based on instant pitch mode, it is recommended to turn on this option.",
        ),
        OptionFieldDef::enumerated(
            "pitch",
            "Pitch input mode",
            "PitchOption",
            PITCH_VARIANTS,
            "PLAIN",
        )
        .with_description(
            "This option controls the range of pitch curve to be imported and the judgment condition. The definition of \"edited part\" is: the pitch deviation in the parameter panel, the pitch transition in the vibrato envelope and the pitch transition in the note properties have been edited.",
        ),
        OptionFieldDef::enumerated(
            "breath",
            "The way to handle breath notes",
            "BreathOption",
            BREATH_VARIANTS,
            "CONVERT",
        ),
        OptionFieldDef::enumerated(
            "group",
            "The way to handle note groups",
            "GroupOption",
            GROUP_VARIANTS,
            "SPLIT",
        )
        .with_description(
            "Notice: If there are too many note groups, please choose \"Keep original position\" to avoid excessive track count. But if there are notes that are adjacent (but not overlapped) between note groups or between note groups and main group, it is recommended to choose \"Split to tracks\" to ensure the paragraph division is not broken.",
        ),
    ]
}

pub fn output_schema() -> Vec<OptionFieldDef> {
    vec![
        OptionFieldDef {
            key: "version_compatibility",
            title: "Version compatibility",
            description: None,
            kind: OptionKind::Enum {
                type_name: "SVProjectVersionCompatibility",
                variants: VERSION_VARIANTS,
            },
            default: OptionValue::Int(VERSION_BELOW_1_9_0),
            default_repr: None,
            choices: None,
            show_enum_values: true,
        },
        OptionFieldDef::enumerated(
            "vibrato",
            "The way to handle vibrato notes",
            "VibratoOption",
            VIBRATO_VARIANTS,
            "NONE",
        ),
        OptionFieldDef::int(
            "down_sample",
            "Set the average sampling interval of parameter points to improve performance (0 means no limit)",
            20,
        )
        .with_description(
            "Reduce the sampling interval to improve the accuracy of parameter curves, but may cause rendering lag (e.g. Synthesizer V Studio Pro + AI voicebank). Please set this value according to your hardware configuration and actual experience.",
        ),
        OptionFieldDef::enumerated(
            "language_override",
            "Override default language for the voicebank",
            "LanguageOption",
            LANGUAGE_VARIANTS,
            "MANDARIN",
        ),
    ]
}

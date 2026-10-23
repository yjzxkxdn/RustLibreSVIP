//! Port of `libresvip.plugins.acep.options` and `libresvip.plugins.acep.enums`.

use libresvip_core::options::{
    ComplexFieldDef, EnumVariant, OptionFieldDef, OptionKind, OptionValue, Options,
};

use super::singers::DEFAULT_SINGER;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// `AcepLyricsLanguage`.
pub const LYRICS_LANGUAGE_VARIANTS: &[EnumVariant] = &[
    EnumVariant {
        name: "CHINESE",
        value: "CHN",
        title: "Chinese",
    },
    EnumVariant {
        name: "JAPANESE",
        value: "JPN",
        title: "Japanese",
    },
    EnumVariant {
        name: "ENGLISH",
        value: "ENG",
        title: "English",
    },
    EnumVariant {
        name: "SPANISH",
        value: "SPA",
        title: "Spanish",
    },
    EnumVariant {
        name: "KOREAN",
        value: "KOR",
        title: "Korean",
    },
    EnumVariant {
        name: "PORTUGUESE",
        value: "POR",
        title: "Portuguese",
    },
    EnumVariant {
        name: "FRENCH",
        value: "FRA",
        title: "French",
    },
    EnumVariant {
        name: "ITALIAN",
        value: "ITA",
        title: "Italian",
    },
];

/// `StrengthMappingOption`.
pub const STRENGTH_MAPPING_VARIANTS: &[EnumVariant] = &[
    EnumVariant {
        name: "BOTH",
        value: "both",
        title: "Both strength and tension",
    },
    EnumVariant {
        name: "ENERGY",
        value: "energy",
        title: "Only strength",
    },
    EnumVariant {
        name: "TENSION",
        value: "tension",
        title: "Only tension",
    },
];

/// `AcepSerialization`.
pub const SERIALIZATION_VARIANTS: &[EnumVariant] = &[
    EnumVariant {
        name: "JSON",
        value: "json",
        title: "JSON",
    },
    EnumVariant {
        name: "CBOR",
        value: "cbor",
        title: "CBOR",
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcepSerialization {
    Json,
    Cbor,
}

impl AcepSerialization {
    pub fn from_value(value: &str) -> Self {
        match value {
            "cbor" => Self::Cbor,
            _ => Self::Json,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrengthMappingOption {
    Both,
    Energy,
    Tension,
}

impl StrengthMappingOption {
    pub fn from_value(value: &str) -> Self {
        match value {
            "energy" => Self::Energy,
            "tension" => Self::Tension,
            _ => Self::Both,
        }
    }
}

// ---------------------------------------------------------------------------
// NormalizationArgument
// ---------------------------------------------------------------------------

pub const NORMALIZATION_NONE: &str = "none";
pub const NORMALIZATION_ZSCORE: &str = "zscore";
pub const NORMALIZATION_MINMAX: &str = "minmax";

/// `libresvip.plugins.acep.options.NormalizationArgument`.
#[derive(Clone, Debug, PartialEq)]
pub struct NormalizationArgument {
    pub normalize_method: String,
    pub lower_threshold: f64,
    pub upper_threshold: f64,
    pub scale: f64,
    pub bias: f64,
}

impl Default for NormalizationArgument {
    fn default() -> Self {
        Self {
            normalize_method: NORMALIZATION_NONE.to_owned(),
            lower_threshold: 0.0,
            upper_threshold: 10.0,
            scale: 0.0,
            bias: 0.0,
        }
    }
}

impl NormalizationArgument {
    /// `NormalizationArgument.default_repr()` — the unvalidated defaults keep
    /// their integer spelling, hence `none,0,10,0,0`.
    pub const DEFAULT_REPR: &'static str = "none,0,10,0,0";

    /// `NormalizationArgument.from_str`.
    pub fn from_str(value: &str) -> Self {
        const NAMES: [&str; 5] = [
            "normalize_method",
            "lower_threshold",
            "upper_threshold",
            "scale",
            "bias",
        ];
        let parts: Vec<&str> = value.split(',').collect();
        if parts.len() > NAMES.len() {
            return Self::default();
        }
        let mut argument = Self::default();
        for (name, part) in NAMES.iter().zip(parts.iter()) {
            let part = part.trim();
            match *name {
                "normalize_method" => argument.normalize_method = part.to_owned(),
                "lower_threshold" => match part.parse::<f64>() {
                    Ok(number) => argument.lower_threshold = number,
                    Err(_) => return Self::default(),
                },
                "upper_threshold" => match part.parse::<f64>() {
                    Ok(number) => argument.upper_threshold = number,
                    Err(_) => return Self::default(),
                },
                "scale" => match part.parse::<f64>() {
                    Ok(number) => argument.scale = number,
                    Err(_) => return Self::default(),
                },
                _ => match part.parse::<f64>() {
                    Ok(number) => argument.bias = number,
                    Err(_) => return Self::default(),
                },
            }
        }
        if !matches!(
            argument.normalize_method.as_str(),
            NORMALIZATION_NONE | NORMALIZATION_ZSCORE | NORMALIZATION_MINMAX
        ) {
            return Self::default();
        }
        let in_range = |value: f64, lower: f64, upper: f64| value >= lower && value <= upper;
        if !in_range(argument.lower_threshold, 0.0, 10.0)
            || !in_range(argument.upper_threshold, 0.0, 10.0)
            || !in_range(argument.scale, -1.0, 1.0)
            || !in_range(argument.bias, -1.0, 1.0)
        {
            return Self::default();
        }
        argument
    }

    /// `NormalizationArgument.to_str`.
    #[allow(dead_code)]
    pub fn to_str(&self) -> String {
        format!(
            "{},{},{},{},{}",
            self.normalize_method,
            self.lower_threshold,
            self.upper_threshold,
            self.scale,
            self.bias
        )
    }

    /// `NormalizationArgument.enabled`.
    pub fn enabled(&self) -> bool {
        self.normalize_method != NORMALIZATION_NONE
    }
}

const NORMALIZATION_FIELDS: &[ComplexFieldDef] = &[
    ComplexFieldDef {
        name: "None",
        type_name: "Literal",
    },
    ComplexFieldDef {
        name: "None",
        type_name: "float",
    },
    ComplexFieldDef {
        name: "None",
        type_name: "float",
    },
    ComplexFieldDef {
        name: "None",
        type_name: "float",
    },
    ComplexFieldDef {
        name: "None",
        type_name: "float",
    },
];

// ---------------------------------------------------------------------------
// Option schemas
// ---------------------------------------------------------------------------

const KEEP_ALL_PRONUNCIATION_DESCRIPTION: &str = "ACE Studio will add pronunciation to all notes. This is a redundant data for most Chinese characters, so by default only modified pronunciation will be kept. When this option is turned on, all pronunciation information will be kept unconditionally, but it may produce a larger file size. When the source file contains non-Chinese singing data, please turn on this option.";
const IMPORT_TENSION_DESCRIPTION: &str =
    "When turned on, the tension envelope will be mapped to the tension channel of the intermediate model.";
const IMPORT_ENERGY_DESCRIPTION: &str =
    "When turned on, the energy envelope will be mapped to the energy channel of the intermediate model.";
const ENERGY_COEFFICIENT_DESCRIPTION: &str = "Since the strength parameter of ACE Studio has a significant impact on the volume, this option is provided to control the mapping coefficient of the strength envelope. The strength envelope will be multiplied by the value of this option as a whole and then mapped to the volume channel of the intermediate model, and the remaining part will be mapped to the strength channel. This option accepts values in the range of 0~1.0.";
const CURVE_SAMPLE_INTERVAL_DESCRIPTION: &str = "The unit is tick (the length of a quarter note is 480 ticks). By default, the parameter points in the acep file are stored in a very dense manner. Increasing the sampling interval appropriately will not cause much loss of accuracy, but it can get a smaller file size. Setting to 0 or a negative value means keeping the original sampling interval.";
const BREATH_NORMALIZATION_DESCRIPTION: &str = "This option is an advanced option. After enabling this option, the breath parameters will be merged with the breath envelope after being transformed by the normalization. It is recommended to enable this option after fixing all parameters with a fixed brush. This option needs to set 5 values, separated by \",\" from each other:\n(1) Normalization method: none means to turn off this option, zscore means to perform Z-Score normalization on the parameter points, and minmax means to Min-Max normalize the parameter points to the [-1.0, 1.0] interval.\n(2) Lower threshold: a real number in the range of 0~10.0, the parameter points lower than this value will not participate in the normalization.\n(3) Upper threshold: a real number in the range of 0~10.0, the parameter points higher than this value will not participate in the normalization.\n(4) Scaling factor: a real number in the range of -1.0~1.0, the normalized parameter value will be multiplied by this value.\n(5) Bias: a real number in the range of -1.0~1.0, the normalized and scaled parameter value will be added to this value.";
const TENSION_NORMALIZATION_DESCRIPTION: &str = "This option is an advanced option. After enabling this option, the tension parameters will be merged with the tension envelope after being transformed by the normalization. It is recommended to enable this option after fixing all parameters with a fixed brush. This option needs to set 5 values, separated by \",\" from each other:\n(1) Normalization method: none means to turn off this option, zscore means to perform Z-Score normalization on the parameter points, and minmax means to Min-Max normalize the parameter points to the [-1.0, 1.0] interval.\n(2) Lower threshold: a real number in the range of 0~10.0, the parameter points lower than this value will not participate in the normalization.\n(3) Upper threshold: a real number in the range of 0~10.0, the parameter points higher than this value will not participate in the normalization.\n(4) Scaling factor: a real number in the range of -1.0~1.0, the normalized parameter value will be multiplied by this value.\n(5) Bias: a real number in the range of -1.0~1.0, the normalized and scaled parameter value will be added to this value.";
const ENERGY_NORMALIZATION_DESCRIPTION: &str = "This option is an advanced option. After enabling this option, the energy parameters will be merged with the energy envelope after being transformed by the normalization. It is recommended to enable this option after fixing all parameters with a fixed brush. This option needs to set 5 values, separated by \",\" from each other:\n(1) Normalization method: none means to turn off this option, zscore means to perform Z-Score normalization on the parameter points, and minmax means to Min-Max normalize the parameter points to the [-1.0, 1.0] interval.\n(2) Lower threshold: a real number in the range of 0~10.0, the parameter points lower than this value will not participate in the normalization.\n(3) Upper threshold: a real number in the range of 0~10.0, the parameter points higher than this value will not participate in the normalization.\n(4) Scaling factor: a real number in the range of -1.0~1.0, the normalized parameter value will be multiplied by this value.\n(5) Bias: a real number in the range of -1.0~1.0, the normalized and scaled parameter value will be added to this value.";
const DEFAULT_SINGER_DESCRIPTION: &str = "Please input the complete and correct singer name";
const BREATH_DESCRIPTION: &str = "This option is used to set the default breath length when the breath mark is converted to a breath parameter. The actual breath length may be less than the default value due to the small gap between notes; some notes may be shortened due to the insertion of breath marks. Setting to 0 or a negative value means ignoring all breath marks.";
const MAP_STRENGTH_DESCRIPTION: &str = "ACE Studio has both strength and tension parameters, both of which can affect the strength of the singing. This option is used to set the mapping target of the strength parameter.";
const SPLIT_THRESHOLD_DESCRIPTION: &str = "When the distance between notes exceeds the set value, they will be split into different segments (patterns) for subsequent editing. The threshold unit is the value of a quarter note, and the default is 1, which means that when the distance between notes exceeds 1 quarter notes (480 ticks), they will be split. If you don't want to split at all, please set this option to 0 or a negative value.";
const LYRIC_LANGUAGE_DESCRIPTION: &str =
    "ACE Studio supports eight languages of lyrics. This option is used to set the language of lyrics.";
const CONSONANT_LENGTH_DESCRIPTION: &str = "Set default consonant length for notes if not specified";

fn normalization_field(
    key: &'static str,
    title: &'static str,
    description: &'static str,
) -> OptionFieldDef {
    OptionFieldDef {
        key,
        title,
        description: Some(description),
        kind: OptionKind::Complex {
            type_name: "NormalizationArgument",
            fields: NORMALIZATION_FIELDS,
        },
        default: OptionValue::Str(NormalizationArgument::DEFAULT_REPR.to_owned()),
        default_repr: None,
        choices: None,
        show_enum_values: true,
    }
}

/// `InputOptions.model_fields`, in declaration order.
pub fn input_option_fields() -> Vec<OptionFieldDef> {
    vec![
        libresvip_core::options::enable_pitch_importation(),
        libresvip_core::options::enable_instrumental_track_importation(),
        libresvip_core::options::enable_gender_importation(),
        libresvip_core::options::enable_breath_importation(),
        OptionFieldDef::bool(
            "keep_all_pronunciation",
            "Keep all pronunciation information",
            false,
        )
        .with_description(KEEP_ALL_PRONUNCIATION_DESCRIPTION),
        OptionFieldDef::bool("import_tension", "Import tension envelope", true)
            .with_description(IMPORT_TENSION_DESCRIPTION),
        OptionFieldDef::bool("import_energy", "Import energy envelope", true)
            .with_description(IMPORT_ENERGY_DESCRIPTION),
        OptionFieldDef::float(
            "energy_coefficient",
            "Strength-volume mapping coefficient",
            0.5,
        )
        .with_description(ENERGY_COEFFICIENT_DESCRIPTION),
        OptionFieldDef::int("curve_sample_interval", "Interval of curve sampling", 5)
            .with_description(CURVE_SAMPLE_INTERVAL_DESCRIPTION),
        normalization_field(
            "breath_normalization",
            "Breath parameter normalization",
            BREATH_NORMALIZATION_DESCRIPTION,
        ),
        normalization_field(
            "tension_normalization",
            "Tension parameter normalization",
            TENSION_NORMALIZATION_DESCRIPTION,
        ),
        normalization_field(
            "energy_normalization",
            "Energy parameter normalization",
            ENERGY_NORMALIZATION_DESCRIPTION,
        ),
    ]
}

/// `OutputOptions.model_fields`, in declaration order.
pub fn output_option_fields() -> Vec<OptionFieldDef> {
    vec![
        OptionFieldDef::string("singer", "Default singer", DEFAULT_SINGER)
            .with_description(DEFAULT_SINGER_DESCRIPTION),
        OptionFieldDef::int("breath", "Default breath length (ms)", 0)
            .with_description(BREATH_DESCRIPTION),
        OptionFieldDef::enumerated(
            "map_strength_info",
            "Map strength and tension parameters to",
            "StrengthMappingOption",
            STRENGTH_MAPPING_VARIANTS,
            "BOTH",
        )
        .with_description(MAP_STRENGTH_DESCRIPTION),
        OptionFieldDef::int("split_threshold", "Threshold for splitting", 1)
            .with_description(SPLIT_THRESHOLD_DESCRIPTION),
        OptionFieldDef::enumerated(
            "lyric_language",
            "Lyrics language",
            "AcepLyricsLanguage",
            LYRICS_LANGUAGE_VARIANTS,
            "CHINESE",
        )
        .with_description(LYRIC_LANGUAGE_DESCRIPTION),
        OptionFieldDef::float(
            "default_consonant_length",
            "Default consonant length (secs)",
            0.0,
        )
        .with_description(CONSONANT_LENGTH_DESCRIPTION),
        OptionFieldDef::enumerated(
            "serialization",
            "Specify the serialization method of the generated .acep file",
            "AcepSerialization",
            SERIALIZATION_VARIANTS,
            "JSON",
        ),
    ]
}

// ---------------------------------------------------------------------------
// Parsed option structures
// ---------------------------------------------------------------------------

/// `InputOptions`.
#[derive(Clone, Debug, PartialEq)]
pub struct InputOptions {
    pub import_pitch: bool,
    pub import_instrumental_track: bool,
    pub import_gender: bool,
    pub import_breath: bool,
    pub keep_all_pronunciation: bool,
    pub import_tension: bool,
    pub import_energy: bool,
    pub energy_coefficient: f64,
    pub curve_sample_interval: i64,
    pub breath_normalization: NormalizationArgument,
    pub tension_normalization: NormalizationArgument,
    pub energy_normalization: NormalizationArgument,
}

impl Default for InputOptions {
    fn default() -> Self {
        Self {
            import_pitch: true,
            import_instrumental_track: true,
            import_gender: true,
            import_breath: true,
            keep_all_pronunciation: false,
            import_tension: true,
            import_energy: true,
            energy_coefficient: 0.5,
            curve_sample_interval: 5,
            breath_normalization: NormalizationArgument::default(),
            tension_normalization: NormalizationArgument::default(),
            energy_normalization: NormalizationArgument::default(),
        }
    }
}

impl InputOptions {
    /// `InputOptions.model_validate(options)`.
    pub fn from_options(options: &Options) -> Self {
        let default = Self::default();
        Self {
            import_pitch: options.bool("import_pitch", default.import_pitch),
            import_instrumental_track: options.bool(
                "import_instrumental_track",
                default.import_instrumental_track,
            ),
            import_gender: options.bool("import_gender", default.import_gender),
            import_breath: options.bool("import_breath", default.import_breath),
            keep_all_pronunciation: options
                .bool("keep_all_pronunciation", default.keep_all_pronunciation),
            import_tension: options.bool("import_tension", default.import_tension),
            import_energy: options.bool("import_energy", default.import_energy),
            energy_coefficient: options.float("energy_coefficient", default.energy_coefficient),
            curve_sample_interval: options
                .int("curve_sample_interval", default.curve_sample_interval),
            breath_normalization: read_normalization(options, "breath_normalization"),
            tension_normalization: read_normalization(options, "tension_normalization"),
            energy_normalization: read_normalization(options, "energy_normalization"),
        }
    }
}

fn read_normalization(options: &Options, key: &str) -> NormalizationArgument {
    match options.get(key) {
        Some(OptionValue::Str(value)) => NormalizationArgument::from_str(value),
        Some(OptionValue::Bool(_)) | Some(OptionValue::Int(_)) | Some(OptionValue::Float(_)) => {
            NormalizationArgument::from_str(&options.str(key, NormalizationArgument::DEFAULT_REPR))
        }
        None => NormalizationArgument::default(),
    }
}

/// `OutputOptions`.
#[derive(Clone, Debug, PartialEq)]
pub struct OutputOptions {
    pub singer: String,
    pub breath: i64,
    pub map_strength_info: StrengthMappingOption,
    pub split_threshold: i64,
    pub lyric_language: String,
    pub default_consonant_length: f64,
    pub serialization: AcepSerialization,
}

impl Default for OutputOptions {
    fn default() -> Self {
        Self {
            singer: DEFAULT_SINGER.to_owned(),
            breath: 0,
            map_strength_info: StrengthMappingOption::Both,
            split_threshold: 1,
            lyric_language: "CHN".to_owned(),
            default_consonant_length: 0.0,
            serialization: AcepSerialization::Json,
        }
    }
}

impl OutputOptions {
    /// `OutputOptions.model_validate(options)`.
    pub fn from_options(options: &Options) -> Self {
        let default = Self::default();
        Self {
            singer: options.str("singer", &default.singer),
            breath: options.int("breath", default.breath),
            map_strength_info: StrengthMappingOption::from_value(
                &options.str("map_strength_info", "both"),
            ),
            split_threshold: options.int("split_threshold", default.split_threshold),
            lyric_language: options.str("lyric_language", &default.lyric_language),
            default_consonant_length: options
                .float("default_consonant_length", default.default_consonant_length),
            serialization: AcepSerialization::from_value(&options.str("serialization", "json")),
        }
    }
}

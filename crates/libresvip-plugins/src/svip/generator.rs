//! Port of `libresvip.plugins.svip.binsvip_generator`.

use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::core::warning_types::show_warning;
use libresvip_core::error::Result;
use libresvip_core::model::base::{
    InstrumentalTrack, Note, ParamCurve, Params, Phones, Project, Track, VibratoParam,
};
use libresvip_core::utils::translation::gettext_lazy;

use super::models::{note_head_tag_value, reverb_value, singer_id};
use super::xstudio::{
    class, xs_error, DeclType, XsClass, XsField, XsValue, MAX_NOTE_DURATION, MIN_NOTE_DURATION,
    VALUE_LIST_VERSION_SONG_BEAT, VALUE_LIST_VERSION_SONG_ITRACK, VALUE_LIST_VERSION_SONG_NOTE,
    VALUE_LIST_VERSION_SONG_TEMPO,
};

/// `OutputOptions` of the plugin.
#[derive(Clone, Debug)]
pub struct OutputOptions {
    pub singer: String,
    pub tempo: i64,
    /// `BinarySvipVersion` value: `auto`, `7.0.0`, `6.0.0` or `0.0.0`.
    pub version: String,
}

pub struct BinarySvipGenerator {
    options: OutputOptions,
    is_absolute_time_mode: bool,
    synchronizer: TimeSynchronizer,
    first_bar_tick: i64,
}

impl BinarySvipGenerator {
    pub fn new(options: OutputOptions) -> Self {
        Self {
            options,
            is_absolute_time_mode: false,
            synchronizer: TimeSynchronizer::new(&[], 0, false, 60.0),
            first_bar_tick: 0,
        }
    }

    /// `BinarySvipGenerator.generate_project`.
    pub fn generate_project(&mut self, project: &Project) -> Result<XsClass> {
        let first_signature = project
            .time_signature_list
            .first()
            .cloned()
            .ok_or_else(|| xs_error("the project has no time signature"))?;
        self.first_bar_tick = round_ticks(first_signature.bar_length())?;
        self.is_absolute_time_mode = project
            .song_tempo_list
            .iter()
            .any(|tempo| tempo.bpm < 20.0 || tempo.bpm > 300.0);
        self.synchronizer = TimeSynchronizer::new(
            &project.song_tempo_list,
            self.first_bar_tick,
            self.is_absolute_time_mode,
            self.options.tempo as f64,
        );

        // beat
        let mut beats: Vec<XsValue> = Vec::new();
        if self.is_absolute_time_mode
            || project
                .time_signature_list
                .iter()
                .any(|beat| beat.numerator > 255 || beat.denominator > 32)
        {
            beats.push(XsValue::Class(Box::new(song_beat_node(0, 4, 4)?)));
        } else {
            for beat in &project.time_signature_list {
                beats.push(XsValue::Class(Box::new(song_beat_node(
                    beat.bar_index,
                    beat.numerator,
                    beat.denominator,
                )?)));
            }
        }
        let beat_list = buf_list_node(class::SONG_BEAT, beats, VALUE_LIST_VERSION_SONG_BEAT);

        // tempo
        let mut tempos: Vec<XsValue> = Vec::new();
        if self.is_absolute_time_mode {
            tempos.push(XsValue::Class(Box::new(song_tempo_node(
                0,
                self.options.tempo * 100,
            )?)));
        } else {
            for tempo in &project.song_tempo_list {
                tempos.push(XsValue::Class(Box::new(song_tempo_node(
                    tempo.position,
                    round_ticks(tempo.bpm * 100.0)?,
                )?)));
            }
        }
        let tempo_list = buf_list_node(class::SONG_TEMPO, tempos, VALUE_LIST_VERSION_SONG_TEMPO);

        // tracks
        let mut tracks: Vec<XsValue> = Vec::new();
        for track in &project.track_list {
            if let Some(node) = self.generate_track(track)? {
                tracks.push(XsValue::Class(Box::new(node)));
            }
        }
        let track_list = buf_node(class::ITRACK, tracks, VALUE_LIST_VERSION_SONG_ITRACK);

        let fields = vec![
            XsField::new(
                "project_file_path",
                "<ProjectFilePath>k__BackingField",
                DeclType::Str,
                XsValue::Str(String::new()),
            ),
            XsField::new(
                "tempo_list",
                "_tempoList",
                DeclType::Class {
                    name: class::SERIAL_ITEM_LIST,
                    generic: Some(class::SONG_TEMPO),
                },
                XsValue::Class(Box::new(tempo_list)),
            ),
            XsField::new(
                "beat_list",
                "_beatList",
                DeclType::Class {
                    name: class::SERIAL_ITEM_LIST,
                    generic: Some(class::SONG_BEAT),
                },
                XsValue::Class(Box::new(beat_list)),
            ),
            XsField::new(
                "track_list",
                "_trackList",
                DeclType::Class {
                    name: class::GENERIC_LIST,
                    generic: Some(class::ITRACK),
                },
                XsValue::Class(Box::new(track_list)),
            ),
            XsField::new("quantize", "_quantize", DeclType::Int, XsValue::Int(8)),
            XsField::new(
                "is_triplet",
                "_isTriplet",
                DeclType::Bool,
                XsValue::Bool(false),
            ),
            XsField::new(
                "is_numerical_key_name",
                "_isNumerialKeyName",
                DeclType::Bool,
                XsValue::Bool(true),
            ),
            XsField::new(
                "first_numerical_key_name_at_index",
                "_firstNumerialKeyNameAtIndex",
                DeclType::Int,
                XsValue::Int(0),
            ),
            XsField::new(
                "actual_project_file_path",
                "<ActualProjectFilePath>k__BackingField",
                DeclType::Str,
                XsValue::Null,
            ),
        ];
        Ok(XsClass::new(class::APP_MODEL, fields))
    }

    fn generate_track(&self, track: &Track) -> Result<Option<XsClass>> {
        match track {
            Track::Singing(track) => {
                let mut ai_singer_id = singer_id(&track.ai_singer_name);
                if ai_singer_id.is_empty() {
                    ai_singer_id = singer_id(&self.options.singer);
                }
                let mut notes: Vec<XsValue> = Vec::new();
                for note in &track.note_list {
                    if let Some(node) = self.generate_note(note)? {
                        notes.push(XsValue::Class(Box::new(node)));
                    }
                }
                let note_list = buf_list_node(class::NOTE, notes, VALUE_LIST_VERSION_SONG_NOTE);
                let params = self.generate_params(&track.edited_params)?;
                let mut fields = vec![
                    XsField::new(
                        "note_list",
                        "_noteList",
                        DeclType::Class {
                            name: class::SERIAL_ITEM_LIST,
                            generic: Some(class::NOTE),
                        },
                        XsValue::Class(Box::new(note_list)),
                    ),
                    XsField::new(
                        "need_refresh_base_metadata_flag",
                        "_needRefreshBaseMetadataFlag",
                        DeclType::Bool,
                        XsValue::Bool(false),
                    ),
                    XsField::new(
                        "edited_pitch_line",
                        "_editedPitchLine",
                        DeclType::Class {
                            name: class::LINE_PARAM,
                            generic: None,
                        },
                        line_value(params.pitch)?,
                    ),
                    XsField::new(
                        "edited_volume_line",
                        "_editedVolumeLine",
                        DeclType::Class {
                            name: class::LINE_PARAM,
                            generic: None,
                        },
                        line_value(params.volume)?,
                    ),
                    XsField::new(
                        "edited_breath_line",
                        "_editedBreathLine",
                        DeclType::Class {
                            name: class::LINE_PARAM,
                            generic: None,
                        },
                        line_value(params.breath)?,
                    ),
                    XsField::new(
                        "edited_gender_line",
                        "_editedGenderLine",
                        DeclType::Class {
                            name: class::LINE_PARAM,
                            generic: None,
                        },
                        line_value(params.gender)?,
                    ),
                ];
                if self.options.version == "7.0.0" {
                    fields.push(XsField::new(
                        "edited_power_line",
                        "_editedPowerLine",
                        DeclType::Class {
                            name: class::LINE_PARAM,
                            generic: None,
                        },
                        line_value(params.strength)?,
                    ));
                }
                fields.extend([
                    XsField::new(
                        "reverb_preset",
                        "_reverbPreset",
                        DeclType::InlineClass {
                            name: class::REVERB_PRESET,
                        },
                        XsValue::Class(Box::new(reverb_preset_node(i64::from(reverb_value(
                            &track.reverb_preset,
                        ))))),
                    ),
                    XsField::new(
                        "volume",
                        "_volume",
                        DeclType::F64,
                        XsValue::F64(track.volume),
                    ),
                    XsField::new("pan", "_pan", DeclType::F64, XsValue::F64(track.pan)),
                    XsField::new(
                        "name",
                        "_name",
                        DeclType::Str,
                        XsValue::Str(track.title.clone()),
                    ),
                    XsField::new("mute", "_mute", DeclType::Bool, XsValue::Bool(track.mute)),
                    XsField::new("solo", "_solo", DeclType::Bool, XsValue::Bool(track.solo)),
                    XsField::new(
                        "ai_singer_id",
                        "<AISingerId>k__BackingField",
                        DeclType::Str,
                        XsValue::Str(ai_singer_id),
                    ),
                ]);
                Ok(Some(XsClass::new(class::SINGING_TRACK, fields)))
            }
            Track::Instrumental(track) => Ok(Some(instrumental_track_node(track)?)),
        }
    }

    fn generate_note(&self, note: &Note) -> Result<Option<XsClass>> {
        if note.lyric.is_empty() && note.pronunciation.as_deref().unwrap_or("").is_empty() {
            return Ok(None);
        }
        let start_pos = round_ticks(
            self.synchronizer
                .get_actual_ticks_from_ticks(note.start_pos),
        )?;
        let key_index = note.key_number + 12;
        let head_tag = note_head_tag_value(note.head_tag.as_deref());
        let lyric = if starts_with_chinese(&note.lyric) {
            note.lyric.clone()
        } else {
            libresvip_core::core::constants::DEFAULT_CHINESE_LYRIC.to_owned()
        };
        let pronouncing = note.pronunciation.clone().unwrap_or_default();
        let end_pos = round_ticks(
            self.synchronizer
                .get_actual_ticks_from_ticks(note.end_pos()),
        )?;
        let width_pos = end_pos - start_pos;
        let note_duration = self
            .synchronizer
            .get_duration_secs_from_ticks(note.start_pos, note.end_pos());
        if note_duration < MIN_NOTE_DURATION {
            show_warning(format!(
                "{} {}",
                gettext_lazy("Note duration is too short:"),
                note.lyric
            ));
        } else if note_duration > MAX_NOTE_DURATION {
            show_warning(format!(
                "{} {}",
                gettext_lazy("Note duration is too long:"),
                note.lyric
            ));
        }
        let phone_info = match &note.edited_phones {
            Some(phones) => XsValue::Class(Box::new(phone_info_node(phones))),
            None => XsValue::Null,
        };
        let (percent_info, vibrato) = match &note.vibrato {
            Some(vibrato) => {
                let (percent, style) = self.generate_vibrato(vibrato)?;
                (
                    XsValue::Class(Box::new(percent)),
                    XsValue::Class(Box::new(style)),
                )
            }
            None => (XsValue::Null, XsValue::Null),
        };
        let fields = vec![
            XsField::new(
                "start_pos",
                "_startPos",
                DeclType::Int,
                XsValue::Int(start_pos),
            ),
            XsField::new(
                "width_pos",
                "_widthPos",
                DeclType::Int,
                XsValue::Int(width_pos),
            ),
            XsField::new(
                "key_index",
                "_keyIndex",
                DeclType::Int,
                XsValue::Int(key_index),
            ),
            XsField::new("lyric", "_lyric", DeclType::Str, XsValue::Str(lyric)),
            XsField::new(
                "pronouncing",
                "_pronouncing",
                DeclType::Str,
                XsValue::Str(pronouncing),
            ),
            XsField::new(
                "head_tag",
                "_headTag",
                DeclType::InlineClass {
                    name: class::NOTE_HEAD_TAG,
                },
                XsValue::Class(Box::new(note_head_tag_node(i64::from(head_tag)))),
            ),
            XsField::new(
                "overlapped",
                "<Overlaped>k__BackingField",
                DeclType::Bool,
                XsValue::Bool(false),
            ),
            XsField::new(
                "note_phone_info",
                "<NotePhoneInfo>k__BackingField",
                DeclType::Class {
                    name: class::NOTE_PHONE_INFO,
                    generic: None,
                },
                phone_info,
            ),
            XsField::new(
                "vibrato_percent",
                "<VibratoPercent>k__BackingField",
                DeclType::Int,
                XsValue::Int(0),
            ),
            XsField::new(
                "vibrato",
                "<Vibrato>k__BackingField",
                DeclType::Class {
                    name: class::VIBRATO_STYLE,
                    generic: None,
                },
                vibrato,
            ),
            XsField::new(
                "vibrato_percent_info",
                "<VibratoPercentInfo>k__BackingField",
                DeclType::Class {
                    name: class::VIBRATO_PERCENT_INFO,
                    generic: None,
                },
                percent_info,
            ),
        ];
        Ok(Some(XsClass::new(class::NOTE, fields)))
    }

    fn generate_vibrato(&self, vibrato: &VibratoParam) -> Result<(XsClass, XsClass)> {
        let percent = XsClass::new(
            class::VIBRATO_PERCENT_INFO,
            vec![
                XsField::new(
                    "start_percent",
                    "_startPercent",
                    DeclType::F32,
                    XsValue::F32(vibrato.start_percent as f32),
                ),
                XsField::new(
                    "end_percent",
                    "_endPercent",
                    DeclType::F32,
                    XsValue::F32(vibrato.end_percent as f32),
                ),
            ],
        );
        // Python sorts the dataclass fields by `metadata["order"]` with a stable
        // sort; `is_anti_phase` has no `order`, so it keeps the `0` it shares
        // with `_ampLine` and ends up *before* `_freqLine` (order 1).
        let style = XsClass::new(
            class::VIBRATO_STYLE,
            vec![
                XsField::new(
                    "amp_line",
                    "_ampLine",
                    DeclType::Class {
                        name: class::LINE_PARAM,
                        generic: None,
                    },
                    XsValue::Class(Box::new(self.generate_param_curve(
                        &vibrato.amplitude,
                        identity,
                        -1,
                        100001,
                        0,
                        false,
                    )?)),
                ),
                XsField::new(
                    "is_anti_phase",
                    "<IsAntiPhase>k__BackingField",
                    DeclType::Bool,
                    XsValue::Bool(vibrato.is_anti_phase),
                ),
                XsField::new(
                    "freq_line",
                    "_freqLine",
                    DeclType::Class {
                        name: class::LINE_PARAM,
                        generic: None,
                    },
                    XsValue::Class(Box::new(self.generate_param_curve(
                        &vibrato.frequency,
                        identity,
                        -1,
                        100001,
                        0,
                        false,
                    )?)),
                ),
            ],
        );
        Ok((percent, style))
    }

    fn generate_params(&self, params: &Params) -> Result<GeneratedParams> {
        Ok(GeneratedParams {
            pitch: self.generate_param_curve(
                &params.pitch,
                pitch_to_svip,
                -192000,
                1073741823,
                0,
                true,
            )?,
            volume: self.generate_param_curve(
                &params.volume,
                identity,
                -192000,
                1073741823,
                0,
                true,
            )?,
            breath: self.generate_param_curve(
                &params.breath,
                identity,
                -192000,
                1073741823,
                0,
                true,
            )?,
            gender: self.generate_param_curve(
                &params.gender,
                identity,
                -192000,
                1073741823,
                0,
                true,
            )?,
            strength: self.generate_param_curve(
                &params.strength,
                identity,
                -192000,
                1073741823,
                0,
                true,
            )?,
        })
    }

    fn generate_param_curve(
        &self,
        param_curve: &ParamCurve,
        op: fn(i64) -> i64,
        left: i64,
        right: i64,
        termination: i64,
        is_ticks: bool,
    ) -> Result<XsClass> {
        let mut nodes: Vec<(i64, i64)> = Vec::new();
        for point in &param_curve.points {
            if left <= point.x && point.x <= right {
                let position = if self.is_absolute_time_mode
                    && is_ticks
                    && point.x != left
                    && point.x != right
                {
                    round_ticks(
                        self.synchronizer
                            .get_actual_ticks_from_ticks(point.x - self.first_bar_tick),
                    )? + self.first_bar_tick
                } else {
                    point.x
                };
                nodes.push((position, op(point.y)));
            }
        }
        if nodes.is_empty() || nodes[0].0 > left {
            nodes.insert(0, (left, termination));
        }
        if nodes.last().map_or(true, |(position, _)| *position < right) {
            nodes.push((right, termination));
        }
        line_param_node(&nodes)
    }
}

struct GeneratedParams {
    pitch: XsClass,
    volume: XsClass,
    breath: XsClass,
    gender: XsClass,
    strength: XsClass,
}

fn identity(value: i64) -> i64 {
    value
}

/// `op=lambda x: x + 1150 if x > -100 else -100`.
fn pitch_to_svip(value: i64) -> i64 {
    if value > -100 {
        value + 1150
    } else {
        -100
    }
}

/// `round(...)` - Python rounds half to even and rejects non-finite values.
fn round_ticks(value: f64) -> Result<i64> {
    if !value.is_finite() {
        return Err(xs_error("cannot round a non-finite tick position"));
    }
    Ok(value.round_ties_even() as i64)
}

/// `project.version if prefix_match(r"^SVIP\d\.\d\.\d$", ...) else "SVIP6.0.0"`.
pub fn project_version(version: &str) -> &str {
    let characters: Vec<char> = version.chars().collect();
    let matches = characters.len() == 9
        && characters[..4].iter().eq(&['S', 'V', 'I', 'P'])
        && characters[4].is_ascii_digit()
        && characters[5] == '.'
        && characters[6].is_ascii_digit()
        && characters[7] == '.'
        && characters[8].is_ascii_digit();
    if matches {
        version
    } else {
        "SVIP6.0.0"
    }
}

/// `prefix_match(CHINESE_RE, lyric)` - the first character is a CJK ideograph.
fn starts_with_chinese(text: &str) -> bool {
    let Some(character) = text.chars().next() else {
        return false;
    };
    let code = u32::from(character);
    matches!(code, 0x3007)
        || (0x4E00..=0x9FFF).contains(&code)
        || (0x3400..=0x4DBF).contains(&code)
        || (0xF900..=0xFAFF).contains(&code)
        || (0x20000..=0x2A6DF).contains(&code)
        || (0x2A700..=0x2B73F).contains(&code)
        || (0x2B740..=0x2B81F).contains(&code)
        || (0x2F800..=0x2FA1F).contains(&code)
}

// ---------------------------------------------------------------------------
// Model node helpers
// ---------------------------------------------------------------------------

fn song_tempo_node(position: i64, tempo: i64) -> Result<XsClass> {
    Ok(XsClass::new(
        class::SONG_TEMPO,
        vec![
            XsField::new("pos", "_pos", DeclType::Int, XsValue::Int(position)),
            XsField::new("tempo", "_tempo", DeclType::Int, XsValue::Int(tempo)),
            XsField::new(
                "overlapped",
                "<Overlaped>k__BackingField",
                DeclType::Bool,
                XsValue::Bool(false),
            ),
        ],
    ))
}

fn song_beat_node(bar_index: i64, numerator: i64, denominator: i64) -> Result<XsClass> {
    let size = XsClass::new(
        class::BEAT_SIZE,
        vec![
            XsField::new("x", "_x", DeclType::Int, XsValue::Int(numerator)),
            XsField::new("y", "_y", DeclType::Int, XsValue::Int(denominator)),
        ],
    );
    Ok(XsClass::new(
        class::SONG_BEAT,
        vec![
            XsField::new(
                "bar_index",
                "_barIndex",
                DeclType::Int,
                XsValue::Int(bar_index),
            ),
            XsField::new(
                "beat_size",
                "_beatSize",
                DeclType::Class {
                    name: class::BEAT_SIZE,
                    generic: None,
                },
                XsValue::Class(Box::new(size)),
            ),
            XsField::new(
                "overlapped",
                "<Overlaped>k__BackingField",
                DeclType::Bool,
                XsValue::Bool(false),
            ),
        ],
    ))
}

fn buf_node(item_class: &'static str, items: Vec<XsValue>, version: i64) -> XsClass {
    let size = items.len() as i64;
    XsClass::new(
        class::GENERIC_LIST,
        vec![
            XsField::new(
                "items",
                "_items",
                DeclType::List {
                    declared: item_class,
                },
                XsValue::List(items),
            ),
            XsField::new("size", "_size", DeclType::Int, XsValue::Int(size)),
            XsField::new("version", "_version", DeclType::Int, XsValue::Int(version)),
        ],
    )
}

fn buf_list_node(item_class: &'static str, items: Vec<XsValue>, version: i64) -> XsClass {
    let buf = buf_node(item_class, items, version);
    XsClass::new(
        class::SERIAL_ITEM_LIST,
        vec![
            XsField::new(
                "buf",
                "_buf",
                DeclType::Class {
                    name: class::GENERIC_LIST,
                    generic: Some(item_class),
                },
                XsValue::Class(Box::new(buf.clone())),
            ),
            XsField::new(
                "buf_1",
                "SerialItemList`1+_buf",
                DeclType::SharedClass {
                    name: class::GENERIC_LIST,
                    generic: Some(item_class),
                },
                XsValue::Class(Box::new(buf)),
            ),
        ],
    )
}

fn note_head_tag_node(value: i64) -> XsClass {
    XsClass::new(
        class::NOTE_HEAD_TAG,
        vec![XsField::new(
            "value",
            "value__",
            DeclType::Int,
            XsValue::Int(value),
        )],
    )
}

fn reverb_preset_node(value: i64) -> XsClass {
    XsClass::new(
        class::REVERB_PRESET,
        vec![XsField::new(
            "value",
            "value__",
            DeclType::Int,
            XsValue::Int(value),
        )],
    )
}

fn phone_info_node(phones: &Phones) -> XsClass {
    XsClass::new(
        class::NOTE_PHONE_INFO,
        vec![
            XsField::new(
                "head_phone_time_in_sec",
                "<HeadPhoneTimeInSec>k__BackingField",
                DeclType::F32,
                XsValue::F32(phones.head_length_in_secs as f32),
            ),
            XsField::new(
                "mid_part_over_tail_part_ratio",
                "<MidPartOverTailPartRatio>k__BackingField",
                DeclType::F32,
                XsValue::F32(phones.mid_ratio_over_tail as f32),
            ),
        ],
    )
}

fn instrumental_track_node(track: &InstrumentalTrack) -> Result<XsClass> {
    Ok(XsClass::new(
        class::INSTRUMENT_TRACK,
        vec![
            XsField::new(
                "volume",
                "_volume",
                DeclType::F64,
                XsValue::F64(track.volume),
            ),
            XsField::new("pan", "_pan", DeclType::F64, XsValue::F64(track.pan)),
            XsField::new(
                "name",
                "_name",
                DeclType::Str,
                XsValue::Str(track.title.clone()),
            ),
            XsField::new("mute", "_mute", DeclType::Bool, XsValue::Bool(track.mute)),
            XsField::new("solo", "_solo", DeclType::Bool, XsValue::Bool(track.solo)),
            XsField::new(
                "sample_rate",
                "<SampleRate>k__BackingField",
                DeclType::F64,
                XsValue::F64(48000.0),
            ),
            XsField::new(
                "sample_count",
                "<SampleCount>k__BackingField",
                DeclType::Int,
                XsValue::Int(0),
            ),
            XsField::new(
                "channel_count",
                "<ChannelCount>k__BackingField",
                DeclType::Int,
                XsValue::Int(0),
            ),
            XsField::new(
                "offset_in_pos",
                "<OffsetInPos>k__BackingField",
                DeclType::Int,
                XsValue::Int(track.offset),
            ),
            XsField::new(
                "instrument_file_path",
                "<InstrumentFilePath>k__BackingField",
                DeclType::Str,
                XsValue::Str(track.audio_file_path.clone()),
            ),
        ],
    ))
}

/// A `SingingTool.Model.Line.LineParam`, or `Null` when it carries no data (the
/// Python writer emits `ObjectNull` in that case).
fn line_value(line: XsClass) -> Result<XsValue> {
    let empty = match line.field("line_param")? {
        XsValue::Bytes(bytes) => bytes.is_empty(),
        _ => false,
    };
    if empty {
        Ok(XsValue::Null)
    } else {
        Ok(XsValue::Class(Box::new(line)))
    }
}

/// `XSLineParam.convert_to_param` - `count`, the `(pos, value)` pairs, padded to
/// at least 64 bytes and to the next power of two.
fn line_param_node(nodes: &[(i64, i64)]) -> Result<XsClass> {
    let count = i32::try_from(nodes.len()).map_err(|_| xs_error("too many line param nodes"))?;
    let mut data = Vec::with_capacity(4 + nodes.len() * 8);
    data.extend_from_slice(&count.to_le_bytes());
    for (position, value) in nodes {
        let position =
            i32::try_from(*position).map_err(|_| xs_error("line param position is out of range"))?;
        let value =
            i32::try_from(*value).map_err(|_| xs_error("line param value is out of range"))?;
        data.extend_from_slice(&position.to_le_bytes());
        data.extend_from_slice(&value.to_le_bytes());
    }
    let expected_len = std::cmp::max(64usize, data.len().next_power_of_two());
    data.resize(expected_len, 0);
    Ok(XsClass::new(
        class::LINE_PARAM,
        vec![XsField::new(
            "line_param",
            "LineParam",
            DeclType::Bytes,
            XsValue::Bytes(data),
        )],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_the_project_version_like_python() {
        assert_eq!(project_version("SVIP6.0.0"), "SVIP6.0.0");
        assert_eq!(project_version("SVIP7.0.0"), "SVIP7.0.0");
        assert_eq!(project_version("SVIP0.0.0"), "SVIP0.0.0");
        // `^SVIP\d\.\d\.\d$` does not match these.
        assert_eq!(project_version("1.0.0"), "SVIP6.0.0");
        assert_eq!(project_version(""), "SVIP6.0.0");
        assert_eq!(project_version("SVIP10.0.0"), "SVIP6.0.0");
        assert_eq!(project_version("SVIP6.0.0 "), "SVIP6.0.0");
        assert_eq!(project_version("svip6.0.0"), "SVIP6.0.0");
    }

    #[test]
    fn detects_chinese_lyrics() {
        assert!(starts_with_chinese("啦"));
        assert!(starts_with_chinese("〇"));
        assert!(starts_with_chinese("㐀"));
        assert!(starts_with_chinese("𠀀"));
        assert!(!starts_with_chinese("la"));
        assert!(!starts_with_chinese(""));
        assert!(!starts_with_chinese("，"));
    }

    #[test]
    fn line_param_blob_is_padded_like_python() {
        let line = line_param_node(&[(-192000, 0), (1073741823, -100)]).unwrap();
        let bytes = match line.field("line_param").unwrap() {
            XsValue::Bytes(bytes) => bytes.clone(),
            other => panic!("unexpected value {other:?}"),
        };
        assert_eq!(bytes.len(), 64);
        assert_eq!(&bytes[..4], &2i32.to_le_bytes());
        assert_eq!(&bytes[4..8], &(-192000i32).to_le_bytes());
        assert_eq!(&bytes[12..16], &1073741823i32.to_le_bytes());
        assert!(bytes[20..].iter().all(|byte| *byte == 0));
    }
}

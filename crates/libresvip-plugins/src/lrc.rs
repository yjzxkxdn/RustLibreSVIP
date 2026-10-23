//! LRC plugin — port of `libresvip.plugins.lrc`.
//!
//! The plugin is write-only (`WriteOnlyConverterMixin`): it renders the first
//! singing track to an `.lrc` lyric file. The Python implementation builds an
//! `LrcFile` model and renders it through a Jinja template; here the template is
//! inlined and produces the same bytes.
//!
//! Notes on faithfulness:
//!
//! * `encoding` is declared by the Python options but never used by `dump` —
//!   `lrc_converter.dump` always writes UTF-8. The same is done here.
//! * `timeline` is likewise unused by `lrc_generator`.
//! * `offset` is applied with `timedelta(microseconds=-offset)` even though the
//!   description says milliseconds; the quirk is reproduced.
//! * The template emits `[key:value]\n` per info tag and then, per lyric line,
//!   its time tags followed by the lyric (which already ends with `\n`).

use std::path::Path;

use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{IoContext, LibreSvipError, Result};
use libresvip_core::model::base::Project;
use libresvip_core::options::{EnumVariant, OptionFieldDef, Options};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};
use libresvip_core::utils::text::supported_charset_names;

use crate::mid::text::{contains_latin, contains_symbol, remove_symbols};
use crate::plugin_infos;

pub struct LrcPlugin;

pub static PLUGIN: LrcPlugin = LrcPlugin;

#[allow(dead_code)]
fn _info() -> &'static PluginInfo {
    &plugin_infos::INFO_LRC
}

const OFFSET_POLICY_VARIANTS: &[EnumVariant] = &[
    EnumVariant {
        name: "TIMELINE",
        value: "timeline",
        title: "Act on timeline",
    },
    EnumVariant {
        name: "META",
        value: "meta",
        title: "Act on metadata",
    },
];

const SPLIT_VARIANTS: &[EnumVariant] = &[
    EnumVariant {
        name: "BOTH",
        value: "both",
        title: "Both note gap and punctuation",
    },
    EnumVariant {
        name: "GAP",
        value: "gap",
        title: "Note gap only",
    },
    EnumVariant {
        name: "SYMBOL",
        value: "symbol",
        title: "Punctuation only",
    },
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum OffsetPolicy {
    Timeline,
    Meta,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SplitOption {
    Both,
    Gap,
    Symbol,
}

struct OutputOptions {
    artist: String,
    title: String,
    album: String,
    by: String,
    offset: i64,
    offset_policy: OffsetPolicy,
    split_by: SplitOption,
    ignore_slur_notes: bool,
    /// Declared by the Python options but never used by `lrc_converter.dump`
    /// (the template output is always UTF-8).
    #[allow(dead_code)]
    encoding: String,
}

impl OutputOptions {
    fn from_options(options: &Options) -> Self {
        let offset_policy = match options.str("offset_policy", "timeline").as_str() {
            "meta" => OffsetPolicy::Meta,
            _ => OffsetPolicy::Timeline,
        };
        let split_by = match options.str("split_by", "both").as_str() {
            "gap" => SplitOption::Gap,
            "symbol" => SplitOption::Symbol,
            _ => SplitOption::Both,
        };
        Self {
            artist: options.str("artist", ""),
            title: options.str("title", ""),
            album: options.str("album", ""),
            by: options.str("by", ""),
            offset: options.int("offset", 0),
            offset_policy,
            split_by,
            ignore_slur_notes: options.bool("ignore_slur_notes", true),
            encoding: options.str("encoding", "utf-8"),
        }
    }
}

struct TimeTag {
    minute: i64,
    second: i64,
    millisecond: i64,
}

impl std::fmt::Display for TimeTag {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{:02}:{:02}.{:03}",
            self.minute, self.second, self.millisecond
        )
    }
}

struct LyricLine {
    time_tags: Vec<TimeTag>,
    lyric: String,
}

struct InfoTag {
    key: &'static str,
    value: String,
}

impl SvsPlugin for LrcPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_LRC
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_LRC_VERSION
    }

    fn readable(&self) -> bool {
        false
    }

    fn output_options(&self) -> Vec<OptionFieldDef> {
        vec![
            OptionFieldDef::string("artist", "Singer name", ""),
            OptionFieldDef::string("title", "Song title", ""),
            OptionFieldDef::string("album", "Album name", ""),
            OptionFieldDef::string("by", "Lyric editor", ""),
            OptionFieldDef::int("offset", "Offset", 0).with_description(
                "In milliseconds, positive means ahead, negative means opposite.",
            ),
            OptionFieldDef::enumerated(
                "offset_policy",
                "Offset policy",
                "OffsetPolicyOption",
                OFFSET_POLICY_VARIANTS,
                "TIMELINE",
            ),
            OptionFieldDef::enumerated(
                "split_by",
                "New line by",
                "SplitOption",
                SPLIT_VARIANTS,
                "BOTH",
            ),
            OptionFieldDef::bool(
                "ignore_slur_notes",
                "Ignore slur notes",
                true,
            )
            .with_description(
                "Ignore '-' lyrics that are used to indicate slur notes in singing synthesizers.",
            ),
            OptionFieldDef::bool(
                "timeline",
                "Write timeline",
                true,
            )
            .with_description("If you need lyrics without timeline, turn off this option."),
            OptionFieldDef::string("encoding", "Lyric Text encoding", "utf-8")
                .with_choices(supported_charset_names()),
        ]
    }

    fn load(&self, _path: &Path, _options: &Options) -> Result<Project> {
        // `WriteOnlyConverterMixin`: the plugin cannot be used as an input.
        Err(LibreSvipError::NotImplemented)
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let options = OutputOptions::from_options(options);
        let rendered = generate_project(project, &options)?;
        libresvip_core::io::write(path, rendered.as_bytes()).at_path(path)?;
        Ok(())
    }
}

/// Port of `LrcGenerator.generate_project` + `render_lrc`.
fn generate_project(project: &Project, options: &OutputOptions) -> Result<String> {
    let singing_track = project.first_singing_track().ok_or(LibreSvipError::NoTrack)?;
    let note_list = &singing_track.note_list;
    let synchronizer = TimeSynchronizer::from_project(&project.song_tempo_list);
    let has_tempo = !project.song_tempo_list.is_empty();

    let mut lyric_lines: Vec<LyricLine> = Vec::new();
    let mut buffer: Vec<(i64, String)> = Vec::new();
    for (index, note) in note_list.iter().enumerate() {
        buffer.push((note.start_pos, note.lyric.clone()));
        let condition_symbol = contains_symbol(&note.lyric);
        let condition_gap = index + 1 < note_list.len()
            && note_list[index + 1].start_pos - note.end_pos() >= 60;
        let mut commit_flag = match options.split_by {
            SplitOption::Symbol => condition_symbol,
            SplitOption::Gap => condition_gap,
            SplitOption::Both => condition_symbol || condition_gap,
        };
        if index + 1 == note_list.len() {
            commit_flag = true;
        }
        if commit_flag {
            commit_lyric_line(&synchronizer, has_tempo, options, &mut lyric_lines, &mut buffer)?;
            buffer.clear();
        }
    }

    let mut info_tags: Vec<InfoTag> = Vec::new();
    if !options.title.is_empty() {
        info_tags.push(InfoTag {
            key: "ti",
            value: options.title.clone(),
        });
    }
    if !options.artist.is_empty() {
        info_tags.push(InfoTag {
            key: "ar",
            value: options.artist.clone(),
        });
    }
    if !options.album.is_empty() {
        info_tags.push(InfoTag {
            key: "al",
            value: options.album.clone(),
        });
    }
    if !options.by.is_empty() {
        info_tags.push(InfoTag {
            key: "by",
            value: options.by.clone(),
        });
    }
    match options.offset_policy {
        OffsetPolicy::Timeline => {
            for line in &mut lyric_lines {
                for time_tag in &mut line.time_tags {
                    shift_time_tag(time_tag, options.offset);
                }
            }
        }
        OffsetPolicy::Meta => info_tags.push(InfoTag {
            key: "offset",
            value: options.offset.to_string(),
        }),
    }

    // `render_lrc`: UTF-8 regardless of the (unused) encoding option.
    let mut rendered = String::new();
    for info_tag in &info_tags {
        rendered.push('[');
        rendered.push_str(info_tag.key);
        rendered.push(':');
        rendered.push_str(&info_tag.value);
        rendered.push_str("]\n");
    }
    for line in &lyric_lines {
        for time_tag in &line.time_tags {
            rendered.push('[');
            rendered.push_str(&time_tag.to_string());
            rendered.push(']');
        }
        rendered.push_str(&line.lyric);
    }
    Ok(rendered)
}

/// Port of `LrcGenerator.commit_current_lyric_line`.
fn commit_lyric_line(
    synchronizer: &TimeSynchronizer,
    has_tempo: bool,
    options: &OutputOptions,
    lyric_lines: &mut Vec<LyricLine>,
    buffer: &mut [(i64, String)],
) -> Result<()> {
    let start_ticks = buffer
        .first()
        .map(|(ticks, _)| *ticks)
        .ok_or_else(|| LibreSvipError::msg("empty lyric buffer"))?;
    let (minute, second, millisecond) = epoch_parts(synchronizer, has_tempo, start_ticks)?;
    let mut lyrics = String::new();
    for (_, lyric) in buffer.iter() {
        let included = if options.ignore_slur_notes {
            lyric != "-"
        } else {
            true
        };
        if included {
            lyrics.push_str(&remove_symbols(lyric));
            if contains_latin(lyric) {
                lyrics.push(' ');
            }
        }
    }
    lyrics.push('\n');
    lyric_lines.push(LyricLine {
        time_tags: vec![TimeTag {
            minute,
            second,
            millisecond,
        }],
        lyric: lyrics,
    });
    Ok(())
}

/// `datetime.fromtimestamp(secs, tz=UTC)` reduced to (minute, second, microsecond).
fn epoch_parts(
    synchronizer: &TimeSynchronizer,
    has_tempo: bool,
    ticks: i64,
) -> Result<(i64, i64, i64)> {
    if !has_tempo {
        // Python would raise IndexError inside `TimeSynchronizer._secs_at_tick`.
        return Err(LibreSvipError::msg(
            "song tempo list must not be empty",
        ));
    }
    let secs = synchronizer.get_actual_secs_from_ticks(ticks);
    // CPython `datetime._fromtimestamp`: `frac, t = math.modf(timestamp)` then
    // `us = round(frac * 1e6)` (round-half-even) with carry into `t`.
    let whole = secs.trunc();
    let frac = secs - whole;
    let mut micros = (frac * 1_000_000.0).round_ties_even();
    let mut seconds = whole as i64;
    if micros >= 1_000_000.0 {
        seconds += 1;
        micros -= 1_000_000.0;
    } else if micros < 0.0 {
        seconds -= 1;
        micros += 1_000_000.0;
    }
    let second_of_day = seconds.rem_euclid(86_400);
    Ok((
        (second_of_day % 3_600) / 60,
        second_of_day % 60,
        // `milisecond=start_time.microsecond // 1000`
        micros as i64 / 1_000,
    ))
}

/// `OffsetPolicyOption.TIMELINE`: `datetime(...) + timedelta(microseconds=-offset)`.
fn shift_time_tag(time_tag: &mut TimeTag, offset: i64) {
    let total_micros = (time_tag.minute * 60 + time_tag.second) * 1_000_000
        + time_tag.millisecond * 1_000
        - offset;
    let total_micros = total_micros.rem_euclid(86_400 * 1_000_000);
    // `datetime(...)` normalises into a real date; only the time-of-day fields
    // are read, so the hour (24h) is dropped and `minute` stays 0..=59.
    time_tag.minute = (total_micros / 60_000_000) % 60;
    time_tag.second = (total_micros / 1_000_000) % 60;
    time_tag.millisecond = (total_micros / 1_000) % 1_000;
}

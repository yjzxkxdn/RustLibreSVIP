//! SRT plugin — port of `libresvip.plugins.srt`.
//!
//! Write-only subtitle generator (`WriteOnlyConverterMixin`). The Python plugin
//! builds a `pysubs2.SSAFile` and serialises it with `to_string("srt")`; the
//! relevant parts of that writer are reproduced here:
//!
//! * one block per lyric line, numbered from 1 over the *visible* events,
//! * `HH:MM:SS,mmm` timestamps clamped to `[0, 99:59:59,999]`,
//! * SSA override sequences are stripped (italic/underline/strikeout keep their
//!   `<i>`/`<u>`/`<s>` markup), `\h`/`\n`/`\N` are expanded, and runs of
//!   newlines are collapsed,
//! * each block ends with a blank line, so the file ends with `\n\n`.
//!
//! Timestamps use `int(seconds * 1000)`, i.e. truncation towards zero, exactly
//! like `srt_generator.commit_current_lyric_line`.

use std::path::Path;

use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{IoContext, LibreSvipError, Result};
use libresvip_core::model::base::{Note, Project};
use libresvip_core::options::{EnumVariant, OptionFieldDef, Options};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};
use libresvip_core::utils::text::supported_charset_names;

use crate::mid::text::{contains_latin, contains_symbol, remove_symbols};
use crate::plugin_infos;

pub struct SrtPlugin;

pub static PLUGIN: SrtPlugin = SrtPlugin;

#[allow(dead_code)]
fn _info() -> &'static PluginInfo {
    &plugin_infos::INFO_SRT
}

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

/// Largest timestamp allowed in SubRip (`pysubs2` `MAX_REPRESENTABLE_TIME`).
const MAX_REPRESENTABLE_TIME: i64 = 359_999_999;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SplitOption {
    Both,
    Gap,
    Symbol,
}

struct OutputOptions {
    track_index: i64,
    /// Declared by the Python options but never used by `srt_generator`.
    #[allow(dead_code)]
    offset: i64,
    split_by: SplitOption,
    ignore_slur_notes: bool,
    encoding: String,
}

impl OutputOptions {
    fn from_options(options: &Options) -> Self {
        let split_by = match options.str("split_by", "both").as_str() {
            "gap" => SplitOption::Gap,
            "symbol" => SplitOption::Symbol,
            _ => SplitOption::Both,
        };
        Self {
            track_index: options.int("track_index", -1),
            offset: options.int("offset", 0),
            split_by,
            ignore_slur_notes: options.bool("ignore_slur_notes", true),
            encoding: options.str("encoding", "utf-8"),
        }
    }
}

struct SubtitleLine {
    start: i64,
    end: i64,
    text: String,
}

impl SvsPlugin for SrtPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_SRT
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_SRT_VERSION
    }

    fn readable(&self) -> bool {
        false
    }

    fn output_options(&self) -> Vec<OptionFieldDef> {
        vec![
            libresvip_core::options::select_single_track(),
            OptionFieldDef::int("offset", "Offset", 0).with_description(
                "In milliseconds, positive means ahead, negative means opposite.",
            ),
            OptionFieldDef::enumerated(
                "split_by",
                "New line by",
                "SplitOption",
                SPLIT_VARIANTS,
                "BOTH",
            ),
            OptionFieldDef::bool("ignore_slur_notes", "Ignore slur notes", true).with_description(
                "Ignore '-' lyrics that are used to indicate slur notes in singing synthesizers.",
            ),
            OptionFieldDef::string("encoding", "Text encoding", "utf-8")
                .with_choices(supported_charset_names()),
        ]
    }

    fn load(&self, _path: &Path, _options: &Options) -> Result<Project> {
        // `WriteOnlyConverterMixin`: the plugin cannot be used as an input.
        Err(LibreSvipError::NotImplemented)
    }

    fn dump(&self, path: &Path, project: &Project, options: &Options) -> Result<()> {
        let options = OutputOptions::from_options(options);
        let text = generate_project(project, &options)?;
        let bytes = encode_strict(&text, &options.encoding)?;
        std::fs::write(path, &bytes).at_path(path)?;
        Ok(())
    }
}

/// Port of `SrtGenerator.generate_project` + `SSAFile.to_string("srt")`.
fn generate_project(project: &Project, options: &OutputOptions) -> Result<String> {
    let note_list: &[Note] = if options.track_index == -1 {
        project
            .track_list
            .iter()
            .find_map(|track| {
                track
                    .as_singing()
                    .filter(|singing| !singing.note_list.is_empty())
            })
            .map(|singing| singing.note_list.as_slice())
            .ok_or(LibreSvipError::NoTrack)?
    } else {
        let track_count = project.track_list.len() as i64;
        let index = if options.track_index < 0 {
            track_count + options.track_index
        } else {
            options.track_index
        };
        if index < 0 || index >= track_count {
            return Err(LibreSvipError::msg(format!(
                "list index out of range: {}",
                options.track_index
            )));
        }
        project.track_list[index as usize]
            .as_singing()
            .ok_or_else(|| LibreSvipError::msg("selected track is not a singing track"))?
            .note_list
            .as_slice()
    };

    let has_tempo = !project.song_tempo_list.is_empty();
    let synchronizer = TimeSynchronizer::from_project(&project.song_tempo_list);

    let mut lines: Vec<SubtitleLine> = Vec::new();
    let mut buffer: Vec<&Note> = Vec::new();
    for (index, note) in note_list.iter().enumerate() {
        buffer.push(note);
        let condition_symbol = contains_symbol(&note.lyric);
        let condition_gap =
            index + 1 < note_list.len() && note_list[index + 1].start_pos - note.end_pos() >= 60;
        let mut commit_flag = match options.split_by {
            SplitOption::Symbol => condition_symbol,
            SplitOption::Gap => condition_gap,
            SplitOption::Both => condition_symbol || condition_gap,
        };
        if index + 1 == note_list.len() {
            commit_flag = true;
        }
        if commit_flag {
            commit_line(&synchronizer, has_tempo, options, &mut lines, &buffer)?;
            buffer.clear();
        }
    }

    let mut rendered = String::new();
    let mut line_number = 1i64;
    for line in &lines {
        let (text, drawing) = prepare_text(&line.text);
        if drawing {
            // `SSAEvent.is_text` is False for SSA drawing lines.
            continue;
        }
        rendered.push_str(&line_number.to_string());
        rendered.push('\n');
        rendered.push_str(&ms_to_timestamp(line.start));
        rendered.push_str(" --> ");
        rendered.push_str(&ms_to_timestamp(line.end));
        rendered.push('\n');
        rendered.push_str(&text);
        rendered.push_str("\n\n");
        line_number += 1;
    }
    Ok(rendered)
}

/// Port of `SrtGenerator.commit_current_lyric_line`.
fn commit_line(
    synchronizer: &TimeSynchronizer,
    has_tempo: bool,
    options: &OutputOptions,
    lines: &mut Vec<SubtitleLine>,
    buffer: &[&Note],
) -> Result<()> {
    let first = buffer
        .first()
        .ok_or_else(|| LibreSvipError::msg("empty lyric buffer"))?;
    let last = buffer
        .last()
        .ok_or_else(|| LibreSvipError::msg("empty lyric buffer"))?;
    if !has_tempo {
        return Err(LibreSvipError::msg("song tempo list must not be empty"));
    }
    let start = (synchronizer.get_actual_secs_from_ticks(first.start_pos) * 1000.0) as i64;
    let end = (synchronizer.get_actual_secs_from_ticks(last.end_pos()) * 1000.0) as i64;
    let mut lyrics = String::new();
    for note in buffer {
        let included = if options.ignore_slur_notes {
            note.lyric != "-"
        } else {
            true
        };
        if included {
            lyrics.push_str(&remove_symbols(&note.lyric));
            if contains_latin(&note.lyric) {
                lyrics.push(' ');
            }
        }
    }
    lines.push(SubtitleLine {
        start,
        end,
        text: lyrics.trim().to_owned(),
    });
    Ok(())
}

/// `SubripFormat.ms_to_timestamp`.
fn ms_to_timestamp(ms: i64) -> String {
    let ms = ms.clamp(0, MAX_REPRESENTABLE_TIME);
    format!(
        "{:02}:{:02}:{:02},{:03}",
        ms / 3_600_000,
        (ms / 60_000) % 60,
        (ms / 1_000) % 60,
        ms % 1_000
    )
}

#[derive(Clone, Copy, Default)]
struct Style {
    italic: bool,
    underline: bool,
    strikeout: bool,
    drawing: bool,
}

/// `SubripFormat.to_file`'s `prepare_text`, returning the text and the
/// `pysubs2` `is_drawing` flag (evaluated on the raw text, like `SSAEvent`).
fn prepare_text(text: &str) -> (String, bool) {
    let (fragments, drawing) = parse_style_fragments(text);
    let mut body = String::new();
    for (fragment, style) in &fragments {
        body.push_str(&decorate(fragment, style));
    }
    let stripped = body.trim();
    (collapse_newlines(stripped), drawing)
}

fn decorate(fragment: &str, style: &Style) -> String {
    let mut decorated = fragment.to_owned();
    if style.italic {
        decorated = format!("<i>{decorated}</i>");
    }
    if style.underline {
        decorated = format!("<u>{decorated}</u>");
    }
    if style.strikeout {
        decorated = format!("<s>{decorated}</s>");
    }
    decorated
}

/// `re.sub("\n+", "\n", text)`.
fn collapse_newlines(text: &str) -> String {
    let mut collapsed = String::with_capacity(text.len());
    let mut previous_was_newline = false;
    for character in text.chars() {
        if character == '\n' {
            if previous_was_newline {
                continue;
            }
            previous_was_newline = true;
        } else {
            previous_was_newline = false;
        }
        collapsed.push(character);
    }
    collapsed
}

/// Port of `pysubs2.formats.substation.parse_tags` for the properties the SRT
/// writer observes, plus the drawing flag.
fn parse_style_fragments(text: &str) -> (Vec<(String, Style)>, bool) {
    // `prepare_text` expands whitespace tags *before* parsing overrides.
    let text = text
        .replace("\\h", " ")
        .replace("\\n", "\n")
        .replace("\\N", "\n");
    let mut style = Style::default();
    let mut drawing = false;
    let mut fragments = Vec::new();
    let mut rest = text.as_str();
    loop {
        let Some(open) = rest.find('{') else {
            fragments.push((rest.to_owned(), style));
            break;
        };
        let Some(close_rel) = rest[open..].find('}') else {
            fragments.push((rest.to_owned(), style));
            break;
        };
        let close = open + close_rel;
        fragments.push((rest[..open].to_owned(), style));
        apply_overrides(&rest[open + 1..close], &mut style);
        drawing |= style.drawing;
        rest = &rest[close + 1..];
    }
    (fragments, drawing)
}

/// `apply_overrides` from `pysubs2.formats.substation`, restricted to the
/// style flags the SubRip writer uses.
fn apply_overrides(overrides: &str, style: &mut Style) {
    let chars: Vec<char> = overrides.chars().collect();
    let mut index = 0usize;
    while index < chars.len() {
        if chars[index] != '\\' || index + 1 >= chars.len() {
            index += 1;
            continue;
        }
        match chars[index + 1] {
            flag @ ('i' | 'b' | 'u' | 's' | 'p') => {
                if index + 2 < chars.len() && chars[index + 2].is_ascii_digit() {
                    let enabled = chars[index + 2] == '1';
                    match flag {
                        'i' => style.italic = enabled,
                        'u' => style.underline = enabled,
                        's' => style.strikeout = enabled,
                        'p' => style.drawing = enabled,
                        _ => {}
                    }
                    index += 3;
                } else {
                    index += 2;
                }
            }
            'r' => {
                // `\r` / `\rStyleName`: reset to the original line style.
                *style = Style::default();
                index += 2;
                while index < chars.len()
                    && (chars[index].is_ascii_alphanumeric()
                        || chars[index] == '_'
                        || chars[index] == ' ')
                {
                    index += 1;
                }
            }
            'f' => {
                // `\fnFontName`: only the font name changes.
                index += 2;
                if index < chars.len() && chars[index] == 'n' {
                    index += 1;
                }
                while index < chars.len()
                    && (chars[index].is_ascii_alphanumeric()
                        || chars[index] == '_'
                        || chars[index] == ' ')
                {
                    index += 1;
                }
            }
            _ => index += 1,
        }
    }
}

/// `str.encode(encoding)` with `errors="strict"` (the SRT converter's default).
fn encode_strict(text: &str, label: &str) -> Result<Vec<u8>> {
    if label.eq_ignore_ascii_case("utf-8") || label.eq_ignore_ascii_case("utf8") {
        return Ok(text.as_bytes().to_vec());
    }
    let normalized = label.trim();
    let encoding = encoding_rs::Encoding::for_label(normalized.as_bytes())
        .ok_or_else(|| LibreSvipError::msg(format!("unknown encoding: {normalized}")))?;
    let mut encoder = encoding.new_encoder();
    let mut output: Vec<u8> = Vec::new();
    let mut rest = text;
    loop {
        let (result, read) =
            encoder.encode_from_utf8_to_vec_without_replacement(rest, &mut output, true);
        rest = &rest[read..];
        match result {
            encoding_rs::EncoderResult::InputEmpty => break,
            encoding_rs::EncoderResult::OutputFull => output.reserve(4096),
            encoding_rs::EncoderResult::Unmappable(character) => {
                return Err(LibreSvipError::msg(format!(
                    "'{normalized}' codec can't encode character {character:?}"
                )));
            }
        }
    }
    Ok(output)
}

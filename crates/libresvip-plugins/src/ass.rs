//! ASS (Advanced SubStation Alpha) subtitle generator — port of
//! `libresvip.plugins.ass`.
//!
//! The Python plugin builds a `pysubs2.SSAFile` and serialises it with
//! `SSAFile.to_string("ass")`; this module reproduces that writer byte for
//! byte (fixed header, default style, timestamp rounding and trailing newline).
//! It is write-only.

use std::fmt::Write as _;
use std::path::Path;

use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{IoContext, LibreSvipError, Result};
use libresvip_core::model::base::{Note, Project, Track};
use libresvip_core::options::{EnumVariant, OptionFieldDef, Options};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};

use crate::plugin_infos;

pub struct AssPlugin;

pub static PLUGIN: AssPlugin = AssPlugin;

// ---------------------------------------------------------------------------
// Options (`libresvip/plugins/ass/options.py`)
// ---------------------------------------------------------------------------

pub static SPLIT_OPTION_VARIANTS: &[EnumVariant] = &[
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

fn output_options() -> Vec<OptionFieldDef> {
    vec![
        libresvip_core::options::select_single_track(),
        OptionFieldDef::int(
            "offset",
            "Offset",
            0,
        )
        .with_description("In milliseconds, positive means ahead, negative means opposite."),
        OptionFieldDef::enumerated(
            "split_by",
            "New line by",
            "SplitOption",
            SPLIT_OPTION_VARIANTS,
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
        OptionFieldDef::string("encoding", "Text encoding", "utf-8"),
    ]
}

// ---------------------------------------------------------------------------
// Text helpers (`libresvip.utils.text`)
// ---------------------------------------------------------------------------

/// Characters matched by `SYMBOL_PATTERN`.
///
/// The Python pattern is `[...]+` with a redundant `(?!-)` prefix; the ASCII
/// hyphen is not part of the class anyway, so a membership test is equivalent
/// to `SYMBOL_PATTERN.search()` / `SYMBOL_PATTERN.sub()`.
fn is_symbol_char(character: char) -> bool {
    matches!(
        character,
        '!' | '"' | '#'
            | '$'
            | '%'
            | '&'
            | '\''
            | '('
            | ')'
            | '*'
            | ','
            | '.'
            | '/'
            | ':'
            | ';'
            | '<'
            | '='
            | '>'
            | '?'
            | '['
            | '\\'
            | ']'
            | '^'
            | '_'
            | '`'
            | '{'
            | '|'
            | '}'
            | '~'
            | '\u{ff02}'
            | '\u{ff03}'
            | '\u{ff04}'
            | '\u{ff05}'
            | '\u{ff06}'
            | '\u{ff07}'
            | '\u{ff08}'
            | '\u{ff09}'
            | '\u{ff0a}'
            | '\u{ff0b}'
            | '\u{ff0c}'
            | '\u{ff0d}'
            | '\u{ff0f}'
            | '\u{ff1a}'
            | '\u{ff1b}'
            | '\u{ff1c}'
            | '\u{ff1d}'
            | '\u{ff1e}'
            | '\u{ff20}'
            | '\u{ff3b}'
            | '\u{ff3c}'
            | '\u{ff3d}'
            | '\u{ff3e}'
            | '\u{ff3f}'
            | '\u{ff40}'
            | '\u{ff5b}'
            | '\u{ff5c}'
            | '\u{ff5d}'
            | '\u{ff5e}'
            | '\u{ff5f}'
            | '\u{ff60}'
            | '\u{ff62}'
            | '\u{ff63}'
            | '\u{ff64}'
            | '\u{3000}'
            | '\u{3001}'
            | '\u{3003}'
            | '\u{3008}'
            | '\u{3009}'
            | '\u{300a}'
            | '\u{300b}'
            | '\u{300c}'
            | '\u{300d}'
            | '\u{300e}'
            | '\u{300f}'
            | '\u{3010}'
            | '\u{3011}'
            | '\u{3014}'
            | '\u{3015}'
            | '\u{3016}'
            | '\u{3017}'
            | '\u{3018}'
            | '\u{3019}'
            | '\u{301a}'
            | '\u{301b}'
            | '\u{301c}'
            | '\u{301d}'
            | '\u{301e}'
            | '\u{301f}'
            | '\u{3030}'
            | '\u{303e}'
            | '\u{303f}'
            | '\u{2013}'
            | '\u{2014}'
            | '\u{2018}'
            | '\u{2019}'
            | '\u{201b}'
            | '\u{201c}'
            | '\u{201d}'
            | '\u{201e}'
            | '\u{201f}'
            | '\u{2026}'
            | '\u{2027}'
            | '\u{fe4f}'
            | '\u{fe51}'
            | '\u{fe54}'
            | '\u{00b7}'
            | '\u{ff0e}'
            | '\u{ff01}'
            | '\u{ff1f}'
            | '\u{ff61}'
            | '\u{3002}'
    )
}

/// `SYMBOL_PATTERN.search(lyric) is not None`.
fn contains_symbol(lyric: &str) -> bool {
    lyric.chars().any(is_symbol_char)
}

/// `SYMBOL_PATTERN.sub("", lyric)`.
fn remove_symbols(lyric: &str) -> String {
    lyric
        .chars()
        .filter(|character| !is_symbol_char(*character))
        .collect()
}

/// `LATIN_ALPHABET.search(lyric) is not None`.
fn contains_latin(lyric: &str) -> bool {
    lyric.chars().any(|character| character.is_ascii_alphabetic())
}

// ---------------------------------------------------------------------------
// pysubs2 writer (`pysubs2/formats/substation.py`)
// ---------------------------------------------------------------------------

/// `SubstationFormat.ms_to_timestamp` — clamps to 9:59:59.99 and rounds to
/// centiseconds the Aegisub way.
fn ms_to_timestamp(requested_ms: i64) -> String {
    const MAX_REPRESENTABLE_TIME: i64 = 10 * 3_600_000 - 10;
    let requested_ms = requested_ms.clamp(0, MAX_REPRESENTABLE_TIME);
    let rounded = (requested_ms + 5) - (requested_ms + 5) % 10;
    let hours = rounded / 3_600_000;
    let minutes = (rounded % 3_600_000) / 60_000;
    let seconds = (rounded % 60_000) / 1000;
    let centiseconds = (rounded % 1000) / 10;
    format!("{hours:01}:{minutes:02}:{seconds:02}.{centiseconds:02}")
}

const SCRIPT_HEADER: &str = "[Script Info]\n\
; Script generated by pysubs2\n\
; https://pypi.python.org/pypi/pysubs2\n\
WrapStyle: 0\n\
ScaledBorderAndShadow: yes\n\
Collisions: Normal\n\
ScriptType: v4.00+\n\
\n\
[V4+ Styles]\n\
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n\
Style: Default,Arial,20,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\n\
\n\
[Events]\n\
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n";

// ---------------------------------------------------------------------------
// Generator (`ass_generator.py`)
// ---------------------------------------------------------------------------

struct Event {
    start: i64,
    end: i64,
    text: String,
}

fn encode_text(text: &str, encoding: &str) -> Result<Vec<u8>> {
    let normalized = encoding.trim();
    if normalized.eq_ignore_ascii_case("utf-8") || normalized.eq_ignore_ascii_case("utf8") {
        return Ok(text.as_bytes().to_vec());
    }
    let Some(encoding) = encoding_rs::Encoding::for_label(normalized.as_bytes()) else {
        return Err(LibreSvipError::Message(format!(
            "unknown encoding: {encoding}"
        )));
    };
    let (bytes, _, _) = encoding.encode(text);
    Ok(bytes.into_owned())
}

fn select_track<'a>(project: &'a Project, track_index: i64) -> Result<&'a [Note]> {
    if track_index == -1 {
        project
            .track_list
            .iter()
            .find_map(|track| match track {
                Track::Singing(singing) if !singing.note_list.is_empty() => {
                    Some(singing.note_list.as_slice())
                }
                _ => None,
            })
            .ok_or(LibreSvipError::NoTrack)
    } else {
        let index = if track_index < 0 {
            project.track_list.len() as i64 + track_index
        } else {
            track_index
        };
        if index < 0 {
            return Err(LibreSvipError::NoTrack);
        }
        let track = project
            .track_list
            .get(index as usize)
            .ok_or(LibreSvipError::NoTrack)?;
        match track {
            Track::Singing(singing) => Ok(singing.note_list.as_slice()),
            Track::Instrumental(_) => Err(LibreSvipError::NoTrack),
        }
    }
}

fn generate_project(project: &Project, options: &Options) -> Result<String> {
    let track_index = options.int("track_index", -1);
    let split_by = options.str("split_by", "both");
    if !SPLIT_OPTION_VARIANTS
        .iter()
        .any(|variant| variant.value == split_by)
    {
        return Err(LibreSvipError::Validation(format!(
            "Input should be 'both', 'gap' or 'symbol'"
        )));
    }
    let ignore_slur_notes = options.bool("ignore_slur_notes", true);

    let note_list = select_track(project, track_index)?;
    let synchronizer = TimeSynchronizer::from_project(&project.song_tempo_list);

    let mut events: Vec<Event> = Vec::new();
    let mut buffer: Vec<&Note> = Vec::new();

    for (index, note) in note_list.iter().enumerate() {
        buffer.push(note);
        let condition_symbol = contains_symbol(&note.lyric);
        let condition_gap = index + 1 < note_list.len()
            && note_list[index + 1].start_pos - note.end_pos() >= 60;
        let mut commit_flag = match split_by.as_str() {
            "symbol" => condition_symbol,
            "gap" => condition_gap,
            "both" => condition_symbol || condition_gap,
            _ => false,
        };
        if index + 1 == note_list.len() {
            commit_flag = true;
        }
        if commit_flag {
            if let Some(event) = commit_current_lyric_line(
                &buffer,
                &synchronizer,
                ignore_slur_notes,
            ) {
                events.push(event);
            }
            buffer.clear();
        }
    }

    let mut output = String::from(SCRIPT_HEADER);
    for event in &events {
        let _ = write!(
            output,
            "Dialogue: 0,{},{},Default,,0,0,0,,{}\n",
            ms_to_timestamp(event.start),
            ms_to_timestamp(event.end),
            event.text
        );
    }
    Ok(output)
}

fn commit_current_lyric_line(
    buffer: &[&Note],
    synchronizer: &TimeSynchronizer,
    ignore_slur_notes: bool,
) -> Option<Event> {
    let first = *buffer.first()?;
    let last = *buffer.last()?;
    // `int(secs * 1000)` truncates toward zero.
    let start_time = (synchronizer.get_actual_secs_from_ticks(first.start_pos) * 1000.0) as i64;
    let end_time = (synchronizer.get_actual_secs_from_ticks(last.end_pos()) * 1000.0) as i64;
    let mut lyrics = String::new();
    for note in buffer {
        if ignore_slur_notes && note.lyric == "-" {
            continue;
        }
        lyrics.push_str(&remove_symbols(&note.lyric));
        if contains_latin(&note.lyric) {
            lyrics.push(' ');
        }
    }
    Some(Event {
        start: start_time,
        end: end_time,
        text: lyrics,
    })
}

impl SvsPlugin for AssPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_ASS
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_ASS_VERSION
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
        let text = generate_project(project, options)?;
        let encoding = options.str("encoding", "utf-8");
        let bytes = encode_text(&text, &encoding)?;
        libresvip_core::io::write(path, bytes).at_path(path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(start: i64, length: i64, lyric: &str) -> Note {
        Note::new(start, length, 60, lyric)
    }

    #[test]
    fn timestamps_match_pysubs2() {
        assert_eq!(ms_to_timestamp(0), "0:00:00.00");
        assert_eq!(ms_to_timestamp(1250), "0:00:01.25");
        assert_eq!(ms_to_timestamp(1255), "0:00:01.26");
        assert_eq!(ms_to_timestamp(-5), "0:00:00.00");
        assert_eq!(ms_to_timestamp(35_999_999), "9:59:59.99");
    }

    #[test]
    fn symbols_are_removed_but_hyphen_is_kept() {
        assert!(contains_symbol("a, b"));
        assert!(!contains_symbol("la-li"));
        assert_eq!(remove_symbols("la, li!"), "la li");
        assert_eq!(remove_symbols("你好，世界。"), "你好世界");
    }

    #[test]
    fn lyric_lines_join_with_latin_spaces() {
        let notes = [note(0, 480, "la"), note(480, 480, "-"), note(960, 240, "lu")];
        let refs: Vec<&Note> = notes.iter().collect();
        let synchronizer = TimeSynchronizer::from_project(&[libresvip_core::model::base::SongTempo::default()]);
        let event = commit_current_lyric_line(&refs, &synchronizer, true).unwrap();
        assert_eq!(event.text, "la lu ");
        assert_eq!(event.end, 1250);
    }
}

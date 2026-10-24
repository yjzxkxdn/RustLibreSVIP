//! Port of `libresvip.plugins.tsmsln.voisona_mobile_generator`.

use libresvip_core::core::constants::KEY_IN_OCTAVE;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::core::warning_types::{show_warning_kind, WarningKind};
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{Note, Project, SingingTrack, SongTempo, TimeSignature, Track};
use libresvip_core::options::Options;

use super::constants::{DEFAULT_PHONEME, OCTAVE_OFFSET, TICK_RATE};
use super::model::{DEFAULT_PROJECT_VERSION, DEFAULT_STATE_VERSION};
use super::pitch::generate_for_techno_speech;
use super::value_tree::{Node, Variant};

/// `VoiSonaMobileGenerator.generate_project`.
pub fn generate_project(project: &Project, options: &Options) -> Result<Node> {
    let track_index = options.int("track_index", -1);
    let first_singing_track: Option<&SingingTrack> = if track_index < 0 {
        project.track_list.iter().find_map(|track| match track {
            Track::Singing(singing) if !singing.note_list.is_empty() => Some(singing),
            _ => None,
        })
    } else {
        let index = track_index as usize;
        match project.track_list.get(index) {
            Some(Track::Singing(singing)) => Some(singing),
            Some(_) => {
                return Err(LibreSvipError::Validation(
                    "the selected track is not a singing track".to_owned(),
                ))
            }
            None => {
                return Err(LibreSvipError::Validation(format!(
                    "track index out of range: {track_index}"
                )))
            }
        }
    };

    let tempo_list = &project.song_tempo_list;
    let time_synchronizer = TimeSynchronizer::from_project(tempo_list);
    let first_time_signature = project
        .time_signature_list
        .first()
        .ok_or_else(|| LibreSvipError::msg("no time signature found"))?;
    let first_bar_length = first_time_signature.bar_length() as i64;
    let default_time_signatures = generate_time_signatures(&project.time_signature_list)?;
    let default_tempos = generate_tempos(tempo_list);

    let mut root = Node::new("MobileSongEditor").with_attr(
        "VersionOfAppFileSaved",
        Variant::Str(DEFAULT_PROJECT_VERSION.to_owned()),
    );
    if let Some(singing) = first_singing_track {
        let mut singer = Node::new("MobileSinger");
        let mut state = Node::new("StateInformation");
        state.push_attr("TempoSync", Variant::Bool(false));
        push_str(
            &mut state,
            "VersionOfAppFileSaved",
            DEFAULT_STATE_VERSION.to_owned(),
        );

        let mut song = Node::new("Song");
        song.push_child(tempo_node(&default_tempos)?);
        song.push_child(beat_node(&default_time_signatures)?);
        song.push_child(score_node(&singing.note_list)?);
        state.push_child(song);

        if let Some(pitch_data) = generate_for_techno_speech(
            &singing.edited_params.pitch,
            tempo_list,
            first_bar_length,
        )? {
            let mut parameters = Node::new("Parameter");
            let mut log_f0 = Node::new("LogF0");
            push_int(&mut log_f0, "Length", pitch_data.length())?;
            for event in &pitch_data.events {
                let mut data = Node::new("Data");
                if let Some(index) = event.idx {
                    push_int(&mut data, "Index", index)?;
                }
                if let Some(repeat) = event.repeat {
                    push_int(&mut data, "Repeat", repeat)?;
                }
                data.push_attr("Value", Variant::Double(event.value));
                log_f0.push_child(data);
            }
            parameters.push_child(log_f0);
            state.push_child(parameters);
        }

        let _ = &time_synchronizer;
        let binary = super::value_tree::build(&state)?;
        singer.push_attr("ModileSingerData", Variant::Binary(binary));
        root.push_child(singer);
    }
    Ok(root)
}

fn push_str(node: &mut Node, name: &str, value: String) {
    node.push_attr(name, Variant::Str(value));
}

fn push_int(node: &mut Node, name: &str, value: i64) -> Result<()> {
    let value = i32::try_from(value).map_err(|_| {
        LibreSvipError::Validation(format!("value out of range for attribute {name}: {value}"))
    })?;
    node.push_attr(name, Variant::Int(value));
    Ok(())
}

/// `VoiSonaMobileGenerator.generate_tempos`.
fn generate_tempos(tempos: &[SongTempo]) -> Vec<(i64, f64)> {
    tempos
        .iter()
        .enumerate()
        .map(|(index, tempo)| {
            let clock = if index == 0 {
                0
            } else {
                (tempo.position as f64 * TICK_RATE).round_ties_even() as i64
            };
            (clock, tempo.bpm)
        })
        .collect()
}

/// `VoiSonaMobileGenerator.generate_time_signatures`.
fn generate_time_signatures(time_signatures: &[TimeSignature]) -> Result<Vec<(i64, i64, i64)>> {
    let first = time_signatures
        .first()
        .ok_or_else(|| LibreSvipError::msg("no time signature found"))?;
    let mut times = vec![(0, first.numerator, first.denominator)];
    let mut tick = 0.0f64;
    let mut prev_time_signature = first;
    for time_signature in &time_signatures[1..] {
        if time_signature.bar_index > prev_time_signature.bar_index {
            tick += (time_signature.bar_index - prev_time_signature.bar_index) as f64
                * prev_time_signature.bar_length();
        }
        times.push((
            (tick * TICK_RATE) as i64,
            time_signature.numerator,
            time_signature.denominator,
        ));
        prev_time_signature = time_signature;
    }
    Ok(times)
}

fn tempo_node(tempos: &[(i64, f64)]) -> Result<Node> {
    let mut node = Node::new("Tempo");
    for (clock, bpm) in tempos {
        let mut sound = Node::new("Sound");
        push_int(&mut sound, "Clock", *clock)?;
        sound.push_attr("Tempo", Variant::Double(*bpm));
        node.push_child(sound);
    }
    Ok(node)
}

fn beat_node(times: &[(i64, i64, i64)]) -> Result<Node> {
    let mut node = Node::new("Beat");
    for (clock, beats, beat_type) in times {
        let mut time = Node::new("Time");
        push_int(&mut time, "Clock", *clock)?;
        push_int(&mut time, "Beats", *beats)?;
        push_int(&mut time, "BeatType", *beat_type)?;
        node.push_child(time);
    }
    Ok(node)
}

fn score_node(notes: &[Note]) -> Result<Node> {
    let mut node = Node::new("Score");
    for note in notes {
        node.push_child(note_node(note)?);
    }
    Ok(node)
}

/// `wanakana.is_char_kana`: hiragana U+3041..=U+3096, katakana U+30A1..=U+30FC
/// (plus the prolonged sound mark and the katakana punctuation inside it).
fn is_char_kana(character: char) -> bool {
    let code = character as u32;
    code == 0x30FC || (0x3041..=0x3096).contains(&code) || (0x30A1..=0x30FC).contains(&code)
}

/// `wanakana.is_kana`.
fn is_kana(text: &str) -> bool {
    !text.is_empty() && text.chars().all(is_char_kana)
}

/// `wanakana.is_char_romaji`: ASCII plus the Hepburn macron ranges.
fn is_char_romaji(character: char) -> bool {
    let code = character as u32;
    code <= 0x007F
        || (0x0100..=0x0101).contains(&code)
        || (0x0112..=0x0113).contains(&code)
        || (0x012A..=0x012B).contains(&code)
        || (0x014C..=0x014D).contains(&code)
        || (0x016A..=0x016B).contains(&code)
}

/// `wanakana.is_romaji`.
fn is_romaji(text: &str) -> bool {
    !text.is_empty() && text.chars().all(is_char_romaji)
}

/// The `phoneme = DEFAULT_PHONEME` branch of `generate_notes`.
fn fallback_phoneme(lyric: &str) -> String {
    if !is_kana(lyric) && !is_romaji(lyric) {
        show_warning_kind(WarningKind::FallbackApplied, format!("Unsupported lyric:  {lyric}"));
        DEFAULT_PHONEME.to_owned()
    } else {
        String::new()
    }
}

/// `VoiSonaMobileGenerator.generate_notes`.
fn note_node(note: &Note) -> Result<Node> {
    let lyric = note.lyric.clone();
    // Python uses truthiness here: an empty pronunciation falls through to the
    // kana/romaji check (unlike the `.tssln` generator, which keeps it).
    let phoneme = if let Some(pronunciation) = &note.pronunciation {
        if pronunciation.is_empty() {
            fallback_phoneme(&lyric)
        } else {
            pronunciation.clone()
        }
    } else {
        fallback_phoneme(&lyric)
    };
    let mut node = Node::new("Note");
    push_int(&mut node, "Clock", (note.start_pos as f64 * TICK_RATE) as i64)?;
    push_int(
        &mut node,
        "Duration",
        (note.length as f64 * TICK_RATE) as i64,
    )?;
    push_int(
        &mut node,
        "PitchStep",
        note.key_number.rem_euclid(KEY_IN_OCTAVE),
    )?;
    push_int(
        &mut node,
        "PitchOctave",
        note.key_number.div_euclid(KEY_IN_OCTAVE) + OCTAVE_OFFSET,
    )?;
    push_str(&mut node, "Lyric", lyric);
    push_int(&mut node, "Syllabic", 0)?;
    push_str(&mut node, "Phoneme", phoneme);
    Ok(node)
}

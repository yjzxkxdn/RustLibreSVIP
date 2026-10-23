//! Port of `libresvip.plugins.tssln.voisona_generator`.

use libresvip_core::core::constants::KEY_IN_OCTAVE;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{Note, Project, SongTempo, TimeSignature, Track};
use libresvip_core::options::Options;

use super::constants::{OCTAVE_OFFSET, TICK_RATE};
use super::model::{DEFAULT_PROJECT_VERSION, DEFAULT_STATE_VERSION};
use super::pitch::generate_for_techno_speech;
use super::value_tree::{Node, Variant};

/// `VoiSonaGenerator.generate_project`, producing the JUCE value tree.
pub fn generate_project(project: &Project, _options: &Options) -> Result<Node> {
    let tempo_list = &project.song_tempo_list;
    let time_synchronizer = TimeSynchronizer::from_project(tempo_list);
    let first_time_signature = project
        .time_signature_list
        .first()
        .ok_or_else(|| LibreSvipError::msg("no time signature found"))?;
    let first_bar_length = first_time_signature.bar_length() as i64;
    let default_time_signatures = generate_time_signatures(&project.time_signature_list)?;
    let default_tempos = generate_tempos(tempo_list);

    let mut root = Node::new("TSSolution").with_attr(
        "VersionOfAppFileSaved",
        Variant::Str(DEFAULT_PROJECT_VERSION.to_owned()),
    );
    let mut tracks_node = Node::new("Tracks");
    for (index, track) in project.track_list.iter().enumerate() {
        let number = index + 1;
        match track {
            Track::Instrumental(instrumental) => {
                let mut node = Node::new("Track");
                push_str(&mut node, "Name", format!("Audio{number}"));
                push_int(&mut node, "State", 0)?;
                node.push_attr("Volume", Variant::Double(0.0));
                node.push_attr("Pan", Variant::Double(0.0));
                push_int(&mut node, "Type", 2)?;
                let mut event = Node::new("AudioEvent");
                push_str(&mut event, "Path", instrumental.audio_file_path.clone());
                if tempo_list.is_empty() {
                    // Python raises `IndexError` from the synchroniser here.
                    return Err(LibreSvipError::msg("no tempo found"));
                }
                event.push_attr(
                    "Offset",
                    Variant::Double(
                        time_synchronizer.get_actual_secs_from_ticks(instrumental.offset),
                    ),
                );
                node.push_child(event);
                tracks_node.push_child(node);
            }
            Track::Singing(singing) => {
                let mut node = Node::new("Track");
                push_str(&mut node, "Name", format!("Singer{number}"));
                push_int(&mut node, "State", 0)?;
                node.push_attr("Volume", Variant::Double(0.0));
                node.push_attr("Pan", Variant::Double(0.0));
                push_int(&mut node, "Type", 0)?;

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

                let binary = super::value_tree::build(&state)?;
                node.push_attr("PluginData", Variant::Binary(binary));
                tracks_node.push_child(node);
            }
        }
    }
    root.push_child(tracks_node);
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

/// `VoiSonaGenerator.generate_tempos`.
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

/// `VoiSonaGenerator.generate_time_signatures`.
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

/// `VoiSonaGenerator.generate_notes`.
fn note_node(note: &Note) -> Result<Node> {
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
    push_str(&mut node, "Lyric", note.lyric.clone());
    push_int(&mut node, "Syllabic", 0)?;
    if let Some(phoneme) = &note.pronunciation {
        push_str(&mut node, "Phoneme", phoneme.clone());
    }
    Ok(node)
}

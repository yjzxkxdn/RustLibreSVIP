//! Port of `libresvip.plugins.mid.midi_generator`.

use libresvip_core::core::lyric_phoneme::chinese::get_pinyin_series;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{Project, SingingTrack, TimeSignature};
use libresvip_core::model::base::TICKS_IN_BEAT;

use super::codec::encode_replace;
use super::midi_file::{bpm2tempo, OutEvent, OutMessage};
use super::pitch::{generate_for_midi, round_to_i64};
use super::text::remove_symbols;
use super::OutputOptions;

/// Result of `MidiGenerator.generate_project`.
pub struct GeneratedMidi {
    pub format: u16,
    pub ticks_per_beat: i64,
    pub tracks: Vec<Vec<OutMessage>>,
}

fn tick_rate(options: &OutputOptions) -> f64 {
    TICKS_IN_BEAT as f64 / options.ticks_per_beat as f64
}

/// Port of `MidiGenerator.generate_project` (the cumulative → delta conversion
/// is applied to every track before returning).
pub fn generate_project(project: &Project, options: &OutputOptions) -> Result<GeneratedMidi> {
    let Some(first_time_signature) = project.time_signature_list.first() else {
        // Python would raise IndexError on `project.time_signature_list[0]`.
        return Err(LibreSvipError::msg("project has no time signature"));
    };
    let first_bar_length = round_to_i64(first_time_signature.bar_length_with(options.ticks_per_beat));
    let synchronizer = TimeSynchronizer::from_project(&project.song_tempo_list);
    let mut tracks: Vec<Vec<OutMessage>> = Vec::new();
    let mut master_track: Vec<OutMessage> = Vec::new();
    generate_tempos(&mut master_track, project, options)?;
    generate_time_signatures(&mut master_track, &project.time_signature_list, options)?;
    let end_time = master_track.last().map(|message| message.time).unwrap_or(0);
    master_track.push(OutMessage {
        time: end_time,
        event: OutEvent::EndOfTrack,
    });
    master_track.sort_by_key(|message| message.time);
    tracks.push(master_track);
    for track in &project.track_list {
        if let Some(singing_track) = track.as_singing() {
            if let Some(messages) = generate_track(
                singing_track,
                &project.time_signature_list,
                options,
                &synchronizer,
                first_bar_length,
            )? {
                tracks.push(messages);
            }
        }
    }
    convert_cumulative_to_delta(&mut tracks);
    Ok(GeneratedMidi {
        format: 1,
        ticks_per_beat: options.ticks_per_beat,
        tracks,
    })
}

/// `_convert_cumulative_to_delta`.
fn convert_cumulative_to_delta(tracks: &mut [Vec<OutMessage>]) {
    for track in tracks.iter_mut() {
        let mut tick = 0i64;
        for message in track.iter_mut() {
            let time = message.time;
            message.time = time - tick;
            tick = time;
        }
    }
}

/// `generate_tempos`.
fn generate_tempos(
    master_track: &mut Vec<OutMessage>,
    project: &Project,
    options: &OutputOptions,
) -> Result<()> {
    let rate = tick_rate(options);
    for tempo in &project.song_tempo_list {
        if tempo.position >= 0 {
            master_track.push(OutMessage {
                time: round_to_i64(tempo.position as f64 / rate),
                event: OutEvent::SetTempo(bpm2tempo(tempo.bpm)?),
            });
        }
    }
    Ok(())
}

/// `generate_time_signatures`.
fn generate_time_signatures(
    master_track: &mut Vec<OutMessage>,
    time_signature_list: &[TimeSignature],
    options: &OutputOptions,
) -> Result<()> {
    let mut prev_ticks = 0i64;
    let mut prev_time_signature: Option<&TimeSignature> = None;
    for time_signature in time_signature_list {
        if time_signature.bar_index >= 0 {
            if let Some(previous) = prev_time_signature {
                prev_ticks += round_to_i64(
                    (time_signature.bar_index - previous.bar_index) as f64
                        * previous.bar_length_with(options.ticks_per_beat),
                );
            }
            master_track.push(OutMessage {
                time: prev_ticks,
                event: OutEvent::TimeSignature {
                    numerator: u8::try_from(time_signature.numerator).map_err(|_| {
                        LibreSvipError::msg(format!(
                            "time signature numerator out of range: {}",
                            time_signature.numerator
                        ))
                    })?,
                    denominator: u8::try_from(time_signature.denominator).map_err(|_| {
                        LibreSvipError::msg(format!(
                            "time signature denominator out of range: {}",
                            time_signature.denominator
                        ))
                    })?,
                    clocks_per_click: 24,
                    notated_32nd_notes_per_quarter: 8,
                },
            });
            prev_time_signature = Some(time_signature);
        }
    }
    Ok(())
}

/// `generate_track`.
fn generate_track(
    track: &SingingTrack,
    time_signature_list: &[TimeSignature],
    options: &OutputOptions,
    synchronizer: &TimeSynchronizer,
    first_bar_length: i64,
) -> Result<Option<Vec<OutMessage>>> {
    let mut lyrics: Vec<String> = track
        .note_list
        .iter()
        .map(|note| {
            note.pronunciation
                .clone()
                .unwrap_or_else(|| note.lyric.clone())
        })
        .collect();
    if options.remove_symbols {
        lyrics = lyrics
            .iter()
            .map(|lyric| remove_symbols(lyric))
            .collect();
    }
    // `get_pinyin_series` is only needed by the compatibility mode; the core
    // pypinyin port is still missing, so it is only invoked when requested.
    let pinyins: Vec<String> = if options.compatible_lyric {
        let lyric_refs: Vec<&str> = lyrics.iter().map(String::as_str).collect();
        get_pinyin_series(&lyric_refs)?
    } else {
        Vec::new()
    };
    let rate = tick_rate(options);
    let mut messages: Vec<OutMessage> = Vec::new();
    messages.push(OutMessage {
        time: 0,
        event: OutEvent::TrackName(encode_replace(
            &track.title,
            &options.lyric_encoding,
        )?),
    });
    for (index, note) in track.note_list.iter().enumerate() {
        if options.export_lyrics {
            let lyric = if options.compatible_lyric {
                pinyins
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| lyrics[index].clone())
            } else {
                lyrics[index].clone()
            };
            messages.push(OutMessage {
                time: round_to_i64(note.start_pos as f64 / rate),
                event: OutEvent::Lyrics(encode_replace(&lyric, &options.lyric_encoding)?),
            });
        }
        let key_number = u8::try_from(note.key_number).map_err(|_| {
            LibreSvipError::msg(format!("note key out of range: {}", note.key_number))
        })?;
        messages.push(OutMessage {
            time: round_to_i64(note.start_pos as f64 / rate),
            event: OutEvent::NoteOn {
                note: key_number,
                velocity: 127,
            },
        });
        messages.push(OutMessage {
            time: round_to_i64(note.end_pos() as f64 / rate),
            event: OutEvent::NoteOff {
                note: key_number,
                velocity: 0,
            },
        });
    }
    if let Some(pitch_data) = generate_for_midi(
        first_bar_length,
        &track.edited_params.pitch,
        &track.note_list,
        time_signature_list,
        synchronizer,
    )? {
        for pbs_event in &pitch_data.pbs {
            let message_time = round_to_i64(pbs_event.tick as f64 / rate);
            messages.push(OutMessage {
                time: message_time,
                event: OutEvent::ControlChange {
                    control: 101,
                    value: 0,
                },
            });
            messages.push(OutMessage {
                time: message_time,
                event: OutEvent::ControlChange {
                    control: 100,
                    value: 0,
                },
            });
            messages.push(OutMessage {
                time: message_time,
                event: OutEvent::ControlChange {
                    control: 6,
                    value: u8::try_from(pbs_event.value).map_err(|_| {
                        LibreSvipError::msg(format!(
                            "pitch bend sensitivity out of range: {}",
                            pbs_event.value
                        ))
                    })?,
                },
            });
        }
        for pitch_event in &pitch_data.pit {
            messages.push(OutMessage {
                time: round_to_i64(pitch_event.tick as f64 / rate),
                event: OutEvent::PitchWheel {
                    pitch: i32::try_from(pitch_event.value).map_err(|_| {
                        LibreSvipError::msg(format!(
                            "pitch bend value out of range: {}",
                            pitch_event.value
                        ))
                    })?,
                },
            });
        }
    }
    messages.sort_by_key(|message| message.time);
    if messages.is_empty() {
        return Ok(None);
    }
    let end_time = messages.last().map(|message| message.time).unwrap_or(0);
    messages.push(OutMessage {
        time: end_time,
        event: OutEvent::EndOfTrack,
    });
    Ok(Some(messages))
}

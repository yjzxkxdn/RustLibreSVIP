//! Port of `libresvip.plugins.tssln.voisona_parser`.

use std::collections::HashSet;

use libresvip_core::core::tick_counter::{shift_beat_list, shift_tempo_list, skip_beat_list, skip_tempo_list};
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::Result;
use libresvip_core::model::base::{
    InstrumentalTrack, Note, Project, SingingTrack, SongTempo, TimeSignature, Track,
};
use libresvip_core::options::Options;

use super::constants::{OCTAVE_OFFSET, TICK_RATE};
use super::model::{VoiSonaProject, VoiSonaSingingTrackItem, VoiSonaTrackItem};
use super::pitch::{pitch_from_track, ParamEvent, TrackPitchData};

pub fn parse_project(voisona_project: &VoiSonaProject, options: &Options) -> Result<Project> {
    let mut time_signatures: Vec<TimeSignature> = Vec::new();
    let mut tempos: Vec<SongTempo> = Vec::new();
    let mut tracks: Vec<Track> = Vec::new();
    for track in &voisona_project.tracks {
        for item in &track.track {
            if let VoiSonaTrackItem::Singing(singing) = item {
                if let Some((singing_track, tempo_part, time_signature_part)) =
                    parse_singing_track(singing, options)?
                {
                    tracks.push(Track::Singing(singing_track));
                    tempos.extend(tempo_part);
                    time_signatures.extend(time_signature_part);
                }
            }
        }
    }
    let tempos = merge_tempos(tempos);
    let time_synchronizer = TimeSynchronizer::from_project(&tempos);
    if options.bool("import_instrumental_track", true) {
        for track in &voisona_project.tracks {
            for item in &track.track {
                if let VoiSonaTrackItem::Audio(audio) = item {
                    if let Some(events) = &audio.audio_event {
                        for (index, event) in events.iter().enumerate() {
                            tracks.push(Track::Instrumental(InstrumentalTrack {
                                title: format!("{} {}", audio.base.name, index + 1),
                                audio_file_path: event.path.clone(),
                                offset: time_synchronizer.get_actual_ticks_from_secs(event.offset)
                                    as i64,
                                ..Default::default()
                            }));
                        }
                    }
                }
            }
        }
    }
    let time_signatures = merge_time_signatures(time_signatures);
    Ok(Project {
        time_signature_list: skip_beat_list(&time_signatures, 0),
        song_tempo_list: skip_tempo_list(&tempos, 0),
        track_list: tracks,
        ..Default::default()
    })
}

fn merge_tempos(tempos: Vec<SongTempo>) -> Vec<SongTempo> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for tempo in tempos {
        if seen.insert(tempo.position) {
            result.push(tempo);
        }
    }
    if result.is_empty() {
        vec![SongTempo::default()]
    } else {
        result
    }
}

fn merge_time_signatures(time_signatures: Vec<TimeSignature>) -> Vec<TimeSignature> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for time_signature in time_signatures {
        if seen.insert(time_signature.bar_index) {
            result.push(time_signature);
        }
    }
    if result.is_empty() {
        vec![TimeSignature::default()]
    } else {
        result
    }
}

type ParsedTrack = (SingingTrack, Vec<SongTempo>, Vec<TimeSignature>);

fn parse_singing_track(
    track: &VoiSonaSingingTrackItem,
    options: &Options,
) -> Result<Option<ParsedTrack>> {
    let state_information = &track.plugin_data.state_information;
    let Some(songs) = &state_information.song else {
        return Ok(None);
    };
    let mut time_signatures = vec![TimeSignature::new(0, 4, 4)];
    let mut prev_tick: i64 = 0;
    let mut tempos: Vec<SongTempo> = Vec::new();
    let mut notes: Vec<Note> = Vec::new();

    let tick_prefix = time_signatures[0].bar_length() as i64;
    for song in songs {
        for beat in &song.beat {
            for time_node in &beat.time {
                let tick = (time_node.clock as f64 / TICK_RATE) as i64;
                let numerator = time_node.beats;
                let denominator = time_node.beat_type;

                let ticks_in_measure = time_signatures
                    .last()
                    .map(TimeSignature::bar_length)
                    .unwrap_or(0.0);
                let tick_diff = tick - prev_tick;
                let measure_diff = tick_diff as f64 / ticks_in_measure;
                let bar_index = time_signatures
                    .last()
                    .map(|beat| beat.bar_index)
                    .unwrap_or(0);
                time_signatures.push(TimeSignature::new(
                    (bar_index as f64 + measure_diff) as i64,
                    numerator,
                    denominator,
                ));
                prev_tick = tick;
            }
        }
        for tempo in &song.tempo {
            for tempo_node in &tempo.sound {
                let tick = (tempo_node.clock as f64 / TICK_RATE) as i64;
                tempos.push(SongTempo::new(tick, tempo_node.tempo));
            }
        }
        for score in &song.score {
            for note_node in &score.note {
                let pitch_octave = note_node.pitch_octave - OCTAVE_OFFSET;
                let phoneme = match &note_node.phoneme {
                    Some(phoneme) if !phoneme.is_empty() => {
                        Some(phoneme.replace(',', " "))
                    }
                    _ => None,
                };
                notes.push(Note {
                    key_number: note_node.pitch_step + pitch_octave * 12,
                    lyric: if note_node.lyric == "\u{30FC}" {
                        "-".to_owned()
                    } else {
                        note_node.lyric.clone()
                    },
                    start_pos: (note_node.clock as f64 / TICK_RATE).floor() as i64,
                    length: (note_node.duration as f64 / TICK_RATE).floor() as i64,
                    pronunciation: phoneme,
                    ..Default::default()
                });
            }
        }
    }
    let tempos = shift_tempo_list(&tempos, tick_prefix);
    let mut voisona_track_pitch_data: Option<TrackPitchData> = None;
    if let Some(parameters) = &state_information.parameter {
        for parameter in parameters {
            if let Some(log_f0) = &parameter.log_f0 {
                let pitch_data_nodes: Vec<&super::model::VoiSonaPointData> = log_f0
                    .iter()
                    .flat_map(|curve| curve.data.iter())
                    .collect();
                let vibrato_amplitude_nodes: Vec<&super::model::VoiSonaPointData> = parameter
                    .vib_amp
                    .as_ref()
                    .map(|curves| curves.iter().flat_map(|curve| curve.data.iter()).collect())
                    .unwrap_or_default();
                let vibrato_frequency_nodes: Vec<&super::model::VoiSonaPointData> = parameter
                    .vib_frq
                    .as_ref()
                    .map(|curves| curves.iter().flat_map(|curve| curve.data.iter()).collect())
                    .unwrap_or_default();
                let pitch_datas: Vec<ParamEvent> = pitch_data_nodes
                    .into_iter()
                    .map(parse_param_data)
                    .collect();
                let vibrato_amplitude_data: Vec<ParamEvent> = vibrato_amplitude_nodes
                    .into_iter()
                    .map(parse_param_data)
                    .collect();
                let vibrato_frequency_data: Vec<ParamEvent> = vibrato_frequency_nodes
                    .into_iter()
                    .map(parse_param_data)
                    .collect();
                voisona_track_pitch_data = Some(TrackPitchData {
                    events: pitch_datas,
                    tempos: tempos.clone(),
                    tick_prefix,
                    vibrato_amplitude_events: vibrato_amplitude_data,
                    vibrato_frequency_events: vibrato_frequency_data,
                });
            }
        }
    }
    let time_signatures = shift_beat_list(&time_signatures, 1);
    let mut singer_name = String::new();
    if let Some(voice_information) = &state_information.voice_information {
        if let Some(first) = voice_information.first() {
            singer_name = first.character_name.clone();
        }
    }
    let mut singing_track = SingingTrack {
        title: track.base.name.clone(),
        note_list: notes,
        ai_singer_name: singer_name,
        ..Default::default()
    };
    if options.bool("import_pitch", true) {
        if let Some(pitch_data) = &voisona_track_pitch_data {
            if let Some(pitch) = pitch_from_track(pitch_data)? {
                singing_track.edited_params.pitch = pitch;
            }
        }
    }
    Ok(Some((singing_track, tempos, time_signatures)))
}

fn parse_param_data(data_element: &super::model::VoiSonaPointData) -> ParamEvent {
    ParamEvent {
        idx: data_element.index.filter(|value| *value != 0),
        repeat: data_element.repeat.filter(|value| *value != 0),
        value: data_element.value,
    }
}

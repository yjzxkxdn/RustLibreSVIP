//! Port of `libresvip.plugins.aisp.aisingers_generator`.

use libresvip_core::core::constants::DEFAULT_PHONEME;
use libresvip_core::core::lyric_phoneme::chinese::{get_pinyin_series, is_chinese_char};
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{Note, Project, SingingTrack, SongTempo, TimeSignature, Track};
use libresvip_core::options::Options;

use super::model::{TRACK_AUDIO, TRACK_SING_VOICE};
use super::pitch::PitchSimulator;
use super::pyutil::{
    floordiv_f64, push_f64, push_i64, push_opt_f64, push_opt_i64, push_opt_str, push_str, py_round,
    pymod_f64, round_ndigits,
};
use libresvip_core::utils::audio::audio_track_info;

struct OutTempo {
    tempo_float: Option<f64>,
    start_128: i64,
    start_bar: i64,
    start_beat_in_bar: Option<i64>,
}

struct OutSignature {
    beat_zi: i64,
    beat_mu: i64,
    start_bar: i64,
}

struct OutNote {
    start: i64,
    length: i64,
    midi_no: i64,
    lyric: Option<String>,
    pinyin: Option<String>,
    vel: Option<i64>,
    triple: bool,
    pit: Vec<f64>,
    bc: Option<i64>,
    bj: Option<i64>,
    bq: Option<i64>,
}

struct OutSingPattern {
    uid: Option<i64>,
    start: i64,
    length: Option<i64>,
    notes: Vec<OutNote>,
}

struct OutAudioPattern {
    uid: Option<i64>,
    start: i64,
    length: Option<i64>,
    path_audio: Option<String>,
    path_wave: Option<String>,
    n_channel: Option<i64>,
    len_sec: Option<i64>,
}

enum OutPatterns {
    Sing(Vec<OutSingPattern>),
    Audio(Vec<OutAudioPattern>),
}

struct OutTrack {
    kind: i64,
    idx: Option<i64>,
    solo: bool,
    mute: bool,
    volume: f64,
    name: Option<String>,
    singer_namecn: Option<String>,
    singer_nameen: Option<String>,
    singer_head_path: Option<String>,
    patterns: OutPatterns,
}

pub fn generate_project(project: &Project, _options: &Options) -> Result<String> {
    let synchronizer = TimeSynchronizer::from_project(&project.song_tempo_list);
    let first_signature = project.time_signature_list.first().ok_or_else(|| {
        LibreSvipError::Validation("project has no time signature".to_owned())
    })?;
    let first_bar_length = py_round(first_signature.bar_length());
    let signatures: Vec<OutSignature> = project
        .time_signature_list
        .iter()
        .map(|signature| OutSignature {
            beat_zi: signature.numerator,
            beat_mu: signature.denominator,
            start_bar: signature.bar_index,
        })
        .collect();
    let tempos = generate_tempos(
        &project.song_tempo_list,
        &project.time_signature_list,
        first_bar_length,
    )?;
    if tempos.is_empty() {
        return Err(LibreSvipError::Validation(
            "project has no tempo".to_owned(),
        ));
    }
    let default_end = project
        .song_tempo_list
        .last()
        .map(|tempo| tempo.position)
        .unwrap_or(0);
    let max_end = project
        .singing_tracks()
        .flat_map(|track| track.note_list.iter())
        .map(Note::end_pos)
        .max()
        .unwrap_or(default_end);
    let max_end_time = py_round(max_end as f64 / 15.0);
    let last_tempo = tempos
        .last()
        .ok_or_else(|| LibreSvipError::Validation("project has no tempo".to_owned()))?;
    let last_signature = signatures
        .last()
        .ok_or_else(|| LibreSvipError::Validation("project has no time signature".to_owned()))?;
    let bars_per_num = 128.0 * last_signature.beat_zi as f64 / last_signature.beat_mu as f64;
    let num_bars = last_tempo.start_bar as f64
        + floordiv_f64(
            (max_end_time - last_tempo.start_128) as f64,
            bars_per_num,
        );
    let bar_value = num_bars.max(100.0);
    if !bar_value.is_finite() || bar_value.fract() != 0.0 {
        return Err(LibreSvipError::Validation(
            "invalid bar count".to_owned(),
        ));
    }
    let bar = bar_value as i64;

    let tracks = generate_tracks(project, &synchronizer, first_bar_length)?;

    let mut out = String::new();
    write_head(
        &mut out,
        &tempos,
        &signatures,
        max_end_time,
        bar,
    );
    out.push('\n');
    write_body(&mut out, &tracks);
    Ok(out)
}

fn generate_tempos(
    tempos: &[SongTempo],
    signatures: &[TimeSignature],
    first_bar_length: i64,
) -> Result<Vec<OutTempo>> {
    let mut previous_bar_length = 1920.0f64;
    let mut previous_bar_index: i64 = 1;
    let mut current_tick = 0.0f64;
    let mut tick_indexes: Vec<i64> = Vec::new();
    for (index, signature) in signatures.iter().enumerate() {
        if signature.bar_index > previous_bar_index && index > 0 {
            current_tick +=
                previous_bar_length * (signature.bar_index - previous_bar_index) as f64;
        }
        tick_indexes.push(current_tick as i64);
        if signature.bar_index > previous_bar_index {
            previous_bar_index = signature.bar_index;
            previous_bar_length = signature.bar_length();
        }
    }
    if tick_indexes.is_empty() {
        return Err(LibreSvipError::Validation(
            "project has no time signature".to_owned(),
        ));
    }
    let mut result = Vec::new();
    for tempo in tempos {
        let ts_index = tick_indexes
            .partition_point(|position| *position < tempo.position)
            .min(tick_indexes.len() - 1);
        let signature = &signatures[ts_index];
        let local = tempo.position as f64 - tick_indexes[ts_index] as f64 - first_bar_length as f64;
        let bar_length = signature.bar_length();
        let start_bar_value = (signature.bar_index as f64 + floordiv_f64(local, bar_length)).max(0.0);
        if !start_bar_value.is_finite() || start_bar_value.fract() != 0.0 {
            return Err(LibreSvipError::Validation(
                "invalid tempo bar index".to_owned(),
            ));
        }
        let beat_value = floordiv_f64(
            pymod_f64(local, bar_length),
            bar_length / signature.numerator as f64,
        );
        if !beat_value.is_finite() || beat_value.fract() != 0.0 {
            return Err(LibreSvipError::Validation(
                "invalid tempo beat index".to_owned(),
            ));
        }
        result.push(OutTempo {
            tempo_float: Some(tempo.bpm),
            start_128: py_round(tempo.position as f64 / 15.0),
            start_bar: start_bar_value as i64,
            start_beat_in_bar: Some(beat_value as i64),
        });
    }
    Ok(result)
}

fn generate_tracks(
    project: &Project,
    synchronizer: &TimeSynchronizer,
    first_bar_length: i64,
) -> Result<Vec<OutTrack>> {
    let total_tracks = project.track_list.len() as i64;
    let mut tracks: Vec<OutTrack> = Vec::new();
    for track in &project.track_list {
        match track {
            Track::Singing(singing) => {
                let notes = generate_notes(singing, synchronizer, first_bar_length)?;
                if notes.is_empty() {
                    continue;
                }
                let max_position = notes
                    .iter()
                    .map(|note| note.start + note.length)
                    .max()
                    .unwrap_or(0);
                let uid = Some(total_tracks + tracks.len() as i64);
                tracks.push(OutTrack {
                    kind: TRACK_SING_VOICE,
                    idx: Some(tracks.len() as i64),
                    solo: singing.solo,
                    mute: singing.mute,
                    volume: 0.0,
                    name: Some(singing.title.clone()),
                    singer_namecn: Some(singing.ai_singer_name.clone()),
                    singer_nameen: Some(String::new()),
                    singer_head_path: Some(String::new()),
                    patterns: OutPatterns::Sing(vec![OutSingPattern {
                        uid,
                        start: 0,
                        length: Some(max_position + first_bar_length),
                        notes,
                    }]),
                });
            }
            Track::Instrumental(instrumental) => {
                let Some(info) = audio_track_info(&instrumental.audio_file_path, true) else {
                    continue;
                };
                let offset_secs = info.duration;
                let end_tick =
                    synchronizer.get_actual_ticks_from_secs_offset(instrumental.offset, offset_secs);
                tracks.push(OutTrack {
                    kind: TRACK_AUDIO,
                    idx: Some(tracks.len() as i64),
                    solo: instrumental.solo,
                    mute: instrumental.mute,
                    volume: 0.0,
                    name: Some(instrumental.title.clone()),
                    singer_namecn: None,
                    singer_nameen: None,
                    singer_head_path: None,
                    patterns: OutPatterns::Audio(vec![OutAudioPattern {
                        uid: None,
                        start: instrumental.offset.div_euclid(15),
                        length: Some(
                            floordiv_f64(end_tick - instrumental.offset as f64, 15.0) as i64,
                        ),
                        path_audio: Some(instrumental.audio_file_path.clone()),
                        path_wave: Some(instrumental.audio_file_path.clone()),
                        n_channel: Some(i64::from(info.channels)),
                        len_sec: Some(offset_secs as i64),
                    }]),
                });
            }
        }
    }
    Ok(tracks)
}

fn generate_notes(
    track: &SingingTrack,
    synchronizer: &TimeSynchronizer,
    first_bar_length: i64,
) -> Result<Vec<OutNote>> {
    let mut notes = Vec::new();
    let mut pitch_simulator: Option<PitchSimulator> = None;
    for note in &track.note_list {
        let note_start = (note.start_pos as f64 / 15.0) as i64;
        let length = (note.end_pos() as f64 / 15.0) as i64 - note_start;
        let pinyin = match &note.pronunciation {
            Some(pronunciation) => pronunciation.clone(),
            None => {
                if is_single_chinese(&note.lyric) {
                    get_pinyin_series(&[note.lyric.as_str()])?.join(" ")
                } else if note.lyric.is_empty() {
                    DEFAULT_PHONEME.to_owned()
                } else {
                    note.lyric.clone()
                }
            }
        };
        if pitch_simulator.is_none() {
            let mut simulator = PitchSimulator::no_portamento(&track.note_list, synchronizer)?;
            simulator.merge_pitch_curve(
                &track.edited_params.pitch.points,
                first_bar_length,
                synchronizer,
            );
            pitch_simulator = Some(simulator);
        }
        let pit = match pitch_simulator.as_ref() {
            Some(simulator) => generate_pitch(simulator, note, synchronizer),
            None => Vec::new(),
        };
        notes.push(OutNote {
            start: note_start,
            length,
            midi_no: note.key_number - 12,
            lyric: Some(note.lyric.clone()),
            pinyin: Some(pinyin),
            vel: Some(50),
            triple: false,
            pit,
            bc: Some(0),
            bj: Some(0),
            bq: Some(0),
        });
    }
    Ok(notes)
}

fn is_single_chinese(lyric: &str) -> bool {
    let mut characters = lyric.chars();
    match (characters.next(), characters.next()) {
        (Some(character), None) => is_chinese_char(character),
        _ => false,
    }
}

fn generate_pitch(simulator: &PitchSimulator, note: &Note, synchronizer: &TimeSynchronizer) -> Vec<f64> {
    let tick_step = note.length as f64 / 500.0;
    let mut values = Vec::with_capacity(500);
    for index in 0..500 {
        let ticks = note.start_pos + (tick_step * index as f64) as i64;
        let base = note.key_number as f64 * 100.0;
        let pitch_value = simulator
            .pitch_at_ticks(ticks, synchronizer)
            .unwrap_or(base);
        values.push((pitch_value - base) / 10.0);
    }
    values
}

// ---------------------------------------------------------------------------
// JSON serialisation (`separators=(", ", ": ")`, ensure_ascii=True)
// ---------------------------------------------------------------------------

fn write_pit(out: &mut String, values: &[f64]) {
    out.push('"');
    let mut index = 0usize;
    let mut first = true;
    while index < values.len() {
        let value = values[index];
        let mut count = 1usize;
        while index + count < values.len() && values[index + count] == value {
            count += 1;
        }
        if !first {
            out.push(' ');
        }
        first = false;
        out.push_str(&super::pyutil::float_repr(round_ndigits(value, 2)));
        if count > 1 {
            out.push('x');
            out.push_str(&count.to_string());
        }
        index += count;
    }
    out.push('"');
}

fn write_head(
    out: &mut String,
    tempos: &[OutTempo],
    signatures: &[OutSignature],
    time: i64,
    bar: i64,
) {
    out.push_str("{\"tempo\": [");
    for (index, tempo) in tempos.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        out.push_str("{\"tempo_float\": ");
        push_opt_f64(out, tempo.tempo_float);
        out.push_str(", \"start_128\": ");
        push_i64(out, tempo.start_128);
        out.push_str(", \"start_bar\": ");
        push_i64(out, tempo.start_bar);
        out.push_str(", \"start_beat_in_bar\": ");
        push_opt_i64(out, tempo.start_beat_in_bar);
        out.push('}');
    }
    out.push_str("], \"signature\": [");
    for (index, signature) in signatures.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        out.push_str("{\"beat_zi\": ");
        push_i64(out, signature.beat_zi);
        out.push_str(", \"beat_mu\": ");
        push_i64(out, signature.beat_mu);
        out.push_str(", \"start_bar\": ");
        push_i64(out, signature.start_bar);
        out.push_str(", \"str\": ");
        let mut value = String::new();
        push_i64(&mut value, signature.beat_zi);
        value.push('/');
        push_i64(&mut value, signature.beat_mu);
        push_str(out, &value);
        out.push('}');
    }
    out.push_str("], \"flags\": ");
    push_opt_i64(out, Some(-256));
    out.push_str(", \"flage\": ");
    push_opt_i64(out, Some(-128));
    out.push_str(", \"time\": ");
    push_i64(out, time);
    out.push_str(", \"bar\": ");
    push_i64(out, bar);
    out.push('}');
}

fn write_body(out: &mut String, tracks: &[OutTrack]) {
    out.push_str("{\"tracks\": [");
    for (index, track) in tracks.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        out.push_str("{\"i\": ");
        push_opt_i64(out, track.idx);
        out.push_str(", \"s\": ");
        out.push_str(if track.solo { "true" } else { "false" });
        out.push_str(", \"m\": ");
        out.push_str(if track.mute { "true" } else { "false" });
        out.push_str(", \"v\": ");
        push_f64(out, track.volume);
        out.push_str(", \"n\": ");
        push_opt_str(out, track.name.as_deref());
        out.push_str(", \"t\": ");
        push_i64(out, track.kind);
        if track.kind == TRACK_SING_VOICE {
            out.push_str(", \"sn\": ");
            push_opt_str(out, track.singer_namecn.as_deref());
            out.push_str(", \"se\": ");
            push_opt_str(out, track.singer_nameen.as_deref());
            out.push_str(", \"sh\": ");
            push_opt_str(out, track.singer_head_path.as_deref());
        }
        out.push_str(", \"im\": [");
        match &track.patterns {
            OutPatterns::Sing(patterns) => {
                for (pattern_index, pattern) in patterns.iter().enumerate() {
                    if pattern_index > 0 {
                        out.push_str(", ");
                    }
                    out.push_str("{\"uid\": ");
                    push_opt_i64(out, pattern.uid);
                    out.push_str(", \"s\": ");
                    push_i64(out, pattern.start);
                    out.push_str(", \"l\": ");
                    push_opt_i64(out, pattern.length);
                    out.push_str(", \"n\": [");
                    for (note_index, note) in pattern.notes.iter().enumerate() {
                        if note_index > 0 {
                            out.push_str(", ");
                        }
                        write_note(out, note);
                    }
                    out.push_str("]}");
                }
            }
            OutPatterns::Audio(patterns) => {
                for (pattern_index, pattern) in patterns.iter().enumerate() {
                    if pattern_index > 0 {
                        out.push_str(", ");
                    }
                    out.push_str("{\"uid\": ");
                    push_opt_i64(out, pattern.uid);
                    out.push_str(", \"s\": ");
                    push_i64(out, pattern.start);
                    out.push_str(", \"l\": ");
                    push_opt_i64(out, pattern.length);
                    out.push_str(", \"pa\": ");
                    push_opt_str(out, pattern.path_audio.as_deref());
                    out.push_str(", \"pw\": ");
                    push_opt_str(out, pattern.path_wave.as_deref());
                    out.push_str(", \"n_channel\": ");
                    push_opt_i64(out, pattern.n_channel);
                    out.push_str(", \"len_sec\": ");
                    push_opt_i64(out, pattern.len_sec);
                    out.push('}');
                }
            }
        }
        out.push_str("]}");
    }
    out.push_str("], \"num_track\": ");
    push_i64(out, tracks.len() as i64);
    out.push('}');
}

fn write_note(out: &mut String, note: &OutNote) {
    out.push_str("{\"s\": ");
    push_i64(out, note.start);
    out.push_str(", \"l\": ");
    push_i64(out, note.length);
    out.push_str(", \"m\": ");
    push_i64(out, note.midi_no);
    out.push_str(", \"ly\": ");
    push_opt_str(out, note.lyric.as_deref());
    out.push_str(", \"py\": ");
    push_opt_str(out, note.pinyin.as_deref());
    out.push_str(", \"vel\": ");
    push_opt_i64(out, note.vel);
    out.push_str(", \"tri\": ");
    out.push_str(if note.triple { "true" } else { "false" });
    out.push_str(", \"pit\": ");
    write_pit(out, &note.pit);
    out.push_str(", \"bc\": ");
    push_opt_i64(out, note.bc);
    out.push_str(", \"bj\": ");
    push_opt_i64(out, note.bj);
    out.push_str(", \"bq\": ");
    push_opt_i64(out, note.bq);
    out.push('}');
}

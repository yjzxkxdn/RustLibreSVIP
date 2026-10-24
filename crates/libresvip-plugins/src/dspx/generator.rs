//! Port of `libresvip.plugins.dspx.dspx_generator` and `dspx_io.dump_model`.

use std::collections::BTreeMap;

use indexmap::IndexMap;
use libresvip_core::core::constants::{DEFAULT_BPM, TICKS_IN_BEAT};
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::model::base::{
    InstrumentalTrack, Note as LibreNote, Project, SingingTrack, SongTempo, TimeSignature, Track,
};
use libresvip_core::model::point::{Point, END_POINT_X, START_POINT_X};
use libresvip_core::options::Options;

use super::model::{
    AudioClip, BusControl, Clip, ClipTime, Content, DspxModel, Global, Master, Note, Param, Phonemes,
    Pronunciation, SingingClip, Tempo, Timeline, TimeSignature as DspxTimeSignature,
    Track as DspxTrack, TrackControl, INT32_MAX,
};
use super::param_utils::export_pitch_param;
use super::pyutil::{clamp, py_round, ratio_to_db};
use libresvip_core::utils::audio::audio_track_info;
use super::vibrato_utils::export_vibrato;

const VALID_DENOMINATORS: [i64; 8] = [1, 2, 4, 8, 16, 32, 64, 128];

fn clamp_int32(value: f64, lower: i64) -> i64 {
    py_round(clamp(py_round(value) as f64, lower as f64, INT32_MAX as f64))
}

fn linear_volume_to_db(volume: f64) -> f64 {
    if !volume.is_finite() {
        return 0.0;
    }
    if volume <= 0.0 {
        return -70.0;
    }
    let result = ratio_to_db(volume);
    if result.is_finite() {
        result
    } else {
        -70.0
    }
}

pub fn generate_project(project: &Project, options: &Options) -> DspxModel {
    let tempos = generate_tempos(&project.song_tempo_list);
    let time_signatures = generate_time_signatures(&project.time_signature_list);
    let first_bar_length = time_signatures
        .first()
        .map(|signature| {
            py_round(
                TimeSignature {
                    bar_index: signature.index,
                    numerator: signature.numerator,
                    denominator: signature.denominator,
                }
                .bar_length(),
            )
        })
        .unwrap_or(TICKS_IN_BEAT * 4);
    let synchronizer = TimeSynchronizer::from_project(
        &tempos
            .iter()
            .map(|tempo| SongTempo {
                position: tempo.pos,
                bpm: tempo.value,
            })
            .collect::<Vec<_>>(),
    );
    DspxModel {
        version: "1.0.0".to_owned(),
        content: Content {
            global: Global {
                author: String::new(),
                cent_shift: 0,
                editor_id: "libresvip".to_owned(),
                editor_name: "LibreSVIP".to_owned(),
                name: String::new(),
            },
            master: Master {
                control: BusControl {
                    gain: 0.0,
                    mute: false,
                    pan: 0.0,
                },
            },
            timeline: Timeline {
                labels: Vec::new(),
                tempos,
                time_signatures,
            },
            tracks: generate_tracks(project, &synchronizer, first_bar_length, options),
            workspace: Default::default(),
        },
    }
}

fn generate_tempos(tempos: &[SongTempo]) -> Vec<Tempo> {
    let mut normalized: BTreeMap<i64, Tempo> = BTreeMap::new();
    for tempo in tempos {
        if !tempo.bpm.is_finite() {
            continue;
        }
        let position = clamp_int32(tempo.position as f64, 0);
        normalized.insert(
            position,
            Tempo {
                pos: position,
                value: clamp(tempo.bpm, 10.0, 1000.0),
            },
        );
    }
    normalized.entry(0).or_insert(Tempo {
        pos: 0,
        value: DEFAULT_BPM,
    });
    normalized.into_values().collect()
}

fn generate_time_signatures(time_signatures: &[TimeSignature]) -> Vec<DspxTimeSignature> {
    let mut normalized: BTreeMap<i64, DspxTimeSignature> = BTreeMap::new();
    for time_signature in time_signatures {
        let position = clamp_int32(time_signature.bar_index as f64, 0);
        normalized.insert(
            position,
            DspxTimeSignature {
                index: position,
                numerator: clamp_int32(time_signature.numerator as f64, 1),
                denominator: if VALID_DENOMINATORS.contains(&time_signature.denominator) {
                    time_signature.denominator
                } else {
                    4
                },
            },
        );
    }
    normalized.entry(0).or_insert(DspxTimeSignature {
        index: 0,
        numerator: 4,
        denominator: 4,
    });
    normalized.into_values().collect()
}

fn generate_tracks(
    project: &Project,
    synchronizer: &TimeSynchronizer,
    first_bar_length: i64,
    options: &Options,
) -> Vec<DspxTrack> {
    let mut result = Vec::new();
    for track in &project.track_list {
        let clip = match track {
            Track::Instrumental(instrumental) => {
                Clip::Audio(generate_audio_clip(instrumental, synchronizer))
            }
            Track::Singing(singing) => Clip::Singing(generate_singing_clip(
                singing,
                first_bar_length,
                options,
            )),
        };
        result.push(DspxTrack {
            name: track.title().to_owned(),
            control: generate_track_control(track),
            clips: vec![clip],
            workspace: Default::default(),
        });
    }
    result
}

fn generate_track_control(track: &Track) -> TrackControl {
    TrackControl {
        control: BusControl {
            gain: linear_volume_to_db(track.volume()),
            mute: track.mute(),
            pan: clamp(if track.pan().is_finite() { track.pan() } else { 0.0 }, -1.0, 1.0),
        },
        solo: track.solo(),
    }
}

fn generate_audio_clip(track: &InstrumentalTrack, synchronizer: &TimeSynchronizer) -> AudioClip {
    let offset = py_round(track.offset as f64);
    let pos = clamp_int32(offset as f64, 0);
    let clip_start = clamp_int32((-offset) as f64, 0);
    let mut duration = 0i64;
    if let Some(info) = audio_track_info(&track.audio_file_path, false) {
        let actual_start = pos - clip_start;
        duration = clamp_int32(
            synchronizer.get_actual_ticks_from_secs_offset(actual_start, info.duration)
                - actual_start as f64,
            0,
        );
    }
    AudioClip {
        name: track.title.clone(),
        path: track.audio_file_path.clone(),
        time: ClipTime {
            pos,
            length: 0,
            clip_start,
            clip_len: (duration - clip_start).max(0),
        },
        control: BusControl {
            gain: 0.0,
            mute: false,
            pan: 0.0,
        },
        workspace: Default::default(),
    }
}

fn generate_singing_clip(
    track: &SingingTrack,
    first_bar_length: i64,
    options: &Options,
) -> SingingClip {
    let source_notes: Vec<&LibreNote> = track
        .note_list
        .iter()
        .filter(|note| note.length > 0 && !note.lyric.trim().is_empty())
        .collect();
    let origin = find_clip_origin(&source_notes, &track.edited_params.pitch.points, first_bar_length);
    let clip_start = clamp_int32((-origin) as f64, 0);
    let notes: Vec<Note> = source_notes
        .iter()
        .filter_map(|note| generate_note(note, origin, options))
        .collect();
    let mut params: IndexMap<String, Param> = IndexMap::new();
    if options.bool("export_pitch", true) {
        if let Some(pitch) =
            export_pitch_param(&track.edited_params.pitch, first_bar_length + origin)
        {
            params.insert("pitch".to_owned(), pitch);
        }
    }
    let mut max_end = 0i64;
    for note in &notes {
        max_end = max_end.max(origin + note.pos + note.length);
    }
    if let Some(pitch_end) = pitch_end(&track.edited_params.pitch.points, first_bar_length) {
        max_end = max_end.max(pitch_end);
    }
    SingingClip {
        name: track.title.clone(),
        time: ClipTime {
            pos: 0,
            length: 0,
            clip_start,
            clip_len: clamp_int32(max_end as f64, 0),
        },
        control: BusControl {
            gain: 0.0,
            mute: false,
            pan: 0.0,
        },
        notes,
        params,
        sources: None,
        workspace: Default::default(),
    }
}

fn find_clip_origin(notes: &[&LibreNote], pitch: &[Point], first_bar_length: i64) -> i64 {
    let mut positions: Vec<i64> = vec![0];
    positions.extend(notes.iter().map(|note| note.start_pos));
    positions.extend(
        pitch
            .iter()
            .filter(|point| point.x != START_POINT_X && point.x != END_POINT_X)
            .map(|point| point.x - first_bar_length),
    );
    let minimum = positions.into_iter().min().unwrap_or(0);
    (-INT32_MAX).max(minimum)
}

fn pitch_end(pitch: &[Point], first_bar_length: i64) -> Option<i64> {
    let positions: Vec<i64> = pitch
        .iter()
        .filter(|point| {
            point.x != START_POINT_X && point.x != END_POINT_X && point.y != -100
        })
        .map(|point| point.x - first_bar_length)
        .collect();
    positions.into_iter().max()
}

fn generate_note(note: &LibreNote, origin: i64, options: &Options) -> Option<Note> {
    let relative_pos = note.start_pos - origin;
    if relative_pos < 0 || relative_pos > INT32_MAX {
        return None;
    }
    Some(Note {
        pos: relative_pos,
        length: clamp_int32(note.length as f64, 1),
        key_num: py_round(clamp(note.key_number as f64, 0.0, 127.0)),
        cent_shift: 0,
        language: String::new(),
        lyric: note.lyric.clone(),
        pronunciation: Pronunciation::default(),
        phonemes: Phonemes::default(),
        vibrato: export_vibrato(
            note.vibrato.as_ref(),
            options.bool("preserve_vibrato", true),
        ),
        workspace: Default::default(),
    })
}

// ---------------------------------------------------------------------------
// `dspx_io.dump_model`
// ---------------------------------------------------------------------------

pub fn dump_model(model: &DspxModel) -> libresvip_core::Result<Vec<u8>> {
    // A "{}" fallback would silently write an empty project; serialization
    // failure must fail the conversion.
    let json = serde_json::to_string(model)?;
    // `ZSTD_compress` (one-shot), like Python's `zstd.compress`: the frame
    // records the decompressed size in its header. An uncompressed fallback
    // would produce a file the target app cannot open, so propagate errors.
    libresvip_core::compression::compress(json.as_bytes(), 3)
}

//! Port of `libresvip.plugins.ace.ace_generator`.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use libresvip_core::core::constants::{DEFAULT_BPM, TICKS_IN_BEAT};
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::model::base::{
    InstrumentalTrack, Note, Project, SingingTrack, SongTempo, Track,
};
use libresvip_core::options::Options;

use super::model::{
    AceBgmInfo, AceBgmTrack, AceDebugInfo, AceNote, AcePitchBend, AceProject, AceRoleInfo,
    AceSongInfo, AceTimedItem, AceTrack,
};
use super::pyutil::{first_pinyin, md5_hex, py_round};
use libresvip_core::utils::audio::audio_track_info;

pub const DEFAULT_SCALE: [i64; 15] = [84, 83, 81, 79, 77, 76, 74, 72, 71, 69, 67, 65, 64, 62, 60];

pub fn generate_project(project: &Project, options: &Options) -> AceProject {
    let tempos = if project.song_tempo_list.is_empty() {
        vec![SongTempo {
            position: 0,
            bpm: DEFAULT_BPM,
        }]
    } else {
        project.song_tempo_list.clone()
    };
    let synchronizer = TimeSynchronizer::from_project(&tempos);
    let first_bar_length = project
        .time_signature_list
        .first()
        .map(|signature| py_round(signature.bar_length()))
        .unwrap_or(TICKS_IN_BEAT * 4);
    let singing_tracks: Vec<AceTrack> = project
        .track_list
        .iter()
        .filter_map(Track::as_singing)
        .map(|track| generate_track(track, options, &synchronizer, first_bar_length))
        .collect();
    let instrumental_tracks: Vec<&InstrumentalTrack> = project
        .track_list
        .iter()
        .filter_map(Track::as_instrumental)
        .collect();
    let duration = project
        .track_list
        .iter()
        .filter_map(Track::as_singing)
        .flat_map(|track| track.note_list.iter())
        .map(|note| synchronizer.get_actual_secs_from_ticks(note.end_pos()))
        .fold(None::<f64>, |maximum, value| {
            Some(maximum.map_or(value, |maximum| maximum.max(value)))
        })
        .unwrap_or(0.0);
    let first_signature = project.time_signature_list.first();
    let song_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0);
    AceProject {
        version: 2,
        debug_info: Some(AceDebugInfo::default()),
        song_info: AceSongInfo {
            author: options.str("author", ""),
            beat_of_bar: first_signature.map(|signature| signature.numerator).unwrap_or(4),
            bpm: tempos[0].bpm,
            duration,
            first_beat_offset: first_note_time(project, &synchronizer),
            key: options.str("key", "C"),
            name: options.str("song_name", "New Project"),
            operate_scale: Some(DEFAULT_SCALE.to_vec()),
            scale: DEFAULT_SCALE.to_vec(),
            segment_of_beat: 4,
            song_id,
            ..Default::default()
        },
        bgm_info: generate_bgm_info(&instrumental_tracks, &synchronizer),
        tracks: singing_tracks,
        ..Default::default()
    }
}

fn generate_track(
    track: &SingingTrack,
    options: &Options,
    synchronizer: &TimeSynchronizer,
    first_bar_length: i64,
) -> AceTrack {
    let role_name = if !track.ai_singer_name.is_empty() {
        track.ai_singer_name.clone()
    } else if !track.title.is_empty() {
        track.title.clone()
    } else {
        options.str("role_name", "")
    };
    let notes: Vec<AceNote> = track
        .note_list
        .iter()
        .filter(|note| note.length > 0)
        .map(|note| generate_note(note, track, options, synchronizer, first_bar_length))
        .collect();
    let lyric = notes.iter().map(|note| note.word.as_str()).collect::<String>();
    AceTrack {
        front: true,
        lyric,
        mute: track.mute,
        notes,
        pan: track.pan,
        role_info: AceRoleInfo {
            name: role_name,
            role_id: options.int("role_id", 0),
        },
        singer_volume: track.volume,
        solo: track.solo,
        ..Default::default()
    }
}

fn generate_note(
    note: &Note,
    track: &SingingTrack,
    options: &Options,
    synchronizer: &TimeSynchronizer,
    first_bar_length: i64,
) -> AceNote {
    let lyric = if note.lyric.is_empty() {
        "la".to_owned()
    } else {
        note.lyric.clone()
    };
    let pinyin = match &note.pronunciation {
        Some(pronunciation) if !pronunciation.is_empty() => pronunciation.clone(),
        _ => generate_pronunciation(&lyric),
    };
    let start_time = synchronizer.get_actual_secs_from_ticks(note.start_pos);
    let end_time = synchronizer.get_actual_secs_from_ticks(note.end_pos());
    let mut pitch_bends = Vec::new();
    if options.bool("export_pitch", true) {
        for point in &track.edited_params.pitch.points {
            let tick = point.x - first_bar_length;
            if note.start_pos <= tick && tick <= note.end_pos() && point.y != -100 {
                pitch_bends.push(AcePitchBend {
                    time: synchronizer.get_actual_secs_from_ticks(tick),
                    pitch: point.y as f64 / 100.0 - note.key_number as f64,
                });
            }
        }
    }
    AceNote {
        base: AceTimedItem {
            start_time,
            end_time,
            ..Default::default()
        },
        br: note.head_tag.as_deref() == Some("V"),
        config: Some(String::new()),
        consonant_time_abs: note
            .edited_phones
            .as_ref()
            .map(|phones| phones.head_length_in_secs),
        key: Some(options.str("key", "C")),
        pinyin,
        pitch: note.key_number + 12,
        pitch_bends,
        scale: Some(DEFAULT_SCALE.to_vec()),
        word: lyric,
        ..Default::default()
    }
}

fn generate_pronunciation(lyric: &str) -> String {
    first_pinyin(lyric)
        .filter(|pronunciation| !pronunciation.is_empty())
        .unwrap_or_else(|| lyric.to_owned())
}

fn generate_bgm_info(
    tracks: &[&InstrumentalTrack],
    synchronizer: &TimeSynchronizer,
) -> AceBgmInfo {
    let mut bgm_tracks = Vec::new();
    for track in tracks {
        let path = Path::new(&track.audio_file_path);
        if !path.is_file() {
            continue;
        }
        let start_time = synchronizer.get_actual_secs_from_ticks(track.offset);
        let duration = audio_track_info(&track.audio_file_path, false)
            .map(|info| info.duration)
            .unwrap_or(0.0);
        let md5 = std::fs::read(path).map(|contents| md5_hex(&contents)).unwrap_or_default();
        bgm_tracks.push(AceBgmTrack {
            bpm: None,
            end_time: start_time + duration,
            file_md5: md5,
            file_name: path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default(),
            file_type: path
                .extension()
                .map(|extension| extension.to_string_lossy().to_lowercase())
                .unwrap_or_default(),
            position: start_time,
            start_time,
        });
    }
    let first_track = tracks.first().copied();
    AceBgmInfo {
        tracks: bgm_tracks,
        bgm_volume: first_track.map(|track| track.volume).unwrap_or(1.0),
        mute: first_track.map(|track| track.mute).unwrap_or(false),
        solo: first_track.map(|track| track.solo).unwrap_or(false),
    }
}

fn first_note_time(project: &Project, synchronizer: &TimeSynchronizer) -> f64 {
    let first_pos = project
        .track_list
        .iter()
        .filter_map(Track::as_singing)
        .flat_map(|track| track.note_list.iter())
        .map(|note| note.start_pos)
        .min()
        .unwrap_or(0);
    synchronizer.get_actual_secs_from_ticks(first_pos)
}

/// `json.dumps(..., indent=2)` (Python's pretty layout matches serde_json's).
pub fn dump_indented(project: &AceProject) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(project)
}

/// `json.dumps(..., separators=(",", ":"))`.
pub fn dump_compact(project: &AceProject) -> Result<String, serde_json::Error> {
    serde_json::to_string(project)
}

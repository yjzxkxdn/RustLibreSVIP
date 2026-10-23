//! Port of `libresvip.plugins.ps_project.pocket_singer_generator`.

use std::path::{Path, PathBuf};

use libresvip_core::core::constants::{DEFAULT_CHINESE_LYRIC, TICKS_IN_BEAT};
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{InstrumentalTrack, Note, Params, Project, SongTempo, Track};
use libresvip_core::options::Options;

use super::model::{
    PocketSingerBgmInfo, PocketSingerBgmTrack, PocketSingerMetadata, PocketSingerNote,
    PocketSingerPitchBend, PocketSingerProject, PocketSingerSongInfo, PocketSingerTrack,
};
use super::zip_aes::content_md5;
use crate::ppsf::audio::audio_track_info;

pub struct PocketSingerGenerator<'a> {
    options: &'a Options,
    pub buffer: Vec<u8>,
    pub audio_paths: Vec<(String, PathBuf)>,
    synchronizer: TimeSynchronizer,
    first_bar_length: i64,
}

impl<'a> PocketSingerGenerator<'a> {
    pub fn new(options: &'a Options) -> Self {
        Self {
            options,
            buffer: Vec::new(),
            audio_paths: Vec::new(),
            synchronizer: TimeSynchronizer::from_project(&[SongTempo::default()]),
            first_bar_length: 0,
        }
    }

    pub fn generate_project(&mut self, project: &Project) -> Result<PocketSingerMetadata> {
        self.synchronizer = TimeSynchronizer::from_project(&project.song_tempo_list);
        self.first_bar_length = TICKS_IN_BEAT * 4;
        let first_tempo = project
            .song_tempo_list
            .first()
            .cloned()
            .ok_or_else(|| LibreSvipError::Validation("song tempo list is empty".to_owned()))?;
        let song_info = self.generate_song_info(&first_tempo);
        let mut ps_project = PocketSingerProject {
            song_info,
            ..Default::default()
        };
        let first_instrumental_track = project.track_list.iter().find_map(|track| match track {
            Track::Instrumental(track) => Some(track),
            Track::Singing(_) => None,
        });
        if let Some(instrumental_track) = first_instrumental_track {
            if let Some(bgm_info) = self.generate_bgm_info(instrumental_track) {
                ps_project.bgm_info = bgm_info;
            }
        }
        for track in &project.track_list {
            if let Track::Singing(singing_track) = track {
                if let Some(ps_track) = self.generate_track(singing_track) {
                    ps_project.tracks.push(ps_track);
                }
            }
        }
        self.buffer = ps_project.to_json()?.into_bytes();
        Ok(PocketSingerMetadata {
            ace_file_name: "ps_structure.ps".to_owned(),
            ..Default::default()
        })
    }

    fn generate_song_info(&self, tempo: &SongTempo) -> PocketSingerSongInfo {
        PocketSingerSongInfo {
            start: 0,
            first_beat_offset: 0.0,
            scale: vec![
                80, 79, 77, 76, 75, 74, 73, 72, 71, 70, 69, 68, 67, 66, 65, 64, 63, 62, 61, 60,
                59, 58, 57, 56, 55, 54, 53, 52, 51, 50, 49, 48, 43,
            ],
            key: "C".to_owned(),
            segment_of_beat: 2,
            operate_scale: vec![72, 71, 69, 67, 65, 64, 62, 60, 59, 57, 55],
            bpm: tempo.bpm,
            name: self.options.str("title", "New Project"),
            duration: 0.0,
            beat_of_bar: 4,
            ..Default::default()
        }
    }

    fn generate_bgm_info(
        &mut self,
        instrumental_track: &InstrumentalTrack,
    ) -> Option<PocketSingerBgmInfo> {
        let audio_path = Path::new(&instrumental_track.audio_file_path);
        let file_type = audio_path
            .extension()
            .map(|extension| extension.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if !matches!(file_type.as_str(), "wav" | "mp3" | "flac" | "aac" | "m4a") {
            return None;
        }
        let track_info = audio_track_info(&instrumental_track.audio_file_path, false)?;
        let audio_content = std::fs::read(audio_path).ok()?;
        let audio_name = audio_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.audio_paths.push((audio_name, audio_path.to_path_buf()));
        let audio_position = self
            .synchronizer
            .get_actual_secs_from_ticks(instrumental_track.offset);
        Some(PocketSingerBgmInfo {
            mute: instrumental_track.mute,
            solo: instrumental_track.solo,
            tracks: vec![PocketSingerBgmTrack {
                file_name: audio_path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                file_type,
                file_md5: content_md5(&audio_content),
                position: audio_position,
                start_time: audio_position,
                end_time: audio_position + track_info.duration,
            }],
            ..Default::default()
        })
    }

    fn generate_track(&self, track: &libresvip_core::model::base::SingingTrack) -> Option<PocketSingerTrack> {
        let mut ps_track = PocketSingerTrack {
            language: self.options.str("lyric_language", "ch"),
            mute: track.mute,
            solo: track.solo,
            pan: track.pan,
            sound_effect: 0,
            singer_volume: 1.0,
            ..Default::default()
        };
        let patched_notes = patch_notes(&track.note_list);
        if patched_notes.is_empty() {
            return None;
        }
        let (ps_notes, lyric) = self.generate_notes(&patched_notes, &track.edited_params);
        ps_track.notes = ps_notes;
        ps_track.lyric = lyric;
        Some(ps_track)
    }

    fn generate_notes(
        &self,
        notes: &[Note],
        params: &Params,
    ) -> (Vec<PocketSingerNote>, String) {
        let mut ps_notes = Vec::new();
        let mut lyrics = Vec::new();
        for note_group in split_when(notes, |_previous, note| note.lyric != "+") {
            let Some(first_note) = note_group.first() else {
                continue;
            };
            let group_lyric = first_note.lyric.clone();
            let group_len = note_group.len();
            for (index, note) in note_group.iter().enumerate() {
                let mut ps_note = PocketSingerNote {
                    language: self.options.str("lyric_language", "ch"),
                    grapheme_count: group_len as i64,
                    grapheme_index: index as i64,
                    grapheme: group_lyric.clone(),
                    pitch: note.key_number,
                    start_time: self.synchronizer.get_actual_secs_from_ticks(note.start_pos),
                    end_time: self.synchronizer.get_actual_secs_from_ticks(note.end_pos()),
                    ..Default::default()
                };
                if self.options.str("lyric_language", "ch") != "en"
                    && group_lyric.chars().count() == 1
                {
                    lyrics.push(group_lyric.clone());
                } else {
                    lyrics.push(DEFAULT_CHINESE_LYRIC.to_owned());
                }
                if let Some(edited_phones) = &note.edited_phones {
                    ps_note.consonant_time_head =
                        Some(vec![edited_phones.head_length_in_secs]);
                }
                for point in &params.pitch.points {
                    if note.start_pos <= point.x - self.first_bar_length
                        && point.x - self.first_bar_length <= note.end_pos()
                    {
                        ps_note.pitch_bends.push(PocketSingerPitchBend {
                            time: self.synchronizer.get_actual_secs_from_ticks(
                                point.x - self.first_bar_length,
                            ),
                            pitch: if point.y != -100 {
                                point.y as f64 / 100.0 - note.key_number as f64
                            } else {
                                0.0
                            },
                        });
                    }
                }
                ps_notes.push(ps_note);
            }
        }
        (ps_notes, lyrics.concat())
    }
}

/// `more_itertools.split_when(iterable, pred)` - starts a new group whenever
/// `pred(previous, current)` is true.
fn split_when<'a, T>(
    items: &'a [T],
    predicate: impl Fn(&T, &T) -> bool,
) -> Vec<&'a [T]> {
    let mut groups: Vec<&[T]> = Vec::new();
    let mut start = 0usize;
    for index in 1..items.len() {
        if predicate(&items[index - 1], &items[index]) {
            groups.push(&items[start..index]);
            start = index;
        }
    }
    if start < items.len() {
        groups.push(&items[start..]);
    }
    groups
}

/// `patch_notes`: merges legato notes into the previous note.
pub fn patch_notes(notes: &[Note]) -> Vec<Note> {
    let mut patched_notes: Vec<Note> = Vec::new();
    let mut prev_index: Option<usize> = None;
    for note in notes {
        if note.lyric.is_empty() {
            // The Python loop `continue`s here, so `prev_note` is unchanged.
            continue;
        }
        let is_legato = note.lyric == "-" || note.lyric == "+";
        if is_legato && prev_index.is_some() {
            let prev = prev_index.unwrap_or(0);
            let prev_end = patched_notes[prev].end_pos();
            if note.start_pos == prev_end {
                if note.lyric == "-" {
                    patched_notes[prev].length += note.length;
                    continue;
                }
                let mut new_note = note.clone();
                new_note.key_number = patched_notes[prev].key_number;
                patched_notes.push(new_note);
                prev_index = Some(patched_notes.len() - 1);
            } else {
                prev_index = None;
                continue;
            }
        } else {
            patched_notes.push(note.clone());
            prev_index = Some(patched_notes.len() - 1);
        }
    }
    patched_notes
}

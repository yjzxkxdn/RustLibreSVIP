//! Port of `libresvip.plugins.dv.dv_generator`.

use libresvip_core::core::lyric_phoneme::chinese::{contains_chinese, get_pinyin_series};
use libresvip_core::core::tick_counter::{shift_beat_list, shift_tempo_list};
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{
    InstrumentalTrack, Note, Project, SingingTrack, SongTempo, TimeSignature, Track,
};
use libresvip_core::utils::audio::audio_track_info;

use super::constants::{DEFAULT_VOLUME, MIN_SEGMENT_LENGTH, NOTE_UNKNOWN_DATA_BLOCK};
use super::model::{
    DvAudioInfo, DvAudioTrack, DvInnerProject, DvNote, DvNoteParameter, DvPhoneme, DvPoint,
    DvProject, DvSegment, DvSingingTrack, DvTempo, DvTimeSignature, DvTrackRecord,
};
use super::pitch::{convert_note_key_int, generate_for_dv};

fn py_round(value: f64) -> i64 {
    value.round_ties_even() as i64
}

pub struct DeepVocalGenerator;

impl DeepVocalGenerator {
    /// `generate_project`.
    pub fn generate_project(&self, project: &Project) -> Result<DvProject> {
        let first_signature = project
            .time_signature_list
            .first()
            .ok_or_else(|| LibreSvipError::msg("list index out of range"))?;
        let first_bar_length = py_round(first_signature.bar_length());
        let tick_prefix = py_round(4.0 * first_signature.bar_length());
        let time_synchronizer = TimeSynchronizer::from_project(&project.song_tempo_list);
        let singing_tracks: Vec<&SingingTrack> = project.singing_tracks().collect();
        let audio_tracks: Vec<&InstrumentalTrack> = project
            .track_list
            .iter()
            .filter_map(Track::as_instrumental)
            .collect();
        let mut tracks = self.generate_singing_tracks(&singing_tracks, tick_prefix, first_bar_length)?;
        tracks.extend(self.generate_instrumental_tracks(
            &audio_tracks,
            tick_prefix,
            &time_synchronizer,
        ));
        Ok(DvProject {
            version: 5,
            inner_project: DvInnerProject {
                features: [true; 7],
                tracks,
                tempos: self.generate_tempos(&project.song_tempo_list, tick_prefix, first_bar_length),
                time_signatures: Self::generate_time_signatures(&project.time_signature_list),
            },
        })
    }

    /// `generate_time_signatures`.
    fn generate_time_signatures(time_signatures: &[TimeSignature]) -> Vec<DvTimeSignature> {
        let adjusted: Vec<TimeSignature> = time_signatures
            .iter()
            .enumerate()
            .map(|(index, signature)| {
                let mut value = signature.clone();
                if index == 0 {
                    value.bar_index = -3;
                }
                value
            })
            .collect();
        shift_beat_list(&adjusted, 1)
            .into_iter()
            .map(|signature| DvTimeSignature {
                measure_position: signature.bar_index as i32,
                numerator: signature.numerator as u32,
                denominator: signature.denominator as u32,
            })
            .collect()
    }

    /// `generate_tempos`.
    fn generate_tempos(
        &self,
        tempos: &[SongTempo],
        tick_prefix: i64,
        first_bar_length: i64,
    ) -> Vec<DvTempo> {
        let adjusted: Vec<SongTempo> = tempos
            .iter()
            .enumerate()
            .map(|(index, tempo)| {
                let mut value = tempo.clone();
                if index == 0 {
                    value.position = 0;
                }
                value
            })
            .collect();
        shift_tempo_list(&adjusted, tick_prefix - first_bar_length)
            .into_iter()
            .map(|tempo| DvTempo {
                position: tempo.position as i32,
                bpm: py_round(tempo.bpm * 100.0) as u32,
            })
            .collect()
    }

    /// `generate_instrumental_tracks`.
    fn generate_instrumental_tracks(
        &self,
        instrumental_tracks: &[&InstrumentalTrack],
        tick_prefix: i64,
        time_synchronizer: &TimeSynchronizer,
    ) -> Vec<DvTrackRecord> {
        let mut track_list = Vec::new();
        for track in instrumental_tracks {
            if let Some(track_info) = audio_track_info(&track.audio_file_path, false) {
                let audio_info = DvAudioInfo {
                    path: track.audio_file_path.clone(),
                    name: track.title.clone(),
                    start: (track.offset + tick_prefix) as u32,
                    length: py_round(
                        time_synchronizer
                            .get_actual_ticks_from_secs_offset(track.offset, track_info.duration),
                    ) as u32,
                };
                let dv_track = DvAudioTrack {
                    name: track.title.clone(),
                    mute: u8::from(track.mute),
                    solo: u8::from(track.solo),
                    volume: DEFAULT_VOLUME,
                    balance: 0,
                    infos: vec![audio_info],
                };
                track_list.push(DvTrackRecord::audio(dv_track));
            }
        }
        track_list
    }

    /// `generate_singing_tracks`.
    fn generate_singing_tracks(
        &self,
        singing_tracks: &[&SingingTrack],
        tick_prefix: i64,
        first_bar_length: i64,
    ) -> Result<Vec<DvTrackRecord>> {
        let mut track_list = Vec::new();
        for track in singing_tracks {
            let dv_notes = self.generate_notes(&track.note_list)?;
            let mut dv_segment = DvSegment {
                start: tick_prefix as u32,
                name: track.title.clone(),
                singer_name: track.ai_singer_name.clone(),
                length: track
                    .note_list
                    .iter()
                    .map(Note::end_pos)
                    .max()
                    .unwrap_or(MIN_SEGMENT_LENGTH) as u32,
                notes: dv_notes,
                volume_data: vec![
                    DvPoint { x: -1, y: 128 },
                    DvPoint { x: 307201, y: 128 },
                ],
                pitch_data: vec![
                    DvPoint { x: -1, y: -1 },
                    DvPoint { x: 307201, y: -1 },
                ],
                breath_data: vec![
                    DvPoint { x: -1, y: 128 },
                    DvPoint { x: 307201, y: 128 },
                ],
                ext3_data: Some(vec![
                    DvPoint { x: -1, y: 128 },
                    DvPoint { x: 307201, y: 128 },
                ]),
                ext5_data: Some(vec![
                    DvPoint { x: -1, y: 128 },
                    DvPoint { x: 307201, y: 128 },
                ]),
                ext6_data: Some(vec![
                    DvPoint { x: -1, y: 128 },
                    DvPoint { x: 307201, y: 128 },
                ]),
                ext7_data: Some(vec![
                    DvPoint { x: -1, y: 128 },
                    DvPoint { x: 307201, y: 128 },
                ]),
            };
            if let Some(raw_data) = generate_for_dv(first_bar_length, &track.edited_params.pitch) {
                dv_segment.pitch_data = raw_data.data;
            }
            let dv_track = DvSingingTrack {
                name: track.title.clone(),
                mute: u8::from(track.mute),
                solo: u8::from(track.solo),
                volume: DEFAULT_VOLUME,
                balance: 0,
                segments: vec![dv_segment],
            };
            track_list.push(DvTrackRecord::singing(dv_track));
        }
        Ok(track_list)
    }

    /// `generate_notes`.
    fn generate_notes(&self, notes: &[Note]) -> Result<Vec<DvNote>> {
        let mut result = Vec::with_capacity(notes.len());
        for note in notes {
            let phoneme = match note
                .pronunciation
                .as_deref()
                .filter(|value| !value.is_empty())
            {
                Some(pronunciation) => pronunciation.to_owned(),
                None => {
                    if contains_chinese(&note.lyric) {
                        let series: Vec<String> =
                            note.lyric.chars().map(|c| c.to_string()).collect();
                        let parts: Vec<&str> = series.iter().map(String::as_str).collect();
                        get_pinyin_series(&parts)?.first().cloned().unwrap_or_default()
                    } else {
                        note.lyric.clone()
                    }
                }
            };
            result.push(DvNote {
                start: note.start_pos as i32,
                length: note.length as u32,
                key: convert_note_key_int(note.key_number) as u32,
                vibrato: 50,
                phoneme,
                word: note.lyric.clone(),
                padding_1: 0,
                note_vibrato_data: DvNoteParameter {
                    amplitude_points: vec![
                        DvPoint { x: -1, y: 0 },
                        DvPoint { x: 100001, y: 0 },
                    ],
                    frequency_points: vec![
                        DvPoint { x: -1, y: 0 },
                        DvPoint { x: 100001, y: 0 },
                    ],
                    vibrato_points: vec![DvPoint { x: 0, y: 0 }, DvPoint { x: 1124, y: 0 }],
                },
                unknown: NOTE_UNKNOWN_DATA_BLOCK.to_vec(),
                phonemes: Some(DvPhoneme {
                    unknown_1: 0,
                    consonant_rate: 1.0,
                    vowel_modified: 0,
                    medial: 1.0,
                    rime: 1.0,
                    ending: 1.0,
                }),
                ben_depth: Some(0),
                ben_length: Some(0),
                por_tail: Some(0),
                por_head: Some(0),
                timbre: Some(-1),
                cross_lyric: Some(String::new()),
                cross_timbre: Some(-1),
            });
        }
        Ok(result)
    }
}

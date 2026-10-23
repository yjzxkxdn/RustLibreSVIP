//! Port of `libresvip.plugins.vxf.vx_beta_generator`.

use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{
    Project, SingingTrack, SongTempo, TimeSignature, Track,
};

use super::binary::{VxEvent, VxFile, VxTrack};
use super::model::{VxPitchData, VxPitchPoint, VxTimeBasedPitchSequence};
use super::music::py_round;

#[derive(Clone, Debug)]
pub struct OutputOptions {
    pub ticks_per_beat: i64,
}

impl Default for OutputOptions {
    fn default() -> Self {
        Self {
            ticks_per_beat: libresvip_core::core::constants::TICKS_IN_BEAT,
        }
    }
}

struct Generator {
    options: OutputOptions,
    synchronizer: TimeSynchronizer,
    first_bar_length: i64,
}

pub fn generate_project(project: &Project, options: &OutputOptions) -> Result<VxFile> {
    if options.ticks_per_beat == 0 {
        return Err(LibreSvipError::Validation(
            "ticks per beat must not be zero".to_owned(),
        ));
    }
    let Some(first_signature) = project.time_signature_list.first() else {
        return Err(LibreSvipError::Validation(
            "project has no time signatures".to_owned(),
        ));
    };
    let first_bar_length = py_round(first_signature.bar_length_with(options.ticks_per_beat));
    let mut generator = Generator {
        options: options.clone(),
        synchronizer: TimeSynchronizer::from_project(&project.song_tempo_list),
        first_bar_length,
    };
    let mut master_track_events = generator.generate_tempos(&project.song_tempo_list);
    master_track_events.extend(generator.generate_time_signatures(&project.time_signature_list));
    let mut tracks = generator.generate_tracks(&project.track_list)?;
    if let Some(first_track) = tracks.first_mut() {
        first_track.events.extend(master_track_events);
    }
    Generator::convert_cumulative_to_delta(&mut tracks);
    Ok(VxFile {
        ticks_per_beat: options.ticks_per_beat,
        tracks,
    })
}

impl Generator {
    fn tick_rate(&self) -> f64 {
        libresvip_core::core::constants::TICKS_IN_BEAT as f64 / self.options.ticks_per_beat as f64
    }

    fn convert_cumulative_to_delta(tracks: &mut [VxTrack]) {
        for track in tracks {
            track.events.sort_by_key(VxEvent::time);
            let mut tick = 0i64;
            for event in &mut track.events {
                let time = event.time_mut();
                let original = *time;
                *time = original - tick;
                tick = original;
            }
        }
    }

    fn generate_tempos(&self, song_tempo_list: &[SongTempo]) -> Vec<VxEvent> {
        song_tempo_list
            .iter()
            .filter(|tempo| tempo.position >= 0)
            .map(|tempo| VxEvent::Tempo {
                tempo: tempo.bpm,
                time: py_round(tempo.position as f64 / self.tick_rate()),
            })
            .collect()
    }

    fn generate_time_signatures(
        &self,
        time_signature_list: &[TimeSignature],
    ) -> Vec<VxEvent> {
        let mut events = Vec::new();
        let mut prev_ticks = 0i64;
        let mut prev_time_signature: Option<&TimeSignature> = None;
        for time_signature in time_signature_list {
            if time_signature.bar_index >= 0 {
                if let Some(previous) = prev_time_signature {
                    prev_ticks += py_round(
                        (time_signature.bar_index - previous.bar_index) as f64
                            * previous.bar_length_with(self.options.ticks_per_beat),
                    );
                }
                events.push(VxEvent::TimeSignature {
                    numerator: time_signature.numerator,
                    denominator: time_signature.denominator,
                    time: prev_ticks,
                });
                prev_time_signature = Some(time_signature);
            }
        }
        events
    }

    fn generate_tracks(&mut self, tracks: &[Track]) -> Result<Vec<VxTrack>> {
        let mut result = Vec::new();
        for track in tracks {
            if let Track::Singing(track) = track {
                if let Some(generated) = self.generate_track(track)? {
                    result.push(generated);
                }
            }
        }
        Ok(result)
    }

    fn generate_track(&self, track: &SingingTrack) -> Result<Option<VxTrack>> {
        let encoded_title = track.title.as_bytes();
        let mut title_parts: Vec<(u8, String)> = Vec::new();
        if !encoded_title.is_empty() {
            if encoded_title.len() <= 12 {
                title_parts.push((0x10, utf8_chunk(encoded_title)?));
            } else {
                let count = encoded_title.len().div_ceil(12);
                for (index, chunk) in encoded_title.chunks(12).enumerate() {
                    let seq_stat = if index == 0 {
                        0x50
                    } else if index + 1 == count {
                        0xD0
                    } else {
                        0x90
                    };
                    title_parts.push((seq_stat, utf8_chunk(chunk)?));
                }
            }
        }
        let mut events: Vec<VxEvent> = Vec::new();
        for (index, note) in track.note_list.iter().enumerate() {
            let mut lyric = "l-aa".to_owned();
            if note.lyric.is_ascii() {
                lyric = note.lyric.clone();
            } else if let Some(pronunciation) = &note.pronunciation {
                if pronunciation.is_ascii() {
                    lyric = pronunciation.clone();
                }
            }
            if !lyric.is_empty() {
                let characters: Vec<char> = lyric.chars().collect();
                if characters.len() <= 10 {
                    events.push(VxEvent::Lyric {
                        time: 0,
                        seq_stat: 0x10,
                        seq_num: Some(index as i64),
                        text: lyric,
                    });
                } else {
                    events.push(VxEvent::Lyric {
                        time: 0,
                        seq_stat: 0x50,
                        seq_num: Some(index as i64),
                        text: characters[..10].iter().collect(),
                    });
                    let mut offset = 10usize;
                    while offset < characters.len() {
                        let is_last = offset + 12 >= characters.len();
                        let text: String = if is_last {
                            characters[offset..].iter().collect()
                        } else {
                            characters[offset..offset + 12].iter().collect()
                        };
                        events.push(VxEvent::Lyric {
                            time: 0,
                            seq_stat: if is_last { 0xD0 } else { 0x90 },
                            seq_num: None,
                            text,
                        });
                        if is_last {
                            break;
                        }
                        offset += 12;
                    }
                }
            }
            events.push(VxEvent::NoteOn {
                time: py_round(note.start_pos as f64 / self.tick_rate()),
                note: note.key_number,
                velocity: 0x7F,
                on_data: index as i64,
            });
            events.push(VxEvent::NoteOff {
                time: py_round(note.end_pos() as f64 / self.tick_rate()),
                note: note.key_number,
                velocity: 0,
                off_data: 0,
            });
        }
        let secs_step = 1.0 / 200.0;
        let mut pitch_data = VxPitchData {
            time_based_pitch_sequence: VxTimeBasedPitchSequence {
                time_frame_period_seconds: secs_step,
                num_frames_overall_sequence: 0,
                pitch_sequence: Vec::new(),
            },
        };
        let mut prev_point: Option<VxPitchPoint> = None;
        for pitch_point in &track.edited_params.pitch.points {
            if pitch_point.y != -100 {
                let position = (self
                    .synchronizer
                    .get_actual_secs_from_ticks(pitch_point.x - self.first_bar_length)
                    / secs_step) as i64;
                if let Some(previous) = prev_point {
                    let mut interp_pos = previous.position + 1;
                    while interp_pos <= position {
                        let pitch = if interp_pos != position {
                            previous.pitch
                                + (pitch_point.y as f64 - 6900.0 - previous.pitch)
                                    * ((interp_pos - previous.position) as f64
                                        / (position - previous.position) as f64)
                        } else {
                            pitch_point.y as f64 - 6900.0
                        };
                        pitch_data
                            .time_based_pitch_sequence
                            .pitch_sequence
                            .push(VxPitchPoint {
                                pitch,
                                position: interp_pos,
                                applicable: true,
                            });
                        interp_pos += 1;
                    }
                } else {
                    pitch_data
                        .time_based_pitch_sequence
                        .pitch_sequence
                        .push(VxPitchPoint {
                            pitch: pitch_point.y as f64 - 6900.0,
                            position,
                            applicable: true,
                        });
                }
                prev_point = pitch_data
                    .time_based_pitch_sequence
                    .pitch_sequence
                    .last()
                    .copied();
            } else {
                prev_point = None;
            }
            if let Some(last) = pitch_data
                .time_based_pitch_sequence
                .pitch_sequence
                .last()
            {
                pitch_data
                    .time_based_pitch_sequence
                    .num_frames_overall_sequence = last.position;
            }
        }
        let pitch_data_json = super::pyjson::dumps(&pitch_data.to_json(), false, true);
        let characters: Vec<char> = pitch_data_json.chars().collect();
        if characters.len() <= 12 {
            events.push(VxEvent::Metadata {
                time: 0,
                seq_stat: 0x10,
                text: pitch_data_json,
            });
        } else {
            let mut offset = 0usize;
            let mut index = 0usize;
            while offset < characters.len() {
                let is_last = offset + 12 >= characters.len();
                let text: String = if is_last {
                    characters[offset..].iter().collect()
                } else {
                    characters[offset..offset + 12].iter().collect()
                };
                let seq_stat = if index == 0 {
                    0x50
                } else if is_last {
                    0xD0
                } else {
                    0x90
                };
                events.push(VxEvent::Metadata {
                    time: 0,
                    seq_stat,
                    text,
                });
                if is_last {
                    break;
                }
                offset += 12;
                index += 1;
            }
        }
        if events.is_empty() {
            return Ok(None);
        }
        Ok(Some(VxTrack {
            title_parts,
            events,
        }))
    }
}

fn utf8_chunk(chunk: &[u8]) -> Result<String> {
    std::str::from_utf8(chunk)
        .map(str::to_owned)
        .map_err(|error| LibreSvipError::Validation(format!("invalid track title chunk: {error}")))
}

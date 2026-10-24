//! Port of `libresvip.plugins.dv.dv_parser`.

use libresvip_core::core::compat::find_last_index;
use libresvip_core::core::tick_counter::shift_tempo_list;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{
    InstrumentalTrack, Note, Project, SingingTrack, SongTempo, TimeSignature, Track,
};

use super::model::{DvAudioTrack, DvNote, DvProject, DvSingingTrack, DvTempo, DvTimeSignature};
use super::pitch::{convert_note_key_int, pitch_from_dv_track, DvNoteWithPitch, DvSegmentPitchRawData};

use libresvip_core::core::compat::py_round;

/// `skip_beat_list` (local copy).
///
/// `libresvip-core` mirrors Python's `find_last_index` but then indexes the
/// slice with `-1 as usize`; Python's negative index means "last element", so
/// the shared helper panics on inputs where no beat matches. Keeping the port
/// here avoids that and reproduces Python exactly.
fn skip_beat_list(beat_list: &[TimeSignature], skip_bars: i64) -> Vec<TimeSignature> {
    let mut result: Vec<TimeSignature> = beat_list
        .iter()
        .filter(|beat| beat.bar_index >= skip_bars)
        .map(|beat| TimeSignature::new(beat.bar_index - skip_bars, beat.numerator, beat.denominator))
        .collect();
    if result.is_empty() || result[0].bar_index > 0 {
        if beat_list.is_empty() {
            result.insert(0, TimeSignature::default());
        } else {
            let index = find_last_index(beat_list, |beat| beat.bar_index <= skip_bars);
            let mut beat = if index < 0 {
                beat_list[beat_list.len() - 1].clone()
            } else {
                beat_list[index as usize].clone()
            };
            beat.bar_index = 0;
            result.insert(0, beat);
        }
    }
    result
}

pub struct DeepVocalParser {
    pub import_instrumental_track: bool,
    pub import_pitch: bool,
}

impl DeepVocalParser {
    /// `parse_project`.
    pub fn parse_project(&self, dv_project: &DvProject) -> Result<Project> {
        let (time_signatures, tick_prefix) =
            self.parse_time_signatures(&dv_project.inner_project.time_signatures)?;
        let first_bar_length = py_round(time_signatures[0].bar_length());
        let tempos = self.parse_tempos(
            &dv_project.inner_project.tempos,
            tick_prefix,
            first_bar_length,
        );
        let audio_tracks: Vec<&DvAudioTrack> = dv_project
            .inner_project
            .tracks
            .iter()
            .filter(|track| track.track_type == 1)
            .filter_map(|track| track.as_audio())
            .filter(|track| !track.infos.is_empty())
            .collect();
        let instrumental_tracks = self.parse_instrumental_tracks(&audio_tracks, tick_prefix);
        let singing_tracks: Vec<&DvSingingTrack> = dv_project
            .inner_project
            .tracks
            .iter()
            .filter(|track| track.track_type == 0)
            .filter_map(|track| track.as_singing())
            .collect();
        let singing_tracks = self.parse_singing_tracks(&singing_tracks, tick_prefix, first_bar_length, &tempos)?;
        let mut track_list = singing_tracks;
        track_list.extend(instrumental_tracks);
        Ok(Project {
            warnings: Vec::new(),
            version: String::new(),
            time_signature_list: time_signatures,
            song_tempo_list: tempos,
            track_list,
        })
    }

    /// `parse_time_signatures`: returns the signatures and `tick_prefix`.
    fn parse_time_signatures(
        &self,
        dv_time_signatures: &[DvTimeSignature],
    ) -> Result<(Vec<TimeSignature>, i64)> {
        let time_signatures: Vec<TimeSignature> = dv_time_signatures
            .iter()
            .map(|signature| {
                TimeSignature::new(
                    signature.measure_position as i64,
                    signature.numerator as i64,
                    signature.denominator as i64,
                )
            })
            .collect();
        if time_signatures.is_empty() {
            return Err(LibreSvipError::msg("list index out of range"));
        }
        let index = find_last_index(&time_signatures, |beat| beat.bar_index <= 1).max(0);
        let mut tick_prefix: i64 = 0;
        for i in 0..=index {
            if i < index {
                tick_prefix += (time_signatures[i as usize + 1].bar_index
                    - time_signatures[i as usize].bar_index)
                    * py_round(time_signatures[i as usize].bar_length());
            } else {
                tick_prefix += (1 - time_signatures[i as usize].bar_index)
                    * py_round(time_signatures[i as usize].bar_length());
            }
        }
        Ok((skip_beat_list(&time_signatures, 1), tick_prefix))
    }

    /// `parse_tempos`.
    fn parse_tempos(
        &self,
        dv_tempos: &[DvTempo],
        tick_prefix: i64,
        first_bar_length: i64,
    ) -> Vec<SongTempo> {
        let tempos: Vec<SongTempo> = dv_tempos
            .iter()
            .map(|tempo| SongTempo::new(tempo.position as i64, tempo.bpm as f64 / 100.0))
            .collect();
        let shifted = shift_tempo_list(&tempos, -tick_prefix + first_bar_length);
        let mut result = Vec::new();
        for (index, mut tempo) in shifted.into_iter().enumerate() {
            if index == 0 {
                tempo.position = 0;
                result.push(tempo);
            } else if tempo.position >= 0 {
                result.push(tempo);
            }
        }
        result
    }

    /// `parse_instrumental_tracks`.
    fn parse_instrumental_tracks(
        &self,
        dv_audio_tracks: &[&DvAudioTrack],
        tick_prefix: i64,
    ) -> Vec<Track> {
        let mut track_list = Vec::new();
        if self.import_instrumental_track {
            for dv_track in dv_audio_tracks {
                let Some(first_info) = dv_track.infos.first() else {
                    continue;
                };
                track_list.push(Track::Instrumental(InstrumentalTrack {
                    title: if dv_track.name.is_empty() {
                        first_info.name.clone()
                    } else {
                        dv_track.name.clone()
                    },
                    mute: dv_track.mute != 0,
                    solo: dv_track.solo != 0,
                    offset: first_info.start as i64 + tick_prefix,
                    audio_file_path: first_info.path.clone(),
                    ..Default::default()
                }));
            }
        }
        track_list
    }

    /// `parse_singing_tracks`.
    fn parse_singing_tracks(
        &self,
        dv_singing_tracks: &[&DvSingingTrack],
        tick_prefix: i64,
        first_bar_length: i64,
        tempo_list: &[SongTempo],
    ) -> Result<Vec<Track>> {
        let mut track_list = Vec::new();
        for dv_track in dv_singing_tracks {
            for (index, segment) in dv_track.segments.iter().enumerate() {
                let mut note_with_pitch: Vec<DvNoteWithPitch> = Vec::new();
                let tick_offset = segment.start as i64;
                let note_list = self.parse_notes(
                    &segment.notes,
                    &mut note_with_pitch,
                    tick_offset - tick_prefix,
                );
                let mut track = SingingTrack {
                    title: format!("{} {}", dv_track.name, index + 1),
                    mute: dv_track.mute != 0,
                    solo: dv_track.solo != 0,
                    ai_singer_name: segment.singer_name.clone(),
                    note_list,
                    ..Default::default()
                };
                if self.import_pitch {
                    let raw = DvSegmentPitchRawData {
                        tick_offset: tick_offset - tick_prefix,
                        data: segment.pitch_data.clone(),
                    };
                    if let Some(pitch) = pitch_from_dv_track(
                        first_bar_length,
                        &[raw],
                        &note_with_pitch,
                        tempo_list,
                    ) {
                        track.edited_params.pitch = pitch;
                    }
                }
                track_list.push(Track::Singing(track));
            }
        }
        Ok(track_list)
    }

    /// `parse_notes`.
    fn parse_notes(
        &self,
        dv_notes: &[DvNote],
        note_with_pitch: &mut Vec<DvNoteWithPitch>,
        tick_offset: i64,
    ) -> Vec<Note> {
        let mut note_list = Vec::new();
        for dv_note in dv_notes {
            let note = Note {
                start_pos: tick_offset + dv_note.start as i64,
                length: dv_note.length as i64,
                key_number: convert_note_key_int(dv_note.key as i64),
                lyric: dv_note.word.clone(),
                pronunciation: if dv_note.phoneme == "-" {
                    None
                } else {
                    Some(dv_note.phoneme.clone())
                },
                ..Default::default()
            };
            note_with_pitch.push(DvNoteWithPitch {
                note: note.clone(),
                por_head: dv_note.por_head.unwrap_or(0) as i64,
                por_tail: dv_note.por_tail.unwrap_or(0) as i64,
                ben_len: dv_note.ben_length.unwrap_or(0) as i64,
                ben_dep: dv_note.ben_depth.unwrap_or(0) as i64,
                vibrato: dv_note.note_vibrato_data.vibrato_points.clone(),
            });
            note_list.push(note);
        }
        note_list
    }
}

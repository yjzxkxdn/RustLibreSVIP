//! Port of `libresvip.plugins.mtp.muta_generator`.

use libresvip_core::core::lyric_phoneme::chinese::{get_pinyin_series, is_chinese_char};
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{
    InstrumentalTrack, Note, ParamCurve, Project, SingingTrack, SongTempo, TimeSignature, Track,
};
use libresvip_core::utils::audio::audio_track_info;

use super::model::{
    MutaAudioTrackData, MutaNote, MutaNoteTiming, MutaParams, MutaPoint, MutaProject,
    MutaSongTrackData, MutaTempo, MutaTimeSignature, MutaTrack, MutaTrackData, TRACK_TYPE_AUDIO,
    TRACK_TYPE_SONG,
};

fn py_round(value: f64) -> i64 {
    value.round_ties_even() as i64
}

/// `LATIN_ALPHABET.fullmatch`.
fn is_latin_letters(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|character| character.is_ascii_alphabetic())
}

/// `CHINESE_RE.fullmatch` — the pattern is a single-character class.
fn is_single_chinese_char(text: &str) -> bool {
    let mut characters = text.chars();
    match (characters.next(), characters.next()) {
        (Some(character), None) => is_chinese_char(character),
        _ => false,
    }
}

pub struct MutaGenerator {
    pub default_singer_name: String,
}

impl MutaGenerator {
    /// `generate_project`.
    pub fn generate_project(&self, project: &Project) -> Result<MutaProject> {
        let first_signature = project
            .time_signature_list
            .first()
            .ok_or_else(|| LibreSvipError::msg("list index out of range"))?;
        let first_bar_length = first_signature.bar_length() as i64;
        let time_synchronizer = TimeSynchronizer::from_project(&project.song_tempo_list);
        let time_signatures = Self::generate_time_signatures(&project.time_signature_list);
        let tempos = Self::generate_tempos(&project.song_tempo_list, first_bar_length);
        let singing_tracks: Vec<&SingingTrack> = project.singing_tracks().collect();
        let singing_tracks = self.generate_singing_tracks(&singing_tracks, first_bar_length)?;
        let instrumental_tracks: Vec<&InstrumentalTrack> = project
            .track_list
            .iter()
            .filter_map(Track::as_instrumental)
            .collect();
        let instrumental_tracks = self.generate_instrumental_tracks(
            &instrumental_tracks,
            &time_synchronizer,
        )?;
        let mut tracks = singing_tracks;
        tracks.extend(instrumental_tracks);
        Ok(MutaProject {
            file_version: 101,
            tempos,
            time_signatures,
            tracks,
        })
    }

    /// `generate_time_signatures`.
    fn generate_time_signatures(time_signatures: &[TimeSignature]) -> Vec<MutaTimeSignature> {
        time_signatures
            .iter()
            .map(|signature| MutaTimeSignature {
                measure_position: (signature.bar_index - 1).max(0) as i32,
                numerator: signature.numerator as u32,
                denominator: signature.denominator as u32,
            })
            .collect()
    }

    /// `generate_tempos`.
    fn generate_tempos(tempos: &[SongTempo], first_bar_length: i64) -> Vec<MutaTempo> {
        tempos
            .iter()
            .map(|tempo| MutaTempo {
                position: (tempo.position - first_bar_length).max(0) as i32,
                bpm: py_round(tempo.bpm * 100.0) as u32,
            })
            .collect()
    }

    /// `generate_singing_tracks`.
    fn generate_singing_tracks(
        &self,
        tracks: &[&SingingTrack],
        first_bar_length: i64,
    ) -> Result<Vec<MutaTrack>> {
        let mut track_list: Vec<MutaTrack> = Vec::new();
        for track in tracks {
            let mut song_track_data = MutaSongTrackData {
                start: first_bar_length as u32,
                length: ((track.note_list.iter().map(Note::end_pos).max().unwrap_or(0))
                    + first_bar_length) as u32,
                singer_name: self.singer_name()?,
                speaker_id: 0,
                notes: Self::generate_notes(&track.note_list, first_bar_length)?,
                params: MutaParams::default(),
            };
            let pitch_points = Self::generate_pitch(&track.edited_params.pitch, first_bar_length);
            if !pitch_points.is_empty() {
                song_track_data.params.pitch_data = pitch_points;
            }
            track_list.push(MutaTrack {
                track_type: TRACK_TYPE_SONG,
                seq_count: 1,
                name: format!("Song{}", track_list.len() + 1),
                mute: track.mute as u32,
                solo: track.solo as u32,
                volume: 50,
                pan: 50,
                padding: [0; 52],
                data: MutaTrackData::Song(vec![song_track_data]),
            });
        }
        Ok(track_list)
    }

    fn singer_name(&self) -> Result<Vec<u16>> {
        let units: Vec<u16> = self.default_singer_name.encode_utf16().collect();
        if units.len() > 258 {
            return Err(LibreSvipError::msg("singer name is too long"));
        }
        let mut values = units;
        values.resize(258, 0);
        Ok(values)
    }

    /// `generate_pitch`.
    fn generate_pitch(pitch: &ParamCurve, first_bar_length: i64) -> Vec<MutaPoint> {
        pitch
            .points
            .iter()
            .map(|point| MutaPoint {
                time: (point.x - 2 * first_bar_length).max(-1) as i32,
                value: if point.y < 0 {
                    12900
                } else {
                    (point.y - 1200) as i32
                },
            })
            .collect()
    }

    /// `generate_notes`.
    fn generate_notes(notes: &[Note], first_bar_length: i64) -> Result<Vec<MutaNote>> {
        let mut result = Vec::with_capacity(notes.len());
        for note in notes {
            let characters: Vec<char> = note.lyric.chars().collect();
            if characters.len() > 8 {
                return Err(LibreSvipError::msg("lyric is too long for a Muta note"));
            }
            let mut lyric = [0u16; 8];
            for (index, character) in characters.iter().enumerate() {
                let value = *character as u32;
                if value > u16::MAX as u32 {
                    return Err(LibreSvipError::msg("lyric contains a non-BMP character"));
                }
                lyric[index] = value as u16;
            }
            result.push(MutaNote {
                start: (note.start_pos - first_bar_length) as i32,
                length: note.length as u32,
                key: (139 - note.key_number) as u32,
                lyric,
                phoneme: Self::generate_phoneme(note)?,
                tmg_data: [MutaNoteTiming {
                    ori_pos: 0,
                    mod_pos: 0,
                }; 5],
            });
        }
        Ok(result)
    }

    /// `generate_phoneme`.
    fn generate_phoneme(note: &Note) -> Result<String> {
        let pronunciation = note
            .pronunciation
            .as_deref()
            .filter(|value| !value.is_empty());
        let mut result = String::new();
        if note.lyric == "-" {
            result.push('-');
        } else {
            let candidate = pronunciation.unwrap_or(note.lyric.as_str());
            if is_latin_letters(candidate) {
                result.push_str(candidate);
            } else if is_single_chinese_char(&note.lyric) {
                let pinyin = get_pinyin_series(&[note.lyric.as_str()])?;
                result.push_str(&pinyin.join(" "));
            }
        }
        if result.encode_utf16().count() * 2 > 16 {
            result.clear();
        }
        Ok(result)
    }

    /// `generate_instrumental_tracks`.
    fn generate_instrumental_tracks(
        &self,
        tracks: &[&InstrumentalTrack],
        time_synchronizer: &TimeSynchronizer,
    ) -> Result<Vec<MutaTrack>> {
        let mut track_list: Vec<MutaTrack> = Vec::new();
        for track in tracks {
            if let Some(track_info) = audio_track_info(&track.audio_file_path, true) {
                let mut file_path = track.audio_file_path.clone();
                file_path.push('\0');
                track_list.push(MutaTrack {
                    track_type: TRACK_TYPE_AUDIO,
                    seq_count: 1,
                    name: format!("Audio{}", track_list.len() + 1),
                    mute: track.mute as u32,
                    solo: track.solo as u32,
                    volume: 50,
                    pan: 50,
                    padding: [0; 52],
                    data: MutaTrackData::Audio(vec![MutaAudioTrackData {
                        start: track.offset as u32,
                        length: py_round(
                            time_synchronizer.get_actual_ticks_from_secs_offset(
                                track.offset,
                                track_info.duration,
                            ),
                        ) as u32,
                        file_path,
                    }]),
                });
            }
        }
        Ok(track_list)
    }
}

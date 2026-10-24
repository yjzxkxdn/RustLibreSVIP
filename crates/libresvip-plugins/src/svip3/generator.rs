//! Port of `libresvip.plugins.svip3.svip3_generator`.

use libresvip_core::core::tick_counter::skip_tempo_list;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::core::warning_types::{show_warning_kind, WarningKind};
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{
    InstrumentalTrack, Note, ParamCurve, Project, SingingTrack, SongTempo, TimeSignature, Track,
};

use super::constants::{
    length_validate_tag, AUDIO_TRACK_URL, COLORS, MAX_NOTE_DURATION, MIN_NOTE_DURATION,
    SINGING_TRACK_URL,
};
use super::model::{
    ProtoAny, Svip3AudioPattern, Svip3AudioTrack, Svip3BeatSize, Svip3LineParamNode, Svip3Note,
    Svip3Project, Svip3SingingPattern, Svip3SingingTrack, Svip3SongBeat, Svip3SongTempo,
};
use super::singers::singer_id;
use crate::ppsf::audio::audio_track_duration;
use crate::ppsf::util::{py_round, random_index};

/// The svip3 protobuf fields are `int32`. Python's `struct` packing raises on
/// overflow, so a value that does not fit must fail the conversion instead of
/// silently wrapping around.
fn i32_of(value: i64) -> Result<i32> {
    i32::try_from(value).map_err(|_| {
        LibreSvipError::Validation(format!("value {value} does not fit the svip3 int32 range"))
    })
}

pub struct Svip3Generator {
    first_bar_length: i64,
    synchronizer: TimeSynchronizer,
    song_duration: i64,
}

impl Svip3Generator {
    pub fn new() -> Self {
        Self {
            first_bar_length: 0,
            synchronizer: TimeSynchronizer::from_project(&[SongTempo::default()]),
            song_duration: 0,
        }
    }

    pub fn generate_project(&mut self, project: &Project) -> Result<Svip3Project> {
        let first_time_signature = project
            .time_signature_list
            .first()
            .ok_or_else(|| LibreSvipError::Validation("time signature list is empty".to_owned()))?;
        self.first_bar_length = py_round(first_time_signature.bar_length());
        self.synchronizer = TimeSynchronizer::from_project(&project.song_tempo_list);
        self.song_duration = 0;
        let beat_list = Self::generate_time_signatures(&project.time_signature_list)?;
        let tempo_list = self.generate_song_tempos(&project.song_tempo_list)?;
        let track_list = self.generate_tracks(&project.track_list)?;
        Ok(Svip3Project {
            duration: i32_of(self.song_duration)?,
            tempo_list,
            beat_list,
            track_list,
        })
    }

    /// `generate_time_signatures`.
    pub fn generate_time_signatures(
        time_signature_list: &[TimeSignature],
    ) -> Result<Vec<Svip3SongBeat>> {
        time_signature_list
            .iter()
            .map(|time_signature| {
                Ok(Svip3SongBeat {
                    pos: i32_of(time_signature.bar_index)?,
                    beat_size: Some(Svip3BeatSize {
                        numerator: i32_of(time_signature.numerator)?,
                        denominator: i32_of(time_signature.denominator)?,
                    }),
                })
            })
            .collect()
    }

    /// `generate_song_tempos`.
    pub fn generate_song_tempos(
        &self,
        song_tempo_list: &[SongTempo],
    ) -> Result<Vec<Svip3SongTempo>> {
        let song_tempo_list = skip_tempo_list(song_tempo_list, self.first_bar_length);
        song_tempo_list
            .iter()
            .map(|song_tempo| {
                Ok(Svip3SongTempo {
                    tempo: i32_of(py_round(song_tempo.bpm * 100.0))?,
                    pos: i32_of(song_tempo.position)?,
                })
            })
            .collect()
    }

    fn generate_tracks(&mut self, track_list: &[Track]) -> Result<Vec<ProtoAny>> {
        let mut svip3_track_list = Vec::new();
        for track in track_list {
            let color = COLORS[random_index(COLORS.len())];
            let (type_url, value) = match track {
                Track::Singing(singing_track) => {
                    let svip3_track = self.generate_singing_track(singing_track, color)?;
                    (SINGING_TRACK_URL, svip3_track.encode())
                }
                Track::Instrumental(instrumental_track) => {
                    let svip3_track = self.generate_instrumental_track(instrumental_track, color)?;
                    (AUDIO_TRACK_URL, svip3_track.encode())
                }
            };
            svip3_track_list.push(ProtoAny {
                type_url: type_url.to_owned(),
                value,
            });
        }
        Ok(svip3_track_list)
    }

    fn generate_instrumental_track(
        &mut self,
        track: &InstrumentalTrack,
        color: &str,
    ) -> Result<Svip3AudioTrack> {
        Ok(Svip3AudioTrack {
            name: track.title.clone(),
            color: color.to_owned(),
            mute: track.mute,
            solo: track.solo,
            volume: to_decibel_volume(track.volume) as f32,
            pan: generate_pan(track.pan) as f32,
            pattern_list: self.generate_audio_patterns(track)?,
        })
    }

    fn generate_audio_patterns(
        &mut self,
        track: &InstrumentalTrack,
    ) -> Result<Vec<Svip3AudioPattern>> {
        let Some(audio_info) = audio_track_duration(&track.audio_file_path, false) else {
            return Ok(Vec::new());
        };
        let audio_duration_in_ticks =
            i32_of(py_round(self.synchronizer.get_actual_ticks_from_secs(audio_info)))?;
        let mut pattern = Svip3AudioPattern {
            real_dur: audio_duration_in_ticks,
            play_dur: audio_duration_in_ticks,
            ..Default::default()
        };
        if track.offset >= 0 {
            pattern.play_pos = 0;
            pattern.real_pos =
                i32_of(py_round(self.synchronizer.get_actual_ticks_from_ticks(track.offset)))?;
        } else {
            // The Python code assigns `play_pos` twice; the first assignment is
            // overwritten immediately, so only the second one has an effect.
            pattern.play_pos = i32_of(i64::from(audio_duration_in_ticks) - track.offset)?;
            pattern.play_pos = i32_of(track.offset)?;
            pattern.real_pos = i32_of(-track.offset)?;
        }
        let pattern_end = i64::from(pattern.real_pos) + i64::from(pattern.play_dur);
        if pattern_end > self.song_duration {
            self.song_duration = py_round(pattern_end as f64 + self.first_bar_length as f64);
        }
        pattern.audio_file_path = track.audio_file_path.clone();
        pattern.name = track.title.clone();
        Ok(vec![pattern])
    }

    fn generate_singing_track(
        &mut self,
        track: &SingingTrack,
        color: &str,
    ) -> Result<Svip3SingingTrack> {
        Ok(Svip3SingingTrack {
            name: track.title.clone(),
            color: color.to_owned(),
            mute: track.mute,
            solo: track.solo,
            volume: to_decibel_volume(track.volume) as f32,
            pan: generate_pan(track.pan) as f32,
            ai_singer_id: singer_id(&track.ai_singer_name).to_owned(),
            pattern_list: self.generate_singing_patterns(track)?,
        })
    }

    fn generate_singing_patterns(
        &mut self,
        track: &SingingTrack,
    ) -> Result<Vec<Svip3SingingPattern>> {
        let Some(last_note) = track.note_list.last() else {
            return Ok(Vec::new());
        };
        let last_note_end = last_note.end_pos();
        if last_note_end > self.song_duration {
            self.song_duration = last_note_end + self.first_bar_length;
        }
        Ok(vec![Svip3SingingPattern {
            name: track.title.clone(),
            real_pos: 0,
            play_pos: 0,
            real_dur: i32_of(last_note_end + self.first_bar_length)?,
            play_dur: i32_of(last_note_end + self.first_bar_length)?,
            is_mute: track.mute,
            note_list: self.generate_notes(&track.note_list)?,
            edited_pitch_line: self.generate_pitch_param(&track.edited_params.pitch)?,
        }])
    }

    fn generate_notes(&self, note_list: &[Note]) -> Result<Vec<Svip3Note>> {
        let mut svip3_note_list = Vec::new();
        for note in note_list {
            let mut consonant_length = 0i32;
            let mut has_consonant = false;
            if let Some(edited_phones) = &note.edited_phones {
                if edited_phones.head_length_in_secs > 0.0 {
                    has_consonant = true;
                    let phone_start_in_secs = self
                        .synchronizer
                        .get_actual_secs_from_ticks(note.start_pos)
                        - edited_phones.head_length_in_secs;
                    let phone_start_in_ticks = self
                        .synchronizer
                        .get_actual_ticks_from_secs(phone_start_in_secs);
                    consonant_length = i32_of(py_round(
                        self.synchronizer.get_actual_ticks_from_ticks(note.start_pos)
                            - phone_start_in_ticks,
                    ))?;
                }
            }
            let pronunciation = if is_pinyin(&note.lyric) {
                note.lyric.clone()
            } else if let Some(pronunciation) = note
                .pronunciation
                .as_ref()
                .filter(|pronunciation| !pronunciation.is_empty())
            {
                pronunciation.clone()
            } else if !note.lyric.is_empty() && is_chinese_lyric(&note.lyric) {
                // pypinyin is not available in the Rust port; fall back to the
                // lyric itself so the unsupported-pinyin warning fires below
                // instead of silently emitting nonsense.
                libresvip_core::core::lyric_phoneme::chinese::lazy_pinyin(&note.lyric)
                    .map(|syllables| syllables.join(" "))
                    .unwrap_or_else(|_| note.lyric.clone())
            } else {
                note.pronunciation
                    .clone()
                    .unwrap_or_else(|| note.lyric.clone())
            };
            let mut pronunciation = pronunciation;
            if note.lyric != "-" && !is_pinyin(&pronunciation) {
                show_warning_kind(WarningKind::PhonemeDropped, format!("Unsupported pinyin: {pronunciation}"));
                pronunciation.clear();
            }
            let note_duration = self
                .synchronizer
                .get_duration_secs_from_ticks(note.start_pos, note.end_pos());
            let length_valid_tag = if note_duration < MIN_NOTE_DURATION {
                show_warning_kind(WarningKind::LengthTruncated, format!("Note duration is too short: {}", note.lyric));
                length_validate_tag::TOO_SHORT
            } else if note_duration > MAX_NOTE_DURATION {
                show_warning_kind(WarningKind::LengthTruncated, format!("Note duration is too long: {}", note.lyric));
                length_validate_tag::TOO_LONG
            } else {
                length_validate_tag::NONE
            };
            svip3_note_list.push(Svip3Note {
                start_pos: i32_of(note.start_pos)?,
                width_pos: i32_of(note.length)?,
                key_index: i32_of(note.key_number)?,
                lyric: note.lyric.clone(),
                pronouncing: pronunciation,
                consonant_len: consonant_length,
                has_consonant,
                sp_len: if note.head_tag.as_deref() == Some("V") {
                    400
                } else {
                    0
                },
                sil_len: if note.head_tag.as_deref() == Some("0") {
                    400
                } else {
                    0
                },
                length_validate_tag: length_valid_tag,
            });
        }
        Ok(svip3_note_list)
    }

    fn generate_pitch_param(&self, curve: &ParamCurve) -> Result<Vec<Svip3LineParamNode>> {
        let mut point_list = Vec::new();
        for segment in curve.split_into_segments(-100) {
            if segment.is_empty() {
                continue;
            }
            let mut prev_pos = 0i64;
            let mut left_interrupt = false;
            for point in &segment {
                if point.x == prev_pos {
                    continue;
                }
                if point.x < self.first_bar_length {
                    continue;
                } else if !left_interrupt {
                    left_interrupt = true;
                    point_list.push(Svip3LineParamNode {
                        pos: i32_of(point.x - self.first_bar_length)?,
                        value: -1.0,
                    });
                }
                let pos = point.x - self.first_bar_length;
                let value = (point.y + 50) as f64 / 100.0;
                point_list.push(Svip3LineParamNode {
                    pos: i32_of(pos)?,
                    value: value as f32,
                });
                prev_pos = point.x;
            }
            if let Some(last_point) = segment.last() {
                point_list.push(Svip3LineParamNode {
                    pos: i32_of(last_point.x - self.first_bar_length)?,
                    value: -1.0,
                });
            }
        }
        Ok(point_list)
    }
}

impl Default for Svip3Generator {
    fn default() -> Self {
        Self::new()
    }
}

/// `to_decibel_volume`.
pub fn to_decibel_volume(volume: f64) -> f64 {
    if volume > 0.0 {
        super::parser::ratio_to_db(volume.max(0.01)).max(-70.0)
    } else {
        -70.0
    }
}

/// `generate_pan`.
pub fn generate_pan(pan: f64) -> f64 {
    pan * 10.0
}

/// `PINYIN_PATTERN = re.compile(r"^[a-z]+$")`, used with `fullmatch`.
fn is_pinyin(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_lowercase())
}

/// `CHINESE_RE.fullmatch(lyric) is not None`.
fn is_chinese_lyric(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(libresvip_core::core::lyric_phoneme::chinese::is_chinese_char)
}

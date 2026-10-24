//! Port of `libresvip.plugins.ustx.ustx_generator`.

use libresvip_core::model::base::{
    InstrumentalTrack, Note, Project, SingingTrack, SongTempo, TimeSignature, Track,
};
use libresvip_core::model::point::Point;
use libresvip_core::utils::audio::audio_track_info;
use libresvip_core::{LibreSvipError, Result};

use crate::ust::math::round_int;

use super::base_pitch::BasePitchGenerator;
use super::lyric_util;
use super::model::{
    Number, PitchPoint, UCurve, UNote, UPitch, URendererSettings, UTimeSignature, UTempo, UTrack,
    UVibrato, UVoicePart, UWavePart, UstxProject, VersionValue,
};

/// `OutputOptions`.
pub struct OutputOptions {
    /// `OpenUtauEnglishPhonemizerCompatibility.ARPA`.
    pub arpa: bool,
}

impl Default for OutputOptions {
    fn default() -> Self {
        Self { arpa: false }
    }
}

/// Port of `UstxGenerator`.
pub struct UstxGenerator<'a> {
    options: &'a OutputOptions,
}

impl<'a> UstxGenerator<'a> {
    pub fn new(options: &'a OutputOptions) -> Self {
        Self { options }
    }

    /// `generate_project`.
    pub fn generate_project(&self, os_project: &Project) -> Result<UstxProject> {
        let mut ustx_time_signatures: Vec<UTimeSignature> = os_project
            .time_signature_list
            .iter()
            .map(Self::generate_time_signature)
            .collect();
        if ustx_time_signatures.is_empty() {
            ustx_time_signatures.push(UTimeSignature::default());
        }
        let first_time_signature = &ustx_time_signatures[0];
        // Python raises ZeroDivisionError here; a zero beat unit must fail the
        // conversion instead of panicking.
        if first_time_signature.beat_unit == 0 {
            return Err(LibreSvipError::msg(
                "time signature beat unit must not be zero",
            ));
        }
        let first_bar_length =
            1920 * first_time_signature.beat_per_bar / first_time_signature.beat_unit;

        let mut tempos: Vec<UTempo> = os_project
            .song_tempo_list
            .iter()
            .map(|tempo| Self::generate_tempo(tempo, first_bar_length))
            .collect();
        if tempos.is_empty() {
            tempos.push(UTempo::default());
        }

        let mut ustx_project = UstxProject {
            ustx_version: VersionValue::Str("0.6".to_owned()),
            bpm: tempos[0].bpm,
            tempos,
            time_signatures: ustx_time_signatures,
            voice_parts: Vec::new(),
            wave_parts: Vec::new(),
            ..Default::default()
        };

        let mut tracks: Vec<UTrack> = Vec::new();
        for (track_no, os_track) in os_project.track_list.iter().enumerate() {
            let mut ustx_track = Self::generate_track(os_track);
            match os_track {
                Track::Singing(singing_track) => {
                    ustx_track.singer = Some(singing_track.ai_singer_name.clone());
                    tracks.push(ustx_track);
                    let part = self.generate_voice_part(
                        singing_track,
                        track_no as i64,
                        &ustx_project,
                        first_bar_length,
                    )?;
                    ustx_project.voice_parts.push(part);
                }
                Track::Instrumental(instrumental_track) => {
                    tracks.push(ustx_track);
                    if let Some(wave_part) =
                        Self::generate_wave_part(instrumental_track, track_no as i64)
                    {
                        ustx_project.wave_parts.push(wave_part);
                    }
                }
            }
        }
        ustx_project.tracks = tracks;
        Ok(ustx_project)
    }

    /// `generate_tempo`.
    fn generate_tempo(os_tempo: &SongTempo, first_bar_length: i64) -> UTempo {
        UTempo {
            position: (os_tempo.position - first_bar_length).max(0),
            bpm: os_tempo.bpm,
        }
    }

    /// `generate_time_signature`.
    fn generate_time_signature(os_time_signature: &TimeSignature) -> UTimeSignature {
        UTimeSignature {
            bar_position: os_time_signature.bar_index,
            beat_per_bar: os_time_signature.numerator,
            beat_unit: os_time_signature.denominator,
        }
    }

    /// `generate_track`.
    fn generate_track(os_track: &Track) -> UTrack {
        UTrack {
            track_name: Some(os_track.title().to_owned()),
            singer: Some(String::new()),
            phonemizer: Some("OpenUtau.Core.DefaultPhonemizer".to_owned()),
            renderer_settings: Some(URendererSettings {
                renderer: Some("CLASSIC".to_owned()),
                resampler: None,
                wavtool: None,
            }),
            mute: os_track.mute(),
            solo: os_track.solo(),
            volume: os_track.volume().log10() * 10.0,
            track_expressions: Some(Vec::new()),
            ..Default::default()
        }
    }

    /// `generate_voice_part`.
    fn generate_voice_part(
        &self,
        os_track: &SingingTrack,
        track_no: i64,
        ustx_project: &UstxProject,
        first_bar_length: i64,
    ) -> Result<UVoicePart> {
        let mut ustx_voice_part = UVoicePart {
            name: os_track.title.clone(),
            position: 0,
            track_no,
            comment: Some(String::new()),
            notes: Vec::new(),
            curves: Vec::new(),
            duration: 0,
        };
        if !os_track.note_list.is_empty() {
            ustx_voice_part.notes = self.generate_notes(&os_track.note_list);
            let mut os_pitch = os_track.edited_params.pitch.points.clone();
            Self::generate_pitch(
                &mut ustx_voice_part,
                ustx_project,
                &mut os_pitch,
                first_bar_length,
            )?;
        }
        ustx_voice_part.duration = ustx_voice_part.computed_duration();
        Ok(ustx_voice_part)
    }

    /// `generate_wave_part`.
    fn generate_wave_part(os_track: &InstrumentalTrack, track_no: i64) -> Option<UWavePart> {
        let track_info = audio_track_info(&os_track.audio_file_path, false)?;
        Some(UWavePart {
            name: os_track.title.clone(),
            track_no,
            position: os_track.offset,
            comment: Some(String::new()),
            relative_path: os_track.audio_file_path.clone(),
            file_duration_ms: Number::Float(track_info.duration * 1000.0),
            skip_ms: Number::Int(0),
            trim_ms: Number::Int(0),
        })
    }

    /// `generate_notes`.
    fn generate_notes(&self, os_notes: &[Note]) -> Vec<UNote> {
        let mut notes: Vec<UNote> = Vec::new();
        let mut last_note_end_pos: i64 = -480;
        let mut last_note_key_number: i64 = 60;
        let mut last_syllable_index: i64 = 1;
        for (index, os_note) in os_notes.iter().enumerate() {
            let snap_first = last_note_end_pos >= os_note.start_pos;
            let y0 = if snap_first {
                (last_note_key_number - os_note.key_number) * 10
            } else {
                0
            };
            let mut lyric = lyric_util::get_symbol_removed_lyric(&os_note.lyric);
            if let Some(pronunciation) = os_note.pronunciation.as_ref() {
                if !pronunciation.is_empty() {
                    lyric = pronunciation.clone();
                }
            }
            if lyric == "-" {
                lyric = "+".to_owned();
            } else if lyric == "+" {
                if self.options.arpa {
                    last_syllable_index += 1;
                    lyric = format!("+{last_syllable_index}");
                } else {
                    let mut j = index as i64 - 1;
                    while j > 0 {
                        if os_notes[j as usize].lyric == "-" {
                            notes[j as usize].lyric = "+~".to_owned();
                        } else {
                            break;
                        }
                        j -= 1;
                    }
                }
            } else if self.options.arpa {
                last_syllable_index = 1;
            }
            if lyric.chars().count() == 2 {
                if let Some(second) = lyric.chars().nth(1) {
                    if lyric_util::is_punctuation(second) {
                        lyric = lyric.chars().next().map(String::from).unwrap_or_default();
                    }
                }
            }
            notes.push(UNote {
                position: os_note.start_pos,
                duration: os_note.length,
                tone: os_note.key_number,
                lyric,
                pitch: UPitch {
                    snap_first: Some(snap_first),
                    data: vec![
                        PitchPoint {
                            x: -40.0,
                            y: y0 as f64,
                            shape: Some("io".to_owned()),
                        },
                        PitchPoint {
                            x: 0.0,
                            y: 0.0,
                            shape: Some("io".to_owned()),
                        },
                    ],
                },
                vibrato: UVibrato {
                    length: 0.0,
                    period: 175.0,
                    depth: 25.0,
                    in_value: 10.0,
                    out: 10.0,
                    shift: 0.0,
                    drift: 0.0,
                    vol_link: 0.0,
                },
                note_expressions: Some(Vec::new()),
                phoneme_expressions: Some(Vec::new()),
                phoneme_overrides: Some(Vec::new()),
            });
            last_note_end_pos = os_note.end_pos();
            last_note_key_number = os_note.key_number;
        }
        notes
    }

    /// `generate_pitch`.
    fn generate_pitch(
        part: &mut UVoicePart,
        project: &UstxProject,
        os_pitch: &mut Vec<Point>,
        first_bar_length: i64,
    ) -> Result<()> {
        let pitch_start = 0i64;
        let pitch_interval = 5i64;
        let base_pitch = BasePitchGenerator::new(project)?.base_pitch(part);
        let pitch_end_x = base_pitch.len() as i64 * pitch_interval + first_bar_length + 1;

        if os_pitch.is_empty() {
            *os_pitch = vec![Point::new(0, -1), Point::new(pitch_end_x, -1)];
        }
        if os_pitch.last().map(|point| point.x).unwrap_or(0) < pitch_end_x {
            let last_x = os_pitch.last().map(|point| point.x).unwrap_or(0);
            os_pitch.push(Point::new(last_x, -1));
            os_pitch.push(Point::new(pitch_end_x, -1));
        }

        let mut pointer: usize = 0;
        let mut pitd = UCurve {
            xs: Vec::new(),
            ys: Vec::new(),
            abbr: Some("pitd".to_owned()),
        };
        for (index, base) in base_pitch.iter().enumerate() {
            let time = index as i64 * pitch_interval + pitch_start;
            while pointer + 1 < os_pitch.len()
                && os_pitch[pointer + 1].x <= time + first_bar_length
            {
                pointer += 1;
            }
            let previous = os_pitch[pointer];
            let next = os_pitch
                .get(pointer + 1)
                .copied()
                .ok_or_else(|| LibreSvipError::msg("missing pitch point"))?;
            if previous.y < 0 {
                pitd.xs.push(time);
                pitd.ys.push(0);
            } else if previous.x == next.x {
                pitd.xs.push(time);
                pitd.ys.push(round_int(
                    (previous.y + next.y) as f64 / 2.0 - base.trunc(),
                ));
            } else {
                let x1 = previous.x - first_bar_length;
                let x2 = next.x - first_bar_length;
                let y1 = previous.y as f64;
                let y2 = next.y as f64;
                pitd.xs.push(time);
                pitd.ys.push(round_int(
                    (y2 - y1) * (time - x1) as f64 / (x2 - x1) as f64 + y1 - base.trunc(),
                ));
            }
        }
        if !pitd.is_empty() {
            part.curves.push(pitd);
        }
        Ok(())
    }
}

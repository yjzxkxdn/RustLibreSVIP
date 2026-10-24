//! Port of `libresvip.plugins.vpr.vpr_generator`.

use std::path::PathBuf;

use libresvip_core::core::constants::TICKS_IN_BEAT;
use libresvip_core::core::lyric_phoneme::chinese::{get_pinyin_series_with, PinyinSeriesOptions};
use libresvip_core::core::lyric_phoneme::japanese::{is_legato_char, to_romaji};
use libresvip_core::core::lyric_phoneme::korean::hangul2xsampa;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::core::warning_types::{show_warning_kind, WarningKind};
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{Params, Project, SingingTrack, SongTempo, TimeSignature, Track};
use libresvip_core::options::Options;
use libresvip_core::utils::audio::audio_track_info;

use super::constants::{BPM_RATE, DEFAULT_CHINESE_PHONEME, DEFAULT_JAPANESE_PHONEME};
use super::controllers::{
    convert_param_points_to_vocaloid_curve, VprControllerAdapter,
};
use super::model::{
    language, PointValue, VocaloidAITrack, VocaloidAIVoice, VocaloidAudioTrack, VocaloidControllers,
    VocaloidLangID, VocaloidNotes, VocaloidPoint, VocaloidProject, VocaloidRegion,
    VocaloidStandardTrack, VocaloidTimeSig, VocaloidTrack, VocaloidVoice, VocaloidVoicePart,
    VocaloidVoices, VocaloidWav, VocaloidWavPart,
};
use super::phonemes::{pinyin2xsampa, romaji2xsampa};
use super::pitch::VocaloidPitchHandler;

/// `OutputOptions`.
#[derive(Clone, Debug)]
pub struct OutputOptions {
    pub is_ai_singer: bool,
    pub default_lang_id: i64,
    pub default_comp_id: String,
    pub default_singer_name: String,
}

/// Maps an enum option value (`"4"`, or a member name) to the integer value.
pub fn parse_language(value: &str) -> Option<i64> {
    let value = value.trim();
    if let Ok(number) = value.parse::<i64>() {
        return Some(number);
    }
    match value {
        "JAPANESE" => Some(language::JAPANESE),
        "ENGLISH" => Some(language::ENGLISH),
        "KOREAN" => Some(language::KOREAN),
        "SPANISH" => Some(language::SPANISH),
        "SIMPLIFIED_CHINESE" => Some(language::SIMPLIFIED_CHINESE),
        _ => None,
    }
}

impl OutputOptions {
    pub fn from_options(options: &Options) -> Self {
        Self {
            is_ai_singer: options.bool("is_ai_singer", false),
            default_lang_id: parse_language(&options.str("default_lang_id", "4"))
                .unwrap_or(language::SIMPLIFIED_CHINESE),
            default_comp_id: options.str("default_comp_id", "BL8CEAM5N4XN3LFK"),
            default_singer_name: options.str("default_singer_name", "Luo_Tianyi_Wan"),
        }
    }
}

pub struct VocaloidGenerator {
    options: OutputOptions,
    time_synchronizer: TimeSynchronizer,
    first_bar_length: i64,
    time_signatures: Vec<TimeSignature>,
    end_tick: i64,
    /// Insertion-ordered `dict[str, Path]` replacement for `wav_paths`.
    pub wav_paths: Vec<(String, PathBuf)>,
}

impl VocaloidGenerator {
    pub fn new(options: OutputOptions) -> Self {
        Self {
            options,
            time_synchronizer: TimeSynchronizer::from_project(&[]),
            first_bar_length: 0,
            time_signatures: Vec::new(),
            end_tick: 0,
            wav_paths: Vec::new(),
        }
    }

    fn add_wav_path(&mut self, name: String, path: PathBuf) {
        if let Some(entry) = self.wav_paths.iter_mut().find(|entry| entry.0 == name) {
            entry.1 = path;
        } else {
            self.wav_paths.push((name, path));
        }
    }

    pub fn generate_project(&mut self, project: &Project) -> Result<VocaloidProject> {
        let mut vpr = VocaloidProject {
            voices: vec![VocaloidVoices {
                comp_id: Some(self.options.default_comp_id.clone()),
                name: Some(self.options.default_singer_name.clone()),
            }],
            ..Default::default()
        };
        let Some(first_time_signature) = project.time_signature_list.first() else {
            // Python raises IndexError on `time_signature_list[0]`.
            return Err(LibreSvipError::msg("time signature list is empty"));
        };
        self.first_bar_length = first_time_signature.bar_length().round_ties_even() as i64;
        self.time_signatures = project.time_signature_list.clone();
        vpr.master_track.time_sig.events =
            self.generate_time_signatures(&project.time_signature_list);
        vpr.master_track.tempo.events = self.generate_tempos(&project.song_tempo_list);
        vpr.master_track
            .volume
            .events
            .push(VocaloidPoint::new(0, 0));
        self.time_synchronizer = TimeSynchronizer::from_project(&project.song_tempo_list);
        vpr.tracks = self.generate_tracks(&project.track_list)?;
        vpr.master_track.loop_.end = Some(self.end_tick as f64);
        Ok(vpr)
    }

    fn generate_time_signatures(
        &mut self,
        time_signature_list: &[TimeSignature],
    ) -> Vec<VocaloidTimeSig> {
        let mut output_tick = 0.0f64;
        let mut time_sig_events: Vec<VocaloidTimeSig> = Vec::new();
        for time_signature in time_signature_list {
            match time_sig_events.last() {
                None => {
                    output_tick += TICKS_IN_BEAT as f64 * 4.0 * time_signature.bar_index as f64;
                }
                Some(last) => {
                    output_tick += TICKS_IN_BEAT as f64
                        * 4.0
                        * (last.numer as f64 / last.denom as f64)
                        * (time_signature.bar_index - last.bar) as f64;
                }
            }
            time_sig_events.push(VocaloidTimeSig {
                bar: time_signature.bar_index,
                denom: time_signature.denominator,
                numer: time_signature.numerator,
            });
        }
        let rounded = output_tick.round_ties_even() as i64;
        if rounded > self.end_tick {
            self.end_tick = rounded;
        }
        time_sig_events
    }

    fn generate_tempos(&mut self, tempo_list: &[SongTempo]) -> Vec<VocaloidPoint> {
        let tempo_events: Vec<VocaloidPoint> = tempo_list
            .iter()
            .map(|tempo| {
                VocaloidPoint::new(
                    tempo.position,
                    PointValue::Int((tempo.bpm * BPM_RATE) as i64),
                )
            })
            .collect();
        let last_position = tempo_events
            .iter()
            .map(|event| event.pos)
            .max()
            .unwrap_or(0);
        if last_position > self.end_tick {
            self.end_tick = last_position;
        }
        tempo_events
    }

    fn generate_phoneme(&self, lyric: &str) -> Result<Option<String>> {
        if is_legato_char(lyric) {
            return Ok(Some("-".to_owned()));
        }
        if self.options.default_lang_id == language::SIMPLIFIED_CHINESE {
            let pinyin = get_pinyin_series_with(
                &[lyric],
                PinyinSeriesOptions {
                    filter_non_chinese: false,
                    ..Default::default()
                },
            )?
            .join(" ");
            Ok(Some(
                pinyin2xsampa(&pinyin)
                    .unwrap_or(DEFAULT_CHINESE_PHONEME)
                    .to_owned(),
            ))
        } else if self.options.default_lang_id == language::JAPANESE {
            let romaji = to_romaji(lyric).ok_or_else(|| {
                LibreSvipError::msg(format!("cannot convert to romaji: {lyric}"))
            })?;
            Ok(Some(
                romaji2xsampa(&romaji)
                    .unwrap_or(DEFAULT_JAPANESE_PHONEME)
                    .to_owned(),
            ))
        } else if self.options.default_lang_id == language::KOREAN {
            Ok(Some(hangul2xsampa(lyric)?))
        } else {
            Ok(Some(DEFAULT_CHINESE_PHONEME.to_owned()))
        }
    }

    fn generate_tracks(&mut self, track_list: &[Track]) -> Result<Vec<VocaloidTrack>> {
        let mut tracks: Vec<VocaloidTrack> = Vec::new();
        let mut singing_track_found = false;
        for track in track_list {
            match track {
                Track::Instrumental(track) => {
                    let wav_path = PathBuf::from(&track.audio_file_path);
                    let Some(track_info) = audio_track_info(&wav_path, true) else {
                        continue;
                    };
                    if track_info.sample_rate != 44100 || track_info.bit_depth != Some(16) {
                        continue;
                    }
                    let audio_duration_in_secs = track_info.duration;
                    let wav_part_region_end = self
                        .time_synchronizer
                        .get_actual_ticks_from_secs_offset(track.offset, audio_duration_in_secs);
                    let wav_name = wav_path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    self.add_wav_path(wav_name.clone(), wav_path);
                    let wav_part = VocaloidWavPart {
                        pos: track.offset,
                        duration: None,
                        name: Some(String::new()),
                        audio_effects: Vec::new(),
                        region: Some(VocaloidRegion {
                            is_enabled: Some(true),
                            begin: Some(0.0),
                            end: Some(wav_part_region_end),
                        }),
                        wav: Some(VocaloidWav {
                            name: wav_name.clone(),
                            original_name: Some(wav_name),
                        }),
                        voice_changer: None,
                    };
                    let region_end_tick = wav_part_region_end as i64;
                    if region_end_tick > self.end_tick {
                        self.end_tick = region_end_tick;
                    }
                    tracks.push(VocaloidTrack::Audio(VocaloidAudioTrack {
                        name: Some(track.title.clone()),
                        parts: vec![wav_part],
                        is_muted: track.mute,
                        is_solo_mode: track.solo,
                        ..Default::default()
                    }));
                }
                Track::Singing(track) => {
                    singing_track_found = true;
                    let lang_id = self.options.default_lang_id;
                    let mut notes: Vec<VocaloidNotes> = Vec::with_capacity(track.note_list.len());
                    for note in &track.note_list {
                        notes.push(VocaloidNotes {
                            pos: note.start_pos,
                            duration: Some(note.length),
                            number: note.key_number,
                            lyric: note.lyric.clone(),
                            phoneme: self.generate_phoneme(&note.lyric)?,
                            lang_id: Some(lang_id),
                            ..Default::default()
                        });
                    }
                    let duration = track.note_list.last().map(|note| note.end_pos());
                    let mut controllers = self.generate_pitch_data(track)?;
                    controllers.extend(Self::generate_params(
                        &track.edited_params,
                        self.first_bar_length,
                    ));
                    let mut part = match duration {
                        Some(duration) if duration != 0 => Some(VocaloidVoicePart {
                            duration: Some(duration),
                            notes,
                            controllers: Some(controllers),
                            ..Default::default()
                        }),
                        _ => None,
                    };
                    if let Some(part) = part.as_mut() {
                        if self.options.is_ai_singer {
                            part.ai_voice = Some(VocaloidAIVoice {
                                comp_id: Some(self.options.default_comp_id.clone()),
                                lang_ids: vec![VocaloidLangID {
                                    lang_id: Some(lang_id),
                                }],
                            });
                        } else {
                            part.voice = Some(VocaloidVoice {
                                lang_id: Some(lang_id),
                                comp_id: Some(self.options.default_comp_id.clone()),
                            });
                        }
                    }
                    let mut vocaloid_track = if self.options.is_ai_singer {
                        VocaloidTrack::Ai(VocaloidAITrack {
                            name: Some(track.title.clone()),
                            parts: part.into_iter().collect(),
                            is_muted: track.mute,
                            is_solo_mode: track.solo,
                            ..Default::default()
                        })
                    } else {
                        VocaloidTrack::Standard(VocaloidStandardTrack {
                            name: Some(track.title.clone()),
                            parts: part.into_iter().collect(),
                            is_muted: track.mute,
                            is_solo_mode: track.solo,
                            ..Default::default()
                        })
                    };
                    match &mut vocaloid_track {
                        VocaloidTrack::Standard(track) => track.panpot.events.push(VocaloidPoint::new(0, 0)),
                        VocaloidTrack::Ai(track) => track.panpot.events.push(VocaloidPoint::new(0, 0)),
                        VocaloidTrack::Audio(track) => track.panpot.events.push(VocaloidPoint::new(0, 0)),
                    }
                    tracks.push(vocaloid_track);
                    if let Some(duration) = duration {
                        if duration > self.end_tick {
                            self.end_tick = duration;
                        }
                    }
                }
            }
        }
        if singing_track_found
            && !matches!(
                self.options.default_lang_id,
                language::SIMPLIFIED_CHINESE | language::JAPANESE | language::KOREAN
            )
        {
            show_warning_kind(WarningKind::PhonemeDropped,
                "Phonemes of all notes were set to \"la\". Please use \"Job\" -> \"Convert Phonemes to Match Languages\" in the menu of VOCALOID to reset them.",
            );
        }
        Ok(tracks)
    }

    fn generate_pitch_data(&mut self, track: &SingingTrack) -> Result<Vec<VocaloidControllers>> {
        let pitch_handler = VocaloidPitchHandler::new(
            self.time_synchronizer.clone(),
            &track.note_list,
            &self.time_signatures,
            self.first_bar_length,
        );
        let result = pitch_handler.from_absolute_pitch(&track.edited_params.pitch)?;
        let mut controllers: Vec<VocaloidControllers> = Vec::new();
        if result.is_empty() {
            return Ok(controllers);
        }
        if !result.pbs.events.is_empty() {
            let pbs_events = result
                .pbs
                .events
                .iter()
                .map(|event| VocaloidPoint::new(event.pos, event.value))
                .collect();
            controllers.push(VocaloidControllers {
                name: Some("pitchBendSens".to_owned()),
                events: pbs_events,
            });
        }
        if !result.pit.events.is_empty() {
            let pit_events = result
                .pit
                .events
                .iter()
                .map(|event| VocaloidPoint::new(event.pos, event.value))
                .collect();
            controllers.push(VocaloidControllers {
                name: Some("pitchBend".to_owned()),
                events: pit_events,
            });
        }
        Ok(controllers)
    }

    fn generate_params(
        params: &Params,
        first_bar_length: i64,
    ) -> Vec<VocaloidControllers> {
        let adapter = VprControllerAdapter::new();
        let mut controllers: Vec<VocaloidControllers> = Vec::new();
        let entries = [
            (&params.volume.points, "dynamics"),
            (&params.breath.points, "breathiness"),
            (&params.gender.points, "gender"),
            (&params.strength.points, "brightness"),
        ];
        for (point_list, param_name) in entries {
            let param_metadata = adapter.get_param_metadata(param_name);
            let curve = convert_param_points_to_vocaloid_curve(
                point_list,
                param_name,
                -first_bar_length,
                param_metadata.map(|metadata| metadata.0),
                param_metadata.map(|metadata| metadata.1),
                param_metadata.map(|metadata| metadata.2),
                false,
            );
            if let Some(controller) = adapter.create(&curve) {
                controllers.push(controller);
            }
        }
        controllers
    }
}

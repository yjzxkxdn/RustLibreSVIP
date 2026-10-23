//! Port of `libresvip.plugins.vpr.vpr_parser`.

use std::collections::HashMap;
use std::path::Path;

use libresvip_core::core::constants::{
    DEFAULT_CHINESE_LYRIC, DEFAULT_ENGLISH_LYRIC, DEFAULT_JAPANESE_LYRIC, DEFAULT_KOREAN_LYRIC,
};
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::core::warning_types::show_warning;
use libresvip_core::error::Result;
use libresvip_core::model::base::{
    InstrumentalTrack, Note, Params, Project, SingingTrack, SongTempo, TimeSignature, Track,
};
use libresvip_core::model::point::Point;
use libresvip_core::options::Options;

use super::constants::BPM_RATE;
use super::controllers::{
    convert_vocaloid_curve_to_param_points, extract_pitch_data, VprControllerAdapter,
};
use super::model::{
    language, PointValue, VocaloidNotes, VocaloidPoint, VocaloidProject, VocaloidTimeSig,
    VocaloidTrack, VocaloidVoicePart,
};
use super::pitch::{PitchBendData, VocaloidPitchHandler};

const CUT_NOTE_WARNING: &str = "Note overlap detected, cutting note ";
const SKIP_NOTE_WARNING: &str = "Note overlap detected, skipping note ";

/// `InputOptions` — the mixins of the pydantic model, in MRO order.
#[derive(Clone, Debug)]
pub struct InputOptions {
    pub import_instrumental_track: bool,
    pub import_pitch: bool,
    pub import_volume: bool,
    pub import_breath: bool,
    pub import_gender: bool,
    pub import_strength: bool,
    pub extract_audio: bool,
}

impl InputOptions {
    pub fn from_options(options: &Options) -> Self {
        Self {
            import_instrumental_track: options.bool("import_instrumental_track", true),
            import_pitch: options.bool("import_pitch", true),
            import_volume: options.bool("import_volume", true),
            import_breath: options.bool("import_breath", true),
            import_gender: options.bool("import_gender", true),
            import_strength: options.bool("import_strength", true),
            extract_audio: options.bool("extract_audio", false),
        }
    }
}

pub struct VocaloidParser<'a> {
    options: InputOptions,
    path: &'a Path,
    first_bar_length: i64,
    time_signatures: Vec<TimeSignature>,
    synchronizer: TimeSynchronizer,
    comp_id2name: HashMap<String, String>,
}

impl<'a> VocaloidParser<'a> {
    pub fn new(options: &Options, path: &'a Path) -> Self {
        Self {
            options: InputOptions::from_options(options),
            path,
            first_bar_length: 0,
            time_signatures: Vec::new(),
            synchronizer: TimeSynchronizer::from_project(&[]),
            comp_id2name: HashMap::new(),
        }
    }

    pub fn parse_project(&mut self, vpr_project: &VocaloidProject) -> Result<Project> {
        self.comp_id2name = vpr_project
            .voices
            .iter()
            .filter_map(|voice| match (&voice.comp_id, &voice.name) {
                (Some(comp_id), Some(name)) => Some((comp_id.clone(), name.clone())),
                _ => None,
            })
            .collect();
        let song_tempo_list = self.parse_tempos(&vpr_project.master_track.tempo.events);
        self.synchronizer = TimeSynchronizer::from_project(&song_tempo_list);
        let time_signatures =
            self.parse_time_signatures(&vpr_project.master_track.time_sig.events)?;
        self.time_signatures = time_signatures;
        Ok(Project {
            version: String::new(),
            song_tempo_list,
            time_signature_list: self.time_signatures.clone(),
            track_list: self.parse_tracks(&vpr_project.tracks)?,
        })
    }

    fn parse_time_signatures(
        &mut self,
        vocaloid_time_signatures: &[VocaloidTimeSig],
    ) -> Result<Vec<TimeSignature>> {
        let time_signatures: Vec<TimeSignature> = vocaloid_time_signatures
            .iter()
            .map(|time_signature| {
                TimeSignature::new(
                    time_signature.bar,
                    time_signature.numer,
                    time_signature.denom,
                )
            })
            .collect();
        let Some(first) = time_signatures.first() else {
            // Python raises IndexError on `time_signatures[0]`.
            return Err(libresvip_core::error::LibreSvipError::msg(
                "time signature list is empty",
            ));
        };
        self.first_bar_length = first.bar_length().round_ties_even() as i64;
        Ok(time_signatures)
    }

    fn parse_tempos(&self, tempos: &[VocaloidPoint]) -> Vec<SongTempo> {
        tempos
            .iter()
            .filter_map(|tempo| {
                let value = tempo.value.as_ref()?;
                let bpm = value.as_f64()?;
                Some(SongTempo::new(tempo.pos, bpm / BPM_RATE))
            })
            .collect()
    }

    fn parse_tracks(&mut self, tracks: &[VocaloidTrack]) -> Result<Vec<Track>> {
        let mut track_list: Vec<Track> = Vec::new();
        for track in tracks {
            match track {
                VocaloidTrack::Audio(track) => {
                    if self.options.import_instrumental_track {
                        for part in &track.parts {
                            let Some(wav) = &part.wav else { continue };
                            let audio_file_path = match self.path.parent() {
                                Some(parent) => parent
                                    .join(wav.original_name.as_deref().unwrap_or(&wav.name))
                                    .display()
                                    .to_string(),
                                None => wav.original_name.clone().unwrap_or_else(|| wav.name.clone()),
                            };
                            track_list.push(Track::Instrumental(InstrumentalTrack {
                                title: part.name.clone().unwrap_or_default(),
                                offset: part.pos,
                                mute: track.is_muted,
                                solo: track.is_solo_mode,
                                audio_file_path,
                                ..Default::default()
                            }));
                        }
                    }
                }
                VocaloidTrack::Standard(track) => {
                    self.parse_voice_parts(&track.parts, track.is_muted, track.is_solo_mode, &mut track_list)?;
                }
                VocaloidTrack::Ai(track) => {
                    self.parse_voice_parts(&track.parts, track.is_muted, track.is_solo_mode, &mut track_list)?;
                }
            }
        }
        Ok(track_list)
    }

    fn parse_voice_parts(
        &mut self,
        parts: &[VocaloidVoicePart],
        is_muted: bool,
        is_solo_mode: bool,
        track_list: &mut Vec<Track>,
    ) -> Result<()> {
        for part in parts {
            let mut comp_id = String::new();
            let mut supported_lang_ids: Vec<i64> = Vec::new();
            if let Some(voice) = &part.voice {
                comp_id = voice.comp_id.clone().unwrap_or_default();
                if let Some(lang_id) = voice.lang_id {
                    supported_lang_ids.push(lang_id);
                }
            } else if let Some(ai_voice) = &part.ai_voice {
                comp_id = ai_voice.comp_id.clone().unwrap_or_default();
                supported_lang_ids.extend(
                    ai_voice
                        .lang_ids
                        .iter()
                        .filter_map(|lang_id| lang_id.lang_id),
                );
            }
            let main_lang_id = supported_lang_ids
                .first()
                .copied()
                .unwrap_or(language::SIMPLIFIED_CHINESE);
            let default_lyric = if main_lang_id == language::JAPANESE {
                DEFAULT_JAPANESE_LYRIC
            } else if main_lang_id == language::KOREAN {
                DEFAULT_KOREAN_LYRIC
            } else if main_lang_id == language::SIMPLIFIED_CHINESE {
                DEFAULT_CHINESE_LYRIC
            } else {
                DEFAULT_ENGLISH_LYRIC
            };
            let (note_list, direct_pitch_points) =
                self.parse_notes(&part.notes, part.pos, default_lyric);
            let mut singing_track = SingingTrack {
                title: part.name.clone().unwrap_or_default(),
                mute: is_muted,
                solo: is_solo_mode,
                note_list,
                ai_singer_name: self.comp_id2name.get(&comp_id).cloned().unwrap_or_default(),
                ..Default::default()
            };
            if self.options.import_pitch {
                if !direct_pitch_points.is_empty() {
                    singing_track
                        .edited_params
                        .pitch
                        .points
                        .extend(direct_pitch_points);
                } else if let Some((pit_curve, pbs_curve)) = extract_pitch_data(part) {
                    let pitch_handler = VocaloidPitchHandler::new(
                        self.synchronizer.clone(),
                        &singing_track.note_list,
                        &self.time_signatures,
                        self.first_bar_length,
                    );
                    let pitch_data = PitchBendData {
                        pit: pit_curve,
                        pbs: pbs_curve,
                    };
                    if let Some(part_pitch) =
                        pitch_handler.to_absolute_pitch(&[pitch_data], Some(&[part.pos]))?
                    {
                        singing_track.edited_params.pitch = part_pitch;
                    }
                }
            }
            self.parse_params(part, &mut singing_track.edited_params, part.pos);
            track_list.push(Track::Singing(singing_track));
        }
        Ok(())
    }

    fn parse_notes(
        &self,
        notes: &[VocaloidNotes],
        pos: i64,
        default_lyric: &str,
    ) -> (Vec<Note>, Vec<Point>) {
        let mut note_list: Vec<Note> = Vec::new();
        let mut pitch_points: Vec<Point> = Vec::new();
        if !notes.is_empty() {
            let mut next_pos: Option<i64> = None;
            for note in notes.iter().rev() {
                let mut normalized_duration = note.duration.unwrap_or(0);
                if self.options.import_pitch {
                    if let Some(direct_pitches) = note.direct_pitches.as_deref() {
                        let mut note_pitch_points: Vec<Point> = Vec::new();
                        for pitch in direct_pitches {
                            let duration = note.duration.unwrap_or(0);
                            if pitch.pos < 0 || (duration != 0 && pitch.pos > duration) {
                                continue;
                            }
                            match pitch.value.as_ref().and_then(PointValue::as_py_int) {
                                Some(value) => {
                                    if note_pitch_points.is_empty()
                                        || note_pitch_points.last().map(|point| point.y) == Some(-100)
                                    {
                                        note_pitch_points.push(Point::new(
                                            note.pos + pos + pitch.pos + self.first_bar_length,
                                            -100,
                                        ));
                                    }
                                    note_pitch_points.push(Point::new(
                                        note.pos + pos + pitch.pos + self.first_bar_length,
                                        value + note.number * 100,
                                    ));
                                }
                                None => {
                                    note_pitch_points.push(Point::new(
                                        note.pos + pos + pitch.pos + self.first_bar_length,
                                        note.number * 100,
                                    ));
                                    note_pitch_points.push(Point::new(
                                        note.pos + pos + pitch.pos + self.first_bar_length,
                                        -100,
                                    ));
                                }
                            }
                        }
                        note_pitch_points.extend(pitch_points);
                        pitch_points = note_pitch_points;
                    }
                }
                if let Some(next_pos) = next_pos {
                    let distance = next_pos - note.pos;
                    if distance < normalized_duration {
                        normalized_duration = distance;
                        if normalized_duration > 0 {
                            show_warning(format!("{CUT_NOTE_WARNING}{}", note.lyric));
                        }
                    }
                }
                if normalized_duration > 0 {
                    note_list.insert(
                        0,
                        Note {
                            start_pos: note.pos + pos,
                            length: normalized_duration,
                            key_number: note.number,
                            lyric: if note.lyric.is_empty() {
                                default_lyric.to_owned()
                            } else {
                                note.lyric.clone()
                            },
                            pronunciation: None,
                            ..Default::default()
                        },
                    );
                } else {
                    show_warning(format!("{SKIP_NOTE_WARNING}{}", note.lyric));
                }
                next_pos = Some(note.pos);
            }
        }
        if !pitch_points.is_empty() {
            pitch_points.insert(0, Point::start_point());
            pitch_points.push(Point::end_point());
        }
        (note_list, pitch_points)
    }

    fn parse_params(&self, part: &VocaloidVoicePart, params: &mut Params, offset: i64) {
        let adapter = VprControllerAdapter::new();
        let position_offset = offset + self.first_bar_length;

        if self.options.import_volume {
            if let Some(curve) = adapter.extract(part, "dynamics") {
                params.volume.points.extend(convert_vocaloid_curve_to_param_points(
                    &curve,
                    position_offset,
                    false,
                ));
            }
        }
        if self.options.import_breath {
            if let Some(curve) = adapter.extract(part, "breathiness") {
                params.breath.points.extend(convert_vocaloid_curve_to_param_points(
                    &curve,
                    position_offset,
                    false,
                ));
            }
        }
        if self.options.import_gender {
            if let Some(curve) = adapter.extract(part, "gender") {
                params.gender.points.extend(convert_vocaloid_curve_to_param_points(
                    &curve,
                    position_offset,
                    false,
                ));
            }
        }
        if self.options.import_strength {
            if let Some(curve) = adapter.extract(part, "brightness") {
                params.strength.points.extend(convert_vocaloid_curve_to_param_points(
                    &curve,
                    position_offset,
                    false,
                ));
            }
        }
    }
}

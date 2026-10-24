//! Port of `libresvip.plugins.acep.ace_studio_parser`.

use libresvip_core::core::lyric_phoneme::chinese::get_pinyin_series;
use libresvip_core::core::tick_counter::shift_tempo_list;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::core::warning_types::{show_warning_kind, WarningKind};
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{
    InstrumentalTrack, Note, ParamCurve, Params, Phones, Project, SingingTrack, SongTempo,
    TimeSignature, Track,
};
use libresvip_core::model::point::Point;
use regex::Regex;
use std::sync::OnceLock;

use super::base_pitch_curve::BasePitchCurve;
use super::model::{
    AcepNote, AcepParamCurve, AcepParamCurveList, AcepParams, AcepProject, AcepTempo,
    AcepTimeSignature, AcepTrack, AcepVocalTrack, LYRICS_LANGUAGE_CHINESE,
    LATIN_LYRICS_LANGUAGES,
};
use super::music_math::clamp;
use super::options::{InputOptions, NormalizationArgument, NORMALIZATION_MINMAX, NORMALIZATION_ZSCORE};
use super::singers::id_to_singer;

fn latin_span_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"#(\d+)$").expect("valid ACEP latin span regex"))
}

/// `get_pinyin_series(<str>)`: Python iterates the *characters* of the string.
fn pinyin_first(lyric: &str) -> Result<Option<String>> {
    let mut parts: Vec<&str> = Vec::with_capacity(lyric.len());
    for (index, character) in lyric.char_indices() {
        parts.push(&lyric[index..index + character.len_utf8()]);
    }
    Ok(get_pinyin_series(&parts)?.into_iter().next())
}

pub struct AceParser {
    options: InputOptions,
    content_version: i64,
    synchronizer: TimeSynchronizer,
    first_bar_ticks: i64,
}

impl AceParser {
    pub fn new(options: InputOptions) -> Self {
        Self {
            options,
            content_version: 0,
            synchronizer: TimeSynchronizer::from_project(&[]),
            first_bar_ticks: 0,
        }
    }

    pub fn parse_project(&mut self, ace_project: &mut AcepProject) -> Result<Project> {
        let mut project = Project::default();
        self.content_version = ace_project.version;
        project.time_signature_list = parse_time_signatures(&ace_project.time_signatures);
        if project.time_signature_list.is_empty() {
            return Err(LibreSvipError::msg(
                "ACE Studio project contains no time signature",
            ));
        }
        self.first_bar_ticks = project.time_signature_list[0].bar_length() as i64;
        project.song_tempo_list = shift_tempo_list(
            &parse_tempos(&ace_project.tempos),
            self.first_bar_ticks,
        );
        self.synchronizer = TimeSynchronizer::from_project(&project.song_tempo_list);
        let master_gain = ace_project.master.gain;
        for ace_track in ace_project.tracks.iter_mut() {
            ace_track.properties_mut().gain += master_gain;
        }
        for ace_track in ace_project.tracks.iter() {
            if let Some(track) = self.parse_track(ace_track)? {
                project.track_list.push(track);
            }
        }
        Ok(project)
    }

    fn parse_track(&self, ace_track: &AcepTrack) -> Result<Option<Track>> {
        let mut track = if self.options.import_instrumental_track {
            match ace_track.as_audio() {
                Some(audio_track) if !audio_track.patterns.is_empty() => {
                    let pattern = &audio_track.patterns[0];
                    let offset = if self.content_version < 7 {
                        pattern.base.pos as i64
                    } else {
                        self.synchronizer.get_actual_ticks_from_secs(pattern.base.pos) as i64
                    };
                    Track::Instrumental(InstrumentalTrack {
                        audio_file_path: pattern.path.clone(),
                        offset,
                        ..Default::default()
                    })
                }
                _ => match ace_track.as_vocal() {
                    Some(vocal_track) => self.parse_vocal_track(vocal_track)?,
                    None => return Ok(None),
                },
            }
        } else {
            match ace_track.as_vocal() {
                Some(vocal_track) => self.parse_vocal_track(vocal_track)?,
                None => return Ok(None),
            }
        };
        let properties = ace_track.properties();
        match &mut track {
            Track::Singing(singing) => singing.title = properties.name.clone(),
            Track::Instrumental(instrumental) => instrumental.title = properties.name.clone(),
        }
        match &mut track {
            Track::Singing(singing) => {
                singing.mute = properties.mute;
                singing.solo = properties.solo;
                singing.volume = 10f64.powf(properties.gain / 20.0);
            }
            Track::Instrumental(instrumental) => {
                instrumental.mute = properties.mute;
                instrumental.solo = properties.solo;
                instrumental.volume = 10f64.powf(properties.gain / 20.0);
            }
        }
        Ok(Some(track))
    }

    fn parse_vocal_track(&self, ace_track: &AcepVocalTrack) -> Result<Track> {
        let singer_id = ace_track
            .singers
            .first()
            .ok_or_else(|| LibreSvipError::msg("ACE Studio vocal track has no singer"))?
            .singer
            .singer_id;
        let mut track = SingingTrack {
            ai_singer_name: id_to_singer(singer_id).unwrap_or("").to_owned(),
            ..Default::default()
        };
        let mut ace_note_list: Vec<AcepNote> = Vec::new();
        let mut ace_params = AcepParams::default();
        let mut patterns: Vec<&super::model::AcepVocalPattern> = ace_track.patterns.iter().collect();
        patterns.sort_by(|left, right| {
            left.base
                .clip_pos
                .partial_cmp(&right.base.clip_pos)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for pattern in patterns {
            let pattern_pos = pattern.base.pos as i64;
            let clip_pos = pattern.base.clip_pos as i64;
            let clip_end = (pattern.base.clip_pos + pattern.base.clip_dur) as i64;
            let mut ace_notes: Vec<AcepNote> = Vec::new();
            for note in pattern.notes.iter() {
                let note_start = note.pos;
                let note_end = note.pos + note.dur;
                let mut clipped_start = note_start.max(clip_pos);
                let clipped_end = note_end.min(clip_end);
                if clipped_start >= clipped_end {
                    continue;
                }
                let mut visible_pos = clipped_start + pattern_pos;
                if visible_pos < 0 {
                    clipped_start -= visible_pos;
                    visible_pos = 0;
                    if clipped_start >= clipped_end {
                        continue;
                    }
                }
                let mut clone = note.clone();
                clone.pos = visible_pos;
                clone.dur = clipped_end - clipped_start;
                ace_notes.push(clone);
            }
            if ace_notes.is_empty() {
                continue;
            }
            for index in 1..ace_notes.len() {
                let (previous, current) = ace_notes.split_at_mut(index);
                let previous = &mut previous[index - 1];
                if previous.pos + previous.dur > current[0].pos {
                    previous.dur = current[0].pos - previous.pos;
                }
            }
            ace_note_list.extend(ace_notes);

            merge_curves(
                &pattern.parameters.pitch_delta,
                &mut ace_params.pitch_delta,
                clip_pos,
                clip_end,
                pattern_pos,
                self.first_bar_ticks,
            );
            merge_curves(
                &pattern.parameters.breathiness,
                &mut ace_params.breathiness,
                clip_pos,
                clip_end,
                pattern_pos,
                self.first_bar_ticks,
            );
            merge_curves(
                &pattern.parameters.gender,
                &mut ace_params.gender,
                clip_pos,
                clip_end,
                pattern_pos,
                self.first_bar_ticks,
            );
            merge_curves(
                &pattern.parameters.energy,
                &mut ace_params.energy,
                clip_pos,
                clip_end,
                pattern_pos,
                self.first_bar_ticks,
            );
            merge_curves(
                &pattern.parameters.tension,
                &mut ace_params.tension,
                clip_pos,
                clip_end,
                pattern_pos,
                self.first_bar_ticks,
            );
            if self.options.breath_normalization.enabled() {
                merge_curves(
                    &pattern.parameters.real_breathiness,
                    &mut ace_params.real_breathiness,
                    clip_pos,
                    clip_end,
                    pattern_pos,
                    self.first_bar_ticks,
                );
            }
            if self.options.tension_normalization.enabled() {
                merge_curves(
                    &pattern.parameters.real_tension,
                    &mut ace_params.real_tension,
                    clip_pos,
                    clip_end,
                    pattern_pos,
                    self.first_bar_ticks,
                );
            }
            if self.options.energy_normalization.enabled() {
                merge_curves(
                    &pattern.parameters.real_energy,
                    &mut ace_params.real_energy,
                    clip_pos,
                    clip_end,
                    pattern_pos,
                    self.first_bar_ticks,
                );
            }
        }
        ace_note_list.sort_by_key(|note| note.pos);
        ace_params
            .pitch_delta
            .0
            .sort_by_key(|curve| curve.offset);
        ace_params
            .breathiness
            .0
            .sort_by_key(|curve| curve.offset);
        ace_params.gender.0.sort_by_key(|curve| curve.offset);
        ace_params.energy.0.sort_by_key(|curve| curve.offset);
        ace_params.tension.0.sort_by_key(|curve| curve.offset);
        if self.options.breath_normalization.enabled() {
            ace_params
                .real_breathiness
                .0
                .sort_by_key(|curve| curve.offset);
        }
        if self.options.tension_normalization.enabled() {
            ace_params
                .real_tension
                .0
                .sort_by_key(|curve| curve.offset);
        }
        if self.options.energy_normalization.enabled() {
            ace_params.real_energy.0.sort_by_key(|curve| curve.offset);
        }
        track.note_list = ace_note_list
            .iter()
            .map(|ace_note| self.parse_note(ace_note))
            .collect::<Result<Vec<Note>>>()?;
        track.edited_params = self.parse_params(&mut ace_params, &ace_note_list)?;
        Ok(Track::Singing(track))
    }

    fn parse_note(&self, ace_note: &AcepNote) -> Result<Note> {
        let pronunciation = if !self.options.keep_all_pronunciation
            && ace_note.language == LYRICS_LANGUAGE_CHINESE
        {
            pinyin_first(&ace_note.lyric)?
        } else {
            None
        };
        let mut note = Note {
            key_number: ace_note.pitch,
            start_pos: ace_note.pos,
            length: ace_note.dur,
            lyric: ace_note.lyric.clone(),
            ..Default::default()
        };
        let is_latin = LATIN_LYRICS_LANGUAGES.contains(&ace_note.language.as_str());
        if is_latin {
            if let Some(captures) = latin_span_regex().captures(&note.lyric) {
                let span_index: i64 = captures
                    .get(1)
                    .and_then(|group| group.as_str().parse().ok())
                    .unwrap_or(0);
                note.lyric = if span_index == 1 {
                    latin_span_regex().replace(&note.lyric, "").into_owned()
                } else {
                    "+".to_owned()
                };
            }
        }
        if !ace_note.syllable.is_empty()
            && Some(ace_note.syllable.as_str()) != ace_note.freezed_default_syllable.as_deref()
        {
            note.pronunciation = Some(ace_note.syllable.clone());
        } else if pronunciation.is_none()
            || (!ace_note.lyric.contains('-')
                && ace_note.pronunciation.as_deref() != pronunciation.as_deref())
        {
            note.pronunciation = ace_note.pronunciation.clone();
        }
        if ace_note.br_len > 0.0 {
            note.head_tag = Some("V".to_owned());
        }
        if let Some(head_consonants) = ace_note.head_consonants.as_ref() {
            if !head_consonants.is_empty() {
                let head_length = if self.content_version < 7 {
                    self.synchronizer.get_duration_secs_from_ticks(
                        note.start_pos - head_consonants[0] as i64,
                        note.start_pos,
                    )
                } else {
                    head_consonants[0]
                };
                note.edited_phones = Some(Phones {
                    head_length_in_secs: head_length,
                    ..Default::default()
                });
            }
        }
        Ok(note)
    }

    fn parse_params(
        &self,
        ace_params: &mut AcepParams,
        ace_note_list: &[AcepNote],
    ) -> Result<Params> {
        if self.options.breath_normalization.enabled() {
            let normalization = &self.options.breath_normalization;
            let mut normalized = ace_params
                .real_breathiness
                .exclude(|value| excluded(value, normalization));
            apply_normalization(&mut normalized, normalization);
            ace_params.breathiness = ace_params.breathiness.plus(
                Some(&normalized),
                1.0,
                |value| if value >= 0.0 { value * 1.5 } else { value * 0.8 },
            );
        }
        if self.options.tension_normalization.enabled() {
            let normalization = &self.options.tension_normalization;
            let mut normalized = ace_params
                .real_tension
                .exclude(|value| excluded(value, normalization));
            apply_normalization(&mut normalized, normalization);
            ace_params.tension = ace_params.tension.plus(
                Some(&normalized),
                1.0,
                |value| if value >= 0.0 { value * 0.5 } else { value * 0.3 },
            );
        }
        if self.options.energy_normalization.enabled() {
            let normalization = &self.options.energy_normalization;
            let mut normalized = ace_params
                .real_energy
                .exclude(|value| excluded(value, normalization));
            apply_normalization(&mut normalized, normalization);
            ace_params.energy = ace_params
                .energy
                .plus(Some(&normalized), 1.0, |value| value);
        }

        let mut parameters = Params::default();
        if self.options.import_pitch {
            parameters.pitch = self.parse_pitch_curve(&ace_params.pitch_delta, ace_note_list);
        }
        if self.options.import_breath {
            let transform = linear_transform(0.2, 1.0, 2.5);
            parameters.breath =
                self.parse_param_curve(&ace_params.breathiness, &transform);
        }
        if self.options.import_gender {
            let transform = linear_transform(-1.0, 0.0, 1.0);
            parameters.gender = self.parse_param_curve(&ace_params.gender, &transform);
        }
        let energy_coefficient = self.options.energy_coefficient;
        if self.options.import_tension && self.options.import_energy {
            let transform = linear_transform(0.0, 1.0, 2.0);
            parameters.volume = self.parse_param_curve(&ace_params.energy, &|value| {
                (energy_coefficient * transform(value) as f64).round_ties_even() as i64
            });
            let remaining_energy = AcepParamCurveList(
                ace_params
                    .energy
                    .0
                    .iter()
                    .map(|curve| {
                        let mut copy = curve.clone();
                        copy.values = curve
                            .values
                            .iter()
                            .map(|value| {
                                value.map(|value| {
                                    (value - 1.0) * (1.0 - energy_coefficient) + 1.0
                                })
                            })
                            .collect();
                        copy
                    })
                    .collect(),
            );
            let energy_plus_tension = remaining_energy.plus(
                Some(&ace_params.tension),
                1.0,
                |value| {
                    if value >= 1.0 {
                        (value - 1.0) * 0.5
                    } else {
                        (value - 1.0) * 0.3
                    }
                },
            );
            parameters.strength = self.parse_param_curve(&energy_plus_tension, &|value| {
                (energy_coefficient * transform(value) as f64).round_ties_even() as i64
            });
        } else if self.options.import_tension {
            let transform = linear_transform(0.7, 1.0, 1.5);
            parameters.strength = self.parse_param_curve(&ace_params.tension, &transform);
        } else if self.options.import_energy {
            let transform = linear_transform(0.0, 1.0, 2.0);
            parameters.volume = self.parse_param_curve(&ace_params.energy, &|value| {
                (energy_coefficient * transform(value) as f64).round_ties_even() as i64
            });
            parameters.strength = self.parse_param_curve(&ace_params.energy, &|value| {
                ((1.0 - energy_coefficient) * transform(value) as f64).round_ties_even() as i64
            });
        }
        Ok(parameters)
    }

    fn parse_pitch_curve(
        &self,
        ace_curves: &AcepParamCurveList,
        ace_note_list: &[AcepNote],
    ) -> ParamCurve {
        let mut curve = ParamCurve::default();
        curve.points.push(Point::start_point());
        if !ace_curves.0.is_empty() {
            let base_pitch = BasePitchCurve::new(ace_note_list, &self.synchronizer, 0, 0.0);
            for ace_curve in ace_curves.0.iter() {
                let mut pos = ace_curve.offset;
                curve
                    .points
                    .push(Point::new(pos + self.first_bar_ticks, -100));
                for value in ace_curve.values.iter() {
                    if ace_curve.curve_type == "anchor" {
                        if let Some(value) = value {
                            curve.points.push(Point::new(
                                pos + self.first_bar_ticks,
                                (value * 100.0).round_ties_even() as i64,
                            ));
                        }
                    } else if let Some(value) = value {
                        if !value.is_nan() {
                            let abs_semitone = base_pitch.semitone_value_at(
                                self.synchronizer.get_actual_secs_from_ticks(pos),
                            ) + value;
                            curve.points.push(Point::new(
                                pos + self.first_bar_ticks,
                                (abs_semitone * 100.0).round_ties_even() as i64,
                            ));
                        }
                    }
                    pos += 1;
                }
                curve
                    .points
                    .push(Point::new(pos - 1 + self.first_bar_ticks, -100));
            }
        }
        curve.points.push(Point::end_point());
        if self.options.curve_sample_interval > 0 {
            curve = curve.reduce_sample_rate(self.options.curve_sample_interval, -100);
        }
        curve
    }

    fn parse_param_curve(
        &self,
        ace_curves: &AcepParamCurveList,
        mapping_func: &dyn Fn(f64) -> i64,
    ) -> ParamCurve {
        let mut curve = ParamCurve::default();
        curve.points.push(Point::start_point_with(0));
        for ace_curve in ace_curves.0.iter() {
            let mut pos = ace_curve.offset;
            curve
                .points
                .push(Point::new(pos + self.first_bar_ticks, 0));
            for value in ace_curve.values.iter() {
                if let Some(value) = value {
                    curve.points.push(Point::new(
                        pos + self.first_bar_ticks,
                        mapping_func(*value),
                    ));
                }
                pos += 1;
            }
            curve
                .points
                .push(Point::new(pos - 1 + self.first_bar_ticks, 0));
        }
        curve.points.push(Point::end_point_with(0));
        if self.options.curve_sample_interval > 0 {
            curve = curve.reduce_sample_rate(self.options.curve_sample_interval, 0);
        }
        curve
    }
}

fn excluded(value: f64, normalization: &NormalizationArgument) -> bool {
    value + 1e-3 < normalization.lower_threshold || value - 1e-3 > normalization.upper_threshold
}

fn apply_normalization(curves: &mut AcepParamCurveList, normalization: &NormalizationArgument) {
    match normalization.normalize_method.as_str() {
        NORMALIZATION_ZSCORE => {
            *curves = curves.z_score_normalize(normalization.scale, normalization.bias);
        }
        NORMALIZATION_MINMAX => {
            *curves = curves.minmax_normalize(normalization.scale, normalization.bias);
        }
        _ => show_warning_kind(WarningKind::FallbackApplied, format!(
            "Unknown normalization method: {}",
            normalization.normalize_method
        )),
    }
}

fn linear_transform(lower_bound: f64, middle_value: f64, upper_bound: f64) -> impl Fn(f64) -> i64 {
    move |x: f64| {
        let value = if x >= middle_value {
            (x - middle_value) / (upper_bound - middle_value) * 1000.0
        } else {
            (x - middle_value) / (middle_value - lower_bound) * 1000.0
        };
        clamp(value, Some(-1000.0), Some(1000.0)).round_ties_even() as i64
    }
}

fn merge_curves(
    source: &AcepParamCurveList,
    destination: &mut AcepParamCurveList,
    clip_pos: i64,
    clip_end: i64,
    pattern_pos: i64,
    first_bar_ticks: i64,
) {
    for ace_curve in source.0.iter() {
        let curve_start = ace_curve.offset;
        let curve_end = ace_curve.offset + ace_curve.values.len() as i64;
        let mut clipped_start = curve_start.max(clip_pos);
        let clipped_end = curve_end.min(clip_end);
        if clipped_start >= clipped_end {
            continue;
        }
        let mut dst_offset = clipped_start + pattern_pos;
        if dst_offset < -first_bar_ticks {
            clipped_start += -first_bar_ticks - dst_offset;
            dst_offset = -first_bar_ticks;
        }
        if clipped_start >= clipped_end {
            continue;
        }
        let start_index = (clipped_start - curve_start) as usize;
        let end_index = (clipped_end - curve_start) as usize;
        let mut copy: AcepParamCurve = ace_curve.clone();
        copy.offset = dst_offset;
        copy.values = ace_curve.values[start_index..end_index].to_vec();
        destination.0.push(copy);
    }
}

pub fn parse_time_signatures(ace_time_signatures: &[AcepTimeSignature]) -> Vec<TimeSignature> {
    ace_time_signatures
        .iter()
        .map(|ace_time_signature| {
            TimeSignature::new(
                ace_time_signature.bar_pos,
                ace_time_signature.numerator,
                ace_time_signature.denominator,
            )
        })
        .collect()
}

pub fn parse_tempos(ace_tempos: &[AcepTempo]) -> Vec<SongTempo> {
    ace_tempos
        .iter()
        .map(|ace_tempo| SongTempo::new(ace_tempo.position, ace_tempo.bpm))
        .collect()
}

//! Port of `libresvip.plugins.svp.synthv_parser`.

use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;

use libresvip_core::core::constants::DEFAULT_BPM;
use libresvip_core::core::tick_counter::{
    find_bar_index, shift_beat_list, shift_tempo_list,
};
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{
    InstrumentalTrack, Note, ParamCurve, Params, Phones, Project, SingingTrack, SongTempo,
    TimeSignature, Track,
};
use libresvip_core::model::point::Point;

use crate::svp::math_utils::{
    db_to_float, find_index, position_to_ticks, py_round, ratio_to_db, Interpolation,
    RangeInterval,
};
use crate::svp::model::{
    SVDatabase, SVGroup, SVMeter, SVNote, SVParamCurve, SVParameters, SVPitchControl, SVProject,
    SVTempo, SVTrack, SVVoice,
};
use crate::svp::options::{BreathOption, GroupOption, InputOptions, PitchOption};
use crate::svp::param_expression::{CurveGenerator, ParamExpr, PitchGenerator};
use crate::svp::phoneme_utils::{default_phone_marks, sv_g2p, xsampa2pinyin};
use crate::svp::track_merge_utils::track_override_with;

fn breath_pattern() -> Option<&'static Regex> {
    static PATTERN: OnceLock<Option<Regex>> = OnceLock::new();
    PATTERN
        .get_or_init(|| Regex::new(r"^\s*\.?\s*br(l?[1-9])?\s*$").ok())
        .as_ref()
}

fn is_breath_note(lyric: &str) -> bool {
    breath_pattern()
        .map(|pattern| pattern.is_match(lyric))
        .unwrap_or(false)
}

pub struct SynthVParser {
    options: InputOptions,
    first_bar_tick: i64,
    first_bpm: f64,
    synchronizer: TimeSynchronizer,
    voice_settings: SVVoice,
    instant_pitch: SVParamCurve,
    group_library: HashMap<String, SVGroup>,
    group_split_counts: HashMap<String, usize>,
    tracks_from_groups: Vec<Track>,
    time_signatures: Vec<TimeSignature>,
}

impl SynthVParser {
    pub fn new(options: InputOptions) -> Self {
        Self {
            options,
            first_bar_tick: 0,
            first_bpm: DEFAULT_BPM,
            synchronizer: TimeSynchronizer::from_project(&[]),
            voice_settings: SVVoice::default(),
            instant_pitch: SVParamCurve::default(),
            group_library: HashMap::new(),
            group_split_counts: HashMap::new(),
            tracks_from_groups: Vec::new(),
            time_signatures: Vec::new(),
        }
    }

    fn clip(value: i64) -> i64 {
        value.clamp(-1000, 1000)
    }

    fn actual_value_at(&self, expression: &ParamExpr, mapping: &dyn Fn(f64) -> i64, ticks: i64) -> i64 {
        Self::clip(mapping(expression.value_at_ticks(ticks) / 1000.0))
    }

    fn parse_audio_offset(&self, offset: i64) -> i64 {
        if offset >= 0 {
            return position_to_ticks(offset);
        }
        py_round(offset as f64 / crate::svp::constants::TICK_RATE as f64 * self.first_bpm / DEFAULT_BPM)
    }

    fn parse_meter(meter: &SVMeter) -> TimeSignature {
        TimeSignature::new(meter.index, meter.numerator, meter.denominator)
    }

    fn parse_tempo(tempo: &SVTempo) -> SongTempo {
        SongTempo::new(position_to_ticks(tempo.position), tempo.bpm)
    }

    fn parse_volume(gain: f64) -> f64 {
        if gain >= 0.0 {
            (gain / ratio_to_db(4.0) + 1.0).min(2.0)
        } else {
            db_to_float(gain)
        }
    }

    fn curve_points(&self, sv_curve: &SVParamCurve, mapping: &dyn Fn(f64) -> i64, base_value: f64) -> Vec<Point> {
        sv_curve
            .points
            .iter()
            .map(|point| {
                Point::new(
                    position_to_ticks(point.offset) + self.first_bar_tick,
                    mapping(point.value + base_value),
                )
            })
            .collect()
    }

    fn clipped_curve(&self, points: Vec<Point>) -> ParamCurve {
        ParamCurve::new(
            points
                .into_iter()
                .map(|point| Point::new(point.x, Self::clip(point.y)))
                .collect(),
        )
    }

    fn parse_param_curve(
        &self,
        sv_curve: &SVParamCurve,
        mapping: &dyn Fn(f64) -> i64,
        base_value: Option<f64>,
        master_curve: Option<&SVParamCurve>,
    ) -> ParamCurve {
        let base_value = base_value.unwrap_or(0.0);
        let interpolation = Interpolation::from_mode(&sv_curve.mode);
        let decoded_base_value = mapping(base_value);
        let generator = CurveGenerator::new(
            self.curve_points(sv_curve, mapping, base_value),
            interpolation,
            decoded_base_value,
            None,
        );
        let Some(master_curve) = master_curve else {
            return self.clipped_curve(generator.get_converted_curve(5));
        };
        if master_curve.points.is_empty() {
            return self.clipped_curve(generator.get_converted_curve(5));
        }
        if sv_curve.points.is_empty() {
            let master_generator = CurveGenerator::new(
                self.curve_points(master_curve, mapping, base_value),
                interpolation,
                decoded_base_value,
                None,
            );
            return self.clipped_curve(master_generator.get_converted_curve(5));
        }
        let group_expr = CurveGenerator::new(
            sv_curve
                .points
                .iter()
                .map(|point| {
                    Point::new(
                        position_to_ticks(point.offset) + self.first_bar_tick,
                        py_round(point.value * 1000.0),
                    )
                })
                .collect::<Vec<_>>(),
            Interpolation::from_mode(&sv_curve.mode),
            py_round(base_value * 1000.0),
            None,
        );
        let master_expr = CurveGenerator::new(
            master_curve
                .points
                .iter()
                .map(|point| {
                    Point::new(
                        position_to_ticks(point.offset) + self.first_bar_tick,
                        py_round(point.value * 1000.0),
                    )
                })
                .collect::<Vec<_>>(),
            Interpolation::from_mode(&master_curve.mode),
            0,
            None,
        );
        let compound_expr = ParamExpr::Curve(group_expr.clone()).add(ParamExpr::Curve(master_expr.clone()));
        let group_points = &group_expr.point_list;
        let master_points = &master_expr.point_list;
        let mut curve = ParamCurve::default();
        if group_points.is_empty() || master_points.is_empty() {
            // Both are non-empty here, but stay defensive (Python would raise).
            return self.clipped_curve(compound_expr_curve(&compound_expr, group_points, master_points));
        }
        let mut i = 0usize;
        let mut j = 0usize;
        let mut prev_point;
        let mut prev_point_is_base;
        if group_points[0].x < master_points[0].x {
            prev_point = group_points[0];
            i += 1;
            prev_point_is_base = prev_point.y == py_round(base_value * 1000.0);
        } else {
            prev_point = master_points[0];
            j += 1;
            prev_point_is_base = prev_point.y == 0;
        }
        curve.points.push(Point::start_point_with(self.actual_value_at(
            &compound_expr,
            mapping,
            0,
        )));
        curve.points.push(Point::new(
            prev_point.x,
            self.actual_value_at(&compound_expr, mapping, prev_point.x),
        ));
        while i < group_points.len() || j < master_points.len() {
            let current_point;
            let current_point_is_base;
            if i < group_points.len()
                && (j >= master_points.len() || group_points[i].x <= master_points[j].x)
            {
                current_point = group_points[i];
                i += 1;
                current_point_is_base = current_point.y == py_round(base_value * 1000.0);
            } else {
                current_point = master_points[j];
                j += 1;
                current_point_is_base = current_point.y == 0;
            }
            if prev_point_is_base && current_point_is_base && prev_point.x <= current_point.x {
                curve.points.push(Point::new(
                    prev_point.x,
                    self.actual_value_at(&compound_expr, mapping, prev_point.x),
                ));
                curve.points.push(Point::new(
                    current_point.x,
                    self.actual_value_at(&compound_expr, mapping, current_point.x),
                ));
            } else {
                let mut position = prev_point.x;
                while position < current_point.x {
                    curve.points.push(Point::new(
                        position,
                        self.actual_value_at(&compound_expr, mapping, position),
                    ));
                    position += 5;
                }
            }
            prev_point = current_point;
            prev_point_is_base = current_point_is_base;
        }
        curve.points.push(Point::new(
            prev_point.x,
            self.actual_value_at(&compound_expr, mapping, prev_point.x),
        ));
        curve.points.push(Point::end_point_with(self.actual_value_at(
            &compound_expr,
            mapping,
            prev_point.x,
        )));
        curve
    }

    #[allow(clippy::too_many_arguments)]
    fn parse_pitch_curve(
        &self,
        pitch_diff: &SVParamCurve,
        vibrato_env: &SVParamCurve,
        sv_notes: &[SVNote],
        step: i64,
        master_pitch_diff: Option<&SVParamCurve>,
        master_vibrato_env: Option<&SVParamCurve>,
        pitch_controls: Option<&[SVPitchControl]>,
    ) -> Result<ParamCurve> {
        let mut curve = ParamCurve::default();
        if sv_notes.is_empty() {
            curve.points.push(Point::start_point());
            curve.points.push(Point::end_point());
            return Ok(curve);
        }
        let mut pitch_diff_expr = ParamExpr::Curve(CurveGenerator::new(
            pitch_diff
                .points
                .iter()
                .map(|point| Point::new(position_to_ticks(point.offset), py_round(point.value)))
                .collect::<Vec<_>>(),
            Interpolation::from_mode(&pitch_diff.mode),
            0,
            None,
        ));
        let mut vibrato_env_expr = ParamExpr::Curve(CurveGenerator::new(
            vibrato_env
                .points
                .iter()
                .map(|point| {
                    Point::new(
                        position_to_ticks(point.offset),
                        py_round(point.value * 1000.0),
                    )
                })
                .collect::<Vec<_>>(),
            Interpolation::from_mode(&vibrato_env.mode),
            1000,
            None,
        ));
        if let Some(master_pitch_diff) = master_pitch_diff {
            pitch_diff_expr = pitch_diff_expr.add(ParamExpr::Curve(CurveGenerator::new(
                master_pitch_diff
                    .points
                    .iter()
                    .map(|point| Point::new(position_to_ticks(point.offset), py_round(point.value)))
                    .collect::<Vec<_>>(),
                Interpolation::from_mode(&master_pitch_diff.mode),
                0,
                None,
            )));
        }
        if let Some(master_vibrato_env) = master_vibrato_env {
            vibrato_env_expr = vibrato_env_expr.add(ParamExpr::Curve(CurveGenerator::new(
                master_vibrato_env
                    .points
                    .iter()
                    .map(|point| {
                        Point::new(
                            position_to_ticks(point.offset),
                            py_round(point.value * 1000.0),
                        )
                    })
                    .collect::<Vec<_>>(),
                // Quirk: Python uses `vibrato_env.mode` here.
                Interpolation::from_mode(&vibrato_env.mode),
                0,
                None,
            )));
        }
        if self.options.instant {
            let instant_interval = RangeInterval::from_pairs(sv_notes.iter().filter_map(|note| {
                if note.instant_mode != Some(false) {
                    Some((
                        position_to_ticks(note.onset),
                        position_to_ticks(note.onset + note.duration),
                    ))
                } else {
                    None
                }
            }));
            pitch_diff_expr = pitch_diff_expr.add(ParamExpr::Curve(CurveGenerator::new(
                self.instant_pitch
                    .points
                    .iter()
                    .map(|point| Point::new(position_to_ticks(point.offset), py_round(point.value)))
                    .collect::<Vec<_>>(),
                Interpolation::from_mode(&self.instant_pitch.mode),
                0,
                Some(instant_interval),
            )));
        }
        let mut interval = RangeInterval::from_pairs(sv_notes.iter().map(|note| {
            (
                position_to_ticks(note.onset),
                position_to_ticks(note.onset + note.duration),
            )
        }))
        .expand(120);
        let generator = PitchGenerator::new(
            self.synchronizer.clone(),
            sv_notes,
            pitch_diff_expr,
            vibrato_env_expr,
            pitch_controls,
        );
        if matches!(self.options.pitch, PitchOption::Vibrato | PitchOption::Plain) {
            let regard_default_vibrato_as_unedited = self.options.pitch == PitchOption::Plain;
            const PITCH_INTERVAL: f64 = 0.1;
            let mut note_edited_range = RangeInterval::new();
            for note in sv_notes {
                if !note.pitch_edited(regard_default_vibrato_as_unedited, self.options.instant) {
                    continue;
                }
                let start_secs = self
                    .synchronizer
                    .get_actual_secs_from_ticks(position_to_ticks(note.onset))
                    - note.attributes.transition_offset().max(0.0)
                    - PITCH_INTERVAL;
                let end_secs = self
                    .synchronizer
                    .get_actual_secs_from_ticks(position_to_ticks(
                        note.onset + note.duration,
                    ))
                    + PITCH_INTERVAL;
                note_edited_range.union_with(&RangeInterval::from_pairs([(
                    py_round(
                        self.synchronizer
                            .get_actual_ticks_from_secs(start_secs.max(0.0)),
                    ),
                    py_round(self.synchronizer.get_actual_ticks_from_secs(end_secs)),
                )]));
            }
            let mut param_edited_range = pitch_diff
                .edited_range(0.0)
                .union(&vibrato_env.edited_range(1.0));
            if let Some(master_pitch_diff) = master_pitch_diff {
                param_edited_range =
                    param_edited_range.union(&master_pitch_diff.edited_range(0.0));
            }
            if let Some(layer) = &generator.pitch_control_layer {
                param_edited_range.union_with(&layer.domain());
            }
            if let Some(master_vibrato_env) = master_vibrato_env {
                param_edited_range =
                    param_edited_range.union(&master_vibrato_env.edited_range(1.0));
            }
            interval = interval.intersection(&note_edited_range.union(&param_edited_range));
        }
        curve.points.push(Point::start_point());
        for (start, end) in interval.shift(self.first_bar_tick).sub_ranges() {
            curve.points.push(Point::new(start, -100));
            let mut position = start;
            while position < end {
                curve
                    .points
                    .push(Point::new(position, py_round(generator.value_at_ticks(position - self.first_bar_tick))));
                position += step;
            }
            curve.points.push(Point::new(
                end,
                py_round(generator.value_at_ticks(end - self.first_bar_tick)),
            ));
            curve.points.push(Point::new(end, -100));
        }
        curve.points.push(Point::end_point());
        Ok(curve)
    }

    fn parse_params(
        &self,
        sv_params: &SVParameters,
        sv_notes: &[SVNote],
        master_params: Option<&SVParameters>,
        pitch_controls: Option<&[SVPitchControl]>,
    ) -> Result<Params> {
        let mut params = Params::default();
        let master_pitch_delta = master_params.map(|params| &params.pitch_delta);
        let master_vibrato_env = master_params.map(|params| &params.vibrato_env);
        if self.options.import_pitch {
            params.pitch = self.parse_pitch_curve(
                &sv_params.pitch_delta,
                &sv_params.vibrato_env,
                sv_notes,
                5,
                master_pitch_delta,
                master_vibrato_env,
                pitch_controls,
            )?;
        }
        if self.options.import_volume {
            let base = self.voice_settings.base.param_loudness.unwrap_or(0.0);
            params.volume = self.parse_param_curve(
                &sv_params.loudness,
                &|value: f64| {
                    if value >= 0.0 {
                        py_round(value / 12.0 * 1000.0)
                    } else {
                        py_round(1000.0 * db_to_float(value) - 1000.0)
                    }
                },
                Some(base),
                master_params.map(|params| &params.loudness),
            );
        }
        if self.options.import_breath {
            let base = self.voice_settings.base.param_breathiness.unwrap_or(0.0);
            params.breath = self.parse_param_curve(
                &sv_params.breathiness,
                &|value: f64| py_round(value * 1000.0),
                Some(base),
                master_params.map(|params| &params.breathiness),
            );
        }
        if self.options.import_gender {
            let base = self.voice_settings.base.param_gender.unwrap_or(0.0);
            params.gender = self.parse_param_curve(
                &sv_params.gender,
                &|value: f64| py_round(value * -1000.0),
                Some(base),
                master_params.map(|params| &params.gender),
            );
        }
        if self.options.import_strength {
            let base = self.voice_settings.base.param_tension.unwrap_or(0.0);
            params.strength = self.parse_param_curve(
                &sv_params.tension,
                &|value: f64| py_round(value * 1000.0),
                Some(base),
                master_params.map(|params| &params.tension),
            );
        }
        Ok(params)
    }

    fn parse_note(sv_note: &SVNote, database: &SVDatabase) -> Note {
        let mut note = Note::new(
            position_to_ticks(sv_note.onset),
            0,
            sv_note.pitch,
            "",
        );
        note.length = position_to_ticks(sv_note.onset + sv_note.duration) - note.start_pos;
        note.lyric = SVNote::normalize_lyric(&sv_note.lyrics);
        if !sv_note.phonemes.is_empty() {
            let language = default_language(&sv_note.attributes, database);
            if language == "japanese" || language == "korean" {
                note.pronunciation = Some(sv_note.phonemes.clone());
            } else if language == "mandarin" || language == "cantonese" {
                note.pronunciation = Some(xsampa2pinyin(&sv_note.phonemes, &language));
            }
        }
        note
    }

    fn parse_note_list(&self, sv_note_list: &[SVNote], database: &SVDatabase) -> Result<Vec<Note>> {
        let mut note_list: Vec<Note> = Vec::new();
        let mut sv_notes: Vec<SVNote> = sv_note_list.to_vec();
        if self.options.breath == BreathOption::Convert {
            if sv_note_list.len() > 1 {
                let mut prev_index: isize = -1;
                loop {
                    let start = (prev_index + 1) as usize;
                    if start > sv_note_list.len() {
                        break;
                    }
                    let slice = &sv_note_list[start..];
                    let offset = find_index(slice, |note| !is_breath_note(&note.lyrics));
                    if offset == -1 {
                        break;
                    }
                    let current_index = prev_index + offset + 1;
                    let mut note =
                        Self::parse_note(&sv_note_list[current_index as usize], database);
                    if current_index > prev_index + 1 {
                        let breath_note = &sv_note_list[(current_index - 1) as usize];
                        if position_to_ticks(sv_note_list[current_index as usize].onset)
                            - position_to_ticks(breath_note.onset + breath_note.duration)
                            < 120
                        {
                            note.head_tag = Some("V".to_owned());
                        }
                    }
                    note_list.push(note);
                    prev_index = current_index;
                }
            } else if sv_note_list.len() == 1
                && !is_breath_note(&sv_note_list[0].lyrics)
            {
                note_list.push(Self::parse_note(&sv_note_list[0], database));
            }
            sv_notes.retain(|note| !is_breath_note(&note.lyrics));
        } else {
            if self.options.breath == BreathOption::Ignore {
                sv_notes.retain(|note| !is_breath_note(&note.lyrics));
            }
            note_list = sv_notes
                .iter()
                .map(|note| Self::parse_note(note, database))
                .collect();
        }
        let mut lyrics: Vec<String> = Vec::new();
        let mut languages: Vec<String> = Vec::new();
        if !sv_notes.is_empty() {
            for (note, sv_note) in note_list.iter().zip(sv_notes.iter()) {
                lyrics.push(SVNote::normalize_phoneme(note));
                languages.push(default_language(&sv_note.attributes, database));
            }
        }
        let lyrics_phoneme = sv_g2p(&lyrics, &languages);
        if note_list.is_empty() {
            return Ok(note_list);
        }
        for pair in note_list.windows(2) {
            if pair[0].end_pos() > pair[1].start_pos {
                let bar_index = find_bar_index(&self.time_signatures, pair[1].start_pos)?;
                return Err(LibreSvipError::msg(format!(
                    "Notes overlapped near bar {}",
                    bar_index
                )));
            }
        }
        let current_sv_note = &sv_notes[0];
        let mut current_duration = current_sv_note.attributes.dur.clone();
        let current_phonemes = current_sv_note.attributes.phonemes.clone();
        let mut current_phone_marks = default_phone_marks(
            lyrics_phoneme.first().map(String::as_str).unwrap_or(""),
            &default_language(&current_sv_note.attributes, database),
        );
        if let Some(phonemes) = &current_phonemes {
            if let (Some(first), Some(left_offset)) =
                (phonemes.first(), phonemes.first().and_then(|p| p.left_offset))
            {
                if first.left_offset.is_some() {
                    note_list[0].edited_phones = Some(Phones {
                        head_length_in_secs: 1.8f64.min(-left_offset),
                        ..Default::default()
                    });
                }
            }
        } else if current_phone_marks.first().copied().unwrap_or(0.0) > 0.0 {
            if let Some(duration) = &current_duration {
                if let Some(first) = duration.first().copied() {
                    if first != 1.0 {
                        note_list[0].edited_phones = Some(Phones {
                            head_length_in_secs: 1.8f64.min(first * current_phone_marks[0]),
                            ..Default::default()
                        });
                    }
                }
            }
        }
        for i in 0..sv_notes.len().saturating_sub(1) {
            let next_sv_note = &sv_notes[i + 1];
            let next_duration = next_sv_note.attributes.dur.clone();
            let next_phonemes = next_sv_note.attributes.phonemes.clone();
            let next_phone_marks = default_phone_marks(
                lyrics_phoneme.get(i + 1).map(String::as_str).unwrap_or(""),
                &default_language(&next_sv_note.attributes, database),
            );
            let index = if next_phone_marks.first().copied().unwrap_or(0.0) > 0.0 {
                1usize
            } else {
                0usize
            };
            let next_head_part_length = if let Some(phonemes) = &next_phonemes {
                if let (Some(first), Some(left_offset)) =
                    (phonemes.first(), phonemes.first().and_then(|p| p.left_offset))
                {
                    let _ = first;
                    Some(-left_offset)
                } else if next_phone_marks.first().copied().unwrap_or(0.0) > 0.0 {
                    next_duration
                        .as_ref()
                        .and_then(|duration| duration.first().copied())
                        .map(|first| first * next_phone_marks[0])
                } else {
                    None
                }
            } else if next_phone_marks.first().copied().unwrap_or(0.0) > 0.0 {
                next_duration
                    .as_ref()
                    .and_then(|duration| duration.first().copied())
                    .map(|first| first * next_phone_marks[0])
            } else {
                None
            };
            if current_phone_marks.get(1).copied().unwrap_or(0.0) > 0.0 {
                if let Some(duration) = &current_duration {
                    if duration.len() > index + 1 {
                        if note_list[i].edited_phones.is_none() {
                            note_list[i].edited_phones = Some(Phones::default());
                        }
                        if let Some(phones) = &mut note_list[i].edited_phones {
                            phones.mid_ratio_over_tail =
                                current_phone_marks[1] * duration[index] / duration[index + 1];
                        }
                    }
                }
            }
            if let Some(mut next_head_part_length) = next_head_part_length {
                if note_list[i + 1].edited_phones.is_none() {
                    note_list[i + 1].edited_phones = Some(Phones::default());
                }
                let space_in_secs = 2.0f64.min(self.synchronizer.get_duration_secs_from_ticks(
                    note_list[i].start_pos + self.first_bar_tick,
                    note_list[i + 1].start_pos + self.first_bar_tick,
                ));
                if current_phone_marks.get(1).copied().unwrap_or(0.0) > 0.0 {
                    if let Some(duration) = &current_duration {
                        if duration.len() > index {
                            let ratio = if duration.len() > index + 1 {
                                2.0 / (duration[index] + duration[index + 1])
                            } else {
                                1.0 / duration[index]
                            };
                            if next_head_part_length * ratio
                                > self.synchronizer.get_duration_secs_from_ticks(
                                    note_list[i].end_pos() + self.first_bar_tick,
                                    note_list[i + 1].start_pos + self.first_bar_tick,
                                )
                            {
                                next_head_part_length *= ratio;
                            }
                        }
                    }
                }
                if let Some(phones) = &mut note_list[i + 1].edited_phones {
                    phones.head_length_in_secs =
                        (0.9 * space_in_secs).min(next_head_part_length);
                }
            }
            current_duration = next_duration;
            current_phone_marks = next_phone_marks;
        }
        let index = if current_phone_marks.first().copied().unwrap_or(0.0) > 0.0 {
            1usize
        } else {
            0usize
        };
        if current_phone_marks.get(1).copied().unwrap_or(0.0) > 0.0 {
            if let Some(duration) = &current_duration {
                if duration.len() > index + 1 {
                    let last_index = note_list.len() - 1;
                    if note_list[last_index].edited_phones.is_none() {
                        note_list[last_index].edited_phones = Some(Phones::default());
                    }
                    if let Some(phones) = &mut note_list[last_index].edited_phones {
                        phones.mid_ratio_over_tail =
                            current_phone_marks[1] * duration[index] / duration[index + 1];
                    }
                }
            }
        }
        Ok(note_list)
    }

    fn parse_track(&mut self, sv_track: &SVTrack) -> Result<Option<Track>> {
        if sv_track.main_ref.is_instrumental {
            if self.options.import_instrumental_track {
                if let Some(audio) = &sv_track.main_ref.audio {
                    let mut track = InstrumentalTrack::new(
                        sv_track.name.clone(),
                        audio.filename.clone(),
                    );
                    track.offset = self.parse_audio_offset(sv_track.main_ref.blick_offset);
                    track.mute = sv_track.mixer.mute;
                    track.solo = sv_track.mixer.solo;
                    track.pan = sv_track.mixer.pan;
                    track.volume = Self::parse_volume(sv_track.mixer.gain_decibel);
                    return Ok(Some(Track::Instrumental(track)));
                }
            }
            return Ok(None);
        }

        self.voice_settings = sv_track.main_ref.voice.clone();
        let master_note_attributes = self.voice_settings.to_attributes();
        if self.options.instant {
            self.instant_pitch = sv_track
                .main_ref
                .system_pitch_delta
                .add_offset(sv_track.main_ref.blick_offset);
        }
        let mut main_notes = sv_track.main_group.notes.clone();
        for note in &mut main_notes {
            note.merge_attributes(&master_note_attributes);
        }
        let note_list = self.parse_note_list(&main_notes, &sv_track.main_ref.database)?;
        let edited_params = self.parse_params(
            &sv_track.main_group.parameters,
            &main_notes,
            None,
            None,
        )?;
        let mut singing_track = SingingTrack {
            title: String::new(),
            ai_singer_name: sv_track.main_ref.database.name.clone(),
            note_list,
            edited_params,
            ..Default::default()
        };

        if self.options.group == GroupOption::Split {
            for sv_ref in &sv_track.groups {
                let group = self.group_for_ref(sv_ref)?;
                let voice_settings = sv_ref.voice.clone();
                let master_note_attributes = voice_settings.to_attributes();
                if self.options.instant {
                    self.instant_pitch = sv_ref
                        .system_pitch_delta
                        .add_offset(sv_ref.blick_offset);
                }
                let mut group = group;
                for note in &mut group.notes {
                    note.merge_attributes(&master_note_attributes);
                }
                let count = self.group_split_counts.entry(sv_ref.group_id.clone()).or_insert(0);
                *count += 1;
                let title = format!("{} ({})", group.name, count);
                let note_list = self.parse_note_list(&group.notes, &sv_ref.database)?;
                let edited_params = self.parse_params(
                    &group.parameters,
                    &group.notes,
                    Some(&sv_track.main_group.parameters),
                    Some(&group.pitch_controls),
                )?;
                self.tracks_from_groups.push(Track::Singing(SingingTrack {
                    title,
                    ai_singer_name: sv_ref.database.name.clone(),
                    note_list,
                    edited_params,
                    ..Default::default()
                }));
            }
        } else if self.options.group == GroupOption::Merge {
            let mut merged_group = sv_track.main_group.clone();
            for sv_ref in &sv_track.groups {
                let mut group = self.group_for_ref(sv_ref)?;
                let voice_settings = sv_ref.voice.clone();
                let master_note_attributes = voice_settings.to_attributes();
                if self.options.instant {
                    self.instant_pitch = sv_ref
                        .system_pitch_delta
                        .add_offset(sv_ref.blick_offset);
                }
                for note in &mut group.notes {
                    note.merge_attributes(&master_note_attributes);
                }
                if merged_group.overlapped_with(&group) {
                    let count = self
                        .group_split_counts
                        .entry(sv_ref.group_id.clone())
                        .or_insert(0);
                    *count += 1;
                    let title = format!("{} ({})", group.name, count);
                    let note_list = self.parse_note_list(&group.notes, &sv_ref.database)?;
                    let edited_params = self.parse_params(
                        &group.parameters,
                        &group.notes,
                        Some(&sv_track.main_group.parameters),
                        Some(&group.pitch_controls),
                    )?;
                    self.tracks_from_groups.push(Track::Singing(SingingTrack {
                        title,
                        ai_singer_name: sv_ref.database.name.clone(),
                        note_list,
                        edited_params,
                        ..Default::default()
                    }));
                } else {
                    let note_list = self.parse_note_list(&group.notes, &sv_ref.database)?;
                    let edited_params = self.parse_params(
                        &group.parameters,
                        &group.notes,
                        Some(&sv_track.main_group.parameters),
                        Some(&group.pitch_controls),
                    )?;
                    track_override_with(
                        &mut singing_track,
                        &note_list,
                        &edited_params,
                        self.first_bar_tick,
                    );
                    merged_group.notes.extend(group.notes.clone());
                    merged_group.notes.sort_by_key(|note| note.onset);
                }
            }
        }

        let mut svip_track = Track::Singing(singing_track);
        svip_track.set_title(sv_track.name.clone());
        match &mut svip_track {
            Track::Singing(track) => {
                track.mute = sv_track.mixer.mute;
                track.solo = sv_track.mixer.solo;
                track.pan = sv_track.mixer.pan;
                track.volume = Self::parse_volume(sv_track.mixer.gain_decibel);
            }
            Track::Instrumental(track) => {
                track.mute = sv_track.mixer.mute;
                track.solo = sv_track.mixer.solo;
                track.pan = sv_track.mixer.pan;
                track.volume = Self::parse_volume(sv_track.mixer.gain_decibel);
            }
        }
        Ok(Some(svip_track))
    }

    fn group_for_ref(&self, sv_ref: &crate::svp::model::SVRef) -> Result<SVGroup> {
        let Some(group) = self.group_library.get(&sv_ref.group_id) else {
            return Err(LibreSvipError::msg(format!(
                "note group {} not found in library",
                sv_ref.group_id
            )));
        };
        Ok(group.add_offset(sv_ref.blick_offset).xor_pitch(sv_ref.pitch_offset))
    }

    pub fn parse_project(&mut self, sv_project: &SVProject) -> Result<Project> {
        let mut project = Project::default();
        let time_sig = &sv_project.time_sig;
        let Some(first_tempo) = time_sig.tempo.first() else {
            return Err(LibreSvipError::msg("project has no tempo"));
        };
        self.first_bpm = first_tempo.bpm;
        self.time_signatures = shift_beat_list(
            &time_sig
                .meter
                .iter()
                .map(Self::parse_meter)
                .collect::<Vec<_>>(),
            1,
        );
        project.time_signature_list = self.time_signatures.clone();
        let Some(first_signature) = project.time_signature_list.first() else {
            return Err(LibreSvipError::msg("project has no time signature"));
        };
        self.first_bar_tick = py_round(first_signature.bar_length());
        project.song_tempo_list = shift_tempo_list(
            &time_sig
                .tempo
                .iter()
                .map(Self::parse_tempo)
                .collect::<Vec<_>>(),
            self.first_bar_tick,
        );
        self.synchronizer = TimeSynchronizer::new(&project.song_tempo_list, 0, false, 60.0);

        for sv_group in &sv_project.library {
            self.group_library
                .insert(sv_group.uuid.clone(), sv_group.clone());
            self.group_split_counts
                .insert(sv_group.uuid.clone(), 0);
        }

        for sv_track in &sv_project.tracks {
            if let Some(track) = self.parse_track(sv_track)? {
                project.track_list.push(track);
            }
        }
        project.track_list.extend(self.tracks_from_groups.drain(..));
        Ok(project)
    }
}

fn default_language(attributes: &crate::svp::model::SVNoteAttributes, database: &SVDatabase) -> String {
    attributes
        .base
        .language_override
        .clone()
        .or_else(|| database.language_override.clone())
        .unwrap_or_else(|| database.language.clone())
}

/// Fallback used when the merged curve has no points (Python would index an
/// empty list); keeps the parser panic-free.
fn compound_expr_curve(
    expression: &ParamExpr,
    group_points: &[Point],
    master_points: &[Point],
) -> Vec<Point> {
    let mut points = Vec::new();
    points.push(Point::start_point_with(py_round(expression.value_at_ticks(0) / 1000.0)));
    points.push(Point::end_point_with(py_round(
        expression.value_at_ticks(0) / 1000.0,
    )));
    let _ = (group_points, master_points);
    points
}


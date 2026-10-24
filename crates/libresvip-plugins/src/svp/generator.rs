//! Port of `libresvip.plugins.svp.synthv_generator`.

use std::collections::HashSet;

use libresvip_core::core::tick_counter::skip_beat_list;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{
    Note, ParamCurve, Params, Project, SongTempo, TimeSignature, Track,
};
use libresvip_core::model::point::{Point, END_POINT_X};

use crate::svp::constants::TICK_RATE;
use crate::svp::math_utils::{
    bisect_right_f64, clamp, find_index, find_last_index, py_round, ratio_to_db, ticks_to_position,
};
use crate::svp::model::{
    SVAudio, SVCurvePitchControl, SVMeter, SVMixer, SVNote, SVParamCurve,
    SVParameters, SVPhonemeAttribute, SVPoint, SVProject, SVTempo, SVTrack,
};
use crate::svp::options::{
    OutputOptions, VibratoOption, VERSION_ABOVE_2_0_0,
    VERSION_BETWEEN_1_10_0_AND_1_11_2,
};
use crate::svp::phoneme_utils::{default_phone_marks, number_of_phones, pinyin_to_xsampa, sv_g2p};

pub struct SynthVGenerator {
    options: OutputOptions,
    first_bar_tick: i64,
    first_bar_tempo: Vec<SongTempo>,
    note_buffer: Vec<Note>,
    synchronizer: TimeSynchronizer,
    pitch_simulator: GeneratorPitchSimulator,
    no_vibrato_indexes: HashSet<usize>,
    lyrics_phonemes: Vec<String>,
}

/// The `PitchSimulator` used by the generator (`PortamentoPitch.no_portamento`):
/// a piecewise-constant pitch curve switching keys at the midpoint between two
/// consecutive notes.
struct GeneratorPitchSimulator {
    boundaries: Vec<f64>,
    keys: Vec<i64>,
}

impl GeneratorPitchSimulator {
    fn new(synchronizer: TimeSynchronizer, note_list: &[Note]) -> Self {
        let mut boundaries = Vec::new();
        let mut keys = Vec::new();
        if !note_list.is_empty() {
            boundaries.push(0.0);
            keys.push(note_list[0].key_number);
            for index in 1..note_list.len() {
                let current_end = synchronizer
                    .get_actual_secs_from_ticks(note_list[index - 1].end_pos());
                let next_start =
                    synchronizer.get_actual_secs_from_ticks(note_list[index].start_pos);
                boundaries.push((next_start + current_end) / 2.0);
                keys.push(note_list[index].key_number);
            }
        }
        Self {
            boundaries,
            keys,
        }
    }

    fn pitch_at_secs(&self, secs: f64) -> Option<f64> {
        if self.keys.is_empty() {
            return None;
        }
        let index = bisect_right_f64(&self.boundaries, secs).checked_sub(1)?;
        Some(self.keys[index] as f64 * 100.0)
    }
}

impl SynthVGenerator {
    pub fn new(options: OutputOptions) -> Self {
        Self {
            options,
            first_bar_tick: 0,
            first_bar_tempo: Vec::new(),
            note_buffer: Vec::new(),
            synchronizer: TimeSynchronizer::from_project(&[]),
            pitch_simulator: GeneratorPitchSimulator::new(
                TimeSynchronizer::from_project(&[]),
                &[],
            ),
            no_vibrato_indexes: HashSet::new(),
            lyrics_phonemes: Vec::new(),
        }
    }

    pub fn generate_project(&mut self, project: &Project) -> Result<SVProject> {
        if self.options.version_compatibility == VERSION_ABOVE_2_0_0
            && self.options.vibrato == VibratoOption::Hybrid
        {
            // Python's `generate_pitch_curve` (which initialises
            // `no_vibrato_indexes`) is skipped for SV2 projects, so this
            // combination raises AttributeError in the reference implementation.
            return Err(LibreSvipError::msg(
                "hybrid vibrato mode cannot be used with a 2.0 project",
            ));
        }
        let mut sv_project = SVProject::default();
        if self.options.version_compatibility == crate::svp::options::VERSION_BELOW_1_9_0 {
            sv_project.instant_mode_enabled = Some(false);
        } else if self.options.version_compatibility == VERSION_BETWEEN_1_10_0_AND_1_11_2 {
            sv_project.version = VERSION_BETWEEN_1_10_0_AND_1_11_2;
        } else {
            sv_project.version = VERSION_ABOVE_2_0_0;
        }
        let new_meters = skip_beat_list(&project.time_signature_list, 1);
        let Some(first_signature) = project.time_signature_list.first() else {
            return Err(LibreSvipError::msg("project has no time signature"));
        };
        self.first_bar_tick = py_round(first_signature.bar_length());
        self.first_bar_tempo = project
            .song_tempo_list
            .iter()
            .filter(|tempo| tempo.position < self.first_bar_tick)
            .cloned()
            .collect();
        self.synchronizer =
            TimeSynchronizer::new(&project.song_tempo_list, self.first_bar_tick, false, 60.0);
        for tempo in self.synchronizer.tempo_list.clone() {
            sv_project.time_sig.tempo.push(Self::generate_tempo(&tempo));
        }
        if project
            .time_signature_list
            .iter()
            .any(|beat| beat.denominator < 2 || beat.denominator > 16)
        {
            sv_project.time_sig.meter.push(SVMeter {
                index: 0,
                numerator: 4,
                denominator: 4,
            });
        } else {
            for meter in &new_meters {
                sv_project.time_sig.meter.push(Self::generate_meter(meter));
            }
        }
        for (track_id, track) in project.track_list.iter().enumerate() {
            if let Some(mut sv_track) =
                self.generate_track(track, &project.time_signature_list)?
            {
                sv_track.disp_order = track_id as i64;
                sv_project.tracks.push(sv_track);
            }
        }
        Ok(sv_project)
    }

    fn generate_tempo(tempo: &SongTempo) -> SVTempo {
        SVTempo {
            position: ticks_to_position(tempo.position),
            bpm: tempo.bpm,
        }
    }

    fn generate_meter(signature: &TimeSignature) -> SVMeter {
        SVMeter {
            index: signature.bar_index,
            numerator: signature.numerator,
            denominator: signature.denominator,
        }
    }

    fn generate_track(
        &mut self,
        track: &Track,
        _time_signature_list: &[TimeSignature],
    ) -> Result<Option<SVTrack>> {
        let mut sv_track = SVTrack {
            name: track.title().to_owned(),
            mixer: SVMixer {
                gain_decibel: Self::generate_volume(track.volume()),
                pan: track.pan(),
                mute: track.mute(),
                solo: track.solo(),
                ..Default::default()
            },
            ..Default::default()
        };
        match track {
            Track::Singing(singing) => {
                sv_track.main_ref.is_instrumental = false;
                sv_track.disp_color = "ff7db235".to_owned();
                let (language, phoneset) = self.options.language_override.preset();
                sv_track.main_ref.database.language_override = Some(language.to_owned());
                sv_track.main_ref.database.phoneset_override = Some(phoneset.to_owned());
                if !singing.ai_singer_name.is_empty() {
                    sv_track.main_ref.database.name = singing.ai_singer_name.clone();
                }
                self.note_buffer = singing.note_list.clone();
                self.pitch_simulator =
                    GeneratorPitchSimulator::new(self.synchronizer.clone(), &self.note_buffer);
                sv_track.main_group.parameters = self.generate_params(&singing.edited_params);
                if self.options.version_compatibility == VERSION_ABOVE_2_0_0 {
                    sv_track.main_group.pitch_controls = self
                        .generate_pitch_controls(
                            &singing
                                .edited_params
                                .pitch
                                .reduce_sample_rate(self.options.down_sample, -100),
                        )
                        .into_iter()
                        .map(crate::svp::model::SVPitchControl::Curve)
                        .collect();
                }
                self.lyrics_phonemes = sv_g2p(
                    &singing
                        .note_list
                        .iter()
                        .map(SVNote::normalize_phoneme)
                        .collect::<Vec<_>>(),
                    &vec![language.to_owned(); singing.note_list.len()],
                );
                sv_track.main_group.notes =
                    self.generate_notes_with_phones(&singing.note_list);
                match self.options.vibrato {
                    VibratoOption::None => {
                        for note in &mut sv_track.main_group.notes {
                            note.attributes.base.d_f0_vbr = Some(0.0);
                        }
                    }
                    VibratoOption::Hybrid => {
                        for index in &self.no_vibrato_indexes {
                            if let Some(note) = sv_track.main_group.notes.get_mut(*index) {
                                note.attributes.base.d_f0_vbr = Some(0.0);
                            }
                        }
                    }
                    VibratoOption::Always => {}
                }
            }
            Track::Instrumental(instrumental) => {
                sv_track.main_ref.is_instrumental = true;
                sv_track.disp_color = "ff4794cb".to_owned();
                sv_track.main_ref.audio = Some(SVAudio {
                    filename: instrumental.audio_file_path.clone(),
                    duration: 0.0,
                    ..Default::default()
                });
                sv_track.main_ref.blick_offset = self.generate_audio_offset(instrumental.offset)?;
                if let Some(track_info) = libresvip_core::utils::audio::audio_track_info(
                    &instrumental.audio_file_path,
                    false,
                ) {
                    if let Some(audio) = &mut sv_track.main_ref.audio {
                        audio.duration = track_info.duration;
                    }
                }
            }
        }
        Ok(Some(sv_track))
    }

    fn generate_audio_offset(&self, offset: i64) -> Result<i64> {
        if offset >= 0 {
            return Ok(ticks_to_position(offset));
        }
        let mut current_pos = self.first_bar_tick;
        let actual_pos = self.first_bar_tick + offset;
        let mut res = 0.0f64;
        let mut index: isize = self.first_bar_tempo.len() as isize - 1;
        let mut position = self.first_bar_tempo.len() as isize - 1;
        while position >= 0 {
            if actual_pos > self.first_bar_tempo[position as usize].position {
                index = position;
                break;
            }
            res -= (current_pos - self.first_bar_tempo[position as usize].position) as f64 * 120.0
                / self.first_bar_tempo[position as usize].bpm;
            current_pos = self.first_bar_tempo[position as usize].position;
            index = position;
            position -= 1;
        }
        if index >= 0 {
            res -= (current_pos - actual_pos) as f64 * 120.0
                / self.first_bar_tempo[index as usize].bpm;
        } else if let Some(first) = self.first_bar_tempo.first() {
            res += actual_pos as f64 * 120.0 / first.bpm;
        } else {
            return Err(LibreSvipError::msg("project has no tempo"));
        }
        Ok(py_round(res * TICK_RATE as f64))
    }

    fn generate_volume(volume: f64) -> f64 {
        ratio_to_db(volume.max(0.06)).max(-24.0)
    }

    fn generate_params(&mut self, parameters: &Params) -> SVParameters {
        let down_sample = self.options.down_sample;
        let loudness = self.generate_param_curve(
            &parameters.volume.reduce_sample_rate(down_sample, 0),
            0,
            0.0,
            &|value: i64| {
                if value >= 0 {
                    value as f64 / 1000.0 * 12.0
                } else {
                    ratio_to_db(if value > -997 {
                        value as f64 / 1000.0 + 1.0
                    } else {
                        0.0039
                    })
                    .max(-48.0)
                }
            },
        );
        let tension = self.generate_param_curve(
            &parameters.strength.reduce_sample_rate(down_sample, 0),
            0,
            0.0,
            &|value: i64| 1000.0 / value as f64,
        );
        let breathiness = self.generate_param_curve(
            &parameters.breath.reduce_sample_rate(down_sample, 0),
            0,
            0.0,
            &|value: i64| 1000.0 / value as f64,
        );
        let gender = self.generate_param_curve(
            &parameters.gender.reduce_sample_rate(down_sample, 0),
            0,
            0.0,
            &|value: i64| -1000.0 / value as f64,
        );
        let mut sv_params = SVParameters {
            loudness,
            tension,
            breathiness,
            gender,
            ..Default::default()
        };
        if self.options.version_compatibility != VERSION_ABOVE_2_0_0 {
            sv_params.pitch_delta = self.generate_pitch_curve(
                &parameters.pitch.reduce_sample_rate(down_sample, -100),
            );
        }
        sv_params
    }

    fn generate_pitch_controls(&self, curve: &ParamCurve) -> Vec<SVCurvePitchControl> {
        let mut pitch_controls: Vec<SVCurvePitchControl> = Vec::new();
        let mut buffer: Vec<Point> = Vec::new();
        for point in &curve.points {
            if point.x >= self.first_bar_tick {
                let point = Point::new(point.x - self.first_bar_tick, point.y);
                if point.y == -100 {
                    if buffer.is_empty() {
                        continue;
                    }
                    let base_position = ticks_to_position(buffer[0].x);
                    let base_pitch = buffer[0].y;
                    let points = buffer
                        .iter()
                        .map(|tmp_point| {
                            SVPoint::new(
                                ticks_to_position(tmp_point.x) - base_position,
                                (tmp_point.y - base_pitch) as f64 / 100.0,
                            )
                        })
                        .collect();
                    pitch_controls.push(SVCurvePitchControl {
                        pos: base_position,
                        pitch: base_pitch as f64 / 100.0,
                        id_value: (pitch_controls.len() + 1).to_string(),
                        points,
                    });
                    buffer.clear();
                } else {
                    buffer.push(point);
                }
            }
        }
        if !buffer.is_empty() {
            let base_position = ticks_to_position(buffer[0].x);
            let base_pitch = buffer[0].y;
            let points = buffer
                .iter()
                .map(|tmp_point| {
                    SVPoint::new(
                        ticks_to_position(tmp_point.x) - base_position,
                        (tmp_point.y - base_pitch) as f64 / 100.0,
                    )
                })
                .collect();
            pitch_controls.push(SVCurvePitchControl {
                pos: base_position,
                pitch: base_pitch as f64 / 100.0,
                id_value: (pitch_controls.len() + 1).to_string(),
                points,
            });
        }
        pitch_controls
    }

    fn generate_pitch_curve(&mut self, curve: &ParamCurve) -> SVParamCurve {
        let mut sv_curve = SVParamCurve::default();
        if self.options.vibrato == VibratoOption::Hybrid {
            self.no_vibrato_indexes = HashSet::new();
        }
        if self.note_buffer.is_empty() {
            return sv_curve;
        }
        let mut buffer: Vec<Point> = Vec::new();
        let min_interval: i64 = 1;
        let mut last_point: Option<Point> = None;
        for point in &curve.points {
            if point.x >= self.first_bar_tick {
                let point = Point::new(point.x - self.first_bar_tick, point.y);
                if point.y == -100 {
                    if buffer.is_empty() {
                        continue;
                    }
                    if last_point
                        .map(|last| last.x + min_interval < buffer[0].x)
                        .unwrap_or(true)
                    {
                        if let Some(last) = last_point {
                            if last.x + 2 * min_interval < buffer[0].x {
                                sv_curve.points.push(SVPoint::new(
                                    ticks_to_position(last.x + min_interval),
                                    0.0,
                                ));
                            }
                        }
                        sv_curve.points.push(SVPoint::new(
                            ticks_to_position(buffer[0].x - min_interval),
                            0.0,
                        ));
                    }
                    for tmp_point in &buffer {
                        sv_curve.points.push(SVPoint::new(
                            ticks_to_position(tmp_point.x),
                            self.generate_pitch_diff(tmp_point.x, tmp_point.y),
                        ));
                    }
                    last_point = buffer.last().copied();
                    buffer.clear();
                } else {
                    buffer.push(point);
                }
            }
        }
        if let Some(last) = last_point {
            sv_curve
                .points
                .push(SVPoint::new(ticks_to_position(last.x + min_interval), 0.0));
        }
        sv_curve
    }

    /// The phoneme text of a note, in the project language's notation.
    ///
    /// `None` when the option is off, when the note carries no pronunciation, or
    /// when the syllable is not in the dictionary for this language (writing a
    /// pinyin syllable as an X-SAMPA phoneme would be invalid).
    fn phonemes_for(&self, note: &Note) -> Option<String> {
        if !self.options.write_phonemes {
            return None;
        }
        let pronunciation = note.pronunciation.as_deref().filter(|value| !value.is_empty())?;
        match self.options.language_override.value() {
            "mandarin" | "cantonese" => {
                pinyin_to_xsampa(pronunciation, self.options.language_override.value())
            }
            // Japanese and Korean imports keep the source phonemes verbatim.
            "japanese" | "korean" => Some(pronunciation.to_owned()),
            _ => None,
        }
    }

    fn generate_note(&self, note: &Note) -> SVNote {
        let onset = ticks_to_position(note.start_pos);
        SVNote {
            phonemes: self.phonemes_for(note).unwrap_or_default(),
            instant_mode: if self.options.version_compatibility
                == VERSION_BETWEEN_1_10_0_AND_1_11_2
            {
                Some(false)
            } else {
                None
            },
            onset,
            pitch: note.key_number,
            lyrics: if !note.lyric.is_empty() {
                note.lyric.clone()
            } else {
                note.pronunciation.clone().unwrap_or_default()
            },
            duration: ticks_to_position(note.end_pos()) - onset,
            ..Default::default()
        }
    }

    fn generate_pitch_diff(&mut self, pos: i64, pitch: i64) -> f64 {
        let target_note_index = find_last_index(&self.note_buffer, |note| note.start_pos <= pos);
        let target_note = if target_note_index >= 0 {
            self.note_buffer.get(target_note_index as usize).cloned()
        } else {
            None
        };
        let simulated_pitch = self
            .pitch_simulator
            .pitch_at_secs(self.synchronizer.get_actual_secs_from_ticks(pos));
        let pitch_diff = match simulated_pitch {
            Some(simulated_pitch) => pitch as f64 - simulated_pitch,
            None => 0.0,
        };
        let Some(target_note) = target_note else {
            return pitch_diff;
        };
        if self.options.vibrato == VibratoOption::Hybrid
            && self
                .synchronizer
                .get_duration_secs_from_ticks(target_note.start_pos, pos)
                > 0.25
            && pos < target_note.end_pos()
        {
            self.no_vibrato_indexes
                .insert(target_note_index as usize);
        }
        pitch_diff
    }

    fn generate_param_curve(
        &self,
        curve: &ParamCurve,
        termination: i64,
        default_value: f64,
        mapping_func: &dyn Fn(i64) -> f64,
    ) -> SVParamCurve {
        let mut sv_curve = SVParamCurve::default();
        if curve.points.is_empty() {
            return sv_curve;
        }
        if self.options.down_sample > 15 {
            sv_curve.mode = "cubic".to_owned();
        }
        let mut skipped = 0usize;
        if curve.points[0].x == -192000 {
            if curve.points.len() == 2 && curve.points[1].x == END_POINT_X {
                if curve.points[0].y != termination {
                    sv_curve.points.push(SVPoint::new(
                        0,
                        mapping_func(curve.points[0].y),
                    ));
                }
                return sv_curve;
            }
            skipped = 1;
            let valid_index = find_index(&curve.points, |point| point.x >= self.first_bar_tick);
            if valid_index != -1
                && curve.points.len() > valid_index as usize + 1
                && (curve.points[valid_index as usize].y != termination
                    || curve.points[valid_index as usize + 1].y != termination
                    || curve.points[valid_index as usize + 1].x >= END_POINT_X)
            {
                skipped = valid_index as usize + 1;
                let x0 = curve.points[valid_index as usize].x;
                let y0 = curve.points[valid_index as usize].y;
                sv_curve.points.push(SVPoint::new(
                    ticks_to_position(x0 - self.first_bar_tick),
                    mapping_func(y0),
                ));
            }
        }
        let mut buffer: Vec<Point> = Vec::new();
        let min_interval: i64 = 1;
        let mut last_point: Option<Point> = None;
        for point in &curve.points[skipped..] {
            if self.first_bar_tick <= point.x && point.x < END_POINT_X {
                let point = Point::new(point.x - self.first_bar_tick, point.y);
                if point.y == termination {
                    if buffer.is_empty() {
                        continue;
                    }
                    if last_point
                        .map(|last| last.x + min_interval < buffer[0].x)
                        .unwrap_or(true)
                    {
                        if let Some(last) = last_point {
                            if last.x + 2 * min_interval < buffer[0].x {
                                sv_curve.points.push(SVPoint::new(
                                    ticks_to_position(last.x + min_interval),
                                    default_value,
                                ));
                            }
                        }
                        sv_curve.points.push(SVPoint::new(
                            ticks_to_position(buffer[0].x - min_interval),
                            default_value,
                        ));
                    }
                    for tmp_point in &buffer {
                        sv_curve.points.push(SVPoint::new(
                            ticks_to_position(tmp_point.x),
                            mapping_func(tmp_point.y),
                        ));
                    }
                    last_point = buffer.last().copied();
                    buffer.clear();
                } else {
                    buffer.push(point);
                }
            }
        }
        if buffer.is_empty() {
            if let Some(last) = last_point {
                sv_curve
                    .points
                    .push(SVPoint::new(ticks_to_position(last.x + min_interval), default_value));
            }
            return sv_curve;
        }
        if last_point
            .map(|last| last.x + min_interval < buffer[0].x)
            .unwrap_or(true)
        {
            if let Some(last) = last_point {
                if last.x + 2 * min_interval < buffer[0].x {
                    sv_curve.points.push(SVPoint::new(
                        ticks_to_position(last.x + min_interval),
                        default_value,
                    ));
                }
            }
            sv_curve.points.push(SVPoint::new(
                ticks_to_position(buffer[0].x - min_interval),
                default_value,
            ));
        }
        for tmp_point in &buffer {
            sv_curve.points.push(SVPoint::new(
                ticks_to_position(tmp_point.x),
                mapping_func(tmp_point.y),
            ));
        }
        let last_point = *buffer.last().unwrap_or(&Point::default());
        buffer.clear();
        if last_point.y == termination {
            sv_curve
                .points
                .push(SVPoint::new(ticks_to_position(last_point.x + min_interval), default_value));
        }
        sv_curve
    }

    fn generate_notes_with_phones(&self, notes: &[Note]) -> Vec<SVNote> {
        let mut sv_note_list: Vec<SVNote> = Vec::new();
        if notes.is_empty() {
            return sv_note_list;
        }
        let language = self.options.language_override.value();
        let current_note = &notes[0];
        let mut current_sv_note = self.generate_note(current_note);
        let mut current_phone_marks = default_phone_marks(
            self.lyrics_phonemes.first().map(String::as_str).unwrap_or(""),
            language,
        );
        if let Some(edited_phones) = &current_note.edited_phones {
            if edited_phones.head_length_in_secs > 0.0 {
                if self.options.version_compatibility == VERSION_ABOVE_2_0_0 {
                    current_sv_note.attributes.phonemes = Some(vec![SVPhonemeAttribute {
                        left_offset: Some(-edited_phones.head_length_in_secs),
                        ..Default::default()
                    }]);
                } else if current_phone_marks.first().copied().unwrap_or(0.0) > 0.0 {
                    let ratio = edited_phones.head_length_in_secs / current_phone_marks[0];
                    current_sv_note
                        .attributes
                        .set_phone_duration(0, clamp(ratio, 0.2, 1.8));
                }
            }
        }
        let count = notes
            .len()
            .saturating_sub(1)
            .min(self.lyrics_phonemes.len().saturating_sub(1));
        let mut current_note_index = 0usize;
        for index in 0..count {
            let current_note = &notes[index];
            let next_note = &notes[index + 1];
            let cur_phoneme = self
                .lyrics_phonemes
                .get(index)
                .map(String::as_str)
                .unwrap_or("");
            let next_phoneme = self
                .lyrics_phonemes
                .get(index + 1)
                .map(String::as_str)
                .unwrap_or("");
            let mut next_sv_note = self.generate_note(next_note);
            let next_phone_marks = default_phone_marks(next_phoneme, language);
            let current_main_part_edited = current_phone_marks.get(1).copied().unwrap_or(0.0)
                > 0.0
                && current_note.edited_phones.is_some()
                && current_note
                    .edited_phones
                    .as_ref()
                    .map(|phones| phones.mid_ratio_over_tail > 0.0)
                    .unwrap_or(false);
            let next_head_part_edited = next_phone_marks.first().copied().unwrap_or(0.0) > 0.0
                && next_note.edited_phones.is_some()
                && next_note
                    .edited_phones
                    .as_ref()
                    .map(|phones| phones.head_length_in_secs > 0.0)
                    .unwrap_or(false);
            let phone_index = if current_phone_marks.first().copied().unwrap_or(0.0) > 0.0 {
                1usize
            } else {
                0usize
            };
            if let Some(edited) = &next_note.edited_phones {
                if edited.head_length_in_secs > 0.0 {
                    next_sv_note.attributes.phonemes = Some(vec![SVPhonemeAttribute {
                        left_offset: Some(-edited.head_length_in_secs),
                        ..Default::default()
                    }]);
                } else if current_main_part_edited && next_head_part_edited {
                    self.apply_both_phone_ratios(
                        &mut current_sv_note,
                        &mut next_sv_note,
                        current_note,
                        next_note,
                        &current_phone_marks,
                        &next_phone_marks,
                        phone_index,
                    );
                } else if current_main_part_edited {
                    self.apply_current_phone_ratios(
                        &mut current_sv_note,
                        current_note,
                        &current_phone_marks,
                        phone_index,
                    );
                } else if next_head_part_edited {
                    self.apply_next_phone_ratio(
                        &mut current_sv_note,
                        &mut next_sv_note,
                        current_note,
                        next_note,
                        &current_phone_marks,
                        &next_phone_marks,
                        phone_index,
                    );
                }
            } else if current_main_part_edited {
                self.apply_current_phone_ratios(
                    &mut current_sv_note,
                    current_note,
                    &current_phone_marks,
                    phone_index,
                );
            } else if next_head_part_edited {
                self.apply_next_phone_ratio(
                    &mut current_sv_note,
                    &mut next_sv_note,
                    current_note,
                    next_note,
                    &current_phone_marks,
                    &next_phone_marks,
                    phone_index,
                );
            }
            if current_sv_note.attributes.dur.is_some() {
                let expected_length = number_of_phones(cur_phoneme, language);
                if current_sv_note
                    .attributes
                    .dur
                    .as_ref()
                    .map(|dur| dur.len() < expected_length)
                    .unwrap_or(false)
                {
                    current_sv_note
                        .attributes
                        .set_phone_duration(expected_length.saturating_sub(1), 1.0);
                }
            }
            sv_note_list.push(current_sv_note.clone());
            current_note_index = index + 1;
            current_sv_note = next_sv_note;
            current_phone_marks = next_phone_marks;
        }
        let current_note = &notes[current_note_index.min(notes.len() - 1)];
        if current_phone_marks.get(1).copied().unwrap_or(0.0) > 0.0
            && current_note
                .edited_phones
                .as_ref()
                .map(|phones| phones.mid_ratio_over_tail > 0.0)
                .unwrap_or(false)
            && notes[notes.len() - 1].edited_phones.is_some()
        {
            let last_phones = notes[notes.len() - 1].edited_phones.as_ref().unwrap();
            let ratio = last_phones.mid_ratio_over_tail / current_phone_marks[1];
            let x = 2.0 * ratio / (1.0 + ratio);
            let y = 2.0 / (1.0 + ratio);
            let phone_index = if current_phone_marks.first().copied().unwrap_or(0.0) > 0.0 {
                1usize
            } else {
                0usize
            };
            current_sv_note
                .attributes
                .set_phone_duration(phone_index, clamp(x, 0.2, 1.8));
            current_sv_note
                .attributes
                .set_phone_duration(phone_index + 1, clamp(y, 0.2, 1.8));
        }
        if current_sv_note.attributes.dur.is_some() {
            let expected_length = number_of_phones(
                self.lyrics_phonemes.last().map(String::as_str).unwrap_or(""),
                language,
            );
            if current_sv_note
                .attributes
                .dur
                .as_ref()
                .map(|dur| dur.len() < expected_length)
                .unwrap_or(false)
            {
                current_sv_note
                    .attributes
                    .set_phone_duration(expected_length.saturating_sub(1), 1.0);
            }
        }
        sv_note_list.push(current_sv_note);
        sv_note_list
    }

    fn apply_both_phone_ratios(
        &self,
        current_sv_note: &mut SVNote,
        next_sv_note: &mut SVNote,
        current_note: &Note,
        next_note: &Note,
        current_phone_marks: &[f64],
        next_phone_marks: &[f64],
        phone_index: usize,
    ) {
        let (Some(current_edited), Some(next_edited)) =
            (&current_note.edited_phones, &next_note.edited_phones)
        else {
            return;
        };
        let current_main_ratio = current_edited.mid_ratio_over_tail / current_phone_marks[1];
        let next_head_ratio = next_edited.head_length_in_secs / next_phone_marks[0];
        let mut x = 2.0 * current_main_ratio / (1.0 + current_main_ratio);
        let mut y = 2.0 / (1.0 + current_main_ratio);
        let mut z = next_head_ratio;
        if self
            .synchronizer
            .get_duration_secs_from_ticks(current_note.end_pos(), next_note.start_pos)
            < next_phone_marks[0]
        {
            let final_ratio = 2.0 / (1.0 + next_head_ratio);
            x *= final_ratio;
            y *= final_ratio;
            z *= final_ratio;
        }
        current_sv_note
            .attributes
            .set_phone_duration(phone_index, clamp(x, 0.2, 1.8));
        current_sv_note
            .attributes
            .set_phone_duration(phone_index + 1, clamp(y, 0.2, 1.8));
        next_sv_note
            .attributes
            .set_phone_duration(0, clamp(z, 0.2, 1.8));
    }

    fn apply_current_phone_ratios(
        &self,
        current_sv_note: &mut SVNote,
        current_note: &Note,
        current_phone_marks: &[f64],
        phone_index: usize,
    ) {
        let ratio = current_note
            .edited_phones
            .as_ref()
            .map(|phones| phones.mid_ratio_over_tail / current_phone_marks[1])
            .unwrap_or(1.0);
        let x = 2.0 * ratio / (1.0 + ratio);
        let y = 2.0 / (1.0 + ratio);
        current_sv_note
            .attributes
            .set_phone_duration(phone_index, clamp(x, 0.2, 1.8));
        current_sv_note
            .attributes
            .set_phone_duration(phone_index + 1, clamp(y, 0.2, 1.8));
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_next_phone_ratio(
        &self,
        current_sv_note: &mut SVNote,
        next_sv_note: &mut SVNote,
        current_note: &Note,
        next_note: &Note,
        current_phone_marks: &[f64],
        next_phone_marks: &[f64],
        phone_index: usize,
    ) {
        let Some(next_edited) = &next_note.edited_phones else {
            return;
        };
        let mut ratio = next_edited.head_length_in_secs / next_phone_marks[0];
        if self
            .synchronizer
            .get_duration_secs_from_ticks(current_note.end_pos(), next_note.start_pos)
            < next_phone_marks[0]
        {
            let ratio_z = clamp(2.0 * ratio / (1.0 + ratio), 0.2, 1.8);
            let ratio_xy = clamp(2.0 / (1.0 + ratio), 0.2, 1.8);
            current_sv_note
                .attributes
                .set_phone_duration(phone_index, ratio_xy);
            if current_phone_marks.get(1).copied().unwrap_or(0.0) > 0.0 {
                current_sv_note
                    .attributes
                    .set_phone_duration(phone_index + 1, ratio_xy);
            }
            ratio = ratio_z;
        }
        next_sv_note.attributes.set_phone_duration(0, ratio);
    }
}

#[cfg(test)]
mod phoneme_layer_tests {
    use super::*;
    use libresvip_core::model::base::{Note, Project, SingingTrack, TimeSignature, Track};

    fn project(lyric: &str, pronunciation: Option<&str>) -> Project {
        Project {
            version: String::new(),
            song_tempo_list: vec![SongTempo::new(0, 120.0)],
            time_signature_list: vec![TimeSignature::default()],
            track_list: vec![Track::Singing(SingingTrack {
                title: "Track 1".to_owned(),
                note_list: vec![Note {
                    start_pos: 0,
                    length: 480,
                    key_number: 60,
                    lyric: lyric.to_owned(),
                    pronunciation: pronunciation.map(str::to_owned),
                    ..Default::default()
                }],
                ..Default::default()
            })],
        }
    }

    fn first_note_phonemes(project: &Project, write_phonemes: bool) -> String {
        let options = OutputOptions {
            write_phonemes,
            ..OutputOptions::default()
        };
        let generated = SynthVGenerator::new(options)
            .generate_project(project)
            .expect("generate");
        generated.tracks[0].main_group.notes[0].phonemes.clone()
    }

    #[test]
    fn phoneme_layer_is_empty_by_default() {
        let notes = first_note_phonemes(&project("wo", Some("wo")), false);
        assert_eq!(notes, "");
    }

    #[test]
    fn pinyin_is_converted_to_sv_notation() {
        assert_eq!(first_note_phonemes(&project("wo", Some("wo")), true), "w uo");
        assert_eq!(first_note_phonemes(&project("ba", Some("ba")), true), "p a");
        assert_eq!(first_note_phonemes(&project("pa", Some("pa")), true), "ph a");
        // No pronunciation, nothing to write.
        assert_eq!(first_note_phonemes(&project("wo", None), true), "");
    }
}
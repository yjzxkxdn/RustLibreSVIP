//! Port of `libresvip.plugins.acep.ace_studio_generator`.

use libresvip_core::core::constants::TICKS_IN_BEAT;
use libresvip_core::core::lyric_phoneme::chinese::get_pinyin_series;
use libresvip_core::core::lyric_phoneme::japanese::to_romaji;
use libresvip_core::core::lyric_phoneme::korean::romanise;
use libresvip_core::core::tick_counter::skip_tempo_list;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{Note, ParamCurve, Params, Project, SongTempo, Track};
use libresvip_core::model::point::Point;
use libresvip_core::utils::audio::audio_track_info;

use super::base_pitch_curve::BasePitchCurve;
use super::color_pool::{count_color, get_color, random_color_index};
use super::model::{
    AcepAudioPattern, AcepAudioTrack, AcepNote, AcepParamCurve, AcepParamCurveList, AcepParams,
    AcepProject, AcepSeedComposition, AcepSingerConfig, AcepTempo, AcepTimeSignature, AcepTrack,
    AcepVocalPattern, AcepVocalTrack, LATIN_LYRICS_LANGUAGES, LYRICS_LANGUAGE_CHINESE,
    LYRICS_LANGUAGE_JAPANESE, LYRICS_LANGUAGE_KOREAN,
};
use super::options::{OutputOptions, StrengthMappingOption};
use super::singers::{singer_to_id, singer_to_seed, DEFAULT_SEED, DEFAULT_SINGER_ID};

/// `get_pinyin_series(<str>)` first item (Python iterates the characters).
fn pinyin_first(lyric: &str) -> Result<Option<String>> {
    let mut parts: Vec<&str> = Vec::with_capacity(lyric.len());
    for (index, character) in lyric.char_indices() {
        parts.push(&lyric[index..index + character.len_utf8()]);
    }
    Ok(get_pinyin_series(&parts)?.into_iter().next())
}

pub struct AceGenerator {
    options: OutputOptions,
    first_bar_tempo: Vec<SongTempo>,
    first_bar_ticks: i64,
    ace_note_list: Vec<AcepNote>,
    pattern_start: i64,
    synchronizer: TimeSynchronizer,
}

impl AceGenerator {
    pub fn new(options: OutputOptions) -> Self {
        Self {
            options,
            first_bar_tempo: Vec::new(),
            first_bar_ticks: 0,
            ace_note_list: Vec::new(),
            pattern_start: 0,
            synchronizer: TimeSynchronizer::from_project(&[]),
        }
    }

    pub fn generate_project(&mut self, project: &Project) -> Result<AcepProject> {
        let mut ace_project = AcepProject::default();
        let Some(first_time_signature) = project.time_signature_list.first() else {
            return Err(LibreSvipError::msg(
                "the project contains no time signature",
            ));
        };
        self.first_bar_ticks = first_time_signature.bar_length() as i64;
        self.first_bar_tempo = project
            .song_tempo_list
            .iter()
            .filter(|tempo| tempo.position < self.first_bar_ticks)
            .cloned()
            .collect();
        let denominator = first_time_signature.denominator;
        let numerator = first_time_signature.numerator;
        if denominator == 0 {
            return Err(LibreSvipError::msg(
                "the first time signature has a zero denominator",
            ));
        }
        ace_project.beats_per_bar = (numerator * 4).div_euclid(denominator);
        if denominator <= 32 && numerator <= 32 {
            ace_project.time_signatures = vec![AcepTimeSignature {
                bar_pos: 0,
                numerator,
                denominator,
            }];
        }
        self.synchronizer =
            TimeSynchronizer::new(&project.song_tempo_list, self.first_bar_ticks, false, 60.0);
        ace_project.tempos = self.generate_tempos(&project.song_tempo_list);
        for track in project.track_list.iter() {
            if let Some(ace_track) = self.generate_track(track)? {
                ace_project.tracks.push(ace_track);
            }
        }
        let vocal_duration = ace_project
            .tracks
            .iter()
            .filter_map(AcepTrack::as_vocal)
            .map(AcepVocalTrack::length)
            .max()
            .unwrap_or(0);
        ace_project.duration = vocal_duration + 115_200;
        let color_count = count_color();
        let mut color_index = random_color_index();
        for ace_track in ace_project.tracks.iter_mut() {
            ace_track.properties_mut().color = get_color(color_index);
            color_index = (color_index + 1) % color_count;
        }
        ace_project.color_index = color_index;
        Ok(ace_project)
    }

    fn generate_tempos(&self, tempos: &[SongTempo]) -> Vec<AcepTempo> {
        skip_tempo_list(tempos, self.first_bar_ticks)
            .into_iter()
            .map(|tempo| AcepTempo {
                bpm: tempo.bpm,
                position: tempo.position,
                ..Default::default()
            })
            .collect()
    }

    fn generate_track(&mut self, track: &Track) -> Result<Option<AcepTrack>> {
        let mut ace_track = match track {
            Track::Instrumental(instrumental) => {
                let mut ace_audio_track = AcepAudioTrack::default();
                let mut audio_pattern = AcepAudioPattern {
                    path: instrumental.audio_file_path.clone(),
                    ..Default::default()
                };
                audio_pattern.base.pos = self
                    .synchronizer
                    .get_actual_secs_from_ticks(instrumental.offset);
                match audio_track_info(&instrumental.audio_file_path, false) {
                    Some(track_info) => {
                        audio_pattern.base.dur = track_info.duration;
                        audio_pattern.base.clip_dur =
                            audio_pattern.base.dur - audio_pattern.base.clip_pos;
                    }
                    None => return Ok(None),
                }
                ace_audio_track.patterns.push(audio_pattern);
                AcepTrack::Audio(ace_audio_track)
            }
            Track::Singing(singing) => {
                let mut ace_vocal_track = AcepVocalTrack {
                    language: self.options.lyric_language.clone(),
                    ..Default::default()
                };
                let mut singer_config = AcepSingerConfig::default();
                if let (Some(singer_id), Some(seed)) = (
                    singer_to_id(&singing.ai_singer_name),
                    singer_to_seed(&singing.ai_singer_name),
                ) {
                    singer_config.singer.singer_id = Some(singer_id);
                    singer_config.singer.composition.push(AcepSeedComposition {
                        code: seed,
                        ..Default::default()
                    });
                } else {
                    singer_config.singer.singer_id = Some(DEFAULT_SINGER_ID);
                    singer_config.singer.composition.push(AcepSeedComposition {
                        code: DEFAULT_SEED,
                        ..Default::default()
                    });
                }
                ace_vocal_track.singers.push(singer_config);
                if !singing.note_list.is_empty() {
                    let threshold = self.options.split_threshold * TICKS_IN_BEAT;
                    let mut buffer: Vec<Note> = vec![singing.note_list[0].clone()];
                    for index in 1..singing.note_list.len() {
                        let previous = &singing.note_list[index - 1];
                        let current = &singing.note_list[index];
                        if current.start_pos - previous.end_pos() > threshold && threshold > 0 {
                            self.generate_vocal_pattern(
                                &mut buffer,
                                &mut ace_vocal_track,
                                &singing.edited_params,
                            )?;
                        }
                        buffer.push(current.clone());
                    }
                    if !buffer.is_empty() {
                        self.generate_vocal_pattern(
                            &mut buffer,
                            &mut ace_vocal_track,
                            &singing.edited_params,
                        )?;
                    }
                }
                AcepTrack::Vocal(ace_vocal_track)
            }
        };
        let properties = ace_track.properties_mut();
        properties.name = match track {
            Track::Singing(singing) => singing.title.clone(),
            Track::Instrumental(instrumental) => instrumental.title.clone(),
        };
        properties.mute = match track {
            Track::Singing(singing) => singing.mute,
            Track::Instrumental(instrumental) => instrumental.mute,
        };
        properties.solo = match track {
            Track::Singing(singing) => singing.solo,
            Track::Instrumental(instrumental) => instrumental.solo,
        };
        properties.pan = match track {
            Track::Singing(singing) => singing.pan,
            Track::Instrumental(instrumental) => instrumental.pan,
        };
        let volume = match track {
            Track::Singing(singing) => singing.volume,
            Track::Instrumental(instrumental) => instrumental.volume,
        };
        properties.gain = (20.0 * volume.log10()).min(6.0);
        Ok(Some(ace_track))
    }

    fn generate_vocal_pattern(
        &mut self,
        buffer: &mut Vec<Note>,
        ace_track: &mut AcepVocalTrack,
        parameters: &Params,
    ) -> Result<()> {
        if buffer.is_empty() {
            return Ok(());
        }
        // The Python generator keeps the *previous* pattern's note list in
        // `self.ace_note_list` while building the new one (`-`/`+` continuation
        // even mutates it), so mirror that list here.
        let mut previous_notes = match ace_track.patterns.last_mut() {
            Some(pattern) => std::mem::take(&mut pattern.notes),
            None => Vec::new(),
        };
        self.pattern_start = (buffer[0].start_pos - 240).max(0);
        let mut ace_note_list: Vec<AcepNote> = Vec::with_capacity(buffer.len());
        for note in buffer.iter() {
            if !note.lyric.is_empty() {
                ace_note_list.push(self.generate_note(note, &mut previous_notes)?);
            }
        }
        if let Some(pattern) = ace_track.patterns.last_mut() {
            pattern.notes = previous_notes;
        }
        let mut vocal_pattern = AcepVocalPattern::default();
        vocal_pattern.base.pos = self.pattern_start as f64;
        let last_note = buffer.last().expect("buffer is not empty");
        vocal_pattern.base.dur = last_note.end_pos() as f64 - self.pattern_start as f64;
        self.ace_note_list = ace_note_list;
        vocal_pattern.notes = self.ace_note_list.clone();
        vocal_pattern.base.clip_dur = vocal_pattern.base.dur;
        buffer.clear();
        if self.options.breath > 0 {
            self.adjust_breath_tags();
            vocal_pattern.notes = self.ace_note_list.clone();
        }
        vocal_pattern.parameters = self.generate_params(parameters)?;
        ace_track.patterns.push(vocal_pattern);
        Ok(())
    }

    fn adjust_breath_tags(&mut self) {
        for index in 1..self.ace_note_list.len() {
            let breath = self.ace_note_list[index].br_len;
            if breath == 0.0 {
                continue;
            }
            let distance = self.synchronizer.get_duration_secs_from_ticks(
                self.ace_note_list[index - 1].pos,
                self.ace_note_list[index].pos,
            );
            let actual_breath = (distance / 2.0).min(breath);
            let shortened = self.synchronizer.get_actual_ticks_from_secs_offset(
                self.ace_note_list[index - 1].pos,
                distance - actual_breath,
            ) as i64
                - self.ace_note_list[index - 1].pos;
            let previous = &mut self.ace_note_list[index - 1];
            previous.dur = previous.dur.min(shortened);
            self.ace_note_list[index].br_len -= actual_breath;
        }
    }

    fn generate_note(
        &mut self,
        note: &Note,
        previous_notes: &mut Vec<AcepNote>,
    ) -> Result<AcepNote> {
        let mut ace_note = AcepNote {
            pos: note.start_pos - self.pattern_start,
            dur: note.length,
            pitch: note.key_number,
            lyric: note.lyric.trim_start_matches('\'').to_owned(),
            language: self.options.lyric_language.clone(),
            ..Default::default()
        };
        if !note.lyric.contains('-') && !note.lyric.contains('+') {
            let pronunciation: Option<String> = match self.options.lyric_language.as_str() {
                LYRICS_LANGUAGE_CHINESE => pinyin_first(&note.lyric)?,
                LYRICS_LANGUAGE_JAPANESE => {
                    to_romaji(&note.lyric).filter(|value| !value.is_empty())
                }
                LYRICS_LANGUAGE_KOREAN => Some(romanise(&note.lyric, "rr").unwrap_or_default())
                    .filter(|value| !value.is_empty()),
                _ => None,
            };
            // `Note.pronunciation` is written as-is: it carries the source's
            // explicit pronunciation, which is not always in the same notation as
            // the lyric (a Synthesizer V phoneme `p` means pinyin `b`, and an
            // `svp` import has already been converted to pinyin). Comparing the
            // two strings would drop such a pronunciation whenever they happen to
            // look alike, so only `write_syllable = false` suppresses the layer.
            if self.options.write_syllable {
                if let Some(pronunciation_in_note) = note.pronunciation.as_ref() {
                    if Some(pronunciation_in_note) != pronunciation.as_ref() {
                        ace_note.syllable = pronunciation_in_note.clone();
                    }
                }
            }
            if let Some(edited_phones) = note.edited_phones.as_ref() {
                if edited_phones.head_length_in_secs >= 0.0 {
                    ace_note.head_consonants = Some(vec![edited_phones.head_length_in_secs]);
                } else if self.options.default_consonant_length != 0.0 {
                    ace_note.head_consonants =
                        Some(vec![self.options.default_consonant_length]);
                }
            } else if self.options.default_consonant_length != 0.0 {
                ace_note.head_consonants = Some(vec![self.options.default_consonant_length]);
            }
        } else if LATIN_LYRICS_LANGUAGES.contains(&self.options.lyric_language.as_str())
            && ace_note.lyric == "+"
            && !previous_notes.is_empty()
        {
            ace_note.pronunciation = Some("-".to_owned());
            let last_index = previous_notes
                .iter()
                .rposition(|note| note.lyric != "-")
                .unwrap_or(previous_notes.len() - 1);
            let last_lyric = previous_notes[last_index].lyric.clone();
            let (base_lyric, span_index) = match last_lyric.split_once('#') {
                Some((base, index)) => (base.to_owned(), Some(index.to_owned())),
                None => (last_lyric.clone(), None),
            };
            match span_index {
                Some(index) if !index.is_empty() && index.chars().all(|c| c.is_ascii_digit()) => {
                    let next_index = index.parse::<i64>().unwrap_or(0) + 1;
                    ace_note.lyric = format!("{base_lyric}#{next_index}");
                }
                _ => {
                    previous_notes[last_index].lyric = format!("{base_lyric}#1");
                    ace_note.lyric = format!("{base_lyric}#2");
                }
            }
        } else {
            ace_note.lyric = "-".to_owned();
            ace_note.pronunciation = Some("-".to_owned());
        }
        if note.head_tag.as_deref() == Some("V") && self.options.breath > 0 {
            let breath_start_in_secs = self
                .synchronizer
                .get_actual_secs_from_ticks(note.start_pos)
                - self.options.breath as f64 / 1000.0;
            let breath_start_in_ticks = self
                .synchronizer
                .get_actual_ticks_from_secs(breath_start_in_secs);
            ace_note.br_len = (note.start_pos as f64 - breath_start_in_ticks).round_ties_even();
        }
        Ok(ace_note)
    }

    fn linear_transform(lower_bound: f64, middle_value: f64, upper_bound: f64) -> impl Fn(f64) -> f64 {
        move |x: f64| {
            if x >= 0.0 {
                x * (upper_bound - middle_value) / 1000.0 + middle_value
            } else {
                x * (middle_value - lower_bound) / 1000.0 + middle_value
            }
        }
    }

    fn generate_params(&self, parameters: &Params) -> Result<AcepParams> {
        let breath_transform = Self::linear_transform(0.2, 1.0, 2.5);
        let gender_transform = Self::linear_transform(-1.0, 0.0, 1.0);
        let mut result = AcepParams {
            breathiness: self.generate_param_curves(&parameters.breath, &breath_transform)?,
            gender: self.generate_param_curves(&parameters.gender, &gender_transform)?,
            ..Default::default()
        };
        result.pitch_delta = self.generate_pitch_curves(&parameters.pitch)?;
        match self.options.map_strength_info {
            StrengthMappingOption::Both => {
                let energy_transform = Self::linear_transform(0.0, 1.0, 2.0);
                let tension_transform = Self::linear_transform(0.7, 1.0, 1.5);
                result.energy = self.generate_param_curves(&parameters.strength, &|x| {
                    energy_transform(x / 2.0)
                })?;
                result.tension = self.generate_param_curves(&parameters.strength, &|x| {
                    tension_transform(x / 2.0)
                })?;
            }
            StrengthMappingOption::Energy => {
                let transform = Self::linear_transform(0.0, 1.0, 2.0);
                result.energy = self.generate_param_curves(&parameters.strength, &transform)?;
            }
            StrengthMappingOption::Tension => {
                let transform = Self::linear_transform(0.7, 1.0, 1.5);
                result.tension = self.generate_param_curves(&parameters.strength, &transform)?;
            }
        }
        Ok(result)
    }

    fn bounds(&self) -> Result<(i64, i64)> {
        let Some(first) = self.ace_note_list.first() else {
            return Err(LibreSvipError::msg(
                "the vocal pattern contains no notes to parameterize",
            ));
        };
        let last = self.ace_note_list.last().expect("not empty");
        let left_bound = (self.pattern_start + first.pos - 240).max(0);
        let right_bound = (self.pattern_start + last.pos + last.dur).max(0);
        Ok((left_bound, right_bound))
    }

    fn generate_pitch_curves(&self, curve: &ParamCurve) -> Result<AcepParamCurveList> {
        let mut ace_curves = AcepParamCurveList::new();
        let (left_bound, right_bound) = self.bounds()?;
        let base_pitch = BasePitchCurve::new(
            &self.ace_note_list,
            &self.synchronizer,
            self.pattern_start,
            // `build_curve` never queries a tick below `left_bound`; one bar of
            // slack absorbs rounding in `tick_step`.
            self.synchronizer
                .get_actual_secs_from_ticks((left_bound - 480).max(0)),
        );
        let segments = self.segment_groups(curve, left_bound, right_bound);
        for segment in segments {
            if let Some((mut ace_curve, mut tick, tick_step, curve_end)) =
                self.build_curve(&segment, left_bound, right_bound)
            {
                let mut pos = ace_curve.offset;
                let curve_limit = (right_bound - self.pattern_start).min(curve_end);
                let mut guard = 0usize;
                while pos < curve_limit {
                    guard += 1;
                    if guard > 10_000_000 || !tick.is_finite() {
                        break;
                    }
                    let second = self
                        .synchronizer
                        .get_actual_secs_from_ticks((tick - self.first_bar_ticks as f64)
                            .round_ties_even() as i64);
                    ace_curve.values.push(Some(
                        self.get_value_from_segment(&segment, tick) / 100.0
                            - base_pitch.semitone_value_at(second),
                    ));
                    pos += 1;
                    tick += tick_step;
                }
                ace_curves.0.push(ace_curve);
            }
        }
        Ok(ace_curves)
    }

    fn generate_param_curves(
        &self,
        curve: &ParamCurve,
        mapping_func: &dyn Fn(f64) -> f64,
    ) -> Result<AcepParamCurveList> {
        let mut ace_curves = AcepParamCurveList::new();
        let (left_bound, right_bound) = self.bounds()?;
        let segments = self.segment_groups(curve, left_bound, right_bound);
        for segment in segments {
            if let Some((mut ace_curve, mut tick, tick_step, curve_end)) =
                self.build_curve(&segment, left_bound, right_bound)
            {
                let mut pos = ace_curve.offset;
                let curve_limit = (right_bound - self.pattern_start).min(curve_end);
                let mut guard = 0usize;
                while pos < curve_limit {
                    guard += 1;
                    if guard > 10_000_000 || !tick.is_finite() {
                        break;
                    }
                    ace_curve
                        .values
                        .push(Some(mapping_func(self.get_value_from_segment(&segment, tick))));
                    pos += 1;
                    tick += tick_step;
                }
                ace_curves.0.push(ace_curve);
            }
        }
        Ok(ace_curves)
    }

    /// `curve.split_into_segments(-100)` followed by the bound filters.
    fn segment_groups(
        &self,
        curve: &ParamCurve,
        left_bound: i64,
        right_bound: i64,
    ) -> Vec<Vec<Point>> {
        let mut segments: Vec<Vec<Point>> = Vec::new();
        for segment in curve.split_into_segments(-100) {
            let Some(last) = segment.last() else {
                continue;
            };
            if last.x < self.first_bar_ticks {
                continue;
            }
            let start_ticks = if segment[0].x > self.first_bar_ticks {
                segment[0].x - self.first_bar_ticks
            } else {
                0
            };
            let end_ticks = last.x - self.first_bar_ticks;
            if start_ticks <= right_bound && end_ticks >= left_bound {
                segments.push(segment);
            }
        }
        segments
    }

    /// The shared prologue of `generate_pitch_curves` / `generate_param_curves`.
    fn build_curve(
        &self,
        segment: &[Point],
        left_bound: i64,
        right_bound: i64,
    ) -> Option<(AcepParamCurve, f64, f64, i64)> {
        let first_bar_ticks = self.first_bar_ticks;
        let pattern_start = self.pattern_start;
        let start_point = binary_find_last(segment, |point| {
            0 <= point.x - first_bar_ticks && point.x - first_bar_ticks <= left_bound
        })
        .unwrap_or(segment[0]);
        let end_point = binary_find_first(segment, |point| {
            right_bound <= point.x - first_bar_ticks
        })
        .unwrap_or(segment[segment.len() - 1]);
        let mut ace_curve = AcepParamCurve::default();
        ace_curve.offset = start_point.x - first_bar_ticks - pattern_start;
        let curve_end = end_point.x - first_bar_ticks - pattern_start;
        let denominator = curve_end - ace_curve.offset;
        if denominator == 0 {
            return None;
        }
        let mut tick_step = (end_point.x - start_point.x) as f64 / denominator as f64;
        let mut tick = start_point.x as f64;
        let mut guard = 0usize;
        while tick < first_bar_ticks as f64 {
            guard += 1;
            if guard > 10_000_000 || !tick_step.is_finite() {
                break;
            }
            ace_curve.offset += 1;
            tick += tick_step;
        }
        tick = (first_bar_ticks as f64).max(tick);
        let denominator = curve_end - ace_curve.offset;
        if denominator == 0 {
            return None;
        }
        tick_step = (end_point.x as f64 - tick) / denominator as f64;
        let mut guard = 0usize;
        while tick - (first_bar_ticks as f64) < left_bound as f64 {
            guard += 1;
            if guard > 10_000_000 || !tick_step.is_finite() {
                break;
            }
            ace_curve.offset += 1;
            tick += tick_step;
        }
        Some((ace_curve, tick, tick_step, curve_end))
    }

    fn get_value_from_segment(&self, segment: &[Point], ticks: f64) -> f64 {
        let Some(left_point) =
            binary_find_last(segment, |point| point.x as f64 <= ticks)
        else {
            return segment[0].y as f64;
        };
        let Some(right_point) =
            binary_find_first(segment, |point| point.x as f64 > ticks)
        else {
            return segment[segment.len() - 1].y as f64;
        };
        let ratio = (ticks - left_point.x as f64) / (right_point.x as f64 - left_point.x as f64);
        (1.0 - ratio) * left_point.y as f64 + ratio * right_point.y as f64
    }
}

/// `libresvip.utils.search.binary_find_first` over `Point`s.
fn binary_find_first(points: &[Point], predicate: impl Fn(&Point) -> bool) -> Option<Point> {
    let mut left = 0usize;
    if points.is_empty() {
        return None;
    }
    let mut right = points.len() - 1;
    while right > left {
        let middle = (left + right) / 2;
        if predicate(&points[middle]) {
            right = middle;
        } else {
            left = middle + 1;
        }
    }
    if predicate(&points[right]) {
        Some(points[right])
    } else {
        None
    }
}

/// `libresvip.utils.search.binary_find_last` over `Point`s.
fn binary_find_last(points: &[Point], predicate: impl Fn(&Point) -> bool) -> Option<Point> {
    if points.is_empty() {
        return None;
    }
    let mut left = 0usize;
    let mut right = points.len() - 1;
    while right > left {
        let middle = (left + right + 1) / 2;
        if predicate(&points[middle]) {
            left = middle;
        } else {
            right = middle - 1;
        }
    }
    if predicate(&points[left]) {
        Some(points[left])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libresvip_core::model::base::{Note, Project, SingingTrack, TimeSignature, Track};

    /// A romanised lyric (OpenUtau pinyin). The parser stores the lyric itself
    /// as the pronunciation, which is exactly the case that produced the invalid
    /// phonemes in ACE Studio.
    fn project_with(notes: &[(&str, Option<&str>)]) -> Project {
        Project {
            warnings: Vec::new(),
            version: String::new(),
            song_tempo_list: vec![SongTempo::new(0, 120.0)],
            time_signature_list: vec![TimeSignature::default()],
            track_list: vec![Track::Singing(SingingTrack {
                title: "Track 1".to_owned(),
                note_list: notes
                    .iter()
                    .enumerate()
                    .map(|(index, (lyric, pronunciation))| Note {
                        start_pos: index as i64 * 480,
                        length: 480,
                        key_number: 60,
                        lyric: (*lyric).to_owned(),
                        pronunciation: pronunciation.map(str::to_owned),
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            })],
        }
    }

    /// Pinyin lyrics: the parser stores the lyric itself as the pronunciation.
    fn echoed() -> Project {
        project_with(&[("wo", Some("wo")), ("cong", Some("cong"))])
    }

    fn syllables(project: &Project, options: OutputOptions) -> Vec<(String, String)> {
        let generated = AceGenerator::new(options)
            .generate_project(project)
            .expect("generate");
        let vocal = generated
            .tracks
            .iter()
            .find_map(AcepTrack::as_vocal)
            .expect("vocal track");
        vocal
            .patterns
            .iter()
            .flat_map(|pattern| pattern.notes.iter())
            .map(|note| (note.lyric.clone(), note.syllable.clone()))
            .collect()
    }

    /// A stored pronunciation is written as-is, even when it happens to look like
    /// the lyric: the two are not necessarily in the same notation (a Synthesizer
    /// V phoneme `p` is pinyin `b`).
    #[test]
    fn explicit_pronunciation_is_written_even_when_it_looks_like_the_lyric() {
        let notes = syllables(&echoed(), OutputOptions::default());
        assert_eq!(notes[0], ("wo".to_owned(), "wo".to_owned()));
        assert_eq!(notes[1], ("cong".to_owned(), "cong".to_owned()));
    }

    /// The Synthesizer V case: the note keeps the lyric `p`, while the stored
    /// pronunciation is the pinyin `b` its phoneme stands for.
    #[test]
    fn foreign_notation_reaches_the_phoneme_layer() {
        let notes = syllables(&project_with(&[("p", Some("b"))]), OutputOptions::default());
        assert_eq!(notes[0], ("p".to_owned(), "b".to_owned()));
    }

    /// A pronunciation that says something *else* (a phonetic hint, an edited
    /// syllable) is still written.
    #[test]
    fn edited_pronunciation_is_still_written() {
        let notes = syllables(&project_with(&[("wo", Some("uo")), ("cong", Some("uo"))]), OutputOptions::default());
        assert_eq!(notes[0], ("wo".to_owned(), "uo".to_owned()));
        assert_eq!(notes[1], ("cong".to_owned(), "uo".to_owned()));
    }

    /// `write_syllable = false` drops even the genuine pronunciations.
    #[test]
    fn write_syllable_can_be_turned_off() {
        let options = OutputOptions {
            write_syllable: false,
            ..OutputOptions::default()
        };
        let notes = syllables(&project_with(&[("wo", Some("uo")), ("cong", Some("uo"))]), options);
        assert_eq!(notes[0], ("wo".to_owned(), String::new()));
    }
}
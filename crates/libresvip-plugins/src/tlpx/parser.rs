//! Port of `libresvip.plugins.tlp.tunelab_parser`.

use libresvip_core::core::tick_counter::shift_tempo_list;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::core::warning_types::show_warning;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{
    InstrumentalTrack, Note, Project, SingingTrack, SongTempo, TimeSignature, Track,
};
use libresvip_core::model::point::Point;

use super::model::{
    TuneLabMidiPart, TuneLabNote, TuneLabPart, TuneLabProject, TuneLabTempo, TuneLabTimeSignature,
    TuneLabTrack,
};
use super::music::{
    db_to_float, numeric_range, py_round, ratio_to_db, HermiteInterpolator, PiecewiseIntervalDict,
};

#[derive(Clone, Debug)]
pub struct InputOptions {
    pub import_instrumental_track: bool,
    pub import_pitch: bool,
}

impl Default for InputOptions {
    fn default() -> Self {
        Self {
            import_instrumental_track: true,
            import_pitch: true,
        }
    }
}

/// Plugin-scoped `gettext`: the CLI merges the plugin catalog into the core
/// one, the converter itself only sees the core catalog.
fn translate(message: &str) -> String {
    use libresvip_core::core::config::read_settings;
    use libresvip_core::utils::translation::{core_catalog, plugin_catalog, TranslationChain};
    let lang = read_settings(|settings| settings.language.value()).to_owned();
    let mut chain = TranslationChain::new();
    chain.push(core_catalog(&lang));
    chain.push(plugin_catalog("tlpx", &lang));
    chain.gettext(message)
}

pub struct TuneLabParser {
    pub options: InputOptions,
    first_bar_length: i64,
    synchronizer: TimeSynchronizer,
}

impl TuneLabParser {
    pub fn new(options: InputOptions) -> Self {
        Self {
            options,
            first_bar_length: 0,
            synchronizer: TimeSynchronizer::from_project(&[]),
        }
    }

    pub fn parse_project(&mut self, project: &TuneLabProject) -> Result<Project> {
        let time_signature_list = Self::parse_time_signatures(&project.time_signatures);
        let Some(first_signature) = time_signature_list.first() else {
            return Err(LibreSvipError::Validation(
                "project has no time signatures".to_owned(),
            ));
        };
        self.first_bar_length = py_round(first_signature.bar_length());
        let tempo_list = self.parse_tempos(&project.tempos);
        self.synchronizer = TimeSynchronizer::from_project(&tempo_list);
        let track_list = self.parse_tracks(&project.tracks)?;
        Ok(Project {
            version: String::new(),
            song_tempo_list: tempo_list,
            time_signature_list,
            track_list,
        })
    }

    fn parse_time_signatures(time_signatures: &[TuneLabTimeSignature]) -> Vec<TimeSignature> {
        time_signatures
            .iter()
            .map(|time_signature| {
                TimeSignature::new(
                    time_signature.bar_index,
                    time_signature.numerator,
                    time_signature.denominator,
                )
            })
            .collect()
    }

    fn parse_tempos(&self, tempos: &[TuneLabTempo]) -> Vec<SongTempo> {
        let list: Vec<SongTempo> = tempos
            .iter()
            .map(|tempo| SongTempo::new(tempo.pos as i64, tempo.bpm))
            .collect();
        shift_tempo_list(&list, self.first_bar_length)
    }

    fn parse_tracks(&mut self, tracks: &[TuneLabTrack]) -> Result<Vec<Track>> {
        let mut track_list: Vec<Track> = Vec::new();
        for track in tracks {
            let mut singing_index: Option<usize> = None;
            for part in &track.parts {
                match part {
                    TuneLabPart::Audio(part) if self.options.import_instrumental_track => {
                        track_list.push(Track::Instrumental(InstrumentalTrack {
                            title: part.base.name.clone(),
                            mute: track.mute,
                            solo: track.solo,
                            volume: Self::parse_volume(track.gain),
                            pan: track.pan,
                            audio_file_path: part.path.clone(),
                            offset: part.base.pos as i64,
                            ..Default::default()
                        }));
                    }
                    TuneLabPart::Midi(part) if !part.notes.is_empty() => {
                        let needs_new = match singing_index {
                            None => true,
                            Some(index) => match track_list.get(index) {
                                Some(Track::Singing(singing)) => {
                                    !singing.note_list.is_empty()
                                        && singing.note_list.last().map(Note::end_pos).unwrap_or(0)
                                            > part.base.pos as i64
                                }
                                _ => true,
                            },
                        };
                        if needs_new {
                            track_list.push(Track::Singing(SingingTrack {
                                title: track.name.clone(),
                                mute: track.mute,
                                solo: track.solo,
                                volume: Self::parse_volume(track.gain),
                                pan: track.pan,
                                ..Default::default()
                            }));
                            singing_index = Some(track_list.len() - 1);
                        }
                        let index = singing_index.unwrap_or(0);
                        if let Some(Track::Singing(singing)) = track_list.get_mut(index) {
                            singing
                                .note_list
                                .extend(Self::parse_notes(&part.notes, part.base.pos as i64));
                            if self.options.import_pitch {
                                let (mut vibrato_base, mut vibrato_envelope) =
                                    self.parse_vibrato(part);
                                let pitch_points = self.parse_pitch(
                                    part,
                                    part.base.pos as i64,
                                    &mut vibrato_base,
                                    &mut vibrato_envelope,
                                );
                                if !pitch_points.is_empty() {
                                    singing.edited_params.pitch.points.extend(pitch_points);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(track_list)
    }

    fn parse_volume(gain: f64) -> f64 {
        if gain >= 0.0 {
            (gain / ratio_to_db(4.0) + 1.0).min(2.0)
        } else {
            db_to_float(gain)
        }
    }

    fn parse_notes(notes: &[TuneLabNote], offset: i64) -> Vec<Note> {
        let mut note_list: Vec<Note> = Vec::new();
        if !notes.is_empty() {
            let mut next_pos: Option<i64> = None;
            for tlp_note in notes.iter().rev() {
                let mut normalized_duration = tlp_note.dur as i64;
                if let Some(next) = next_pos {
                    let distance = next as f64 - tlp_note.pos;
                    if distance < normalized_duration as f64 {
                        normalized_duration = distance as i64;
                        if normalized_duration > 0 {
                            show_warning(format!(
                                "{}{}",
                                translate("Note overlap detected, cutting note "),
                                tlp_note.lyric
                            ));
                        }
                    }
                }
                if normalized_duration > 0 {
                    note_list.insert(
                        0,
                        Note {
                            start_pos: (tlp_note.pos + offset as f64) as i64,
                            length: normalized_duration,
                            key_number: tlp_note.pitch,
                            lyric: tlp_note.lyric.clone(),
                            pronunciation: tlp_note.pronunciation.clone(),
                            ..Default::default()
                        },
                    );
                } else {
                    show_warning(format!(
                        "{}{}",
                        translate("Note overlap detected, skipping note "),
                        tlp_note.lyric
                    ));
                }
                next_pos = Some(tlp_note.pos as i64);
            }
        }
        note_list
    }

    fn parse_vibrato(
        &self,
        part: &TuneLabMidiPart,
    ) -> (PiecewiseIntervalDict, PiecewiseIntervalDict) {
        let mut vibrato_base = PiecewiseIntervalDict::default();
        let mut vibrato_envelope = PiecewiseIntervalDict::default();
        for vibrato in &part.vibratos {
            let vibrato_start = self
                .synchronizer
                .get_actual_secs_from_ticks(vibrato.pos as i64);
            let vibrato_end = self
                .synchronizer
                .get_actual_secs_from_ticks((vibrato.pos + vibrato.dur) as i64);
            vibrato_base.set_closed_vibrato(
                vibrato_start,
                vibrato_end,
                vibrato_start,
                vibrato.frequency,
                vibrato.amplitude,
                vibrato.phase,
            );
        }
        if let Some(automation) = part
            .automations
            .iter()
            .find(|(key, _)| key == "VibratoEnvelope")
            .map(|(_, value)| value)
        {
            let mut groups: Vec<(f64, Vec<f64>)> = Vec::new();
            for point in &automation.values {
                match groups.last_mut() {
                    Some((value, positions)) if *value == point.value => positions.push(point.pos),
                    _ => groups.push((point.value, vec![point.pos])),
                }
            }
            for (value, positions) in groups {
                let start = self
                    .synchronizer
                    .get_actual_secs_from_ticks(positions[0] as i64);
                let end = self.synchronizer.get_actual_secs_from_ticks(
                    (positions[positions.len() - 1] + 5.0) as i64,
                );
                vibrato_envelope.set_closedopen_constant(start, end, (value + 1.0).max(0.0));
            }
        }
        (vibrato_base, vibrato_envelope)
    }

    fn parse_pitch(
        &self,
        part: &TuneLabMidiPart,
        offset: i64,
        vibrato_base: &mut PiecewiseIntervalDict,
        vibrato_envelope: &mut PiecewiseIntervalDict,
    ) -> Vec<Point> {
        let mut points: Vec<Point> = vec![Point::start_point()];
        for pitch_part in &part.pitch {
            let mut anchor_group: Vec<(f64, f64)> = Vec::new();
            for point in pitch_part {
                if point.value.is_nan() {
                    continue;
                }
                if anchor_group.last().map(|last| last.0 == point.pos) == Some(true) {
                    continue;
                }
                anchor_group.push((point.pos, point.value));
            }
            if anchor_group.len() < 2 {
                continue;
            }
            let interpolator = HermiteInterpolator::new(anchor_group.clone());
            let first_pos = anchor_group[0].0;
            let last_pos = anchor_group[anchor_group.len() - 1].0;
            let xs = numeric_range(first_pos, last_pos + 1.0, 5.0);
            let ys = interpolator.interpolate(&xs);
            for (index, (x, y)) in xs.iter().zip(ys.iter()).enumerate() {
                let pitch_pos = *x as i64 + offset;
                if index == 0 {
                    points.push(Point::new(pitch_pos + self.first_bar_length, -100));
                }
                let pitch_secs = self.synchronizer.get_actual_secs_from_ticks(pitch_pos);
                let mut pitch_value = *y;
                if let Some(mut vibrato_value) = vibrato_base.get(pitch_secs) {
                    vibrato_value *= vibrato_envelope.get(pitch_secs).unwrap_or(1.0);
                    pitch_value += vibrato_value;
                }
                points.push(Point::new(
                    pitch_pos + self.first_bar_length,
                    py_round(pitch_value * 100.0),
                ));
            }
            let last_x = points.last().map(|point| point.x).unwrap_or(0);
            points.push(Point::new(last_x, -100));
        }
        points.push(Point::end_point());
        points
    }
}

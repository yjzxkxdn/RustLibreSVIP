//! Port of `libresvip.plugins.y77.y77_generator`.

use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{Note, Project, SingingTrack, TimeSignature, Track};
use libresvip_core::options::Options;

use super::model::{PitValue, Y77Note, Y77Project};
use super::pinyin;
use super::pitch::PitchSimulator;

pub struct Y77Generator<'a> {
    options: &'a Options,
    synchronizer: Option<TimeSynchronizer>,
    first_bar_length: i64,
}

impl<'a> Y77Generator<'a> {
    pub fn new(options: &'a Options) -> Self {
        Self {
            options,
            synchronizer: None,
            first_bar_length: 0,
        }
    }

    pub fn generate_project(&mut self, project: &Project) -> Result<Y77Project> {
        let track_index = self.options.int("track_index", -1);
        let first_singing_track: Option<&SingingTrack> = if track_index < 0 {
            project
                .track_list
                .iter()
                .filter_map(Track::as_singing)
                .find(|track| !track.note_list.is_empty())
        } else {
            let index = track_index as usize;
            let track = project
                .track_list
                .get(index)
                .ok_or_else(|| LibreSvipError::msg("track index out of range"))?;
            Some(
                track
                    .as_singing()
                    .ok_or_else(|| LibreSvipError::msg("selected track is not a singing track"))?,
            )
        };
        let time_signature = project
            .time_signature_list
            .first()
            .cloned()
            .ok_or_else(|| LibreSvipError::msg("missing time signature"))?;
        self.first_bar_length = time_signature.bar_length() as i64;
        let tempo = project
            .song_tempo_list
            .first()
            .cloned()
            .ok_or_else(|| LibreSvipError::msg("no tempo found"))?;
        self.synchronizer = Some(TimeSynchronizer::new(
            &project.song_tempo_list,
            self.first_bar_length,
            false,
            60.0,
        ));
        let mut y77_project = Y77Project {
            bpm: tempo.bpm,
            bars: 100,
            bbar: time_signature.numerator,
            bbeat: time_signature.denominator,
            ..Default::default()
        };
        if let Some(track) = first_singing_track {
            y77_project.notes = self.generate_notes(track, &[time_signature])?;
            y77_project.nnote = y77_project.notes.len() as i64;
        }
        Ok(y77_project)
    }

    fn generate_notes(
        &mut self,
        singing_track: &SingingTrack,
        time_signatures: &[TimeSignature],
    ) -> Result<Vec<Y77Note>> {
        let synchronizer = self
            .synchronizer
            .clone()
            .ok_or_else(|| LibreSvipError::msg("missing synchronizer"))?;
        let mut pitch_simulator = PitchSimulator::new(
            synchronizer,
            &singing_track.note_list,
            time_signatures,
        )?;
        pitch_simulator.merge_pitch_curve(&singing_track.edited_params.pitch, self.first_bar_length);

        let mut y77_notes = Vec::with_capacity(singing_track.note_list.len());
        for note in &singing_track.note_list {
            let pronunciation = match &note.pronunciation {
                Some(value) if !value.is_empty() => value.clone(),
                _ => pinyin::lazy_pinyin(&note.lyric).join(" "),
            };
            let mut y77_note = Y77Note {
                py: pronunciation,
                length: round_ties_even(note.length as f64 / 30.0),
                start: round_ties_even(note.start_pos as f64 / 30.0),
                lyric: note.lyric.clone(),
                pitch: 88 - note.key_number,
                ..Default::default()
            };
            let (pit, pbs) = self.generate_pitch(&pitch_simulator, note);
            y77_note.pit = pit;
            y77_note.pbs = pbs;
            y77_notes.push(y77_note);
        }
        Ok(y77_notes)
    }

    fn generate_pitch(
        &self,
        pitch_simulator: &PitchSimulator,
        note: &Note,
    ) -> (Vec<PitValue>, i64) {
        let tick_step = note.length as f64 / 500.0;
        let mut rel_pitch_values = Vec::with_capacity(500);
        for i in 0..500 {
            let tick = note.start_pos + (tick_step * i as f64) as i64;
            let value = pitch_simulator.pitch_at_ticks(tick);
            let absolute = match value {
                Some(value) => value / 100.0,
                None => note.key_number as f64,
            };
            rel_pitch_values.push(absolute - note.key_number as f64);
        }
        let max_abs_value = rel_pitch_values
            .iter()
            .map(|value| value.abs())
            .fold(0.0f64, f64::max);
        let pbs_for_this_note = (max_abs_value.ceil() as i64).min(12);
        let pit = rel_pitch_values
            .iter()
            .map(|value| {
                if pbs_for_this_note > 0 {
                    PitValue::Float(value * 50.0 / pbs_for_this_note as f64 + 50.0)
                } else {
                    PitValue::Int(50)
                }
            })
            .collect();
        (pit, pbs_for_this_note - 1)
    }
}

fn round_ties_even(value: f64) -> i64 {
    value.round_ties_even() as i64
}

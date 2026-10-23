//! Port of `libresvip.plugins.nn.niaoniao_generator`.

use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{Project, SingingTrack, SongTempo, TimeSignature, Track};
use libresvip_core::options::Options;

use super::model::{NNInfoLine, NNNote, NNPoints, NNProject, NNTimeSignature};
use super::pinyin;
use super::pitch::PitchSimulator;

pub struct NiaoniaoGenerator<'a> {
    options: &'a Options,
    length_multiplier: i64,
    first_bar_length: i64,
    time_signatures: Vec<TimeSignature>,
    synchronizer: Option<TimeSynchronizer>,
}

impl<'a> NiaoniaoGenerator<'a> {
    pub fn new(options: &'a Options) -> Self {
        Self {
            options,
            length_multiplier: 60,
            first_bar_length: 1920,
            time_signatures: Vec::new(),
            synchronizer: None,
        }
    }

    pub fn generate_project(&mut self, project: &Project) -> Result<NNProject> {
        let version = self.options.int("version", 19);
        self.length_multiplier = if version == 19 { 60 } else { 30 };
        let track_index = self.options.int("track_index", -1);
        let first_singing_track: &SingingTrack = if track_index < 0 {
            project
                .track_list
                .iter()
                .filter_map(Track::as_singing)
                .find(|track| !track.note_list.is_empty())
                .ok_or_else(|| LibreSvipError::msg("No singing track found"))?
        } else {
            let index = track_index as usize;
            project
                .track_list
                .get(index)
                .ok_or_else(|| LibreSvipError::msg("track index out of range"))?
                .as_singing()
                .ok_or_else(|| LibreSvipError::msg("selected track is not a singing track"))?
        };

        self.time_signatures = if project.time_signature_list.is_empty() {
            vec![TimeSignature::default()]
        } else {
            project.time_signature_list.clone()
        };
        let nn_time_signature = self.generate_time_signature(&project.time_signature_list);
        self.synchronizer = Some(TimeSynchronizer::new(
            &project.song_tempo_list,
            self.first_bar_length,
            false,
            60.0,
        ));
        let nn_tempo = self.generate_tempo(&project.song_tempo_list)?;
        let nn_info_line = NNInfoLine {
            tempo: nn_tempo,
            time_signature: nn_time_signature,
            bar_count: 0,
            version,
        };
        let mut nn_project = NNProject {
            info_line: Some(nn_info_line.clone()),
            note_count: 0,
            notes: self.generate_notes(first_singing_track)?,
        };
        nn_project.note_count = nn_project.notes.len() as i64;
        if nn_project.note_count > 0 {
            let last = nn_project.notes.last().expect("note count is non-zero");
            let end = (last.start + last.duration) as f64 * self.length_multiplier as f64;
            let bar_count = (end / self.first_bar_length as f64).ceil() as i64;
            if let Some(info_line) = nn_project.info_line.as_mut() {
                info_line.bar_count = bar_count;
            }
        }
        Ok(nn_project)
    }

    fn generate_time_signature(&mut self, time_signature_list: &[TimeSignature]) -> NNTimeSignature {
        let Some(first) = time_signature_list.first() else {
            self.first_bar_length = 1920;
            return NNTimeSignature::default();
        };
        self.first_bar_length = first.bar_length() as i64;
        NNTimeSignature {
            numerator: first.numerator,
            denominator: first.denominator,
        }
    }

    fn generate_tempo(&self, tempo_list: &[SongTempo]) -> Result<f64> {
        tempo_list
            .first()
            .map(|tempo| tempo.bpm)
            .ok_or_else(|| LibreSvipError::msg("no tempo found"))
    }

    fn generate_notes(&mut self, singing_track: &SingingTrack) -> Result<Vec<NNNote>> {
        let synchronizer = self
            .synchronizer
            .clone()
            .ok_or_else(|| LibreSvipError::msg("missing synchronizer"))?;
        let mut pitch_simulator = PitchSimulator::new(
            synchronizer,
            &singing_track.note_list,
            &self.time_signatures,
        )?;
        pitch_simulator.merge_pitch_curve(&singing_track.edited_params.pitch, self.first_bar_length);

        let mut nn_notes = Vec::with_capacity(singing_track.note_list.len());
        for note in &singing_track.note_list {
            let pronunciation = match &note.pronunciation {
                Some(value) if !value.is_empty() => value.clone(),
                _ => pinyin::lazy_pinyin(&note.lyric).join(" "),
            };
            let mut nn_note = NNNote {
                lyric: note.lyric.clone(),
                pronunciation,
                start: note.start_pos.div_euclid(self.length_multiplier),
                duration: note.length.div_euclid(self.length_multiplier),
                key: 83 - note.key_number,
                ..Default::default()
            };
            let (pitch, sensitivity) = self.generate_pitch(&pitch_simulator, note);
            nn_note.pitch = pitch;
            nn_note.pitch_bend_sensitivity = sensitivity;
            nn_notes.push(nn_note);
        }
        Ok(nn_notes)
    }

    fn generate_pitch(&self, pitch_simulator: &PitchSimulator, note: &libresvip_core::model::base::Note) -> (NNPoints, i64) {
        let tick_step = note.length as f64 / 100.0;
        let mut rel_pitch_values = Vec::with_capacity(100);
        for i in 0..100 {
            let tick = note.start_pos + (tick_step * i as f64) as i64;
            let value = pitch_simulator.pitch_at_ticks(tick);
            rel_pitch_values.push(match value {
                Some(value) => value / 100.0,
                None => 0.0,
            });
        }
        let max_abs_value = rel_pitch_values
            .iter()
            .map(|value| value.abs())
            .fold(0.0f64, f64::max);
        let pbs_for_this_note = (max_abs_value.ceil() as i64).min(12);
        let points: Vec<i64> = rel_pitch_values
            .iter()
            .map(|value| {
                if pbs_for_this_note > 0 {
                    50 + round_ties_even(value / pbs_for_this_note as f64)
                } else {
                    50
                }
            })
            .collect();
        let nn_points = NNPoints {
            point_count: NNPoints::default().point_count,
            points,
        };
        (nn_points, pbs_for_this_note - 1)
    }
}

fn round_ties_even(value: f64) -> i64 {
    value.round_ties_even() as i64
}

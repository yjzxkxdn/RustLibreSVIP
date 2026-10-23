//! Port of `libresvip.plugins.ust.ust_parser`.

use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::core::warning_types::show_warning;
use libresvip_core::model::base::{
    Note, Project, SingingTrack, SongTempo, TimeSignature, Track, DEFAULT_BPM,
};
use libresvip_core::{LibreSvipError, Result};

use super::model as ust_model;
use super::model::UtauProject;
use super::pitch1::{pitch_from_utau_mode1_track, Mode1NotePitchData, Mode1TrackPitchData};
use super::pitch2::{pitch_from_utau_mode2_track, Mode2NotePitchData, Mode2TrackPitchData};

/// One `parse_notes` yield: a run of singing notes plus the raw pitch data.
struct Segment {
    notes: Vec<Note>,
    mode1: Mode1TrackPitchData,
    mode2: Mode2TrackPitchData,
    /// `len(self.tempos)` at the moment the segment was yielded — the Python
    /// code builds a `TimeSynchronizer` snapshot right there.
    tempo_count: usize,
}

/// Port of `USTParser`.
pub struct UstParser {
    import_pitch: bool,
    tempos: Vec<SongTempo>,
}

impl UstParser {
    pub fn new(import_pitch: bool) -> Self {
        Self {
            import_pitch,
            tempos: Vec::new(),
        }
    }

    pub fn parse_project(&mut self, ust_project: &UtauProject) -> Result<Project> {
        if ust_project.track.is_empty() {
            return Err(LibreSvipError::msg("Project has no tracks"));
        }
        let time_signatures = Self::parse_time_signatures(&ust_project.time_signatures);
        let mut tracks: Vec<Track> = Vec::new();
        for ust_track in &ust_project.track {
            let mut track = SingingTrack::default();
            let segments = self.parse_notes(ust_project.tempo, &ust_track.notes)?;
            for segment in segments {
                track.note_list.extend(segment.notes.iter().cloned());
                if self.import_pitch {
                    let tempos: Vec<SongTempo> = self
                        .tempos
                        .iter()
                        .take(segment.tempo_count)
                        .cloned()
                        .collect();
                    let synchronizer = TimeSynchronizer::new(&tempos, 0, false, 60.0);
                    if ust_project.pitch_mode2 {
                        let curve = pitch_from_utau_mode2_track(
                            &segment.mode2,
                            &synchronizer,
                            &segment.notes,
                            &time_signatures,
                        )?;
                        track
                            .edited_params
                            .pitch
                            .points
                            .extend(curve.points.iter().copied());
                    } else {
                        let curve = pitch_from_utau_mode1_track(
                            &segment.mode1,
                            &synchronizer,
                            &segment.notes,
                            &time_signatures,
                        )?;
                        track
                            .edited_params
                            .pitch
                            .points
                            .extend(curve.points.iter().copied());
                    }
                }
            }
            tracks.push(Track::Singing(track));
        }
        Ok(Project {
            version: String::new(),
            song_tempo_list: self.tempos.clone(),
            time_signature_list: time_signatures,
            track_list: tracks,
        })
    }

    fn parse_time_signatures(time_signatures: &[ust_model::TimeSignature]) -> Vec<TimeSignature> {
        if time_signatures.is_empty() {
            return vec![TimeSignature::default()];
        }
        time_signatures
            .iter()
            .map(|time_signature| TimeSignature {
                bar_index: time_signature.bar_index,
                numerator: time_signature.numerator,
                denominator: time_signature.denominator,
            })
            .collect()
    }

    /// `parse_notes` — the Python generator, materialised into a `Vec`.
    fn parse_notes(
        &mut self,
        initial_tempo: Option<f64>,
        notes: &[ust_model::Note],
    ) -> Result<Vec<Segment>> {
        let mut segments: Vec<Segment> = Vec::new();
        let mut note_list: Vec<Note> = Vec::new();
        let mut mode1_track_pitch_data = Mode1TrackPitchData::default();
        let mut mode2_track_pitch_data = Mode2TrackPitchData::default();
        let mut time: i64 = 0;
        if let Some(initial_tempo) = initial_tempo {
            if self.tempos.is_empty() {
                self.add_tempo(time, initial_tempo);
            }
        }
        if self.tempos.is_empty() {
            show_warning("No tempo labels found in the imported project.");
            self.add_tempo(time, DEFAULT_BPM);
        }
        for note in notes {
            if let Some(tempo) = note.tempo {
                self.add_tempo(time, tempo);
            }
            if !note.lyric.to_uppercase().eq("R") {
                note_list.push(Note {
                    start_pos: time,
                    length: note.length,
                    key_number: note.note_num,
                    lyric: note.lyric.clone(),
                    ..Default::default()
                });
                let last_bpm = self
                    .tempos
                    .last()
                    .map(|tempo| tempo.bpm)
                    .unwrap_or(DEFAULT_BPM);
                // Python's `any(getattr(...) is not None ...)` is always true
                // because the `widths`/`shifts`/`curve_types` lists are never
                // `None`, so an entry is appended for every singing note.
                mode2_track_pitch_data.notes.push(Some(Mode2NotePitchData {
                    bpm: last_bpm,
                    start: note.pbs.first().and_then(|value| value.as_f64()),
                    start_shift: note.pbs.get(1).and_then(|value| value.as_f64()),
                    widths: note
                        .pbw
                        .iter()
                        .map(|value| value.as_f64().unwrap_or(0.0))
                        .collect(),
                    shifts: note
                        .pby
                        .iter()
                        .map(|value| value.as_f64().unwrap_or(0.0))
                        .collect(),
                    curve_types: note.pbm.clone(),
                    vibrato_params: note.vbr.clone(),
                }));
                mode1_track_pitch_data.notes.push(Some(Mode1NotePitchData {
                    pitch_points: note.pitch_bend_points.clone(),
                }));
            } else if !note_list.is_empty() {
                segments.push(Segment {
                    notes: std::mem::take(&mut note_list),
                    mode1: std::mem::take(&mut mode1_track_pitch_data),
                    mode2: std::mem::take(&mut mode2_track_pitch_data),
                    tempo_count: self.tempos.len(),
                });
            }
            time += note.length;
        }
        if !note_list.is_empty() {
            segments.push(Segment {
                notes: note_list,
                mode1: mode1_track_pitch_data,
                mode2: mode2_track_pitch_data,
                tempo_count: self.tempos.len(),
            });
        }
        Ok(segments)
    }

    /// `SortedKeyList.add` — insert after every tempo with an equal key.
    fn add_tempo(&mut self, position: i64, bpm: f64) {
        let index = self
            .tempos
            .partition_point(|tempo| tempo.position <= position);
        self.tempos.insert(index, SongTempo::new(position, bpm));
    }
}

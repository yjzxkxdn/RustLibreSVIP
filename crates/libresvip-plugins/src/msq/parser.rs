//! Port of `libresvip.experimental.msq.msq_parser`.

use libresvip_core::core::compat::py_round;
use libresvip_core::core::constants::{DEFAULT_BPM, TICKS_IN_BEAT};
use libresvip_core::core::tick_counter::{find_bar_index, skip_beat_list, skip_tempo_list};
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{
    InstrumentalTrack, Note, ParamCurve, Params, Project, SingingTrack, SongTempo, TimeSignature,
    Track,
};
use libresvip_core::model::point::{Point, END_POINT_X, START_POINT_X};

use crate::mid::pitch::{PitchSimulator, Portamento, RelativePitchCurve};

use super::model::{
    MikotoStudioSequenceFormat, MsqBpmEvent, MsqParameter, MsqSingerClip, MsqSingerTrack,
    MsqTimeSignatureEvent, MsqTrack,
};

/// `InputOptions` — the six enable-import mixins.
#[derive(Clone, Debug)]
pub struct InputOptions {
    pub import_pitch: bool,
    pub import_volume: bool,
    pub import_breath: bool,
    pub import_gender: bool,
    pub import_strength: bool,
    pub import_instrumental_track: bool,
}

impl Default for InputOptions {
    fn default() -> Self {
        Self {
            import_pitch: true,
            import_volume: true,
            import_breath: true,
            import_gender: true,
            import_strength: true,
            import_instrumental_track: true,
        }
    }
}

pub struct MsqParser {
    options: InputOptions,
    ppq: f64,
}

impl MsqParser {
    pub fn new(options: InputOptions) -> Self {
        Self {
            options,
            ppq: TICKS_IN_BEAT as f64,
        }
    }

    pub fn parse_project(&mut self, msq_project: &MikotoStudioSequenceFormat) -> Result<Project> {
        self.ppq = if msq_project.setup.ppq != 0.0 {
            msq_project.setup.ppq
        } else {
            TICKS_IN_BEAT as f64
        };
        let mut tracks: Vec<Track> = Vec::new();
        let tempos = self.parse_tempos(msq_project);
        let time_signatures = self.parse_time_signatures(msq_project);

        for track in &msq_project.tracks {
            match track {
                MsqTrack::Singer(singer) => {
                    tracks.push(Track::Singing(
                        self.parse_singing_track(singer, &tempos, &time_signatures)?,
                    ));
                }
                MsqTrack::Audio(audio) if self.options.import_instrumental_track => {
                    tracks.extend(
                        self.parse_instrumental_tracks(audio)
                            .into_iter()
                            .map(Track::Instrumental),
                    );
                }
                MsqTrack::Audio(_) => {}
            }
        }

        Ok(Project {
            version: String::new(),
            song_tempo_list: skip_tempo_list(&tempos, 0),
            time_signature_list: skip_beat_list(&time_signatures, 0),
            track_list: tracks,
            warnings: Vec::new(),
        })
    }

    fn to_ticks(&self, value: i64) -> i64 {
        py_round(value as f64 * TICKS_IN_BEAT as f64 / self.ppq)
    }

    fn to_project_ticks(&self, clip_start: i64, value: i64) -> i64 {
        self.to_ticks(clip_start + value)
    }

    fn parse_tempos(&self, msq_project: &MikotoStudioSequenceFormat) -> Vec<SongTempo> {
        let mut bpm_events: Vec<MsqBpmEvent> = if msq_project.setup.bpm_events.is_empty() {
            vec![MsqBpmEvent {
                position: 0,
                bpm: if msq_project.setup.bpm != 0.0 {
                    msq_project.setup.bpm
                } else {
                    DEFAULT_BPM
                },
            }]
        } else {
            msq_project.setup.bpm_events.clone()
        };
        bpm_events.sort_by_key(|event| event.position);
        if bpm_events[0].position > 0 {
            bpm_events.insert(
                0,
                MsqBpmEvent {
                    position: 0,
                    bpm: if msq_project.setup.bpm != 0.0 {
                        msq_project.setup.bpm
                    } else {
                        DEFAULT_BPM
                    },
                },
            );
        }
        bpm_events
            .into_iter()
            .map(|event| SongTempo::new(self.to_ticks(event.position), event.bpm))
            .collect()
    }

    fn parse_time_signatures(
        &self,
        msq_project: &MikotoStudioSequenceFormat,
    ) -> Vec<TimeSignature> {
        let mut events: Vec<MsqTimeSignatureEvent> =
            if msq_project.setup.time_signature_events.is_empty() {
                vec![MsqTimeSignatureEvent {
                    position: 0,
                    time_sig: msq_project.setup.time_signature.clone(),
                }]
            } else {
                msq_project.setup.time_signature_events.clone()
            };
        events.sort_by_key(|event| event.position);

        let mut time_signatures: Vec<TimeSignature> = Vec::new();
        let mut prev_position = 0i64;
        let mut prev_time_signature = msq_project.setup.time_signature.clone();
        let mut bar_index = 0i64;
        for (index, event) in events.iter().enumerate() {
            if index == 0 {
                bar_index = if event.position == 0 {
                    0
                } else {
                    py_round(
                        event.position as f64
                            / (self.ppq
                                * 4.0
                                * prev_time_signature.numerator as f64
                                / prev_time_signature.denominator as f64),
                    )
                };
            } else {
                let bar_length = self.ppq
                    * 4.0
                    * prev_time_signature.numerator as f64
                    / prev_time_signature.denominator as f64;
                bar_index += py_round((event.position - prev_position) as f64 / bar_length);
            }
            time_signatures.push(TimeSignature::new(
                bar_index,
                event.time_sig.numerator,
                event.time_sig.denominator,
            ));
            prev_position = event.position;
            prev_time_signature = event.time_sig.clone();
        }
        if time_signatures.is_empty() || time_signatures[0].bar_index > 0 {
            time_signatures.insert(
                0,
                TimeSignature::new(
                    0,
                    msq_project.setup.time_signature.numerator,
                    msq_project.setup.time_signature.denominator,
                ),
            );
        }
        time_signatures
    }

    fn parse_singing_track(
        &self,
        track: &MsqSingerTrack,
        tempos: &[SongTempo],
        time_signatures: &[TimeSignature],
    ) -> Result<SingingTrack> {
        let note_groups: Vec<(&MsqSingerClip, Vec<Note>)> = track
            .clips
            .iter()
            .map(|clip| (clip, self.parse_notes(clip)))
            .collect();
        let mut notes: Vec<Note> = note_groups
            .iter()
            .flat_map(|(_, note_list)| note_list.iter().cloned())
            .collect();
        notes.sort_by_key(|note| note.start_pos);
        self.validate_notes(&notes, time_signatures)?;

        let mut all_pitch_points: Vec<Point> = Vec::new();
        if self.options.import_pitch {
            let first_bar_length = time_signatures
                .first()
                .map(|signature| py_round(signature.bar_length()))
                .unwrap_or(TICKS_IN_BEAT * 4);
            for (clip, clip_notes) in &note_groups {
                if let Some(parameters) = &clip.parameters {
                    if !parameters.pitch.points.is_empty() && !clip_notes.is_empty() {
                        let clip_pitch_points = self.get_pitch_points(
                            &parameters.pitch,
                            clip_notes,
                            tempos,
                            time_signatures,
                            first_bar_length,
                            clip.start,
                        )?;
                        all_pitch_points.extend(
                            clip_pitch_points
                                .into_iter()
                                .filter(|point| point.x != START_POINT_X && point.x != END_POINT_X),
                        );
                    }
                }
            }
        }
        all_pitch_points.sort_by_key(|point| point.x);
        let mut params = Params::default();
        if !all_pitch_points.is_empty() {
            let mut points = vec![Point::start_point()];
            points.extend(all_pitch_points);
            points.push(Point::end_point());
            params.pitch = ParamCurve::new(points);
        }
        self.parse_params(&note_groups, &mut params);
        Ok(SingingTrack {
            title: track.name.clone(),
            mute: track.mixer.mute,
            solo: track.mixer.solo,
            volume: track.mixer.gain,
            pan: track.mixer.pan,
            ai_singer_name: track.instrument.singer.name.clone(),
            note_list: notes,
            edited_params: params,
            ..Default::default()
        })
    }

    fn validate_notes(&self, notes: &[Note], time_signatures: &[TimeSignature]) -> Result<()> {
        for pair in notes.windows(2) {
            if pair[0].start_pos + pair[0].length > pair[1].start_pos {
                return Err(LibreSvipError::msg(format!(
                    "Notes overlapped near bar {}",
                    find_bar_index(time_signatures, pair[1].start_pos)
                        .unwrap_or_default()
                )));
            }
        }
        Ok(())
    }

    fn parse_notes(&self, clip: &MsqSingerClip) -> Vec<Note> {
        clip.notes
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|msq_note| self.parse_note(msq_note, clip.start))
            .collect()
    }

    fn parse_note(&self, msq_note: &super::model::MsqNote, clip_start: i64) -> Note {
        let pronunciation = if msq_note.phonemes.is_empty() {
            None
        } else {
            Some(
                msq_note
                    .phonemes
                    .iter()
                    .map(|phoneme| phoneme.phoneme.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
            )
        };
        Note {
            start_pos: self.to_project_ticks(clip_start, msq_note.start),
            length: self.to_ticks(msq_note.length),
            key_number: msq_note.pitch as i64,
            lyric: if msq_note.lyric.is_empty() {
                String::new()
            } else {
                msq_note.lyric.clone()
            },
            pronunciation,
            ..Default::default()
        }
    }

    fn parse_params(&self, note_groups: &[(&MsqSingerClip, Vec<Note>)], params: &mut Params) {
        let mut volume_points: Vec<Point> = Vec::new();
        let mut breath_points: Vec<Point> = Vec::new();
        let mut gender_points: Vec<Point> = Vec::new();
        let mut strength_points: Vec<Point> = Vec::new();
        for (clip, _) in note_groups {
            let Some(parameters) = &clip.parameters else {
                continue;
            };
            if self.options.import_volume {
                volume_points.extend(self.parse_param_curve(&parameters.dynamics, clip.start));
            }
            if self.options.import_breath {
                breath_points.extend(self.parse_param_curve(&parameters.breathiness, clip.start));
            }
            if self.options.import_gender {
                gender_points.extend(self.parse_param_curve(&parameters.character, clip.start));
            }
            if self.options.import_strength {
                strength_points.extend(self.parse_param_curve(&parameters.tension, clip.start));
            }
        }
        let sorted_curve = |points: Vec<Point>| {
            let mut sorted = points;
            sorted.sort_by_key(|point| point.x);
            ParamCurve::new(sorted)
        };
        if !volume_points.is_empty() {
            params.volume = sorted_curve(volume_points);
        }
        if !breath_points.is_empty() {
            params.breath = sorted_curve(breath_points);
        }
        if !gender_points.is_empty() {
            params.gender = sorted_curve(gender_points);
        }
        if !strength_points.is_empty() {
            params.strength = sorted_curve(strength_points);
        }
    }

    fn parse_param_curve(&self, parameter: &MsqParameter, clip_start: i64) -> Vec<Point> {
        let mut points: Vec<MsqPointSorted> = parameter
            .points
            .iter()
            .map(|point| MsqPointSorted(point.time, point.value))
            .collect();
        points.sort_by_key(|point| point.0);
        points
            .into_iter()
            .map(|MsqPointSorted(time, value)| Point {
                x: self.to_project_ticks(clip_start, time),
                y: py_round(value * 1000.0),
            })
            .collect()
    }

    fn get_pitch_points(
        &self,
        parameter: &MsqParameter,
        notes: &[Note],
        tempos: &[SongTempo],
        time_signatures: &[TimeSignature],
        first_bar_length: i64,
        clip_start: i64,
    ) -> Result<Vec<Point>> {
        let synchronizer = TimeSynchronizer::from_project(tempos);
        let pitch_simulator = PitchSimulator::new(
            &synchronizer,
            Portamento::no_portamento(),
            notes,
            time_signatures,
        )?;
        let mut sorted: Vec<MsqPointSorted> = parameter
            .points
            .iter()
            .map(|point| MsqPointSorted(point.time, point.value))
            .collect();
        sorted.sort_by_key(|point| point.0);
        let relative_points: Vec<Point> = sorted
            .into_iter()
            .map(|MsqPointSorted(time, value)| Point {
                x: self.to_project_ticks(clip_start, time),
                y: py_round(value * 100.0),
            })
            .collect();
        // `RelativePitchCurve(first_bar_length, is_staircase=False)`.
        let curve = RelativePitchCurve {
            first_bar_length,
            is_staircase: false,
            ..RelativePitchCurve::new(first_bar_length)
        };
        Ok(curve
            .to_absolute(&relative_points, &pitch_simulator)?
            .points)
    }

    fn parse_instrumental_tracks(
        &self,
        track: &super::model::MsqAudioTrack,
    ) -> Vec<InstrumentalTrack> {
        track
            .clips
            .iter()
            .filter_map(|clip| {
                clip.audio.as_ref().map(|audio| InstrumentalTrack {
                    title: if clip.name.is_empty() {
                        track.name.clone()
                    } else {
                        clip.name.clone()
                    },
                    mute: track.mixer.mute,
                    solo: track.mixer.solo,
                    volume: track.mixer.gain,
                    pan: track.mixer.pan,
                    audio_file_path: audio.path.clone(),
                    offset: self.to_ticks(clip.start),
                    ..Default::default()
                })
            })
            .collect()
    }
}

/// Sort helper mirroring `sorted(..., key=attrgetter("time"))`.
struct MsqPointSorted(i64, f64);

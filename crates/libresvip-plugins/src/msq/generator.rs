//! Port of `libresvip.experimental.msq.msq_generator`.

use libresvip_core::core::compat::py_round;
use libresvip_core::core::constants::{DEFAULT_BPM, TICKS_IN_BEAT};
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::model::base::{
    InstrumentalTrack, Note, ParamCurve, Params, Project, SingingTrack, SongTempo, TimeSignature,
};
use libresvip_core::model::point::{Point, END_POINT_X};
use libresvip_core::utils::audio::audio_track_info;
use libresvip_core::utils::text::uuid_str;
use serde_json::json;

use crate::mid::pitch::{PitchSimulator, Portamento, RelativePitchCurve};

use super::model::{
    MsqAudio, MsqAudioClip, MsqAudioTrack, MsqBpmEvent, MsqMainOut, MsqMixer, MsqNote,
    MsqParameter, MsqParameters, MsqPoint, MsqSetup, MsqSinger, MsqSingerClip,
    MsqSingerInstrument, MsqSingerTrack, MsqTimeSignature, MsqTimeSignatureEvent,
    MsqTrack, MikotoStudioSequenceFormat,
};

pub struct MsqGenerator {
    ppq: f64,
    first_bar_length: i64,
    synchronizer: TimeSynchronizer,
    time_signatures: Vec<TimeSignature>,
}

impl MsqGenerator {
    pub fn new() -> Self {
        Self {
            ppq: TICKS_IN_BEAT as f64,
            first_bar_length: 1920,
            synchronizer: TimeSynchronizer::from_project(&[]),
            time_signatures: Vec::new(),
        }
    }

    pub fn generate_project(&mut self, project: &Project) -> MikotoStudioSequenceFormat {
        let song_tempo_list: Vec<SongTempo> = if project.song_tempo_list.is_empty() {
            vec![SongTempo::new(0, DEFAULT_BPM)]
        } else {
            project.song_tempo_list.clone()
        };
        self.time_signatures = if project.time_signature_list.is_empty() {
            vec![TimeSignature::default()]
        } else {
            project.time_signature_list.clone()
        };
        self.first_bar_length = py_round(self.time_signatures[0].bar_length());
        self.synchronizer = TimeSynchronizer::from_project(&song_tempo_list);
        let out_id = uuid_str();
        let mut tracks: Vec<MsqTrack> = Vec::new();
        for (index, track) in project.track_list.iter().enumerate() {
            match track {
                libresvip_core::model::base::Track::Singing(singing) => {
                    tracks.push(MsqTrack::Singer(
                        self.generate_singing_track(singing, index as i64, &out_id),
                    ));
                }
                libresvip_core::model::base::Track::Instrumental(instrumental) => {
                    tracks.push(MsqTrack::Audio(
                        self.generate_instrumental_track(instrumental, index as i64, &out_id),
                    ));
                }
            }
        }
        let setup = MsqSetup {
            ppq: self.ppq,
            bpm: song_tempo_list[0].bpm,
            time_signature: MsqTimeSignature {
                numerator: self.time_signatures[0].numerator,
                denominator: self.time_signatures[0].denominator,
            },
            bpm_events: self.generate_bpm_events(&song_tempo_list),
            time_signature_events: self
                .generate_time_signature_events(&self.time_signatures.clone()),
        };
        MikotoStudioSequenceFormat {
            id: uuid_str(),
            setup,
            main_out: MsqMainOut {
                id: out_id,
                name: "Main Out".to_owned(),
            },
            aux: Vec::new(),
            tracks,
            settings: Default::default(),
            version: 110,
        }
    }

    fn to_msq_ticks(&self, value: i64) -> i64 {
        py_round(value as f64 * self.ppq / TICKS_IN_BEAT as f64)
    }

    fn to_relative_msq_ticks(&self, clip_start: i64, value: i64) -> i64 {
        self.to_msq_ticks(value - clip_start)
    }

    fn generate_bpm_events(&self, tempos: &[SongTempo]) -> Vec<MsqBpmEvent> {
        let mut tempo_events: Vec<SongTempo> = tempos.to_vec();
        tempo_events.sort_by_key(|tempo| tempo.position);
        if tempo_events.is_empty() || tempo_events[0].position > 0 {
            let bpm = tempo_events.first().map(|tempo| tempo.bpm).unwrap_or(DEFAULT_BPM);
            tempo_events.insert(0, SongTempo::new(0, bpm));
        }
        tempo_events
            .into_iter()
            .map(|tempo| MsqBpmEvent {
                position: self.to_msq_ticks(tempo.position),
                bpm: tempo.bpm,
            })
            .collect()
    }

    fn generate_time_signature_events(
        &self,
        time_signatures: &[TimeSignature],
    ) -> Vec<MsqTimeSignatureEvent> {
        let mut sorted: Vec<TimeSignature> = time_signatures.to_vec();
        sorted.sort_by_key(|signature| signature.bar_index);
        if sorted.is_empty() {
            sorted.push(TimeSignature::default());
        }
        let mut events = Vec::new();
        let mut position = 0.0f64;
        let mut prev = sorted[0].clone();
        events.push(MsqTimeSignatureEvent {
            position: 0,
            time_sig: MsqTimeSignature {
                numerator: prev.numerator,
                denominator: prev.denominator,
            },
        });
        for time_signature in sorted.iter().skip(1) {
            position += (time_signature.bar_index - prev.bar_index) as f64
                * self.ppq
                * 4.0
                * prev.numerator as f64
                / prev.denominator as f64;
            events.push(MsqTimeSignatureEvent {
                position: py_round(position),
                time_sig: MsqTimeSignature {
                    numerator: time_signature.numerator,
                    denominator: time_signature.denominator,
                },
            });
            prev = time_signature.clone();
        }
        events
    }

    fn generate_singing_track(&mut self, track: &SingingTrack, index: i64, out_id: &str) -> MsqSingerTrack {
        let track_id = uuid_str();
        let singer_id = format!("usid:{}", &uuid_str()[4..23]);
        MsqSingerTrack {
            id: track_id.clone(),
            mixer: MsqMixer {
                gain: track.volume,
                pan: track.pan,
                record_arm: false,
                solo: track.solo,
                mute: track.mute,
                output: out_id.to_owned(),
                bus: uuid_str(),
                sends: Vec::new(),
            },
            name: track.title.clone(),
            color: "orange".to_owned(),
            index,
            effects: serde_json::Value::Null,
            tracks: Vec::new(),
            track_type: "Singer".to_owned(),
            instrument: MsqSingerInstrument {
                singer: MsqSinger {
                    name: if track.ai_singer_name.is_empty() {
                        "Singer".to_owned()
                    } else {
                        track.ai_singer_name.clone()
                    },
                    id: singer_id,
                    primary_expression: uuid_str(),
                    loaded_expressions: Vec::new(),
                    language: "SingerDefault".to_owned(),
                    accent: "SingerDefault".to_owned(),
                    phoneset: "SingerDefault".to_owned(),
                },
            },
            clips: self.generate_singing_clips(track, &track_id),
        }
    }

    fn generate_singing_clips(&mut self, track: &SingingTrack, track_id: &str) -> Vec<MsqSingerClip> {
        if track.note_list.is_empty() {
            return Vec::new();
        }
        let clip_id = uuid_str();
        let mut notes = track.note_list.clone();
        notes.sort_by_key(|note| note.start_pos);
        let clip_start = self.get_clip_start(&track.edited_params, &notes);
        let clip_end = self.get_clip_end(&track.edited_params, &notes);
        let msq_notes: Vec<MsqNote> = notes
            .iter()
            .map(|note| self.generate_note(note, &clip_id, clip_start))
            .collect();
        vec![MsqSingerClip {
            id: clip_id.clone(),
            clip_id: Some(clip_id),
            track_id: track_id.to_owned(),
            name: "Clip".to_owned(),
            // Python: `MsqColor(track_color="orange")`.
            color: json!({"TrackColor": "orange"}),
            start: self.to_msq_ticks(clip_start),
            length: self.to_msq_ticks(clip_end - clip_start),
            kind: "Singer".to_owned(),
            notes: Some(msq_notes),
            parameters: self.generate_parameters(&track.edited_params, &notes, clip_start),
        }]
    }

    fn get_clip_start(&self, params: &Params, notes: &[Note]) -> i64 {
        let curve_points = self.get_param_curve_points(params);
        notes
            .first()
            .map(|note| note.start_pos)
            .unwrap_or(i64::MAX)
            .min(curve_points.iter().copied().min().unwrap_or(i64::MAX))
    }

    fn get_clip_end(&self, params: &Params, notes: &[Note]) -> i64 {
        let curve_points = self.get_param_curve_points(params);
        notes
            .last()
            .map(|note| note.start_pos + note.length)
            .unwrap_or(i64::MIN)
            .max(curve_points.iter().copied().max().unwrap_or(i64::MIN))
    }

    fn get_param_curve_points(&self, params: &Params) -> Vec<i64> {
        let mut points: Vec<i64> = Vec::new();
        points.extend(Self::get_curve_points(&params.pitch, Some(-100)));
        points.extend(Self::get_curve_points(&params.volume, None));
        points.extend(Self::get_curve_points(&params.breath, None));
        points.extend(Self::get_curve_points(&params.gender, None));
        points.extend(Self::get_curve_points(&params.strength, None));
        points
    }

    fn get_curve_points(curve: &ParamCurve, interrupt_value: Option<i64>) -> Vec<i64> {
        curve
            .points
            .iter()
            .filter(|point| 0 <= point.x && point.x < END_POINT_X)
            .filter(|point| interrupt_value.is_none_or(|value| point.y != value))
            .map(|point| point.x)
            .collect()
    }

    fn generate_note(&self, note: &Note, clip_id: &str, clip_start: i64) -> MsqNote {
        MsqNote {
            id: uuid_str(),
            clip_id: clip_id.to_owned(),
            start: self.to_relative_msq_ticks(clip_start, note.start_pos),
            length: self.to_msq_ticks(note.length),
            pitch: note.key_number as f64,
            lyric: note.lyric.clone(),
            phonemes: Vec::new(),
        }
    }

    fn generate_parameters(
        &mut self,
        params: &Params,
        notes: &[Note],
        clip_start: i64,
    ) -> Option<MsqParameters> {
        let mut parameters = MsqParameters::default();
        let mut has_any_param = false;
        if !params.pitch.points.is_empty() && !notes.is_empty() {
            let pitch_points = self.generate_pitch_parameter(&params.pitch, notes, clip_start);
            if !pitch_points.is_empty() {
                parameters.pitch = MsqParameter {
                    value: 0.0,
                    points: pitch_points,
                };
                has_any_param = true;
            }
        }
        if let Some(volume_points) = self.generate_param_curve(&params.volume, clip_start) {
            if !volume_points.is_empty() {
                parameters.dynamics = MsqParameter {
                    value: 0.0,
                    points: volume_points,
                };
                has_any_param = true;
            }
        }
        if let Some(breath_points) = self.generate_param_curve(&params.breath, clip_start) {
            if !breath_points.is_empty() {
                parameters.breathiness = MsqParameter {
                    value: 0.0,
                    points: breath_points,
                };
                has_any_param = true;
            }
        }
        if let Some(gender_points) = self.generate_param_curve(&params.gender, clip_start) {
            if !gender_points.is_empty() {
                parameters.character = MsqParameter {
                    value: 0.0,
                    points: gender_points,
                };
                has_any_param = true;
            }
        }
        if let Some(strength_points) = self.generate_param_curve(&params.strength, clip_start) {
            if !strength_points.is_empty() {
                parameters.tension = MsqParameter {
                    value: 0.0,
                    points: strength_points,
                };
                has_any_param = true;
            }
        }
        if has_any_param {
            Some(parameters)
        } else {
            None
        }
    }

    fn generate_pitch_parameter(
        &mut self,
        pitch: &ParamCurve,
        notes: &[Note],
        clip_start: i64,
    ) -> Vec<MsqPoint> {
        let Ok(pitch_simulator) = PitchSimulator::new(
            &self.synchronizer,
            Portamento::no_portamento(),
            notes,
            &self.time_signatures,
        ) else {
            return Vec::new();
        };
        let mut relative_points: Vec<MsqPoint> = Vec::new();
        // `more_itertools.split_at(points, lambda p: p.y == -100)`: every
        // delimiter starts a fresh, empty part.
        let mut segment: Vec<&Point> = Vec::new();
        let flush = |segment: &mut Vec<&Point>, out: &mut Vec<MsqPoint>| {
            if segment.is_empty() {
                return;
            }
            let owned: Vec<Point> = segment.iter().map(|point| **point).collect();
            segment.clear();
            let curve = RelativePitchCurve::new(self.first_bar_length);
            let relative_segment = curve.from_absolute(&owned, &pitch_simulator);
            out.extend(relative_segment.into_iter().map(|point| MsqPoint {
                time: self.to_relative_msq_ticks(clip_start, point.x + self.first_bar_length),
                value: point.y as f64 / 100.0,
            }));
        };
        for point in &pitch.points {
            if point.y == -100 {
                flush(&mut segment, &mut relative_points);
            } else {
                segment.push(point);
            }
        }
        flush(&mut segment, &mut relative_points);
        relative_points
    }

    fn generate_param_curve(&self, curve: &ParamCurve, clip_start: i64) -> Option<Vec<MsqPoint>> {
        let mut points = Vec::new();
        for point in &curve.points {
            if 0 <= point.x && point.x < END_POINT_X {
                points.push(MsqPoint {
                    time: self.to_relative_msq_ticks(clip_start, point.x),
                    value: point.y as f64 / 1000.0,
                });
            }
        }
        Some(points)
    }

    fn generate_instrumental_track(
        &mut self,
        track: &InstrumentalTrack,
        index: i64,
        out_id: &str,
    ) -> MsqAudioTrack {
        let track_id = uuid_str();
        let clip_id = uuid_str();
        let mut clip_length = 0i64;
        // Python's msq generator relies on the `only_wav=False` default.
        if let Some(track_info) = audio_track_info(&track.audio_file_path, false) {
            clip_length = self.to_msq_ticks(
                self.synchronizer
                    .get_actual_ticks_from_secs_offset(track.offset, track_info.duration)
                    as i64
                    - track.offset,
            );
        }
        MsqAudioTrack {
            id: track_id.clone(),
            mixer: MsqMixer {
                gain: track.volume,
                pan: track.pan,
                record_arm: false,
                solo: track.solo,
                mute: track.mute,
                output: out_id.to_owned(),
                bus: uuid_str(),
                sends: Vec::new(),
            },
            name: track.title.clone(),
            color: "orange".to_owned(),
            index,
            effects: serde_json::Value::Null,
            tracks: Vec::new(),
            track_type: "Audio".to_owned(),
            instrument: "AudioPlayer".to_owned(),
            clips: vec![MsqAudioClip {
                id: clip_id.clone(),
                clip_id: Some(clip_id),
                track_id,
                name: track.title.clone(),
                color: json!("ThemeDefault"),
                start: self.to_msq_ticks(track.offset),
                length: clip_length,
                kind: "Audio".to_owned(),
                audio: Some(MsqAudio {
                    path: track.audio_file_path.clone(),
                    start: 0.0,
                    speed: 1.0,
                    bpm: 0.0,
                }),
            }],
        }
    }
}

impl Default for MsqGenerator {
    fn default() -> Self {
        Self::new()
    }
}

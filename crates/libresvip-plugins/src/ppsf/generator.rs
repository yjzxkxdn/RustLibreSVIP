//! Port of `libresvip.plugins.ppsf.piapro_studio_generator` (NT ZIP/JSON format).

use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{Note, ParamCurve, Project, SongTempo, TimeSignature, Track};

use super::audio::audio_track_info;
use super::interval_dict::{ppsf_key_interval_dict, KeyIntervalDict};
use super::model::{
    PpsfAudioTrackEvent, PpsfAudioTrackItem, PpsfBaseSequence, PpsfCurvePoint, PpsfCurvePointSeq,
    PpsfCurveType, PpsfDvlTrackEvent, PpsfDvlTrackItem, PpsfEventTrack, PpsfFileAudioData,
    PpsfMeter, PpsfMeters, PpsfNote, PpsfParamPoint, PpsfProject, PpsfRegion, PpsfSeqParam,
    PpsfSyllable, PpsfTempo, PpsfTempos,
};
use super::util::py_round;
use super::xsampa::symbols_for_lyric;

pub struct PiaproStudioGenerator<'a> {
    _options: &'a libresvip_core::options::Options,
    first_bar_length: i64,
    time_synchronizer: TimeSynchronizer,
}

impl<'a> PiaproStudioGenerator<'a> {
    pub fn new(options: &'a libresvip_core::options::Options) -> Self {
        Self {
            _options: options,
            first_bar_length: 0,
            time_synchronizer: TimeSynchronizer::from_project(&[SongTempo::default()]),
        }
    }

    pub fn generate_project(&mut self, project: &Project) -> Result<PpsfProject> {
        let first_time_signature = project
            .time_signature_list
            .first()
            .ok_or_else(|| LibreSvipError::Validation("time signature list is empty".to_owned()))?;
        // `int(...)` truncates (it is not `round`).
        self.first_bar_length = first_time_signature.bar_length() as i64;
        self.time_synchronizer = TimeSynchronizer::from_project(&project.song_tempo_list);
        let mut ppsf_project = PpsfProject::default();
        ppsf_project.ppsf.project.meter = generate_time_signatures(&project.time_signature_list);
        ppsf_project.ppsf.project.tempo = generate_tempos(&project.song_tempo_list);
        let mut event_tracks =
            std::mem::take(&mut ppsf_project.ppsf.gui_settings.track_editor.event_tracks);
        let dvl_tracks = self.generate_singing_tracks(&project.track_list, &mut event_tracks);
        let audio_tracks =
            self.generate_instrumental_tracks(&project.track_list, &mut event_tracks);
        ppsf_project.ppsf.project.dvl_track = Some(dvl_tracks);
        ppsf_project.ppsf.project.audio_track = audio_tracks;
        ppsf_project.ppsf.gui_settings.track_editor.event_tracks = event_tracks;
        ppsf_project.ppsf.gui_settings.project_length = self.first_bar_length;
        for event_track in &ppsf_project.ppsf.gui_settings.track_editor.event_tracks {
            for region in &event_track.regions {
                ppsf_project.ppsf.gui_settings.project_length =
                    ppsf_project.ppsf.gui_settings.project_length.max(
                        region.position + region.length + self.first_bar_length,
                    );
            }
        }
        Ok(ppsf_project)
    }

    fn generate_singing_tracks(
        &mut self,
        tracks: &[Track],
        event_tracks: &mut Vec<PpsfEventTrack>,
    ) -> Vec<PpsfDvlTrackItem> {
        let mut dvl_tracks = Vec::new();
        for track in tracks {
            let Track::Singing(track) = track else {
                continue;
            };
            let mute_flag = mute_flag(track.mute, track.solo);
            let mut event_track = PpsfEventTrack {
                index: event_tracks.len() as i64,
                track_type: 4,
                mute_solo: mute_flag,
                notes: Vec::new(),
                nt_envelope_preset_id: Some(2),
                ..Default::default()
            };
            let (track_events, ppsf_notes) = self.generate_notes(&track.note_list);
            event_track.notes = ppsf_notes;
            let region_length = track_events
                .last()
                .map(|event| event.pos + event.length)
                .unwrap_or(0);
            event_track.regions.push(PpsfRegion {
                auto_expand_left: Some(true),
                auto_expand_right: Some(true),
                position: 0,
                length: region_length,
                ..Default::default()
            });
            let key_interval_dict = ppsf_key_interval_dict(&track_events, &event_track.notes);
            let (pitch_param, pitch_curve_point) = self.generate_pitch(
                &track.edited_params.pitch,
                &key_interval_dict,
                &track.note_list,
            );
            event_track.curve_points.push(pitch_curve_point);
            dvl_tracks.push(PpsfDvlTrackItem {
                name: track.title.clone(),
                events: track_events,
                parameters: vec![pitch_param],
                ..Default::default()
            });
            event_tracks.push(event_track);
        }
        dvl_tracks
    }

    fn generate_pitch(
        &self,
        pitch: &ParamCurve,
        key_interval_dict: &KeyIntervalDict,
        notes: &[Note],
    ) -> (PpsfSeqParam, PpsfCurvePoint) {
        let mut pitch_param = PpsfSeqParam::default();
        let mut curve_point = PpsfCurvePoint {
            sub_track_category: 7,
            sub_track_id: 2,
            ..Default::default()
        };
        let mut base_sequence = PpsfBaseSequence {
            constant: 0,
            name: "pitch_bend".to_owned(),
            sequence: Vec::new(),
            use_sequence: true,
        };
        let mut in_pitch_part = false;
        for point in &pitch.points {
            let pos = point.x - self.first_bar_length;
            if point.y == -100 {
                if point.x != -192_000
                    && point.x != 1_073_741_823
                    && in_pitch_part
                    && !base_sequence.sequence.is_empty()
                {
                    let last = base_sequence.sequence[base_sequence.sequence.len() - 1];
                    base_sequence.sequence.push(PpsfParamPoint {
                        curve_type: PpsfCurveType::BORDER,
                        pos: last.pos,
                        value: last.value,
                        ..Default::default()
                    });
                    curve_point.sequence.push(PpsfCurvePointSeq {
                        border_type: Some(7),
                        abs_value: Some(-1),
                        edited_by_user: Some(true),
                        region_index: -1,
                        note_index: Some(-1),
                        seg_array_id: Some(-1),
                    });
                    in_pitch_part = false;
                }
            } else if let Some(base_key) = key_interval_dict.get(pos) {
                if !in_pitch_part {
                    base_sequence.sequence.push(PpsfParamPoint {
                        curve_type: PpsfCurveType::BORDER,
                        pos: pos - 1,
                        value: 0,
                        ..Default::default()
                    });
                    curve_point.sequence.push(PpsfCurvePointSeq {
                        border_type: Some(6),
                        abs_value: Some(-1),
                        edited_by_user: Some(true),
                        region_index: -1,
                        note_index: Some(-1),
                        seg_array_id: Some(-1),
                    });
                    in_pitch_part = true;
                }
                base_sequence.sequence.push(PpsfParamPoint {
                    curve_type: PpsfCurveType::NORMAL,
                    pos,
                    value: point.y - py_round(base_key * 100.0),
                    ..Default::default()
                });
                curve_point.sequence.push(PpsfCurvePointSeq {
                    border_type: Some(1),
                    abs_value: Some(point.y),
                    edited_by_user: Some(true),
                    region_index: -1,
                    note_index: Some(find_index(notes, pos)),
                    seg_array_id: Some(-1),
                });
            }
        }
        pitch_param.base_sequence = Some(base_sequence);
        (pitch_param, curve_point)
    }

    fn generate_instrumental_tracks(
        &mut self,
        tracks: &[Track],
        event_tracks: &mut Vec<PpsfEventTrack>,
    ) -> Vec<PpsfAudioTrackItem> {
        let mut audio_tracks = Vec::new();
        for track in tracks {
            let Track::Instrumental(track) = track else {
                continue;
            };
            let Some(info) = audio_track_info(&track.audio_file_path, true) else {
                continue;
            };
            let offset = self
                .time_synchronizer
                .get_actual_secs_from_ticks(track.offset);
            let tick_length = py_round(
                self.time_synchronizer
                    .get_actual_ticks_from_secs(offset + info.duration)
                    - track.offset as f64,
            );
            audio_tracks.push(PpsfAudioTrackItem {
                name: track.title.clone(),
                events: vec![PpsfAudioTrackEvent {
                    tick_length,
                    tick_pos: track.offset,
                    file_audio_data: PpsfFileAudioData {
                        file_path: track.audio_file_path.clone(),
                        tempo: 0,
                    },
                    ..Default::default()
                }],
                ..Default::default()
            });
            let mute_flag = mute_flag(track.mute, track.solo);
            event_tracks.push(PpsfEventTrack {
                index: event_tracks.len() as i64,
                track_type: 1,
                mute_solo: mute_flag,
                regions: vec![PpsfRegion {
                    length: tick_length,
                    position: track.offset,
                    audio_event_index: Some(0),
                    ..Default::default()
                }],
                ..Default::default()
            });
        }
        audio_tracks
    }

    fn generate_notes(&self, notes: &[Note]) -> (Vec<PpsfDvlTrackEvent>, Vec<PpsfNote>) {
        let mut track_events = Vec::new();
        let mut ppsf_notes = Vec::new();
        for (index, note) in notes.iter().enumerate() {
            let symbols = symbols_for_lyric(&note.lyric);
            track_events.push(PpsfDvlTrackEvent {
                note_number: note.key_number,
                pos: note.start_pos,
                length: note.length,
                lyric: note.lyric.clone(),
                symbols: symbols.clone(),
                ..Default::default()
            });
            ppsf_notes.push(PpsfNote {
                region_index: 0,
                event_index: Some(index as i64),
                length: Some(note.length),
                syllables: vec![PpsfSyllable {
                    lyric_text: note.lyric.clone(),
                    symbols_text: symbols,
                    ..Default::default()
                }],
                ..Default::default()
            });
        }
        (track_events, ppsf_notes)
    }
}

/// `generate_time_signatures`.
pub fn generate_time_signatures(time_signatures: &[TimeSignature]) -> PpsfMeters {
    let mut ppsf_meters = PpsfMeters::default();
    if let Some(first) = time_signatures.first() {
        ppsf_meters.const_meter.nume = first.numerator;
        ppsf_meters.const_meter.denomi = first.denominator;
    }
    if time_signatures.len() > 1 {
        ppsf_meters.use_sequence = true;
        ppsf_meters.sequence = time_signatures[1..]
            .iter()
            .map(|time_signature| PpsfMeter {
                nume: time_signature.numerator,
                denomi: time_signature.denominator,
                measure: time_signature.bar_index,
            })
            .collect();
    }
    ppsf_meters
}

/// `generate_tempos`.
pub fn generate_tempos(tempos: &[SongTempo]) -> PpsfTempos {
    let mut ppsf_tempos = PpsfTempos::default();
    if let Some(first) = tempos.first() {
        ppsf_tempos.const_tempo = py_round(first.bpm * 10_000.0);
    }
    if tempos.len() > 1 {
        ppsf_tempos.use_sequence = true;
        ppsf_tempos.sequence = tempos[1..]
            .iter()
            .map(|tempo| PpsfTempo {
                value: py_round(tempo.bpm * 10_000.0),
                tick: tempo.position,
                curve_type: PpsfCurveType::BORDER,
            })
            .collect();
    }
    ppsf_tempos
}

/// `"-1 if mute else 1 if solo else 0"`.
fn mute_flag(mute: bool, solo: bool) -> i64 {
    if mute {
        -1
    } else if solo {
        1
    } else {
        0
    }
}

/// `libresvip.utils.search.find_index` — first matching index or `-1`.
fn find_index(notes: &[Note], pos: i64) -> i64 {
    notes
        .iter()
        .position(|note| note.start_pos <= pos && pos <= note.end_pos())
        .map(|index| index as i64)
        .unwrap_or(-1)
}

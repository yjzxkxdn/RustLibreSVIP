//! Port of `libresvip.deprecated.gj.gjgj_generator`.

use libresvip_core::core::compat::py_round;
use libresvip_core::core::constants::TICKS_IN_BEAT;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::core::warning_types::{show_warning_kind, WarningKind};
use libresvip_core::model::base::{
    Note, ParamCurve, Project, SongTempo, TimeSignature, Track,
};
use libresvip_core::utils::translation::gettext_lazy;

use super::constants::{DEFAULT_SINGER_ID, SINGER_TO_ID, SUPPORTED_PINYIN};
use super::model::{
    GjgjBeatItems, GjgjBeatStyle, GjgjInstrumentalTrack, GjgjPoint, GjgjProject, GjgjProjectSetting,
    GjgjSingingTrack, GjgjTempoMap, GjgjTempos, GjgjTimeSignature, GjgjTone, GjgjTrackVolume,
    GjgjVolumeMap,
};

/// `OutputOptions`.
#[derive(Clone, Debug)]
pub struct OutputOptions {
    pub down_sample: i64,
    pub singer: String,
}

impl Default for OutputOptions {
    fn default() -> Self {
        Self {
            down_sample: 32,
            singer: "扇宝".to_owned(),
        }
    }
}

/// `bpm2tempo` (`libresvip.utils.binary.midi`).
fn bpm2tempo(bpm: f64) -> i64 {
    py_round(60_000_000.0 / bpm)
}

pub struct GjgjGenerator {
    options: OutputOptions,
    max_note_id: i64,
    first_bar_length: i64,
    first_bar_bpm: f64,
    time_synchronizer: TimeSynchronizer,
}

impl GjgjGenerator {
    pub fn new(options: OutputOptions) -> Self {
        Self {
            options,
            max_note_id: 1,
            first_bar_length: 0,
            first_bar_bpm: 120.0,
            time_synchronizer: TimeSynchronizer::from_project(&[]),
        }
    }

    pub fn generate_project(&mut self, project: &Project) -> GjgjProject {
        self.max_note_id = 1;
        self.time_synchronizer = TimeSynchronizer::from_project(&project.song_tempo_list);
        // Python builds the tempo map first: generate_tempos/generate_time_
        // signatures set first_bar_(bpm/length), which generate_tracks reads.
        let tempos = self.generate_tempos(&project.song_tempo_list);
        let time_signatures = self.generate_time_signatures(&project.time_signature_list);
        let (tracks, accompaniments) = self.generate_tracks(&project.track_list);
        GjgjProject {
            gjgj_version: Some(2),
            project_setting: Some(GjgjProjectSetting::default()),
            tempo_map: GjgjTempoMap {
                ticks_per_quarter_note: TICKS_IN_BEAT,
                tempos,
                time_signature: time_signatures,
            },
            accompaniments,
            tracks,
            midi_tracks: Vec::new(),
        }
    }

    fn generate_tempos(&mut self, song_tempo_list: &[SongTempo]) -> Vec<GjgjTempos> {
        self.first_bar_bpm = song_tempo_list
            .first()
            .map(|tempo| tempo.bpm)
            .unwrap_or(120.0);
        song_tempo_list
            .iter()
            .map(|tempo| GjgjTempos {
                microseconds_per_quarter_note: bpm2tempo(tempo.bpm),
                time: tempo.position,
            })
            .collect()
    }

    fn generate_time_signatures(
        &mut self,
        time_signature_list: &[TimeSignature],
    ) -> Vec<GjgjTimeSignature> {
        self.first_bar_length = time_signature_list
            .first()
            .map(|signature| signature.bar_length() as i64)
            .unwrap_or(1920);
        let mut gjgj_time_signatures = Vec::new();
        let mut prev_ticks = 0i64;
        for time_signature in time_signature_list {
            gjgj_time_signatures.push(GjgjTimeSignature {
                numerator: time_signature.numerator,
                denominator: time_signature.denominator,
                time: prev_ticks,
            });
            // Faithful to Python: `bar_index * bar_length` (not a span), so
            // `time` accumulates absolute positions of each signature's bar.
            prev_ticks +=
                py_round(time_signature.bar_index as f64 * time_signature.bar_length());
        }
        gjgj_time_signatures
    }

    fn generate_tracks(
        &mut self,
        track_list: &[Track],
    ) -> (Vec<GjgjSingingTrack>, Vec<GjgjInstrumentalTrack>) {
        let mut singing_tracks = Vec::new();
        let mut instrumental_tracks = Vec::new();
        for (index, track) in track_list.iter().enumerate() {
            let track_index = (index + 1) as i64;
            match track {
                Track::Singing(singing) => {
                    singing_tracks.push(self.generate_singing_track(singing, track_index));
                }
                Track::Instrumental(instrumental) => {
                    instrumental_tracks
                        .push(self.generate_instrumental_track(instrumental, track_index));
                }
            }
        }
        (singing_tracks, instrumental_tracks)
    }

    fn generate_singing_track(
        &mut self,
        track: &libresvip_core::model::base::SingingTrack,
        track_index: i64,
    ) -> GjgjSingingTrack {
        // Faithful to Python: the *singer id* is written into `Name`.
        let singer_name = if self.options.singer.is_empty() {
            track.ai_singer_name.as_str()
        } else {
            self.options.singer.as_str()
        };
        GjgjSingingTrack {
            id_value: Some(track_index.to_string()),
            name: Some(
                SINGER_TO_ID
                    .iter()
                    .find(|(known, _)| *known == singer_name)
                    .map(|(_, id)| *id)
                    .unwrap_or(DEFAULT_SINGER_ID)
                    .to_owned(),
            ),
            master_volume: Some(GjgjTrackVolume {
                mute: Some(track.mute),
                ..Default::default()
            }),
            beat_items: self.generate_notes(&track.note_list),
            tone: self.generate_pitch(&track.edited_params.pitch),
            volume_map: self.generate_volume(&track.edited_params.volume),
            ..Default::default()
        }
    }

    fn generate_instrumental_track(
        &mut self,
        track: &libresvip_core::model::base::InstrumentalTrack,
        track_index: i64,
    ) -> GjgjInstrumentalTrack {
        GjgjInstrumentalTrack {
            id_value: Some(track_index.to_string()),
            // Python's GjgjInstrumentalTrack.type_ defaults to 1.
            type_: Some(1),
            path: Some(track.audio_file_path.clone()),
            offset: self.position_to_time(track.offset),
            master_volume: Some(GjgjTrackVolume {
                mute: Some(track.mute),
                ..Default::default()
            }),
            eq_program: Some(String::new()),
            sort_index: Some(0),
        }
    }

    fn position_to_time(&self, position: i64) -> i64 {
        if position > 0 {
            py_round(self.time_synchronizer.get_actual_secs_from_ticks(position) * 10_000_000.0)
        } else {
            py_round(position as f64 / TICKS_IN_BEAT as f64 * 60.0 / self.first_bar_bpm * 10_000_000.0)
        }
    }

    fn generate_notes(&mut self, note_list: &[Note]) -> Vec<GjgjBeatItems> {
        // Python: `get_pinyin_series([note.lyric for note in note_list])`
        // returns one phoneme string per note ("wo men" style).
        let lyrics: Vec<String> = note_list.iter().map(|note| note.lyric.clone()).collect();
        let pinyin_list: Vec<String> = lyrics
            .iter()
            .map(|lyric| {
                libresvip_core::core::lyric_phoneme::chinese::lazy_pinyin(lyric)
                    .map(|syllables| syllables.join(" "))
                    .unwrap_or_else(|_| lyric.clone())
            })
            .collect();
        let mut notes = Vec::with_capacity(note_list.len());
        for (note, pinyin) in note_list.iter().zip(pinyin_list.iter()) {
            let mut pronunciation = note
                .pronunciation
                .clone()
                .unwrap_or_else(|| pinyin.clone());
            if !SUPPORTED_PINYIN.contains(&pronunciation.as_str()) {
                show_warning_kind(
                    WarningKind::PhonemeDropped,
                    format!("{} {pronunciation}", gettext_lazy("Unsupported pinyin:")),
                );
                pronunciation = String::new();
            }
            let beat_style = match note.head_tag.as_deref() {
                Some("0") => GjgjBeatStyle::Sil,
                Some("V") => GjgjBeatStyle::Sp,
                _ => GjgjBeatStyle::None,
            };
            let mut pre_time = 0.0f64;
            let mut post_time = 0.0f64;
            if let Some(edited_phones) = &note.edited_phones {
                if edited_phones.head_length_in_secs != -1.0 {
                    let note_start_pos_in_secs = self
                        .time_synchronizer
                        .get_actual_secs_from_ticks(note.start_pos);
                    let phone_head_position_in_secs =
                        note_start_pos_in_secs + edited_phones.head_length_in_secs;
                    let phone_head_position_in_ticks = self
                        .time_synchronizer
                        .get_actual_ticks_from_secs(phone_head_position_in_secs);
                    let difference = note.start_pos - py_round(phone_head_position_in_ticks);
                    pre_time = -difference as f64 * (2000.0 / 3.0) / TICKS_IN_BEAT as f64;
                }
                if edited_phones.mid_ratio_over_tail != -1.0 {
                    post_time = -(note.length as f64
                        / (1.0 + edited_phones.mid_ratio_over_tail))
                        * (2000.0 / 3.0)
                        / TICKS_IN_BEAT as f64;
                }
            }
            notes.push(GjgjBeatItems {
                id_value: Some(self.max_note_id),
                lyric: note.lyric.clone(),
                pinyin: Some(pronunciation),
                start_tick: note.start_pos + self.first_bar_length,
                duration: note.length,
                track: Some(note.key_number),
                style: Some(beat_style),
                pre_time,
                post_time,
                velocity: Some(127),
            });
            self.max_note_id += 1;
        }
        notes
    }

    fn generate_pitch(&self, pitch: &ParamCurve) -> GjgjTone {
        let mut ticks_buffer: Vec<i64> = Vec::new();
        let mut value_buffer: Vec<f64> = Vec::new();
        let mut ori_prev_ticks: i64 = -100;
        let mut pitch_points: Vec<GjgjPoint> = Vec::new();
        let mut modify_ranges: Vec<GjgjPoint> = Vec::new();
        for point in &pitch.points {
            let (ori_ticks, ori_value) = (point.x, point.y);
            if ori_prev_ticks != ori_ticks {
                if ori_value != -100 && ori_ticks != -192000 && ori_ticks != 1073741823 {
                    ticks_buffer.push(ori_ticks - self.first_bar_length);
                    value_buffer.push(ori_value as f64);
                } else if !ticks_buffer.is_empty() && !value_buffer.is_empty() {
                    pitch_points.extend(
                        ticks_buffer
                            .iter()
                            .zip(value_buffer.iter())
                            .map(|(&tick, &value)| GjgjPoint {
                                x: tick as f64,
                                y: value,
                            }),
                    );
                    modify_ranges.push(GjgjPoint {
                        x: ticks_buffer[0] as f64,
                        y: ticks_buffer[ticks_buffer.len() - 1] as f64,
                    });
                    ticks_buffer.clear();
                    value_buffer.clear();
                }
            }
            ori_prev_ticks = ori_ticks;
        }
        GjgjTone {
            modifies: pitch_points,
            modify_ranges,
        }
    }

    fn generate_volume(&self, volume_curve: &ParamCurve) -> Vec<GjgjVolumeMap> {
        let reduced = volume_curve.reduce_sample_rate(self.options.down_sample, 0);
        let mut volume_map: Vec<GjgjVolumeMap> = Vec::new();
        let mut ticks_buffer: Vec<i64> = Vec::new();
        let mut value_buffer: Vec<f64> = Vec::new();
        let mut prev_ticks = 0i64;
        for volume in reduced.points.iter().skip(1) {
            let tick = volume.x;
            if prev_ticks != tick {
                let ori_value = volume.y;
                let value = (ori_value as f64 + 1000.0) / 1000.0;
                if ori_value != 0 {
                    ticks_buffer.push(tick - self.first_bar_length);
                    value_buffer.push(value);
                } else if !ticks_buffer.is_empty() && !value_buffer.is_empty() {
                    volume_map.push(GjgjVolumeMap {
                        time: ticks_buffer[0] - 5,
                        volume: 1.0,
                    });
                    volume_map.extend(
                        ticks_buffer
                            .iter()
                            .zip(value_buffer.iter())
                            .map(|(&tick, &value)| GjgjVolumeMap { time: tick, volume: value }),
                    );
                    volume_map.push(GjgjVolumeMap {
                        time: ticks_buffer[ticks_buffer.len() - 1] + 5,
                        volume: 1.0,
                    });
                    ticks_buffer.clear();
                    value_buffer.clear();
                }
            }
            prev_ticks = tick;
        }
        volume_map
    }
}

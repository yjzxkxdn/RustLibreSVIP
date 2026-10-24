//! Port of `libresvip.deprecated.gj.gjgj_parser`.

use libresvip_core::core::compat::py_round;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::model::base::{
    InstrumentalTrack, Note, ParamCurve, Params, Phones, Project, SingingTrack, SongTempo,
    TimeSignature, Track,
};
use libresvip_core::model::point::Point;

use super::constants::{id_to_singer, DEFAULT_SINGER};
use super::model::{GjgjBeatItems, GjgjBeatStyle, GjgjSingingTrack, GjgjTone};

/// `InputOptions`.
#[derive(Clone, Debug)]
pub struct InputOptions {
    pub import_instrumental_track: bool,
    pub import_pitch: bool,
    pub import_volume: bool,
}

impl Default for InputOptions {
    fn default() -> Self {
        Self {
            import_instrumental_track: true,
            import_pitch: true,
            import_volume: true,
        }
    }
}

pub struct GjgjParser {
    options: InputOptions,
    time_synchronizer: TimeSynchronizer,
    first_bar_length: i64,
    ticks_in_beat: i64,
}

/// `tempo2bpm` (`libresvip.utils.binary.midi`).
fn tempo2bpm(microseconds_per_quarter_note: i64) -> f64 {
    60_000_000.0 / microseconds_per_quarter_note as f64
}

impl GjgjParser {
    pub fn new(options: InputOptions) -> Self {
        Self {
            options,
            time_synchronizer: TimeSynchronizer::from_project(&[]),
            first_bar_length: 0,
            ticks_in_beat: 480,
        }
    }

    pub fn parse_project(&mut self, gjgj_project: &super::model::GjgjProject) -> Result<Project, String> {
        self.ticks_in_beat = gjgj_project.tempo_map.ticks_per_quarter_note;
        let tempos = self.parse_tempos(&gjgj_project.tempo_map.tempos);
        let time_signatures =
            self.parse_time_signatures(gjgj_project.tempo_map.time_signature.clone());
        let mut track_list: Vec<Track> = self
            .parse_singing_tracks(&gjgj_project.tracks)
            .into_iter()
            .map(Track::Singing)
            .collect();
        if self.options.import_instrumental_track {
            track_list.extend(
                self.parse_instrumental_tracks(&gjgj_project.accompaniments)
                    .into_iter()
                    .map(Track::Instrumental),
            );
        }
        Ok(Project {
            version: String::new(),
            song_tempo_list: tempos,
            time_signature_list: time_signatures,
            track_list,
            warnings: Vec::new(),
        })
    }

    fn parse_tempos(&mut self, tempos: &[super::model::GjgjTempos]) -> Vec<SongTempo> {
        let song_tempos = tempos
            .iter()
            .map(|tempo| SongTempo::new(tempo.time, tempo2bpm(tempo.microseconds_per_quarter_note)))
            .collect::<Vec<_>>();
        self.time_synchronizer = TimeSynchronizer::from_project(&song_tempos);
        song_tempos
    }

    fn parse_time_signatures(
        &mut self,
        mut time_signatures: Vec<super::model::GjgjTimeSignature>,
    ) -> Vec<TimeSignature> {
        if time_signatures.is_empty() || time_signatures[0].time != 0 {
            time_signatures.insert(0, super::model::GjgjTimeSignature {
                time: 0,
                numerator: 4,
                denominator: 4,
            });
        }
        let mut changes = vec![TimeSignature::new(
            0,
            time_signatures[0].numerator,
            time_signatures[0].denominator,
        )];
        self.first_bar_length = changes[0].bar_length_with(self.ticks_in_beat) as i64;

        let mut prev_ticks = 0i64;
        let mut measure = 0.0f64;
        for time_signature in time_signatures.iter().skip(1) {
            let tick = time_signature.time;
            measure += (tick - prev_ticks) as f64
                / changes
                    .last()
                    .map(|last| last.bar_length_with(self.ticks_in_beat))
                    .unwrap_or(1920.0);
            changes.push(TimeSignature::new(
                measure.floor() as i64,
                time_signature.numerator,
                time_signature.denominator,
            ));
            prev_ticks = tick;
        }
        changes
    }

    fn parse_singing_tracks(&self, tracks: &[GjgjSingingTrack]) -> Vec<SingingTrack> {
        tracks
            .iter()
            .map(|track| SingingTrack {
                ai_singer_name: id_to_singer(&track.singer_info.display_name)
                    .unwrap_or(DEFAULT_SINGER)
                    .to_owned(),
                mute: track.master_volume.as_ref().and_then(|v| v.mute).unwrap_or(false),
                note_list: self.parse_notes(&track.beat_items),
                edited_params: self.parse_params(track),
                ..Default::default()
            })
            .collect()
    }

    fn parse_instrumental_tracks(
        &self,
        accompaniments: &[super::model::GjgjInstrumentalTrack],
    ) -> Vec<InstrumentalTrack> {
        accompaniments
            .iter()
            .map(|accompaniment| InstrumentalTrack {
                mute: accompaniment
                    .master_volume
                    .as_ref()
                    .and_then(|v| v.mute)
                    .unwrap_or(false),
                audio_file_path: accompaniment.path.clone().unwrap_or_default(),
                offset: py_round(
                    self.time_synchronizer
                        .get_actual_ticks_from_secs(accompaniment.offset as f64 / 10_000_000.0),
                ),
                ..Default::default()
            })
            .collect()
    }

    fn parse_notes(&self, beat_items: &[GjgjBeatItems]) -> Vec<Note> {
        beat_items
            .iter()
            .map(|beat_item| {
                let start_pos = beat_item.start_tick;
                let head_tag = match beat_item.style {
                    Some(GjgjBeatStyle::Sp) => Some("V".to_owned()),
                    Some(GjgjBeatStyle::Sil) => Some("0".to_owned()),
                    _ => None,
                };
                Note {
                    start_pos,
                    length: beat_item.duration,
                    lyric: beat_item.lyric.trim().to_owned(),
                    key_number: beat_item.track.unwrap_or(0),
                    head_tag,
                    pronunciation: beat_item.pinyin.clone(),
                    edited_phones: Some(self.parse_phones(beat_item, start_pos)),
                    ..Default::default()
                }
            })
            .collect()
    }

    /// `parse_phones` — the Python body sits inside `contextlib.suppress`, so
    /// every failure path leaves the default (-1, -1) phones behind.
    fn parse_phones(&self, beat_item: &GjgjBeatItems, start_pos: i64) -> Phones {
        let mut phones = Phones::default();
        if beat_item.pre_time != 0.0 {
            let difference = py_round(beat_item.pre_time * 480.0 * 3.0 / 2000.0);
            if difference > 0 {
                phones.head_length_in_secs = self
                    .time_synchronizer
                    .get_actual_secs_from_ticks(start_pos)
                    - self
                        .time_synchronizer
                        .get_actual_secs_from_ticks(difference);
            } else {
                phones.head_length_in_secs = -1.0;
            }
        }
        if beat_item.post_time != 0.0 {
            let essential_vowel_length =
                beat_item.duration as f64 * (2000.0 / 3.0) / 480.0 + beat_item.post_time;
            let tail_vowel_length = -beat_item.post_time;
            phones.mid_ratio_over_tail = essential_vowel_length / tail_vowel_length;
        } else {
            phones.mid_ratio_over_tail = -1.0;
        }
        phones
    }

    fn parse_params(&self, track: &GjgjSingingTrack) -> Params {
        let mut params = Params::default();
        if self.options.import_pitch {
            params.pitch = self.parse_pitch_curve(&track.tone);
        }
        if self.options.import_volume {
            params.volume = self.parse_volume_curve(&track.volume_map);
        }
        params
    }

    fn pitch_time_to_position(&self, pitch_time: f64) -> i64 {
        py_round(pitch_time * 5.0) + self.first_bar_length
    }

    fn parse_pitch_curve(&self, tone: &GjgjTone) -> ParamCurve {
        let mut points = vec![Point::start_point()];
        for mod_range in &tone.modify_ranges {
            let left_point = Point::new(self.pitch_time_to_position(mod_range.x), -100);
            let right_point = Point::new(self.pitch_time_to_position(mod_range.y), -100);
            // `find_index`: first modify point starting inside the range.
            let index = tone
                .modifies
                .iter()
                .position(|point| point.x >= mod_range.x && point.y <= mod_range.y);
            let Some(mut index) = index else {
                continue;
            };
            points.push(left_point);
            while index < tone.modifies.len() && tone.modifies[index].x <= mod_range.y {
                points.push(Point::new(
                    self.pitch_time_to_position(tone.modifies[index].x),
                    self.pitch_time_to_position(tone.modifies[index].y),
                ));
                index += 1;
            }
            points.push(right_point);
        }
        points.push(Point::end_point());
        ParamCurve { points }
    }

    fn parse_volume_curve(&self, volume_map: &[super::model::GjgjVolumeMap]) -> ParamCurve {
        ParamCurve {
            points: volume_map
                .iter()
                .map(|item| Point::new(py_round(item.time as f64), py_round(item.volume)))
                .collect(),
        }
    }
}


//! Port of `libresvip.plugins.vshp.vocalshifter_parser`.
//!
//! The Python plugin is a `ReadOnlyConverterMixin`: it only ever imports.

use std::path::Path;

use libresvip_core::core::constants::DEFAULT_CHINESE_LYRIC;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::Result;
use libresvip_core::model::base::{
    InstrumentalTrack, Note, ParamCurve, Params, Project, SingingTrack, SongTempo, TimeSignature,
    Track,
};
use libresvip_core::model::point::Point;

use super::model::{
    self, VocalShifterControlPoint, VocalShifterPatternData, VocalShifterPatternMetadata,
    VocalShifterProjectData, VocalShifterProjectMetadata, VocalShifterTrackMetadata,
    PATTERN_TYPE_WAVE,
};
use super::utils::ansi2unicode;

/// `InputOptions` of the plugin.
#[derive(Clone, Debug)]
pub struct InputOptions {
    pub wave_to_singing: bool,
    pub use_edited_pitch: bool,
    pub use_edited_dynamics: bool,
    pub import_dynamics: bool,
    pub import_formant: bool,
    pub import_breath: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParamAttribute {
    Dyn,
    OriDyn,
    Bre,
    Frm,
}

impl ParamAttribute {
    fn of(self, point: &VocalShifterControlPoint) -> f64 {
        match self {
            ParamAttribute::Dyn => point.dynamic,
            ParamAttribute::OriDyn => point.ori_dyn,
            ParamAttribute::Bre => f64::from(point.bre),
            ParamAttribute::Frm => f64::from(point.frm),
        }
    }
}

pub struct VocalShifterParser {
    options: InputOptions,
    synchronizer: TimeSynchronizer,
    tick_rate: f64,
}

impl VocalShifterParser {
    pub fn new(options: InputOptions) -> Self {
        Self {
            options,
            synchronizer: TimeSynchronizer::new(&[], 0, false, 60.0),
            tick_rate: 0.0,
        }
    }

    /// `VocalShifterParser.parse_project`.
    pub fn parse_project(&mut self, data: &VocalShifterProjectData) -> Result<Project> {
        let song_tempo_list = self.parse_tempo(&data.project_metadata);
        let time_signature_list = self.parse_time_signature(&data.project_metadata);
        let track_list = self.parse_track_list(data)?;
        Ok(Project {
            version: String::new(),
            song_tempo_list,
            time_signature_list,
            track_list,
        })
    }

    fn parse_tempo(&mut self, metadata: &VocalShifterProjectMetadata) -> Vec<SongTempo> {
        let tempo_list = vec![SongTempo {
            position: 0,
            bpm: metadata.tempo,
        }];
        self.synchronizer = TimeSynchronizer::new(&tempo_list, 0, false, 60.0);
        self.tick_rate = metadata.tempo / 25.0;
        tempo_list
    }

    fn parse_time_signature(
        &self,
        metadata: &VocalShifterProjectMetadata,
    ) -> Vec<TimeSignature> {
        vec![TimeSignature {
            bar_index: 0,
            numerator: i64::from(metadata.numerator),
            denominator: i64::from(metadata.denominator),
        }]
    }

    fn parse_track_list(&self, data: &VocalShifterProjectData) -> Result<Vec<Track>> {
        let mut track_list = Vec::new();
        for (pattern_metadata, pattern_data) in data
            .pattern_metadatas
            .iter()
            .zip(data.pattern_datas.iter())
        {
            let track_metadata = data
                .track_metadatas
                .get(pattern_metadata.track_num as usize)
                .ok_or_else(|| {
                    model::parse_error(format!(
                        "vshp: pattern refers to missing track {}",
                        pattern_metadata.track_num
                    ))
                })?;
            let track = if pattern_data.header.pattern_type == PATTERN_TYPE_WAVE
                && !self.options.wave_to_singing
            {
                self.parse_instrumental_track(pattern_metadata, pattern_data, track_metadata)?
            } else {
                self.parse_singing_track(pattern_metadata, pattern_data, track_metadata)?
            };
            track_list.push(track);
        }
        Ok(track_list)
    }

    fn parse_instrumental_track(
        &self,
        pattern_metadata: &VocalShifterPatternMetadata,
        pattern_data: &VocalShifterPatternData,
        track_metadata: &VocalShifterTrackMetadata,
    ) -> Result<Track> {
        let sample_rate = f64::from(pattern_data.header.sample_rate);
        let sample_offset = pattern_metadata.offset + f64::from(pattern_metadata.offset_add);
        let offset_in_seconds = sample_offset / sample_rate;
        let offset_in_ticks = self
            .synchronizer
            .get_actual_ticks_from_secs(offset_in_seconds);
        Ok(Track::Instrumental(InstrumentalTrack {
            audio_file_path: model::fixed_string(&pattern_metadata.path_and_ext),
            // `InstrumentalTrack.offset` is an `int` field; pydantic rejects a
            // fractional float, so the Python loader only accepts integral
            // offsets here.
            offset: int_exact(offset_in_ticks, "instrumental track offset")?,
            solo: track_metadata.solo != 0,
            mute: track_metadata.mute != 0,
            volume: track_metadata.volume,
            pan: track_metadata.pan,
            ..InstrumentalTrack::default()
        }))
    }

    fn parse_singing_track(
        &self,
        pattern_metadata: &VocalShifterPatternMetadata,
        pattern_data: &VocalShifterPatternData,
        track_metadata: &VocalShifterTrackMetadata,
    ) -> Result<Track> {
        let file_path = model::fixed_string(&pattern_metadata.path_and_ext);
        let title = Path::new(&file_path)
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        let sample_offset = pattern_metadata.offset + f64::from(pattern_metadata.offset_add);
        let offset_in_seconds = sample_offset / f64::from(pattern_data.header.sample_rate);
        let offset_in_ticks = truncate_to_i64(
            self.synchronizer
                .get_actual_ticks_from_secs(offset_in_seconds),
            "singing track offset",
        )?;
        let labels: &[model::VocalShifterLabel] = match &pattern_data.labels {
            Some(labels) => labels,
            None => &[],
        };
        let mut track = SingingTrack {
            title,
            solo: track_metadata.solo != 0,
            mute: track_metadata.mute != 0,
            volume: track_metadata.volume,
            pan: track_metadata.pan,
            note_list: self.parse_note_list(offset_in_ticks, &pattern_data.notes, labels)?,
            ..SingingTrack::default()
        };
        track.edited_params = self.parse_params(offset_in_seconds, pattern_data)?;
        Ok(Track::Singing(track))
    }

    fn parse_note_list(
        &self,
        offset: i64,
        notes: &[model::VocalShifterNote],
        labels: &[model::VocalShifterLabel],
    ) -> Result<Vec<Note>> {
        let mut note_list = Vec::new();
        if !labels.is_empty() {
            for label in labels {
                let target_note = notes.iter().find(|note| {
                    i64::from(note.start_tick) <= i64::from(label.start_tick)
                        && i64::from(label.start_tick)
                            <= i64::from(note.start_tick) + i64::from(note.length)
                });
                let start_pos = offset
                    + round_to_i64(
                        f64::from(label.start_tick) * self.tick_rate,
                        "label start tick",
                    )?;
                let length = truncate_to_i64(
                    (f64::from(label.end_tick) - f64::from(label.start_tick)) * self.tick_rate,
                    "label length",
                )?;
                let key_number = match target_note {
                    Some(note) => i64::from(note.pitch / 100),
                    None => 60,
                };
                note_list.push(Note {
                    start_pos,
                    length,
                    key_number,
                    lyric: ansi2unicode(crate::vshp::utils::until_nul(&label.name)),
                    ..Note::default()
                });
            }
        } else {
            for note in notes {
                note_list.push(Note {
                    start_pos: offset
                        + round_to_i64(
                            f64::from(note.start_tick) * self.tick_rate,
                            "note start tick",
                        )?,
                    length: truncate_to_i64(
                        f64::from(note.length) * self.tick_rate,
                        "note length",
                    )?,
                    key_number: i64::from(note.pitch / 100),
                    lyric: DEFAULT_CHINESE_LYRIC.to_owned(),
                    ..Note::default()
                });
            }
        }
        Ok(note_list)
    }

    /// `VocalShifterParser.volume_to_value`.
    pub fn volume_to_value(volume: f64) -> Result<f64> {
        if !(volume > 0.0) {
            // `math.log(0)` / `math.log(-1)` raise ValueError in Python.
            return Err(model::parse_error(
                "vshp: dynamics value must be positive to convert it",
            ));
        }
        let db = volume.ln() / (1.0 + 1.0 / 9.0f64).ln();
        let float_value = 10f64.powf(db / 20.0);
        Ok((1000.0 * float_value - 1000.0).round_ties_even())
    }

    fn parse_params(
        &self,
        offset: f64,
        pattern_data: &VocalShifterPatternData,
    ) -> Result<Params> {
        let mut params = Params {
            pitch: self.parse_pitch_curve(offset, pattern_data)?,
            ..Params::default()
        };
        if self.options.import_dynamics {
            let attribute = if self.options.use_edited_dynamics {
                ParamAttribute::Dyn
            } else {
                ParamAttribute::OriDyn
            };
            params.volume = self.parse_param_curve(
                offset,
                pattern_data,
                attribute,
                Self::volume_to_value,
            )?;
        }
        if self.options.import_breath {
            params.breath = self.parse_param_curve(
                offset,
                pattern_data,
                ParamAttribute::Bre,
                floor_divide_ten,
            )?;
        }
        if self.options.import_formant {
            params.gender = self.parse_param_curve(
                offset,
                pattern_data,
                ParamAttribute::Frm,
                floor_divide_ten,
            )?;
        }
        Ok(params)
    }

    fn parse_pitch_curve(
        &self,
        offset: f64,
        pattern_data: &VocalShifterPatternData,
    ) -> Result<ParamCurve> {
        let mut pitch_curve = ParamCurve::new(vec![Point::start_point()]);
        let mut has_pitch = false;
        let points_per_second = f64::from(pattern_data.header.points_per_second);
        if points_per_second == 0.0 {
            // Python: `1 / points_per_second` → ZeroDivisionError.
            return Err(model::parse_error(
                "vshp: points_per_second is zero",
            ));
        }
        let time_step = 1.0 / points_per_second;
        let mut offset = offset;
        for point in &pattern_data.points {
            let value = if self.options.use_edited_pitch {
                point.pit
            } else {
                point.ori_pit
            };
            let tick = round_to_i64(
                self.synchronizer.get_actual_ticks_from_secs(offset),
                "pitch curve tick",
            )? + 1920;
            if value != 0 {
                if !has_pitch {
                    pitch_curve.points.push(Point::new(tick, -100));
                    has_pitch = true;
                }
                pitch_curve.points.push(Point::new(tick, i64::from(value)));
            } else if has_pitch {
                pitch_curve.points.push(Point::new(tick, -100));
                has_pitch = false;
            }
            offset += time_step;
        }
        pitch_curve.points.push(Point::end_point());
        Ok(pitch_curve)
    }

    fn parse_param_curve(
        &self,
        offset: f64,
        pattern_data: &VocalShifterPatternData,
        attribute: ParamAttribute,
        mapping_func: fn(f64) -> Result<f64>,
    ) -> Result<ParamCurve> {
        let mut param_curve = ParamCurve::default();
        let points_per_second = f64::from(pattern_data.header.points_per_second);
        if points_per_second == 0.0 {
            return Err(model::parse_error(
                "vshp: points_per_second is zero",
            ));
        }
        let time_step = 1.0 / points_per_second;
        let mut offset = offset;
        for point in &pattern_data.points {
            let value = attribute.of(point);
            let mapped = mapping_func(value)?;
            param_curve.points.push(Point::new(
                round_to_i64(
                    self.synchronizer.get_actual_ticks_from_secs(offset),
                    "parameter curve tick",
                )? + 1920,
                truncate_to_i64(mapped, "parameter value")?,
            ));
            offset += time_step;
        }
        Ok(param_curve)
    }
}

/// `(10.0).__rfloordiv__` — Python's *reflected* floor division, i.e.
/// `mapping_func(value) == value // 10.0`.
fn floor_divide_ten(value: f64) -> Result<f64> {
    if !value.is_finite() {
        return Err(model::parse_error(
            "vshp: envelope value is not a finite number",
        ));
    }
    Ok((value / 10.0).floor())
}

/// Python `round()` — ties to even, rejects non-finite values.
fn round_to_i64(value: f64, what: &str) -> Result<i64> {
    if !value.is_finite() {
        return Err(model::parse_error(format!(
            "vshp: {what} is not a finite number"
        )));
    }
    Ok(value.round_ties_even() as i64)
}

/// Python `int()` — truncates towards zero, rejects non-finite values.
fn truncate_to_i64(value: f64, what: &str) -> Result<i64> {
    if !value.is_finite() {
        return Err(model::parse_error(format!(
            "vshp: {what} is not a finite number"
        )));
    }
    Ok(value as i64)
}

/// pydantic `int` validation: a float is only accepted when it is integral.
fn int_exact(value: f64, what: &str) -> Result<i64> {
    if !value.is_finite() || value.fract() != 0.0 {
        return Err(model::parse_error(format!(
            "vshp: {what} must be integral, found {value}"
        )));
    }
    Ok(value as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vshp::model::{
        VocalShifterLabel, VocalShifterNote, VocalShifterPatternHeader, VocalShifterProjectData,
        VocalShifterProjectMetadata, VocalShifterTime,
    };

    fn header(
        pattern_type: u32,
        points_per_second: u32,
        points_count: u32,
        notes_count: u32,
        label_count: u32,
    ) -> VocalShifterPatternHeader {
        VocalShifterPatternHeader {
            size: 0,
            sample_count: 0,
            sample_rate: 44100,
            channels: 1,
            pattern_type,
            points_per_second,
            points_count,
            time_count: 0,
            analyze_params: 0,
            analyze_prm1: 0,
            analyze_prm2: 0,
            analyze_prm3: 0,
            synth_mode: 0,
            attack_threshold: 0,
            fix_threshold: 0,
            fix_threshold_dyn: 0,
            synth_option: 0,
            fade_in_sample: 0,
            fade_out_sample: 0,
            spectrum_key_min: 0,
            spectrum_key_scale: 1,
            spectrum_points_density: 1,
            spectrum_points_per_group: 0,
            spectrum_points_count: 0,
            spectrum_unknown: 0,
            notes_count,
            label_count,
            eq1: [0; 16],
            eq2: [0; 16],
            heq: [0; 16],
        }
    }

    fn point(ori_pit: u16, pit: u16, ori_dyn: f64, dynamic: f64, bre: i16, frm: i16) -> VocalShifterControlPoint {
        VocalShifterControlPoint {
            pit_analyze: [0; 4],
            wave: [0; 4],
            fix_org: 0,
            fix_edit: 0,
            ori_pit,
            pit,
            frm,
            bre,
            brightness: 0,
            clearness: 0,
            ori_dyn,
            dynamic,
            vol: 1.0,
            pan: 0.0,
            sdyn: 0.0,
            dsdyn: 0.0,
            unknown: 0,
            heq: 0,
        }
    }

    fn project_data(
        pattern_type: u32,
        points_per_second: u32,
        points: Vec<VocalShifterControlPoint>,
        notes: Vec<VocalShifterNote>,
        labels: Option<Vec<VocalShifterLabel>>,
        tempo: f64,
        sample_rate: u32,
    ) -> VocalShifterProjectData {
        let points_count = points.len() as u32;
        let notes_count = notes.len() as u32;
        let label_count = labels.as_ref().map_or(0, |labels| labels.len()) as u32;
        VocalShifterProjectData {
            version: [0; 8],
            size: 0,
            project_metadata: VocalShifterProjectMetadata {
                size: 0,
                track_count: 1,
                pattern_count: 1,
                save_option: 0,
                vslib_version: 0,
                sample_rate: 44100,
                numerator: 3,
                denominator: 4,
                tempo,
                master_volume: 1.0,
                is_float: 0,
                bit_depth: 16,
                channels: 2,
            },
            track_metadatas: vec![VocalShifterTrackMetadata {
                size: 0,
                name: [0; 64],
                volume: 0.8,
                pan: -0.25,
                mute: 1,
                solo: 0,
                sel_flg: 0,
                color: 0,
                invert_flg: 0,
                morphing_group: 0,
                option: 0,
            }],
            pattern_metadatas: vec![VocalShifterPatternMetadata {
                size: 0,
                path_and_ext: {
                    let mut path = [0u8; 256];
                    path[..9].copy_from_slice(b"vocal.wav");
                    path
                },
                id: 0,
                sel_flg: 0,
                track_num: 0,
                offset_add: 0,
                offset: 0.0,
                meas_offset: 0.0,
                freq_a4: 440.0,
                key: 60,
                tune: 0,
                user_tune: [0; 12],
                base_freq_key: 69,
                base_freq_scale: 0,
                base_freq: 440.0,
                key_type: 0,
                option: 0,
            }],
            pattern_datas: vec![VocalShifterPatternData {
                size: 0,
                header: VocalShifterPatternHeader {
                    sample_rate,
                    ..header(pattern_type, points_per_second, points_count, notes_count, label_count)
                },
                points,
                start_time: VocalShifterTime {
                    original_time: 0.0,
                    time: 0.0,
                },
                end_time: VocalShifterTime {
                    original_time: 1.0,
                    time: 1.0,
                },
                spectrum: None,
                notes,
                labels,
            }],
        }
    }

    fn default_options() -> InputOptions {
        InputOptions {
            wave_to_singing: true,
            use_edited_pitch: true,
            use_edited_dynamics: true,
            import_dynamics: false,
            import_formant: false,
            import_breath: false,
        }
    }

    #[test]
    fn builds_a_singing_track_from_pattern_data() {
        let notes = vec![VocalShifterNote {
            start_tick: 0,
            length: 100,
            pitch: 6000,
            flag: 0,
        }];
        let mut points = vec![point(5900, 6000, 0.0, 0.0, 0, 0)];
        points.extend((0..4).map(|_| point(0, 0, 0.0, 0.0, 0, 0)));
        let data = project_data(1, 100, points, notes, None, 120.0, 44100);
        let project = VocalShifterParser::new(default_options())
            .parse_project(&data)
            .expect("project");

        assert_eq!(project.song_tempo_list.len(), 1);
        assert_eq!(project.song_tempo_list[0].bpm, 120.0);
        assert_eq!(project.time_signature_list.len(), 1);
        assert_eq!(project.time_signature_list[0].numerator, 3);
        assert_eq!(project.time_signature_list[0].denominator, 4);
        assert_eq!(project.track_list.len(), 1);

        let track = project.track_list[0].as_singing().expect("singing");
        // `pathlib.Path("vocal.wav").stem`
        assert_eq!(track.title, "vocal");
        assert!(track.mute);
        assert!(!track.solo);
        assert_eq!(track.volume, 0.8);
        assert_eq!(track.pan, -0.25);
        assert_eq!(track.note_list.len(), 1);
        assert_eq!(track.note_list[0].start_pos, 0);
        // int(100 * 120 / 25)
        assert_eq!(track.note_list[0].length, 480);
        assert_eq!(track.note_list[0].key_number, 60);
        assert_eq!(track.note_list[0].lyric, "啦");

        // (-192000, -100) start point, then the run of pitch, then the end point.
        assert_eq!(
            track.edited_params.pitch.points,
            vec![
                Point::new(-192000, -100),
                Point::new(1920, -100),
                Point::new(1920, 6000),
                // round(0.01 * 120 * 8) = round(9.6) = 10
                Point::new(1930, -100),
                Point::new(1073741823, -100),
            ]
        );
    }

    #[test]
    fn honours_use_edited_pitch() {
        let mut points = vec![point(5900, 6000, 0.0, 0.0, 0, 0)];
        points.push(point(0, 0, 0.0, 0.0, 0, 0));
        let data = project_data(1, 100, points, Vec::new(), None, 120.0, 44100);
        let mut options = default_options();
        options.use_edited_pitch = false;
        let project = VocalShifterParser::new(options)
            .parse_project(&data)
            .expect("project");
        let track = project.track_list[0].as_singing().expect("singing");
        assert_eq!(track.edited_params.pitch.points[2], Point::new(1920, 5900));
    }

    #[test]
    fn labels_become_notes() {
        let notes = vec![
            VocalShifterNote {
                start_tick: 0,
                length: 100,
                pitch: 6000,
                flag: 0,
            },
            VocalShifterNote {
                start_tick: 200,
                length: 100,
                pitch: 6200,
                flag: 0,
            },
        ];
        let mut labels = vec![
            VocalShifterLabel {
                start_tick: 50,
                end_tick: 150,
                flag: 0,
                name: {
                    let mut name = [0u8; 64];
                    name[..2].copy_from_slice(b"la");
                    name
                },
            },
            VocalShifterLabel {
                start_tick: 500,
                end_tick: 600,
                flag: 0,
                name: {
                    let mut name = [0u8; 64];
                    name[..2].copy_from_slice(b"li");
                    name
                },
            },
        ];
        labels[0].name[2] = 0;
        let data = project_data(1, 100, Vec::new(), notes, Some(labels), 120.0, 44100);
        let project = VocalShifterParser::new(default_options())
            .parse_project(&data)
            .expect("project");
        let track = project.track_list[0].as_singing().expect("singing");
        assert_eq!(track.note_list.len(), 2);
        assert_eq!(track.note_list[0].start_pos, 240);
        assert_eq!(track.note_list[0].length, 480);
        assert_eq!(track.note_list[0].key_number, 60);
        assert_eq!(track.note_list[0].lyric, "la");
        // The second label points at no note, so it falls back to key 60.
        assert_eq!(track.note_list[1].start_pos, 2400);
        assert_eq!(track.note_list[1].key_number, 60);
        assert_eq!(track.note_list[1].lyric, "li");
    }

    #[test]
    fn imports_dynamics_breath_and_formant() {
        let mut points: Vec<_> = (0..3)
            .map(|_| point(0, 0, 0.5, 0.5, 5, 3))
            .collect();
        points.push(point(0, 0, 1.0, 1.0, 7, 4));
        let data = project_data(1, 100, points, Vec::new(), None, 120.0, 44100);
        let mut options = default_options();
        options.import_dynamics = true;
        options.import_breath = true;
        options.import_formant = true;
        let project = VocalShifterParser::new(options)
            .parse_project(&data)
            .expect("project");
        let track = project.track_list[0].as_singing().expect("singing");
        // Python: round(1000 * db_to_float(log(0.5, 10/9)) - 1000) == -531.
        assert_eq!(track.edited_params.volume.points[0], Point::new(1920, -531));
        // `(10.0).__rfloordiv__(value)` == `value // 10.0`.
        assert_eq!(track.edited_params.breath.points[0], Point::new(1920, 0));
        assert_eq!(track.edited_params.gender.points[0], Point::new(1920, 0));
        assert_eq!(track.edited_params.volume.points.len(), 4);
        // round(0.01 * 960), round(0.02 * 960), round(0.03 * 960)
        assert_eq!(
            track
                .edited_params
                .breath
                .points
                .iter()
                .map(|point| point.x)
                .collect::<Vec<_>>(),
            vec![1920, 1930, 1939, 1949]
        );
        // No synthetic start/end points on non-pitch curves.
        assert_eq!(track.edited_params.breath.points.last().unwrap().y, 0);
        // A negative envelope value floors towards negative infinity.
        assert_eq!(super::floor_divide_ten(-5.0).unwrap(), -1.0);
    }

    #[test]
    fn wave_patterns_can_stay_instrumental() {
        let data = project_data(0, 100, Vec::new(), Vec::new(), None, 120.0, 44100);
        let mut options = default_options();
        options.wave_to_singing = false;
        let project = VocalShifterParser::new(options)
            .parse_project(&data)
            .expect("project");
        let track = project.track_list[0]
            .as_instrumental()
            .expect("instrumental");
        assert_eq!(track.audio_file_path, "vocal.wav");
        assert_eq!(track.offset, 0);
        assert!(track.mute);
        assert_eq!(track.volume, 0.8);
        assert_eq!(track.pan, -0.25);
        assert_eq!(track.title, "");
    }

    #[test]
    fn wave_patterns_can_become_singing() {
        let data = project_data(0, 100, Vec::new(), Vec::new(), None, 120.0, 44100);
        let project = VocalShifterParser::new(default_options())
            .parse_project(&data)
            .expect("project");
        assert!(project.track_list[0].is_singing());
    }

    #[test]
    fn rejects_a_fractional_instrumental_offset_like_pydantic() {
        let mut data = project_data(0, 100, Vec::new(), Vec::new(), None, 120.0, 44100);
        // `offset` is measured in samples: 100 samples at 44100 Hz corresponds
        // to a fractional number of ticks (100 / 44100 * 960).
        data.pattern_metadatas[0].offset = 100.0;
        let mut options = default_options();
        options.wave_to_singing = false;
        let mut parser = VocalShifterParser::new(options);
        assert!(parser.parse_project(&data).is_err());

        // 22050 samples = 0.5 s = 480 ticks, which pydantic accepts.
        data.pattern_metadatas[0].offset = 22050.0;
        let mut options = default_options();
        options.wave_to_singing = false;
        let project = VocalShifterParser::new(options)
            .parse_project(&data)
            .expect("project");
        assert_eq!(project.track_list[0].as_instrumental().unwrap().offset, 480);
    }

    #[test]
    fn volume_to_value_matches_python() {
        for (value, expected) in [
            (1.0, 0.0),
            (0.5, -531.0),
            (0.25, -780.0),
            (2.0, 1133.0),
            (0.1, -919.0),
            (3.0, 2322.0),
        ] {
            assert_eq!(
                VocalShifterParser::volume_to_value(value).unwrap(),
                expected,
                "volume {value}"
            );
        }
        assert!(VocalShifterParser::volume_to_value(0.0).is_err());
        assert!(VocalShifterParser::volume_to_value(-1.0).is_err());
    }

    #[test]
    fn zero_points_per_second_is_an_error() {
        let data = project_data(1, 0, Vec::new(), Vec::new(), None, 120.0, 44100);
        assert!(VocalShifterParser::new(default_options())
            .parse_project(&data)
            .is_err());
    }
}


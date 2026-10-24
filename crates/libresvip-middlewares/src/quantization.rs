//! Quantization middlewares (E8/E9 of the review backlog).
//!
//! * pitch quantization snaps curve values to the nearest semitone;
//! * rhythm quantization snaps note starts/lengths to a tick grid.
//!
//! Rust-port additions — opt-in only, default conversions untouched.

use libresvip_core::error::Result;
use libresvip_core::model::base::{Params, Project, Track};
use libresvip_core::options::{OptionFieldDef, Options};
use libresvip_core::plugin::{Middleware, PluginInfo};

pub static INFO_PITCH_QUANTIZATION: PluginInfo = PluginInfo {
    id: "pitch_quantization",
    name: "Pitch Quantization",
    author: "SoulMelody",
    website: "https://github.com/SoulMelody/LibreSVIP",
    file_format: "",
    description: "Snap pitch curve values to the nearest semitone (useful before exporting to step-based engines).",
    suffixes: &[],
    icon_base64: None,
};

pub static INFO_RHYTHM_QUANTIZATION: PluginInfo = PluginInfo {
    id: "rhythm_quantization",
    name: "Rhythm Quantization",
    author: "SoulMelody",
    website: "https://github.com/SoulMelody/LibreSVIP",
    file_format: "",
    description: "Snap note starts and lengths to a tick grid (default 1/32 note = 60 ticks).",
    suffixes: &[],
    icon_base64: None,
};

/// Curve value rounded to the nearest multiple of `step` (ties to even, like
/// Python's `round`).
fn snap(value: i64, step: f64) -> i64 {
    if step <= 0.0 {
        return value;
    }
    (libresvip_core::core::compat::py_round(value as f64 / step) as f64 * step) as i64
}

fn snap_i64(value: i64, grid: i64) -> i64 {
    if grid <= 1 {
        return value;
    }
    value.div_euclid(grid) * grid + {
        let remainder = value.rem_euclid(grid);
        if remainder * 2 >= grid {
            grid
        } else {
            0
        }
    }
}

pub struct PitchQuantizationMiddleware;

impl Middleware for PitchQuantizationMiddleware {
    fn info(&self) -> &'static PluginInfo {
        &INFO_PITCH_QUANTIZATION
    }

    fn process_options(&self) -> Vec<OptionFieldDef> {
        vec![OptionFieldDef::float("step_cents", "Step in cents", 100.0)
            .with_description(
                "100 snaps to semitones; 50 to quarter tones; 0 disables the \
                 middleware.",
            )]
    }

    fn process(&self, mut project: Project, options: &Options) -> Result<Project> {
        let step = options.float("step_cents", 100.0);
        if step <= 0.0 {
            return Ok(project);
        }
        for track in &mut project.track_list {
            if let Track::Singing(singing) = track {
                let Params {
                    pitch,
                    volume,
                    breath,
                    gender,
                    strength,
                } = &mut singing.edited_params;
                for curve in [pitch, volume, breath, gender, strength] {
                    for point in &mut curve.points {
                        point.y = snap(point.y, step);
                    }
                }
            }
        }
        Ok(project)
    }
}

pub struct RhythmQuantizationMiddleware;

impl Middleware for RhythmQuantizationMiddleware {
    fn info(&self) -> &'static PluginInfo {
        &INFO_RHYTHM_QUANTIZATION
    }

    fn process_options(&self) -> Vec<OptionFieldDef> {
        vec![
            OptionFieldDef::int("grid_ticks", "Grid in ticks", 60)
                .with_description("1920 ticks per quarter note: 60 = 1/32, 120 = 1/16."),
            OptionFieldDef::bool("quantize_lengths", "Also quantize lengths", true),
        ]
    }

    fn process(&self, mut project: Project, options: &Options) -> Result<Project> {
        let grid = options.int("grid_ticks", 60).max(1);
        let quantize_lengths = options.bool("quantize_lengths", true);
        if grid <= 1 {
            return Ok(project);
        }
        for track in &mut project.track_list {
            if let Track::Singing(singing) = track {
                for note in &mut singing.note_list {
                    let snapped_start = snap_i64(note.start_pos, grid);
                    if quantize_lengths {
                        let snapped_end =
                            snap_i64(note.start_pos + note.length, grid).max(snapped_start + grid);
                        note.length = snapped_end - snapped_start;
                    }
                    note.start_pos = snapped_start;
                }
            }
        }
        Ok(project)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libresvip_core::model::base::{Note, ParamCurve, SingingTrack};
    use libresvip_core::model::point::Point;
    use libresvip_core::options::OptionValue;

    fn options(pairs: &[(&str, OptionValue)]) -> Options {
        Options::from_pairs(
            pairs
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone())),
        )
    }

    #[test]
    fn pitch_snaps_to_semitones() {
        let mut curve = ParamCurve::default();
        curve.points.push(Point::new(0, 6049));
        curve.points.push(Point::new(10, -1050));
        let mut project = Project {
            track_list: vec![Track::Singing(SingingTrack {
                edited_params: Params {
                    pitch: curve,
                    ..Default::default()
                },
                ..Default::default()
            })],
            ..Default::default()
        };
        project = PitchQuantizationMiddleware
            .process(project, &options(&[]))
            .expect("process");
        let Track::Singing(singing) = &project.track_list[0] else {
            panic!("singing");
        };
        assert_eq!(singing.edited_params.pitch.points[0].y, 6000);
        // -10.5 rounds to -10 (ties to even).
        assert_eq!(singing.edited_params.pitch.points[1].y, -1000);
    }

    #[test]
    fn rhythm_snaps_starts_and_lengths() {
        let mut project = Project {
            track_list: vec![Track::Singing(SingingTrack {
                note_list: vec![Note {
                    start_pos: 67,
                    length: 130,
                    ..Default::default()
                }],
                ..Default::default()
            })],
            ..Default::default()
        };
        project = RhythmQuantizationMiddleware
            .process(project, &options(&[("grid_ticks", OptionValue::Int(60))]))
            .expect("process");
        let Track::Singing(singing) = &project.track_list[0] else {
            panic!("singing");
        };
        let note = &singing.note_list[0];
        assert_eq!(note.start_pos, 60);
        assert_eq!(note.start_pos + note.length, 180);
    }
}

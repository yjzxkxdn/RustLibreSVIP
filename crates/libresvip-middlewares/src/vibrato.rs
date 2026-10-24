//! Vibrato handling middleware (E7 of the review backlog).
//!
//! `strip` bakes every note's vibrato parameters into the edited pitch curve
//! (a sinusoid sampled every 5 ticks between the vibrato's start/end
//! percentages) and removes the parameters afterwards, so formats without a
//! vibrato layer still render the effect. Rust-port addition — opt-in only.

use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::Result;
use libresvip_core::model::base::{ParamCurve, Project, Track};
use libresvip_core::model::point::Point;
use libresvip_core::options::{EnumVariant, OptionFieldDef, Options};
use libresvip_core::plugin::{Middleware, PluginInfo};

pub static INFO_VIBRATO: PluginInfo = PluginInfo {
    id: "vibrato",
    name: "Vibrato",
    author: "SoulMelody",
    website: "https://github.com/SoulMelody/LibreSVIP",
    file_format: "",
    description: "Bake note vibrato parameters into the pitch curve so engines without a vibrato layer keep the effect.",
    suffixes: &[],
    icon_base64: None,
};

/// Sample spacing of the baked sinusoid, in ticks.
const SAMPLE_STEP_TICKS: i64 = 5;

/// Step-sampled value of a percent-indexed curve (x in 0..=100).
fn curve_value_at_percent(curve: &ParamCurve, percent: f64) -> f64 {
    let x = percent as i64;
    let index = curve
        .points
        .partition_point(|point| point.x <= x)
        .saturating_sub(1);
    curve
        .points
        .get(index)
        .map(|point| point.y)
        .unwrap_or(0) as f64
}

/// Generates the vibrato points for one note, absolute-cents over ticks.
pub fn vibrato_points(
    note: &libresvip_core::model::base::Note,
    synchronizer: &TimeSynchronizer,
    step_ticks: i64,
) -> Vec<Point> {
    let Some(vibrato) = &note.vibrato else {
        return Vec::new();
    };
    let start_percent = vibrato.start_percent.min(vibrato.end_percent);
    let end_percent = vibrato.start_percent.max(vibrato.end_percent);
    if end_percent <= start_percent || note.length <= 0 {
        return Vec::new();
    }
    let vibrato_start = note.start_pos + py_round(note.length as f64 * start_percent / 100.0);
    let vibrato_end = note.start_pos + py_round(note.length as f64 * end_percent / 100.0);
    if vibrato_end <= vibrato_start {
        return Vec::new();
    }
    let base_cents = note.key_number as f64 * 100.0;
    let phase = if vibrato.is_anti_phase { std::f64::consts::PI } else { 0.0 };
    let mut points = Vec::new();
    let mut tick = vibrato_start;
    while tick <= vibrato_end {
        let span = (vibrato_end - vibrato_start) as f64;
        let percent = start_percent + (tick - vibrato_start) as f64 / span
            * (end_percent - start_percent);
        let amplitude = curve_value_at_percent(&vibrato.amplitude, percent);
        let frequency = curve_value_at_percent(&vibrato.frequency, percent).max(0.0);
        let elapsed_secs = synchronizer.get_actual_secs_from_ticks(tick)
            - synchronizer.get_actual_secs_from_ticks(vibrato_start);
        let value = base_cents
            + amplitude * (2.0 * std::f64::consts::PI * frequency * elapsed_secs + phase).sin();
        points.push(Point::new(tick, py_round(value)));
        tick += step_ticks.max(1);
    }
    points
}

fn py_round(value: f64) -> i64 {
    libresvip_core::core::compat::py_round(value)
}

/// Merges baked points into a pitch curve: sort by x, and where x collides
/// the baked value replaces the existing one.
fn merge_points(curve: &mut ParamCurve, extra: Vec<Point>) {
    if extra.is_empty() {
        return;
    }
    curve.points.extend(extra);
    curve.points.sort_by_key(|point| point.x);
    curve.points.dedup_by_key(|point| point.x);
}

const MODE_VARIANTS: &[EnumVariant] = &[
    EnumVariant { name: "STRIP", value: "strip", title: "Bake vibrato into pitch, remove parameters" },
    EnumVariant { name: "KEEP", value: "keep", title: "Leave the project unchanged" },
];

pub struct VibratoMiddleware;

impl Middleware for VibratoMiddleware {
    fn info(&self) -> &'static PluginInfo {
        &INFO_VIBRATO
    }

    fn process_options(&self) -> Vec<OptionFieldDef> {
        vec![OptionFieldDef::enumerated(
                "mode",
                "Mode",
                "VibratoMode",
                MODE_VARIANTS,
                "STRIP",
            )
            .with_description(
                "Re-fitting vibrato parameters from a baked curve is not \
                 supported; use strip before exporting to engines without a \
                 vibrato layer.",
            )]
    }

    fn process(&self, mut project: Project, options: &Options) -> Result<Project> {
        if options.str("mode", "strip") != "strip" {
            return Ok(project);
        }
        let synchronizer = TimeSynchronizer::from_project(&project.song_tempo_list);
        for track in &mut project.track_list {
            if let Track::Singing(singing) = track {
                let mut baked: Vec<Point> = Vec::new();
                for note in &mut singing.note_list {
                    if let Some(points) = note
                        .vibrato
                        .as_ref()
                        .map(|_| vibrato_points(note, &synchronizer, SAMPLE_STEP_TICKS))
                    {
                        baked.extend(points);
                    }
                    note.vibrato = None;
                }
                merge_points(&mut singing.edited_params.pitch, baked);
            }
        }
        Ok(project)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libresvip_core::model::base::{Note, SingingTrack, SongTempo, VibratoParam};

    fn note_with_vibrato(anti_phase: bool) -> Note {
        let mut amplitude = ParamCurve::default();
        amplitude.points.push(Point::new(0, 50));
        let mut frequency = ParamCurve::default();
        frequency.points.push(Point::new(0, 5));
        Note {
            start_pos: 1920,
            length: 1920,
            key_number: 60,
            vibrato: Some(VibratoParam {
                start_percent: 25.0,
                end_percent: 100.0,
                is_anti_phase: anti_phase,
                amplitude,
                frequency,
            }),
            ..Default::default()
        }
    }

    fn tempo_project(note: Note) -> Project {
        Project {
            song_tempo_list: vec![SongTempo::new(0, 120.0)],
            track_list: vec![Track::Singing(SingingTrack {
                note_list: vec![note],
                ..Default::default()
            })],
            ..Default::default()
        }
    }

    #[test]
    fn baking_covers_the_vibrato_span_and_clears_params() {
        let project = tempo_project(note_with_vibrato(false));
        let options = Options::new();
        let processed = VibratoMiddleware
            .process(project, &options)
            .expect("process");
        let Track::Singing(singing) = &processed.track_list[0] else {
            panic!("singing track");
        };
        assert!(singing.note_list[0].vibrato.is_none(), "params stripped");
        let pitch = &singing.edited_params.pitch.points;
        assert!(!pitch.is_empty(), "points were baked");
        assert!(pitch.first().unwrap().x >= 1920 + 1920 / 4);
        assert!(pitch.last().unwrap().x <= 1920 + 1920);
        // Around the base pitch (6000 cents ± 50): extrema must appear.
        let max = pitch.iter().map(|p| p.y).max().unwrap();
        let min = pitch.iter().map(|p| p.y).min().unwrap();
        assert!(max > 6000, "upper excursion baked (max {max})");
        assert!(min < 6000, "lower excursion baked (min {min})");
    }

    #[test]
    fn anti_phase_starts_downward() {
        let synchronizer = TimeSynchronizer::from_project(&[SongTempo::new(0, 120.0)]);
        let note = note_with_vibrato(true);
        let points = vibrato_points(&note, &synchronizer, 5);
        assert!(
            points.iter().take(3).any(|p| p.y < 6000),
            "anti-phase dips first: {:?}",
            &points[..points.len().min(3)]
        );
    }
}

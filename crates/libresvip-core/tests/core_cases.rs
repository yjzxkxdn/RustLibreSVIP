//! Differential tests for the core helpers, driven by
//! `scripts/gen_core_cases.py` (Python reference).

use libresvip_core::core::tick_counter::{
    find_bar_index, shift_beat_list, shift_tempo_list, skip_beat_list, skip_tempo_list,
};
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::model::base::{ParamCurve, Project, SongTempo, TimeSignature};
use libresvip_core::model::point::Point;
use libresvip_core::model::reset_time_axis::{limit_bars, reset_time_axis, zoom_project};
use serde_json::Value;

fn workspace_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn cases() -> Value {
    let path = workspace_root().join("testdata").join("core_cases.json");
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "read {}: {error} (run scripts/gen_core_cases.py)",
            path.display()
        )
    });
    serde_json::from_slice(&bytes).expect("parse core_cases.json")
}

fn tempos(value: &Value) -> Vec<SongTempo> {
    serde_json::from_value(value.clone()).expect("tempos")
}

fn signatures(value: &Value) -> Vec<TimeSignature> {
    serde_json::from_value(value.clone()).expect("signatures")
}

fn points(value: &Value) -> Vec<Point> {
    serde_json::from_value(value.clone()).expect("points")
}

fn points_to_value(points: &[Point]) -> Value {
    serde_json::to_value(points).expect("serialize points")
}

fn approx(left: f64, right: f64) -> bool {
    if left == right {
        return true;
    }
    let scale = left.abs().max(right.abs()).max(1.0);
    (left - right).abs() <= 1e-9 * scale
}

#[test]
fn tempo_list_helpers() {
    let data = cases();
    for case in data["tempo"].as_array().unwrap() {
        let input = tempos(&case["tempos"]);
        let skip = case["skip"].as_i64().unwrap();
        let expected: Vec<SongTempo> = serde_json::from_value(case["expected"].clone()).unwrap();
        let got = skip_tempo_list(&input, skip);
        assert_eq!(
            serde_json::to_value(&got).unwrap(),
            serde_json::to_value(&expected).unwrap(),
            "skip_tempo_list({skip})"
        );
        let expected_shift: Vec<SongTempo> =
            serde_json::from_value(case["shifted"].clone()).unwrap();
        let got_shift = shift_tempo_list(&input, 480);
        assert_eq!(
            serde_json::to_value(&got_shift).unwrap(),
            serde_json::to_value(&expected_shift).unwrap(),
            "shift_tempo_list"
        );
    }
}

#[test]
fn beat_list_helpers() {
    let data = cases();
    for case in data["beat"].as_array().unwrap() {
        let input = signatures(&case["signatures"]);
        let skip = case["skip"].as_i64().unwrap();
        let expected: Vec<TimeSignature> =
            serde_json::from_value(case["expected"].clone()).unwrap();
        let got = skip_beat_list(&input, skip);
        assert_eq!(
            serde_json::to_value(&got).unwrap(),
            serde_json::to_value(&expected).unwrap(),
            "skip_beat_list({skip})"
        );
        let expected_shift: Vec<TimeSignature> =
            serde_json::from_value(case["shifted"].clone()).unwrap();
        let got_shift = shift_beat_list(&input, 2);
        assert_eq!(
            serde_json::to_value(&got_shift).unwrap(),
            serde_json::to_value(&expected_shift).unwrap(),
            "shift_beat_list"
        );
    }
}

#[test]
fn bar_index_lookup() {
    let data = cases();
    for case in data["bar"].as_array().unwrap() {
        let input = signatures(&case["signatures"]);
        let ticks = case["ticks"].as_i64().unwrap();
        let expected = case["expected"].as_i64().unwrap();
        assert_eq!(find_bar_index(&input, ticks).unwrap(), expected, "ticks={ticks}");
    }
}

#[test]
fn time_synchronizer() {
    let data = cases();
    for case in data["sync"].as_array().unwrap() {
        let input = tempos(&case["tempos"]);
        let skip = case["skip"].as_i64().unwrap();
        let absolute = case["absolute"].as_bool().unwrap();
        let default_tempo = case["default_tempo"].as_f64().unwrap();
        let synchronizer = TimeSynchronizer::new(&input, skip, absolute, default_tempo);
        let ticks: Vec<i64> = serde_json::from_value(case["ticks"].clone()).unwrap();
        let expected_secs: Vec<f64> = serde_json::from_value(case["secs"].clone()).unwrap();
        let expected_actual: Vec<f64> =
            serde_json::from_value(case["actual_ticks"].clone()).unwrap();
        for ((tick, expected_secs), expected_actual) in ticks
            .iter()
            .zip(expected_secs.iter())
            .zip(expected_actual.iter())
        {
            let secs = synchronizer.get_actual_secs_from_ticks(*tick);
            assert!(
                approx(secs, *expected_secs),
                "secs at {tick}: got {secs}, want {expected_secs}"
            );
            let actual = synchronizer.get_actual_ticks_from_ticks(*tick);
            assert!(
                approx(actual, *expected_actual),
                "actual ticks at {tick}: got {actual}, want {expected_actual}"
            );
        }
        let expected_offsets: Vec<f64> =
            serde_json::from_value(case["secs_offsets"].clone()).unwrap();
        let got_offsets = [
            synchronizer.get_actual_ticks_from_secs_offset(480, 0.5),
            synchronizer.get_actual_ticks_from_secs_offset(0, 2.0),
        ];
        for (got, expected) in got_offsets.iter().zip(expected_offsets.iter()) {
            assert!(approx(*got, *expected), "offsets: {got} vs {expected}");
        }
        let expected_batch: Vec<f64> = serde_json::from_value(case["batch"].clone()).unwrap();
        let batch = synchronizer.get_actual_secs_from_ticks_batch(&ticks);
        for (got, expected) in batch.iter().zip(expected_batch.iter()) {
            assert!(approx(*got, *expected), "batch: {got} vs {expected}");
        }
    }
}

#[test]
fn param_curve_sampling() {
    let data = cases();
    for case in data["curve"].as_array().unwrap() {
        let curve = ParamCurve::new(points(&case["points"]));
        let interval = case["interval"].as_i64().unwrap();
        let reduced = curve.reduce_sample_rate(interval, 0);
        assert_eq!(
            points_to_value(&reduced.points),
            case["reduced"],
            "reduce_sample_rate({interval})"
        );
        let segments = curve.split_into_segments(0);
        let expected: Vec<Vec<Point>> = serde_json::from_value(case["segments"].clone()).unwrap();
        assert_eq!(segments, expected, "split_into_segments");
    }
}

#[test]
fn project_transforms() {
    let data = cases();
    for case in data["transforms"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let project: Project =
            serde_json::from_value(data["projects"][name].clone()).expect("project");
        let compare = |got: Project, expected: &Value, label: &str| {
            let got_value = serde_json::to_value(&got).unwrap();
            assert_eq!(&got_value, expected, "{name}: {label}");
        };
        compare(zoom_project(&project, 2.0).unwrap(), &case["expected_zoom_2"], "zoom 2");
        compare(
            zoom_project(&project, 0.5).unwrap(),
            &case["expected_zoom_half"],
            "zoom 0.5",
        );
        compare(
            limit_bars(&project, 1).unwrap(),
            &case["expected_limit_1"],
            "limit 1",
        );
        compare(
            reset_time_axis(project.clone(), 100.0).unwrap(),
            &case["expected_reset"],
            "reset",
        );
    }
}

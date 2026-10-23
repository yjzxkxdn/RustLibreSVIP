//! Port of `libresvip.plugins.ust.pitch_mode1`.

use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::model::base::{Note, ParamCurve, TimeSignature};
use libresvip_core::model::point::Point;
use libresvip_core::Result;

use super::constants::MODE1_PITCH_SAMPLING_INTERVAL_TICK;
use super::math::round_int;
use super::pitch::{PitchSimulator, Portamento, RelativePitchCurve};
use super::resampling::dot_resampled;

/// `UtauMode1NotePitchData`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mode1NotePitchData {
    pub pitch_points: Vec<i64>,
}

/// `UtauMode1TrackPitchData`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mode1TrackPitchData {
    pub notes: Vec<Option<Mode1NotePitchData>>,
}

/// `pitch_from_utau_mode1_track`.
pub fn pitch_from_utau_mode1_track(
    pitch_data: &Mode1TrackPitchData,
    synchronizer: &TimeSynchronizer,
    notes: &[Note],
    time_signatures: &[TimeSignature],
) -> Result<ParamCurve> {
    let mut pitch_points: Vec<Point> = Vec::new();
    for (note, note_pitch) in notes.iter().zip(pitch_data.notes.iter()) {
        if let Some(note_pitch) = note_pitch {
            for (index, value) in note_pitch.pitch_points.iter().enumerate() {
                pitch_points.push(Point::new(
                    note.start_pos + index as i64 * MODE1_PITCH_SAMPLING_INTERVAL_TICK,
                    *value,
                ));
            }
        }
    }
    let pitch_simulator = PitchSimulator::new(
        synchronizer,
        Portamento::no_portamento(),
        notes,
        time_signatures,
    )?;
    let curve = RelativePitchCurve {
        lower_bound: notes.first().map(|note| note.start_pos).unwrap_or(0) as f64,
        upper_bound: notes.last().map(|note| note.end_pos()).unwrap_or(0) as f64,
        ..Default::default()
    };
    Ok(curve.to_absolute(&pitch_points, synchronizer, &pitch_simulator))
}

/// `pitch_to_utau_mode1_track`.
pub fn pitch_to_utau_mode1_track(pitch: &ParamCurve, notes: &[Note]) -> Mode1TrackPitchData {
    let mut note_pitch_data: Vec<Option<Mode1NotePitchData>> = Vec::new();
    for note in notes {
        let data: Vec<Point> = pitch
            .points
            .iter()
            .filter(|point| {
                note.start_pos <= point.x - 1920 && point.x - 1920 < note.end_pos()
                    && point.y != -100
            })
            .copied()
            .collect();
        if data.is_empty() {
            note_pitch_data.push(Some(Mode1NotePitchData::default()));
            continue;
        }
        let resampled_data = dot_resampled(&data, MODE1_PITCH_SAMPLING_INTERVAL_TICK);
        let pitch_points: Vec<i64> = resampled_data
            .iter()
            .map(|point| round_int(point.y as f64) - note.key_number * 100)
            .collect();
        note_pitch_data.push(Some(Mode1NotePitchData { pitch_points }));
    }
    Mode1TrackPitchData {
        notes: note_pitch_data,
    }
}

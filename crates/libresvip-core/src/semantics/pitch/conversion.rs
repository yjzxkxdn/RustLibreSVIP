//! Pitch-curve representation conversions: step (sample-and-hold) ↔ linear,
//! absolute ↔ note-relative, and note-border padding.
//!
//! The hub `ParamCurve` has no explicit contract saying whether its points
//! are "values at changes" (steps) or "samples to interpolate through"
//! (linear); each engine decides. These helpers make the two renderings
//! explicit so adapters stop hand-rolling them (design following
//! UtaFormatix3's `PitchCalculation`, implemented from scratch).

use crate::model::base::Note;
use crate::model::point::Point;

/// Raised when note boundaries overlap, which makes per-note conversions
/// ambiguous (UtaFormatix3 throws the same class of error).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NotesOverlappedError {
    /// Index of the first note whose range intrudes into its predecessor.
    pub index: usize,
}

impl std::fmt::Display for NotesOverlappedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "notes overlapped at index {}", self.index)
    }
}

impl std::error::Error for NotesOverlappedError {}

/// Step (sample-and-hold) points → points safe for a linear-interpolation
/// engine: before every value change a point with the *previous* value is
/// inserted one tick earlier, so the linear renderer draws a near-vertical
/// step instead of a ramp.
///
/// Points must be sorted by x. The first/last points are always kept.
pub fn sample_and_hold_to_linear(points: &[Point]) -> Vec<Point> {
    let mut result: Vec<Point> = Vec::with_capacity(points.len() * 2);
    for &point in points {
        if let Some(&last) = result.last() {
            if last.y != point.y && point.x > last.x + 1 {
                result.push(Point::new(point.x - 1, last.y));
            } else if last.y != point.y && point.x == last.x + 1 {
                // No room for a lead-in point; the one-tick ramp is the best a
                // linear engine can express.
            }
        }
        match result.last() {
            Some(last) if last.x == point.x => result.last_mut().unwrap().y = point.y,
            _ => result.push(point),
        }
    }
    result
}

/// Returns `(start, end)` tick ranges of the notes, verifying they neither
/// overlap nor interleave; `None` when any two do.
fn note_windows(notes: &[Note]) -> Result<Vec<(i64, i64, i64)>, NotesOverlappedError> {
    let mut windows: Vec<(i64, i64, i64)> = notes
        .iter()
        .map(|note| (note.start_pos, note.start_pos + note.length, note.key_number))
        .collect();
    windows.sort_by_key(|(start, _, _)| *start);
    for pair in windows.windows(2) {
        if pair[1].0 < pair[0].1 {
            // Recover the original index for the error message.
            let index = notes
                .iter()
                .position(|note| note.start_pos == pair[1].0)
                .unwrap_or(0);
            return Err(NotesOverlappedError { index });
        }
    }
    Ok(windows)
}

/// Absolute cents → offset from each covering note's key (in cents).
///
/// Points not covered by any note pass through unchanged (there is no base to
/// subtract). Fails with [`NotesOverlappedError`] when notes overlap.
pub fn relative_from_absolute(curve: &[Point], notes: &[Note]) -> Result<Vec<Point>, NotesOverlappedError> {
    let windows = note_windows(notes)?;
    Ok(curve
        .iter()
        .map(|&point| {
            let key = windows
                .iter()
                .find(|window| window.0 <= point.x && point.x < window.1)
                .map(|&(_, _, key)| key);
            match key {
                Some(key) => Point::new(point.x, point.y - key * 100),
                None => point,
            }
        })
        .collect())
}

/// Offset from each covering note's key (in cents) → absolute cents.
pub fn absolute_from_relative(curve: &[Point], notes: &[Note]) -> Result<Vec<Point>, NotesOverlappedError> {
    let windows = note_windows(notes)?;
    Ok(curve
        .iter()
        .map(|&point| {
            let key = windows
                .iter()
                .find(|window| window.0 <= point.x && point.x < window.1)
                .map(|&(_, _, key)| key);
            match key {
                Some(key) => Point::new(point.x, point.y + key * 100),
                None => point,
            }
        })
        .collect())
}

/// Step-sampled value of `curve` at `x` (last point at or before `x`).
fn step_value_at(curve: &[Point], x: i64) -> Option<i64> {
    let index = curve.partition_point(|point| point.x <= x).checked_sub(1)?;
    Some(curve[index].y)
}

/// Pads the curve with a point `radius` ticks before every note start and
/// `radius` ticks after every note end (VOCALOID-style borders use radius 5).
///
/// The inserted value is the step value the curve already has at that
/// position, so the rendered shape is unchanged — only its anchor density
/// around boundaries grows, which is what some editors require to render
/// curve segments continuously.
pub fn append_points_at_note_borders(curve: &[Point], notes: &[Note], radius: i64) -> Vec<Point> {
    if curve.is_empty() || radius <= 0 {
        return curve.to_vec();
    }
    let mut extra: Vec<Point> = Vec::new();
    for note in notes {
        let start = note.start_pos - radius;
        let end = note.start_pos + note.length + radius;
        for position in [start, end] {
            if let Some(value) = step_value_at(curve, position) {
                extra.push(Point::new(position, value));
            }
        }
    }
    let mut merged = curve.to_vec();
    merged.extend(extra);
    merged.sort_by_key(|point| point.x);
    // Where the original and the padded points collide, the later-written
    // original point must win: it is the authoritative sample.
    merged.dedup_by_key(|point| point.x);
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notes(list: &[(i64, i64, i64)]) -> Vec<Note> {
        list.iter()
            .map(|&(start, length, key)| Note {
                start_pos: start,
                length,
                key_number: key,
                ..Default::default()
            })
            .collect()
    }

    #[test]
    fn steps_become_linear_friendly() {
        let steps = [
            Point::new(0, 100),
            Point::new(10, 200),
            Point::new(20, 100),
        ];
        let linear = sample_and_hold_to_linear(&steps);
        assert_eq!(
            linear,
            vec![
                Point::new(0, 100),
                Point::new(9, 100),
                Point::new(10, 200),
                Point::new(19, 200),
                Point::new(20, 100),
            ]
        );
    }

    #[test]
    fn adjacent_change_keeps_point_count() {
        let steps = [Point::new(0, 0), Point::new(1, 500)];
        assert_eq!(sample_and_hold_to_linear(&steps), steps.to_vec());
    }

    #[test]
    fn duplicate_x_last_value_wins() {
        let steps = [Point::new(0, 0), Point::new(5, 100), Point::new(5, 300)];
        let linear = sample_and_hold_to_linear(&steps);
        assert_eq!(linear.last().copied(), Some(Point::new(5, 300)));
    }

    #[test]
    fn absolute_relative_round_trip() {
        let note_list = notes(&[(0, 480, 60), (480, 480, 62)]);
        let absolute = vec![Point::new(100, 6000), Point::new(600, 6300)];
        let relative = relative_from_absolute(&absolute, &note_list).expect("convert");
        assert_eq!(relative, vec![Point::new(100, 0), Point::new(600, 100)]);
        let back = absolute_from_relative(&relative, &note_list).expect("convert");
        assert_eq!(back, absolute);
    }

    #[test]
    fn points_outside_notes_pass_through() {
        let note_list = notes(&[(480, 480, 60)]);
        let curve = [Point::new(0, 123)];
        assert_eq!(
            relative_from_absolute(&curve, &note_list).expect("convert"),
            curve.to_vec()
        );
    }

    #[test]
    fn overlapping_notes_are_rejected() {
        let note_list = notes(&[(0, 480, 60), (240, 480, 62)]);
        let err = relative_from_absolute(&[Point::new(100, 0)], &note_list)
            .expect_err("overlap must fail");
        assert_eq!(err, NotesOverlappedError { index: 1 });
    }

    #[test]
    fn border_points_pad_without_reshaping() {
        let note_list = notes(&[(480, 480, 60)]);
        let curve = [Point::new(0, 0), Point::new(480, 100), Point::new(960, 0)];
        let padded = append_points_at_note_borders(&curve, &note_list, 5);
        assert_eq!(
            padded,
            vec![
                Point::new(0, 0),
                Point::new(475, 0),
                Point::new(480, 100),
                Point::new(960, 0),
                Point::new(965, 0),
            ]
        );
    }
}

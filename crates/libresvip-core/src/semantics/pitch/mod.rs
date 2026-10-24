//! Pitch-curve semantics: interpolation, resampling, simplification and the
//! absolute/relative conversions shared by every format adapter.

pub mod conversion;
pub mod interpolation;
pub mod rdp;
pub mod resampling;

pub use conversion::{
    append_points_at_note_borders, relative_from_absolute, sample_and_hold_to_linear,
    NotesOverlappedError,
};

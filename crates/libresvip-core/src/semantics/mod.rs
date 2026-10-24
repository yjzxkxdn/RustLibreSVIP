//! Format-agnostic semantics layer.
//!
//! Cross-format rules — how pitch curves are sampled, interpolated and
//! converted between absolute and note-relative forms; how lyrics and phonemes
//! map across engines — live here instead of inside individual plugins
//! (plugins only parse bytes into the hub model and write it back).
//!
//! Hard rules for code in this module (they keep the future OpenSVIP 2 pivot
//! cheap, see `docs/` upstream-report `utaformatix3-comparison.md` §7.2):
//!
//! * signatures take narrow types (`&[Point]`, `&[Note]` views, `&str`) —
//!   never `&Project` / `&mut SingingTrack`;
//! * time is project-absolute ticks;
//! * no new point structs: curves are `ParamCurve`/`Point` or SoA slices;
//! * dropping or approximating data must produce a
//!   [`crate::core::warning_types::ConversionWarning`], not a bare eprintln.

pub mod lyrics;
pub mod pitch;

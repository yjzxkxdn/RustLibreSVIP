//! Port of `libresvip.core.warning_types`, extended with a structured
//! warning channel.
//!
//! `show_warning` remains the single emission API and keeps its exact string
//! behaviour (stderr outside a [`CatchWarnings`] scope, buffered inside one).
//! On top of that, every warning is mirrored into a thread-local typed buffer
//! as a [`ConversionWarning`], so drivers can attach warnings to
//! `Project::warnings` and UIs can filter, deduplicate and localise them —
//! the Rust counterpart of UtaFormatix3's `ImportWarning`.

use std::cell::RefCell;
use std::fmt;

/// What kind of information a warning is about.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum WarningKind {
    /// One or more tempo events were dropped, ignored or missing.
    TempoDropped,
    /// One or more time signatures were dropped, ignored or missing.
    TimeSignatureDropped,
    /// Pitch data was dropped or not representable.
    PitchDropped,
    /// A parameter curve (volume/breath/gender/strength) was dropped.
    ParamDropped,
    /// Pronunciation/phoneme information was dropped or replaced.
    PhonemeDropped,
    /// Vibrato information was dropped or ignored.
    VibratoDropped,
    /// Content was truncated to satisfy a format limit.
    LengthTruncated,
    /// A note was skipped entirely.
    NoteSkipped,
    /// Data was read from the file but the hub model cannot express it.
    FieldDropped,
    /// The file declares a newer serialization version than this build knows.
    IncompatibleVersion,
    /// A fallback value was used instead of the original data.
    FallbackApplied,
    /// Anything else.
    Other,
}

impl WarningKind {
    /// Short stable label, used by `Display`.
    pub const fn label(self) -> &'static str {
        match self {
            WarningKind::TempoDropped => "tempo dropped",
            WarningKind::TimeSignatureDropped => "time signature dropped",
            WarningKind::PitchDropped => "pitch dropped",
            WarningKind::ParamDropped => "parameter dropped",
            WarningKind::PhonemeDropped => "phoneme dropped",
            WarningKind::VibratoDropped => "vibrato dropped",
            WarningKind::LengthTruncated => "length truncated",
            WarningKind::NoteSkipped => "note skipped",
            WarningKind::FieldDropped => "field dropped",
            WarningKind::IncompatibleVersion => "incompatible version",
            WarningKind::FallbackApplied => "fallback applied",
            WarningKind::Other => "warning",
        }
    }
}

impl fmt::Display for WarningKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// One structured conversion warning: a machine-readable kind plus the same
/// human-readable detail string `show_warning` would have printed.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ConversionWarning {
    pub kind: WarningKind,
    pub detail: String,
}

impl ConversionWarning {
    pub fn new(kind: WarningKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for ConversionWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.detail)
    }
}

thread_local! {
    /// Always-on mirror of every warning raised on this thread.
    static TYPED_BUFFER: RefCell<Vec<ConversionWarning>> = const { RefCell::new(Vec::new()) };
    /// String collector installed by a `CatchWarnings` scope, if any.
    static ACTIVE_COLLECTOR: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

/// `show_warning`: emits a warning for the current conversion.
///
/// Warnings go to stderr; inside a [`CatchWarnings`] scope they are buffered so
/// callers can present them in one go (the GUI does this). The warning is also
/// recorded as a typed [`ConversionWarning`] with kind [`WarningKind::Other`].
pub fn show_warning(message: impl Into<String>) {
    record(ConversionWarning::new(WarningKind::Other, message));
}

/// `show_warning` with a machine-readable kind. The printed/buffered string is
/// exactly the message passed here.
pub fn show_warning_kind(kind: WarningKind, message: impl Into<String>) {
    record(ConversionWarning::new(kind, message));
}

fn record(warning: ConversionWarning) {
    let message = warning.detail.clone();
    TYPED_BUFFER.with(|buffer| buffer.borrow_mut().push(warning));
    let captured = ACTIVE_COLLECTOR.with(|collector| {
        let mut collector = collector.borrow_mut();
        match collector.as_mut() {
            Some(buffer) => {
                buffer.push(message.clone());
                true
            }
            None => false,
        }
    });
    if !captured {
        // The CLI installs a loguru sink with format="{message}".
        eprintln!("{message}");
    }
}

/// Drains the structured warnings raised on this thread so far. Independent
/// of [`CatchWarnings`]; drivers typically call this after loading a project
/// and again after writing it.
pub fn take_typed_warnings() -> Vec<ConversionWarning> {
    TYPED_BUFFER.with(|buffer| {
        let mut buffer = buffer.borrow_mut();
        std::mem::take(&mut *buffer)
    })
}

/// Port of `CatchWarnings` — collects warning *strings* raised in the scope.
pub struct CatchWarnings {
    previous: Option<Option<Vec<String>>>,
    output: Vec<String>,
}

impl CatchWarnings {
    pub fn start() -> Self {
        let previous = ACTIVE_COLLECTOR
            .with(|collector| collector.borrow_mut().replace(Vec::new()));
        Self {
            previous: Some(previous),
            output: Vec::new(),
        }
    }

    pub fn finish(mut self) -> Vec<String> {
        self.output = self
            .restore_previous()
            .unwrap_or_default();
        self.output.clone()
    }

    /// Like [`CatchWarnings::finish`], additionally draining the structured
    /// warnings raised in the scope.
    pub fn finish_with_typed(mut self) -> (Vec<String>, Vec<ConversionWarning>) {
        self.output = self.restore_previous().unwrap_or_default();
        (self.output.clone(), take_typed_warnings())
    }

    /// `CatchWarnings.output` — newline joined messages.
    pub fn output(&self) -> String {
        self.output.join("\n")
    }

    /// Puts the previous collector back and returns the strings this scope
    /// collected. Returns `None` if the scope was already finished.
    fn restore_previous(&mut self) -> Option<Vec<String>> {
        let previous = self.previous.take()?;
        Some(ACTIVE_COLLECTOR.with(|collector| {
            let mut cell = collector.borrow_mut();
            let collected = cell.take().unwrap_or_default();
            *cell = previous;
            collected
        }))
    }
}

impl Drop for CatchWarnings {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            ACTIVE_COLLECTOR.with(|collector| {
                *collector.borrow_mut() = previous;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_and_string_channels_stay_in_sync() {
        let collector = CatchWarnings::start();
        show_warning("plain");
        show_warning_kind(WarningKind::NoteSkipped, "skipped note A4");
        let (messages, typed) = collector.finish_with_typed();
        assert_eq!(
            messages,
            vec!["plain".to_owned(), "skipped note A4".to_owned()]
        );
        assert_eq!(typed.len(), 2);
        assert_eq!(typed[0].kind, WarningKind::Other);
        assert_eq!(typed[1].kind, WarningKind::NoteSkipped);
        assert_eq!(typed[1].detail, "skipped note A4");
    }

    #[test]
    fn typed_warnings_record_without_a_collector() {
        show_warning_kind(WarningKind::TempoDropped, "ignored tempo");
        let typed = take_typed_warnings();
        assert_eq!(typed, vec![ConversionWarning::new(WarningKind::TempoDropped, "ignored tempo")]);
        assert!(take_typed_warnings().is_empty(), "drained");
    }

    #[test]
    fn nested_scopes_restore_the_previous_collector() {
        let outer = CatchWarnings::start();
        show_warning("outer");
        {
            let inner = CatchWarnings::start();
            show_warning("inner");
            assert_eq!(inner.finish(), vec!["inner".to_owned()]);
        }
        show_warning("outer2");
        assert_eq!(
            outer.finish(),
            vec!["outer".to_owned(), "outer2".to_owned()]
        );
    }

    #[test]
    fn typed_drain_does_not_touch_string_capture() {
        let collector = CatchWarnings::start();
        show_warning_kind(WarningKind::NoteSkipped, "skipped");
        let drained = take_typed_warnings();
        assert_eq!(drained.len(), 1);
        assert_eq!(collector.finish(), vec!["skipped".to_owned()]);
    }
}

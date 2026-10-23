//! Port of `libresvip.core.warning_types`.

use std::cell::RefCell;

thread_local! {
    static ACTIVE_COLLECTOR: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

/// `show_warning`: emits a warning for the current conversion.
///
/// Warnings go to stderr; inside a [`CatchWarnings`] scope they are buffered so
/// callers can present them in one go (the GUI does this).
pub fn show_warning(message: impl Into<String>) {
    let message = message.into();
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

/// Port of `CatchWarnings` — collects warnings raised in the scope.
pub struct CatchWarnings {
    previous: Option<Vec<String>>,
    output: Vec<String>,
}

impl CatchWarnings {
    pub fn start() -> Self {
        let previous = ACTIVE_COLLECTOR.with(|collector| collector.borrow_mut().replace(Vec::new()));
        Self {
            previous,
            output: Vec::new(),
        }
    }

    pub fn finish(mut self) -> Vec<String> {
        self.output = ACTIVE_COLLECTOR
            .with(|collector| collector.borrow_mut().replace(self.previous.take().unwrap_or_default()))
            .unwrap_or_default();
        self.output.clone()
    }

    /// `CatchWarnings.output` — newline joined messages.
    pub fn output(&self) -> String {
        self.output.join("\n")
    }
}

impl Drop for CatchWarnings {
    fn drop(&mut self) {
        ACTIVE_COLLECTOR.with(|collector| {
            *collector.borrow_mut() = self.previous.take();
        });
    }
}

//! The system folder picker, told apart from a failed one.
//!
//! `rfd` answers `None` both when the user cancels and when no picker
//! answers at all: on Linux it tries the XDG portal, then zenity, and
//! only logs when both fail. A logger that watches for that record
//! tells the two apart, so a missing picker falls back to the in-app
//! folder browser as the reference does.

use std::sync::atomic::{AtomicBool, Ordering};

/// The module `rfd` logs its last fallback's failure from.
const FALLBACK_TARGET: &str = "rfd::backend::xdg_desktop_portal";

/// What the picker answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Picked {
    /// A folder, as its path.
    Folder(String),
    /// The user closed the picker without choosing.
    Cancelled,
    /// No platform picker answered.
    Failed,
}

static FAILED: AtomicBool = AtomicBool::new(false);

/// Prints errors, and notes the one `rfd` logs when no picker answers.
struct Watch;

impl log::Log for Watch {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Error
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        if record.target() == FALLBACK_TARGET {
            FAILED.store(true, Ordering::Relaxed);
        }
        eprintln!("pure-desktop: {}: {}", record.target(), record.args());
    }

    fn flush(&self) {}
}

static WATCH: Watch = Watch;

/// Opens the platform's folder picker at `start` and blocks until it
/// answers. One picker runs at a time.
pub(crate) fn pick_folder(start: Option<String>) -> Picked {
    if log::set_logger(&WATCH).is_ok() {
        log::set_max_level(log::LevelFilter::Error);
    }
    FAILED.store(false, Ordering::Relaxed);
    let mut dialog = rfd::FileDialog::new().set_title("Open folder");
    if let Some(start) = start {
        dialog = dialog.set_directory(start);
    }
    match dialog.pick_folder() {
        Some(path) => Picked::Folder(path.to_string_lossy().into_owned()),
        None if FAILED.load(Ordering::Relaxed) => Picked::Failed,
        None => Picked::Cancelled,
    }
}

#[cfg(test)]
mod tests {
    use super::{FAILED, FALLBACK_TARGET, WATCH};
    use log::Log;
    use std::sync::atomic::Ordering;

    fn record(target: &str, level: log::Level) {
        WATCH.log(
            &log::Record::builder()
                .target(target)
                .level(level)
                .args(format_args!("x"))
                .build(),
        );
    }

    #[test]
    fn only_the_last_fallback_failing_marks_the_picker_failed() {
        FAILED.store(false, Ordering::Relaxed);
        record(
            "rfd::backend::xdg_desktop_portal::portal",
            log::Level::Error,
        );
        record(FALLBACK_TARGET, log::Level::Warn);
        assert!(!FAILED.load(Ordering::Relaxed));
        record(FALLBACK_TARGET, log::Level::Error);
        assert!(FAILED.load(Ordering::Relaxed));
    }
}

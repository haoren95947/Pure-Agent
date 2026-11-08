//! The native binary of the pure client. The application lives in the
//! library; the browser build has no binary, its entry is the wasm
//! glue in `web.rs`.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    // Only the shipped binary follows the operating system's language;
    // tests and tools resolve System to English so assertions stay stable.
    pure_desktop::i18n::detect_os_language(true);
    pure_desktop::launch::run();
}

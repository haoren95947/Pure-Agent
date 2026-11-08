//! The Simplified Chinese table, one module per area of the interface.

use std::collections::HashMap;
use std::sync::OnceLock;

mod app;
mod chrome;
mod forms;
mod main_ui;
mod settings;
mod settings_config;
mod transcript;
mod workbench;

static TABLE: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();

fn tables() -> [&'static [(&'static str, &'static str)]; 8] {
    [
        app::ENTRIES,
        chrome::ENTRIES,
        forms::ENTRIES,
        main_ui::ENTRIES,
        settings::ENTRIES,
        settings_config::ENTRIES,
        transcript::ENTRIES,
        workbench::ENTRIES,
    ]
}

/// The Chinese text for an English source string, when the table has it.
#[must_use]
pub fn lookup(text: &str) -> Option<&'static str> {
    TABLE
        .get_or_init(|| {
            let mut table = HashMap::new();
            for entries in tables() {
                for (source, translated) in entries {
                    table.insert(*source, *translated);
                }
            }
            table
        })
        .get(text)
        .copied()
}

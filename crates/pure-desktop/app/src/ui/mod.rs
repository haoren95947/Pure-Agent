//! The shell's view modules: the chrome the shell mounts over the
//! panels.
//!
//! Each module is a plain state machine plus a `show` that draws it
//! with egui widgets, so the logic tests without a window and the
//! shell only wires keys and store data in.

pub mod acp_form;
pub mod agents;
pub mod approval;
pub mod chrome;
pub mod composer;
pub mod constellation;
pub mod dialog;
pub mod directory_picker;
pub mod dock;
pub mod find;
pub mod forms;
pub mod keys;
pub mod mark;
pub mod mcp_form;
pub mod palette;
pub mod pickers;
pub mod provider_form;
pub mod settings;
pub mod settings_config;
pub mod setup;
pub mod shell;
pub mod sidebar;
pub mod splash;
pub mod status;
pub mod toasts;
pub mod toolbar;
pub mod topbar;
pub mod transcript;
pub mod welcome;
pub mod workbench;

//! The client's own preferences: the theme choice, the toggles the
//! settings dialog writes, and the marks the user puts on sessions.
//!
//! None of this is engine state, so none of it rides the wire. It is
//! stored as one JSON document: `desktop.json` in `~/.pure-agent`
//! natively, and one `localStorage` entry in the browser. A
//! document that is missing reads as the defaults, and a field it
//! lacks takes its default, so older documents keep loading. A
//! document that does not parse also reads as the defaults, but not
//! quietly: the unreadable file is copied to `desktop.json.corrupt`
//! before the next save can overwrite it, and the reset is reported.

use std::collections::BTreeSet;

use pure_client::wire::SessionConfigOption;
use serde::{Deserialize, Serialize};

use crate::theme::ThemeChoice;

/// Everything the client remembers about itself across launches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Prefs {
    /// The theme: one of the pure themes, or the platform's choice.
    pub theme: ThemeChoice,
    /// The interface language: the platform's, English or Simplified Chinese.
    pub language: crate::i18n::Language,
    /// Whether Enter sends. Off, Enter adds a newline and Ctrl+Enter
    /// sends.
    pub enter_sends: bool,
    /// Whether the sidebar groups sessions under their project.
    pub group_by_project: bool,
    /// Whether turning swarm mode on asks first.
    pub confirm_swarm: bool,
    /// Lab: swarm cards draw one star per worker.
    pub constellation: bool,
    /// Lab: the context ring opens the gauge with a Compact action.
    pub fuel: bool,
    /// Lab: the turn timeline rail beside the transcript.
    pub rail: bool,
    /// Whether a reply that arrives in bursts shows at a steady pace.
    pub smooth_stream: bool,
    /// Sessions pinned to the top of the sidebar.
    pub pinned: BTreeSet<String>,
    /// Models starred in the model picker, as `provider/model`.
    pub starred_models: BTreeSet<String>,
    /// Sessions left out of the sidebar until restored.
    pub archived: BTreeSet<String>,
    /// The `pure` binary the setup screen chose, run when none ships
    /// beside this client.
    pub engine_path: Option<String>,
    /// Unix seconds of the last completed release check, which runs
    /// at most once a day.
    pub update_checked_at: Option<i64>,
    /// The newest release the last check saw, without the `v`. The
    /// engine, this client and the web client share it.
    pub latest_version: Option<String>,
    /// Which release line updates follow: tagged stable releases or
    /// the rolling nightly built from main. One choice governs both
    /// the engine and this client's checks.
    pub channel: Channel,
    /// The newest nightly the last check saw, when the channel is
    /// nightly.
    pub latest_nightly: Option<NightlyBuild>,
    /// Why the last release check failed, when it did. The next
    /// successful check clears it.
    pub check_error: Option<String>,
    /// Whether a release check saw a newer engine or client, raised
    /// as a toast and a badge on the settings entry, and cleared
    /// when the Updates row is looked at.
    pub update_badge: bool,
    /// The model, thinking and mode options the last session showed,
    /// which the welcome pane offers before a session exists.
    pub template: Vec<SessionConfigOption>,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::System,
            language: crate::i18n::Language::System,
            enter_sends: true,
            group_by_project: true,
            confirm_swarm: true,
            constellation: true,
            fuel: true,
            rail: true,
            smooth_stream: true,
            pinned: BTreeSet::new(),
            starred_models: BTreeSet::new(),
            archived: BTreeSet::new(),
            engine_path: None,
            update_checked_at: None,
            latest_version: None,
            channel: Channel::default(),
            latest_nightly: None,
            check_error: None,
            update_badge: false,
            template: Vec::new(),
        }
    }
}

/// Which release line the update checks follow.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    /// Tagged stable releases, the default.
    #[default]
    Latest,
    /// The rolling `nightly` pre-release built from main.
    Nightly,
}

/// One nightly build the last check saw.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NightlyBuild {
    /// The full commit sha the nightly was built from.
    pub commit: String,
    /// The UTC date it was published, `YYYY-MM-DD`.
    pub date: String,
}

impl Prefs {
    /// The stored preferences, or `None` when the document does not
    /// parse; the caller warns and backs the file up.
    #[must_use]
    pub fn try_parse(text: &str) -> Option<Self> {
        serde_json::from_str(text).ok()
    }

    /// Reads a stored document; anything unreadable is the defaults.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        Self::try_parse(text).unwrap_or_default()
    }

    /// The document to store.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("prefs serialize")
    }
}

/// The stored preferences, or the defaults when none are stored. A
/// document that is present but unreadable warns and is copied aside
/// as `desktop.json.corrupt`, so nothing silently eats it.
#[must_use]
pub fn load() -> Prefs {
    let Some(text) = storage::read() else {
        return Prefs::default();
    };
    match Prefs::try_parse(&text) {
        Some(prefs) => prefs,
        None => {
            storage::backup(&text);
            crate::warn(
                "the saved preferences did not parse; the defaults are in use and the file is kept as desktop.json.corrupt",
            );
            Prefs::default()
        }
    }
}

/// Stores `prefs`. The write lands through a sibling temporary file
/// and a rename, so a crash mid-write leaves the old document, never
/// a torn one. A failed write is reported on stderr and otherwise
/// ignored: the running app keeps its state either way.
pub fn save(prefs: &Prefs) {
    if let Err(error) = storage::write(&prefs.to_json()) {
        crate::warn(&format!("saving preferences failed: {error}"));
    }
}

/// The Pure Agent data directory, for display: `PURE_HOME` when set,
/// else `~/.pure-agent`.
#[must_use]
pub fn display_config_dir() -> String {
    let env = |key: &str| {
        std::env::var_os(key)
            .map(|value| value.to_string_lossy().into_owned())
            .filter(|value| !value.trim().is_empty())
    };
    if let Some(dir) = env("PURE_HOME") {
        return dir;
    }
    match env("USERPROFILE").or_else(|| env("HOME")) {
        Some(home) => std::path::PathBuf::from(home)
            .join(".pure-agent")
            .to_string_lossy()
            .into_owned(),
        None => "~/.pure-agent".to_owned(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod storage {
    use std::path::{Path, PathBuf};

    use pure_client::unquote_and_trim;

    /// The data directory from the environment values: `PURE_HOME` when
    /// set, else `.pure-agent` under `USERPROFILE` (Windows) or `HOME`.
    /// Quoted and spaced values are cleaned, and a value that cleans to
    /// empty falls through.
    fn home_base(
        pure_home: Option<&str>,
        userprofile: Option<&str>,
        home: Option<&str>,
    ) -> Option<PathBuf> {
        fn clean(value: Option<&str>) -> Option<&str> {
            value
                .map(unquote_and_trim)
                .filter(|value| !value.is_empty())
        }
        if let Some(dir) = clean(pure_home) {
            return Some(PathBuf::from(dir));
        }
        clean(userprofile)
            .or_else(|| clean(home))
            .map(|user| PathBuf::from(user).join(".pure-agent"))
    }

    /// `desktop.json` in the data directory: `PURE_HOME` or
    /// `~/.pure-agent`.
    fn path() -> Option<PathBuf> {
        let env =
            |key: &str| std::env::var_os(key).map(|value| value.to_string_lossy().into_owned());
        let base = home_base(
            env("PURE_HOME").as_deref(),
            env("USERPROFILE").as_deref(),
            env("HOME").as_deref(),
        )?;
        Some(base.join("desktop.json"))
    }

    /// The unreadable document's sibling, kept for diagnosis.
    fn corrupt_path(path: &Path) -> PathBuf {
        path.with_extension("json.corrupt")
    }

    /// Writes `text` to a sibling temporary file and renames it over
    /// `path`, so `path` holds either the old document or the new one,
    /// never a torn half.
    fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub(super) fn read() -> Option<String> {
        std::fs::read_to_string(path()?).ok()
    }

    pub(super) fn write(text: &str) -> Result<(), String> {
        let path = path().ok_or("no config directory")?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        write_atomic(&path, text)
    }

    /// Copies `text` to the document's `.corrupt` sibling, so a save
    /// cannot be what erases it. Best effort: the reset is already
    /// reported.
    pub(super) fn backup(text: &str) {
        let Some(path) = path() else {
            return;
        };
        let _ = std::fs::write(corrupt_path(&path), text);
    }
    #[cfg(test)]
    mod tests {
        use super::{corrupt_path, home_base, write_atomic};
        use std::path::PathBuf;

        #[test]
        fn the_data_directory_is_pure_home_or_the_user_home() {
            assert_eq!(
                home_base(Some(" \"/var/pure\" "), Some("C:\\Users\\u"), None),
                Some(PathBuf::from("/var/pure"))
            );
            assert_eq!(
                home_base(None, Some("C:\\Users\\u"), Some("/home/u")),
                Some(PathBuf::from("C:\\Users\\u").join(".pure-agent"))
            );
            assert_eq!(
                home_base(Some(""), None, Some("/home/u")),
                Some(PathBuf::from("/home/u").join(".pure-agent"))
            );
            assert_eq!(home_base(None, None, None), None);
        }

        #[test]
        fn the_backup_sits_beside_the_document() {
            assert_eq!(
                corrupt_path(&PathBuf::from("/cfg/pure/desktop.json")),
                PathBuf::from("/cfg/pure/desktop.json.corrupt")
            );
        }

        #[test]
        fn an_atomic_write_leaves_no_temporary_file_and_lands_whole() {
            let dir = std::env::temp_dir().join(format!(
                "pure-desktop-prefs-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|since| since.subsec_nanos())
                    .unwrap_or(0)
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("desktop.json");
            write_atomic(&path, "first").unwrap();
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "first");
            write_atomic(&path, "second").unwrap();
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");
            assert_eq!(
                std::fs::read_dir(&dir).unwrap().count(),
                1,
                "no temporary file survives a save"
            );
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod storage {
    /// The `localStorage` key the document lives under, and the key
    /// the unreadable document is copied to.
    const KEY: &str = "pure.desktop";
    const CORRUPT: &str = "pure.desktop.corrupt";

    fn local() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok().flatten()
    }

    pub(super) fn read() -> Option<String> {
        local()?.get_item(KEY).ok().flatten()
    }

    pub(super) fn write(text: &str) -> Result<(), String> {
        local()
            .ok_or("no local storage")?
            .set_item(KEY, text)
            .map_err(|_| "local storage refused the write".to_owned())
    }

    /// Copies the unreadable document aside under its own key, so the
    /// next save cannot be what erases it. Best effort.
    pub(super) fn backup(text: &str) {
        if let Some(storage) = local() {
            let _ = storage.set_item(CORRUPT, text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Prefs;
    use crate::theme::ThemeChoice;

    #[test]
    fn a_document_round_trips_and_missing_fields_take_defaults() {
        let mut prefs = Prefs {
            theme: ThemeChoice::Named("kimi-light".into()),
            enter_sends: false,
            ..Prefs::default()
        };
        prefs.pinned.insert("s1".into());
        assert_eq!(Prefs::parse(&prefs.to_json()), prefs);

        let old = Prefs::parse(r#"{"theme": "shadow"}"#);
        assert_eq!(
            old.theme,
            ThemeChoice::Named("pure-shadow".into()),
            "an older client's name still reads"
        );
        assert!(
            old.enter_sends && old.constellation,
            "absent fields default"
        );
        assert_eq!(Prefs::parse("not json"), Prefs::default());
    }

    #[test]
    fn a_corrupt_document_reports_and_a_partial_one_does_not() {
        assert!(
            Prefs::try_parse("{not json").is_none(),
            "the caller learns the document is unreadable"
        );
        assert!(
            Prefs::try_parse(r#"{"theme": "pure-shadow"}"#).is_some(),
            "a valid document with absent fields still parses"
        );
    }
}

//! The client's interface language.
//!
//! Every visible string is written in English at its call site and passed
//! through [`t`] (or [`tf`] for a formatted template). English is the
//! source locale: a string the Chinese table does not cover reads as
//! written. That keeps untranslated corners usable and makes adding a
//! locale a matter of filling a table, not rewriting the views.
//!
//! Tables live in [`zh_cn`], one file per area, so each area's view and
//! its translations change together.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

mod zh_cn;

/// The interface language the user picked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Language {
    /// Follow the operating system, English when it is not Chinese.
    #[default]
    System,
    /// English, the source locale.
    English,
    /// Simplified Chinese.
    Chinese,
}

impl Language {
    /// The name the language picker shows, in the language itself.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::English => "English",
            Self::Chinese => "简体中文",
        }
    }
}

const SYSTEM: u8 = 0;
const ENGLISH: u8 = 1;
const CHINESE: u8 = 2;

static LANGUAGE: AtomicU8 = AtomicU8::new(SYSTEM);
static DETECT_OS: AtomicBool = AtomicBool::new(false);

/// Lets [`Language::System`] follow the operating system. The native binary
/// turns this on at start-up; tests and builds that do not run the desktop
/// keep System at English so text assertions stay stable.
pub fn detect_os_language(enabled: bool) {
    DETECT_OS.store(enabled, Ordering::Relaxed);
}

/// Picks the interface language; System is resolved on every lookup so a
/// locale change in the environment is honoured the next frame.
pub fn set_language(language: Language) {
    let value = match language {
        Language::System => SYSTEM,
        Language::English => ENGLISH,
        Language::Chinese => CHINESE,
    };
    LANGUAGE.store(value, Ordering::Relaxed);
}

/// The language the user picked.
#[must_use]
pub fn language() -> Language {
    match LANGUAGE.load(Ordering::Relaxed) {
        ENGLISH => Language::English,
        CHINESE => Language::Chinese,
        _ => Language::System,
    }
}

/// The language text actually renders in.
#[must_use]
pub fn resolved() -> Language {
    match language() {
        Language::System => system_language(),
        picked => picked,
    }
}

/// The operating system's language: Chinese where the Windows UI language,
/// `PURE_DESKTOP_LANG` or `LANG`/`LC_ALL` say so, English otherwise.
fn system_language() -> Language {
    let value = ["PURE_DESKTOP_LANG", "LC_ALL", "LANG"]
        .iter()
        .find_map(|key| std::env::var(key).ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if value.contains("zh") || value.contains("chinese") {
        return Language::Chinese;
    }
    if value.is_empty() && DETECT_OS.load(Ordering::Relaxed) && windows_ui_is_chinese() {
        return Language::Chinese;
    }
    Language::English
}

/// Whether the Windows UI language is any Chinese variant (primary
/// language id 0x04).
#[cfg(windows)]
fn windows_ui_is_chinese() -> bool {
    unsafe extern "system" {
        fn GetUserDefaultUILanguage() -> u16;
    }
    // SAFETY: the call reads the user's UI language and has no other effect.
    let language = unsafe { GetUserDefaultUILanguage() };
    language & 0x3ff == 0x04
}

#[cfg(not(windows))]
fn windows_ui_is_chinese() -> bool {
    false
}

/// Translates a source string. The argument type is `&'static str` so the
/// result stays `&'static str`: call sites keep their types and tests that
/// read the UI see the English source unless Chinese is selected.
#[must_use]
pub fn t(text: &'static str) -> &'static str {
    if resolved() == Language::Chinese {
        zh_cn::lookup(text).unwrap_or(text)
    } else {
        text
    }
}

/// Translates a string the engine sends at runtime. Unknown text reads as
/// written, and a missing table entry never hides the engine's own wording.
#[must_use]
pub fn dyn_t(text: &str) -> &str {
    if resolved() == Language::Chinese {
        zh_cn::lookup(text).unwrap_or(text)
    } else {
        text
    }
}

/// Translates a `format!`-style template, then fills its `{}` holes in
/// order. Only positional `{}` holes are supported; templates with other
/// format specs stay in English at their call site.
#[must_use]
pub fn tf(template: &'static str, args: &[&dyn std::fmt::Display]) -> String {
    let template = t(template);
    let mut out = String::with_capacity(template.len() + 16);
    let mut holes = template.split("{}");
    if let Some(first) = holes.next() {
        out.push_str(first);
    }
    for (arg, rest) in args.iter().zip(holes) {
        out.push_str(&arg.to_string());
        out.push_str(rest);
    }
    out
}

/// `format!` over a translated template: `tr!("{} of {}", done, total)`.
#[macro_export]
macro_rules! tr {
    ($template:literal $(, $arg:expr)* $(,)?) => {
        $crate::i18n::tf($template, &[$(&$arg as &dyn ::std::fmt::Display),*])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_is_the_source_locale() {
        set_language(Language::English);
        assert_eq!(t("Permissions"), "Permissions");
        assert_eq!(crate::tr!("{} sessions", 3), "3 sessions");
        set_language(Language::System);
    }

    #[test]
    fn chinese_table_covers_the_primary_pages() {
        for (source, translated) in [
            ("Settings", "设置"),
            ("Permissions", "权限"),
            ("Plugins", "插件"),
            ("Full access", "完全访问"),
            ("New session", "新建会话"),
        ] {
            let found = zh_cn::lookup(source);
            assert_eq!(found, Some(translated), "missing or wrong: {source}");
        }
    }

    #[test]
    fn a_missing_entry_reads_as_written() {
        assert_eq!(zh_cn::lookup("Not a translated string"), None);
    }
}

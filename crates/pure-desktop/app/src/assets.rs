//! The embedded brand art: logos and welcome art, included from
//! `assets/brand/`. The icon catalog (Lucide SVGs) arrives with S01.4.

/// The nekomata mark, dark palette.
pub(crate) static MARK_SHADOW: &[u8] = include_bytes!("../assets/brand/pure-mark-shadow.svg");
/// The nekomata mark, light palette.
pub(crate) static MARK_DAWN: &[u8] = include_bytes!("../assets/brand/pure-mark-dawn.svg");
/// The app icon at 256 px, as the hicolor set ships it.
pub(crate) static ICON_PNG: &[u8] = include_bytes!("../assets/icons/hicolor/256x256/apps/pure.png");
/// The welcome screen's glow backdrop, dark palette.
pub(crate) static WELCOME_GLOW_PNG: &[u8] = include_bytes!("../assets/brand/welcome-glow.png");
/// The welcome screen's glow backdrop, light palette.
pub(crate) static WELCOME_GLOW_DAWN_PNG: &[u8] =
    include_bytes!("../assets/brand/welcome-glow-dawn.png");

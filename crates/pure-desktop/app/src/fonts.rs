//! The font fallback: CJK coverage for the text the engine streams.
//!
//! egui's bundled faces cover Latin only. The engine's transcripts,
//! the prompts and the composer carry Japanese, Korean and Chinese
//! text, and an IME commits exactly that, so a missing glyph renders
//! as tofu. This module finds a system font that covers a CJK
//! codepoint and registers it behind the bundled families, so the
//! Latin faces still render every glyph they own and only the CJK
//! runs reach the fallback.
//!
//! Desktop asks `fontique`, which reads the platform's own font
//! registry: CoreText on macOS, DirectWrite on Windows and fontconfig
//! on Linux through its dlopen mode, so no path is hardcoded and the
//! binary never hard-links the C library. Web has no system fonts at
//! all, so it bundles a subset instead (E08 owns that bundle's size
//! against NFR-05).

/// The codepoint the fallback must cover: U+5F71, the first character
/// of the CJK measurement line.
pub const PROBE_CODEPOINT: u32 = 0x5F71;

/// The family name the discovered font registers under.
pub const FALLBACK_FAMILY: &str = "pure-cjk-fallback";

/// Whether a font's charmap covers [`PROBE_CODEPOINT`]. Native only:
/// the browser carries its own fonts and never scans a charmap.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn covers_cjk(charmap: Option<fontique::Charmap<'_>>) -> bool {
    charmap.is_some_and(|charmap| charmap.map(PROBE_CODEPOINT).is_some())
}

/// Registers a CJK fallback font, when the system has one. The font
/// lands behind every family's own faces, so a glyph a bundled face
/// owns still renders from that face.
///
/// Returns the family name the fallback registered under, or `None`
/// when the system carries no CJK font.
#[cfg(not(target_arch = "wasm32"))]
pub fn install_cjk_fallback(fonts: &mut egui::FontDefinitions) -> Option<String> {
    use fontique::CollectionOptions;

    let mut collection = fontique::Collection::new(CollectionOptions::default());
    collection.load_system_fonts();
    let mut source_cache = fontique::SourceCache::default();
    // The platform's own Han fallback first, then every other family in
    // name order: the name list is a hash map, so its order changes per
    // run and would pick a different face, with different advances, on
    // a system that has several.
    let mut ids: Vec<fontique::FamilyId> = collection
        .fallback_families(fontique::Script::from_bytes(*b"Hani"))
        .collect();
    let mut names: Vec<String> = collection.family_names().map(str::to_owned).collect();
    names.sort_unstable();
    ids.extend(names.iter().filter_map(|name| collection.family_id(name)));
    for id in ids {
        let Some(family) = collection.family(id) else {
            continue;
        };
        let Some(font) = family.default_font() else {
            continue;
        };
        let Some(blob) = font.load(Some(&mut source_cache)) else {
            continue;
        };
        if !covers_cjk(font.charmap_index().charmap(blob.as_ref())) {
            continue;
        }
        // The face may sit inside a collection, as macOS ships its CJK
        // fonts, so egui must read the same face the charmap came from.
        let mut data = egui::FontData::from_owned(blob.as_ref().to_vec());
        data.index = font.index();
        fonts
            .font_data
            .insert(FALLBACK_FAMILY.to_owned(), Arc::new(data));
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .push(FALLBACK_FAMILY.to_owned());
        }
        for name in [
            crate::theme::FAMILY_BODY_REGULAR,
            crate::theme::FAMILY_BODY_MEDIUM,
            crate::theme::FAMILY_BODY_SEMIBOLD,
            crate::theme::FAMILY_BODY_BOLD,
            crate::theme::FAMILY_BODY_EXTRABOLD,
            crate::theme::FAMILY_MONO_REGULAR,
            crate::theme::FAMILY_MONO_MEDIUM,
            crate::theme::FAMILY_MONO_SEMIBOLD,
            crate::theme::FAMILY_MONO_BOLD,
            crate::theme::FAMILY_MONO_EXTRABOLD,
            crate::theme::FAMILY_PROSE,
            crate::theme::FAMILY_PROSE_SEMIBOLD,
            crate::theme::FAMILY_PROSE_MONO,
        ] {
            fonts
                .families
                .entry(crate::theme::family(name))
                .or_default()
                .push(FALLBACK_FAMILY.to_owned());
        }
        return Some(FALLBACK_FAMILY.to_owned());
    }
    None
}

/// The web build carries no system fonts, so its CJK coverage is a
/// bundled subset (E08). Until that bundle lands, the fallback is
/// absent and the CJK text renders as tofu on the web target.
#[cfg(target_arch = "wasm32")]
pub fn install_cjk_fallback(_fonts: &mut egui::FontDefinitions) -> Option<String> {
    None
}

// The blob's Arc only exists where a system font was scanned.
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

#[cfg(test)]
mod tests {
    #[cfg(not(target_arch = "wasm32"))]
    use super::covers_cjk;
    use super::{FALLBACK_FAMILY, PROBE_CODEPOINT};

    #[test]
    fn the_probe_codepoint_is_the_cjk_measurement_character() {
        assert_eq!(PROBE_CODEPOINT, 0x5F71, "the CJK measurement glyph");
        assert_eq!(
            char::from_u32(PROBE_CODEPOINT),
            Some('\u{5F71}'),
            "the codepoint is the glyph"
        );
    }

    #[test]
    fn a_charmap_without_the_probe_is_not_a_fallback() {
        assert!(!covers_cjk(None));
    }

    #[test]
    fn the_fallback_registers_under_its_own_name() {
        assert_eq!(FALLBACK_FAMILY, "pure-cjk-fallback");
    }
}

//! The transcript's markdown pipeline: one cache per entry, a syntect
//! theme derived from the pure palette, and the viewer that renders
//! one entry's text.
//!
//! The cache discipline is the spec's: re-parse only the streaming tail
//! entry per frame. `egui_commonmark`'s own cache holds images and
//! scroll state, not the parsed document, so the parse decision is
//! ours to make: a completed entry is parsed once, when its text first
//! lands, and never again while the text stands; only the entry still
//! receiving chunks is parsed every frame. [`Parses`] is the counter
//! the spec's guard asserts on.

use std::collections::HashMap;
use std::ops::Range;

/// The cache name the palette-derived syntect themes register under.
pub const THEME_SHADOW: &str = "pure-shadow";
/// The light palette's theme name.
pub const THEME_DAWN: &str = "pure-dawn";

/// The parse counter the spec's cache-discipline guard reads.
#[derive(Debug, Default, Clone)]
pub struct Parses {
    /// How many times each entry id was parsed.
    per_entry: HashMap<String, usize>,
}

impl Parses {
    /// How many times `id` was parsed.
    #[must_use]
    pub fn count(&self, id: &str) -> usize {
        self.per_entry.get(id).copied().unwrap_or(0)
    }

    /// Records one parse of `id`.
    fn record(&mut self, id: &str) {
        *self.per_entry.entry(id.to_owned()).or_default() += 1;
    }
}

/// One entry's parse bookkeeping.
#[derive(Debug, Default)]
pub struct Entry {
    /// The text this entry last rendered, so a repeat render with no
    /// change is not a parse.
    last_text: Option<String>,
    /// The top-level blocks of `last_text`, each as the source the
    /// viewer renders, the block the transcript renders itself, or
    /// `None` for the viewer's, and whether the block is a table.
    blocks: Vec<(String, Option<super::prose::Block>, bool)>,
}

/// The parsed form of the transcript, keyed by the entry's id.
#[derive(Debug, Default)]
pub struct Pipeline {
    /// One entry per transcript item id.
    entries: HashMap<String, Entry>,
    /// The viewer's cache, shared by every entry: it loads the syntax
    /// and theme sets, megabytes each, and keys its scroll and image
    /// state by the viewer's id.
    cache: egui_commonmark::CommonMarkCache,
    /// The parse counter.
    pub parses: Parses,
}

impl Pipeline {
    /// A pipeline with no entries.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Drops every entry. Ids are row indexes, so the entries of one
    /// session mean nothing to the next one.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.parses.per_entry.clear();
    }

    /// Renders one entry's markdown. `streaming` marks the entry still
    /// receiving chunks: it is parsed on every frame, and a completed
    /// entry is parsed once, when its text first lands, and never
    /// again while the text stands.
    pub fn show(&mut self, id: &str, text: &str, streaming: bool, ui: &mut egui::Ui) {
        let Self {
            entries,
            cache,
            parses,
        } = self;
        let entry = entries.entry(id.to_owned()).or_default();
        let changed = entry.last_text.as_deref() != Some(text);
        if changed || streaming {
            entry.last_text = Some(text.to_owned());
            entry.blocks = blocks(text)
                .into_iter()
                .map(|range| {
                    let source = &text[range];
                    (
                        upright(source),
                        super::prose::parse(source),
                        is_table(source),
                    )
                })
                .collect();
            parses.record(id);
        }
        ui.scope(|ui| {
            // Prose sets at the reference's line height, through the
            // faces registered with that line gap, when they are.
            let prose = crate::theme::family(crate::theme::FAMILY_PROSE);
            let registered = ui
                .ctx()
                .fonts(|fonts| fonts.definitions().families.contains_key(&prose));
            let styles = &mut ui.style_mut().text_styles;
            for (style, face) in [
                (egui::TextStyle::Body, crate::theme::FAMILY_PROSE),
                (egui::TextStyle::Heading, crate::theme::FAMILY_PROSE),
                (egui::TextStyle::Monospace, crate::theme::FAMILY_PROSE_MONO),
            ] {
                if let Some(font) = styles.get_mut(&style)
                    && registered
                {
                    font.family = crate::theme::family(face);
                }
            }
            // Each top-level block renders on its own, so the viewer's
            // blank line between paragraphs gives way to the
            // reference's paragraph gap.
            ui.spacing_mut().item_spacing.y = 0.0;
            let palette = crate::theme::Palette::active_of(ui.ctx());
            let mut prev = None;
            for (n, (source, block, table)) in entry.blocks.iter().enumerate() {
                if n > 0 {
                    ui.add_space(super::prose::gap_after(prev));
                }
                ui.push_id(n, |ui| match block {
                    Some(block) if registered => super::prose::show(ui, block, palette),
                    _ => {
                        let mut viewer = |ui: &mut egui::Ui| {
                            egui_commonmark::CommonMarkViewer::new()
                                .syntax_theme_light(THEME_DAWN)
                                .syntax_theme_dark(THEME_SHADOW)
                                .show(ui, cache, source);
                        };
                        // A table's cells do not wrap, so a wide one
                        // scrolls sideways inside the column instead of
                        // pushing the column wider.
                        if *table {
                            egui::ScrollArea::horizontal()
                                .auto_shrink([false, true])
                                .show(ui, viewer);
                        } else {
                            viewer(ui);
                        }
                    }
                });
                prev = block.as_ref();
            }
        });
    }
}

/// The byte ranges of `text`'s top-level blocks, in order. A text with
/// a link reference definition stays one block, since a block cut away
/// from its definition would lose the link.
fn blocks(text: &str) -> Vec<Range<usize>> {
    use pulldown_cmark::{Event, Options, Parser};

    let whole = || {
        vec![Range {
            start: 0,
            end: text.len(),
        }]
    };
    let defines_links = text.lines().any(|line| {
        let line = line.trim_start();
        line.starts_with('[') && line.contains("]:")
    });
    if defines_links {
        return whole();
    }
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_DEFINITION_LIST;
    let mut out = Vec::new();
    let mut depth = 0usize;
    for (event, range) in Parser::new_ext(text, options).into_offset_iter() {
        match event {
            Event::Start(_) => {
                if depth == 0 {
                    out.push(range);
                }
                depth += 1;
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(range),
            _ => {}
        }
    }
    if out.is_empty() { whole() } else { out }
}

/// Whether `source` is a table block.
fn is_table(source: &str) -> bool {
    use pulldown_cmark::{Event, Options, Parser, Tag};

    matches!(
        Parser::new_ext(source, Options::ENABLE_TABLES).next(),
        Some(Event::Start(Tag::Table(_)))
    )
}

/// `source` with its emphasis markers dropped, so the viewer sets
/// emphasized text upright, as the reference does for want of an
/// italic face.
fn upright(source: &str) -> String {
    use pulldown_cmark::{Event, Options, Parser, Tag};

    let options = Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_DEFINITION_LIST;
    let mut cut: Vec<usize> = Parser::new_ext(source, options)
        .into_offset_iter()
        .filter(|(event, range)| matches!(event, Event::Start(Tag::Emphasis)) && range.len() > 1)
        .flat_map(|(_, range)| [range.start, range.end - 1])
        .collect();
    cut.sort_unstable();
    let mut out = String::with_capacity(source.len());
    let mut from = 0;
    for at in cut {
        out.push_str(&source[from..at]);
        from = at + 1;
    }
    out.push_str(&source[from..]);
    out
}

/// The syntect theme the pure palette implies, as `.tmTheme` bytes.
///
/// The mapping is the design's own: code blocks sit on the `deep`
/// surface with `ink` text, and the palette's hues carry the token
/// colors, so a code block reads as part of the shell rather than as a
/// pasted-in editor theme. Both palettes build the same structure over
/// their own tokens.
#[must_use]
pub fn theme_bytes(palette: &crate::theme::Palette) -> Vec<u8> {
    let hex = |c: egui::Color32| format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b());
    let bg = hex(palette.deep);
    let fg = hex(palette.ink);
    let comment = hex(palette.faint);
    let keyword = hex(palette.done);
    let string = hex(palette.ok);
    let number = hex(palette.accent);
    let function = hex(palette.info);
    let constant = hex(palette.warn);
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<plist version=\"1.0\">\n<dict>\n");
    out.push_str("<key>name</key>\n<string>pure</string>\n");
    out.push_str("<key>settings</key>\n<array>\n");
    // The base settings every token falls back to.
    out.push_str(&format!(
        "<dict><key>settings</key><dict>\
         <key>background</key><string>{bg}</string>\
         <key>foreground</key><string>{fg}</string>\
         <key>caret</key><string>{fg}</string>\
         <key>selection</key><string>{sel}</string>\
         </dict></dict>\n",
        sel = hex(palette.selected),
    ));
    // The token scopes, in syntect's order.
    let scopes = [
        ("comment", comment.as_str()),
        ("keyword", keyword.as_str()),
        ("keyword.operator", keyword.as_str()),
        ("storage", keyword.as_str()),
        ("string", string.as_str()),
        ("constant.numeric", number.as_str()),
        ("constant.language", constant.as_str()),
        ("entity.name.function", function.as_str()),
        ("variable.parameter", fg.as_str()),
        ("entity.name.tag", keyword.as_str()),
        ("support.function", function.as_str()),
        ("constant.character", number.as_str()),
    ];
    for (scope, color) in scopes {
        out.push_str(&format!(
            "<dict><key>scope</key><string>{scope}</string>\
             <key>settings</key><dict><key>foreground</key><string>{color}</string></dict></dict>\n"
        ));
    }
    out.push_str("</array>\n</dict>\n</plist>\n");
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::{Parses, Pipeline, blocks, theme_bytes, upright};

    #[test]
    fn the_text_splits_into_its_top_level_blocks() {
        let text =
            "One para\nstill one.\n\n- a\n\n- b\n\n```rust\nfn x() {}\n\nfn y() {}\n```\n\nLast.";
        let parts: Vec<&str> = blocks(text)
            .into_iter()
            .map(|r| text[r].trim_end())
            .collect();
        assert_eq!(parts.len(), 4, "{parts:?}");
        assert_eq!(parts[0], "One para\nstill one.");
        assert!(parts[1].starts_with("- a") && parts[1].ends_with("- b"));
        assert!(parts[2].starts_with("```rust") && parts[2].contains("fn y()"));
        assert_eq!(parts[3], "Last.");
    }

    #[test]
    fn a_text_with_link_definitions_stays_whole() {
        let text = "See [the docs][d].\n\n[d]: https://example.com";
        assert_eq!(blocks(text), vec![0..text.len()]);
    }

    #[test]
    fn the_viewer_source_drops_only_emphasis_markers() {
        assert_eq!(
            upright("| a | b |\n|---|---|\n| *x* | _y_ |"),
            "| a | b |\n|---|---|\n| x | y |"
        );
        assert_eq!(upright("***both*** and **bold**"), "**both** and **bold**");
        assert_eq!(upright("`*code*` and 2 * 3 * 4"), "`*code*` and 2 * 3 * 4");
        assert_eq!(upright("[*link*](https://e.com)"), "[link](https://e.com)");
    }
    use crate::theme::Palette;

    #[test]
    fn the_theme_bytes_parse_as_a_tmtheme() {
        for palette in [Palette::shadow(), Palette::dawn()] {
            let bytes = theme_bytes(&palette);
            let text = String::from_utf8(bytes).expect("the theme is text");
            assert!(text.contains("<plist"), "the theme is a plist");
            assert!(
                text.contains("<key>background</key>"),
                "it sets a background"
            );
            let mut cursor = std::io::Cursor::new(text.into_bytes());
            syntect::highlighting::ThemeSet::load_from_reader(&mut cursor)
                .expect("syntect parses the palette theme");
        }
    }

    #[test]
    fn the_two_palettes_build_different_themes() {
        assert_ne!(
            theme_bytes(&Palette::shadow()),
            theme_bytes(&Palette::dawn())
        );
    }

    #[test]
    fn a_completed_entry_is_parsed_once() {
        let mut parses = Parses::default();
        parses.record("s1");
        assert_eq!(parses.count("s1"), 1);
        assert_eq!(parses.count("s2"), 0, "an untouched entry counts zero");
    }

    #[test]
    fn clear_forgets_every_entry() {
        let mut pipeline = Pipeline::new();
        pipeline.parses.record("a");
        pipeline
            .entries
            .insert("a".to_owned(), super::Entry::default());
        pipeline.clear();
        assert_eq!(pipeline.parses.count("a"), 0);
        assert!(pipeline.entries.is_empty());
    }
}

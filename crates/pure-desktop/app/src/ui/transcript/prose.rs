//! The transcript's own rendering of the common markdown blocks, set
//! the way the reference's text view sets them: paragraphs, headings,
//! lists, code blocks, quotes and rules, at the reference's sizes, line
//! height and gaps.
//!
//! A block with anything else in it (a link, an image, a table, a
//! footnote, raw html) is left to `egui_commonmark`, which [`parse`]
//! says by returning `None`.

use crate::icons::PaintSnapped as _;
use crate::ui::forms::ShowCrisp as _;
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::theme::{
    FAMILY_MONO_REGULAR, FAMILY_PROSE, FAMILY_PROSE_SEMIBOLD, FS_BASE, Palette, R_LG, family,
};

/// The space between two blocks.
const GAP: f32 = 9.0;
/// The space under a heading, before the block it heads.
const HEADING_GAP: f32 = 4.0;
/// How far a quote's text sits in from its bar.
const QUOTE_INDENT: f32 = 17.0;
/// The code block's font size.
const CODE_SIZE: f32 = 12.5;
/// Inline code's font size.
const INLINE_CODE_SIZE: f32 = 12.0;

/// One run of text with its inline styles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Span {
    text: String,
    strong: bool,
    strike: bool,
    code: bool,
}

/// One block the transcript renders itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Block {
    /// A paragraph of inline runs.
    Para(Vec<Span>),
    /// A heading at its level, one being the largest.
    Heading(u8, Vec<Span>),
    /// A fenced or indented code block's text.
    Code(String),
    /// A quote around its blocks.
    Quote(Vec<Block>),
    /// A list, numbered from `start` when ordered, of items' blocks,
    /// each with its task box's state when it is a task.
    List {
        start: Option<u64>,
        items: Vec<(Option<bool>, Vec<Block>)>,
    },
    /// A thematic break.
    Rule,
}

/// A block still being read.
enum Open {
    Para(Vec<Span>),
    Heading(u8, Vec<Span>),
    Code(String),
    Quote(Vec<Block>),
    List(Option<u64>, Vec<(Option<bool>, Vec<Block>)>),
    Item(Option<bool>, Vec<Span>, Vec<Block>),
}

/// The block `text` holds, when it is one top-level block made only of
/// what this module renders.
pub(crate) fn parse(text: &str) -> Option<Block> {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_DEFINITION_LIST;
    let mut stack: Vec<Open> = Vec::new();
    let mut done: Vec<Block> = Vec::new();
    let (mut strong, mut strike) = (0u32, 0u32);
    for event in Parser::new_ext(text, options) {
        match event {
            Event::Start(tag) => {
                if starts_block(&tag) {
                    flush_item(&mut stack);
                }
                match tag {
                    Tag::Paragraph => stack.push(Open::Para(Vec::new())),
                    Tag::Heading { level, .. } => {
                        stack.push(Open::Heading(heading_level(level), Vec::new()));
                    }
                    Tag::CodeBlock(_) => stack.push(Open::Code(String::new())),
                    Tag::BlockQuote(_) => stack.push(Open::Quote(Vec::new())),
                    Tag::List(start) => stack.push(Open::List(start, Vec::new())),
                    Tag::Item => stack.push(Open::Item(None, Vec::new(), Vec::new())),
                    Tag::Strong => strong += 1,
                    Tag::Emphasis => {}
                    Tag::Strikethrough => strike += 1,
                    _ => return None,
                }
            }
            Event::End(tag) => match tag {
                TagEnd::Strong => strong = strong.saturating_sub(1),
                TagEnd::Emphasis => {}
                TagEnd::Strikethrough => strike = strike.saturating_sub(1),
                _ => {
                    let block = match stack.pop()? {
                        Open::Para(spans) => Block::Para(spans),
                        Open::Heading(level, spans) => Block::Heading(level, spans),
                        Open::Code(text) => Block::Code(text),
                        Open::Quote(blocks) => Block::Quote(blocks),
                        Open::List(start, items) => Block::List { start, items },
                        Open::Item(checked, spans, mut blocks) => {
                            if !spans.is_empty() {
                                blocks.push(Block::Para(spans));
                            }
                            match stack.last_mut() {
                                Some(Open::List(_, items)) => items.push((checked, blocks)),
                                _ => return None,
                            }
                            continue;
                        }
                    };
                    match stack.last_mut() {
                        None => done.push(block),
                        Some(Open::Quote(blocks) | Open::Item(_, _, blocks)) => {
                            blocks.push(block);
                        }
                        Some(_) => return None,
                    }
                }
            },
            Event::TaskListMarker(done) => match stack.last_mut() {
                Some(Open::Item(checked, ..)) => *checked = Some(done),
                _ => return None,
            },
            Event::Text(text) => {
                if let Some(Open::Code(code)) = stack.last_mut() {
                    code.push_str(&text);
                } else {
                    inline(&mut stack)?.push(span(&text, strong, strike, false));
                }
            }
            Event::Code(text) => {
                inline(&mut stack)?.push(span(&text, strong, strike, true));
            }
            Event::SoftBreak => inline(&mut stack)?.push(span(" ", strong, strike, false)),
            Event::HardBreak => inline(&mut stack)?.push(span("\n", strong, strike, false)),
            Event::Rule => {
                flush_item(&mut stack);
                match stack.last_mut() {
                    None => done.push(Block::Rule),
                    Some(Open::Quote(blocks) | Open::Item(_, _, blocks)) => {
                        blocks.push(Block::Rule);
                    }
                    Some(_) => return None,
                }
            }
            _ => return None,
        }
    }
    if done.len() == 1 && stack.is_empty() {
        done.pop()
    } else {
        None
    }
}

/// Whether `tag` opens a block, which ends an item's loose leading text.
fn starts_block(tag: &Tag<'_>) -> bool {
    matches!(
        tag,
        Tag::Paragraph
            | Tag::Heading { .. }
            | Tag::CodeBlock(_)
            | Tag::BlockQuote(_)
            | Tag::List(_)
    )
}

/// Turns the text an item has gathered so far into its first paragraph,
/// so a nested block lands after it.
fn flush_item(stack: &mut [Open]) {
    if let Some(Open::Item(_, spans, blocks)) = stack.last_mut()
        && !spans.is_empty()
    {
        blocks.push(Block::Para(std::mem::take(spans)));
    }
}

/// The runs the innermost inline holder gathers into.
fn inline(stack: &mut [Open]) -> Option<&mut Vec<Span>> {
    match stack.last_mut()? {
        Open::Para(spans) | Open::Heading(_, spans) | Open::Item(_, spans, _) => Some(spans),
        _ => None,
    }
}

fn span(text: &str, strong: u32, strike: u32, code: bool) -> Span {
    Span {
        text: text.to_owned(),
        strong: strong > 0,
        strike: strike > 0,
        code,
    }
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// The space `next` sits below `prev`: tight under a heading, the
/// block gap otherwise.
pub(crate) fn gap_after(prev: Option<&Block>) -> f32 {
    match prev {
        Some(Block::Heading(..)) => HEADING_GAP,
        _ => GAP,
    }
}

/// Draws `block` across the available width.
pub(crate) fn show(ui: &mut egui::Ui, block: &Block, palette: &Palette) {
    show_at(ui, block, palette, 0);
}

/// Draws `block` inside `depth` lists, which picks its list markers.
fn show_at(ui: &mut egui::Ui, block: &Block, palette: &Palette, depth: usize) {
    let ink = ui.visuals().text_color();
    match block {
        Block::Para(spans) => {
            let job = layout(spans, FS_BASE, false, ink, palette);
            ui.add(egui::Label::new(job).wrap().selectable(true));
        }
        Block::Heading(level, spans) => {
            let size = match level {
                1 => 17.0,
                2 => 15.0,
                _ => 14.0,
            };
            let job = layout(spans, size, true, palette.ink_strong, palette);
            ui.add(egui::Label::new(job).wrap().selectable(true));
        }
        Block::Code(text) => code_block(ui, text, palette),
        Block::Quote(blocks) => {
            let inner = egui::Frame::NONE
                .inner_margin(egui::Margin {
                    left: QUOTE_INDENT as i8,
                    ..Default::default()
                })
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.visuals_mut().override_text_color = Some(palette.muted);
                    stack(ui, blocks, palette, depth);
                });
            let rect = inner.response.rect;
            ui.painter().rect_filled(
                egui::Rect::from_min_size(rect.min, egui::vec2(2.0, rect.height())),
                egui::CornerRadius::ZERO,
                palette.line_strong,
            );
        }
        Block::List { start, items } => {
            for (n, (checked, item)) in items.iter().enumerate() {
                let marker = match start {
                    Some(first) => format!("{}.", first + n as u64),
                    None => match depth {
                        0 => "\u{2022}",
                        1 => "\u{25E6}",
                        _ => "\u{25AA}",
                    }
                    .to_owned(),
                };
                // The marker sets inline, so the text starts after the
                // marker and a space, as the reference's list runs. A
                // task's box takes the marker's place.
                let indent = if checked.is_some() {
                    TASK_BOX + TASK_GAP
                } else {
                    ui.painter()
                        .layout_no_wrap(
                            format!("{marker} "),
                            egui::FontId::new(FS_BASE, family(FAMILY_PROSE)),
                            ink,
                        )
                        .size()
                        .x
                        .ceil()
                };
                let item_rect = ui
                    .push_id(n, |ui| {
                        egui::Frame::NONE
                            .inner_margin(egui::Margin {
                                left: indent as i8,
                                ..Default::default()
                            })
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                stack_tight(ui, item, palette, depth + 1);
                            })
                            .response
                            .rect
                    })
                    .inner;
                let mid = item_rect.top() + crate::theme::line_h(FS_BASE) / 2.0;
                match checked {
                    Some(done) => {
                        task_box(
                            ui,
                            egui::pos2(item_rect.left(), mid),
                            *done,
                            ink,
                            palette.bg,
                        );
                    }
                    None => {
                        ui.painter().text(
                            egui::pos2(item_rect.left(), mid),
                            egui::Align2::LEFT_CENTER,
                            marker,
                            egui::FontId::new(FS_BASE, family(FAMILY_PROSE)),
                            ink,
                        );
                    }
                }
            }
        }
        Block::Rule => {
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), crate::theme::line_h(FS_BASE)),
                egui::Sense::hover(),
            );
            ui.painter().hline(
                rect.x_range(),
                rect.center().y,
                egui::Stroke::new(1.0, palette.line),
            );
        }
    }
}

/// A task box's side.
const TASK_BOX: f32 = 14.0;
/// The space between a task box and its text.
const TASK_GAP: f32 = 6.0;

/// A task item's box with its left edge at `left_mid`: a hairline
/// square in `ink`, filled with a `check` colored tick when `done`, as
/// the reference's text view draws a task.
fn task_box(
    ui: &egui::Ui,
    left_mid: egui::Pos2,
    done: bool,
    ink: egui::Color32,
    check: egui::Color32,
) {
    let rect = egui::Rect::from_min_size(
        egui::pos2(left_mid.x, crate::theme::snap(left_mid.y - TASK_BOX / 2.0)),
        egui::vec2(TASK_BOX, TASK_BOX),
    );
    if done {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::ZERO, ink);
        crate::icons::themed_image(crate::icons::Icon::Check, 10.0, check).paint_snapped(
            ui,
            egui::Rect::from_center_size(rect.center(), egui::vec2(10.0, 10.0)),
        );
    } else {
        ui.painter().rect_stroke(
            rect,
            egui::CornerRadius::ZERO,
            egui::Stroke::new(1.0, ink),
            egui::StrokeKind::Inside,
        );
    }
}

/// `blocks` one under the other, at the block gaps.
fn stack(ui: &mut egui::Ui, blocks: &[Block], palette: &Palette, depth: usize) {
    let mut prev = None;
    for (n, block) in blocks.iter().enumerate() {
        if n > 0 {
            ui.add_space(gap_after(prev));
        }
        ui.push_id(n, |ui| show_at(ui, block, palette, depth));
        prev = Some(block);
    }
}

/// An item's blocks, with no gap between them, as a tight list sets.
fn stack_tight(ui: &mut egui::Ui, blocks: &[Block], palette: &Palette, depth: usize) {
    for (n, block) in blocks.iter().enumerate() {
        ui.push_id(n, |ui| show_at(ui, block, palette, depth));
    }
}

/// The code block: plain mono on the deep surface, in a hairline box,
/// scrolling sideways when a line runs past the width.
fn code_block(ui: &mut egui::Ui, text: &str, palette: &Palette) {
    let text = text.strip_suffix('\n').unwrap_or(text);
    // The lines step at the reference's snapped line height, and the
    // block's text moves down to the baseline it snaps the line to,
    // where egui would set the glyphs at the line's top.
    let line = crate::theme::line_h(CODE_SIZE);
    let job = egui::text::LayoutJob::single_section(
        text.to_owned(),
        egui::TextFormat {
            font_id: egui::FontId::new(CODE_SIZE, family(FAMILY_MONO_REGULAR)),
            color: palette.ink,
            line_height: Some(line),
            ..Default::default()
        },
    );
    let shift = ui
        .painter()
        .layout_job(job.clone())
        .rows
        .first()
        .and_then(|row| row.row.glyphs.first())
        .map_or(0.0, |glyph| {
            let lead = (line - glyph.font_height) / 2.0;
            crate::theme::snap(lead + glyph.font_face_ascent) - glyph.pos.y
        })
        .clamp(0.0, 9.0) as i8;
    egui::Frame::NONE
        .fill(palette.deep)
        .stroke(egui::Stroke::new(1.0, palette.line))
        .corner_radius(egui::CornerRadius::same(R_LG as u8))
        .inner_margin(egui::Margin {
            left: 12,
            right: 12,
            top: 9 + shift,
            bottom: 11 - shift,
        })
        .show_crisp(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::horizontal()
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.add(egui::Label::new(job).extend().selectable(true));
                });
        });
}

/// The layout of `spans` at `size`, semibold throughout when `bold`.
fn layout(
    spans: &[Span],
    size: f32,
    bold: bool,
    ink: egui::Color32,
    palette: &Palette,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    for span in spans {
        let face = if span.code {
            FAMILY_MONO_REGULAR
        } else if bold || span.strong {
            FAMILY_PROSE_SEMIBOLD
        } else {
            FAMILY_PROSE
        };
        let font_size = if span.code { INLINE_CODE_SIZE } else { size };
        let mut format = egui::TextFormat {
            font_id: egui::FontId::new(font_size, family(face)),
            color: ink,
            valign: egui::Align::Center,
            ..Default::default()
        };
        if span.code {
            format.background = palette.fill;
        }
        if span.strike {
            format.strikethrough = egui::Stroke::new(1.0, ink);
        }
        if span.code {
            // Thin spaces pad the chip the background paints, which
            // takes the mono face's own line, not the prose line.
            job.append(&format!("\u{2009}{}\u{2009}", span.text), 0.0, format);
        } else {
            job.append(&span.text, 0.0, format);
        }
    }
    job
}

#[cfg(test)]
mod tests {
    use super::{Block, parse};

    fn text(block: &Block) -> String {
        match block {
            Block::Para(spans) | Block::Heading(_, spans) => {
                spans.iter().map(|span| span.text.as_str()).collect()
            }
            other => format!("{other:?}"),
        }
    }

    #[test]
    fn paragraphs_carry_their_inline_styles() {
        let Some(Block::Para(spans)) = parse("A **bold** and `code` run.") else {
            panic!("a plain paragraph is ours");
        };
        assert!(spans.iter().any(|span| span.strong && span.text == "bold"));
        assert!(spans.iter().any(|span| span.code && span.text == "code"));
    }

    #[test]
    fn headings_lists_and_code_are_ours() {
        let Some(Block::Heading(2, _)) = parse("## Lists") else {
            panic!("a heading is ours");
        };
        let Some(Block::List { start, items }) = parse("3. one\n4. two\n   - nested") else {
            panic!("a list is ours");
        };
        assert_eq!(start, Some(3));
        assert_eq!(items.len(), 2);
        assert_eq!(text(&items[0].1[0]), "one");
        assert!(matches!(items[1].1[1], Block::List { start: None, .. }));
        let Some(Block::Code(code)) = parse("```rust\nfn x() {}\n```") else {
            panic!("a code block is ours");
        };
        assert_eq!(code, "fn x() {}\n");
    }

    #[test]
    fn task_items_carry_their_box() {
        let Some(Block::List { items, .. }) = parse("- [ ] open\n- [x] done\n- plain") else {
            panic!("a task list is ours");
        };
        let checked: Vec<Option<bool>> = items.iter().map(|(checked, _)| *checked).collect();
        assert_eq!(checked, vec![Some(false), Some(true), None]);
        assert_eq!(text(&items[1].1[0]), "done");
    }

    #[test]
    fn links_and_tables_go_to_the_viewer() {
        assert!(parse("See [the docs](https://example.com).").is_none());
        assert!(parse("| a | b |\n|---|---|\n| 1 | 2 |").is_none());
    }
}

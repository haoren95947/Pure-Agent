//! The transcript's text-bearing rows, drawn with egui widgets.
//!
//! One renderer per row kind that carries words rather than tool
//! output: the user bubble, the prompt the engine built from agent
//! text, the assistant reply, the compaction summary, the thinking
//! block, the notice, the engine note, the compaction row, the
//! permission decision and the working row. The tool, group, plan,
//! agent and changes rows live in their own modules.
//!
//! Each renderer takes `&mut egui::Ui` and the data its row carries,
//! draws with the design's tokens and hands back an [`egui::Response`]
//! plus, where the row offers an action, the action itself: the caller
//! owns the store, the composer and the clipboard, so a renderer never
//! reaches past its own rect. The click that opens or closes a body
//! comes back through the returned response.

use crate::icons::PaintSnapped as _;
use crate::ui::forms::ShowCrisp as _;
use std::time::Duration;

use pure_client::agent_text::{self, AgentText, Report};
use pure_client::wire::NoticeTone;
use pure_client::{Session, TranscriptItem};

use super::head::{self, Head, Lead, Target};
use super::model::last_sentence;
use crate::theme::{
    FAMILY_BODY_REGULAR, FAMILY_MONO_REGULAR, FS_2XS, FS_BASE, FS_SM, FS_XS, Palette, R_FULL, R_MD,
    SP_1, SP_2, SP_3, SP_4, SP_5, family,
};

/// The activity row's height, the design's minimum row height.
const ROW_H: f32 = 28.0;
/// The user bubble's corner radius.
const BUBBLE_R: f32 = 12.0;
/// The user bubble's horizontal padding.
const BUBBLE_PX: f32 = 14.0;
/// The user bubble's vertical padding.
const BUBBLE_PY: f32 = 9.0;
/// The bubble's widest share of the column.
const BUBBLE_MAX: f32 = 0.85;
/// The left indent of a detail box or an expanded body.
const DETAIL_INDENT: f32 = 22.0;
/// The line height of the thinking body.
const THINK_LINE: f32 = 20.0;
/// The icon size of the design's small rows.
const ICON_SM: f32 = 14.0;
/// The icon size of the design's extra small rows.
const ICON_XS: f32 = 12.0;
/// The user row's hover strip: the reference's extra small ghost
/// buttons, which keep their room while hidden.
const USER_STRIP_H: f32 = 17.0;
/// The steered bubble's dashed border, one dash and one gap.
const DASH: f32 = 4.0;

/// What a user row's hover actions ask the caller to do.
///
/// The renderer owns the drawing; the caller owns the composer and the
/// clipboard, so the actions travel back out rather than reaching into
/// the shell from inside a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserAction {
    /// Copy the message text to the clipboard.
    Copy(String),
    /// Put the message text in the composer to edit and resend.
    Edit(String),
    /// Fork a new session from this message's turn.
    Fork,
    /// Drop this message and everything after it.
    Rewind,
}

/// The names of a prompt's non-text blocks, for its attachment chips:
/// images and audio by their kind, a resource link by its name and an
/// embedded resource by the file its uri names.
fn attachment_names(content: &[pure_client::wire::ContentBlock]) -> Vec<String> {
    use pure_client::wire::ContentBlock;

    content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Image(_) => Some("image".to_owned()),
            ContentBlock::Audio(_) => Some("audio".to_owned()),
            ContentBlock::ResourceLink(link) => Some(link.name.clone()),
            ContentBlock::Resource(resource) => Some(
                resource.resource["uri"]
                    .as_str()
                    .map_or("resource", |uri| uri.rsplit('/').next().unwrap_or(uri))
                    .to_owned(),
            ),
            _ => None,
        })
        .collect()
}

/// The monospace font of the design's paths and commands.
fn mono(size: f32) -> egui::FontId {
    egui::FontId::new(size, family(FAMILY_MONO_REGULAR))
}

/// One icon of the design's catalog at its extra small size.
fn icon_xs(ui: &mut egui::Ui, icon: crate::icons::Icon, color: egui::Color32) -> egui::Response {
    crate::icons::icon(ui, icon, ICON_XS, color)
}

/// The body an expanded row drops under its head: indented, ruled on
/// its left edge in the design's border token.
fn detail_frame(ui: &mut egui::Ui, palette: &Palette, contents: impl FnOnce(&mut egui::Ui)) {
    let rule_x = ui.cursor().left() + DETAIL_INDENT + 1.0;
    let top = ui.cursor().top();
    let body = egui::Rect::from_min_max(
        egui::pos2(rule_x + 1.0 + SP_5, top),
        ui.max_rect().right_bottom(),
    );
    let inner = ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(body)
            .layout(egui::Layout::top_down(egui::Align::Min)),
        contents,
    );
    ui.painter().vline(
        rule_x,
        egui::Rangef::new(top, inner.response.rect.bottom()),
        egui::Stroke::new(2.0, palette.line),
    );
}

/// The steered bubble's border, drawn as dashes because egui strokes
/// carry no dash pattern.
pub(crate) fn dashed_rect(ui: &egui::Ui, rect: egui::Rect, radius: f32, color: egui::Color32) {
    let r = radius
        .min(rect.width() / 2.0)
        .min(rect.height() / 2.0)
        .max(0.0);
    let quarter = std::f32::consts::FRAC_PI_2;
    let corners = [
        (egui::pos2(rect.right() - r, rect.top() + r), -quarter),
        (egui::pos2(rect.right() - r, rect.bottom() - r), 0.0),
        (egui::pos2(rect.left() + r, rect.bottom() - r), quarter),
        (egui::pos2(rect.left() + r, rect.top() + r), 2.0 * quarter),
    ];
    let mut outline: Vec<egui::Pos2> = Vec::new();
    for (center, start) in corners {
        for step in 0..=8 {
            let angle = start + quarter * step as f32 / 8.0;
            outline.push(center + r * egui::vec2(angle.cos(), angle.sin()));
        }
    }
    outline.push(outline[0]);
    let stroke = egui::Stroke::new(1.0, color);
    let mut on = true;
    let mut left = DASH;
    for pair in outline.windows(2) {
        let (mut from, to) = (pair[0], pair[1]);
        let mut length = (to - from).length();
        while length > 0.0 {
            let take = left.min(length);
            let next = from + (to - from) * (take / length);
            if on {
                ui.painter().line_segment([from, next], stroke);
            }
            from = next;
            length -= take;
            left -= take;
            if left <= 0.0 {
                on = !on;
                left = DASH;
            }
        }
    }
}

/// A user message: its attachments, a right-aligned bubble, the steered
/// tag when it rode into a running turn, and hover actions.
pub fn render_user(
    ui: &mut egui::Ui,
    text: &str,
    attached: &[String],
    steered: bool,
    lineage: bool,
) -> (egui::Response, Vec<UserAction>) {
    let palette = Palette::active_of(ui.ctx());
    let mut actions = Vec::new();
    let top = ui.cursor().top();
    let row = ui
        .with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            if !attached.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = SP_3;
                    for name in attached {
                        egui::Frame::NONE
                            .stroke(egui::Stroke::new(1.0, palette.line))
                            .corner_radius(egui::CornerRadius::same(R_FULL as u8))
                            .inner_margin(egui::Margin::symmetric(SP_4 as i8, 0))
                            .show_crisp(ui, |ui| {
                                ui.set_min_height(24.0);
                                ui.spacing_mut().item_spacing.x = SP_2;
                                icon_xs(ui, crate::icons::Icon::Paperclip, palette.muted);
                                ui.label(
                                    egui::RichText::new(name).size(FS_XS).color(palette.muted),
                                );
                            });
                    }
                });
                ui.add_space(SP_3);
            }
            let row_w = ui.available_width();
            let bubble = egui::Frame::NONE
                .fill(if steered {
                    egui::Color32::TRANSPARENT
                } else {
                    palette.bubble
                })
                .corner_radius(egui::CornerRadius::same(BUBBLE_R as u8))
                .inner_margin(egui::Margin::symmetric(BUBBLE_PX as i8, BUBBLE_PY as i8))
                .show(ui, |ui| {
                    // The bubble takes up to its share of the row, and
                    // the text that share less the bubble's padding.
                    let max_text = (row_w * BUBBLE_MAX - 2.0 * BUBBLE_PX).max(1.0);
                    ui.set_max_width(max_text);
                    // Text that wraps keeps the bubble at its full share,
                    // ragged on the right, as the reference's does.
                    let font = egui::FontId::new(FS_BASE, crate::theme::prose_family(ui.ctx()));
                    let natural = ui
                        .painter()
                        .layout_no_wrap(text.to_owned(), font.clone(), egui::Color32::PLACEHOLDER)
                        .size()
                        .x;
                    if natural > max_text {
                        ui.set_min_width(max_text);
                    } else {
                        ui.set_max_width(natural.ceil());
                    }
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.add(egui::Label::new(
                            egui::RichText::new(text).font(font).color(if steered {
                                palette.ink
                            } else {
                                palette.ink_strong
                            }),
                        ));
                    });
                });
            if steered {
                dashed_rect(ui, bubble.response.rect, BUBBLE_R, palette.line_strong);
                ui.add_space(SP_2);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = SP_2;
                    icon_xs(ui, crate::icons::Icon::CornerDownRight, palette.faint);
                    ui.label(
                        egui::RichText::new("steered into the running turn")
                            .size(FS_2XS)
                            .color(palette.faint),
                    );
                });
            }
            ui.add_space(SP_2);
            let (strip, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), USER_STRIP_H),
                egui::Sense::hover(),
            );
            let over = egui::Rect::from_min_max(egui::pos2(strip.left(), top), strip.max);
            if ui.rect_contains_pointer(over) {
                let mut buttons = vec![
                    (
                        crate::icons::Icon::Copy,
                        "Copy",
                        UserAction::Copy(text.to_owned()),
                    ),
                    (
                        crate::icons::Icon::Pencil,
                        "Edit and resend",
                        UserAction::Edit(text.to_owned()),
                    ),
                ];
                if lineage {
                    buttons.push((
                        crate::icons::Icon::GitFork,
                        "Fork from here",
                        UserAction::Fork,
                    ));
                    buttons.push((
                        crate::icons::Icon::Undo2,
                        "Rewind to here",
                        UserAction::Rewind,
                    ));
                }
                let step = USER_STRIP_H + SP_1;
                let mut x = strip.right() - step * buttons.len() as f32 + SP_1;
                for (glyph, tip, action) in buttons {
                    let at = egui::Rect::from_min_size(
                        egui::pos2(x, strip.top()),
                        egui::vec2(USER_STRIP_H, USER_STRIP_H),
                    );
                    if head::icon_button_sized(ui, at, glyph, ICON_XS, tip, palette).clicked() {
                        actions.push(action);
                    }
                    x += step;
                }
            }
        })
        .response;
    (row, actions)
}

/// Draws one agent report, the card the caller's agent module owns.
pub type ReportDraw<'a> = &'a mut dyn FnMut(&mut egui::Ui, usize, &Report);

/// Draws the delivered prompt: the typed words as their own bubble when
/// there are any, then each agent text part in order.
///
/// `report` draws one agent report, the card the caller's agent module
/// owns; `None` leaves a quiet line naming the agent instead, so no
/// delivered part is dropped.
pub fn render_delivered(
    ui: &mut egui::Ui,
    words: &str,
    parts: &[AgentText],
    mut report: Option<ReportDraw<'_>>,
    lineage: bool,
) -> (egui::Response, Vec<UserAction>) {
    let palette = Palette::active_of(ui.ctx());
    let mut actions = Vec::new();
    let mut row = None;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = SP_3;
        if !words.is_empty() {
            let (words_row, words_actions) = render_user(ui, words, &[], false, lineage);
            row = Some(words_row);
            actions.extend(words_actions);
        }
        for (n, part) in parts.iter().enumerate() {
            match part {
                AgentText::Report(report_text) => match report.as_mut() {
                    Some(draw) => draw(ui, n, report_text),
                    None => {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} {}",
                                    report_text.name, report_text.state
                                ))
                                .size(FS_XS)
                                .color(palette.faint),
                            );
                        });
                    }
                },
                AgentText::Mail { from, body } => {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = SP_3;
                        icon_xs(ui, crate::icons::Icon::MessageSquare, palette.faint);
                        ui.label(
                            egui::RichText::new(format!("Message from {from}: {body}"))
                                .size(FS_XS)
                                .color(palette.muted),
                        );
                    });
                }
            };
            row = Some(ui.response());
        }
    });
    let response =
        row.unwrap_or_else(|| ui.allocate_response(egui::Vec2::ZERO, egui::Sense::hover()));
    (response, actions)
}

/// Assistant reply text as markdown, with a caret while it streams. The
/// pipeline owns the parse caches, so the caller threads its own in.
pub fn render_assistant(
    ui: &mut egui::Ui,
    pipeline: &mut super::markdown::Pipeline,
    ix: usize,
    text: &str,
    live: bool,
) -> egui::Response {
    let palette = Palette::active_of(ui.ctx());
    let id = format!("assistant-{ix}");
    ui.scope(|ui| {
        pipeline.show(&id, text, live, ui);
        if live {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(7.0, 14.0), egui::Sense::hover());
            // The caret blinks once a second, lit for the first half.
            let phase = ui.input(|i| i.time) % 1.0;
            if phase < 0.5 {
                ui.painter()
                    .rect_filled(rect, egui::CornerRadius::ZERO, palette.ink_strong);
            }
            ui.ctx()
                .request_repaint_after(Duration::from_secs_f64(0.5 - phase % 0.5));
        }
    })
    .response
}

/// The summary a compaction left in place of the history before it,
/// folded under one line and read as markdown when opened.
pub fn render_summary(
    ui: &mut egui::Ui,
    pipeline: &mut super::markdown::Pipeline,
    ix: usize,
    summary: &str,
    expanded: bool,
) -> egui::Response {
    let palette = Palette::active_of(ui.ctx());
    let response = head::show(
        ui,
        Head {
            lead: Lead::Icon(crate::icons::Icon::Layers, palette.muted),
            verb: "Context summary".to_owned(),
            verb_color: palette.ink,
            target: None,
            meta: Vec::new(),
            chevron: Some(expanded),
            nested: false,
        },
    );
    if expanded {
        ui.add_space(SP_2);
        let id = format!("summary-{ix}");
        detail_frame(ui, palette, |ui| {
            pipeline.show(&id, summary, false, ui);
        });
    }
    response
}

/// Assistant reasoning: a live peek, then a collapsible body.
pub fn render_thinking(
    ui: &mut egui::Ui,
    text: &str,
    live: bool,
    expanded: bool,
    duration: Option<Duration>,
) -> egui::Response {
    let palette = Palette::active_of(ui.ctx());
    let label = match (live, duration) {
        (true, _) => "Thinking".to_owned(),
        (false, Some(span)) => format!("Thought for {}s", span.as_secs().max(1)),
        (false, None) => "Thought".to_owned(),
    };
    let peek = (live && !expanded).then(|| Target {
        text: last_sentence(text),
        font: egui::FontId::new(FS_SM, family(FAMILY_BODY_REGULAR)),
        color: palette.faint,
    });
    let response = head::show(
        ui,
        Head {
            lead: if live {
                Lead::Working
            } else {
                Lead::Icon(crate::icons::Icon::Lightbulb, palette.muted)
            },
            verb: label,
            verb_color: if live { palette.muted } else { palette.ink },
            target: peek,
            meta: Vec::new(),
            chevron: Some(expanded),
            nested: false,
        },
    );
    if expanded {
        ui.add_space(SP_2);
        detail_frame(ui, palette, |ui| {
            ui.label(
                egui::RichText::new(text)
                    .size(FS_SM)
                    .color(palette.muted)
                    .line_height(Some(THINK_LINE)),
            );
        });
    }
    response
}

/// The design's notice row: the text between two flanking hairlines, in
/// the tone's color. An error rides in a danger pill.
pub fn render_notice(ui: &mut egui::Ui, ix: usize, tone: NoticeTone, text: &str) -> egui::Response {
    let palette = Palette::active_of(ui.ctx());
    let (color, pill) = match tone {
        NoticeTone::Info => (palette.faint, false),
        NoticeTone::Warn => (palette.warn, false),
        NoticeTone::Error => (palette.danger, true),
        NoticeTone::Success => (palette.ok, false),
    };
    let _ = ix;
    ruled_line(ui, text, color, pill, palette)
}

/// A note the engine added as a user message, such as swarm mode
/// switching on: one quiet line, not a bubble the user typed.
pub fn render_engine_note(ui: &mut egui::Ui, ix: usize, note: &str) -> egui::Response {
    let palette = Palette::active_of(ui.ctx());
    let x_text = ICON_XS + SP_2;
    let galley = small_text(ui, note, palette.faint, ui.available_width() - x_text);
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), galley.size().y),
        egui::Sense::hover(),
    );
    let first = rect.top() + crate::theme::line_h(FS_XS) / 2.0;
    crate::icons::themed_image(crate::icons::Icon::Info, ICON_XS, palette.faint).paint_snapped(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(rect.left(), first - ICON_XS / 2.0),
            egui::vec2(ICON_XS, ICON_XS),
        ),
    );
    ui.painter().galley(
        egui::pos2(rect.left() + x_text, rect.top()),
        galley,
        palette.faint,
    );
    let _ = ix;
    response
}

/// What a compaction card shows: the item's index (its expansion id),
/// the counts the update delivered and, when the agent reported them,
/// the summary text. `expanded` is the row's fold state.
pub struct CompactionCard<'a> {
    /// The transcript item index.
    pub ix: usize,
    /// Recent turns kept verbatim.
    pub kept: u64,
    /// Context tokens in use before the compaction, when the agent
    /// reported them.
    pub before: Option<u64>,
    /// Context tokens in use by the first turn after it, when that
    /// turn reported within the same run.
    pub after: Option<u64>,
    /// Older turns the summary replaced, when the agent reported it.
    pub summarized: Option<u64>,
    /// The summary that replaced the older turns, when reported.
    pub summary: Option<&'a str>,
    /// Whether the summary body shows.
    pub expanded: bool,
}

/// A compaction card: the counts the summary saved in its head, the
/// summary itself folded under it and read as markdown when opened.
/// Without a summary (an agent that predates reporting it) the card
/// stays one line with no chevron.
pub fn render_compaction(
    ui: &mut egui::Ui,
    pipeline: &mut super::markdown::Pipeline,
    card: CompactionCard<'_>,
) -> egui::Response {
    let palette = Palette::active_of(ui.ctx());
    let CompactionCard {
        ix,
        kept,
        before,
        after,
        summarized,
        summary,
        expanded,
    } = card;
    let mut meta = Vec::new();
    match summarized {
        Some(summarized) => meta.push(head::Meta::Faint(format!(
            "{summarized} {} summarized",
            if summarized == 1 { "turn" } else { "turns" }
        ))),
        None => meta.push(head::Meta::Faint(format!(
            "kept {kept} {}",
            if kept == 1 { "turn" } else { "turns" }
        ))),
    }
    if let (Some(before), Some(after)) = (before, after) {
        meta.push(head::Meta::Faint(format!(
            "{} \u{2192} {} tokens",
            compact_tokens(before),
            compact_tokens(after)
        )));
    }
    let response = head::show(
        ui,
        Head {
            lead: Lead::Icon(crate::icons::Icon::Layers, palette.muted),
            verb: "Context compacted".to_owned(),
            verb_color: palette.ink,
            target: None,
            meta,
            chevron: summary.map(|_| expanded),
            nested: false,
        },
    );
    if expanded && let Some(body) = summary.and_then(summary_of) {
        ui.add_space(SP_2);
        let id = format!("compaction-{ix}");
        detail_frame(ui, palette, |ui| {
            pipeline.show(&id, body, false, ui);
        });
    }
    response
}

/// Compact token count in the modeline's style: `999`, `84.2k`,
/// `1.2M`.
fn compact_tokens(n: u64) -> String {
    if n < 1_000 {
        n.to_string()
    } else if n < 1_000_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    }
}

/// Small text centered between two hairlines that take the rest of the
/// width, wrapping when it runs past it, on a danger pill when asked.
fn ruled_line(
    ui: &mut egui::Ui,
    text: &str,
    color: egui::Color32,
    pill: bool,
    palette: &Palette,
) -> egui::Response {
    let pad = if pill {
        egui::vec2(10.0, SP_2)
    } else {
        egui::Vec2::ZERO
    };
    let width = ui.available_width();
    let galley = small_text(ui, text, color, width - 2.0 * SP_4 - 2.0 * pad.x);
    let body = galley.size() + 2.0 * pad;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, body.y), egui::Sense::hover());
    let mid = rect.center().y;
    let body = egui::Rect::from_center_size(rect.center(), body);
    let stroke = egui::Stroke::new(1.0, palette.subtle);
    ui.painter()
        .hline(rect.left()..=body.left() - SP_4, mid, stroke);
    ui.painter()
        .hline(body.right() + SP_4..=rect.right(), mid, stroke);
    if pill {
        ui.painter().rect_filled(
            body,
            egui::CornerRadius::same(R_MD as u8),
            palette.danger_soft,
        );
    }
    ui.painter().galley(body.min + pad, galley, color);
    response
}

/// `text` at the small size and the design's line height, wrapped at
/// `width`.
fn small_text(
    ui: &egui::Ui,
    text: &str,
    color: egui::Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let format = egui::TextFormat {
        font_id: egui::FontId::new(FS_XS, family(FAMILY_BODY_REGULAR)),
        color,
        line_height: Some(crate::theme::line_h(FS_XS)),
        ..Default::default()
    };
    let mut job = egui::text::LayoutJob::single_section(text.to_owned(), format);
    job.wrap.max_width = width.max(0.0);
    ui.painter().layout_job(job)
}

/// The one-line record of an answered permission ask: what the ask was
/// about, the chosen label as offered, and the feedback quoted when it
/// rode along.
pub fn render_decision(
    ui: &mut egui::Ui,
    ix: usize,
    subject: &str,
    label: &str,
    allowed: bool,
    feedback: Option<&str>,
) -> egui::Response {
    let palette = Palette::active_of(ui.ctx());
    let color = if allowed { palette.ok } else { palette.danger };
    let glyph = if allowed {
        crate::icons::Icon::ShieldCheck
    } else {
        crate::icons::Icon::ShieldX
    };
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), ROW_H),
        egui::Sense::hover(),
    );
    let aria = match feedback {
        Some(feedback) => format!("{label} {subject} \"{feedback}\""),
        None => format!("{label} {subject}"),
    };
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &aria));
    let mid = rect.center().y;
    let mut x = rect.left() + SP_3;
    crate::icons::themed_image(glyph, ICON_XS, color).paint_snapped(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(x, mid - ICON_XS / 2.0),
            egui::vec2(ICON_XS, ICON_XS),
        ),
    );
    x += ICON_XS + SP_4;
    let small = egui::FontId::new(FS_XS, family(FAMILY_BODY_REGULAR));
    let label = ui
        .painter()
        .layout_no_wrap(label.to_owned(), small.clone(), palette.muted);
    let label_w = label.size().x;
    ui.painter().galley(
        egui::pos2(x, crate::theme::line_y(mid, 0.0, &label)),
        label,
        palette.muted,
    );
    x += label_w + SP_4;
    let mut right = rect.right() - SP_3;
    if let Some(feedback) = feedback {
        let quote = ui
            .painter()
            .layout_job(egui::text::LayoutJob::single_section(
                format!("\"{feedback}\""),
                egui::TextFormat {
                    font_id: small,
                    color: palette.faint,
                    ..Default::default()
                },
            ));
        right -= quote.size().x;
        ui.painter().galley(
            egui::pos2(right, crate::theme::line_y(mid, 0.0, &quote)),
            quote,
            palette.faint,
        );
        right -= SP_4;
    }
    let subject = head::one_line(
        ui,
        subject.to_owned(),
        egui::TextFormat::simple(mono(12.0), palette.ink),
        right - x,
    );
    ui.painter().galley(
        egui::pos2(x, crate::theme::line_y(mid, 0.0, &subject)),
        subject,
        palette.ink,
    );
    let _ = ix;
    response
}

/// The working row: a spinner and a quiet label while the run goes on
/// with nothing streaming yet. `label` names what it is doing, such
/// as `Working` or `Compacting`.
pub fn render_waiting(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let palette = Palette::active_of(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), ROW_H),
        egui::Sense::hover(),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, label));
    let mid = rect.center().y;
    head::paint_spinner(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(rect.left(), mid - ICON_SM / 2.0),
            egui::vec2(ICON_SM, ICON_SM),
        ),
        palette.accent,
    );
    ui.painter().text(
        egui::pos2(rect.left() + ICON_SM + SP_4, mid),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::new(FS_SM, family(FAMILY_BODY_REGULAR)),
        palette.muted,
    );
    response
}

/// What one prompt delivered as agent text, for the rows that read it:
/// the typed words and the parts after them, or `None` for a prompt
/// that is plain words.
#[must_use]
pub fn split_prompt(text: &str) -> Option<(&str, Vec<AgentText>)> {
    agent_text::split(text)
}

/// What a note the engine added as a user message says, or `None` for
/// any other text.
#[must_use]
pub fn note_of(text: &str) -> Option<String> {
    agent_text::engine_note(text)
}

/// The body of the summary the engine wrote in place of compacted
/// history, or `None` for any other text.
#[must_use]
pub fn summary_of(text: &str) -> Option<&str> {
    agent_text::compaction_summary(text)
}

/// The names of a prompt's non-text blocks, for its attachment chips.
#[must_use]
pub fn attachments(session: &Session, ix: usize) -> Vec<String> {
    match session.items.get(ix) {
        Some(TranscriptItem::User { content, .. }) => attachment_names(content),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::{BUBBLE_MAX, DASH, ICON_SM, ROW_H, THINK_LINE, UserAction, attachment_names, mono};
    use pure_client::wire::{BlobContent, ContentBlock};

    #[test]
    fn attachment_names_read_each_non_text_block() {
        let image = ContentBlock::Image(BlobContent {
            data: String::new(),
            mime_type: "image/png".into(),
            uri: None,
        });
        let audio = ContentBlock::Audio(BlobContent {
            data: String::new(),
            mime_type: "audio/wav".into(),
            uri: None,
        });
        assert_eq!(
            attachment_names(&[image, audio]),
            ["image".to_owned(), "audio".to_owned()]
        );
    }

    #[test]
    fn attachment_names_ignore_text_and_unknown_blocks() {
        let plain = [ContentBlock::text("plain"), ContentBlock::Unknown];
        assert!(attachment_names(&plain).is_empty());
    }

    #[test]
    fn an_embedded_resource_reads_the_file_its_uri_names() {
        let resource = ContentBlock::Resource(pure_client::wire::EmbeddedResource {
            resource: serde_json::json!({"uri": "file:///w/src/main.rs"}),
        });
        assert_eq!(attachment_names(&[resource]), ["main.rs".to_owned()]);
    }

    #[test]
    fn the_user_row_offers_copy_and_edit_for_its_text() {
        let copy = UserAction::Copy("hello".to_owned());
        let edit = UserAction::Edit("hello".to_owned());
        assert_eq!(copy, UserAction::Copy("hello".to_owned()));
        assert_eq!(edit, UserAction::Edit("hello".to_owned()));
        assert_ne!(copy, edit);
    }

    #[test]
    fn the_design_metrics_hold_their_tokens() {
        const _: () = assert!(BUBBLE_MAX > 0.5 && BUBBLE_MAX < 1.0);
        assert_eq!(ROW_H, 28.0);
        assert_eq!(ICON_SM, 14.0);
        assert_eq!(THINK_LINE, 20.0);
        assert_eq!(DASH, 4.0);
        assert_eq!(mono(12.0).size, 12.0);
    }
}

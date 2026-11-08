//! The one-line head a collapsible transcript row opens with.
//!
//! Tool calls, tool groups, todo updates and reasoning all share one
//! anatomy: a lead glyph, the verb, an optional target that truncates,
//! an optional meta cluster pushed to the right edge, and the chevron.
//! The chevron trails the meta cluster when there is one and the label
//! otherwise. The head is painted at a fixed height, so it can never
//! wrap or grow with the space its parent offers.

use crate::icons::PaintSnapped as _;
use std::f32::consts::TAU;
use std::sync::Arc;

use crate::icons::Icon;
use crate::theme::{
    FAMILY_BODY_REGULAR, FAMILY_MONO_REGULAR, FS_2XS, FS_SM, Palette, R_FULL, R_SM, SP_3, SP_4,
    SP_5, family,
};

/// The head's height.
pub(crate) const ROW_H: f32 = 28.0;
/// The lead glyph and the chevron's size.
const ICON: f32 = 14.0;
/// A meta chip's height.
const CHIP_H: f32 = 20.0;
/// How long the spinner takes for one turn, in seconds.
const SPIN_TURN: f32 = 0.9;

/// What a head leads with.
pub(crate) enum Lead {
    /// A catalog icon in one color.
    Icon(Icon, egui::Color32),
    /// The working spinner in one color.
    Spinner(egui::Color32),
    /// The working cat of live reasoning.
    Working,
}

/// One item of the meta cluster at the head's right edge.
pub(crate) enum Meta {
    /// A rounded mono chip on a tint.
    Chip {
        /// The chip's text.
        text: String,
        /// The text color.
        fg: egui::Color32,
        /// The fill.
        bg: egui::Color32,
    },
    /// Quiet mono text, such as a duration.
    Faint(String),
}

/// The text after the verb, cut with an ellipsis where room runs out.
pub(crate) struct Target {
    /// The text.
    pub text: String,
    /// The font it sets in.
    pub font: egui::FontId,
    /// Its color.
    pub color: egui::Color32,
}

impl Target {
    /// The design's mono target at the detail size, in the muted ink.
    pub(crate) fn mono(text: impl Into<String>, palette: &Palette) -> Self {
        Self {
            text: text.into(),
            font: egui::FontId::new(12.0, family(FAMILY_MONO_REGULAR)),
            color: palette.muted,
        }
    }
}

/// Everything a head shows.
pub(crate) struct Head {
    /// The lead glyph.
    pub lead: Lead,
    /// The verb, such as `Read` or `Thought for 2s`.
    pub verb: String,
    /// The verb's color.
    pub verb_color: egui::Color32,
    /// The text after the verb.
    pub target: Option<Target>,
    /// The meta cluster, in reading order.
    pub meta: Vec<Meta>,
    /// The chevron, pointing down when open, when the row expands.
    pub chevron: Option<bool>,
    /// Whether the head sits inside a group's list: inset further,
    /// square, and without the outdent a top-level head hangs by.
    pub nested: bool,
}

/// Paints `head` across the available width and reports clicks on it.
/// A head without a chevron only reports hover.
pub(crate) fn show(ui: &mut egui::Ui, head: Head) -> egui::Response {
    let palette = Palette::active_of(ui.ctx());
    let sense = if head.chevron.is_some() {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (mut rect, mut response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW_H), sense);
    if !head.nested {
        rect.min.x -= SP_3;
    }
    let label = head.verb.clone();
    let kind = if head.chevron.is_some() {
        egui::WidgetType::Button
    } else {
        egui::WidgetType::Label
    };
    response.widget_info(|| egui::WidgetInfo::labeled(kind, true, &label));
    if head.chevron.is_some() {
        response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    }
    if response.hovered() {
        let radius = if head.nested { 0 } else { R_SM as u8 };
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(radius), palette.hover);
    }
    let mid = rect.center().y;
    let mut x = rect.left() + if head.nested { SP_5 } else { SP_3 };
    let lead = egui::Rect::from_min_size(egui::pos2(x, mid - ICON / 2.0), egui::vec2(ICON, ICON));
    match head.lead {
        Lead::Icon(icon, color) => {
            crate::icons::themed_image(icon, ICON, color).paint_snapped(ui, lead);
        }
        Lead::Spinner(color) => paint_spinner(ui, lead, color),
        Lead::Working => crate::ui::mark::working(ui, lead, crate::clock::epoch()),
    }
    x = lead.right() + SP_4;

    let verb = ui.painter().layout_no_wrap(
        head.verb,
        egui::FontId::new(FS_SM, family(FAMILY_BODY_REGULAR)),
        head.verb_color,
    );
    let verb_w = verb.size().x;
    ui.painter().galley(
        egui::pos2(x, crate::theme::line_y(mid, 0.0, &verb)),
        verb,
        head.verb_color,
    );
    x += verb_w + SP_4;

    let mut right = rect.right() - SP_3;
    let trailing_chevron = !head.meta.is_empty();
    if let Some(open) = head.chevron
        && trailing_chevron
    {
        paint_chevron(
            ui,
            egui::pos2(right - ICON, mid - ICON / 2.0),
            open,
            palette,
        );
        right -= ICON + SP_4;
    }
    for meta in head.meta.into_iter().rev() {
        right = paint_meta(ui, right, mid, meta, palette) - SP_4;
    }

    let chevron_room = if head.chevron.is_some() && !trailing_chevron {
        ICON + SP_4
    } else {
        0.0
    };
    if let Some(target) = head.target {
        let format = egui::TextFormat {
            font_id: target.font,
            color: target.color,
            ..Default::default()
        };
        let galley = one_line(ui, target.text, format, right - chevron_room - x);
        let width = galley.size().x;
        ui.painter().galley(
            egui::pos2(x, crate::theme::line_y(mid, 0.0, &galley)),
            galley,
            target.color,
        );
        x += width + SP_4;
    }
    if let Some(open) = head.chevron
        && !trailing_chevron
    {
        paint_chevron(ui, egui::pos2(x, mid - ICON / 2.0), open, palette);
    }
    response
}

/// The square a quiet icon button takes.
pub(crate) const ICON_BTN: f32 = 20.0;

/// A quiet icon button at `rect`: no frame, the hover fill painted
/// behind it.
pub(crate) fn icon_button(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    glyph: crate::icons::Icon,
    tooltip: &str,
    palette: &Palette,
) -> egui::Response {
    icon_button_sized(ui, rect, glyph, ICON, tooltip, palette)
}

/// [`icon_button`] with its glyph at `size`.
pub(crate) fn icon_button_sized(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    glyph: crate::icons::Icon,
    size: f32,
    tooltip: &str,
    palette: &Palette,
) -> egui::Response {
    let id = ui
        .id()
        .with(("icon-button", tooltip, rect.left().to_bits()));
    let response = ui
        .interact(rect, id, egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(4), palette.fill_hover);
    }
    let color = if response.hovered() {
        palette.ink
    } else {
        palette.muted
    };
    crate::icons::themed_image(glyph, size, color).paint_snapped(
        ui,
        egui::Rect::from_center_size(rect.center(), egui::vec2(size, size)),
    );
    response.on_hover_text(tooltip)
}

/// `text` on one line, cut with an ellipsis past `max_width` the way
/// the reference truncates: a line whose shaped width fits stays
/// whole, and otherwise the cut sums unkerned character widths and
/// keeps only the characters that leave room for the ellipsis, then
/// drops trailing spaces and punctuation.
pub(crate) fn one_line(
    ui: &egui::Ui,
    text: String,
    format: egui::TextFormat,
    max_width: f32,
) -> Arc<egui::Galley> {
    let max = max_width.max(0.0);
    let painter = ui.painter();
    let full = painter.layout_job(egui::text::LayoutJob::single_section(
        text.clone(),
        format.clone(),
    ));
    if full.size().x <= max {
        return full;
    }
    let font = &format.font_id;
    let (widths, ellipsis) = painter.fonts_mut(|fonts| {
        let widths: Vec<f32> = text.chars().map(|c| fonts.glyph_width(font, c)).collect();
        (widths, fonts.glyph_width(font, '\u{2026}'))
    });
    let mut fits = 0;
    let mut width = 0.0;
    let mut cut = None;
    for ((at, _), advance) in text.char_indices().zip(widths) {
        if width + ellipsis < max {
            fits = at;
        }
        width += advance;
        if f32::floor(width) > max {
            cut = Some(fits);
            break;
        }
    }
    let Some(cut) = cut else {
        return full;
    };
    let kept =
        text[..cut].trim_end_matches(|c: char| c.is_whitespace() || c.is_ascii_punctuation());
    painter.layout_job(egui::text::LayoutJob::single_section(
        format!("{kept}\u{2026}"),
        format,
    ))
}

/// Paints one meta item with its right edge at `right`, centered on
/// `mid`, and gives back its left edge.
pub(crate) fn paint_meta(
    ui: &egui::Ui,
    right: f32,
    mid: f32,
    meta: Meta,
    palette: &Palette,
) -> f32 {
    let font = egui::FontId::new(FS_2XS, family(FAMILY_MONO_REGULAR));
    match meta {
        Meta::Chip { text, fg, bg } => {
            let galley = ui.painter().layout_no_wrap(text, font, fg);
            let size = egui::vec2(galley.size().x + 14.0, CHIP_H);
            let rect =
                egui::Rect::from_min_size(egui::pos2(right - size.x, mid - CHIP_H / 2.0), size);
            ui.painter()
                .rect_filled(rect, egui::CornerRadius::same(R_FULL as u8), bg);
            ui.painter()
                .galley(rect.center() - galley.size() / 2.0, galley, fg);
            rect.left()
        }
        Meta::Faint(text) => {
            let galley = ui.painter().layout_no_wrap(text, font, palette.faint);
            let left = right - galley.size().x;
            ui.painter().galley(
                egui::pos2(left, crate::theme::line_y(mid, 0.0, &galley)),
                galley,
                palette.faint,
            );
            left
        }
    }
}

/// Paints a meta chip with its left edge at `at.x`, centered on
/// `at.y`, and gives back where it landed.
pub(crate) fn paint_chip(
    ui: &egui::Ui,
    at: egui::Pos2,
    text: &str,
    fg: egui::Color32,
    bg: egui::Color32,
) -> egui::Rect {
    let galley = ui.painter().layout_no_wrap(
        text.to_owned(),
        egui::FontId::new(FS_2XS, family(FAMILY_MONO_REGULAR)),
        fg,
    );
    let rect = egui::Rect::from_min_size(
        egui::pos2(at.x, at.y - CHIP_H / 2.0),
        egui::vec2(galley.size().x + 14.0, CHIP_H),
    );
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(R_FULL as u8), bg);
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, fg);
    rect
}

/// Paints the row chevron at `at`: right when closed, down when open.
fn paint_chevron(ui: &egui::Ui, at: egui::Pos2, open: bool, palette: &Palette) {
    let icon = if open {
        Icon::ChevronDown
    } else {
        Icon::ChevronRight
    };
    crate::icons::themed_image(icon, ICON, palette.faint)
        .paint_snapped(ui, egui::Rect::from_min_size(at, egui::vec2(ICON, ICON)));
}

/// Paints the working spinner into `rect`: an arc turning once every
/// [`SPIN_TURN`], asking for the frames it turns through.
pub(crate) fn paint_spinner(ui: &egui::Ui, rect: egui::Rect, color: egui::Color32) {
    ui.ctx().request_repaint();
    let center = rect.center();
    let radius = rect.width().min(rect.height()) / 2.0 - 1.0;
    let turn = (ui.ctx().input(|i| i.time) % f64::from(SPIN_TURN)) as f32 / SPIN_TURN;
    let start = turn * TAU;
    let steps = 24;
    let points: Vec<egui::Pos2> = (0..=steps)
        .map(|step| {
            let angle = start + TAU * 0.75 * (step as f32 / steps as f32);
            egui::pos2(
                center.x + radius * angle.cos(),
                center.y + radius * angle.sin(),
            )
        })
        .collect();
    ui.painter()
        .add(egui::Shape::line(points, egui::Stroke::new(2.0, color)));
}

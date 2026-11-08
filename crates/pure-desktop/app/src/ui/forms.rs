//! Pieces the settings forms share: a labeled field, name and value
//! rows, segmented choices, and how a form learns whether its write
//! landed.
//!
//! Every text field is made with its value as a default rather than set
//! after: setting text lays it out at once, and before the field's first
//! render that layout uses a font the web cannot resolve.

use crate::icons::PaintSnapped as _;
use std::collections::BTreeMap;

use crate::theme::{
    FAMILY_BODY_REGULAR, FAMILY_BODY_SEMIBOLD, FAMILY_MONO_REGULAR, FS_SM, FS_XS, Palette, R_FULL,
    R_MD, family,
};

/// What the snapshot shows in place of a secret value.
pub const REDACTED: &str = "<redacted>";

/// The name and value rows of a map a form edits, such as headers or
/// environment variables. A row whose value the snapshot redacted
/// starts blank and keeps the saved value unless one is typed.
#[derive(Debug, Default)]
pub struct Pairs {
    rows: Vec<Pair>,
}

impl Pairs {
    /// Rows for `map`, as the snapshot shows it.
    #[must_use]
    pub fn new(map: &BTreeMap<String, String>) -> Self {
        let rows = map
            .iter()
            .map(|(name, value)| {
                let kept = value == REDACTED;
                Pair {
                    name: name.clone(),
                    value: if kept { String::new() } else { value.clone() },
                    kept,
                }
            })
            .collect();
        Self { rows }
    }

    /// The rows, as the form draws them.
    #[must_use]
    pub fn rows(&self) -> &[Pair] {
        &self.rows
    }

    /// Sets row `ix`'s name.
    pub fn set_name(&mut self, ix: usize, name: String) {
        if let Some(row) = self.rows.get_mut(ix) {
            row.name = name;
        }
    }

    /// Sets row `ix`'s value.
    pub fn set_value(&mut self, ix: usize, value: String) {
        if let Some(row) = self.rows.get_mut(ix) {
            row.value = value;
        }
    }

    /// Adds a blank row.
    pub fn push(&mut self) {
        self.rows.push(Pair {
            name: String::new(),
            value: String::new(),
            kept: false,
        });
    }

    /// Drops row `ix`.
    pub fn remove(&mut self, ix: usize) {
        if ix < self.rows.len() {
            self.rows.remove(ix);
        }
    }

    /// The map the rows hold. A row without a name drops out, and a kept
    /// row left blank sends the redaction marker, which keeps the saved
    /// value.
    #[must_use]
    pub fn values(&self) -> BTreeMap<String, String> {
        self.rows
            .iter()
            .filter_map(|row| {
                let name = row.name.trim().to_owned();
                if name.is_empty() {
                    return None;
                }
                let value = if row.value.is_empty() && row.kept {
                    REDACTED.to_owned()
                } else {
                    row.value.clone()
                };
                Some((name, value))
            })
            .collect()
    }
}

/// One name and value row, as the form draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pair {
    /// The name the row holds.
    pub name: String,
    /// The value the row holds.
    pub value: String,
    /// The saved value is secret and stays unless a new one is typed.
    pub kept: bool,
}

/// Where a form's save stands.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Saving {
    /// Nothing sent.
    #[default]
    Idle,
    /// Writes in flight, by request id; the form closes once all land.
    Sent(Vec<u64>),
    /// Why the form cannot save, or why the engine refused.
    Refused(String),
}

impl Saving {
    /// Follows the store's answers to the writes in flight: `true` once
    /// every write landed, a refusal once one failed.
    pub fn settle(&mut self, store: &crate::store::Store) -> bool {
        let Self::Sent(ids) = self else {
            return false;
        };
        let mut done = true;
        for id in ids.iter() {
            match store.write_outcome(*id) {
                Some(Ok(())) => {}
                Some(Err(why)) => {
                    *self = Self::Refused(why.clone());
                    return false;
                }
                None => done = false,
            }
        }
        done
    }

    /// The line a form shows under its fields: the refusal, or that the
    /// save is on its way.
    #[must_use]
    pub fn line(&self, palette: &Palette) -> Option<(String, egui::Color32)> {
        match self {
            Self::Idle => None,
            Self::Sent(_) => Some((crate::i18n::t("Saving\u{2026}").to_owned(), palette.muted)),
            Self::Refused(why) => Some((why.clone(), palette.danger)),
        }
    }

    /// Whether a save is in flight.
    #[must_use]
    pub fn busy(&self) -> bool {
        matches!(self, Self::Sent(_))
    }
}

/// A labeled form row: the label with a star when required, the
/// control, and a hint under it. The control is drawn by `control`
/// inside a full-width column.
pub fn field(
    ui: &mut egui::Ui,
    label: &str,
    required: bool,
    hint: Option<&str>,
    control: impl FnOnce(&mut egui::Ui),
) {
    ui.add_space(5.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.label(
            egui::RichText::new(label)
                .size(FS_XS)
                .color(ui.visuals().text_color()),
        );
        if required {
            ui.label(
                egui::RichText::new("*")
                    .size(FS_XS)
                    .color(crate::theme::Palette::active_of(ui.ctx()).danger),
            );
        }
    });
    control(ui);
    if let Some(hint) = hint {
        ui.label(
            egui::RichText::new(hint)
                .size(FS_XS)
                .color(crate::theme::Palette::active_of(ui.ctx()).faint),
        );
    }
}

/// A segmented choice: one segment per `(value, label)`, each as wide
/// as its label, the current one filled. Returns the value clicked
/// this frame, if any.
pub fn segments(
    ui: &mut egui::Ui,
    id: &str,
    choices: &[(&'static str, &'static str)],
    current: &str,
    palette: &Palette,
) -> Option<&'static str> {
    let font = egui::FontId::new(FS_XS, family(FAMILY_BODY_REGULAR));
    let galleys: Vec<_> = choices
        .iter()
        .map(|(value, label)| {
            let ink = if *value == current {
                palette.ink_strong
            } else {
                palette.muted
            };
            ui.painter()
                .layout_no_wrap((*label).to_owned(), font.clone(), ink)
        })
        .collect();
    let width = galleys
        .iter()
        .map(|galley| galley.size().x + 20.0)
        .sum::<f32>()
        + 2.0 * (choices.len() as f32 + 1.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 30.0), egui::Sense::hover());
    let radius = egui::CornerRadius::same(R_MD as u8);
    ui.painter().rect_filled(rect, radius, palette.fill);
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(1.0, palette.line),
        egui::StrokeKind::Inside,
    );
    let mut left = rect.left() + 2.0;
    let mut picked = None;
    for ((value, label), galley) in choices.iter().zip(galleys) {
        let on = *value == current;
        let segment = egui::Rect::from_min_size(
            egui::pos2(left, rect.top() + 3.0),
            egui::vec2(galley.size().x + 20.0, 24.0),
        );
        left = segment.right() + 2.0;
        let response = ui
            .interact(
                segment,
                ui.make_persistent_id((id, *value)),
                egui::Sense::click(),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, on, *label)
        });
        if on {
            ui.painter().rect_filled(segment, radius, palette.surface);
        }
        let ink = if on {
            palette.ink_strong
        } else {
            palette.muted
        };
        ui.painter()
            .galley(segment.center() - galley.size() / 2.0, galley, ink);
        if response.clicked() {
            picked = Some(*value);
        }
    }
    picked
}

/// The four colors an icon button paints with: its fill and its icon,
/// at rest and under the pointer.
#[derive(Debug, Clone, Copy)]
pub struct IconTones {
    /// The fill at rest, transparent for a ghost button.
    pub fill: egui::Color32,
    /// The fill under the pointer.
    pub hover_fill: egui::Color32,
    /// The icon at rest.
    pub ink: egui::Color32,
    /// The icon under the pointer.
    pub hover_ink: egui::Color32,
}

/// A `size` square button holding `icon` at `icon_size`, rounded by
/// `radius`, painted in `tones`, named `label` for assistive tech.
/// Drawn at the cursor.
pub fn icon_btn(
    ui: &mut egui::Ui,
    label: &str,
    icon: crate::icons::Icon,
    size: f32,
    icon_size: f32,
    radius: f32,
    tones: IconTones,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let hovered = response.hovered();
    let fill = if hovered {
        tones.hover_fill
    } else {
        tones.fill
    };
    let radius = egui::CornerRadius::same(radius.min(size / 2.0) as u8);
    ui.painter().rect_filled(rect, radius, fill);
    let ink = if hovered { tones.hover_ink } else { tones.ink };
    crate::icons::themed_image(icon, icon_size, ink).paint_snapped(
        ui,
        egui::Rect::from_center_size(rect.center(), egui::vec2(icon_size, icon_size)),
    );
    response
}

/// A small square button holding one icon, drawn at the cursor.
pub fn icon_button(
    ui: &mut egui::Ui,
    id: &str,
    icon: crate::icons::Icon,
    palette: &Palette,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(R_MD as u8),
            palette.fill_hover,
        );
    }
    let icon_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(13.0, 13.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(icon_rect), |ui| {
        crate::icons::icon(ui, icon, 13.0, palette.faint);
    });
    let _ = id;
    response
}

/// A form's small button: 26px tall, in one of three tones, as the
/// settings pages draw theirs.
pub fn button(ui: &mut egui::Ui, label: &str, tone: Tone, palette: &Palette) -> egui::Response {
    let tone = match tone {
        Tone::Primary => crate::ui::settings_config::BtnTone::Primary,
        Tone::Danger => crate::ui::settings_config::BtnTone::Danger,
        Tone::Plain => crate::ui::settings_config::BtnTone::Plain,
    };
    crate::ui::settings_config::btn_sm(ui, label, None, tone, palette)
}

/// The three tones a form's small button carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// The filled accent button, for the main action.
    Primary,
    /// The outlined danger button.
    Danger,
    /// The plain hairline button.
    Plain,
}

/// A pill button with a dashed border, such as the forms' Add row.
pub fn add_button(
    ui: &mut egui::Ui,
    label: &str,
    icon: crate::icons::Icon,
    palette: &Palette,
) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::new(FS_XS, family(FAMILY_BODY_REGULAR)),
        palette.muted,
    );
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(8.0 + 12.0 + 4.0 + galley.size().x + 8.0, 24.0),
        egui::Sense::click(),
    );
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    crate::ui::transcript::text_rows::dashed_rect(ui, rect, 12.0, palette.line);
    crate::icons::themed_image(icon, 12.0, palette.muted).paint_snapped(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(rect.left() + 8.0, rect.center().y - 6.0),
            egui::vec2(12.0, 12.0),
        ),
    );
    let h = galley.size().y;
    ui.painter().galley(
        egui::pos2(rect.left() + 24.0, rect.center().y - h / 2.0),
        galley,
        palette.muted,
    );
    response
}

/// A switch in one of two states, drawn at the cursor.
pub fn switch(ui: &mut egui::Ui, on: bool, palette: &Palette) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(36.0, 20.0), egui::Sense::click());
    ui.painter().rect_filled(
        rect,
        egui::CornerRadius::same(R_FULL as u8),
        if on {
            palette.accent
        } else {
            palette.fill_hover
        },
    );
    let knob = egui::pos2(
        if on {
            rect.right() - 10.0
        } else {
            rect.left() + 10.0
        },
        rect.center().y,
    );
    ui.painter().circle_filled(knob, 8.0, egui::Color32::WHITE);
    response
}

/// A one-line text field, drawn full width inside the current column.
pub fn text_field(
    ui: &mut egui::Ui,
    value: &mut String,
    placeholder: &str,
    mono: bool,
    palette: &Palette,
) -> egui::Response {
    text_field_w(ui, value, placeholder, mono, f32::INFINITY, palette)
}

/// A one-line text field at an explicit width; [`f32::INFINITY`] takes
/// the column's remaining width.
pub fn text_field_w(
    ui: &mut egui::Ui,
    value: &mut String,
    placeholder: &str,
    mono: bool,
    width: f32,
    palette: &Palette,
) -> egui::Response {
    add_field(
        ui,
        one_line(value, placeholder, mono, width, palette),
        palette,
    )
}

/// A one-line field for a secret, full width: what it holds draws as
/// dots, never in the clear.
pub fn secret_field(
    ui: &mut egui::Ui,
    value: &mut String,
    placeholder: &str,
    palette: &Palette,
) -> egui::Response {
    let field = one_line(value, placeholder, false, f32::INFINITY, palette).password(true);
    add_field(ui, field, palette)
}

/// The one-line text edit the form fields share.
fn one_line<'t>(
    value: &'t mut String,
    placeholder: &str,
    mono: bool,
    width: f32,
    palette: &Palette,
) -> egui::TextEdit<'t> {
    let mut font = egui::FontId::new(FS_SM, family(FAMILY_BODY_REGULAR));
    if mono {
        font = egui::FontId::new(FS_XS, family(FAMILY_MONO_REGULAR));
    }
    egui::TextEdit::singleline(value)
        .hint_text(
            egui::RichText::new(placeholder)
                .size(FS_XS)
                .color(palette.faint),
        )
        .font(font)
        .desired_width(width)
        .margin(egui::Margin::symmetric(8, 6))
}

/// A multi-line text field, one entry per line, drawn full width.
pub fn lines_field(
    ui: &mut egui::Ui,
    value: &mut String,
    placeholder: &str,
    palette: &Palette,
) -> egui::Response {
    let field = egui::TextEdit::multiline(value)
        .hint_text(
            egui::RichText::new(placeholder)
                .size(FS_XS)
                .color(palette.faint),
        )
        .font(egui::FontId::new(FS_XS, family(FAMILY_MONO_REGULAR)))
        .desired_width(f32::INFINITY)
        .desired_rows(3)
        .margin(egui::Margin::symmetric(8, 6));
    add_field(ui, field, palette)
}

/// Adds `field` with the reference input's focus look: the focus
/// border where egui would stroke a focused field in the strong ink,
/// and the accent ring around it.
pub(crate) fn add_field(
    ui: &mut egui::Ui,
    field: egui::TextEdit<'_>,
    palette: &Palette,
) -> egui::Response {
    let response = ui
        .scope(|ui| {
            ui.visuals_mut().selection.stroke = egui::Stroke::new(1.0, palette.composer_focus_line);
            ui.add(field)
        })
        .inner;
    if response.has_focus() {
        focus_ring(ui, response.rect, palette);
    }
    response
}

/// The accent at half strength just outside a focused field's border,
/// taking no room of its own.
fn focus_ring(ui: &egui::Ui, rect: egui::Rect, palette: &Palette) {
    ui.painter().rect_stroke(
        rect,
        egui::CornerRadius::same(R_MD as u8),
        egui::Stroke::new(3.0, palette.accent.gamma_multiply(0.5)),
        egui::StrokeKind::Outside,
    );
}

/// The header of a form page: a back button and the title on one line.
/// The back button's response is returned for the caller to read.
pub fn form_head(
    ui: &mut egui::Ui,
    title: &str,
    palette: &Palette,
) -> (egui::Response, egui::Response) {
    let (back, title_response) = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let back = icon_button(ui, "form-back", crate::icons::Icon::ChevronLeft, palette);
            let title = ui.add(
                egui::Label::new(
                    egui::RichText::new(title)
                        .size(FS_SM + 4.0)
                        .family(family(FAMILY_BODY_SEMIBOLD))
                        .color(palette.ink_strong),
                )
                .selectable(false),
            );
            (back, title)
        })
        .inner;
    ui.add_space(14.0);
    (back, title_response)
}

/// The TOML a save would write at `path`, as a preview block. `None`
/// while the form cannot save.
#[must_use]
pub fn toml_preview(path: &[String], value: &serde_json::Value) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let mut out = String::new();
    out.push('[');
    let mut keys = path.iter().peekable();
    while let Some(key) = keys.next() {
        out.push_str(&toml_key(key));
        if keys.peek().is_some() {
            out.push('.');
        }
    }
    out.push_str("]\n");
    let table = value.as_object()?;
    for (key, item) in table {
        match item {
            serde_json::Value::Array(items) => {
                for entry in items {
                    let path = path.iter().map(|key| toml_key(key)).collect::<Vec<_>>();
                    out.push_str(&format!("[[{}.{}]]\n", path.join("."), toml_key(key)));
                    if let Some(entry) = entry.as_object() {
                        for (name, item) in entry {
                            out.push_str(&format!("{name} = {}\n", toml_of(item)));
                        }
                    }
                }
            }
            serde_json::Value::Object(inner) => {
                for (name, item) in inner {
                    out.push_str(&format!("{key}.{name} = {}\n", toml_of(item)));
                }
            }
            other => out.push_str(&format!("{key} = {}\n", toml_of(other))),
        }
    }
    Some(out)
}

/// One table key as TOML, quoted where a bare key would not parse.
fn toml_key(key: &str) -> String {
    let bare = !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if bare {
        key.to_owned()
    } else {
        format!("\"{key}\"")
    }
}

/// One value as TOML.
fn toml_of(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => format!("\"{text}\""),
        serde_json::Value::Bool(on) => on.to_string(),
        serde_json::Value::Number(number) => number.to_string(),
        other => other.to_string(),
    }
}

/// A collapsible block showing the TOML `preview` holds, with its
/// toggle's response returned for the caller to read.
pub fn preview_block(
    ui: &mut egui::Ui,
    open: bool,
    preview: Option<String>,
    palette: &Palette,
) -> egui::Response {
    let Some(preview) = preview else {
        let (_, response) = ui.allocate_exact_size(egui::Vec2::ZERO, egui::Sense::hover());
        return response;
    };
    let response = ui.add(
        egui::Button::new(
            egui::RichText::new(if open {
                crate::i18n::t("Hide the config.toml entry")
            } else {
                crate::i18n::t("Show the config.toml entry")
            })
            .size(FS_XS)
            .color(palette.muted),
        )
        .fill(egui::Color32::TRANSPARENT)
        .stroke(egui::Stroke::NONE),
    );
    if open {
        egui::Frame::NONE
            .fill(palette.deep)
            .corner_radius(egui::CornerRadius::same(R_MD as u8))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut preview.to_owned())
                        .font(egui::FontId::new(FS_XS, family(FAMILY_MONO_REGULAR)))
                        .desired_width(f32::INFINITY)
                        .interactive(false),
                );
            });
    }
    response
}

/// Width of the fixed name cell of a [`pairs`] row.
const PAIR_NAME_W: f32 = 160.0;
/// Width of the remove button of a [`pairs`] row: [`icon_button`]'s cell.
pub(crate) const ROW_REMOVE_W: f32 = 26.0;

/// The name and value rows of a map a form edits, then an add button.
/// The rows report their edits through `edit` and the add button's
/// response is returned for the caller to read.
pub fn pairs(ui: &mut egui::Ui, id: &str, pairs: &mut Pairs, palette: &Palette) -> egui::Response {
    for ix in 0..pairs.rows().len() {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            // The value cell fills what the name cell and the remove
            // button leave, so the row never grows past its column.
            let value_w =
                (ui.available_width() - PAIR_NAME_W - ROW_REMOVE_W - 2.0 * 6.0).max(120.0);
            let mut name = pairs.rows()[ix].name.clone();
            let mut value = pairs.rows()[ix].value.clone();
            let name_response = text_field_w(
                ui,
                &mut name,
                crate::i18n::t("NAME"),
                true,
                PAIR_NAME_W,
                palette,
            );
            let value_placeholder = if pairs.rows()[ix].kept {
                crate::i18n::t("unchanged")
            } else {
                crate::i18n::t("value")
            };
            let value_response =
                text_field_w(ui, &mut value, value_placeholder, false, value_w, palette);
            if name_response.changed() {
                pairs.set_name(ix, name);
            }
            if value_response.changed() {
                pairs.set_value(ix, value);
            }
            let remove = icon_button(
                ui,
                &format!("{id}-rm-{ix}"),
                crate::icons::Icon::Trash,
                palette,
            );
            if remove.clicked() {
                pairs.remove(ix);
            }
        });
    }
    let add = add_button(ui, crate::i18n::t("Add"), crate::icons::Icon::Plus, palette);
    if add.clicked() {
        pairs.push();
    }
    add
}

/// The height of the reference's text input.
pub(crate) const FIELD_H: f32 = 28.0;

/// The reference's text input: no box of its own until it holds the
/// focus, then the focus border with the accent ring around it.
/// `focus` asks for the focus this frame.
pub(crate) fn input_field(
    ui: &mut egui::Ui,
    text: &mut String,
    hint: &str,
    focus: bool,
    palette: &Palette,
) -> egui::Response {
    let face = crate::theme::prose_family(ui.ctx());
    // The reference pads 10px inside a 1px border, and sets the line a
    // whole pixel from the top; centering here would round the line's
    // offset and its baseline apart, a pixel lower.
    let above = ((FIELD_H - crate::theme::line_h(FS_SM)) / 2.0) as i8;
    let response = ui.add(
        egui::TextEdit::singleline(text)
            .hint_text(
                egui::RichText::new(hint)
                    .font(egui::FontId::new(FS_SM, face.clone()))
                    .color(palette.faint),
            )
            .frame(egui::Frame::NONE.inner_margin(egui::Margin {
                left: 11,
                right: 11,
                top: above,
                bottom: 0,
            }))
            .min_size(egui::vec2(ui.available_width(), FIELD_H))
            .desired_width(f32::INFINITY)
            .vertical_align(egui::Align::TOP)
            .text_color(palette.ink)
            .font(egui::FontId::new(FS_SM, face)),
    );
    if focus {
        response.request_focus();
    }
    if response.has_focus() {
        focus_ring(ui, response.rect, palette);
        ui.painter().rect_stroke(
            response.rect,
            egui::CornerRadius::same(R_MD as u8),
            egui::Stroke::new(1.0, palette.composer_focus_line),
            egui::StrokeKind::Inside,
        );
    }
    response
}

/// Showing a stroked frame with its border painted on its own, crisp on
/// the pixel grid, as the reference draws borders. egui paints a fill
/// and an inside stroke as one shape, which softens the border into
/// what is around it.
pub(crate) trait ShowCrisp {
    /// Lays the frame out as [`egui::Frame::show`] does, then paints
    /// its border over the content.
    fn show_crisp<R>(
        self,
        ui: &mut egui::Ui,
        add: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R>;

    /// [`ShowCrisp::show_crisp`] over `layers` of drop shadow, painted
    /// under the frame as the reference paints a box's shadow.
    fn show_shadowed<R>(
        self,
        ui: &mut egui::Ui,
        layers: &[crate::theme::ShadowLayer],
        add: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R>;
}

impl ShowCrisp for egui::Frame {
    fn show_crisp<R>(
        self,
        ui: &mut egui::Ui,
        add: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R> {
        let stroke = self.stroke;
        let width = stroke.width.round() as i8;
        let mut frame = self;
        frame.stroke = egui::Stroke::NONE;
        frame.inner_margin = egui::Margin {
            left: frame.inner_margin.left + width,
            right: frame.inner_margin.right + width,
            top: frame.inner_margin.top + width,
            bottom: frame.inner_margin.bottom + width,
        };
        let mut prepared = frame.begin(ui);
        let inner = add(&mut prepared.content_ui);
        let edge = frame.widget_rect(prepared.content_ui.min_rect());
        let response = prepared.end(ui);
        if !stroke.is_empty() {
            ui.painter()
                .rect_stroke(edge, frame.corner_radius, stroke, egui::StrokeKind::Inside);
        }
        egui::InnerResponse::new(inner, response)
    }

    fn show_shadowed<R>(
        self,
        ui: &mut egui::Ui,
        layers: &[crate::theme::ShadowLayer],
        add: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R> {
        let at = ui.painter().add(egui::Shape::Noop);
        let inner = self.show_crisp(ui, add);
        let rect = inner.response.rect - self.outer_margin;
        ui.painter()
            .set(at, crate::shapes::shadow(rect, self.corner_radius, layers));
        inner
    }
}

#[cfg(test)]
mod tests {
    use super::{Pairs, Saving, Tone};

    fn map(pairs: &[(&str, &str)]) -> std::collections::BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn the_rows_carry_the_map_as_the_snapshot_shows_it() {
        let pairs = Pairs::new(&map(&[("X-Org", "acme"), ("X-Key", "<redacted>")]));
        let rows = pairs.rows();
        assert_eq!(rows.len(), 2);
        // A BTreeMap orders the rows, so the key sorts first.
        assert_eq!(rows[0].name, "X-Key");
        assert_eq!(rows[0].value, "");
        assert!(rows[0].kept, "a redacted value starts blank and kept");
        assert_eq!(rows[1].name, "X-Org");
        assert_eq!(rows[1].value, "acme");
        assert!(!rows[1].kept);
    }

    #[test]
    fn a_row_without_a_name_drops_out_of_the_values() {
        let mut pairs = Pairs::default();
        pairs.push();
        pairs.set_name(0, "  ".to_owned());
        pairs.set_value(0, "x".to_owned());
        pairs.push();
        pairs.set_name(1, "X-Org".to_owned());
        pairs.set_value(1, "acme".to_owned());
        assert_eq!(
            pairs.values(),
            map(&[("X-Org", "acme")]),
            "the unnamed row drops out"
        );
    }

    #[test]
    fn a_kept_row_left_blank_sends_the_redaction_marker() {
        let mut pairs = Pairs::new(&map(&[("X-Key", "<redacted>")]));
        assert_eq!(pairs.values(), map(&[("X-Key", "<redacted>")]));
        pairs.set_value(0, "new".to_owned());
        assert_eq!(pairs.values(), map(&[("X-Key", "new")]));
    }

    #[test]
    fn adding_and_removing_rows_moves_the_list() {
        let mut pairs = Pairs::new(&map(&[("A", "1"), ("B", "2")]));
        pairs.push();
        pairs.set_name(2, "C".to_owned());
        pairs.remove(0);
        let rows = pairs.rows();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "B");
        assert_eq!(rows[1].name, "C");
    }

    #[test]
    fn an_idle_saving_has_no_line_and_is_not_busy() {
        let saving = Saving::Idle;
        assert_eq!(saving.line(&crate::theme::Palette::shadow()), None);
        assert!(!saving.busy());
    }

    #[test]
    fn a_saving_in_flight_reports_it() {
        let saving = Saving::Sent(vec![1, 2]);
        assert!(saving.busy());
        let (line, _) = saving
            .line(&crate::theme::Palette::shadow())
            .expect("a line");
        assert_eq!(line, "Saving\u{2026}");
    }

    #[test]
    fn a_refusal_names_why() {
        let saving = Saving::Refused("the engine said no".to_owned());
        let (line, _) = saving
            .line(&crate::theme::Palette::shadow())
            .expect("a line");
        assert_eq!(line, "the engine said no");
    }

    #[test]
    fn a_settled_saving_closes_once_every_write_landed() {
        let mut store = crate::store::Store::new("/w", false);
        let mut saving = Saving::Sent(vec![1]);
        assert!(!saving.settle(&store), "still in flight");
        let _ = store.config_set(&["x"], None);
        // The write has no answer yet, so it stays in flight.
        assert!(!saving.settle(&store));
        let mut saving = Saving::Idle;
        assert!(!saving.settle(&store), "nothing was sent");
    }

    #[test]
    fn the_three_tones_are_distinct() {
        assert_ne!(Tone::Primary, Tone::Danger);
        assert_ne!(Tone::Danger, Tone::Plain);
    }
}

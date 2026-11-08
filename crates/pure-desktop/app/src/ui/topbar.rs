//! The chat column's top bar: the project and session crumbs, the fork
//! and working pills, and the panel toggles at the far end.

use crate::icons::PaintSnapped as _;
use crate::ui::forms::ShowCrisp as _;

use crate::icons::Icon;
use crate::store::Store;
use crate::theme::{FS_SM, FS_XS, PANEL_HEAD_H, Palette, R_LG, R_MD, R_SM, SP_4};
use crate::ui::sidebar::project_name;

/// What the top bar asked for; the shell carries it out after its
/// panels are drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TopbarEvent {
    ShowSidebar,
    ToggleWorkbench,
    Rename(String),
}

/// The session actions of a sidebar row's menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionAction {
    Pin,
    Fork,
    Compact,
    Export,
    CopyId,
    Close,
    Archive,
}

const RENAME_DRAFT: &str = "topbar-rename-draft";
const RENAME_INPUT: &str = "topbar-rename-input";
const RENAME_FOCUS: &str = "topbar-rename-focus";

pub(crate) fn begin_rename(ctx: &egui::Context, title: String) {
    // A fresh state puts the caret at the title's end, as the
    // reference's input does, not where a past rename left it.
    egui::text_edit::TextEditState::default().store(ctx, egui::Id::new(RENAME_INPUT));
    ctx.data_mut(|data| {
        data.insert_temp(egui::Id::new(RENAME_DRAFT), title);
        data.insert_temp(egui::Id::new(RENAME_FOCUS), true);
    });
}

/// Draws the top bar over the chat column. `sidebar_hidden` shows the
/// control that brings the sidebar back. The empty stretch between the
/// crumbs and the controls drags the frameless window.
pub fn show(
    ui: &mut egui::Ui,
    store: &Store,
    sidebar_hidden: bool,
    workbench_visible: bool,
    controls_here: bool,
    events: &mut Vec<TopbarEvent>,
) {
    let palette = Palette::active_of(ui.ctx());
    let (bar, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), PANEL_HEAD_H),
        egui::Sense::hover(),
    );
    let inner = egui::Rect::from_min_max(
        bar.min + egui::vec2(14.0, 0.0),
        bar.max - egui::vec2(10.0, 0.0),
    );
    // Where the drawn content stops on each side, so the drag strip
    // covers only the gap between them.
    let mut left_end = inner.left();
    let mut right_start = inner.right();
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            ui.spacing_mut().item_spacing.x = SP_4;
            if sidebar_hidden
                && ghost(
                    ui,
                    Icon::PanelLeft,
                    false,
                    crate::i18n::t("Show sidebar (Ctrl \\)"),
                    palette,
                )
                .clicked()
            {
                events.push(TopbarEvent::ShowSidebar);
            }
            if let Some(session) = store.active_session() {
                ui.scope(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    crumb(
                        ui,
                        &project_name(session.cwd.as_deref()),
                        palette.faint,
                        egui::Sense::hover(),
                        PROJECT_CRUMB_W,
                    );
                    let title = store.display_title(&session.id);
                    rename_or_title(ui, palette, title, events);
                });
                if store.fork_parent(&session.id).is_some() {
                    pill(ui, palette, |ui| {
                        ui.label(
                            egui::RichText::new(crate::i18n::t("fork"))
                                .size(FS_XS)
                                .color(palette.muted),
                        );
                    });
                }
            }
            left_end = ui.min_rect().right();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = SP_4;
                // A wide workbench draws the caption buttons at the
                // window's corner; otherwise the bar keeps them here.
                if controls_here {
                    if let Some(action) = crate::ui::chrome::controls(ui, palette) {
                        crate::ui::chrome::run(ui.ctx(), action);
                    }
                    ui.add_space(6.0);
                }
                if ghost(
                    ui,
                    Icon::PanelRight,
                    workbench_visible,
                    crate::i18n::t("Workbench (Ctrl B)"),
                    palette,
                )
                .clicked()
                {
                    events.push(TopbarEvent::ToggleWorkbench);
                }
                if let Some(session) = store.active_session() {
                    // The working pill at the bar's end, with how long
                    // the run has gone, as the reference shows it.
                    if session.in_turn || session.running || session.compacting {
                        let took = store
                            .timings(&session.id)
                            .and_then(|times| times.running_for())
                            .unwrap_or_default();
                        working_pill(
                            ui,
                            session.compacting,
                            &crate::clock::duration(took.as_secs() as i64),
                            palette,
                        );
                    }
                }
                right_start = ui.min_rect().left();
            });
        },
    );
    crate::ui::chrome::drag_surface(
        ui,
        egui::Rect::from_min_max(
            egui::pos2(left_end + SP_4, bar.top()),
            egui::pos2(right_start - SP_4, bar.bottom()),
        ),
    );
}

/// The working pill: the working cat and the elapsed label on the
/// fill, 24px tall. `compacting` renames the run to what it is doing
/// while the agent summarizes older turns.
fn working_pill(ui: &mut egui::Ui, compacting: bool, elapsed: &str, palette: &Palette) {
    let verb = if compacting {
        crate::i18n::t("Compacting")
    } else {
        crate::i18n::t("Working")
    };
    let label = format!("{verb} {elapsed}");
    let galley = ui.painter().layout_no_wrap(
        label.clone(),
        egui::FontId::new(
            FS_XS,
            crate::theme::family(crate::theme::FAMILY_BODY_REGULAR),
        ),
        palette.muted,
    );
    let size = egui::vec2(9.0 + 14.0 + 6.0 + galley.size().x + 9.0, 24.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &label));
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(u8::MAX), palette.fill);
    let mark = egui::Rect::from_min_size(
        egui::pos2(rect.left() + 9.0, rect.center().y - 7.0),
        egui::vec2(14.0, 14.0),
    );
    crate::ui::mark::working(ui, mark, crate::clock::epoch());
    ui.painter().galley(
        egui::pos2(
            mark.right() + 6.0,
            crate::theme::line_y(rect.center().y, 0.0, &galley),
        ),
        galley,
        palette.muted,
    );
}

/// A rounded fill holding a small row of content.
fn pill(ui: &mut egui::Ui, palette: &Palette, content: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::NONE
        .fill(palette.fill)
        .corner_radius(egui::CornerRadius::same(u8::MAX))
        .inner_margin(egui::Margin::symmetric(9, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                content(ui);
            });
        });
}

/// The session title as the reference's title pill: the name on a
/// rounded fill, as wide as the name, cut with an ellipsis when the bar
/// runs out. A double click opens the rename field.
fn title_pill(ui: &mut egui::Ui, palette: &Palette, title: String) {
    let galley = crate::ui::transcript::head::one_line(
        ui,
        title.clone(),
        egui::TextFormat::simple(
            egui::FontId::new(
                FS_SM,
                crate::theme::family(crate::theme::FAMILY_BODY_MEDIUM),
            ),
            palette.ink_strong,
        ),
        TITLE_PILL_MAX_W,
    );
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(galley.size().x.ceil() + 28.0, TITLE_PILL_H),
        egui::Sense::click(),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &title));
    let fill = if response.hovered() {
        palette.fill_hover
    } else {
        palette.fill
    };
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(u8::MAX), fill);
    let y = crate::theme::snap(rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(
        egui::pos2(rect.center().x - galley.size().x / 2.0, y),
        galley,
        palette.ink_strong,
    );
    if response
        .on_hover_text(crate::i18n::t("Double-click to rename"))
        .double_clicked()
    {
        begin_rename(ui.ctx(), title);
    }
}

/// One crumb of the topbar: its text on the reference's 13px line, as
/// wide as the text rounded up, cut with an ellipsis at `max_w`.
fn crumb(
    ui: &mut egui::Ui,
    text: &str,
    color: egui::Color32,
    sense: egui::Sense,
    max_w: f32,
) -> egui::Response {
    let line = crate::theme::line_h(FS_SM);
    let galley = crate::ui::transcript::head::one_line(
        ui,
        text.to_owned(),
        egui::TextFormat::simple(
            egui::FontId::new(
                FS_SM,
                crate::theme::family(crate::theme::FAMILY_BODY_REGULAR),
            ),
            color,
        ),
        max_w.min(ui.available_width()),
    );
    let (rect, response) = ui.allocate_exact_size(egui::vec2(galley.size().x.ceil(), line), sense);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, text));
    let y = crate::theme::line_y(crate::theme::snap(rect.top()), line, &galley);
    ui.painter()
        .galley(egui::pos2(rect.left(), y), galley, color);
    response
}

/// The session title, which a double click or the menu turns into an
/// inline field. Enter or leaving the field names the session; Escape
/// drops the edit.
fn rename_or_title(
    ui: &mut egui::Ui,
    palette: &Palette,
    title: String,
    events: &mut Vec<TopbarEvent>,
) {
    let draft_id = egui::Id::new(RENAME_DRAFT);
    let Some(mut draft) = ui.data(|data| data.get_temp::<String>(draft_id)) else {
        title_pill(ui, palette, title);
        return;
    };
    // The reference's small input: 260px by 26, its text 12px in, the
    // accent ring around it while it holds the focus.
    let edit = ui.add(
        egui::TextEdit::singleline(&mut draft)
            .id(egui::Id::new(RENAME_INPUT))
            .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(12, 0)))
            .min_size(egui::vec2(RENAME_W, RENAME_H))
            .desired_width(RENAME_W - 24.0)
            .vertical_align(egui::Align::Center)
            .text_color(palette.ink_strong)
            .font(egui::FontId::new(
                FS_SM,
                crate::theme::family(crate::theme::FAMILY_BODY_REGULAR),
            )),
    );
    // The focus lands the frame the field first draws: asked for
    // before, it names a widget the accessibility tree does not hold.
    if ui
        .data_mut(|data| data.remove_temp::<bool>(egui::Id::new(RENAME_FOCUS)))
        .unwrap_or(false)
    {
        edit.request_focus();
    }
    ui.painter().rect_stroke(
        edit.rect,
        egui::CornerRadius::same(R_LG as u8),
        egui::Stroke::new(2.0, palette.accent_bd),
        egui::StrokeKind::Outside,
    );
    let (enter, escape) = ui.input(|input| {
        (
            input.key_pressed(egui::Key::Enter),
            input.key_pressed(egui::Key::Escape),
        )
    });
    if escape {
        ui.data_mut(|data| data.remove::<String>(draft_id));
    } else if enter || edit.lost_focus() {
        ui.data_mut(|data| data.remove::<String>(draft_id));
        events.push(TopbarEvent::Rename(draft));
    } else {
        ui.data_mut(|data| data.insert_temp(draft_id, draft));
    }
}

/// What the session action menu picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionMenuChoice {
    /// Turn the session title into an inline field.
    Rename,
    /// One of the menu's actions.
    Action(SessionAction),
}

/// Whether the menu stored under `open_id` shows now.
pub(crate) fn menu_open(ui: &egui::Ui, open_id: egui::Id) -> bool {
    ui.data(|data| data.get_temp::<bool>(open_id).unwrap_or(false))
}

/// Draws the session actions popup under `button`, the menu the top bar
/// and the sidebar rows share. A click on the button toggles it; the
/// choice it returns closes it.
pub(crate) fn session_menu(
    ui: &mut egui::Ui,
    button: &egui::Response,
    open_id: egui::Id,
    pinned: bool,
    running: bool,
    palette: &Palette,
) -> Option<SessionMenuChoice> {
    let mut open = menu_open(ui, open_id) ^ button.clicked();
    if !open {
        ui.data_mut(|data| data.insert_temp(open_id, false));
        return None;
    }
    let (pin_label, pin_icon) = if pinned {
        (crate::i18n::t("Unpin"), Icon::PinOff)
    } else {
        (crate::i18n::t("Pin"), Icon::Pin)
    };
    let items: [Option<MenuItem>; 9] = [
        Some((Icon::Pencil, crate::i18n::t("Rename"), true, None)),
        Some((pin_icon, pin_label, true, Some(SessionAction::Pin))),
        Some((
            Icon::GitFork,
            crate::i18n::t("Fork session"),
            true,
            Some(SessionAction::Fork),
        )),
        Some((
            Icon::Layers,
            crate::i18n::t("Compact now"),
            !running,
            Some(SessionAction::Compact),
        )),
        Some((
            Icon::ExternalLink,
            crate::i18n::t("Export markdown"),
            true,
            Some(SessionAction::Export),
        )),
        Some((
            Icon::Copy,
            crate::i18n::t("Copy session ID"),
            true,
            Some(SessionAction::CopyId),
        )),
        None,
        Some((
            Icon::X,
            crate::i18n::t("Close session"),
            true,
            Some(SessionAction::Close),
        )),
        Some((
            Icon::Archive,
            crate::i18n::t("Archive"),
            true,
            Some(SessionAction::Archive),
        )),
    ];
    let mut choice = None;
    let area = egui::Area::new(egui::Id::new("session-menu"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::RIGHT_TOP)
        .fixed_pos(button.rect.right_bottom() + egui::vec2(0.0, MENU_GAP))
        .show(ui.ctx(), |ui| {
            egui::Frame::NONE
                .fill(palette.menu)
                .stroke(egui::Stroke::new(1.0, palette.line))
                .corner_radius(egui::CornerRadius::same(R_MD as u8))
                .inner_margin(egui::Margin::symmetric(0, MENU_PAD_Y as i8))
                .show_shadowed(ui, &palette.shadow_menu, |ui| {
                    ui.set_width(MENU_W);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for item in items {
                        let Some((icon, label, enabled, action)) = item else {
                            ui.add_space(4.0);
                            let (rule, _) = ui
                                .allocate_exact_size(egui::vec2(MENU_W, 1.0), egui::Sense::hover());
                            ui.painter()
                                .rect_filled(rule, egui::CornerRadius::ZERO, palette.line);
                            ui.add_space(3.0);
                            continue;
                        };
                        if menu_row(ui, icon, label, enabled, palette).clicked() {
                            open = false;
                            choice = Some(match action {
                                Some(action) => SessionMenuChoice::Action(action),
                                None => SessionMenuChoice::Rename,
                            });
                        }
                    }
                });
        });
    let outside = ui.input(|i| i.pointer.any_pressed())
        && ui
            .input(|i| i.pointer.interact_pos())
            .is_some_and(|at| !area.response.rect.contains(at) && !button.rect.contains(at));
    if outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        open = false;
    }
    ui.data_mut(|data| data.insert_temp(open_id, open));
    choice
}

/// One session menu row: its icon, label, whether it is enabled, and
/// the action it asks for, or `None` for rename, which the bar does
/// itself.
type MenuItem<'a> = (Icon, &'a str, bool, Option<SessionAction>);

/// The rename field's width, the reference's 260px floor.
const RENAME_W: f32 = 260.0;
/// The rename field's height, the reference's small input.
const RENAME_H: f32 = 26.0;
/// The title pill's height.
const TITLE_PILL_H: f32 = 30.0;
/// The widest the title pill's text draws before the ellipsis.
const TITLE_PILL_MAX_W: f32 = 340.0;
/// The project crumb's width cap, so a deep path leaves the title room.
const PROJECT_CRUMB_W: f32 = 160.0;

/// The session menu's width, the reference's 220px floor.
const MENU_W: f32 = 220.0;
/// The space between the ellipsis button and the menu under it.
const MENU_GAP: f32 = 4.0;
/// The menu's padding above its first row and under its last.
const MENU_PAD_Y: f32 = 2.0;
/// One menu row's height.
const MENU_ROW_H: f32 = 28.0;

/// One menu row: the small icon and the label, the hover fill inset.
fn menu_row(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    enabled: bool,
    palette: &Palette,
) -> egui::Response {
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(MENU_W, MENU_ROW_H), sense);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    if enabled && response.hovered() {
        ui.painter().rect_filled(
            rect.shrink2(egui::vec2(4.0, 0.0)),
            egui::CornerRadius::same(R_SM as u8),
            palette.hover,
        );
    }
    let ink = if enabled { palette.ink } else { palette.faint };
    let mid = rect.center().y;
    crate::icons::themed_image(icon, 12.0, ink).paint_snapped(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(rect.left() + 12.0, mid - 6.0),
            egui::vec2(12.0, 12.0),
        ),
    );
    // The reference's 4px gap after the 12px icon.
    ui.painter().text(
        egui::pos2(rect.left() + 12.0 + 12.0 + 4.0, mid),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::new(
            FS_SM,
            crate::theme::family(crate::theme::FAMILY_BODY_REGULAR),
        ),
        ink,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The reference's ghost icon button: 28px, the icon in the ink, filled
/// while hovered or while what it toggles is on.
fn ghost(
    ui: &mut egui::Ui,
    icon: Icon,
    selected: bool,
    tooltip: &str,
    palette: &Palette,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tooltip));
    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(R_MD as u8),
            palette.fill_hover,
        );
    }
    crate::icons::themed_image(icon, 16.0, palette.ink).paint_snapped(
        ui,
        egui::Rect::from_center_size(rect.center(), egui::vec2(16.0, 16.0)),
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(tooltip)
}

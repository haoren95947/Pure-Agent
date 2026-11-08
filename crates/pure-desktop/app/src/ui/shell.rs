//! The three-panel shell: the sidebar, the chat column and the
//! workbench, plus the narrow-width overlays.
//!
//! The sidebar takes the left column at [`SIDE_W`], the workbench the
//! right at [`WORKBENCH_W`], and the chat keeps the middle. Both
//! panels are resizable and collapsible: ctrl-\\ toggles the sidebar,
//! ctrl-b the workbench. Under the web client's breakpoints the side
//! panels stop taking a column and float over the content instead
//! (1180px for the workbench, 860px for the sidebar), which is what
//! the old shell did.

use crate::theme::{Palette, R_XL, SIDE_W, SP_4};

/// The gutter between the window's edge and the floating content cards.
const CARD_GAP: i8 = 6;

/// The default workbench width.
const WORKBENCH_W: f32 = 460.0;

/// The narrowest the chat pane gets when a side panel is dragged wider.
const CHAT_MIN_W: f32 = 360.0;

/// The narrowest the workbench drags to: room for its five tabs as
/// bare icons beside the close button.
const WORKBENCH_MIN_W: f32 = 240.0;

/// The width the workbench takes the window's caption buttons at: a
/// narrower head keeps them in the chat column's bar instead.
pub const WORKBENCH_CHROME_W: f32 = 400.0;

/// The web client's breakpoint: at or under it the workbench overlays
/// instead of taking a column.
const MID_W: f32 = 1180.0;

/// The web client's narrow breakpoint: at or under it the sidebar takes
/// no column and floats over the content when shown.
const NARROW_W: f32 = 860.0;

/// The panel layout state the shell keeps.
#[derive(Debug, Clone)]
pub struct Panels {
    /// Whether the sidebar takes its column.
    pub sidebar_visible: bool,
    /// Whether the workbench takes its column.
    pub workbench_visible: bool,
    /// Whether the sidebar floats over the content while narrow.
    pub side_float: bool,
    /// The viewport width, measured each frame. Wide until measured.
    pub viewport_width: f32,
    /// The sidebar's current width, so a re-show keeps the drag.
    pub sidebar_width: f32,
    /// The workbench's current width.
    pub workbench_width: f32,
    /// The session the shell followed when the sidebar last floated.
    followed: Option<String>,
}

impl Default for Panels {
    fn default() -> Self {
        Self {
            sidebar_visible: true,
            // Closed at first, as the design has it: the workbench is a
            // panel the user asks for with ctrl-b, and opening it by
            // default narrows the transcript on every launch.
            workbench_visible: false,
            side_float: false,
            viewport_width: f32::MAX,
            sidebar_width: SIDE_W,
            workbench_width: WORKBENCH_W,
            followed: None,
        }
    }
}

impl Panels {
    /// Measures the viewport, so the overlays follow the window.
    pub fn measure(&mut self, width: f32) {
        self.viewport_width = width;
    }

    /// Closes the floating sidebar once the followed session changes,
    /// so picking a row in it shows that session.
    pub fn follow(&mut self, active: Option<&str>) {
        if self.followed.as_deref() != active {
            self.followed = active.map(str::to_owned);
            self.side_float = false;
        }
    }

    /// Toggles the sidebar. Under the narrow breakpoint it floats over
    /// the content instead of taking a column.
    pub fn toggle_sidebar(&mut self) {
        if self.sidebar_floats() {
            self.side_float = !self.side_float;
        } else {
            self.sidebar_visible = !self.sidebar_visible;
        }
    }

    /// Toggles the workbench.
    pub fn toggle_workbench(&mut self) {
        self.workbench_visible = !self.workbench_visible;
    }

    /// Whether the workbench overlays rather than takes a column.
    #[must_use]
    pub fn workbench_overlays(&self) -> bool {
        self.viewport_width <= MID_W
    }

    /// Whether the sidebar floats over the content.
    #[must_use]
    pub fn sidebar_floats(&self) -> bool {
        self.viewport_width <= NARROW_W
    }
}

/// The divider between two columns, its own pixel past the panel.
const DIVIDER_W: f32 = 1.0;

/// Draws `body` on a child that lays out nothing in the panel itself,
/// so contents wider than the panel never resize it. Only the user's
/// drag does, and the panel's clip still bounds what the child paints.
fn contained(ui: &mut egui::Ui, body: &impl Fn(&mut egui::Ui)) {
    let room = ui.max_rect();
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(room));
    body(&mut child);
    ui.allocate_rect(room, egui::Sense::hover());
}

/// Lays out the three panels and calls `chat` for the middle column,
/// `sidebar` for the left panel's contents and `workbench` for the
/// right's. Each callback draws inside the panel's frame.
pub fn show(
    ui: &mut egui::Ui,
    panels: &mut Panels,
    sidebar: impl Fn(&mut egui::Ui),
    chat: impl FnOnce(&mut egui::Ui),
    workbench: impl Fn(&mut egui::Ui),
) {
    panels.measure(ui.available_width());
    let narrow = panels.sidebar_floats();
    let mid = panels.workbench_overlays();

    // Each panel paints its own palette token, the way the old shell
    // drew them: the sidebar the `sidebar` surface, the workbench the
    // `well`, and the chat keeps the app background.
    let palette = Palette::active_of(ui.ctx());
    // The window's ground: the sidebar surface shows through the
    // gutter around the chat card, as the reference lays its surfaces.
    ui.painter()
        .rect_filled(ui.max_rect(), egui::CornerRadius::ZERO, palette.sidebar);
    // Neither side panel may take the chat's minimum, however wide its
    // contents ask to be, or the chat collapses and the panel runs
    // off the window.
    let width = ui.available_width();
    let docked = panels.workbench_visible && !mid;
    let side_min = CHAT_MIN_W.min(SIDE_W);
    let mut side_taken = 0.0;
    if panels.sidebar_visible && !narrow {
        // The reference's sidebar keeps its full width and the divider
        // takes its own pixel past it, crisp, where egui would draw a
        // half-pixel line inside the panel.
        let panel = egui::Panel::left("sidebar")
            .resizable(true)
            .show_separator_line(false)
            .default_size(panels.sidebar_width + DIVIDER_W)
            .size_range(
                side_min + DIVIDER_W
                    ..=(width - CHAT_MIN_W - if docked { WORKBENCH_MIN_W } else { 0.0 })
                        .max(side_min + DIVIDER_W),
            )
            .frame(
                egui::Frame::side_top_panel(ui.style())
                    .fill(palette.sidebar)
                    .stroke(egui::Stroke::NONE)
                    .inner_margin(egui::Margin {
                        left: SP_4 as i8,
                        right: SP_4 as i8 + DIVIDER_W as i8,
                        top: SP_4 as i8,
                        bottom: SP_4 as i8,
                    }),
            )
            .show(ui, |ui| contained(ui, &sidebar));
        let rect = panel.response.rect;
        panels.sidebar_width = (rect.width() - DIVIDER_W).max(side_min);
        side_taken = rect.width();
    }

    if docked {
        let panel = egui::Panel::right("workbench")
            .resizable(true)
            .show_separator_line(false)
            .default_size(panels.workbench_width)
            .size_range(WORKBENCH_MIN_W..=(width - side_taken - CHAT_MIN_W).max(WORKBENCH_MIN_W))
            .frame(
                egui::Frame::NONE
                    .fill(palette.bg)
                    .stroke(egui::Stroke::new(1.0, palette.line))
                    .corner_radius(egui::CornerRadius::same(R_XL as u8))
                    .outer_margin(egui::Margin::same(CARD_GAP))
                    .inner_margin(egui::Margin::ZERO),
            )
            .show(ui, |ui| contained(ui, &workbench));
        let rect = panel.response.rect;
        // Here the divider is the panel's own first pixel, as the
        // reference's handle belongs to the panel after it.
        panels.workbench_width = rect.width().max(WORKBENCH_MIN_W);
    }

    // The chat column keeps what is left, as a rounded card the
    // sidebar's surface runs around.
    egui::CentralPanel::default()
        .frame(
            egui::Frame::NONE
                .fill(palette.bg)
                .stroke(egui::Stroke::new(1.0, palette.line))
                .corner_radius(egui::CornerRadius::same(R_XL as u8))
                .outer_margin(egui::Margin::same(CARD_GAP))
                .inner_margin(egui::Margin::ZERO),
        )
        .show(ui, |ui| {
            chat(ui);
        });

    // The narrow overlays, over the chat column. Each draws into the
    // screen rect it computes itself: an area's own bounds are last
    // frame's size, so a panel laid out in them would keep a stale
    // height.
    let screen = ui.ctx().content_rect();
    if narrow && panels.side_float {
        let width = panels.sidebar_width.min(screen.width() * 0.85);
        let mut close = false;
        egui::Area::new(egui::Id::new("side-scrim"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ui.ctx(), |ui| {
                // The scrim takes the clicks the sidebar does not, so
                // nothing under it answers while the sidebar floats.
                let scrim = ui.allocate_rect(screen, egui::Sense::click());
                ui.painter().rect_filled(
                    screen,
                    0.0,
                    egui::Color32::from_black_alpha((0.3 * 255.0) as u8),
                );
                let rect =
                    egui::Rect::from_min_size(screen.min, egui::vec2(width, screen.height()));
                ui.painter().add(crate::shapes::shadow(
                    rect,
                    egui::CornerRadius::ZERO,
                    &palette.shadow_2,
                ));
                ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                    egui::Panel::left("side-float")
                        .resizable(false)
                        .exact_size(width)
                        .frame(
                            egui::Frame::NONE
                                .fill(palette.sidebar)
                                .inner_margin(SP_4 as i8),
                        )
                        .show(ui, |ui| {
                            sidebar(ui);
                        });
                });
                let on_sidebar = ui
                    .input(|i| i.pointer.interact_pos())
                    .is_some_and(|at| rect.contains(at));
                close = scrim.clicked() && !on_sidebar;
            });
        if close {
            panels.side_float = false;
        }
    }

    if mid && panels.workbench_visible {
        let width = panels.workbench_width.min(screen.width());
        let rect =
            egui::Rect::from_min_max(egui::pos2(screen.right() - width, screen.top()), screen.max);
        egui::Area::new(egui::Id::new("workbench-float"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .show(ui.ctx(), |ui| {
                ui.painter().add(crate::shapes::shadow(
                    rect,
                    egui::CornerRadius::ZERO,
                    &palette.shadow_2,
                ));
                ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                    egui::Panel::right("workbench-float-panel")
                        .resizable(false)
                        .exact_size(width)
                        .frame(egui::Frame::NONE.fill(palette.sidebar))
                        .show(ui, |ui| {
                            workbench(ui);
                        });
                });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::{CHAT_MIN_W, MID_W, NARROW_W, Panels, SIDE_W, WORKBENCH_W};

    #[test]
    fn the_defaults_match_the_design() {
        let panels = Panels::default();
        assert!(panels.sidebar_visible);
        assert!(!panels.workbench_visible, "closed until asked for");
        assert_eq!(panels.sidebar_width, SIDE_W);
        assert_eq!(panels.workbench_width, WORKBENCH_W);
    }

    #[test]
    fn the_sidebar_toggles_its_column_while_wide() {
        let mut panels = Panels::default();
        panels.measure(1200.0);
        panels.toggle_sidebar();
        assert!(!panels.sidebar_visible);
        assert!(!panels.side_float, "wide takes no float");
        panels.toggle_sidebar();
        assert!(panels.sidebar_visible);
    }

    #[test]
    fn the_sidebar_floats_while_narrow() {
        let mut panels = Panels::default();
        panels.measure(NARROW_W - 1.0);
        panels.toggle_sidebar();
        assert!(panels.sidebar_visible, "the column stays, it floats over");
        assert!(panels.side_float);
    }

    #[test]
    fn the_workbench_takes_a_column_only_while_wide() {
        let mut panels = Panels::default();
        panels.measure(MID_W + 1.0);
        panels.toggle_workbench();
        assert!(panels.workbench_visible);
        assert!(!panels.workbench_overlays());
        panels.measure(MID_W - 1.0);
        assert!(panels.workbench_overlays());
    }

    /// The design's floor sits above the sidebar's default, so a
    /// dragged-wide sidebar cannot squeeze the chat to nothing, and
    /// below both panels together.
    const _: () = {
        assert!(CHAT_MIN_W > SIDE_W);
        assert!(CHAT_MIN_W < WORKBENCH_W + SIDE_W);
    };
}

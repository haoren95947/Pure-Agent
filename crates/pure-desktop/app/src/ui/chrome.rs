//! The window's own title bar: the caption controls of the frameless
//! window, the strips that drag it, and the edges that resize it.
//!
//! The browser has no window to command, so the controls draw nothing
//! there and the drag and resize helpers find no viewport to move.

#[cfg(not(target_arch = "wasm32"))]
use crate::icons::{Icon, PaintSnapped as _};
use crate::theme::Palette;
#[cfg(not(target_arch = "wasm32"))]
use crate::theme::R_MD;

/// One caption button's width.
pub const CONTROL_W: f32 = 34.0;

/// The caption buttons' height.
pub const CONTROL_H: f32 = 28.0;

/// How close to a border a press starts a resize, in points.
const RESIZE_EDGE: f32 = 6.0;

/// What a caption button asked the window to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowAction {
    /// Collapse the window to the taskbar.
    Minimize,
    /// Maximize the window, or restore it when it is maximized.
    ToggleMaximize,
    /// Close the window.
    Close,
}

/// Whether the native window is maximized. The browser never is.
#[must_use]
pub fn maximized(ctx: &egui::Context) -> bool {
    ctx.input(|input| input.viewport().maximized.unwrap_or(false))
}

/// Carries out a caption button's ask.
pub fn run(ctx: &egui::Context, action: WindowAction) {
    let command = match action {
        WindowAction::Minimize => egui::ViewportCommand::Minimized(true),
        WindowAction::ToggleMaximize => egui::ViewportCommand::Maximized(!maximized(ctx)),
        WindowAction::Close => egui::ViewportCommand::Close,
    };
    ctx.send_viewport_cmd(command);
}

/// The three caption buttons at the end of a bar, in minimize, restore
/// and close order. Returns what one of them asked for.
#[cfg(target_arch = "wasm32")]
pub fn controls(_ui: &mut egui::Ui, _palette: &Palette) -> Option<WindowAction> {
    None
}

/// The three caption buttons at the end of a bar, in minimize, restore
/// and close order. Returns what one of them asked for.
#[cfg(not(target_arch = "wasm32"))]
pub fn controls(ui: &mut egui::Ui, palette: &Palette) -> Option<WindowAction> {
    let maximized = maximized(ui.ctx());
    let mut action = None;
    // Laid out right to left, so the close button keeps the corner
    // whatever side of the bar the callers put the cluster on.
    let width = CONTROL_W * 3.0 + 4.0;
    ui.allocate_ui_with_layout(
        egui::vec2(width, CONTROL_H),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if caption(ui, Icon::WindowClose, true, "Close", palette).clicked() {
                action = Some(WindowAction::Close);
            }
            let (icon, label): (Icon, &'static str) = if maximized {
                (Icon::WindowRestore, "Restore")
            } else {
                (Icon::WindowMaximize, "Maximize")
            };
            if caption(ui, icon, false, label, palette).clicked() {
                action = Some(WindowAction::ToggleMaximize);
            }
            if caption(ui, Icon::WindowMinimize, false, "Minimize", palette).clicked() {
                action = Some(WindowAction::Minimize);
            }
        },
    );
    action
}

/// One caption button: the small glyph, a hover fill, and the danger
/// tint the close button takes.
#[cfg(not(target_arch = "wasm32"))]
fn caption(
    ui: &mut egui::Ui,
    icon: Icon,
    close: bool,
    label: &'static str,
    palette: &Palette,
) -> egui::Response {
    let label = crate::i18n::t(label);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(CONTROL_W, CONTROL_H), egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    if response.hovered() {
        let fill = if close {
            palette.danger_soft
        } else {
            palette.fill_hover
        };
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(R_MD as u8), fill);
    }
    let ink = if close && response.hovered() {
        palette.danger
    } else {
        palette.muted
    };
    crate::icons::themed_image(icon, 12.0, ink).paint_snapped(
        ui,
        egui::Rect::from_center_size(rect.center(), egui::vec2(12.0, 12.0)),
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(label)
}

/// Makes `rect` a strip that moves the window when dragged and
/// maximizes it on a double click. Call it after the widgets the strip
/// must not steal, so their own clicks land first.
pub fn drag_surface(ui: &mut egui::Ui, rect: egui::Rect) {
    if rect.width() < 24.0 || rect.height() < 8.0 {
        return;
    }
    let response = ui.interact(
        rect,
        ui.id().with("window-drag"),
        egui::Sense::click_and_drag(),
    );
    dragging(ui.ctx(), &response);
}

/// The drag behavior of a strip that already carries its own sense.
pub fn dragging(ctx: &egui::Context, response: &egui::Response) {
    if response.drag_started_by(egui::PointerButton::Primary) {
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    } else if response.double_clicked() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized(ctx)));
    }
}

/// Resizes the frameless window from its borders: six points in, the
/// cursor names the direction, and a press starts the OS resize.
#[cfg(target_arch = "wasm32")]
pub fn resize_edges(_ctx: &egui::Context) {}

/// Resizes the frameless window from its borders: six points in, the
/// cursor names the direction, and a press starts the OS resize.
#[cfg(not(target_arch = "wasm32"))]
pub fn resize_edges(ctx: &egui::Context) {
    if maximized(ctx) {
        return;
    }
    let rect = ctx.content_rect();
    let Some(at) = ctx.input(|input| input.pointer.interact_pos()) else {
        return;
    };
    let Some((direction, cursor)) = edge_at(rect, at) else {
        return;
    };
    ctx.set_cursor_icon(cursor);
    if ctx.input(|input| input.pointer.primary_pressed()) {
        ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
    }
}

/// The resize direction and cursor for a press at `at`, or `None` when
/// the point is not on a border. Corners win over edges.
#[cfg(not(target_arch = "wasm32"))]
fn edge_at(rect: egui::Rect, at: egui::Pos2) -> Option<(egui::ResizeDirection, egui::CursorIcon)> {
    let west = at.x <= rect.left() + RESIZE_EDGE;
    let east = at.x >= rect.right() - RESIZE_EDGE;
    let north = at.y <= rect.top() + RESIZE_EDGE;
    let south = at.y >= rect.bottom() - RESIZE_EDGE;
    match (north, south, east, west) {
        (true, _, _, true) => Some((
            egui::ResizeDirection::NorthWest,
            egui::CursorIcon::ResizeNorthWest,
        )),
        (true, _, true, _) => Some((
            egui::ResizeDirection::NorthEast,
            egui::CursorIcon::ResizeNorthEast,
        )),
        (_, true, _, true) => Some((
            egui::ResizeDirection::SouthWest,
            egui::CursorIcon::ResizeSouthWest,
        )),
        (_, true, true, _) => Some((
            egui::ResizeDirection::SouthEast,
            egui::CursorIcon::ResizeSouthEast,
        )),
        (true, _, _, _) => Some((egui::ResizeDirection::North, egui::CursorIcon::ResizeNorth)),
        (_, true, _, _) => Some((egui::ResizeDirection::South, egui::CursorIcon::ResizeSouth)),
        (_, _, true, _) => Some((egui::ResizeDirection::East, egui::CursorIcon::ResizeEast)),
        (_, _, _, true) => Some((egui::ResizeDirection::West, egui::CursorIcon::ResizeWest)),
        _ => None,
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::{RESIZE_EDGE, edge_at};
    use egui::{CursorIcon, Pos2, Rect, ResizeDirection, pos2, vec2};

    fn window() -> Rect {
        Rect::from_min_size(pos2(10.0, 20.0), vec2(300.0, 200.0))
    }

    fn direction(at: Pos2) -> Option<ResizeDirection> {
        edge_at(window(), at).map(|(direction, _)| direction)
    }

    #[test]
    fn the_corners_take_their_diagonals_before_the_edges() {
        assert_eq!(
            direction(pos2(12.0, 22.0)),
            Some(ResizeDirection::NorthWest)
        );
        assert_eq!(
            direction(pos2(308.0, 218.0)),
            Some(ResizeDirection::SouthEast)
        );
        assert_eq!(
            direction(pos2(12.0, 218.0)),
            Some(ResizeDirection::SouthWest)
        );
        assert_eq!(
            direction(pos2(308.0, 22.0)),
            Some(ResizeDirection::NorthEast)
        );
    }

    #[test]
    fn the_edges_take_their_own_direction_and_the_middle_takes_none() {
        assert_eq!(direction(pos2(150.0, 21.0)), Some(ResizeDirection::North));
        assert_eq!(direction(pos2(150.0, 219.0)), Some(ResizeDirection::South));
        assert_eq!(direction(pos2(309.0, 100.0)), Some(ResizeDirection::East));
        assert_eq!(direction(pos2(11.0, 100.0)), Some(ResizeDirection::West));
        assert_eq!(direction(pos2(150.0, 100.0)), None, "inside the window");
        assert_eq!(
            direction(pos2(10.0 + RESIZE_EDGE + 0.5, 20.0 + RESIZE_EDGE + 0.5)),
            None,
            "just past the corner band"
        );
    }

    #[test]
    fn each_edge_names_a_matching_cursor() {
        assert_eq!(
            edge_at(window(), pos2(11.0, 100.0)).map(|(_, cursor)| cursor),
            Some(CursorIcon::ResizeWest)
        );
        assert_eq!(
            edge_at(window(), pos2(150.0, 219.0)).map(|(_, cursor)| cursor),
            Some(CursorIcon::ResizeSouth)
        );
    }
}

//! The welcome pane: what the chat column shows while no session is
//! open. The wordmark over the pane's glow, the project pill and its
//! menu, and the composer hosted between them.

#[cfg(not(target_arch = "wasm32"))]
use crate::assets::{WELCOME_GLOW_DAWN_PNG, WELCOME_GLOW_PNG};
use crate::icons::Icon;
use crate::icons::PaintSnapped as _;
use crate::store::Store;
use crate::theme::{
    FAMILY_BODY_EXTRABOLD, FAMILY_BODY_REGULAR, FAMILY_BODY_SEMIBOLD, FS_2XS, FS_SM, FS_XS, Palette,
    R_MD, WELCOME_PAD_BOTTOM_SHARE, WELCOME_PAD_TOP, WELCOME_W, family,
};

/// The mark's height, as the design draws it.
const MARK_H: f32 = 76.0;
/// The name's hard offset shadow.
const NAME_OFFSET: f32 = 4.2;
/// The wordmark's size, as the design draws it.
const NAME_PX: f32 = 60.0;
/// The gap between the mark and the name.
const WORDMARK_GAP: f32 = 16.0;
/// How far below the wordmark the rest of the stack sits.
const WORDMARK_MB: f32 = 30.0;
/// The pill's left inset in the column, as the reference shell indents it.
const PILL_ML: f32 = 12.0;
/// The gap between the pill and the composer under it.
const PILL_TO_COMPOSER: f32 = 8.0;
/// How far the project menu hangs under the pill.
const MENU_DROP: f32 = 7.0;
/// How far the project menu sits in from the pill's left edge, where
/// the reference anchors it.
const MENU_INDENT: f32 = 10.0;
/// The project menu's width, as the reference shell draws it.
const MENU_W: f32 = 320.0;
/// How many projects the menu lists, as the reference shell took them.
const MENU_ROWS: usize = 8;

/// What the pane asked the shell for this frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WelcomeEvent {
    /// The pill asked for the folder dialog.
    OpenFolder,
}

/// The welcome pane's state: whether the project menu shows, and the
/// stack height the last frame measured, which centers the stack
/// without a second layout pass.
#[derive(Debug, Default)]
pub struct Welcome {
    /// Whether the project menu shows.
    project_open: bool,
    /// The stack height the last frame drew, seeded with a first-frame
    /// estimate that one frame corrects.
    stack_h: f32,
}

impl Welcome {
    /// A welcome pane with its menu closed.
    pub fn new() -> Self {
        Self {
            project_open: false,
            stack_h: 420.0,
        }
    }

    /// Draws the pane over the column's free area and reports what the
    /// pill and the hosted composer asked for. The glow paints over the
    /// whole pane, and the composer mounts inside the stack under the
    /// pill, as the reference pane hosts it.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        store: &mut Store,
        composer: &mut crate::ui::composer::Composer,
        commands: &[serde_json::Value],
        running: bool,
        enter_sends: bool,
    ) -> (Vec<WelcomeEvent>, crate::ui::composer::ComposerEvent) {
        let mut events = Vec::new();
        let full = ui.available_rect_before_wrap();
        // The reference's chat band stays under the pane with no session,
        // collapsed to its 16px bottom padding, and the stack centers in
        // what is left above it.
        let pane = full.with_max_y(full.max.y - crate::theme::SP_6);
        let dark = ui.ctx().theme() == egui::Theme::Dark;
        let palette = Palette::active_of(ui.ctx());
        glow(ui, full, dark);

        // The column centers itself, and clamps to the padded pane the
        // way the web client's `@media (max-width: 860px)` pads it. The
        // floor keeps a degenerate first frame from collapsing the
        // stack while the panel widths settle.
        let col_w = WELCOME_W.min(pane.width() - 2.0 * 24.0).max(320.0);
        let left = crate::theme::snap(pane.left() + (pane.width() - col_w) / 2.0);
        // The bottom pad is a share of the window's height, not the
        // pane's, as the reference measures it.
        let viewport = ui.ctx().viewport_rect().height();
        let inner_h = pane.height() - WELCOME_PAD_TOP - viewport * WELCOME_PAD_BOTTOM_SHARE;
        let offset = ((inner_h - self.stack_h) / 2.0).max(0.0);
        let mut stack = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(egui::Rect::from_min_size(
                    egui::pos2(left, pane.top()),
                    egui::vec2(col_w, pane.height()),
                ))
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        // The reference snaps every box it lays out to a whole pixel.
        let top = crate::theme::snap(pane.top() + WELCOME_PAD_TOP + offset);
        stack.add_space(top - pane.top());
        // The height the next frame centers on starts here: the
        // leading pad and offset are the pane's, not the stack's, so
        // counting them would feed the offset back into itself and
        // settle the stack high of center.
        let content_top = stack.cursor().top();
        wordmark(&mut stack, palette);
        stack.add_space(WORDMARK_MB);
        let pill = self.project_pill(&mut stack, store, palette);
        stack.add_space(PILL_TO_COMPOSER);
        let composer_event = composer.show(&mut stack, store, commands, running, enter_sends);
        self.stack_h = stack.cursor().top() - content_top;
        self.project_menu(&mut stack, store, palette, pill, &mut events);
        (events, composer_event)
    }

    /// The project pill: the folder and the name of the directory the
    /// next session opens in. Clicking it toggles the project menu.
    fn project_pill(&mut self, ui: &mut egui::Ui, store: &Store, palette: &Palette) -> egui::Rect {
        let id = egui::Id::new("welcome-project-pill");
        let hovered = ui.ctx().read_response(id).is_some_and(|r| r.hovered());
        // A flex row, so the pill keeps its own width instead of
        // stretching across the column, as the reference shell drew it.
        let mut rect = egui::Rect::NOTHING;
        ui.horizontal(|ui| {
            ui.add_space(PILL_ML);
            let project = crate::ui::sidebar::project_name(store.session_dir());
            let name = ui.painter().layout_no_wrap(
                project.clone(),
                egui::FontId::new(FS_SM, crate::theme::prose_family(ui.ctx())),
                palette.ink,
            );
            let width = 10.0 + 14.0 + 7.0 + name.size().x.ceil() + 7.0 + 12.0 + 10.0;
            let (pill, _) = ui.allocate_exact_size(egui::vec2(width, 28.0), egui::Sense::hover());
            let fill = if hovered {
                palette.hover
            } else {
                palette.surface
            };
            ui.painter()
                .rect_filled(pill, egui::CornerRadius::same(R_MD as u8), fill);
            let mid = pill.center().y;
            crate::icons::themed_image(Icon::Folder, 14.0, palette.muted).paint_snapped(
                ui,
                egui::Rect::from_min_size(
                    egui::pos2(pill.left() + 10.0, mid - 7.0),
                    egui::vec2(14.0, 14.0),
                ),
            );
            let text_x = pill.left() + 10.0 + 14.0 + 7.0;
            let text_w = name.size().x.ceil();
            // The line centers in the pill, a whole pixel from its top.
            let above = ((28.0 - crate::theme::line_h(FS_SM)) / 2.0).floor();
            ui.painter()
                .galley(egui::pos2(text_x, pill.top() + above), name, palette.ink);
            crate::icons::themed_image(Icon::ChevronDown, 12.0, palette.faint).paint_snapped(
                ui,
                egui::Rect::from_min_size(
                    egui::pos2(text_x + text_w + 7.0, mid - 6.0),
                    egui::vec2(12.0, 12.0),
                ),
            );
            let response = ui.interact(pill, id, egui::Sense::click());
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &project)
            });
            if response.clicked() {
                self.project_open = !self.project_open;
            }
            rect = pill;
        });
        rect
    }

    /// The project menu over the pill: every directory a session
    /// worked in, most recent first, then the folder action. A click
    /// outside pill and menu closes it.
    fn project_menu(
        &mut self,
        ui: &mut egui::Ui,
        store: &mut Store,
        palette: &Palette,
        pill: egui::Rect,
        events: &mut Vec<WelcomeEvent>,
    ) {
        if !self.project_open {
            return;
        }
        let dirs = store
            .projects()
            .into_iter()
            .take(MENU_ROWS)
            .collect::<Vec<_>>();
        let current = store.session_dir().map(str::to_owned);
        let mut picked = None;
        let mut open_folder = false;
        let mut menu_rect = pill;
        egui::Window::new("welcome-project-menu")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .fixed_pos(pill.left_bottom() + egui::vec2(MENU_INDENT, MENU_DROP))
            .min_width(MENU_W)
            .order(egui::Order::Foreground)
            .frame(egui::Frame::NONE)
            .show(ui.ctx(), |ui| {
                crate::ui::toolbar::surface_in(ui, palette, palette.bg, 5, MENU_W - 10.0, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let (head, header) = ui.allocate_exact_size(
                        egui::vec2(
                            ui.available_width(),
                            6.0 + crate::theme::line_h(FS_2XS) + 3.0,
                        ),
                        egui::Sense::hover(),
                    );
                    header.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Label,
                            true,
                            crate::i18n::t("PROJECT"),
                        )
                    });
                    ui.painter().text(
                        egui::pos2(
                            head.left() + 9.0,
                            head.top() + 6.0 + crate::theme::line_h(FS_2XS) / 2.0,
                        ),
                        egui::Align2::LEFT_CENTER,
                        crate::i18n::t("PROJECT"),
                        egui::FontId::new(FS_2XS, family(FAMILY_BODY_SEMIBOLD)),
                        palette.faint,
                    );
                    for dir in &dirs {
                        let id = egui::Id::new("welcome-project-row").with(dir.as_str());
                        let row =
                            project_row(ui, palette, id, dir, current.as_deref() == Some(dir));
                        if row {
                            picked = Some(dir.clone());
                        }
                    }
                    divider(ui, palette);
                    let open = menu_row(
                        ui,
                        palette,
                        egui::Id::new("welcome-project-open"),
                        Icon::Plus,
                        crate::i18n::t("Open folder\u{2026}"),
                    );
                    if open {
                        open_folder = true;
                    }
                    menu_rect = ui.min_rect();
                });
            });
        let (clicked, at) = ui.input(|i| (i.pointer.any_click(), i.pointer.interact_pos()));
        if clicked && at.is_some_and(|pos| !pill.contains(pos) && !menu_rect.contains(pos)) {
            self.project_open = false;
        }
        if let Some(dir) = picked {
            store.set_project(Some(dir));
            self.project_open = false;
        }
        if open_folder {
            events.push(WelcomeEvent::OpenFolder);
            self.project_open = false;
        }
    }
}

/// The glow backdrop behind the stack, stretched over the pane the way
/// the web client's `.welcome` fills it. Palettes that draw flat skip
/// it; the browser build carries no PNG decoder and washes the pane
/// with the palette instead.
fn glow(ui: &mut egui::Ui, pane: egui::Rect, dark: bool) {
    if !Palette::active_of(ui.ctx()).glow {
        return;
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = dark;
        let palette = crate::theme::Palette::active_of(ui.ctx());
        ui.painter().rect_filled(pane, 0, palette.sunken);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let ctx = ui.ctx();
        let bytes = if dark {
            WELCOME_GLOW_PNG
        } else {
            WELCOME_GLOW_DAWN_PNG
        };
        let tex = png_texture(
            ctx,
            egui::Id::new("welcome-glow").with(dark),
            "welcome-glow",
            bytes,
        );
        // The reference's image keeps the square art's aspect: as wide
        // as the pane and as tall as it is wide, from the pane's top,
        // clipped to the pane.
        let square = egui::Rect::from_min_size(pane.min, egui::vec2(pane.width(), pane.width()));
        ui.painter().with_clip_rect(pane).image(
            tex.id(),
            square,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }
}

/// The wordmark: the nekomata mark, then the name over its hard
/// offset shadow, centered in the column.
fn wordmark(ui: &mut egui::Ui, palette: &Palette) {
    let font = egui::FontId::new(NAME_PX, family(FAMILY_BODY_EXTRABOLD));
    let name = ui
        .painter()
        .layout_no_wrap("pure".to_owned(), font.clone(), palette.ink_strong);
    let mark_size = crate::ui::mark::size(MARK_H);
    let total = mark_size.x + WORDMARK_GAP + name.size().x.ceil();
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), MARK_H),
        egui::Sense::hover(),
    );
    let left = crate::theme::snap(rect.left() + (rect.width() - total) / 2.0);
    let mark = egui::Rect::from_min_size(egui::pos2(left, rect.top()), mark_size);
    crate::ui::mark::paint(ui, mark);
    let name_top = mark.center().y - name.size().y / 2.0;
    let name_left = mark.right() + WORDMARK_GAP;
    let painter = ui.painter();
    painter.text(
        egui::pos2(name_left + NAME_OFFSET, name_top + NAME_OFFSET),
        egui::Align2::LEFT_TOP,
        "pure",
        font.clone(),
        palette.accent_soft,
    );
    painter.text(
        egui::pos2(name_left, name_top),
        egui::Align2::LEFT_TOP,
        "pure",
        font,
        palette.ink_strong,
    );
}

/// One project menu row: the folder, its name over its path, and the
/// check when the session opens there now.
fn project_row(
    ui: &mut egui::Ui,
    palette: &Palette,
    id: egui::Id,
    dir: &str,
    checked: bool,
) -> bool {
    let name_h = crate::theme::line_h(FS_SM);
    let path_h = crate::theme::line_h(FS_XS);
    let name = crate::ui::sidebar::project_name(Some(dir));
    let (rect, response) = menu_rect(ui, id, name_h + path_h, &name, palette);
    let mid = rect.center().y;
    let mut right = rect.right() - 9.0;
    if checked {
        right -= 14.0;
        crate::icons::themed_image(Icon::Check, 14.0, palette.accent).paint_snapped(
            ui,
            egui::Rect::from_min_size(egui::pos2(right, mid - 7.0), egui::vec2(14.0, 14.0)),
        );
        right -= 10.0;
    }
    let x = paint_menu_icon(ui, rect, Icon::Folder, palette);
    let top = mid - (name_h + path_h) / 2.0;
    let lines = [
        (name, FS_SM, palette.ink, top, name_h),
        (dir.to_owned(), FS_XS, palette.faint, top + name_h, path_h),
    ];
    for (text, size, color, top, height) in lines {
        let galley = crate::ui::transcript::head::one_line(
            ui,
            text,
            egui::TextFormat::simple(egui::FontId::new(size, family(FAMILY_BODY_REGULAR)), color),
            right - x,
        );
        ui.painter().galley(
            egui::pos2(x, top + (height - galley.size().y) / 2.0),
            galley,
            color,
        );
    }
    response.clicked()
}

/// One simple menu row: an icon and a label.
fn menu_row(ui: &mut egui::Ui, palette: &Palette, id: egui::Id, icon: Icon, label: &str) -> bool {
    let (rect, response) = menu_rect(ui, id, crate::theme::line_h(FS_SM), label, palette);
    let x = paint_menu_icon(ui, rect, icon, palette);
    ui.painter().text(
        egui::pos2(x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::new(FS_SM, family(FAMILY_BODY_REGULAR)),
        palette.ink,
    );
    response.clicked()
}

/// A menu row's rect around `content_h` of text, with its hover fill.
fn menu_rect(
    ui: &mut egui::Ui,
    id: egui::Id,
    content_h: f32,
    label: &str,
    palette: &Palette,
) -> (egui::Rect, egui::Response) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), content_h + 14.0),
        egui::Sense::hover(),
    );
    let response = ui
        .interact(rect, id, egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(R_MD as u8), palette.selected);
    }
    (rect, response)
}

/// Paints a menu row's leading icon and gives back where its text
/// starts.
fn paint_menu_icon(ui: &egui::Ui, rect: egui::Rect, icon: Icon, palette: &Palette) -> f32 {
    let at = egui::pos2(rect.left() + 9.0, rect.center().y - 7.0);
    crate::icons::themed_image(icon, 14.0, palette.muted)
        .paint_snapped(ui, egui::Rect::from_min_size(at, egui::vec2(14.0, 14.0)));
    at.x + 14.0 + 10.0
}

/// The hairline between the menu's rows and its action.
fn divider(ui: &mut egui::Ui, palette: &Palette) {
    ui.add_space(4.0);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width() - 4.0, 1.0),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, 0, palette.subtle);
    ui.add_space(4.0);
}

/// Decodes an embedded PNG once and keeps the texture in the context,
/// so a repaint never re-decodes brand art. The lookups stay outside
/// `load_texture`: the context's data lock and the texture manager's
/// lock do not nest.
#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::map_clone)] // `.cloned()` is ambiguous on IdTypeMap's two Option impls
fn png_texture(ctx: &egui::Context, id: egui::Id, name: &str, bytes: &[u8]) -> egui::TextureHandle {
    if let Some(tex) = ctx.data_mut(|data| data.get_temp(id).map(Clone::clone)) {
        return tex;
    }
    let decoded = image::load_from_memory(bytes).expect("the embedded brand art decodes");
    let size = [decoded.width() as usize, decoded.height() as usize];
    let tex = ctx.load_texture(
        name,
        egui::ColorImage::from_rgba_unmultiplied(size, decoded.to_rgba8().as_raw()),
        egui::TextureOptions::LINEAR,
    );
    ctx.data_mut(|data| data.insert_temp(id, tex.clone()));
    tex
}

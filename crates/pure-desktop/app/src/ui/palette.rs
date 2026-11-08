//! The command palette: a modal over the action registry, the agent's
//! commands and the session directory.
//!
//! The modal is a plain state machine (open, query, selected row) over
//! pure row building: [`app_actions`] names what the shell can carry
//! out, [`palette_entries`] narrows the agent's commands and the
//! sessions, and [`Registry::rows`] puts both under one query.
//!
//! The shell's key router owns the nav keys while the palette is open:
//! up and down call [`Palette::move_selection`], Enter calls
//! [`Palette::run`] and Escape calls [`Palette::close`]. The query
//! field inside the modal takes egui's focus, so typing lands in it.

use crate::icons::PaintSnapped as _;
use crate::ui::forms::ShowCrisp as _;
use pure_client::Session;
use serde_json::Value;

use crate::theme::{
    FAMILY_BODY_MEDIUM, FAMILY_BODY_REGULAR, FS_2XS, FS_SM, FS_XS, Palette as Theme, R_DIALOG,
    R_MD, family,
};

/// The dialog's width, the old palette's 620px.
const DIALOG_W: f32 = 620.0;

/// The dialog's height cap, the old palette's 520px.
const DIALOG_MAX_H: f32 = 520.0;

/// The space a small window keeps clear around the dialog.
const EDGE: f32 = 16.0;

/// The list's height cap, the old palette's 420px.
const LIST_MAX_H: f32 = 420.0;

/// The rows an empty query lists before the rest give way.
const RECENT_CAP: usize = 5;

/// The rows a query lists, a little wider than the empty list.
const QUERY_CAP: usize = 8;

/// An app command the palette offers. The shell matches on it; this
/// module only names the commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Show the welcome pane for a fresh session.
    NewSession,
    /// Turn swarm mode on or off.
    Swarm,
    /// Turn plan mode on or off.
    Plan,
    /// Set or clear the goal.
    Goal,
    /// Show or hide the workbench.
    Workbench,
    /// Show or hide the sidebar.
    Sidebar,
    /// Show the agents list.
    Agents,
    /// Show the files the session changed.
    Changes,
    /// Open the settings.
    Settings,
    /// Open the theme cards.
    Theme,
}

/// One command palette row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteEntry {
    /// An app command.
    Action {
        /// What runs.
        action: Action,
        /// The row text.
        label: String,
        /// Its shortcut, when it has one.
        keys: Option<&'static str>,
    },
    /// An agent command; running it fills the composer draft with the
    /// command text, ready for arguments.
    Command {
        /// The command name, without the leading slash.
        name: String,
        /// What the agent described the command as.
        description: String,
    },
    /// A session; running it follows the session.
    Session {
        /// The session id.
        id: String,
        /// The display title, when the agent set one.
        title: Option<String>,
        /// Whether the connection holds the session open.
        open: bool,
    },
}

impl PaletteEntry {
    /// The text the row renders and the query narrows over.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Action { label, .. } => label.clone(),
            Self::Command { name, .. } => format!("/{name}"),
            Self::Session { title, .. } => title
                .clone()
                .unwrap_or_else(|| crate::i18n::t("untitled session").to_owned()),
        }
    }

    /// The kind word the row carries.
    #[must_use]
    pub fn badge(&self) -> &'static str {
        match self {
            Self::Action { .. } | Self::Command { .. } => "command",
            Self::Session { open: true, .. } => "session",
            Self::Session { open: false, .. } => "recorded",
        }
    }

    /// The detail line a row carries beside its label.
    #[must_use]
    pub fn detail(&self) -> String {
        match self {
            Self::Action { .. } => String::new(),
            Self::Command { description, .. } => description.clone(),
            Self::Session { id, .. } => id.clone(),
        }
    }
}

/// The app commands for a palette over `session`: the session ones
/// only while a session shows, named for the state they flip.
#[must_use]
pub fn app_actions(session: Option<&Session>, plan_on: bool) -> Vec<PaletteEntry> {
    let action = |action, label: &'static str, keys: Option<&'static str>| PaletteEntry::Action {
        action,
        label: crate::i18n::t(label).to_owned(),
        keys,
    };
    let mut out = vec![action(Action::NewSession, "New session", Some("Ctrl N"))];
    if let Some(session) = session {
        let swarm = session
            .config_options
            .iter()
            .any(|option| option.id == "swarm" && option.current_value == "on");
        out.push(action(
            Action::Swarm,
            if swarm {
                "Turn swarm mode off"
            } else {
                "Turn swarm mode on"
            },
            None,
        ));
        out.push(action(
            Action::Plan,
            if plan_on {
                "Turn plan mode off"
            } else {
                "Turn plan mode on"
            },
            None,
        ));
        out.push(action(Action::Goal, "Set goal", None));
    }
    out.push(action(
        Action::Workbench,
        "Toggle workbench",
        Some("Ctrl B"),
    ));
    out.push(action(Action::Sidebar, "Toggle sidebar", Some("Ctrl \\")));
    if session.is_some() {
        out.push(action(Action::Agents, "Show agents", None));
        out.push(action(Action::Changes, "Review changes", None));
    }
    out.push(action(Action::Settings, "Settings", Some("Ctrl ,")));
    out.push(action(Action::Theme, "Change theme", None));
    out
}

/// The palette rows for the agent's commands and the session
/// directory, narrowed by `query` case insensitively. Commands come
/// first, then open sessions, then recorded ones. A session narrows
/// over its title and its id.
#[must_use]
pub fn palette_entries(
    commands: &[Value],
    sessions: &[(String, Option<String>, bool)],
    query: &str,
) -> Vec<PaletteEntry> {
    let needle = query.to_lowercase();
    let mut entries = Vec::new();
    for command in commands {
        let Some(name) = command.get("name").and_then(Value::as_str) else {
            continue;
        };
        let entry = PaletteEntry::Command {
            name: name.to_owned(),
            description: command
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        };
        if needle.is_empty() || entry.label().to_lowercase().contains(&needle) {
            entries.push(entry);
        }
    }
    for (id, title, open) in sessions {
        let entry = PaletteEntry::Session {
            id: id.clone(),
            title: title.clone(),
            open: *open,
        };
        let hit =
            entry.label().to_lowercase().contains(&needle) || id.to_lowercase().contains(&needle);
        if needle.is_empty() || hit {
            entries.push(entry);
        }
    }
    entries
}

/// What the palette filters over, read off the store each frame.
#[derive(Debug, Clone, Copy)]
pub struct Registry<'a> {
    /// The active session, when one shows.
    pub session: Option<&'a Session>,
    /// Whether plan mode is on for the active session.
    pub plan_on: bool,
    /// The agent's commands, raw as the wire carried them.
    pub commands: &'a [Value],
    /// The sessions to offer: id, display title, and whether the
    /// connection holds the session open.
    pub sessions: &'a [(String, Option<String>, bool)],
}

impl Registry<'_> {
    /// The rows for `query`: the app commands first, then the agent's
    /// commands and the sessions, narrowed case insensitively. Recent
    /// sessions are a short list; a query widens it a little.
    #[must_use]
    pub fn rows(&self, query: &str) -> Vec<PaletteEntry> {
        let needle = query.to_lowercase();
        let mut entries: Vec<PaletteEntry> = app_actions(self.session, self.plan_on)
            .into_iter()
            .filter(|entry| needle.is_empty() || entry.label().to_lowercase().contains(&needle))
            .collect();
        let cap = if needle.is_empty() {
            RECENT_CAP
        } else {
            QUERY_CAP
        };
        let mut found = palette_entries(self.commands, self.sessions, query);
        let commands = found
            .iter()
            .take_while(|entry| matches!(entry, PaletteEntry::Command { .. }))
            .count();
        found.truncate(commands + cap);
        entries.extend(found);
        entries
    }
}

/// The registry a store offers, owned so the palette's rows can be
/// built once per frame without borrowing the store across the draw.
#[derive(Debug, Clone, Default)]
pub struct RegistryOwned {
    /// The active session, when one shows.
    pub session: Option<Session>,
    /// Whether plan mode is on for the active session.
    pub plan_on: bool,
    /// The agent's commands, raw as the wire carried them.
    pub commands: Option<Vec<Value>>,
    /// The sessions to offer: id, display title, and whether the
    /// connection holds the session open.
    pub sessions: Vec<(String, Option<String>, bool)>,
}

impl RegistryOwned {
    /// The registry a store offers: the active session, its plan
    /// state, its commands and every session's title.
    #[must_use]
    pub fn of(store: &crate::store::Store) -> RegistryOwned {
        let session = store.active_session();
        let plan_on = session.is_some_and(|session| {
            crate::modes::active_mode(session).is_some_and(|mode| mode == crate::modes::PLAN_MODE)
        });
        let commands = session.map(|session| session.commands.clone());
        let state = store.state();
        let mut sessions: Vec<(String, Option<String>, bool)> = state
            .sessions
            .iter()
            .filter(|(_, session)| session.parent.is_none())
            .map(|(id, _)| (id.clone(), Some(store.display_title(id)), true))
            .collect();
        for info in &state.directory {
            if !state.sessions.contains_key(&info.session_id) {
                sessions.push((info.session_id.clone(), info.title.clone(), false));
            }
        }
        RegistryOwned {
            session: session.cloned(),
            plan_on,
            commands,
            sessions,
        }
    }

    /// The borrowed view the rows are built from.
    #[must_use]
    pub fn view(&self) -> Registry<'_> {
        Registry {
            session: self.session.as_ref(),
            plan_on: self.plan_on,
            commands: self.commands.as_deref().unwrap_or_default(),
            sessions: &self.sessions,
        }
    }
}

/// The command palette: a modal over the action registry, the agent's
/// commands and the session directory. The query narrows, the arrows
/// move, Enter runs and Esc closes.
#[derive(Debug, Clone, Default)]
pub struct Palette {
    /// Whether the modal shows.
    open: bool,
    /// The query the rows narrow over.
    query: String,
    /// The highlighted row index.
    selected: usize,
}

impl Palette {
    /// A closed palette over an empty query.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the palette shows.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Opens the palette over a fresh query with the first row
    /// highlighted.
    pub fn open(&mut self) {
        self.open = true;
        self.query.clear();
        self.selected = 0;
    }

    /// Closes the palette.
    pub fn close(&mut self) {
        self.open = false;
    }

    /// The query the rows narrow over.
    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Replaces the query, which returns the highlight to the first
    /// row the way a keystroke in the query field did.
    pub fn set_query(&mut self, query: impl Into<String>) {
        self.query = query.into();
        self.selected = 0;
    }

    /// The highlighted row index.
    #[must_use]
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Moves the highlight by `delta` over `total` rows, wrapping at
    /// the ends. No rows leaves it where it is.
    pub fn move_selection(&mut self, delta: isize, total: usize) {
        if total == 0 {
            return;
        }
        let total = total as isize;
        let next = self.selected as isize + delta;
        self.selected = (((next % total) + total) % total) as usize;
    }

    /// Clamps the highlight to the row count, dropping back to the
    /// first row when the rows shrank past it.
    pub fn clamp_selection(&mut self, total: usize) {
        if self.selected >= total {
            self.selected = 0;
        }
    }

    /// Runs the highlighted row: the palette closes and the row comes
    /// back for the shell to carry out. `None` when there are no rows.
    pub fn run(&mut self, rows: &[PaletteEntry]) -> Option<PaletteEntry> {
        self.clamp_selection(rows.len());
        let entry = rows.get(self.selected).cloned()?;
        self.close();
        Some(entry)
    }

    /// The search glyph and the query field across the dialog's top.
    fn query_row(&mut self, ui: &mut egui::Ui, palette: &Theme) {
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(16, 14))
            .show(ui, |ui| {
                let size = egui::vec2(ui.available_width(), 28.0);
                let layout = egui::Layout::left_to_right(egui::Align::Center);
                ui.allocate_ui_with_layout(size, layout, |ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    crate::icons::icon(ui, crate::icons::Icon::Search, 16.0, palette.faint);
                    // The reference's input pads its text 10px in and
                    // sets the line a whole pixel from the top.
                    let font = egui::FontId::new(15.0, crate::theme::prose_family(ui.ctx()));
                    let above = ((size.y - crate::theme::line_h(15.0)) / 2.0) as i8;
                    let field = egui::TextEdit::singleline(&mut self.query)
                        .id(egui::Id::new("pure-palette-query"))
                        .hint_text(
                            egui::RichText::new(crate::i18n::t("Run a command or switch session"))
                                .font(font.clone())
                                .color(palette.faint),
                        )
                        .text_color(palette.ink_strong)
                        .font(font)
                        .frame(egui::Frame::NONE.inner_margin(egui::Margin {
                            left: 10,
                            right: 10,
                            top: above,
                            bottom: 0,
                        }))
                        .vertical_align(egui::Align::TOP)
                        .min_size(egui::vec2(0.0, size.y))
                        .desired_width(ui.available_width());
                    ui.add(field).request_focus();
                });
            });
    }

    /// The grouped rows, or the empty note. Gives back the row a click
    /// ran.
    fn list(
        &self,
        ui: &mut egui::Ui,
        rows: &[PaletteEntry],
        selected: usize,
        reveal: bool,
        palette: &Theme,
    ) -> Option<usize> {
        if rows.is_empty() {
            let galley = ui.painter().layout_no_wrap(
                crate::i18n::t("Nothing matches").to_owned(),
                egui::FontId::new(FS_SM, family(FAMILY_BODY_REGULAR)),
                palette.faint,
            );
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), crate::theme::line_h(FS_SM) + 48.0),
                egui::Sense::hover(),
            );
            ui.painter()
                .galley(rect.min + egui::vec2(24.0, 24.0), galley, palette.faint);
            return None;
        }
        let mut clicked = None;
        let mut last_kind = "";
        for (index, entry) in rows.iter().enumerate() {
            let kind = match entry {
                PaletteEntry::Action { .. } | PaletteEntry::Command { .. } => "command",
                PaletteEntry::Session { .. } => "session",
            };
            if kind != last_kind {
                last_kind = kind;
                let label = match kind {
                    "session" if self.query.trim().is_empty() => crate::i18n::t("RECENT SESSIONS"),
                    "session" => crate::i18n::t("SESSIONS"),
                    _ => crate::i18n::t("COMMANDS"),
                };
                section_label(ui, label, palette);
            }
            let response = entry_row(ui, entry, index == selected, palette);
            if reveal && index == selected {
                response.scroll_to_me(None);
            }
            if response.clicked() {
                clicked = Some(index);
            }
        }
        clicked
    }

    /// Draws the modal over `rows`, centered with a dimmed backdrop.
    /// A click on a row runs it and the row comes back for the shell
    /// to carry out; a click on the backdrop closes the palette.
    pub fn show(&mut self, ui: &mut egui::Ui, rows: &[PaletteEntry]) -> Option<PaletteEntry> {
        self.clamp_selection(rows.len());
        let palette = Theme::active_of(ui.ctx());
        let mut run = None;
        let mut backdrop_clicked = false;
        let mut backdrop_pos = None;
        let mut card_rect = egui::Rect::NOTHING;
        let (up, down, enter, escape) = ui.input(|input| {
            (
                input.key_pressed(egui::Key::ArrowUp),
                input.key_pressed(egui::Key::ArrowDown),
                input.key_pressed(egui::Key::Enter),
                input.key_pressed(egui::Key::Escape),
            )
        });
        if escape {
            self.close();
            return None;
        }
        if up {
            self.move_selection(-1, rows.len());
        }
        if down {
            self.move_selection(1, rows.len());
        }
        if enter && !rows.is_empty() {
            run = Some(self.selected);
        }
        let selected = self.selected;
        let mut query_before = self.query.clone();
        egui::Area::new(egui::Id::new("pure-palette"))
            .anchor(egui::Align2::LEFT_TOP, egui::Vec2::ZERO)
            .order(egui::Order::Foreground)
            .interactable(true)
            .movable(false)
            .show(ui.ctx(), |ui| {
                let screen = ui.ctx().viewport_rect();
                ui.set_min_size(screen.size());
                let backdrop = ui.allocate_response(screen.size(), egui::Sense::click());
                ui.painter().rect_filled(
                    backdrop.rect,
                    egui::CornerRadius::ZERO,
                    egui::Color32::from_black_alpha(115),
                );
                backdrop_clicked = backdrop.clicked();
                backdrop_pos = ui.input(|input| input.pointer.interact_pos());
                // The dialog caps at the design's height: the content
                // below draws at its own height inside it.
                // A window smaller than the design shrinks the dialog and
                // its list, so the footer and both edges stay on screen.
                let width = DIALOG_W.min(screen.width() - 2.0 * EDGE);
                let top = screen.height() * 0.12;
                let height = DIALOG_MAX_H.min(screen.height() - top - EDGE);
                let list_h = (LIST_MAX_H - (DIALOG_MAX_H - height)).max(0.0);
                let dialog = egui::Rect::from_min_size(
                    egui::pos2((screen.width() - width) / 2.0, top),
                    egui::vec2(width, height),
                );
                let card = ui.scope_builder(egui::UiBuilder::new().max_rect(dialog), |ui| {
                    egui::Frame::NONE
                        .fill(palette.bg)
                        .stroke(egui::Stroke::new(1.0, palette.line))
                        .corner_radius(egui::CornerRadius::same(R_DIALOG as u8))
                        .show_shadowed(ui, &palette.shadow_2, |ui| {
                            ui.set_width(width - 2.0);
                            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                            self.query_row(ui, palette);
                            divider(ui, palette);
                            egui::ScrollArea::vertical()
                                .max_height(list_h)
                                .auto_shrink([false, true])
                                .show(ui, |ui| {
                                    egui::Frame::NONE.inner_margin(egui::Margin::same(6)).show(
                                        ui,
                                        |ui| {
                                            ui.set_width(ui.available_width());
                                            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                                            if let Some(index) =
                                                self.list(ui, rows, selected, up || down, palette)
                                            {
                                                run = Some(index);
                                            }
                                        },
                                    );
                                });
                            divider(ui, palette);
                            egui::Frame::NONE
                                .inner_margin(egui::Margin::symmetric(16, 8))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 14.0;
                                        hint(
                                            ui,
                                            "\u{2191}\u{2193}",
                                            crate::i18n::t("move"),
                                            palette,
                                        );
                                        hint(ui, "Enter", crate::i18n::t("run"), palette);
                                        hint(ui, "Esc", crate::i18n::t("close"), palette);
                                    });
                                });
                        });
                });
                card_rect = card.response.rect;
            });
        // The backdrop closes the palette, but only for clicks beside
        // the card: it answers the card's dead space too, and a click
        // there must not close what the user is using.
        if backdrop_clicked && backdrop_pos.is_none_or(|pos| !card_rect.contains(pos)) {
            self.close();
            return None;
        }
        if let Some(index) = run {
            // A click carries its own row: the run goes to the row
            // under the pointer, not to the keyboard highlight.
            self.selected = index;
            return self.run(rows);
        }
        if self.query != query_before {
            query_before = std::mem::replace(&mut self.query, query_before);
            self.set_query(query_before);
        }
        None
    }
}

/// The icon an app command's row leads with.
#[must_use]
fn action_icon(action: Action) -> crate::icons::Icon {
    match action {
        Action::NewSession => crate::icons::Icon::SquarePen,
        Action::Swarm => crate::icons::Icon::Waypoints,
        Action::Plan => crate::icons::Icon::ListTodo,
        Action::Goal => crate::icons::Icon::Target,
        Action::Workbench => crate::icons::Icon::PanelRight,
        Action::Sidebar => crate::icons::Icon::PanelLeft,
        Action::Agents => crate::icons::Icon::Users,
        Action::Changes => crate::icons::Icon::FileDiff,
        Action::Settings => crate::icons::Icon::Settings,
        Action::Theme => crate::icons::Icon::Moon,
    }
}

/// `text` laid out on one line, cut with an ellipsis past `max_width`.
fn one_line(
    ui: &egui::Ui,
    text: String,
    font: egui::FontId,
    color: egui::Color32,
    max_width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text, font, color);
    job.wrap.max_width = max_width.max(0.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('\u{2026}');
    ui.painter().layout_job(job)
}

/// A full-width hairline between the dialog's parts.
fn divider(ui: &mut egui::Ui, palette: &Theme) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::ZERO, palette.subtle);
}

/// A group's caps label over its rows.
fn section_label(ui: &mut egui::Ui, label: &str, palette: &Theme) {
    let line = crate::theme::line_h(FS_2XS);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 6.0 + line + 3.0),
        egui::Sense::hover(),
    );
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::new(FS_2XS, crate::theme::prose_semibold_family(ui.ctx())),
        palette.faint,
    );
    ui.painter().galley(
        egui::pos2(rect.left() + 9.0, rect.top() + 6.0),
        galley,
        palette.faint,
    );
}

/// One row: the icon, the label over its detail, and the shortcut or
/// kind chip at the right.
fn entry_row(
    ui: &mut egui::Ui,
    entry: &PaletteEntry,
    selected: bool,
    palette: &Theme,
) -> egui::Response {
    let prose = crate::theme::prose_family(ui.ctx());
    let (icon, label_font) = match entry {
        PaletteEntry::Action { action, .. } => (
            action_icon(*action),
            egui::FontId::new(FS_SM, prose.clone()),
        ),
        PaletteEntry::Command { .. } => (
            crate::icons::Icon::Command,
            egui::FontId::new(12.5, crate::theme::prose_mono_family(ui.ctx())),
        ),
        PaletteEntry::Session { .. } => (
            crate::icons::Icon::MessageSquare,
            egui::FontId::new(FS_SM, prose.clone()),
        ),
    };
    let detail = entry.detail();
    let label_h = crate::theme::line_h(label_font.size);
    let detail_h = if detail.is_empty() {
        0.0
    } else {
        crate::theme::line_h(FS_XS)
    };
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 7.0 + label_h + detail_h + 7.0),
        egui::Sense::click(),
    );
    let label = entry.label();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label));
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if selected || response.hovered() {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(R_MD as u8), palette.selected);
    }
    let top = rect.top() + 7.0;
    let chip_left = match entry {
        PaletteEntry::Action { keys: None, .. } => rect.right() - 9.0,
        PaletteEntry::Action {
            keys: Some(keys), ..
        } => paint_kbd(ui, egui::pos2(rect.right() - 9.0, top), keys, palette).left() - 10.0,
        _ => {
            paint_badge(
                ui,
                egui::pos2(rect.right() - 9.0, top + 2.0),
                entry.badge(),
                palette,
            )
            .left()
                - 10.0
        }
    };
    let icon_rect = egui::Rect::from_min_size(
        egui::pos2(rect.left() + 9.0, top + 2.0),
        egui::vec2(14.0, 14.0),
    );
    crate::icons::themed_image(icon, 14.0, palette.faint).paint_snapped(ui, icon_rect);
    let text_x = icon_rect.right() + 10.0;
    let label = one_line(
        ui,
        entry.label(),
        label_font,
        palette.ink,
        chip_left - text_x,
    );
    ui.painter()
        .galley(egui::pos2(text_x, top), label, palette.ink);
    if !detail.is_empty() {
        let galley = one_line(
            ui,
            detail,
            egui::FontId::new(FS_XS, prose),
            palette.muted,
            chip_left - text_x,
        );
        ui.painter()
            .galley(egui::pos2(text_x, top + label_h), galley, palette.muted);
    }
    response
}

fn kbd_galley(ui: &egui::Ui, label: &str, palette: &Theme) -> std::sync::Arc<egui::Galley> {
    ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::new(10.5, crate::theme::prose_mono_family(ui.ctx())),
        palette.faint,
    )
}

/// A key cap's size: 5px either side of the label inside the border,
/// its width rounded up as the reference rounds text.
fn kbd_size(galley: &egui::Galley) -> egui::Vec2 {
    egui::vec2(galley.size().x.ceil() + 12.0, 18.0)
}

/// Paints the keyboard chip with its right edge and top at `anchor`
/// and gives back where it landed.
pub(crate) fn paint_kbd(
    ui: &egui::Ui,
    anchor: egui::Pos2,
    label: &str,
    palette: &Theme,
) -> egui::Rect {
    let galley = kbd_galley(ui, label, palette);
    let size = kbd_size(&galley);
    let rect = egui::Rect::from_min_size(anchor - egui::vec2(size.x, 0.0), size);
    ui.painter().rect_stroke(
        rect,
        egui::CornerRadius::same(5),
        egui::Stroke::new(1.0, palette.line),
        egui::StrokeKind::Inside,
    );
    // The reference centers the label's line, a pixel taller than the
    // box inside the border, so the line starts at the box's top.
    ui.painter()
        .galley(rect.min + egui::vec2(6.0, 0.0), galley, palette.faint);
    rect
}

/// Paints the rounded kind badge with its right edge and top at
/// `anchor` and gives back where it landed.
fn paint_badge(ui: &egui::Ui, anchor: egui::Pos2, label: &str, palette: &Theme) -> egui::Rect {
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::new(10.5, family(FAMILY_BODY_MEDIUM)),
        palette.muted,
    );
    let size = egui::vec2(galley.size().x + 16.0, crate::theme::line_h(10.5) + 4.0);
    let rect = egui::Rect::from_min_size(anchor - egui::vec2(size.x, 0.0), size);
    let radius = egui::CornerRadius::same((size.y / 2.0) as u8);
    ui.painter().rect_filled(rect, radius, palette.fill);
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(1.0, palette.line),
        egui::StrokeKind::Inside,
    );
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, palette.muted);
    rect
}

/// The bordered keyboard hint chip of the design.
pub(crate) fn kbd_chip(ui: &mut egui::Ui, label: &str, palette: &Theme) -> egui::Response {
    let galley = kbd_galley(ui, label, palette);
    let (rect, response) = ui.allocate_exact_size(kbd_size(&galley), egui::Sense::hover());
    paint_kbd(ui, rect.right_top(), label, palette);
    response
}

/// One footer hint: the chip and the word beside it.
fn hint(ui: &mut egui::Ui, keys: &str, word: &str, palette: &Theme) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        kbd_chip(ui, keys, palette);
        let galley = ui.painter().layout_no_wrap(
            word.to_owned(),
            egui::FontId::new(FS_2XS, crate::theme::prose_family(ui.ctx())),
            palette.faint,
        );
        let size = egui::vec2(galley.size().x.ceil(), galley.size().y);
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, word));
        ui.painter().galley(rect.min, galley, palette.faint);
    });
}

#[cfg(test)]
mod tests {
    use super::{Action, Palette, PaletteEntry, app_actions, palette_entries};
    use pure_client::Session;

    /// The action rows a sessionless palette offers.
    fn sessionless_actions() -> Vec<PaletteEntry> {
        app_actions(None, false)
    }

    /// The action rows a palette over one open session offers.
    fn session_actions(swarm: bool, plan_on: bool) -> Vec<PaletteEntry> {
        let mut session = Session::new("s1");
        if swarm {
            session
                .config_options
                .push(pure_client::wire::SessionConfigOption {
                    id: "swarm".into(),
                    name: "Swarm".into(),
                    description: None,
                    category: None,
                    kind: pure_client::wire::SessionConfigKind::Select,
                    current_value: "on".into(),
                    options: Vec::new(),
                });
        }
        app_actions(Some(&session), plan_on)
    }

    #[test]
    fn app_actions_lead_with_the_ones_that_need_no_session() {
        let rows = sessionless_actions();
        assert_eq!(rows.len(), 5, "no session: the always-on commands only");
        assert_eq!(rows[0].label(), "New session");
        assert_eq!(rows[0].badge(), "command");
        assert_eq!(
            rows[0].detail(),
            "",
            "an app command carries no detail line"
        );
        assert_eq!(
            rows.last().map(PaletteEntry::label).as_deref(),
            Some("Change theme")
        );
        for row in &rows {
            assert!(
                matches!(row, PaletteEntry::Action { .. }),
                "every row is an action without a session"
            );
        }
    }

    #[test]
    fn the_session_commands_join_only_while_a_session_shows() {
        let rows = session_actions(false, false);
        assert_eq!(
            rows.len(),
            10,
            "the app commands lead, the session ones included"
        );
        let labels: Vec<String> = rows.iter().map(PaletteEntry::label).collect();
        assert_eq!(
            labels,
            [
                "New session",
                "Turn swarm mode on",
                "Turn plan mode on",
                "Set goal",
                "Toggle workbench",
                "Toggle sidebar",
                "Show agents",
                "Review changes",
                "Settings",
                "Change theme",
            ]
        );
        assert_eq!(
            session_actions(true, false)[1].label(),
            "Turn swarm mode off"
        );
        assert_eq!(
            session_actions(false, true)[2].label(),
            "Turn plan mode off"
        );
    }

    #[test]
    fn palette_entries_order_commands_then_sessions_and_narrow() {
        let commands = vec![serde_json::json!({
            "name": "review", "description": "review the diff",
        })];
        let sessions = vec![
            ("s1".to_owned(), Some("fix the null check".to_owned()), true),
            ("s2".to_owned(), None, true),
            ("rec1".to_owned(), None, false),
        ];
        let all = palette_entries(&commands, &sessions, "");
        assert_eq!(all.len(), 4);
        assert_eq!(
            all[0],
            PaletteEntry::Command {
                name: "review".into(),
                description: "review the diff".into(),
            }
        );
        assert_eq!(all[0].badge(), "command");
        assert_eq!(all[1].badge(), "session");
        assert_eq!(all[3].badge(), "recorded");
        assert_eq!(all[2].label(), "untitled session");

        let narrowed = palette_entries(&commands, &sessions, "rev");
        assert_eq!(narrowed.len(), 1);
        let by_id = palette_entries(&commands, &sessions, "s2");
        assert_eq!(by_id.len(), 1, "sessions narrow over their id too");
        assert_eq!(by_id[0].badge(), "session");
        assert!(palette_entries(&commands, &sessions, "zzz").is_empty());
    }

    #[test]
    fn a_command_row_narrows_over_its_slash_name() {
        let commands = vec![serde_json::json!({"name": "review"})];
        let found = palette_entries(&commands, &[], "/rev");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].label(), "/review");
        assert_eq!(found[0].detail(), "", "a command with no description");
    }

    #[test]
    fn the_registry_narrows_the_app_commands_with_the_rest() {
        let session = Session::new("s1");
        let registry = super::Registry {
            session: Some(&session),
            plan_on: false,
            commands: &[serde_json::json!({"name": "review", "description": "review the diff"})],
            sessions: &[
                ("s1".to_owned(), Some("fix the null check".to_owned()), true),
                ("s2".to_owned(), None, true),
            ],
        };
        let all = registry.rows("");
        assert_eq!(
            all.len(),
            13,
            "ten app commands, the command and both sessions"
        );
        assert_eq!(all[10].label(), "/review");

        let narrowed = registry.rows("rev");
        assert_eq!(narrowed.len(), 2, "the app command and the agent's");
        assert_eq!(narrowed[0].label(), "Review changes");
        assert_eq!(narrowed[1].label(), "/review");

        let by_id = registry.rows("s2");
        assert_eq!(by_id.len(), 1, "the sessions narrow over their id");
        assert!(registry.rows("zzz").is_empty());
    }

    #[test]
    fn an_empty_query_caps_the_recent_sessions_and_a_query_widens_them() {
        let session = Session::new("s1");
        let sessions: Vec<(String, Option<String>, bool)> = (0..9)
            .map(|index| (format!("s{index}"), None, true))
            .collect();
        let registry = super::Registry {
            session: Some(&session),
            plan_on: false,
            commands: &[],
            sessions: &sessions,
        };
        let recent = registry.rows("");
        assert_eq!(
            recent.len(),
            15,
            "ten app commands and five recent sessions"
        );
        let widened = registry.rows("s");
        let sessions = widened
            .iter()
            .filter(|entry| matches!(entry, PaletteEntry::Session { .. }))
            .count();
        assert_eq!(sessions, 8, "a query widens the sessions to eight");
    }

    #[test]
    fn a_closed_palette_opens_over_a_fresh_query_and_closes() {
        let mut palette = Palette::new();
        assert!(!palette.is_open());
        assert_eq!(palette.query(), "");
        assert_eq!(palette.selected(), 0);

        palette.set_query("stale");
        palette.move_selection(1, 3);
        assert_eq!(palette.selected(), 1);

        palette.open();
        assert!(palette.is_open());
        assert_eq!(palette.query(), "", "the opening query is fresh");
        assert_eq!(palette.selected(), 0, "the highlight starts at the top");

        palette.close();
        assert!(!palette.is_open());
    }

    #[test]
    fn the_highlight_moves_and_wraps_and_stays_without_rows() {
        let mut palette = Palette::new();
        palette.move_selection(1, 0);
        assert_eq!(palette.selected(), 0, "no rows, no move");

        palette.move_selection(1, 3);
        assert_eq!(palette.selected(), 1);
        palette.move_selection(1, 3);
        assert_eq!(palette.selected(), 2);
        palette.move_selection(1, 3);
        assert_eq!(palette.selected(), 0, "down wraps at the end");
        palette.move_selection(-1, 3);
        assert_eq!(palette.selected(), 2, "up wraps at the top");
    }

    #[test]
    fn typing_in_the_query_returns_the_highlight_to_the_top() {
        let mut palette = Palette::new();
        palette.open();
        palette.move_selection(2, 5);
        assert_eq!(palette.selected(), 2);
        palette.set_query("/re");
        assert_eq!(
            palette.selected(),
            0,
            "a new query re-highlights the first row"
        );
        assert_eq!(palette.query(), "/re");
    }

    #[test]
    fn a_run_takes_the_highlighted_row_and_closes() {
        let mut palette = Palette::new();
        palette.open();
        let rows = session_actions(false, false);
        palette.move_selection(1, rows.len());
        assert_eq!(palette.selected(), 1);

        let entry = palette.run(&rows).expect("the highlighted row runs");
        assert_eq!(entry.label(), "Turn swarm mode on");
        assert!(!palette.is_open(), "a run closes the palette");
    }

    #[test]
    fn a_run_over_no_rows_does_nothing() {
        let mut palette = Palette::new();
        palette.open();
        assert_eq!(palette.run(&[]), None);
        assert!(
            palette.selected() == 0,
            "no rows leaves the highlight alone"
        );
    }

    #[test]
    fn a_stale_highlight_drops_back_to_the_first_row() {
        let mut palette = Palette::new();
        palette.open();
        palette.move_selection(4, 5);
        assert_eq!(palette.selected(), 4);
        palette.clamp_selection(2);
        assert_eq!(palette.selected(), 0, "the rows shrank past the highlight");
        palette.clamp_selection(5);
        assert_eq!(palette.selected(), 0, "a clamp never raises the highlight");
    }

    #[test]
    fn every_action_row_names_a_shell_action() {
        for row in session_actions(false, false) {
            let PaletteEntry::Action { action, .. } = row else {
                panic!("every row is an app command here");
            };
            assert!(
                matches!(
                    action,
                    Action::NewSession
                        | Action::Swarm
                        | Action::Plan
                        | Action::Goal
                        | Action::Workbench
                        | Action::Sidebar
                        | Action::Agents
                        | Action::Changes
                        | Action::Settings
                        | Action::Theme
                ),
                "{action:?} is one the shell matches on"
            );
        }
    }
}

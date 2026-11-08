//! The MCP server form: adds a server to `config.toml` or edits one,
//! through `_pure/config/set`.

use serde_json::{Map, Value, json};

use crate::store::Store;
use crate::theme::{FS_SM, FS_XS, Palette};
use crate::ui::forms::{self, Pairs, Saving, Tone};
use crate::ui::settings_config::McpServer;

/// The form closed: saved, removed or cancelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormDone;

/// Servers the form can start from: a name, the command line or URL.
const PRESETS: [(&str, Preset); 5] = [
    (
        "Filesystem",
        Preset::Stdio(
            "filesystem",
            "npx",
            &["-y", "@modelcontextprotocol/server-filesystem", "."],
        ),
    ),
    (
        "GitHub",
        Preset::Http("github", "https://api.githubcopilot.com/mcp/"),
    ),
    (
        "Context7",
        Preset::Http("context7", "https://mcp.context7.com/mcp"),
    ),
    (
        "Playwright",
        Preset::Stdio("playwright", "npx", &["@playwright/mcp@latest"]),
    ),
    (
        "SQLite",
        Preset::Stdio(
            "sqlite",
            "uvx",
            &["mcp-server-sqlite", "--db-path", "./db.sqlite"],
        ),
    ),
];

/// What a preset fills in.
#[derive(Debug, Clone, Copy)]
enum Preset {
    /// A stdio server: name, command and its arguments.
    Stdio(&'static str, &'static str, &'static [&'static str]),
    /// An http server: name and URL.
    Http(&'static str, &'static str),
}

/// The MCP server form.
#[derive(Debug, Default)]
pub struct McpForm {
    /// The server being edited; `None` adds one.
    editing: Option<String>,
    /// The server's name.
    name: String,
    /// Whether the server speaks Streamable HTTP.
    http: bool,
    /// The command that starts a stdio server.
    command: String,
    /// The stdio server's arguments, one per line.
    args: String,
    /// The stdio server's environment.
    env: Pairs,
    /// The http server's URL.
    url: String,
    /// The http server's headers.
    headers: Pairs,
    /// Whether the server logs in with OAuth.
    oauth: bool,
    /// The pre-registered OAuth client id.
    client_id: String,
    /// The OAuth scope the server advertises.
    scope: String,
    /// Whether the server is configured but not started.
    disabled: bool,
    /// Where the save stands.
    saving: Saving,
    /// What the controls asked for this frame.
    asked: McpAsked,
}

/// What the MCP form's controls asked for this frame.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct McpAsked {
    /// Close without saving.
    close: bool,
    /// Save what the fields hold.
    save: bool,
    /// Drop the server's entry.
    remove: bool,
}

impl McpForm {
    /// A form for `server`, saved as `name`, or for a new server.
    #[must_use]
    pub fn new(existing: Option<(&str, &McpServer)>) -> Self {
        let blank = McpServer::default();
        let (name, server) = existing.unwrap_or(("", &blank));
        let oauth = server.oauth.as_ref();
        let text = |key: &str| {
            oauth
                .and_then(|oauth| oauth[key].as_str())
                .unwrap_or_default()
                .to_owned()
        };
        Self {
            editing: existing.map(|(name, _)| name.to_owned()),
            name: name.to_owned(),
            http: server.url.is_some(),
            command: server.command.clone().unwrap_or_default(),
            args: server.args.join("\n"),
            env: Pairs::new(&server.env),
            url: server.url.clone().unwrap_or_default(),
            headers: Pairs::new(&server.headers),
            oauth: oauth.is_some(),
            client_id: text("client_id"),
            scope: text("scope"),
            disabled: server.disabled,
            saving: Saving::Idle,
            asked: McpAsked::default(),
        }
    }

    /// The server being edited, or `None` when the form adds one.
    #[must_use]
    pub fn editing(&self) -> Option<&str> {
        self.editing.as_deref()
    }

    /// Follows the store: `true` once the save landed.
    pub fn follow(&mut self, store: &Store) -> bool {
        self.saving.settle(store)
    }

    /// The server's name and its `[mcp.servers.<name>]` table, or why
    /// the form cannot save.
    pub fn entry(&self) -> Result<(String, Value), String> {
        let name = self
            .editing
            .clone()
            .unwrap_or_else(|| self.name.trim().to_owned());
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(crate::i18n::t(
                "Name the server with letters, digits, dashes or underscores.",
            )
            .to_owned());
        }
        let mut table = Map::new();
        if self.http {
            let url = self.url.trim().to_owned();
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                return Err(crate::i18n::t("Set an http(s) URL.").to_owned());
            }
            table.insert("url".into(), json!(url));
            let headers = self.headers.values();
            if !headers.is_empty() {
                table.insert("headers".into(), json!(headers));
            }
            if self.oauth {
                let mut oauth = Map::new();
                for (key, value) in [("client_id", &self.client_id), ("scope", &self.scope)] {
                    let value = value.trim().to_owned();
                    if !value.is_empty() {
                        oauth.insert(key.into(), json!(value));
                    }
                }
                table.insert("oauth".into(), Value::Object(oauth));
            }
        } else {
            let command = self.command.trim().to_owned();
            if command.is_empty() {
                return Err(crate::i18n::t("Set the command that starts the server.").to_owned());
            }
            table.insert("command".into(), json!(command));
            let args: Vec<String> = self
                .args
                .lines()
                .map(str::trim)
                .filter(|arg| !arg.is_empty())
                .map(str::to_owned)
                .collect();
            if !args.is_empty() {
                table.insert("args".into(), json!(args));
            }
            let env = self.env.values();
            if !env.is_empty() {
                table.insert("env".into(), json!(env));
            }
        }
        if self.disabled {
            table.insert("disabled".into(), json!(true));
        }
        Ok((name, Value::Object(table)))
    }

    /// Queues the write the form describes.
    pub fn save(&mut self, store: &mut Store) -> Result<u64, String> {
        let (name, value) = self.entry()?;
        let id = store.config_set(&["mcp", "servers", &name], Some(value));
        self.saving = Saving::Sent(vec![id]);
        Ok(id)
    }

    /// Drops the server's entry.
    pub fn remove(&mut self, store: &mut Store) -> Option<u64> {
        let name = self.editing.clone()?;
        let id = store.config_set(&["mcp", "servers", &name], None);
        self.saving = Saving::Sent(vec![id]);
        Some(id)
    }

    /// The page's title.
    #[must_use]
    pub fn title(&self) -> String {
        match &self.editing {
            Some(name) => crate::tr!("Edit {}", name),
            None => crate::i18n::t("Add MCP server").to_owned(),
        }
    }

    /// Draws the form. Returns whether the form asked to close.
    pub fn show(&mut self, ui: &mut egui::Ui, store: &mut Store) -> bool {
        let palette = Palette::active_of(ui.ctx());
        self.asked = McpAsked::default();
        let mut edit = McpEdit::default();
        ui.vertical(|ui| {
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.add_space(18.0);
                ui.set_max_width(ui.available_width() - 36.0);
                ui.vertical(|ui| {
                    let (back, _) = forms::form_head(ui, &self.title(), palette);
                    if back.clicked() {
                        self.asked.close = true;
                    }
                    if self.editing.is_none() {
                        self.presets(ui, palette, &mut edit);
                        forms::field(
                            ui,
                            crate::i18n::t("Name"),
                            true,
                            Some(crate::i18n::t("Tools show as name__tool")),
                            |ui| {
                                let mut name = self.name.clone();
                                let response =
                                    forms::text_field(ui, &mut name, "server-name", true, palette);
                                if response.changed() {
                                    edit.name = name;
                                    edit.name_set = true;
                                }
                            },
                        );
                    }
                    let mut picked = None;
                    forms::field(ui, crate::i18n::t("Transport"), false, None, |ui| {
                        let choices: &[(&str, &str)] =
                            &[("stdio", "stdio"), ("http", "Streamable HTTP")];
                        picked = forms::segments(
                            ui,
                            "mcp-transport",
                            choices,
                            if self.http { "http" } else { "stdio" },
                            palette,
                        );
                    });
                    if let Some(value) = picked {
                        edit.http = Some(value == "http");
                    }
                    if edit.http.unwrap_or(self.http) {
                        self.http_fields(ui, palette, &mut edit);
                    } else {
                        forms::field(
                            ui,
                            crate::i18n::t("Command"),
                            true,
                            Some(crate::i18n::t("Runs in the session's directory.")),
                            |ui| {
                                let mut command = self.command.clone();
                                let response = forms::text_field(
                                    ui,
                                    &mut command,
                                    crate::i18n::t("command"),
                                    true,
                                    palette,
                                );
                                if response.changed() {
                                    edit.command = command;
                                    edit.command_set = true;
                                }
                            },
                        );
                        forms::field(
                            ui,
                            crate::i18n::t("Arguments"),
                            false,
                            Some(crate::i18n::t("One per line.")),
                            |ui| {
                                let mut args = self.args.clone();
                                let response = forms::lines_field(
                                    ui,
                                    &mut args,
                                    crate::i18n::t("one argument per line"),
                                    palette,
                                );
                                if response.changed() {
                                    edit.args = args;
                                    edit.args_set = true;
                                }
                            },
                        );
                        let mut env = std::mem::take(&mut self.env);
                        forms::field(
                            ui,
                            crate::i18n::t("Environment"),
                            false,
                            Some(crate::i18n::t("A blank value keeps the saved one.")),
                            |ui| {
                                forms::pairs(ui, "mcp-env", &mut env, palette);
                            },
                        );
                        self.env = env;
                    }
                    ui.horizontal(|ui| {
                        ui.add_space(0.0);
                        ui.spacing_mut().item_spacing.x = 10.0;
                        let toggle = forms::switch(ui, self.disabled, palette);
                        if toggle.clicked() {
                            edit.disabled = Some(!self.disabled);
                        }
                        ui.label(
                            egui::RichText::new(crate::i18n::t(
                                "Disabled: configured but not started",
                            ))
                            .size(FS_SM)
                            .color(palette.ink),
                        );
                    });
                    if let Some((line, color)) = self.saving.line(palette) {
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new(line).size(FS_XS).color(color));
                    }
                    self.actions(ui, palette);
                });
            });
        });
        edit.apply(self);
        self.carry(store)
    }

    /// The preset chips a new server starts from.
    fn presets(&self, ui: &mut egui::Ui, palette: &Palette, edit: &mut McpEdit) {
        forms::field(ui, crate::i18n::t("Start from"), false, None, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.spacing_mut().item_spacing.y = 6.0;
                for (label, preset) in PRESETS {
                    let response = forms::button(ui, crate::i18n::t(label), Tone::Plain, palette);
                    if response.clicked() {
                        edit.preset = Some(preset);
                    }
                }
            });
        });
    }

    /// The fields an http server carries.
    fn http_fields(&mut self, ui: &mut egui::Ui, palette: &Palette, edit: &mut McpEdit) {
        forms::field(ui, "URL", true, None, |ui| {
            let mut url = self.url.clone();
            let response =
                forms::text_field(ui, &mut url, "https://example.com/mcp", true, palette);
            if response.changed() {
                edit.url = url;
                edit.url_set = true;
            }
        });
        let mut headers = std::mem::take(&mut self.headers);
        forms::field(
            ui,
            crate::i18n::t("Headers"),
            false,
            Some(crate::i18n::t(
                "Sent on every request, such as Authorization. A blank value keeps the saved one.",
            )),
            |ui| {
                forms::pairs(ui, "mcp-headers", &mut headers, palette);
            },
        );
        self.headers = headers;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let toggle = forms::switch(ui, self.oauth, palette);
            if toggle.clicked() {
                edit.oauth = Some(!self.oauth);
            }
            ui.label(
                egui::RichText::new(crate::i18n::t("Log in with OAuth"))
                    .size(FS_SM)
                    .color(palette.ink),
            );
        });
        if edit.oauth.unwrap_or(self.oauth) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                forms::field(
                    ui,
                    crate::i18n::t("Client id"),
                    false,
                    Some(crate::i18n::t(
                        "Leave blank to register pure with the server.",
                    )),
                    |ui| {
                        let mut client_id = self.client_id.clone();
                        let response = forms::text_field(ui, &mut client_id, "", false, palette);
                        if response.changed() {
                            edit.client_id = client_id;
                            edit.client_id_set = true;
                        }
                    },
                );
                forms::field(ui, crate::i18n::t("Scope"), false, None, |ui| {
                    let mut scope = self.scope.clone();
                    let response = forms::text_field(ui, &mut scope, "", false, palette);
                    if response.changed() {
                        edit.scope = scope;
                        edit.scope_set = true;
                    }
                });
            });
        }
    }

    /// The action row.
    fn actions(&mut self, ui: &mut egui::Ui, palette: &Palette) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if self.editing.is_some() {
                let remove =
                    forms::button(ui, crate::i18n::t("Remove server"), Tone::Danger, palette);
                if remove.clicked() {
                    self.asked.remove = true;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let save = forms::button(ui, crate::i18n::t("Save"), Tone::Primary, palette);
                if save.clicked() && !self.saving.busy() {
                    self.asked.save = true;
                }
                let cancel = forms::button(ui, crate::i18n::t("Cancel"), Tone::Plain, palette);
                if cancel.clicked() {
                    self.asked.close = true;
                }
            });
        });
    }

    /// Carries out what the controls asked for.
    fn carry(&mut self, store: &mut Store) -> bool {
        let asked = self.asked;
        self.asked = McpAsked::default();
        if asked.remove {
            self.remove(store);
        }
        if asked.save
            && let Err(why) = self.save(store)
        {
            self.saving = Saving::Refused(why);
        }
        asked.close
    }
}

/// What the MCP form's fields asked to change this frame.
#[derive(Debug, Default)]
struct McpEdit {
    /// The typed server name.
    name: String,
    /// Whether the name field changed.
    name_set: bool,
    /// The chosen transport.
    http: Option<bool>,
    /// The typed command.
    command: String,
    /// Whether the command field changed.
    command_set: bool,
    /// The typed arguments.
    args: String,
    /// Whether the arguments field changed.
    args_set: bool,
    /// The typed URL.
    url: String,
    /// Whether the URL field changed.
    url_set: bool,
    /// The chosen OAuth state.
    oauth: Option<bool>,
    /// The typed OAuth client id.
    client_id: String,
    /// Whether the client id field changed.
    client_id_set: bool,
    /// The typed OAuth scope.
    scope: String,
    /// Whether the scope field changed.
    scope_set: bool,
    /// The chosen disabled state.
    disabled: Option<bool>,
    /// The preset to fill the fields from.
    preset: Option<Preset>,
}

impl McpEdit {
    /// Applies the edits to `form`.
    fn apply(self, form: &mut McpForm) {
        if self.name_set {
            form.name = self.name;
        }
        if let Some(http) = self.http {
            form.http = http;
        }
        if self.command_set {
            form.command = self.command;
        }
        if self.args_set {
            form.args = self.args;
        }
        if self.url_set {
            form.url = self.url;
        }
        if let Some(oauth) = self.oauth {
            form.oauth = oauth;
        }
        if self.client_id_set {
            form.client_id = self.client_id;
        }
        if self.scope_set {
            form.scope = self.scope;
        }
        if let Some(disabled) = self.disabled {
            form.disabled = disabled;
        }
        if let Some(preset) = self.preset {
            form.apply_preset(preset);
        }
    }
}

impl McpForm {
    /// Fills the fields from `preset`.
    fn apply_preset(&mut self, preset: Preset) {
        match preset {
            Preset::Stdio(name, command, args) => {
                self.http = false;
                self.name = name.to_owned();
                self.command = command.to_owned();
                self.args = args.join("\n");
            }
            Preset::Http(name, url) => {
                self.http = true;
                self.name = name.to_owned();
                self.url = url.to_owned();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::McpForm;
    use crate::ui::settings_config::Snapshot;

    #[test]
    fn an_edited_server_keeps_its_secret_values() {
        let snapshot = Snapshot::parse(&json!({"mcp": {"servers": {"fs": {
            "command": "npx",
            "args": ["-y", "server-fs", "."],
            "env": {"TOKEN": "<redacted>", "MODE": "ro"},
        }}}}));
        let form = McpForm::new(Some(("fs", snapshot.mcp_server("fs").unwrap())));
        let entry = form.entry();
        let (name, value) = entry.unwrap();
        assert_eq!(name, "fs");
        assert_eq!(
            value,
            json!({
                "command": "npx",
                "args": ["-y", "server-fs", "."],
                "env": {"TOKEN": "<redacted>", "MODE": "ro"},
            })
        );
    }

    #[test]
    fn a_new_server_needs_a_name_and_a_url() {
        let mut form = McpForm::new(None);
        let entry = form.entry();
        assert!(entry.unwrap_err().contains("Name the server"));
        form.http = true;
        form.editing = Some("hub".into());
        assert_eq!(form.entry().unwrap_err(), "Set an http(s) URL.");
    }

    #[test]
    fn an_http_server_carries_its_headers_and_oauth() {
        let mut form = McpForm::new(None);
        form.http = true;
        form.editing = Some("hub".into());
        form.url = "https://mcp.example.com".to_owned();
        form.oauth = true;
        form.client_id = "pure".to_owned();
        let (name, value) = form.entry().unwrap();
        assert_eq!(name, "hub");
        assert_eq!(value["url"], "https://mcp.example.com");
        assert_eq!(value["oauth"]["client_id"], "pure");
    }

    #[test]
    fn a_disabled_server_says_so_in_its_table() {
        let mut form = McpForm::new(None);
        form.editing = Some("off".into());
        form.command = "x".to_owned();
        form.disabled = true;
        let (_, value) = form.entry().unwrap();
        assert_eq!(value["disabled"], true);
    }

    #[test]
    fn a_preset_fills_the_fields() {
        let mut form = McpForm::new(None);
        form.apply_preset(super::Preset::Stdio(
            "filesystem",
            "npx",
            &["-y", "@modelcontextprotocol/server-filesystem", "."],
        ));
        assert!(!form.http);
        assert_eq!(form.name, "filesystem");
        assert_eq!(form.command, "npx");
        assert_eq!(form.args, "-y\n@modelcontextprotocol/server-filesystem\n.");
    }

    #[test]
    fn the_edited_title_names_the_server() {
        let form = McpForm::new(None);
        assert_eq!(form.title(), "Add MCP server");
    }
}

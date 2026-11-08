//! The provider directory picker: reads models.dev, or another
//! directory in its `api.json` shape, through the engine, lets the user
//! pick a provider and its models, and hands the provider form an entry
//! holding them, prices and limits included.

use std::collections::BTreeSet;

use pure_client::wire::{DirectoryModel, DirectoryProvider};
use serde_json::{Map, Value, json};

use crate::store::Store;
use crate::theme::{FS_SM, FS_XS, Palette};
use crate::ui::forms::{self, Tone};

/// The picker chose a provider: its id and the entry to start from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picked {
    /// The provider id.
    pub id: String,
    /// The `[providers.custom.<id>]` table with the chosen models.
    pub entry: Value,
}

/// The directory picker.
#[derive(Debug, Default)]
pub struct DirectoryPicker {
    /// Whether the directory is one the user names, not models.dev.
    named: bool,
    /// The named directory's URL.
    url: String,
    /// The named directory's key.
    key: String,
    /// The search narrowing the lists.
    search: String,
    /// The read in flight.
    reading: Option<u64>,
    /// The providers the directory listed.
    providers: Vec<DirectoryProvider>,
    /// Why the read failed.
    error: Option<String>,
    /// The provider being picked from, by index.
    chosen: Option<usize>,
    /// The models picked, by id.
    picked: BTreeSet<String>,
    /// What the controls asked for this frame.
    asked: PickerAsked,
}

/// What the picker's controls asked for this frame.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct PickerAsked {
    /// Close without importing.
    close: bool,
    /// Read the directory again.
    load: bool,
    /// Import the chosen models.
    import: bool,
}

impl DirectoryPicker {
    /// A picker for models.dev, read at once, or for a directory the
    /// user names when `named`.
    #[must_use]
    pub fn new(store: &mut Store, named: bool) -> Self {
        let mut picker = Self {
            named,
            ..Self::default()
        };
        if !named {
            picker.load(store);
        }
        picker
    }

    /// Follows the store: takes the directory read's answer.
    pub fn follow(&mut self, store: &Store) {
        let Some(id) = self.reading else {
            return;
        };
        if let Some(read) = store.directory(id) {
            self.reading = None;
            match read {
                Ok(providers) => {
                    self.providers.clone_from(providers);
                    self.error = None;
                }
                Err(why) => self.error = Some(why.clone()),
            }
        }
    }

    /// Asks the engine to read the directory.
    fn load(&mut self, store: &mut Store) {
        let url = self.url.trim().to_owned();
        let key = self.key.trim().to_owned();
        if self.named && url.is_empty() {
            self.error = Some(crate::i18n::t("Name the directory's URL.").to_owned());
            return;
        }
        let id = store.ask_directory(
            self.named.then_some(url.as_str()),
            (!key.is_empty()).then_some(key.as_str()),
        );
        self.reading = Some(id);
        self.error = None;
        self.chosen = None;
        self.picked.clear();
    }

    /// The search narrowing both lists.
    fn query(&self) -> String {
        self.search.trim().to_lowercase()
    }

    /// Picks provider `ix`, starting with every model of it picked.
    fn choose(&mut self, ix: Option<usize>) {
        self.chosen = ix;
        self.picked = ix
            .and_then(|ix| self.providers.get(ix))
            .map(|provider| provider.models.iter().map(|m| m.id.clone()).collect())
            .unwrap_or_default();
    }

    /// The provider and the picked models, or `None` while nothing
    /// importable is picked.
    fn import(&self) -> Option<Picked> {
        let provider = self.chosen.and_then(|ix| self.providers.get(ix))?;
        if self.picked.is_empty() {
            return None;
        }
        Some(picked(provider, &self.picked))
    }

    /// The page's title.
    #[must_use]
    pub fn title(&self) -> &'static str {
        if self.named {
            crate::i18n::t("Import from api.json")
        } else {
            crate::i18n::t("Import from models.dev")
        }
    }

    /// Draws the picker. Returns the provider to start a form from, or
    /// `None` when the picker asked to close.
    pub fn show(&mut self, ui: &mut egui::Ui, store: &mut Store) -> Option<Picked> {
        self.follow(store);
        let palette = Palette::active_of(ui.ctx());
        self.asked = PickerAsked::default();
        let mut edit = PickerEdit::default();
        ui.vertical(|ui| {
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.add_space(18.0);
                ui.set_max_width(ui.available_width() - 36.0);
                ui.vertical(|ui| {
                    let (back, _) = forms::form_head(ui, self.title(), palette);
                    if back.clicked() {
                        self.asked.close = true;
                    }
                    if self.named {
                        self.named_row(ui, palette, &mut edit);
                    }
                    if self.reading.is_some() {
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(crate::i18n::t("Reading the directory\u{2026}"))
                                .size(FS_XS)
                                .color(palette.muted),
                        );
                    }
                    if let Some(why) = &self.error {
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(why.clone())
                                .size(FS_XS)
                                .color(palette.danger),
                        );
                    }
                    if !self.providers.is_empty() {
                        forms::field(ui, crate::i18n::t("Search"), false, None, |ui| {
                            let mut search = self.search.clone();
                            let response = forms::text_field(
                                ui,
                                &mut search,
                                crate::i18n::t("Search providers and models"),
                                false,
                                palette,
                            );
                            if response.changed() {
                                edit.search = search;
                                edit.search_set = true;
                            }
                        });
                        match self.chosen {
                            Some(ix) => self.models(ui, ix, palette, &mut edit),
                            None => self.provider_list(ui, palette, &mut edit),
                        }
                    }
                });
            });
        });
        edit.apply(self);
        let asked = self.asked;
        self.asked = PickerAsked::default();
        if asked.load {
            self.load(store);
        }
        if asked.import {
            return self.import();
        }
        if asked.close {
            return Some(Picked {
                id: String::new(),
                entry: Value::Null,
            });
        }
        None
    }

    /// The URL and key row a named directory reads from.
    fn named_row(&self, ui: &mut egui::Ui, palette: &Palette, edit: &mut PickerEdit) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            forms::field(ui, crate::i18n::t("Registry URL"), true, None, |ui| {
                let mut url = self.url.clone();
                let response =
                    forms::text_field(ui, &mut url, "https://example.com/api.json", true, palette);
                if response.changed() {
                    edit.url = url;
                    edit.url_set = true;
                }
            });
            forms::field(ui, crate::i18n::t("API key"), false, None, |ui| {
                let mut key = self.key.clone();
                let response =
                    forms::text_field(ui, &mut key, crate::i18n::t("optional"), false, palette);
                if response.changed() {
                    edit.key = key;
                    edit.key_set = true;
                }
            });
            let load = forms::button(ui, crate::i18n::t("Load"), Tone::Primary, palette);
            if load.clicked() {
                edit.load = true;
            }
        });
    }

    /// The provider list, narrowed by the search.
    fn provider_list(&self, ui: &mut egui::Ui, palette: &Palette, edit: &mut PickerEdit) {
        let query = self.query();
        let mut rows: Vec<usize> = Vec::new();
        for (ix, provider) in self.providers.iter().enumerate() {
            let matches = query.is_empty()
                || provider.name.to_lowercase().contains(&query)
                || provider.id.contains(&query)
                || provider
                    .models
                    .iter()
                    .any(|m| m.id.to_lowercase().contains(&query));
            if matches {
                rows.push(ix);
            }
        }
        if rows.is_empty() {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(crate::i18n::t("No provider matches."))
                    .size(FS_XS)
                    .color(palette.muted),
            );
            return;
        }
        for ix in rows {
            let provider = &self.providers[ix];
            let endpoint = provider
                .api
                .as_deref()
                .unwrap_or_else(|| crate::i18n::t("no endpoint listed"));
            let mut detail = crate::tr!("{} \u{b7} {} models", endpoint, provider.models.len());
            if provider.kind.is_none() {
                detail.push_str(crate::i18n::t(" \u{b7} protocol unknown, assumed OpenAI"));
            }
            if provider_row(ui, &provider.name, &detail, palette).clicked() {
                edit.chosen = Some(ix);
            }
        }
    }

    /// The models of provider `ix`, as pickable pills.
    fn models(&self, ui: &mut egui::Ui, ix: usize, palette: &Palette, edit: &mut PickerEdit) {
        let Some(provider) = self.providers.get(ix) else {
            return;
        };
        let query = self.query();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let back = forms::button(ui, crate::i18n::t("All providers"), Tone::Plain, palette);
            if back.clicked() {
                edit.chosen = Some(usize::MAX);
            }
            let all = forms::button(ui, crate::i18n::t("Select all"), Tone::Plain, palette);
            if all.clicked() {
                edit.every = true;
            }
            let none = forms::button(ui, crate::i18n::t("Select none"), Tone::Plain, palette);
            if none.clicked() {
                edit.none = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let count = self.picked.len();
                let label = if count == 1 {
                    crate::i18n::t("Import 1 model").to_owned()
                } else {
                    crate::tr!("Import {} models", count)
                };
                let import = forms::button(ui, &label, Tone::Primary, palette);
                if import.clicked() {
                    edit.import = true;
                }
            });
        });
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.spacing_mut().item_spacing.y = 6.0;
            for model in &provider.models {
                if !query.is_empty()
                    && !model.id.to_lowercase().contains(&query)
                    && !model.name.to_lowercase().contains(&query)
                {
                    continue;
                }
                let on = self.picked.contains(&model.id);
                let context = model
                    .context
                    .map(|context| format!(" \u{b7} {}k", context / 1000))
                    .unwrap_or_default();
                let pill = egui::Button::new(
                    egui::RichText::new(format!("{}{context}", model.name))
                        .size(FS_XS)
                        .color(if on {
                            palette.ink_strong
                        } else {
                            palette.muted
                        }),
                )
                .fill(if on {
                    palette.accent_soft
                } else {
                    palette.surface
                })
                .stroke(egui::Stroke::new(
                    1.0,
                    if on { palette.accent } else { palette.line },
                ))
                .corner_radius(egui::CornerRadius::same(12))
                .min_size(egui::vec2(0.0, 24.0));
                let response = ui.add(pill);
                if response.clicked() {
                    edit.flipped = Some(model.id.clone());
                }
            }
        });
    }
}

/// What the picker's fields and rows asked to change this frame.
#[derive(Debug, Default)]
struct PickerEdit {
    /// The typed search.
    search: String,
    /// Whether the search field changed.
    search_set: bool,
    /// The typed URL.
    url: String,
    /// Whether the URL field changed.
    url_set: bool,
    /// The typed key.
    key: String,
    /// Whether the key field changed.
    key_set: bool,
    /// The provider row clicked, or the list again.
    chosen: Option<usize>,
    /// Whether every model of the chosen provider turned on.
    every: bool,
    /// Whether every model of the chosen provider turned off.
    none: bool,
    /// The model pill flipped this frame.
    flipped: Option<String>,
    /// Whether the Load button was pressed.
    load: bool,
    /// Whether the import button was pressed.
    import: bool,
}

impl PickerEdit {
    /// Applies the edits to `picker`.
    fn apply(self, picker: &mut DirectoryPicker) {
        if self.search_set {
            picker.search = self.search;
        }
        if self.url_set {
            picker.url = self.url;
        }
        if self.key_set {
            picker.key = self.key;
        }
        match self.chosen {
            Some(usize::MAX) => picker.choose(None),
            Some(ix) => picker.choose(Some(ix)),
            None => {}
        }
        if let Some(ix) = picker.chosen
            && let Some(provider) = picker.providers.get(ix)
        {
            if self.every {
                picker.picked = provider.models.iter().map(|m| m.id.clone()).collect();
            }
            if self.none {
                picker.picked.clear();
            }
        }
        if let Some(id) = self.flipped
            && !picker.picked.remove(&id)
        {
            picker.picked.insert(id);
        }
        if self.load {
            picker.asked.load = true;
        }
        if self.import {
            picker.asked.import = true;
        }
    }
}

/// One provider in the list: its name over its endpoint and model
/// count, the whole row one target, as the reference draws it.
fn provider_row(ui: &mut egui::Ui, name: &str, detail: &str, palette: &Palette) -> egui::Response {
    use crate::theme::{FAMILY_BODY_REGULAR, family, line_h};
    let name_h = line_h(FS_SM);
    let detail_h = line_h(FS_XS);
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 8.0 + name_h + 2.0 + detail_h + 8.0),
        egui::Sense::click(),
    );
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
    if response.hovered() {
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(crate::theme::R_MD as u8),
            palette.fill_hover,
        );
    }
    let width = rect.width() - 24.0;
    let x = rect.left() + 12.0;
    let line = |text: &str, size: f32, color: egui::Color32| {
        crate::ui::transcript::head::one_line(
            ui,
            text.to_owned(),
            egui::TextFormat::simple(egui::FontId::new(size, family(FAMILY_BODY_REGULAR)), color),
            width,
        )
    };
    let top = rect.top() + 8.0;
    let galley = line(name, FS_SM, palette.ink);
    let y = crate::theme::line_y(top, name_h, &galley);
    ui.painter().galley(egui::pos2(x, y), galley, palette.ink);
    let galley = line(detail, FS_XS, palette.muted);
    let y = crate::theme::line_y(top + name_h + 2.0, detail_h, &galley);
    ui.painter().galley(egui::pos2(x, y), galley, palette.muted);
    response
}

/// The entry provider `provider` makes with the models in `picked`.
fn picked(provider: &DirectoryProvider, picked: &BTreeSet<String>) -> Picked {
    let id: String = provider
        .id
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let models: Vec<Value> = provider
        .models
        .iter()
        .filter(|model| picked.contains(&model.id))
        .map(model_entry)
        .collect();
    let mut entry = Map::new();
    entry.insert("display_name".into(), json!(provider.name));
    entry.insert(
        "kind".into(),
        json!(provider.kind.as_deref().unwrap_or("openai")),
    );
    if let Some(api) = &provider.api {
        entry.insert("base_url".into(), json!(api));
    }
    if let Some(env) = provider.env.first() {
        entry.insert("api_key_env".into(), json!(env));
    }
    entry.insert("models".into(), Value::Array(models));
    Picked {
        id,
        entry: Value::Object(entry),
    }
}

/// One `[[providers.custom.<id>.models]]` entry for `model`.
fn model_entry(model: &DirectoryModel) -> Value {
    let mut entry = Map::new();
    entry.insert("id".into(), json!(model.id));
    entry.insert("name".into(), json!(model.name));
    if let Some(context) = model.context {
        entry.insert("context".into(), json!(context));
    }
    if let Some(output) = model.output {
        entry.insert("max_output".into(), json!(output));
    }
    if model.reasoning {
        entry.insert("reasoning".into(), json!(true));
    }
    if !model.input.is_empty() {
        entry.insert("input".into(), json!(model.input));
    }
    if model.produces.iter().any(|kind| kind != "text") {
        entry.insert("produces".into(), json!(model.produces));
    }
    if let Some(cost) = &model.cost {
        let mut prices = Map::new();
        prices.insert("input".into(), json!(cost.input));
        prices.insert("output".into(), json!(cost.output));
        if let Some(read) = cost.cache_read {
            prices.insert("cache_read".into(), json!(read));
        }
        if let Some(write) = cost.cache_write {
            prices.insert("cache_write".into(), json!(write));
        }
        entry.insert("cost".into(), Value::Object(prices));
    }
    Value::Object(entry)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use pure_client::wire::{DirectoryCost, DirectoryModel, DirectoryProvider};

    use super::picked;

    #[test]
    fn an_import_keeps_the_picked_models_with_their_prices() {
        let provider = DirectoryProvider {
            id: "Moon.AI".into(),
            name: "Moon AI".into(),
            api: Some("https://moon/v1".into()),
            env: vec!["MOON_KEY".into()],
            kind: None,
            models: vec![
                DirectoryModel {
                    id: "m1".into(),
                    name: "One".into(),
                    context: Some(128_000),
                    output: Some(8_000),
                    reasoning: true,
                    input: vec!["text".into()],
                    produces: vec!["text".into(), "image".into()],
                    cost: Some(DirectoryCost {
                        input: 1.0,
                        output: 2.0,
                        cache_read: Some(0.1),
                        cache_write: None,
                    }),
                },
                DirectoryModel {
                    id: "m2".into(),
                    name: "Two".into(),
                    ..DirectoryModel::default()
                },
            ],
        };
        let chosen: BTreeSet<String> = ["m1".to_owned()].into();
        let picked = picked(&provider, &chosen);
        assert_eq!(picked.id, "moon-ai");
        let entry = &picked.entry;
        assert_eq!(entry["kind"], "openai");
        assert_eq!(entry["api_key_env"], "MOON_KEY");
        assert_eq!(entry["models"].as_array().unwrap().len(), 1);
        let model = &entry["models"][0];
        assert_eq!(model["max_output"], 8_000);
        assert_eq!(model["reasoning"], true);
        assert_eq!(model["cost"]["cache_read"], 0.1);
        assert!(model["cost"].get("cache_write").is_none());
    }

    #[test]
    fn an_import_without_a_choice_has_nothing_to_carry() {
        let provider = DirectoryProvider {
            id: "moon".into(),
            name: "Moon".into(),
            ..DirectoryProvider::default()
        };
        let picked = picked(&provider, &BTreeSet::new());
        assert!(picked.entry["models"].as_array().unwrap().is_empty());
    }
}

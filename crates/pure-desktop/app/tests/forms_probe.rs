//! Headless E07 probes: the forms the settings pages open. A page that
//! draws its Add button but cannot open the form behind it is a dead
//! control, so each button is pressed by the pointer, the form's own
//! widgets are read, and the writes the form queues are followed out
//! of the store's outgoing frames.

use egui_kittest::{Harness, kittest::Queryable};
use pure_desktop::store::Store;
use pure_desktop::ui::settings::{Section, SettingsEvent, SettingsView};

mod testutil;

/// The directory models.dev answers with, as the picker lists it.
fn directory_answer() -> serde_json::Value {
    serde_json::json!({
        "providers": [{
            "id": "lab",
            "name": "Lab",
            "api": "http://lab:8080",
            "env": ["LAB_KEY"],
            "kind": "openai",
            "models": [{
                "id": "m1", "name": "Lab One", "context": 128000, "output": 4096,
                "cost": {"input": 1.0, "output": 2.0, "cacheRead": 0.5, "cacheWrite": 2.5}
            }]
        }]
    })
}

/// The configuration snapshot the recorded engine answers with.
fn config_answer() -> serde_json::Value {
    serde_json::json!({
        "providers": {
            "custom": {"lab": {"base_url": "http://lab:8080", "api_key_env": "LAB_KEY",
                "models": [{"id": "m1", "name": "Lab One", "context": 128000}]}},
            "deepseek": {"api_key_env": "DS", "headers": {"X-Org": "x"}}
        },
        "providerKeys": {"lab": {"env": "LAB_KEY", "source": "auth"}},
        "mcp": {"servers": {
            "lab": {"command": "lab-mcp", "args": ["--port", "9"]},
            "web": {"url": "https://mcp.example.com"}
        }},
        "acp": {"agents": {
            "claude": {"command": "npx", "args": ["-y", "claude-code-acp"]}
        }},
        "permissions": {
            "tools": {"shell": {"default": "ask", "allow": ["cargo *"], "deny": ["rm *"]}}
        },
        "plugins": {
            "dir": "/plug",
            "enabled": ["lab"],
            "capabilities": {"lab": ["net"], "web": ["fs"]},
            "config": {"lab": {"key": 1}}
        }
    })
}

/// The probe's harness state: the store, the dialog, what it asked
/// for, the config requests already answered, and the one-frame flag.
type Probe = (
    Store,
    SettingsView,
    Vec<SettingsEvent>,
    Vec<pure_client::RequestId>,
    bool,
);

/// A harness over the settings dialog with `store`'s session open.
fn harness_for(store: Store) -> Harness<'static, Probe> {
    Harness::builder()
        .with_size(egui::Vec2::new(1000.0, 760.0))
        .build_ui_state(
            |ui, (store, settings, events, answered, installed): &mut Probe| {
                if testutil::theme_installed(installed, ui.ctx()) {
                    return;
                }
                events.extend(settings.show(
                    ui,
                    store,
                    &pure_desktop::ui::settings::UpdateState::default(),
                ));
                if settings.is_open() && settings.section().reads_config() {
                    store.ask_config();
                }
                // The engine answers both reads the settings pages make:
                // the configuration snapshot and the provider directory
                // the picker lists.
                let outgoing = store.take_outgoing();
                let mut answers: Vec<(pure_client::RequestId, serde_json::Value)> = Vec::new();
                for frame in &outgoing {
                    if let pure_client::Frame::Request { id, method, .. } = frame
                        && !answered.contains(id)
                    {
                        let result = match method.as_str() {
                            "_pure/config/get" => {
                                answered.push(id.clone());
                                Some(config_answer())
                            }
                            "_pure/providers/directory" => {
                                answered.push(id.clone());
                                Some(directory_answer())
                            }
                            _ => None,
                        };
                        if let Some(result) = result {
                            answers.push((id.clone(), result));
                        }
                    }
                }
                for (id, result) in answers {
                    store.absorb(pure_client::Frame::Success { id, result });
                }
            },
            (store, SettingsView::new(), Vec::new(), Vec::new(), false),
        )
}

/// A store with one open session.
fn booted() -> Store {
    let mut store = Store::new("/w", false);
    store.set_connect(pure_desktop::transport::State::Connected);
    store.handshake(false);
    store.new_session();
    let _ = store.take_outgoing();
    store.absorb(pure_client::Frame::Success {
        id: pure_client::RequestId::Number(2),
        result: serde_json::json!({"sessionId": "s1"}),
    });
    let _ = store.take_outgoing();
    store.set_active("s1");
    store
}

/// A store whose session offers the model and mode options, so the
/// config pages read them beside the snapshot the harness answers.
fn configured() -> Store {
    let mut store = booted();
    store.absorb(pure_client::Frame::Notification {
        method: "session/update".to_owned(),
        params: serde_json::json!({
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "config_option_update",
                "configOptions": [{
                "id": "model", "name": "Model", "type": "select", "currentValue": "anthropic:a",
                "options": [
                    {"value": "anthropic:a", "name": "Claude A"},
                    {"value": "anthropic:b", "name": "Claude B"},
                    {"value": "lab:m1", "name": "Lab One"}
                ]
            }, {
                "id": "mode", "name": "Mode", "type": "select", "currentValue": "ask",
                "options": [
                    {"value": "plan", "name": "Plan"},
                    {"value": "ask", "name": "Ask"},
                    {"value": "allow", "name": "Allow"}
                ]
            }]
            }
        }),
    });
    store
}

/// Runs two frames and drains what the store sent.
fn settle(harness: &mut Harness<'_, Probe>) {
    harness.run();
    harness.run();
    let _ = harness.state_mut().0.take_outgoing();
}

#[test]
fn the_providers_page_add_button_opens_the_provider_form() {
    let mut harness = harness_for(configured());
    harness.state_mut().1.open(Section::Providers);
    settle(&mut harness);
    harness.get_by_label("Add provider").click();
    harness.run();
    assert_eq!(
        testutil::count(&harness, "Custom"),
        1,
        "the chooser offers a custom endpoint"
    );
    harness.get_by_label("Custom").click();
    harness.run();
    assert_eq!(
        testutil::count(&harness, "Add provider"),
        1,
        "the provider form draws over the page"
    );
    assert_eq!(
        testutil::count(&harness, "Base URL"),
        1,
        "the endpoint field draws"
    );
    assert_eq!(
        testutil::count(&harness, "Models"),
        1,
        "the models table draws"
    );
}

#[test]
fn a_provider_row_opens_the_form_on_that_provider() {
    let mut harness = harness_for(configured());
    harness.state_mut().1.open(Section::Providers);
    settle(&mut harness);
    harness.get_by_label("lab").click();
    harness.run();
    assert_eq!(
        testutil::count(&harness, "Edit lab"),
        1,
        "the form names the custom provider it edits"
    );
    assert!(
        testutil::field_holds(&harness, "http://lab:8080"),
        "the endpoint field carries the saved value"
    );
}

#[test]
fn the_forget_saved_key_button_stays_inside_the_page_column() {
    // The key row lays itself out right-to-left, so a field drawn
    // before the button once pushed the button past the column's left
    // edge, and the vertical layout followed it, dragging every later
    // label out of the card with it.
    let mut harness = harness_for(configured());
    harness.state_mut().1.open(Section::Providers);
    settle(&mut harness);
    harness.get_by_label("lab").click();
    harness.run();
    // The saved key makes paste the default source, so the button
    // draws with the field.
    let forget = harness
        .query_by_label("Forget saved key")
        .expect("a saved key draws the forget button");
    // The card sits at x 20..980 and the nav ends at 244, so anything
    // left of 244 is over the nav, not the form.
    assert!(
        forget.rect().left() > 244.0,
        "the forget button stays inside the page column: {:?}",
        forget.rect()
    );
    let base = harness
        .query_by_label("Base URL")
        .expect("the endpoint field draws under the key row");
    assert!(
        base.rect().left() > 244.0,
        "the key row's overflow must not drag the labels after it: {:?}",
        base.rect()
    );
}

#[test]
fn the_mcp_page_add_button_opens_the_server_form() {
    let mut harness = harness_for(configured());
    harness.state_mut().1.open(Section::Mcp);
    settle(&mut harness);
    assert_eq!(
        testutil::count(&harness, "Add server"),
        1,
        "the page draws its add button"
    );
    harness.get_by_label("Add server").click();
    harness.run();
    assert_eq!(
        testutil::count(&harness, "Add MCP server"),
        1,
        "the MCP form draws over the page"
    );
    assert_eq!(
        testutil::count(&harness, "Add server"),
        0,
        "the page it opened from is gone"
    );
    assert_eq!(
        testutil::count(&harness, "Transport"),
        1,
        "the transport choice draws"
    );
    assert_eq!(
        testutil::count(&harness, "stdio"),
        1,
        "the stdio segment draws"
    );
}

#[test]
fn the_chooser_opens_the_acp_agent_form() {
    let mut harness = harness_for(configured());
    harness.state_mut().1.open(Section::Providers);
    settle(&mut harness);
    assert_eq!(
        testutil::count(&harness, "acp:claude"),
        1,
        "the configured agent lists with the providers"
    );
    harness.get_by_label("Add provider").click();
    harness.run();
    harness.get_by_label("ACP agent").click();
    harness.run();
    assert_eq!(
        testutil::count(&harness, "Add ACP agent"),
        1,
        "the agent form draws"
    );
    assert_eq!(
        testutil::count(&harness, "Command"),
        1,
        "the command field draws"
    );
    assert_eq!(
        testutil::count(&harness, "Arguments"),
        1,
        "the arguments field draws"
    );
    assert_eq!(
        testutil::count(&harness, "Test handshake"),
        1,
        "the handshake button draws"
    );
}

#[test]
fn an_acp_agent_row_opens_the_form_on_that_agent() {
    let mut harness = harness_for(configured());
    harness.state_mut().1.open(Section::Providers);
    settle(&mut harness);
    harness.get_by_label("acp:claude").click();
    harness.run();
    assert_eq!(
        testutil::count(&harness, "Edit acp:claude"),
        1,
        "the form names the agent it edits"
    );
    assert!(
        testutil::field_holds(&harness, "npx"),
        "the command field carries its value"
    );
}

#[test]
fn the_directory_button_opens_the_models_dev_picker() {
    let mut harness = harness_for(configured());
    harness.state_mut().1.open(Section::Providers);
    settle(&mut harness);
    harness.get_by_label("Add provider").click();
    harness.run();
    harness.get_by_label("From models.dev").click();
    harness.run();
    assert_eq!(
        testutil::count(&harness, "Import from models.dev"),
        1,
        "the picker draws under the button that opened it"
    );
    assert_eq!(
        testutil::count(&harness, "Search"),
        1,
        "the search field draws"
    );
    assert_eq!(
        testutil::count(&harness, "Reading the directory"),
        0,
        "the answer landed: the picker lists its providers"
    );
    assert!(
        testutil::count(&harness, "Lab") >= 1,
        "the provider the directory lists draws"
    );
}

#[test]
fn the_escape_key_leaves_a_form_before_the_dialog() {
    let mut harness = harness_for(configured());
    harness.state_mut().1.open(Section::Mcp);
    settle(&mut harness);
    harness.get_by_label("Add server").click();
    harness.run();
    assert_eq!(
        testutil::count(&harness, "Add MCP server"),
        1,
        "the form draws before the key"
    );
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert_eq!(
        testutil::count(&harness, "Add server"),
        1,
        "the key left the form and the page is back"
    );
    assert!(harness.state().1.is_open(), "the dialog itself stayed open");
}

/// Renders every settings page to `/tmp/settings-<page>.png` for a
/// side-by-side against the reference, when `PURE_PROBE_PNG` is set.
#[test]
fn every_page_renders_for_review() {
    if std::env::var_os("PURE_PROBE_PNG").is_none() {
        return;
    }
    for section in Section::ALL {
        let mut harness = harness_for(configured());
        harness.state_mut().1.open(section);
        settle(&mut harness);
        if section == Section::Providers {
            // The logos land from models.dev on threads of their own.
            for _ in 0..30 {
                std::thread::sleep(std::time::Duration::from_millis(200));
                harness.run();
            }
        }
        let name = section.label().to_lowercase().replace([' ', '&'], "-");
        let image = harness.render().expect("the page renders");
        image
            .save(format!("/tmp/settings-{name}.png"))
            .expect("the page image saves");
    }
    let forms: [(&str, Section, &[&str]); 4] = [
        ("provider", Section::Providers, &["Add provider", "Custom"]),
        ("acp", Section::Providers, &["Add provider", "ACP agent"]),
        (
            "directory",
            Section::Providers,
            &["Add provider", "From models.dev"],
        ),
        ("mcp", Section::Mcp, &["Add server"]),
    ];
    for (name, section, clicks) in forms {
        let mut harness = harness_for(configured());
        harness.state_mut().1.open(section);
        settle(&mut harness);
        for label in clicks {
            harness.get_by_label(label).click();
            harness.run();
        }
        harness.run();
        let image = harness.render().expect("the form renders");
        image
            .save(format!("/tmp/settings-form-{name}.png"))
            .expect("the form image saves");
    }
}

#[test]
fn a_focused_form_field_draws_the_reference_focus_border() {
    use egui_kittest::kittest::NodeT as _;
    let mut harness = harness_for(configured());
    harness.state_mut().1.open(Section::Mcp);
    settle(&mut harness);
    harness.get_by_label("Add server").click();
    harness.run();
    testutil::first_field(&harness)
        .expect("the name field draws")
        .click();
    harness.run();
    harness.run();
    let field = testutil::first_field(&harness)
        .expect("the name field draws")
        .accesskit_node()
        .bounding_box()
        .expect("the field has bounds");
    let image = harness.render().expect("the form renders");
    let ink = pure_desktop::theme::Palette::shadow().ink_strong;
    let x = field.x0.round() as u32;
    let mid = ((field.y0 + field.y1) / 2.0).round() as u32;
    // The border's column, across the field's middle: the reference
    // paints its focus line in the accent, never the strong ink egui
    // gives a focused field by default.
    let stark = (mid - 4..mid + 4)
        .filter(|&y| {
            let [r, g, b, _] = image.get_pixel(x, y).0;
            r.abs_diff(ink.r()) < 24 && g.abs_diff(ink.g()) < 24 && b.abs_diff(ink.b()) < 24
        })
        .count();
    assert_eq!(
        stark, 0,
        "the focused border is not drawn in the strong ink"
    );
}

#[test]
fn a_directory_row_chooses_its_provider_from_its_detail_line() {
    use egui_kittest::kittest::NodeT as _;
    let mut harness = harness_for(configured());
    harness.state_mut().1.open(Section::Providers);
    settle(&mut harness);
    harness.get_by_label("Add provider").click();
    harness.run();
    harness.get_by_label("From models.dev").click();
    harness.run();
    harness.run();
    let row = harness
        .get_by_label("Lab")
        .accesskit_node()
        .bounding_box()
        .expect("the row has bounds");
    // The row is one target holding both lines, as the reference's is,
    // so a press on its second line chooses the provider.
    assert!(
        row.height() >= 44.0,
        "the row holds the name and the detail: {row:?}"
    );
    let detail = egui::pos2(row.x0 as f32 + 20.0, row.y1 as f32 - 8.0);
    harness.hover_at(detail);
    harness.drag_at(detail);
    harness.drop_at(detail);
    harness.run();
    assert_eq!(
        testutil::count(&harness, "Select all"),
        1,
        "the provider's models show"
    );
}

#[test]
fn a_pasted_key_is_masked_as_it_is_typed() {
    let mut harness = harness_for(configured());
    harness.state_mut().1.open(Section::Providers);
    settle(&mut harness);
    harness.get_by_label("lab").click();
    harness.run();
    let row = harness
        .query_by_label("Forget saved key")
        .expect("a saved key draws the key row")
        .rect();
    harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::PasswordInput
                || node.role() == egui::accesskit::Role::TextInput
        })
        .find(|node| node.rect().y_range().contains(row.center().y))
        .expect("the key field sits beside the forget button")
        .click();
    harness.run();
    harness.event(egui::Event::Text("sk-secret".to_owned()));
    harness.run();
    let shown = harness
        .query_all_by(|node| node.role() == egui::accesskit::Role::PasswordInput)
        .count();
    assert_eq!(shown, 1, "the key field is a password field");
    assert!(
        !testutil::field_values(&harness)
            .iter()
            .any(|value| value.contains("sk-secret")),
        "the typed key never shows in the clear"
    );
}

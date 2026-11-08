//! Headless probes for the engine-backed switches: turning a plugin off
//! writes `plugins.<name>.enabled` and the permission page's access cards
//! write `permissions.mode`.

use egui_kittest::{Harness, kittest::Queryable};
use pure_desktop::store::Store;
use pure_desktop::ui::settings::{Section, SettingsEvent, SettingsView};

mod testutil;

fn config_answer() -> serde_json::Value {
    serde_json::json!({
        "providers": {"custom": {}},
        "mcp": {"servers": {}},
        "permissions": {
            "mode": "default",
            "confine_paths": false,
            "tools": {},
            "mcp": {}
        },
        "plugins": {"dir": "/plug", "enabled": [], "disabled": [], "capabilities": {}, "config": {}},
        "installedPlugins": [
            {"name": "goal", "enabled": true, "description": "Session goal"},
            {"name": "todo", "enabled": true, "description": "Todo list"}
        ]
    })
}

type Probe = (
    Store,
    SettingsView,
    Vec<SettingsEvent>,
    Vec<pure_client::RequestId>,
    bool,
    Vec<pure_client::Frame>,
);

fn harness_for(store: Store) -> Harness<'static, Probe> {
    Harness::builder()
        .with_size(egui::Vec2::new(1000.0, 1000.0))
        .build_ui_state(
            |ui, (store, settings, events, answered, installed, sent): &mut Probe| {
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
                    let outgoing = store.take_outgoing();
                    let mut answers: Vec<(pure_client::RequestId, serde_json::Value)> = Vec::new();
                    for frame in &outgoing {
                        if let pure_client::Frame::Request { id, method, .. } = frame
                            && method == "_pure/config/get"
                            && !answered.contains(id)
                        {
                            answered.push(id.clone());
                            answers.push((id.clone(), config_answer()));
                        }
                    }
                    for (id, result) in answers {
                        store.absorb(pure_client::Frame::Success { id, result });
                    }
                    sent.extend(outgoing);
                }
            },
            (
                store,
                SettingsView::new(),
                Vec::new(),
                Vec::new(),
                false,
                Vec::new(),
            ),
        )
}

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

/// Whether the store sent `_pure/config/set` for `path` with `value`.
fn wrote(frames: &[pure_client::Frame], path: serde_json::Value, value: serde_json::Value) -> bool {
    frames.iter().any(|frame| {
        matches!(frame, pure_client::Frame::Request { method, params, .. }
            if method == "_pure/config/set"
                && params["path"] == path
                && params["value"] == value)
    })
}

#[test]
fn a_plugin_switch_writes_its_enabled_state() {
    let mut harness = harness_for(booted());
    harness.state_mut().1.open(Section::Plugins);
    harness.run();
    harness.run();
    assert_eq!(testutil::count(&harness, "todo"), 1, "the todo row draws");
    harness.get_by_label("Toggle todo").click();
    harness.run();
    assert!(
        wrote(
            &harness.state().5,
            serde_json::json!(["plugins", "todo", "enabled"]),
            serde_json::json!(false),
        ),
        "turning todo off writes plugins.todo.enabled=false"
    );
}

#[test]
fn the_access_card_writes_full_access() {
    let mut harness = harness_for(booted());
    harness.state_mut().1.open(Section::Permissions);
    harness.run();
    harness.run();
    harness.get_by_label("Full access").click();
    harness.run();
    assert!(
        wrote(
            &harness.state().5,
            serde_json::json!(["permissions", "mode"]),
            serde_json::json!("full"),
        ),
        "the Full access card writes permissions.mode=full"
    );
}

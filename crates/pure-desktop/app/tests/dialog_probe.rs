//! Headless E07 probes: the dialog layer draws over the shell, takes
//! the Esc key, and its controls write through to the store. A dialog
//! that draws but cannot be clicked is the E05 frame bug over again,
//! so each control is pressed by the pointer and the store's outgoing
//! frame is read.

use egui_kittest::{Harness, kittest::Queryable};
use pure_desktop::store::Store;
use pure_desktop::ui::dialog::{DialogEvent, DialogKind, Dialogs};

mod testutil;

/// A harness over the dialog layer with `store`'s session open.
fn harness_for(store: Store) -> Harness<'static, (Store, Dialogs, Vec<DialogEvent>, bool)> {
    Harness::builder()
        .with_size(egui::Vec2::new(900.0, 700.0))
        .build_ui_state(
        |ui, (store, dialogs, events, installed): &mut (Store, Dialogs, Vec<DialogEvent>, bool)| {
            if testutil::theme_installed(installed, ui.ctx()) {
                return;
            }
            let asked = dialogs.show(ui, store, std::time::Instant::now());
            events.extend(asked);
        },
        (store, Dialogs::new(), Vec::new(), false),
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

#[test]
fn the_goal_dialog_draws_over_the_shell() {
    let mut store = booted();
    let mut dialogs = Dialogs::new();
    dialogs.show_dialog(DialogKind::Goal, &mut store);
    let mut harness = harness_for(store);
    harness.state_mut().1 = dialogs;
    harness.run();
    harness.run();
    assert!(
        harness.query_by_label("Set a goal").is_some(),
        "the dialog names itself"
    );
    assert!(
        harness.query_by_label("Set goal").is_some(),
        "the primary button draws"
    );
    assert!(
        harness.query_by_label("Cancel").is_some(),
        "the cancel button draws"
    );
}

#[test]
fn the_goal_button_writes_the_option_through_the_store() {
    let mut store = booted();
    let mut dialogs = Dialogs::new();
    dialogs.show_dialog(DialogKind::Goal, &mut store);
    let mut harness = harness_for(store);
    harness.state_mut().1 = dialogs;
    harness.run();
    harness.run();
    harness.get_by_label("Set goal").click();
    harness.run();
    let (store, dialogs, events, _) = harness.state_mut();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, DialogEvent::SaveGoal(_))),
        "the button asks for the goal to be saved"
    );
    assert!(!dialogs.is_open(), "the dialog closes after its action");
    let _ = store;
}

#[test]
fn the_swarm_dialog_offers_the_default_mode_under_ask() {
    let mut store = booted();
    store.absorb(pure_client::Frame::Notification {
        method: "session/update".to_owned(),
        params: serde_json::json!({
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "config_option_update",
                "configOptions": [{
                    "id": "mode", "name": "Mode", "category": "mode",
                    "type": "select", "currentValue": "ask",
                    "options": [
                        {"value": "default", "name": "Default"},
                        {"value": "ask", "name": "Ask first"}
                    ]
                }]
            },
        }),
    });
    let mut dialogs = Dialogs::new();
    dialogs.show_dialog(DialogKind::ConfirmSwarm, &mut store);
    let mut harness = harness_for(store);
    harness.state_mut().1 = dialogs;
    harness.run();
    harness.run();
    assert!(
        harness.query_by_label("Enable swarm mode?").is_some(),
        "the dialog names itself"
    );
    harness.get_by_label("Enable with Default").click();
    harness.run();
    let (_, dialogs, events, _) = harness.state();
    assert!(
        events.iter().any(
            |event| matches!(event, DialogEvent::EnableSwarm(Some(mode)) if mode == "default")
        ),
        "the offer turns swarm on under the default mode"
    );
    assert!(!dialogs.is_open());
}

#[test]
fn the_swarm_button_keeps_the_mode() {
    let mut store = booted();
    let mut dialogs = Dialogs::new();
    dialogs.show_dialog(DialogKind::ConfirmSwarm, &mut store);
    let mut harness = harness_for(store);
    harness.state_mut().1 = dialogs;
    harness.run();
    harness.run();
    assert!(
        harness.query_by_label("Enable with Default").is_none(),
        "the offer only shows under the mode that asks"
    );
    harness.get_by_label("Enable swarm").click();
    harness.run();
    let (_, _, events, _) = harness.state();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, DialogEvent::EnableSwarm(None))),
        "the button turns swarm on without touching the mode"
    );
}

#[test]
fn the_layer_draws_nothing_while_no_dialog_is_open() {
    let mut harness = harness_for(booted());
    harness.run();
    harness.run();
    assert!(
        harness.query_by_label("Set a goal").is_none(),
        "no dialog draws"
    );
    assert!(
        harness.query_by_label("Enable swarm").is_none(),
        "no swarm button draws"
    );
}

#[test]
fn cancel_closes_the_dialog_without_an_event() {
    let mut store = booted();
    let mut dialogs = Dialogs::new();
    dialogs.show_dialog(DialogKind::Goal, &mut store);
    let mut harness = harness_for(store);
    harness.state_mut().1 = dialogs;
    harness.run();
    harness.run();
    harness.get_by_label("Cancel").click();
    harness.run();
    let (_, dialogs, events, _) = harness.state();
    assert!(!dialogs.is_open(), "cancel closes the dialog");
    assert!(events.is_empty(), "cancel asks for nothing");
}

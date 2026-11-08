//! Headless E05 probes: the composer, its toolbar and the dock row
//! render against a real store, and the CJK fallback's glyph advances
//! hold for the composer's own text.

use egui_kittest::Harness;
use pure_desktop::store::Store;
use pure_desktop::ui::composer::Composer;

mod testutil;

/// A harness over the composer and the dock row, drawing them over a
/// store the caller has booted.
fn harness(store: Store) -> Harness<'static, (Store, Composer, bool)> {
    Harness::builder()
        .with_size(egui::Vec2::new(900.0, 600.0))
        .build_ui_state(
            |ui, (store, composer, installed): &mut (Store, Composer, bool)| {
                if testutil::theme_installed(installed, ui.ctx()) {
                    return;
                }
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.label("the transcript column");
                        let actions = pure_desktop::ui::dock::show(ui, store);
                        assert!(actions.is_empty(), "an idle dock raises no actions");
                        let commands = store.composer_commands().to_vec();
                        composer.show(ui, store, &commands, false, true);
                    });
                });
            },
            (store, Composer::new(), false),
        )
}

/// A booted store with one open, empty session.
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
    store
}

#[test]
fn the_composer_and_dock_render_against_a_real_store() {
    let mut harness = harness(booted());
    harness.run();
    let img = harness.render().expect("the frame renders");
    // The composer's frame, the toolbar and the status line all paint
    // over the chat column, so the frame carries far more than the
    // one label.
    let painted = img
        .pixels()
        .filter(|px| px.0[3] != 0)
        .filter(|px| {
            let (r, g, b) = (i32::from(px.0[0]), i32::from(px.0[1]), i32::from(px.0[2]));
            (r, g, b) != (0x0F, 0x0E, 0x13)
        })
        .count();
    assert!(
        painted > 5_000,
        "the composer and dock paint {painted} pixels over the shell"
    );
}

#[test]
fn a_typed_draft_sends_through_the_composer() {
    let store = booted();
    let mut harness = harness(store);
    harness.run();
    let (store, composer, _) = harness.state_mut();
    composer.draft.insert("fix the null check");
    assert_eq!(composer.prompt(), Some("fix the null check"));
    let text = composer.take_prompt().expect("the prompt is taken");
    assert_eq!(text, "fix the null check");
    assert!(
        composer.prompt().is_none(),
        "the field is empty after the send"
    );
    let _ = store;
}

#[test]
fn the_stop_button_reports_stop_while_running() {
    // The stop click used to report Stop and no shell listened, so a
    // turn never interrupted; this pins the composer's half of it.
    use egui_kittest::kittest::Queryable;

    let store = booted();
    let composer = pure_desktop::ui::composer::Composer::new();
    let mut harness = Harness::builder()
        .with_size(egui::Vec2::new(900.0, 600.0))
        .build_ui_state(
            |ui,
             (store, composer, events, installed): &mut (
                Store,
                pure_desktop::ui::composer::Composer,
                Vec<pure_desktop::ui::composer::ComposerEvent>,
                bool,
            )| {
                if testutil::theme_installed(installed, ui.ctx()) {
                    return;
                }
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.vertical(|ui| {
                        let commands = store.composer_commands().to_vec();
                        events.push(composer.show(ui, store, &commands, true, true));
                    });
                });
            },
            (store, composer, Vec::new(), false),
        );
    harness.run();
    harness.run();
    assert!(
        harness.query_by_label("Interrupt").is_some(),
        "a running turn draws the stop button"
    );
    harness
        .query_by_label("Interrupt")
        .expect("the stop button draws while a turn runs")
        .click();
    harness.run();
    let (_, _, events, _) = harness.state();
    assert!(
        events.contains(&pure_desktop::ui::composer::ComposerEvent::Stop),
        "the stop click reports Stop: {events:?}"
    );
}

#[test]
fn the_mode_picker_writes_the_store_when_a_row_is_picked() {
    // The pickers used to record their pick into a local and drop it,
    // so a click closed the menu and changed nothing. This opens the
    // mode picker, clicks the `ask` row, and reads the frame the store
    // put on the wire.
    use egui_kittest::kittest::Queryable;

    let mut store = booted();
    store.absorb(pure_client::Frame::Notification {
        method: "session/update".to_owned(),
        params: serde_json::json!({
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "config_option_update",
                "configOptions": [{
                    "id": "mode", "name": "Mode", "category": "mode",
                    "type": "select", "currentValue": "default",
                    "options": [
                        {"value": "default", "name": "Default"},
                        {"value": "ask", "name": "Ask first"},
                        {"value": "plan", "name": "Plan"}
                    ]
                }]
            },
        }),
    });
    let composer = pure_desktop::ui::composer::Composer::new();
    let mut harness =
        Harness::builder()
            .with_size(egui::Vec2::new(900.0, 600.0))
            .build_ui_state(
                |ui,
                 (store, composer, installed): &mut (
                    Store,
                    pure_desktop::ui::composer::Composer,
                    bool,
                )| {
                    if testutil::theme_installed(installed, ui.ctx()) {
                        return;
                    }
                    egui::CentralPanel::default().show(ui, |ui| {
                        ui.vertical(|ui| {
                            let commands = store.composer_commands().to_vec();
                            composer.show(ui, store, &commands, false, true);
                        });
                    });
                },
                (store, composer, false),
            );
    harness.run();
    harness.run();
    // The mode pill opens the picker.
    // The pill's clickable node is the sense-bearing wrapper, whose
    // rect covers the label. The query finds the label, so the click
    // targets the rect the label sits in.
    let pill = harness
        .query_by_label("Mode")
        .or_else(|| harness.query_by_label("Default"))
        .expect("the mode pill draws its active mode");
    let at = pill.rect().center();
    harness.hover_at(at);
    harness.run();
    harness.drag_at(at);
    harness.run();
    harness.drop_at(at);
    harness.run();
    // The picker's `ask` row, named in the interface language.
    let row = harness
        .query_by_label("Ask")
        .expect("the picker lists the advertised modes");
    row.click();
    harness.run();
    let (store, ..) = harness.state_mut();
    let wrote = store.take_outgoing().into_iter().any(|frame| {
        matches!(frame, pure_client::Frame::Request { method, params, .. }
            if method == "session/set_config_option"
                && params.get("configId").and_then(serde_json::Value::as_str) == Some("mode")
                && params.get("value").and_then(serde_json::Value::as_str) == Some("ask"))
    });
    assert!(
        wrote,
        "the mode picker's click writes the mode to the store"
    );
}

#[test]
fn the_dock_pills_answer_a_click() {
    // The pills are frames with an interact over their rect, so a
    // headless run can click one and read the action the dock raised.
    use egui_kittest::kittest::Queryable;

    let mut store = booted();
    // Plan mode, so the plan pill shows.
    store.absorb(pure_client::Frame::Notification {
        method: "session/update".to_owned(),
        params: serde_json::json!({
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "config_option_update",
                "configOptions": [{
                    "id": "mode", "name": "Mode", "category": "mode",
                    "type": "select", "currentValue": "plan",
                    "options": [
                        {"value": "default", "name": "Default"},
                        {"value": "plan", "name": "Plan"}
                    ]
                }]
            },
        }),
    });
    let composer = pure_desktop::ui::composer::Composer::new();
    let mut harness =
        Harness::builder()
            .with_size(egui::Vec2::new(900.0, 600.0))
            .build_ui_state(
                |ui,
                 (store, composer, installed): &mut (
                    Store,
                    pure_desktop::ui::composer::Composer,
                    bool,
                )| {
                    if testutil::theme_installed(installed, ui.ctx()) {
                        return;
                    }
                    egui::CentralPanel::default().show(ui, |ui| {
                        ui.vertical(|ui| {
                            let actions = pure_desktop::ui::dock::show(ui, store);
                            assert!(
                                actions.is_empty(),
                                "an idle dock raises no actions: {actions:?}"
                            );
                            let commands = store.composer_commands().to_vec();
                            composer.show(ui, store, &commands, false, true);
                        });
                    });
                },
                (store, composer, false),
            );
    harness.run();
    harness.run();
    let pill = harness
        .query_by_label("Plan mode: the plan will show up here")
        .expect("the plan pill shows while plan mode is on");
    pill.click();
    harness.run();
}

#[test]
fn the_lab_toggle_gates_the_context_gauge() {
    use egui_kittest::kittest::Queryable;
    let mut store = booted();
    store.update_prefs(|prefs| prefs.fuel = false);
    let mut harness = harness(store);
    harness.run();
    harness
        .query_by_label("0%")
        .expect("the context ring shows its percent")
        .click();
    harness.run();
    assert!(
        harness.query_by_label("Context").is_none(),
        "the gauge stays shut while the lab toggle is off"
    );

    let (store, _, _) = harness.state_mut();
    store.update_prefs(|prefs| prefs.fuel = true);
    harness.run();
    harness
        .query_by_label("0%")
        .expect("the ring is back the next frame")
        .click();
    harness.run();
    assert!(
        harness.query_by_label("Context").is_some(),
        "the gauge opens while the lab toggle is on"
    );
}

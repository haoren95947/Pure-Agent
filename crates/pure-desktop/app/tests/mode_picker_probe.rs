//! Headless probes: the permission mode and model pickers open above
//! their pills over the recorded session, inside the window, and the
//! mode picker lists the engine's modes.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use pure_desktop::store::{Command, Store};
use pure_desktop::transport::{State, replay};
use pure_desktop::ui::toolbar::{self, Toolbar};

mod testutil;

/// Carries out the store's pending commands the way the shell does.
fn run_commands(store: &mut Store) {
    for command in store.take_commands() {
        match command {
            Command::Handshake { replay_sessions } => store.handshake(replay_sessions),
            Command::NewSession => store.new_session(),
            Command::ReplayPrompt => {
                store.prompt("fix the null check");
            }
        }
    }
}

/// A store driven through the recording the way the replay smoke runs.
fn replayed_store() -> Store {
    let mut store = Store::new("/w", true);
    store.set_connect(State::Connected);
    run_commands(&mut store);
    let _ = store.take_outgoing();
    let frames = replay::transcript();
    for frame in &frames[..2] {
        store.absorb(frame.clone());
        run_commands(&mut store);
        let _ = store.take_outgoing();
    }
    for frame in &frames[2..] {
        store.absorb(frame.clone());
    }
    store
}

type Pane = (Store, Toolbar, bool, bool);

const SIZE: egui::Vec2 = egui::Vec2::new(900.0, 600.0);

fn harness(which: toolbar::Picker) -> Harness<'static, Pane> {
    Harness::builder().with_size(SIZE).build_ui_state(
        move |ui, (store, toolbar, opened, installed): &mut Pane| {
            if testutil::theme_installed(installed, ui.ctx()) {
                return;
            }
            if !*opened {
                toolbar.toggle_picker(which);
                *opened = true;
            }
            egui::CentralPanel::default().show(ui, |ui| {
                let rect = egui::Rect::from_min_size(
                    egui::pos2(40.0, SIZE.y - 60.0),
                    egui::vec2(800.0, 40.0),
                );
                let mut strip = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(rect)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                toolbar::show(&mut strip, toolbar, store, false, false);
            });
        },
        (replayed_store(), Toolbar::new(), false, false),
    )
}

#[test]
fn the_mode_picker_lists_the_modes_above_its_pill() {
    let mut harness = harness(toolbar::Picker::Mode);
    for _ in 0..4 {
        harness.run();
    }
    if std::env::var_os("PURE_PROBE_PNG").is_some() {
        let image = harness.render().expect("the probe renders");
        image
            .save("/tmp/mode-probe.png")
            .expect("the probe image saves");
    }
    for mode in ["Default", "Ask", "Allow", "Deny"] {
        assert!(
            harness.query_all_by_label(mode).count() > 0,
            "the picker lists {mode}"
        );
    }
    let card = harness
        .ctx
        .memory(|memory| memory.area_rect(egui::Id::new("mode-picker")))
        .expect("the picker is up");
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE);
    assert!(
        screen.contains_rect(card),
        "the picker {card:?} stays inside the window"
    );
    assert!(
        card.center().y < SIZE.y - 60.0,
        "the picker opens above the strip"
    );
}

#[test]
fn the_model_picker_opens_above_its_pill_inside_the_window() {
    let mut harness = harness(toolbar::Picker::Model);
    for _ in 0..4 {
        harness.run();
    }
    if std::env::var_os("PURE_PROBE_PNG").is_some() {
        let image = harness.render().expect("the probe renders");
        image
            .save("/tmp/model-probe.png")
            .expect("the probe image saves");
    }
    let card = harness
        .ctx
        .memory(|memory| memory.area_rect(egui::Id::new("model-picker")))
        .expect("the picker is up");
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE);
    assert!(
        screen.contains_rect(card),
        "the picker {card:?} stays inside the window"
    );
    assert!(
        card.center().y < SIZE.y - 60.0,
        "the picker opens above the strip"
    );
}

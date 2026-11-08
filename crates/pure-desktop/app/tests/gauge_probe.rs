//! Headless probe: the context gauge opens over the recorded session,
//! above its pill when the composer sits at the window's foot, and
//! lists the token totals the engine reported.

use egui_kittest::Harness;
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

fn harness() -> Harness<'static, Pane> {
    Harness::builder().with_size(SIZE).build_ui_state(
        |ui, (store, toolbar, opened, installed): &mut Pane| {
            if testutil::theme_installed(installed, ui.ctx()) {
                return;
            }
            if !*opened {
                toolbar.toggle_gauge();
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
fn the_gauge_opens_above_its_pill_inside_the_window() {
    let mut harness = harness();
    for _ in 0..4 {
        harness.run();
    }
    if std::env::var_os("PURE_PROBE_PNG").is_some() {
        let image = harness.render().expect("the probe renders");
        image
            .save("/tmp/gauge-probe.png")
            .expect("the probe image saves");
    }
    let card = harness
        .ctx
        .memory(|memory| memory.area_rect(egui::Id::new("context-gauge")))
        .expect("the gauge card is up");
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE);
    assert!(
        screen.contains_rect(card),
        "the card {card:?} stays inside the window"
    );
    assert!(
        card.center().y < SIZE.y - 60.0,
        "the card opens above the composer strip"
    );
    assert!(
        card.width() <= 420.0,
        "the card keeps its width: {}",
        card.width()
    );
}

#[test]
fn escape_and_a_click_elsewhere_close_the_gauge() {
    let mut harness = harness();
    for _ in 0..3 {
        harness.run();
    }
    assert!(harness.state().1.gauge_open(), "the gauge starts open");
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(!harness.state().1.gauge_open(), "Escape closes the gauge");

    harness.state_mut().1.toggle_gauge();
    harness.run();
    harness.run();
    harness.drag_at(egui::pos2(60.0, 60.0));
    harness.run();
    harness.drop_at(egui::pos2(60.0, 60.0));
    harness.run();
    assert!(
        !harness.state().1.gauge_open(),
        "a click outside the card closes the gauge"
    );
}

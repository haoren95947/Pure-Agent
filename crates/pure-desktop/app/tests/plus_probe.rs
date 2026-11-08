//! Headless probes: the composer's plus popover closes on Escape and
//! on a click outside its card, and a row click still runs the row
//! and closes the popover.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use pure_desktop::store::{Command, Store};
use pure_desktop::transport::{State, replay};
use pure_desktop::ui::toolbar::{self, Toolbar, ToolbarEvent};

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

const SIZE: egui::Vec2 = egui::Vec2::new(900.0, 760.0);

fn harness() -> Harness<'static, Pane> {
    Harness::builder().with_size(SIZE).build_ui_state(
        |ui, (store, toolbar, opened, installed): &mut Pane| {
            if testutil::theme_installed(installed, ui.ctx()) {
                return;
            }
            if !*opened {
                toolbar.toggle_plus();
                *opened = true;
            }
            egui::CentralPanel::default().show(ui, |ui| {
                let rect =
                    egui::Rect::from_min_size(egui::pos2(40.0, 500.0), egui::vec2(800.0, 40.0));
                let mut strip = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(rect)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                for event in toolbar::show(&mut strip, toolbar, store, false, false) {
                    if let ToolbarEvent::TogglePlan = event {
                        if store.plan_on() {
                            store.exit_plan();
                        } else {
                            store.enter_plan();
                        }
                    }
                }
            });
        },
        (replayed_store(), Toolbar::new(), false, false),
    )
}

#[test]
fn escape_and_a_click_elsewhere_close_the_plus_popover() {
    let mut harness = harness();
    for _ in 0..3 {
        harness.run();
    }
    assert!(harness.state().1.plus_open(), "the popover starts open");

    harness.drag_at(egui::pos2(700.0, 60.0));
    harness.run();
    harness.drop_at(egui::pos2(700.0, 60.0));
    harness.run();
    assert!(
        !harness.state().1.plus_open(),
        "a click outside the card closes the popover"
    );

    harness.state_mut().1.toggle_plus();
    harness.run();
    harness.run();
    assert!(harness.state().1.plus_open(), "the popover reopens");
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(!harness.state().1.plus_open(), "Escape closes the popover");
}

#[test]
fn the_plus_popover_opens_above_its_pill() {
    let mut harness = harness();
    for _ in 0..3 {
        harness.run();
    }
    let card = harness
        .ctx
        .memory(|memory| memory.area_rect(egui::Id::new("plus-popover")))
        .expect("the popover is up");
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE);
    assert!(
        screen.contains_rect(card),
        "the popover {card:?} stays inside the window"
    );
    assert!(
        card.bottom() <= 500.0,
        "the popover opens above the strip, not over it: bottom {}",
        card.bottom()
    );
}

#[test]
fn a_click_on_a_plus_row_runs_the_row_and_closes() {
    let mut harness = harness();
    for _ in 0..3 {
        harness.run();
    }
    let plan = harness
        .query_by_label("Plan")
        .expect("the popover lists Plan");
    plan.click();
    harness.run();
    harness.run();
    assert!(
        !harness.state().1.plus_open(),
        "the row click closes the popover"
    );
    assert!(
        harness.state().0.plan_on(),
        "the row click turns plan mode on"
    );
}

#[test]
fn the_mode_rows_carry_their_state_and_an_unoffered_swarm_is_inert() {
    let mut harness = harness();
    for _ in 0..3 {
        harness.run();
    }
    let middle = |label: &str| {
        harness
            .query_by_label(label)
            .unwrap_or_else(|| panic!("the popover draws {label}"))
            .rect()
            .center()
            .y
    };
    let (plan, off) = (middle("Plan"), middle("off"));
    let description = middle("Turn plan mode on");
    assert!(
        off > plan && off < description,
        "the off chip sits on the Plan row, between its label ({plan}) and description ({description}), at {off}"
    );
    assert_eq!(
        testutil::count(&harness, "The agent sent no swarm option"),
        1,
        "the swarm row says why it is inert"
    );
    harness.get_by_label("Swarm").click();
    harness.run();
    harness.run();
    assert!(
        harness.state().1.plus_open(),
        "a click on the inert swarm row does nothing"
    );
}

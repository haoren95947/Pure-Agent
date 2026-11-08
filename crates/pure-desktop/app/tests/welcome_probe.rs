//! Headless E05 probes: the welcome pane draws its pill and composer
//! over a real store with no session, and the pill's menu asks for the
//! folder dialog.

use egui_kittest::Harness;
use pure_desktop::store::Store;
use pure_desktop::ui::composer::Composer;
use pure_desktop::ui::welcome::{Welcome, WelcomeEvent};

mod testutil;

/// A sessionless store: the link is up, no session is open, so the
/// chat column shows the welcome pane.
fn sessionless() -> Store {
    let mut store = Store::new("/w", false);
    store.set_connect(pure_desktop::transport::State::Connected);
    store.handshake(false);
    let _ = store.take_outgoing();
    store
}

type Pane = (Store, Welcome, Composer, Vec<WelcomeEvent>, bool);

/// A harness over the welcome pane, wired the way the shell is: the
/// pane hosts the composer inside its centered stack, and an
/// `OpenFolder` is recorded for the shell's dialog layer.
fn harness(store: Store) -> Harness<'static, Pane> {
    Harness::builder()
        .with_size(egui::Vec2::new(1000.0, 760.0))
        .build_ui_state(
            |ui, (store, welcome, composer, events, installed): &mut Pane| {
                if testutil::theme_installed(installed, ui.ctx()) {
                    return;
                }
                egui::CentralPanel::default().show(ui, |ui| {
                    let mut pane = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(ui.available_rect_before_wrap())
                            .layout(egui::Layout::top_down(egui::Align::Min)),
                    );
                    let commands = store.composer_commands().to_vec();
                    let (evs, composer_event) =
                        welcome.show(&mut pane, store, composer, &commands, false, true);
                    let _ = composer_event;
                    for event in evs {
                        events.push(event);
                    }
                });
            },
            (store, Welcome::new(), Composer::new(), Vec::new(), false),
        )
}

#[test]
fn the_welcome_draws_its_pill_and_composer_without_cards() {
    use egui_kittest::kittest::Queryable;

    let mut harness = harness(sessionless());
    harness.run();
    harness.run();
    // The pill names the directory the next session opens in.
    harness
        .query_by_label("w")
        .expect("the pill names the store's directory");
    // The suggestion cards and the shortcut hint are gone.
    for gone in [
        "Find the failing tests and fix them",
        "Review every module for unhandled errors",
        "Plan the next change before touching code",
        "Delegate a docs check to subagents",
        "commands and sessions",
    ] {
        assert!(
            harness.query_by_label(gone).is_none(),
            "{gone} must not draw"
        );
    }
}

#[test]
fn the_pill_opens_the_menu_and_the_folder_row_asks_the_shell() {
    use egui_kittest::kittest::Queryable;

    let mut harness = harness(sessionless());
    harness.run();
    harness.run();
    let pill = harness.query_by_label("w").expect("the pill draws");
    pill.click();
    harness.run();
    harness
        .query_by_label("PROJECT")
        .expect("the menu opens with its header");
    harness
        .query_by_label("Open folder\u{2026}")
        .expect("the menu carries the folder action");
    let row = harness
        .query_by_label("Open folder\u{2026}")
        .expect("the folder row draws");
    row.click();
    harness.run();
    let (.., events, _) = harness.state_mut();
    assert_eq!(events.as_slice(), &[WelcomeEvent::OpenFolder]);
}

#[test]
fn the_mode_picker_offers_full_access_from_a_stale_template() {
    // A template saved before full access existed carries only default
    // and plan; the picker must still offer the choice Settings saves.
    use egui_kittest::kittest::Queryable;
    use pure_client::wire::{SessionConfigKind, SessionConfigOption, SessionConfigSelectOption};

    let mut store = sessionless();
    store.update_prefs(|prefs| {
        prefs.template = vec![SessionConfigOption {
            id: "mode".into(),
            name: "Mode".into(),
            description: None,
            category: None,
            kind: SessionConfigKind::Select,
            current_value: "default".into(),
            options: ["default", "plan"]
                .into_iter()
                .map(|value| SessionConfigSelectOption {
                    value: value.into(),
                    name: value.into(),
                    description: None,
                })
                .collect(),
        }];
    });
    let mut harness = harness(store);
    harness.run();
    harness.run();
    let pill = harness
        .query_by_label("Default")
        .expect("the mode pill draws its value");
    pill.click();
    harness.run();
    harness
        .query_by_label("Full access")
        .expect("the picker offers full access");
}

#[test]
fn the_stack_sits_above_the_composer_band() {
    // The shell reserves the composer's band under the pane. The
    // stack must center in what is left and never reach into the
    // band, or the pill draws over the composer.
    use egui_kittest::kittest::Queryable;

    let mut harness = harness(sessionless());
    harness.run();
    harness.run();
    harness.run();
    let pill = harness.query_by_label("w").expect("the pill draws");
    let composer = harness
        .query_by_label("Mode")
        .expect("the composer's mode pill draws");
    assert!(
        pill.rect().bottom() < composer.rect().top(),
        "the pill sits above the composer, pill={:?} composer={:?}",
        pill.rect(),
        composer.rect()
    );
    let img = harness.render().expect("the frame renders");
    if std::env::var_os("PURE_PROBE_PNG").is_some() {
        img.save("/tmp/welcome-pane.png")
            .expect("the pane view saves");
    }
}

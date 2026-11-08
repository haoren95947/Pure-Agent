//! The find bar in the real shell: Enter and Shift+Enter step through
//! the matches without losing the query, and Escape closes the bar and
//! hands the keys back to the composer.

use egui_kittest::Harness;

mod testutil;

/// The state a shell harness carries between frames.
struct Rig {
    /// The real window body, built on the harness's own context.
    app: Option<pure_desktop::launch::ShellApp>,
    /// Set once the theme has been installed on a frame that did not
    /// draw.
    installed: bool,
}

/// A harness over the real shell over the replay transport.
fn harness() -> Harness<'static, Rig> {
    Harness::builder()
        .with_size(egui::Vec2::new(1000.0, 760.0))
        .build_ui_state(
            |ui, rig: &mut Rig| {
                if !rig.installed {
                    testutil::install_theme(ui.ctx(), "pure-shadow");
                    rig.installed = true;
                    return;
                }
                let app = rig.app.get_or_insert_with(|| {
                    pure_desktop::launch::ShellApp::new(
                        std::env::temp_dir().to_string_lossy().into_owned(),
                        true,
                        Box::new(pure_desktop::transport::replay::ReplayTransport::new()),
                        ui.ctx(),
                    )
                });
                eframe::App::ui(app, ui, &mut eframe::Frame::_new_kittest());
            },
            Rig {
                app: None,
                installed: false,
            },
        )
}

/// Steps until the recording's last reply is on screen. The replay
/// pump asks for a repaint on every event, so `run` would refuse;
/// `step` drives the frames one at a time.
fn played(harness: &mut Harness<'static, Rig>) {
    for _ in 0..4_000 {
        harness.step();
        if testutil::count(harness, "The null check is in; cargo test passes.") > 0 {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("the recording never finished");
}

/// Presses `key` with `modifiers` and steps a frame.
fn press(harness: &mut Harness<'static, Rig>, key: egui::Key, modifiers: egui::Modifiers) {
    for pressed in [true, false] {
        harness.event(egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        });
    }
    harness.step();
    harness.step();
}

/// The widget that holds the keyboard.
fn focused(harness: &Harness<'static, Rig>) -> Option<egui::Id> {
    harness.ctx.memory(|memory| memory.focused())
}

#[test]
fn enter_steps_the_matches_and_escape_hands_back_the_keys() {
    let mut harness = harness();
    played(&mut harness);

    press(&mut harness, egui::Key::F, egui::Modifiers::CTRL);
    assert_eq!(focused(&harness), Some(egui::Id::new("find-query")));
    harness.event(egui::Event::Text("main".to_owned()));
    harness.step();
    harness.step();
    assert!(
        testutil::field_holds(&harness, "main"),
        "the query took the text"
    );

    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    assert!(
        testutil::field_holds(&harness, "main"),
        "Enter steps to the next match and keeps the query"
    );
    assert_eq!(
        focused(&harness),
        Some(egui::Id::new("find-query")),
        "the query keeps the keys after a step"
    );
    press(&mut harness, egui::Key::Enter, egui::Modifiers::SHIFT);
    assert!(
        testutil::field_holds(&harness, "main"),
        "Shift+Enter steps back and keeps the query"
    );

    press(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(
        !testutil::field_holds(&harness, "main"),
        "Escape closes the bar"
    );
    assert_eq!(
        focused(&harness),
        Some(egui::Id::new("composer-input")),
        "the composer takes the keys back"
    );
}

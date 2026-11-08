//! The palette in the real shell: the sidebar's Search row opens it
//! and the opening click does not close it. The palette draws from a
//! clone the shell writes back at the end of the frame, so an open
//! raised while the frame runs has to land in that clone, or the
//! palette flashes for one frame and vanishes.

use egui_kittest::{Harness, kittest::Queryable};

mod testutil;

/// The state a shell harness carries between frames.
struct Rig {
    /// The real window body, built on the harness's own context the
    /// first frame it is drawn, so the fonts it installs are the ones
    /// the harness renders with.
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

/// Steps until the sidebar's Search row is on screen, then hands the
/// harness back. The replay pump asks for a repaint on every event,
/// so `run` would refuse; `step` drives the frames one at a time.
fn booted(harness: &mut Harness<'static, Rig>) {
    for _ in 0..4_000 {
        harness.step();
        if harness.query_by_label("Search").is_some()
            && harness
                .state()
                .app
                .as_ref()
                .is_some_and(testutil::played_out)
        {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("the sidebar's Search row never drew");
}

/// How many of the builtin slash rows the menu shows now.
fn slash_rows(harness: &Harness<'static, Rig>) -> usize {
    ["/swarm", "/plan", "/goal", "/new"]
        .iter()
        .map(|label| harness.query_all_by_label(label).count())
        .sum()
}

#[test]
fn the_menu_matches_the_field_one_frame_after_each_key() {
    let mut harness = harness();
    booted(&mut harness);

    // A bare slash: the full list, on the very frame the key lands.
    harness.event(egui::Event::Text("/".to_owned()));
    harness.step();
    let first = slash_rows(&harness);
    println!("after /: {first}");
    assert!(first >= 4, "a bare slash lists every command");

    // Typing on narrows the list, on the same frame.
    harness.event(egui::Event::Text("sw".to_owned()));
    harness.step();
    let narrowed = slash_rows(&harness);
    println!("after /sw: {narrowed}");
    assert_eq!(narrowed, 1, "only /swarm matches /sw");

    // Deleting back to the slash restores the full list, again on the
    // same frame: this is where the stale one-row menu stuck.
    harness.key_press(egui::Key::Backspace);
    harness.key_press(egui::Key::Backspace);
    harness.step();
    let restored = slash_rows(&harness);
    println!("after backspaces: {restored}");
    assert!(restored >= 4, "the bare slash lists every command again");

    // And after running a command, the list comes back whole.
    harness.event(egui::Event::Text("swarm on".to_owned()));
    for _ in 0..3 {
        harness.step();
    }
    harness.key_press(egui::Key::Enter);
    for _ in 0..10 {
        harness.step();
    }
    harness.event(egui::Event::Text("/".to_owned()));
    harness.step();
    let again = slash_rows(&harness);
    println!("after send and /: {again}");
    assert!(again >= 4, "a bare slash lists every command again");
}

/// The suggestion menu's height on screen, when it is open.
fn menu_height(harness: &Harness<'static, Rig>) -> Option<f32> {
    harness
        .ctx
        .memory(|memory| memory.area_rect(egui::Id::new("composer-suggest")))
        .map(|rect| rect.height())
}

#[test]
fn a_menu_reopened_after_one_row_draws_every_row() {
    let mut harness = harness();
    booted(&mut harness);

    harness.event(egui::Event::Text("/".to_owned()));
    harness.step();
    harness.step();
    let full = menu_height(&harness).expect("a bare slash opens the menu");

    harness.event(egui::Event::Text("plan".to_owned()));
    harness.step();
    harness.step();
    let one = menu_height(&harness).expect("/plan keeps the menu open");
    assert!(one < full, "one row is shorter than four: {one} vs {full}");

    for _ in 0.."/plan".len() {
        harness.key_press(egui::Key::Backspace);
    }
    harness.step();
    harness.step();
    harness.event(egui::Event::Text("/".to_owned()));
    harness.step();
    harness.step();
    let again = menu_height(&harness).expect("a bare slash opens the menu again");
    println!("full {full}, one {one}, again {again}");
    assert!(
        (again - full).abs() < 1.0,
        "the reopened menu is as tall as the first: {again} vs {full}"
    );
}

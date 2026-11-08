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

#[test]
fn clicking_search_opens_the_palette_and_it_stays_open() {
    let mut harness = harness();
    booted(&mut harness);

    // The click opens the palette; its rows draw the frame after.
    harness
        .query_by_label("Search")
        .expect("Search row")
        .click();
    harness.step();
    harness.step();
    assert!(
        harness.query_by_label("Settings").is_some(),
        "the click on Search opens the palette"
    );

    // The clone write-back at the end of the opening frame must not
    // close it again.
    harness.step();
    harness.step();
    harness.step();
    assert!(
        harness.query_by_label("Settings").is_some(),
        "the palette stays open after the frame that opened it"
    );

    // A click on the card's dead space must not close it: the card
    // answers its own clicks before the backdrop below. The replay
    // streams the palette's rows in over the first frames, so the
    // hints wait for the card to settle before aiming; the point is
    // in the hint row, left of every hint widget, where only the
    // card can answer.
    let close_word = {
        let mut held: Option<egui::Rect> = None;
        let mut still = 0;
        let mut found: Option<egui::Rect> = None;
        for _ in 0..2_000 {
            harness.step();
            let rect = harness.query_by_label("close").map(|node| node.rect());
            if let Some(rect) = rect {
                if held == Some(rect) {
                    still += 1;
                    if still == 8 {
                        found = Some(rect);
                        break;
                    }
                } else {
                    still = 0;
                }
                held = Some(rect);
            }
        }
        found.expect("the palette's hint row never drew")
    };
    let dead = egui::pos2(close_word.left() - 60.0, close_word.center().y);
    harness.hover_at(dead);
    harness.drag_at(dead);
    harness.drop_at(dead);
    harness.step();
    harness.step();
    assert!(
        harness.query_by_label("Settings").is_some(),
        "a click on the card's dead space keeps the palette open"
    );

    // A click on the backdrop, outside the card, still closes it.
    let outside = egui::pos2(10.0, 700.0);
    harness.hover_at(outside);
    harness.drag_at(outside);
    harness.drop_at(outside);
    harness.step();
    harness.step();
    assert!(
        harness.query_by_label("Settings").is_none(),
        "a click outside the card closes the palette"
    );
}

#[test]
fn clicking_a_session_row_switches_to_it() {
    // A row click used to run the keyboard-highlighted row instead of
    // the one under the pointer, so a session click ran the first
    // command (New session) and the session never switched.
    let mut harness = harness();
    booted(&mut harness);

    // The replay streams the transcript in after boot; wait for its
    // rows before judging what the click did.
    for _ in 0..2_000 {
        harness.step();
        if harness
            .state()
            .app
            .as_ref()
            .is_some_and(|app| app.shell.transcript().row_count() > 5)
        {
            break;
        }
    }

    // Open the palette by Search, the way the user does.
    harness
        .query_by_label("Search")
        .expect("Search row")
        .click();
    harness.step();
    harness.step();
    assert!(
        harness.query_by_label("Settings").is_some(),
        "the click on Search opens the palette"
    );

    // The palette's rows stream in with the replay; wait for the
    // session's row, scrolling the list to it if it sits past the
    // viewport. The row is a button carrying the session's title; the
    // transcript's own user message is a label with the same words,
    // so the role tells them apart.
    let session_row = {
        let mut found = None;
        harness.hover_at(egui::pos2(500.0, 300.0));
        for _ in 0..200 {
            harness.step();
            let row = harness
                .query_all_by(|node| {
                    node.role() == egui::accesskit::Role::Button
                        && node.label() == Some("Fix the null dereference in main".to_owned())
                })
                .next();
            if let Some(node) = row {
                // The tree reports clipped rows too; only a row the
                // scroll viewport fully shows can take the click.
                let rect = node.rect();
                if rect.center().y < 540.0 {
                    found = Some(rect);
                    break;
                }
            }
            harness.event(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -80.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            });
        }
        found.expect("the palette lists the replay's session")
    };

    // Click the session row: press and release land in their own
    // frames, the way the shell's other probes click.
    harness.hover_at(session_row.center());
    harness.step();
    harness.drag_at(session_row.center());
    harness.step();
    harness.drop_at(session_row.center());
    harness.step();
    harness.step();
    let app = harness.state().app.as_ref().expect("the shell");
    assert!(
        app.shell.store().active_id().is_some(),
        "the session row ran the session: the welcome screen would have cleared it"
    );
    assert!(
        app.shell.transcript().row_count() > 5,
        "the transcript keeps its rows"
    );
    assert!(
        harness.query_by_label("Settings").is_none(),
        "the palette closed after the run"
    );
}

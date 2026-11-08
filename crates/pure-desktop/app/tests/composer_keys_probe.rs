//! The composer's keys and picks in the real shell, over the replay
//! transport: a pick or a seed leaves the field with the keyboard and
//! the caret after the inserted text, Enter sends only from the field,
//! the newline keys add one newline, and Esc twice interrupts.

use egui_kittest::{
    Harness,
    kittest::{NodeT as _, Queryable},
};

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

/// Steps until the replayed session's composer holds the keyboard. The
/// replay pump asks for a repaint on every event, so `run` would
/// refuse; `step` drives the frames one at a time.
fn booted() -> Harness<'static, Rig> {
    let mut harness = harness();
    for _ in 0..4_000 {
        harness.step();
        if harness.query_all_by_label("Add").next().is_some()
            && field_focused(&harness)
            && harness
                .state()
                .app
                .as_ref()
                .is_some_and(testutil::played_out)
        {
            steps(&mut harness, 3);
            return harness;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("the composer never drew with the keyboard");
}

/// Runs `n` frames.
fn steps(harness: &mut Harness<'static, Rig>, n: usize) {
    for _ in 0..n {
        harness.step();
    }
}

/// Whether the composer's field holds the keyboard.
fn field_focused(harness: &Harness<'static, Rig>) -> bool {
    harness.ctx.memory(|memory| memory.focused()) == Some(egui::Id::new("composer-input"))
}

/// The composer's text: the one multiline field on screen.
fn composer_text(harness: &Harness<'static, Rig>) -> String {
    harness
        .query_all_by(|node| node.role() == egui::accesskit::Role::MultilineTextInput)
        .next()
        .and_then(|node| node.accesskit_node().value())
        .unwrap_or_default()
}

/// Types `text` and lets the field take it.
fn type_text(harness: &mut Harness<'static, Rig>, text: &str) {
    harness.event(egui::Event::Text(text.to_owned()));
    steps(harness, 3);
}

/// Clicks the node labeled `label` and lets the frame settle.
fn click(harness: &mut Harness<'static, Rig>, label: &str) {
    harness.get_by_label(label).click();
    steps(harness, 3);
}

#[test]
fn a_picked_command_keeps_the_keyboard_for_its_arguments() {
    let mut harness = booted();
    type_text(&mut harness, "/pl");
    click(&mut harness, "/plan");
    assert!(field_focused(&harness), "the field keeps the keyboard");
    type_text(&mut harness, "on");
    assert_eq!(composer_text(&harness), "/plan on");
}

#[test]
fn the_plus_seeds_leave_the_caret_after_the_seed() {
    let mut harness = booted();
    click(&mut harness, "Add");
    click(&mut harness, "Commands");
    type_text(&mut harness, "go");
    assert_eq!(composer_text(&harness), "/go", "the slash seed");

    for _ in 0..3 {
        harness.key_press(egui::Key::Backspace);
    }
    steps(&mut harness, 2);
    click(&mut harness, "Add");
    click(&mut harness, "Mention");
    type_text(&mut harness, "src");
    assert_eq!(
        composer_text(&harness),
        "@src",
        "an empty draft takes a bare at, no space before it"
    );
}

#[test]
fn shift_enter_adds_one_newline_and_enter_sends_without_one() {
    let mut harness = booted();
    type_text(&mut harness, "abc");
    harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::Enter);
    type_text(&mut harness, "def");
    assert_eq!(composer_text(&harness), "abc\ndef");

    for _ in 0..7 {
        harness.key_press(egui::Key::Backspace);
    }
    type_text(&mut harness, "fix the bug");
    for _ in 0..4 {
        harness.key_press(egui::Key::ArrowLeft);
    }
    steps(&mut harness, 2);
    harness.key_press(egui::Key::Enter);
    steps(&mut harness, 10);
    assert_eq!(composer_text(&harness), "", "the prompt went out");
    assert!(
        harness.query_by_label("fix the bug").is_some(),
        "the prompt went out without the newline the caret was at"
    );
}

#[test]
fn enter_in_the_mode_picker_chooses_and_does_not_send() {
    let mut harness = booted();
    type_text(&mut harness, "keep me");
    click(&mut harness, "Default");
    harness.key_press(egui::Key::ArrowDown);
    steps(&mut harness, 2);
    harness.key_press(egui::Key::Enter);
    steps(&mut harness, 4);
    assert_eq!(composer_text(&harness), "keep me", "the draft stays");
    assert!(
        harness.query_by_label("Ask").is_some(),
        "Down then Enter chose the row under Default"
    );
    assert!(field_focused(&harness), "the field has the keys back");
}

#[test]
fn the_model_picker_search_takes_the_typing() {
    let mut harness = booted();
    harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::Button
                && node
                    .label()
                    .is_some_and(|label| label.starts_with("GLM 4.7"))
        })
        .next()
        .expect("the model pill draws")
        .click();
    steps(&mut harness, 3);
    type_text(&mut harness, "glm 5");
    assert!(
        testutil::field_holds(&harness, "glm 5"),
        "the query field holds the typing"
    );
    assert_eq!(composer_text(&harness), "", "the composer took none of it");
}

#[test]
fn esc_twice_arms_then_interrupts_and_keeps_the_field() {
    let mut harness = booted();
    // The recording has played out, so a fresh prompt starts a turn
    // nothing answers: it runs until the Escapes interrupt it.
    type_text(&mut harness, "keep going");
    harness.key_press(egui::Key::Enter);
    steps(&mut harness, 4);
    harness.key_press(egui::Key::Escape);
    steps(&mut harness, 2);
    assert!(
        harness
            .query_by_label("Press Esc again to interrupt")
            .is_some(),
        "the first press arms the interrupt and says so"
    );
    assert!(field_focused(&harness), "Esc does not drop the field");
    harness.key_press(egui::Key::Escape);
    steps(&mut harness, 2);
    assert!(
        harness
            .query_by_label("Press Esc again to interrupt")
            .is_none(),
        "the second press interrupts and drops the hint"
    );
}

#[test]
fn a_bare_goal_command_opens_the_goal_dialog() {
    let mut harness = booted();
    type_text(&mut harness, "/goal");
    harness.key_press(egui::Key::Enter);
    steps(&mut harness, 4);
    assert!(
        harness.query_by_label("Set a goal").is_some(),
        "a bare /goal asks for the goal rather than clearing it"
    );
}

#[test]
fn a_session_gets_its_own_draft_back() {
    let mut harness = booted();
    type_text(&mut harness, "half a thought");
    click(&mut harness, "New session");
    steps(&mut harness, 3);
    assert_eq!(composer_text(&harness), "", "the welcome pane starts empty");
    // The sidebar's one session row, under its project header. The
    // row carries no accessible name, so the click lands by position.
    let at = egui::pos2(140.0, 220.0);
    harness.event(egui::Event::PointerMoved(at));
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        steps(&mut harness, 1);
    }
    steps(&mut harness, 5);
    assert_eq!(composer_text(&harness), "half a thought");
}

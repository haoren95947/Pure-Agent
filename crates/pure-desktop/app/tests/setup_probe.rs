//! Headless S07.5 probes: the setup screen. The screen replaces the
//! whole shell while no engine can start, so each test drives the
//! screen's own controls and reads what it asked the shell to do.

use egui_kittest::{Harness, kittest::Queryable};
use pure_desktop::ui::setup::{SetupEvent, SetupView};

mod testutil;

/// The probe's harness state: the screen, what it asked for, and the
/// one-frame font-install flag.
type Probe = (SetupView, Option<SetupEvent>, bool);

/// A harness over the setup screen.
fn harness_for(view: SetupView) -> Harness<'static, Probe> {
    Harness::builder()
        .with_size(egui::Vec2::new(1000.0, 760.0))
        .build_ui_state(
            |ui, (screen, asked, installed): &mut Probe| {
                if testutil::theme_installed(installed, ui.ctx()) {
                    return;
                }
                screen.show(ui, false);
                // A single run spans several frames; only a fresh
                // event replaces the one a button asked for.
                if let Some(event) = screen.take_asked() {
                    *asked = Some(event);
                }
            },
            (view, None, false),
        )
}

#[test]
fn the_screen_says_what_is_missing_and_how_to_fix_it() {
    let mut harness = harness_for(SetupView::new());
    harness.run();
    assert_eq!(
        testutil::count(&harness, "Pure Agent backend not found"),
        1,
        "the screen names the problem"
    );
    assert_eq!(
        testutil::field_values(&harness).len(),
        1,
        "the path field draws"
    );
    assert_eq!(
        testutil::count(&harness, "Use"),
        1,
        "the probe button draws"
    );
    assert_eq!(
        testutil::count(&harness, "Check again"),
        1,
        "the retry button draws"
    );
    assert_eq!(
        testutil::count(&harness, "keep pure-backend.exe beside pure-agent.exe"),
        1,
        "the archive note draws"
    );
    assert_eq!(
        testutil::count(
            &harness,
            "powershell -ExecutionPolicy Bypass -File scripts/build.ps1"
        ),
        1,
        "the source build command draws"
    );
    assert_eq!(
        testutil::count(&harness, "set RPI_BIN to the pure-backend.exe path"),
        1,
        "the backend override draws"
    );
}

#[test]
fn an_empty_path_is_refused_where_the_user_can_see_it() {
    let mut harness = harness_for(SetupView::new());
    harness.run();
    harness.get_by_label("Use").click();
    harness.run();
    assert_eq!(
        harness.state().0.error(),
        Some("Enter the path to pure-backend."),
        "the screen says what is missing"
    );
    assert_eq!(harness.state().1, None, "nothing is handed to the shell");
    assert_eq!(
        testutil::count(&harness, "Enter the path to pure-backend."),
        1,
        "the refusal draws on the screen"
    );
}

/// Serialises the tests that start a child process: a stub written by
/// one test can be caught mid-fork by another and fail with "text file
/// busy".
static SPAWNS: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn a_program_that_is_not_pure_is_refused_with_why() {
    let _spawns = SPAWNS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut harness = harness_for(SetupView::new());
    harness.run();
    testutil::first_field(&harness)
        .expect("the path field draws")
        .click();
    harness.run();
    testutil::first_field(&harness)
        .expect("the path field draws")
        .type_text("rustc");
    harness.run();
    harness.get_by_label("Use").click();
    harness.run();
    assert_eq!(
        harness.state().0.error(),
        Some("rustc is not a Pure Agent backend"),
        "the probe named what it found"
    );
    assert_eq!(harness.state().1, None, "the shell is not told to run it");
}

// The stub is a shell script, which only Unix runs directly.
#[cfg(unix)]
#[test]
fn a_probed_binary_is_handed_to_the_shell() {
    use std::os::unix::fs::PermissionsExt as _;
    let _spawns = SPAWNS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let stub = std::env::temp_dir().join("pure-setup-probe-probe");
    std::fs::write(&stub, "#!/bin/sh\necho 'pure 9.9.9'\n").expect("the stub is written");
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).expect("the stub runs");
    let path = stub.to_string_lossy().into_owned();
    let mut harness = harness_for(SetupView::new());
    harness.run();
    testutil::first_field(&harness)
        .expect("the path field draws")
        .click();
    harness.run();
    testutil::first_field(&harness)
        .expect("the path field draws")
        .type_text(&path);
    harness.run();
    harness.get_by_label("Use").click();
    harness.run();
    assert_eq!(
        harness.state().1,
        Some(SetupEvent::Use(path.clone())),
        "the shell is told which binary to run"
    );
    assert_eq!(harness.state().0.error(), None, "the refusal is cleared");
    let _ = std::fs::remove_file(&stub);
}

#[test]
fn the_check_again_button_asks_for_the_path_again() {
    let mut harness = harness_for(SetupView::new());
    harness.run();
    harness.get_by_label("Check again").click();
    harness.run();
    assert_eq!(
        harness.state().1,
        Some(SetupEvent::Retry),
        "the screen asks the shell to look again"
    );
}

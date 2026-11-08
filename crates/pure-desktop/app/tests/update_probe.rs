//! Headless S07.5 probes: the About page's Updates row. The row reads
//! the release check's outcome, so each test puts an outcome in the
//! preferences and reads what the row draws, then presses the buttons
//! and reads what the dialog asks the shell to do.

use egui_kittest::{Harness, kittest::Queryable};
use pure_desktop::prefs::{Channel, Prefs};
use pure_desktop::store::Store;
use pure_desktop::ui::settings::{Install, Section, SettingsEvent, SettingsView, UpdateState};

mod testutil;

/// The probe's harness state: the store, the dialog, what it asked
/// for, the release state the row reads, and the font-install flag.
type Probe = (Store, SettingsView, Vec<SettingsEvent>, UpdateState, bool);

/// A harness over the settings dialog's About page.
fn harness_for(prefs: Prefs, updates: UpdateState) -> Harness<'static, Probe> {
    let mut store = Store::new("/w", false);
    store.set_connect(pure_desktop::transport::State::Connected);
    store.handshake(false);
    let id = store
        .take_outgoing()
        .iter()
        .find_map(|frame| match frame {
            pure_client::Frame::Request { id, method, .. } if method == "initialize" => {
                Some(id.clone())
            }
            _ => None,
        })
        .expect("the handshake is the first request");
    store.absorb(pure_client::Frame::Success {
        id,
        result: serde_json::json!({
            "protocolVersion": 1,
            "agentCapabilities": {"steer": true},
            "agentInfo": {"name": "pure", "version": "0.1.0"}
        }),
    });
    store.update_prefs(|current| *current = prefs);
    Harness::builder()
        .with_size(egui::Vec2::new(1000.0, 760.0))
        .build_ui_state(
            |ui, (store, settings, events, updates, installed): &mut Probe| {
                if testutil::theme_installed(installed, ui.ctx()) {
                    return;
                }
                events.extend(settings.show(ui, store, updates));
            },
            (store, SettingsView::new(), Vec::new(), updates, false),
        )
}

/// A harness over the About page, open.
fn about_page(prefs: Prefs, updates: UpdateState) -> Harness<'static, Probe> {
    let mut harness = harness_for(prefs, updates);
    harness.state_mut().1.open(Section::About);
    harness.run();
    harness
}

/// Preferences that saw one check on the stable channel.
fn checked() -> Prefs {
    Prefs {
        latest_version: Some("0.2.0".to_owned()),
        update_checked_at: Some(1),
        ..Prefs::default()
    }
}

#[test]
fn the_row_says_when_no_check_has_run() {
    let harness = about_page(Prefs::default(), UpdateState::default());
    assert_eq!(
        testutil::count(&harness, "engine: no check has run yet"),
        1,
        "the engine line says what it does not know"
    );
    assert_eq!(
        testutil::count(&harness, "desktop: no check has run yet"),
        1,
        "the desktop line says the same"
    );
    assert_eq!(
        testutil::count(&harness, "Check now"),
        1,
        "the row can check now"
    );
}

#[test]
fn a_current_client_is_told_it_is_current() {
    let mut prefs = checked();
    // The desktop is built at the crate version, so a lower latest
    // reads as current.
    prefs.latest_version = Some("0.1.0".to_owned());
    let harness = about_page(prefs, UpdateState::default());
    assert_eq!(
        testutil::count(&harness, "engine 0.1.0; current with 0.1.0"),
        1,
        "the engine is current"
    );
    assert!(
        testutil::count(&harness, "0.1.0 is available") == 0,
        "a lower latest is not newer"
    );
}

#[test]
fn an_install_button_appears_for_each_thing_that_is_behind() {
    let mut prefs = checked();
    prefs.latest_version = Some("9.9.9".to_owned());
    let harness = about_page(prefs, UpdateState::default());
    assert_eq!(
        testutil::count(&harness, "Install engine"),
        1,
        "a newer engine offers its install"
    );
    assert_eq!(
        testutil::count(&harness, "Install desktop"),
        1,
        "a newer client offers its install"
    );
    assert_eq!(
        testutil::count(&harness, "Installing\u{2026}"),
        0,
        "nothing is installing yet"
    );
}

#[test]
fn an_install_in_flight_reads_as_installing() {
    let mut prefs = checked();
    prefs.latest_version = Some("9.9.9".to_owned());
    let updates = UpdateState {
        installing: Some(Install::Engine),
        ..UpdateState::default()
    };
    let harness = about_page(prefs, updates);
    assert_eq!(
        testutil::count(&harness, "Installing\u{2026}"),
        1,
        "the button shows the download is running"
    );
    assert_eq!(
        testutil::count(&harness, "Install engine"),
        0,
        "the plain label is gone"
    );
}

#[test]
fn a_failed_check_names_its_error() {
    let mut prefs = checked();
    prefs.check_error = Some("the release list did not load".to_owned());
    let harness = about_page(prefs, UpdateState::default());
    assert_eq!(
        testutil::count(&harness, "last check failed: the release list did not load"),
        1,
        "the refusal draws under the lines"
    );
}

#[test]
fn an_install_note_reports_what_happened() {
    let mut prefs = checked();
    prefs.latest_version = Some("9.9.9".to_owned());
    let updates = UpdateState {
        install_note: Some((false, "the download did not load".to_owned())),
        ..UpdateState::default()
    };
    let harness = about_page(prefs, updates);
    assert_eq!(
        testutil::count(&harness, "the download did not load"),
        1,
        "the note draws under the buttons"
    );
}

#[test]
fn the_check_button_asks_for_a_check_now() {
    let mut harness = about_page(Prefs::default(), UpdateState::default());
    harness.get_by_label("Check now").click();
    harness.run();
    assert!(
        harness.state().2.contains(&SettingsEvent::CheckNow),
        "the dialog asks the shell to check now: {:?}",
        harness.state().2
    );
}

#[test]
fn the_channel_buttons_switch_the_channel() {
    let mut harness = about_page(Prefs::default(), UpdateState::default());
    harness.get_by_label("Nightly").click();
    harness.run();
    assert!(
        harness
            .state()
            .2
            .contains(&SettingsEvent::Channel(Channel::Nightly)),
        "the dialog asks for the nightly channel"
    );
}

#[test]
fn the_install_button_asks_for_that_install() {
    let mut prefs = checked();
    prefs.latest_version = Some("9.9.9".to_owned());
    let mut harness = about_page(prefs, UpdateState::default());
    harness.get_by_label("Install engine").click();
    harness.run();
    assert!(
        harness
            .state()
            .2
            .contains(&SettingsEvent::Install(Install::Engine)),
        "the dialog asks for the engine install"
    );
}

#[test]
fn the_releases_button_asks_for_the_page() {
    let mut harness = about_page(Prefs::default(), UpdateState::default());
    harness.get_by_label("Releases").click();
    harness.run();
    assert!(
        harness.state().2.contains(&SettingsEvent::OpenReleases),
        "the dialog asks the shell to open the releases page"
    );
}

//! The welcome pane in the OpenBitFun pair: the shell with no session
//! and a transport that cannot answer, so the splash steps aside and
//! the pane draws its wordmark, project pill and composer.
//!
//! The splash probe is the regression for a corner-bound box: an
//! `Area` sizes to its content unless told otherwise, so the boot
//! splash used to paint a small panel in the window's top-left corner
//! instead of covering the window.

use egui_kittest::Harness;
use pure_desktop::transport::{Event, EventSender, Link, State, Transport};
use pure_desktop::ui::splash::Splash;

mod testutil;

/// A transport whose first link never comes up, so the splash goes and
/// the shell draws its welcome pane.
struct Offline;

impl Transport for Offline {
    fn start(&mut self, events: EventSender) {
        let _ = events.try_send(Event::State(State::Reconnecting {
            attempt: 1,
            delay: std::time::Duration::from_secs(1),
        }));
    }
    fn send(&self, _frame: pure_client::Frame) {}
    fn close(&self) {}
    fn link(&self) -> Link {
        Link {
            name: "pure rpc",
            detail: "local".to_owned(),
        }
    }
}

struct Rig {
    app: Option<pure_desktop::launch::ShellApp>,
    theme: &'static str,
    installed: bool,
}

fn harness(theme: &'static str) -> Harness<'static, Rig> {
    Harness::builder()
        .with_size(egui::Vec2::new(1252.0, 900.0))
        .build_ui_state(
            |ui, rig: &mut Rig| {
                if !rig.installed {
                    testutil::install_theme(ui.ctx(), rig.theme);
                    rig.installed = true;
                    return;
                }
                let app = rig.app.get_or_insert_with(|| {
                    pure_desktop::launch::ShellApp::new(
                        "/tmp".to_owned(),
                        true,
                        Box::new(Offline),
                        ui.ctx(),
                    )
                });
                eframe::App::ui(app, ui, &mut eframe::Frame::_new_kittest());
            },
            Rig {
                app: None,
                theme,
                installed: false,
            },
        )
}

fn welcome(theme: &'static str, name: &str) {
    let mut harness = harness(theme);
    for _ in 0..12 {
        harness.step();
    }
    harness.snapshot(name);
}

#[test]
fn the_bitfun_light_welcome_baseline() {
    welcome("bitfun-light", "bitfun-light-welcome");
}

#[test]
fn the_bitfun_dark_welcome_baseline() {
    welcome("bitfun-dark", "bitfun-dark-welcome");
}

#[test]
fn the_splash_covers_the_whole_window() {
    let mut harness = Harness::builder()
        .with_size(egui::Vec2::new(400.0, 300.0))
        .build_ui_state(
            |ui, installed: &mut bool| {
                if !*installed {
                    testutil::install_theme(ui.ctx(), "bitfun-light");
                    *installed = true;
                    return;
                }
                // Paint the window red first: any pixel the splash does
                // not cover stays red.
                ui.painter()
                    .rect_filled(ui.max_rect(), 0, egui::Color32::RED);
                Splash::Showing.show(ui);
            },
            false,
        );
    harness.step();
    harness.step();
    let image = harness.render().expect("the frame renders");
    let bg = pure_desktop::theme::Palette::active_of(&harness.ctx).bg;
    for (x, y) in [(398, 298), (398, 2), (2, 298)] {
        let [r, g, b, _] = image.get_pixel(x, y).0;
        assert_eq!(
            egui::Color32::from_rgb(r, g, b),
            bg,
            "the splash leaves ({x}, {y}) uncovered"
        );
    }
}

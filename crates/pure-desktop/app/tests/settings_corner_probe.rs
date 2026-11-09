//! The settings card's four corners: nothing inside the card may
//! paint past its rounded edge.
//!
//! The card rounds its own fill, but egui clips to rectangles only, so
//! a column that fills its side of the card square shows a plain
//! corner outside that arc. The nav column did exactly this until the
//! probe was written, so the probe reads rendered pixels, not the
//! shapes that made them.

use egui_kittest::Harness;
use pure_desktop::store::Store;
use pure_desktop::ui::settings::{Section, SettingsEvent, SettingsView, UpdateState};

mod testutil;

/// The canvas: wide enough that the card takes its own 960 by 720.
const SIZE: egui::Vec2 = egui::Vec2::new(1014.0, 777.0);

/// The card `SettingsView::show` centers on that canvas.
const CARD: egui::Rect = egui::Rect::from_min_max(egui::pos2(27.0, 28.5), egui::pos2(987.0, 748.5));

/// A pixel this bright reads as the card; the scrim outside it is dark.
const LIGHT: u8 = 180;

type Probe = (Store, SettingsView, Vec<SettingsEvent>, bool);

/// A booted store, so the dialog reads its prefs from a session.
fn booted() -> Store {
    let mut store = Store::new("/w", false);
    store.set_connect(pure_desktop::transport::State::Connected);
    store.handshake(false);
    store.new_session();
    let _ = store.take_outgoing();
    store.absorb(pure_client::Frame::Success {
        id: pure_client::RequestId::Number(2),
        result: serde_json::json!({"sessionId": "s1"}),
    });
    let _ = store.take_outgoing();
    store.set_active("s1");
    store
}

/// A harness over the open settings dialog on the General page.
fn harness() -> Harness<'static, Probe> {
    Harness::builder().with_size(SIZE).build_ui_state(
        |ui, (store, settings, events, installed): &mut Probe| {
            if !*installed {
                testutil::install_theme(ui.ctx(), "bitfun-light");
                *installed = true;
                return;
            }
            if !settings.is_open() {
                settings.open(Section::General);
            }
            events.extend(settings.show(ui, store, &UpdateState::default()));
        },
        (booted(), SettingsView::new(), Vec::new(), false),
    )
}

#[test]
fn no_corner_of_the_card_paints_square() {
    let mut harness = harness();
    harness.run();
    harness.run();
    harness.run();
    let image = harness.render().expect("the dialog renders");
    if std::env::var_os("PURE_PROBE_PNG").is_some() {
        image
            .save(std::env::temp_dir().join("settings-card-corners.png"))
            .expect("the image saves");
    }
    for (name, corner, in_x, in_y) in [
        ("top left", CARD.min, 1.0, 1.0),
        ("top right", egui::pos2(CARD.right(), CARD.top()), -1.0, 1.0),
        (
            "bottom left",
            egui::pos2(CARD.left(), CARD.bottom()),
            1.0,
            -1.0,
        ),
        ("bottom right", CARD.max, -1.0, -1.0),
    ] {
        // The card's arc centers one radius in from the square corner,
        // so a point six in from the corner on the diagonal falls
        // outside it and one twelve in falls inside.
        for (off, wants_card) in [(6.0, false), (12.0, true)] {
            let at = egui::pos2(corner.x + in_x * off, corner.y + in_y * off);
            let pixel = image.get_pixel(at.x as u32, at.y as u32);
            let brightest = pixel[0].max(pixel[1]).max(pixel[2]);
            assert_eq!(
                brightest > LIGHT,
                wants_card,
                "{name}: {} the rounded corner at {at:?} reads {brightest}, and the \
                 card's own fills start at 248",
                if wants_card { "inside" } else { "outside" }
            );
        }
    }
}

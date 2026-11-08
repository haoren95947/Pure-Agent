//! The popover surfaces' shadows in the real shell. egui clips a
//! window's content to the size the window remembers, which is not the
//! surface's own size, so a shadow painted inside it was cut into a
//! flat, hard-edged slab beside the menu. The light theme shows it,
//! where a shadow is a visible gray on the page.

use egui_kittest::{
    Harness,
    kittest::{NodeT as _, Queryable},
};

mod testutil;

/// The state a shell harness carries between frames.
struct Rig {
    /// The real window body, built on the harness's own context the
    /// first frame it is drawn.
    app: Option<pure_desktop::launch::ShellApp>,
    /// Set once the theme has been installed on a frame that did not
    /// draw.
    installed: bool,
}

/// A harness over the real shell over the replay transport, in the
/// light theme.
fn harness() -> Harness<'static, Rig> {
    Harness::builder()
        .with_size(egui::Vec2::new(960.0, 640.0))
        .build_ui_state(
            |ui, rig: &mut Rig| {
                if !rig.installed {
                    testutil::install_theme(ui.ctx(), "pure-dawn");
                    rig.installed = true;
                    return;
                }
                let app = rig.app.get_or_insert_with(|| {
                    pure_desktop::launch::ShellApp::new(
                        "/tmp".to_owned(),
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

/// Steps `n` frames. The replay pump asks for a repaint on every
/// event, so `run` would refuse; `step` drives the frames one at a
/// time.
fn steps(harness: &mut Harness<'static, Rig>, n: usize) {
    for _ in 0..n {
        harness.step();
    }
}

#[test]
fn the_project_menu_shadow_fades_out_without_a_slab() {
    let mut harness = harness();
    for _ in 0..2_000 {
        harness.step();
        if harness.query_by_label("Search").is_some() {
            break;
        }
    }
    steps(&mut harness, 30);
    harness.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::N);
    steps(&mut harness, 10);
    harness.get_by_label("tmp").click();
    steps(&mut harness, 4);
    let row = harness
        .get_by_label("Open folder\u{2026}")
        .accesskit_node()
        .bounding_box()
        .expect("the folder row has bounds");
    let image = harness.render().expect("the shell renders");
    let y = ((row.y0 + row.y1) / 2.0) as u32;
    let lum = |x: u32| {
        let [r, g, b, _] = image.get_pixel(x, y).0;
        u32::from(r) + u32::from(g) + u32::from(b)
    };
    // From just past the menu's border out across its shadow: a shadow
    // lightens a little per pixel, and a clipped one ends in one step.
    let from = row.x1 as u32 + 8;
    let largest = (from..from + 80)
        .map(|x| lum(x).abs_diff(lum(x + 1)))
        .max()
        .unwrap_or(0);
    assert!(
        largest <= 15,
        "the shadow ends in a hard edge: a {largest} step"
    );
}

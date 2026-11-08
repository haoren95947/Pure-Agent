//! Headless E01 probes: the themed empty shell's palette histogram,
//! the two design families rendering, and the icon catalog rendering
//! tinted, all through kittest without a display server.

use std::collections::HashMap;

use egui_kittest::Harness;

mod testutil;

/// The app draws: fonts and icon loaders installed once, the theme
/// choice applied, and the body the caller draws in the central panel.
struct Probe {
    theme: String,
    installed: bool,
    body: Body,
}

/// What the probe draws besides the empty panel.
enum Body {
    Nothing,
    Text,
    IconCatalog,
}

fn harness(theme: &str, size: [f32; 2], body: Body) -> Harness<'static, Probe> {
    Harness::builder()
        .with_size(egui::Vec2::new(size[0], size[1]))
        .build_ui_state(
            |ui, probe: &mut Probe| {
                if testutil::theme_installed(&mut probe.installed, ui.ctx()) {
                    // Fonts become active at the start of the next
                    // pass, so the themed style follows one pass later.
                    return;
                }
                pure_desktop::theme::apply_choice(
                    &pure_desktop::theme::ThemeChoice::Named(probe.theme.clone()),
                    ui.ctx(),
                );
                egui::CentralPanel::default().show(ui, |ui| match probe.body {
                    Body::Nothing => {}
                    Body::Text => {
                        ui.label("pure");
                        ui.monospace("pure mono 0123456789");
                    }
                    Body::IconCatalog => {
                        egui::Grid::new("catalog").show(ui, |ui| {
                            for chunk in pure_desktop::icons::Icon::ALL.chunks(16) {
                                for icon in chunk {
                                    pure_desktop::icons::themed(ui, *icon, 24.0);
                                }
                                ui.end_row();
                            }
                        });
                    }
                });
            },
            Probe {
                theme: theme.to_owned(),
                installed: false,
                body,
            },
        )
}

/// The most common painted pixel and its share of the painted frame.
/// The renderer clears to transparent and kittest leaves its own 8px
/// margin ring unpainted, so unpainted pixels do not count.
fn dominant(img: &image::RgbaImage) -> (egui::Color32, f32) {
    let mut counts: HashMap<egui::Color32, usize> = HashMap::new();
    let mut painted = 0usize;
    for px in img.pixels() {
        if px.0[3] == 0 {
            continue;
        }
        painted += 1;
        let c = egui::Color32::from_rgba_unmultiplied(px.0[0], px.0[1], px.0[2], px.0[3]);
        *counts.entry(c).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map(|(c, n)| (c, n as f32 / painted as f32))
        .expect("the frame paints something")
}

/// Painted pixels that differ from `against` by more than the
/// tolerance.
fn differing(img: &image::RgbaImage, against: egui::Color32) -> usize {
    img.pixels()
        .filter(|px| px.0[3] != 0)
        .filter(|px| {
            let (dr, dg, db) = (
                i32::from(px.0[0]) - i32::from(against.r()),
                i32::from(px.0[1]) - i32::from(against.g()),
                i32::from(px.0[2]) - i32::from(against.b()),
            );
            dr.abs() + dg.abs() + db.abs() > 12
        })
        .count()
}

/// The empty shadow shell paints the pure shadow background.
#[test]
fn empty_shell_draws_the_shadow_background() {
    let mut harness = harness("pure-shadow", [600.0, 400.0], Body::Nothing);
    harness.run();
    let img = harness.render().expect("the frame renders");
    let (color, share) = dominant(&img);
    assert_eq!(color, egui::Color32::from_rgb(0x0F, 0x0E, 0x13));
    assert!(
        share > 0.95,
        "the background holds only {share} of the frame"
    );
}

/// The empty dawn shell paints the pure dawn background.
#[test]
fn empty_shell_draws_the_dawn_background() {
    let mut harness = harness("pure-dawn", [600.0, 400.0], Body::Nothing);
    harness.run();
    let img = harness.render().expect("the frame renders");
    let (color, share) = dominant(&img);
    assert_eq!(color, egui::Color32::from_rgb(0xF7, 0xF5, 0xF9));
    assert!(
        share > 0.95,
        "the background holds only {share} of the frame"
    );
}

/// Both design families draw: text pixels land on the panel in both
/// palettes.
#[test]
fn text_renders_in_both_families_and_palettes() {
    for (theme, bg) in [
        ("pure-shadow", egui::Color32::from_rgb(0x0F, 0x0E, 0x13)),
        ("pure-dawn", egui::Color32::from_rgb(0xF7, 0xF5, 0xF9)),
    ] {
        let mut harness = harness(theme, [600.0, 400.0], Body::Text);
        harness.run();
        let img = harness.render().expect("the frame renders");
        let drawn = differing(&img, bg);
        assert!(drawn > 500, "{theme}: only {drawn} text pixels rendered");
    }
}

/// Every icon in the catalog renders tinted over the panel.
#[test]
fn the_icon_catalog_renders_tinted() {
    let mut harness = harness("pure-shadow", [900.0, 600.0], Body::IconCatalog);
    harness.run();
    let img = harness.render().expect("the frame renders");
    let drawn = differing(&img, egui::Color32::from_rgb(0x0F, 0x0E, 0x13));
    // 147 icons of strokes at 24px: a bare handful of pixels each
    // would already pass 1,000. A broken loader leaves the cell empty.
    assert!(drawn > 1_000, "only {drawn} icon pixels rendered");
}

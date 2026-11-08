//! Headless E03 probes: the three-panel layout and the themed shell,
//! through kittest without a display server. The panels' geometry is
//! what these assert, since the shell's chrome returns view by view in
//! the later epics.

use std::collections::HashMap;

use egui_kittest::{Harness, kittest::Queryable};
use pure_desktop::ui::shell::Panels;

mod testutil;

/// A harness over the panels at `width`, drawing the three bodies as
/// labeled columns so the geometry is readable.
fn harness_for(width: f32, panels: Panels) -> Harness<'static, Panels> {
    Harness::builder()
        .with_size(egui::Vec2::new(width, 600.0))
        .build_ui_state(
            |ui, panels: &mut Panels| {
                testutil::install_theme(ui.ctx(), "pure-shadow");
                pure_desktop::ui::shell::show(
                    ui,
                    panels,
                    |ui| {
                        let palette = pure_desktop::theme::Palette::active_of(ui.ctx());
                        ui.colored_label(palette.accent, "sidebar");
                    },
                    |ui| {
                        let palette = pure_desktop::theme::Palette::active_of(ui.ctx());
                        ui.colored_label(palette.ink_strong, "chat");
                    },
                    |ui| {
                        let palette = pure_desktop::theme::Palette::active_of(ui.ctx());
                        ui.colored_label(palette.info, "workbench");
                    },
                );
            },
            panels,
        )
}

/// The x offset where the column `label` drew, or `None` when it did
/// not draw at all.
fn column_x(harness: &mut Harness<'static, Panels>, label: &str) -> Option<f32> {
    harness.run();
    harness.query_by_label(label).map(|node| node.rect().min.x)
}

#[test]
fn the_sidebar_takes_the_left_column_at_the_design_width() {
    let mut harness = harness_for(1200.0, Panels::default());
    let sidebar = column_x(&mut harness, "sidebar").expect("the sidebar draws");
    let chat = column_x(&mut harness, "chat").expect("the chat draws");
    // The panel's frame insets its contents, so the label sits a
    // little in from the edge; the column itself starts at zero.
    assert!(
        sidebar.abs() < 60.0,
        "the sidebar starts at the left edge, got {sidebar}"
    );
    assert!(
        chat > 200.0,
        "the chat starts past the sidebar's column, got {chat}"
    );
}

#[test]
fn the_workbench_takes_the_right_column_when_open() {
    let mut panels = Panels::default();
    panels.toggle_workbench();
    let mut harness = harness_for(1400.0, panels);
    let workbench = column_x(&mut harness, "workbench").expect("the workbench draws");
    assert!(
        workbench > 900.0,
        "the workbench takes the right column, got {workbench}"
    );
}

#[test]
fn the_sidebar_collapses_with_its_toggle() {
    let mut panels = Panels::default();
    panels.toggle_sidebar();
    let mut harness = harness_for(1200.0, panels);
    assert!(
        column_x(&mut harness, "sidebar").is_none(),
        "a toggled-off sidebar takes no column"
    );
    assert!(column_x(&mut harness, "chat").is_some(), "the chat stays");
}

#[test]
fn the_narrow_sidebar_floats_over_the_content() {
    let mut panels = Panels::default();
    panels.measure(700.0);
    panels.toggle_sidebar();
    assert!(panels.sidebar_floats(), "the narrow sidebar floats");
    assert!(panels.sidebar_visible, "and stays visible");
}

#[test]
fn the_themed_shell_paints_the_palette_background() {
    let mut harness = harness_for(1200.0, Panels::default());
    harness.run();
    let img = harness.render().expect("the frame renders");
    // The chat column holds the app background, and the sidebar its
    // own surface token: the two together cover the painted frame.
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
    let bg = counts
        .get(&egui::Color32::from_rgb(0x0F, 0x0E, 0x13))
        .copied()
        .unwrap_or(0);
    let sidebar = counts
        .get(&egui::Color32::from_rgb(0x0B, 0x0A, 0x0E))
        .copied()
        .unwrap_or(0);
    let share = (bg + sidebar) as f32 / painted as f32;
    assert!(
        share > 0.9,
        "the palette surfaces hold {share} of the frame"
    );
}

#[test]
fn the_floating_workbench_runs_the_window_height() {
    let mut harness = harness_for(1000.0, Panels::default());
    harness.run();
    harness.state_mut().toggle_workbench();
    harness.run();
    harness.run();
    let rect = harness
        .ctx
        .memory(|memory| memory.area_rect(egui::Id::new("workbench-float")))
        .expect("the workbench floats at this width");
    assert!(
        (rect.height() - 600.0).abs() < 1.0,
        "the float covers the window top to bottom, got {rect:?}"
    );
}

#[test]
fn the_floating_sidebar_and_its_scrim_run_the_window_height() {
    let mut harness = harness_for(800.0, Panels::default());
    harness.run();
    harness.state_mut().toggle_sidebar();
    harness.run();
    harness.run();
    let rect = harness
        .ctx
        .memory(|memory| memory.area_rect(egui::Id::new("side-scrim")))
        .expect("the sidebar floats at this width");
    assert!(
        (rect.height() - 600.0).abs() < 1.0 && (rect.width() - 800.0).abs() < 1.0,
        "the scrim covers the window, got {rect:?}"
    );
}

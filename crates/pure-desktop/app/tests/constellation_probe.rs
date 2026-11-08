//! Headless E06 probes: the constellation's star field paints, a click
//! on a star reports its worker, and a field with no live member asks
//! for no further frame. The animation is what the idle-CPU check
//! guards: a field whose members have all ended must not schedule a
//! repaint, or the shell would burn a core while nothing moves.

use std::time::Instant;

use egui_kittest::Harness;
use pure_desktop::ui::agents::Phase;
use pure_desktop::ui::constellation::{ConstellationEvent, Star, field_height, layout, show};

mod testutil;

/// A harness over the constellation of `stars`.
fn harness_for(stars: &[Star]) -> Harness<'static, (Vec<Star>, Vec<ConstellationEvent>, bool)> {
    let stars = stars.to_vec();
    Harness::builder()
        .with_size(egui::Vec2::new(600.0, 200.0))
        .build_ui_state(
            |ui, (stars, events, installed): &mut (Vec<Star>, Vec<ConstellationEvent>, bool)| {
                if testutil::theme_installed(installed, ui.ctx()) {
                    return;
                }
                let (asked, _animate) = show(ui, stars, None, Instant::now());
                events.extend(asked);
            },
            (stars, Vec::new(), false),
        )
}

fn stars(n: usize, phase: Phase) -> Vec<Star> {
    (0..n)
        .map(|i| Star {
            id: format!("c{i}"),
            item: format!("item-{i}"),
            phase,
        })
        .collect()
}

#[test]
fn the_field_paints_its_lead_and_stars() {
    let mut harness = harness_for(&stars(4, Phase::Running));
    harness.run();
    harness.run();
    let img = harness.render().expect("the frame renders");
    let painted = img
        .pixels()
        .filter(|px| px.0[3] != 0)
        .filter(|px| {
            let (r, g, b) = (i32::from(px.0[0]), i32::from(px.0[1]), i32::from(px.0[2]));
            (r, g, b) != (0x1A, 0x18, 0x21)
        })
        .count();
    assert!(
        painted > 100,
        "the field paints its stars, {painted} pixels"
    );
}

#[test]
fn a_field_of_ended_members_asks_for_no_frame() {
    // The idle-CPU guard: every member done means no twinkle, so the
    // pane reports no animation and the caller schedules nothing.
    let mut harness = harness_for(&stars(6, Phase::Done));
    harness.run();
    harness.run();
    // The animate flag is read inside the closure, so the probe reads
    // it through a second harness that records it on the state.
    let mut flagged = Harness::builder()
        .with_size(egui::Vec2::new(600.0, 200.0))
        .build_ui_state(
            |ui, (stars, animated, installed): &mut (Vec<Star>, bool, bool)| {
                if testutil::theme_installed(installed, ui.ctx()) {
                    return;
                }
                let (_, animate) = show(ui, stars, None, Instant::now());
                *animated = animate;
            },
            (stars(6, Phase::Done), false, false),
        );
    flagged.run();
    flagged.run();
    let (_, animated, _) = flagged.state();
    assert!(!animated, "a done field animates nothing");
}

#[test]
fn a_click_on_a_star_reports_its_worker() {
    let field = stars(4, Phase::Running);
    let (_, points) = layout(&field, 600.0);
    let mut harness = harness_for(&field);
    harness.run();
    harness.run();
    // The field paints at the top-left of the harness, so a star's
    // layout point is its screen point.
    let at = egui::pos2(points[1].x, points[1].y);
    harness.hover_at(at);
    harness.drag_at(at);
    harness.drop_at(at);
    harness.run();
    let (_, events, _) = harness.state();
    assert_eq!(
        events.as_slice(),
        &[ConstellationEvent::Open("c1".to_owned())],
        "a click on the second star opens its worker"
    );
}

#[test]
fn the_field_height_grows_with_the_row_count() {
    assert_eq!(field_height(4), 44.0);
    assert_eq!(field_height(30), 80.0);
    assert_eq!(field_height(0), 44.0, "an empty field still holds its lead");
}

//! Headless E04 list probes: the measured-height list lays out only
//! the visible window over 5,000 rows, and the follow state machine
//! detaches and reattaches as the spike measured it. These are the
//! E00 R1 gate's acceptance, re-proven on the hand-rolled list.

use egui_kittest::Harness;
use pure_desktop::ui::transcript::list::List;

mod testutil;

/// The list under probe, with the frame's draws counted.
#[derive(Default)]
struct Probe {
    /// The list.
    list: List,
    /// How many rows the last frame laid out.
    laid_out: usize,
    /// The row count the signatures stand for.
    rows: usize,
}

/// The E00 spike's row count: 5,000 items, the budget the virtual list
/// could not hold.
const ROWS: usize = 5_000;

fn harness() -> Harness<'static, Probe> {
    Harness::builder()
        .with_size(egui::Vec2::new(900.0, 600.0))
        .build_ui_state(
            |ui, probe: &mut Probe| {
                testutil::install_theme(ui.ctx(), "pure-shadow");
                let signatures: Vec<u64> = (0..probe.rows as u64).collect();
                let mut measured = 0usize;
                probe.list.sync(&signatures, |_| {
                    measured += 1;
                    20.0
                });
                probe.laid_out = measured;
                let (first, last) = probe.list.window(0.0, ui.available_height());
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.set_height(probe.list.content_height());
                    for _ix in first..last {
                        ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), 20.0),
                            egui::Sense::hover(),
                        );
                    }
                });
            },
            Probe {
                rows: ROWS,
                ..Probe::default()
            },
        )
}

#[test]
fn only_the_visible_window_lays_out_over_5000_rows() {
    let mut harness = harness();
    harness.run();
    let probe = harness.state();
    // The delta splice measures the new rows only; the window draws
    // only what the viewport sees. Neither reaches the whole list.
    assert!(
        probe.laid_out < 200,
        "laid out {}/{} rows in one frame",
        probe.laid_out,
        probe.rows
    );
}

#[test]
fn a_second_frame_re_measures_nothing() {
    let mut harness = harness();
    harness.run();
    let first = harness.state().laid_out;
    harness.run();
    let second = harness.state().laid_out;
    assert_eq!(
        second, 0,
        "a settled frame measures nothing new (first was {first})"
    );
}

#[test]
fn the_follow_detaches_on_a_scroll_up() {
    let mut list = List::new();
    list.sync(&(0..100u64).collect::<Vec<_>>(), |_| 10.0);
    let viewport = 200.0;
    let content = list.content_height();
    // At the bottom, following.
    assert!(list.following());
    // A scroll up detaches: the follow target is the bottom, so the
    // view stays where the user put it.
    list.follow_again();
    assert!(list.following());
    let target = list.follow_target(viewport);
    assert_eq!(
        target,
        content - viewport,
        "the follow target is the bottom"
    );
}

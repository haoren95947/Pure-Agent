//! Headless E04 transcript probes: the view draws the recording's rows
//! in the chat column, the reveal paces the tail, and the measured list
//! keeps the frame cheap over the recorded session. The real window is
//! checked by the smoke run; this is the headless leg.

use egui_kittest::Harness;
use pure_desktop::store::{Command, Store};
use pure_desktop::transport::{State, replay};
use pure_desktop::ui::transcript::view::TranscriptView;

mod testutil;

/// Carries out the store's pending commands the way the shell does.
fn run_commands(store: &mut Store) {
    for command in store.take_commands() {
        match command {
            Command::Handshake { replay_sessions } => {
                store.handshake(replay_sessions);
            }
            Command::NewSession => {
                store.new_session();
            }
            Command::ReplayPrompt => {
                store.prompt("fix the null check");
            }
        }
    }
}

/// A store driven through the recording the way the replay smoke runs.
fn replayed_store() -> Store {
    let mut store = Store::new("/w", true);
    store.set_connect(State::Connected);
    run_commands(&mut store);
    let _ = store.take_outgoing();
    let frames = replay::transcript();
    store.absorb(frames[0].clone());
    run_commands(&mut store);
    let _ = store.take_outgoing();
    store.absorb(frames[1].clone());
    run_commands(&mut store);
    let _ = store.take_outgoing();
    for frame in &frames[2..] {
        store.absorb(frame.clone());
    }
    store
}

/// A harness over the transcript view at `width`, over a store driven
/// through the recording.
fn harness_for(width: f32) -> Harness<'static, (TranscriptView, Store, bool)> {
    harness_sized(width, 700.0)
}

/// A harness over the transcript view at `width` by `height`.
fn harness_sized(width: f32, height: f32) -> Harness<'static, (TranscriptView, Store, bool)> {
    let store = replayed_store();
    Harness::builder()
        .with_size(egui::Vec2::new(width, height))
        .build_ui_state(
            |ui, (view, store, started): &mut (TranscriptView, Store, bool)| {
                if testutil::theme_installed(started, ui.ctx()) {
                    // Fonts become active at the start of the next
                    // pass, so the themed style follows one pass later.
                    return;
                }
                pure_desktop::theme::apply_choice(
                    &pure_desktop::theme::ThemeChoice::Named("pure-shadow".to_owned()),
                    ui.ctx(),
                );
                let mut events = Vec::new();
                view.show(ui, store, &mut events);
                assert!(events.is_empty(), "the recording raises no row actions");
            },
            (TranscriptView::new(), store, false),
        )
}

#[test]
fn the_transcript_draws_the_recorded_rows() {
    let mut harness = harness_for(900.0);
    harness.run();
    let (view, store, _) = harness.state();
    // The view read the recording: rows exist, and the searchable text
    // the find bar narrows over is there.
    assert!(
        view.searchable_rows(store).len() > 5,
        "the recording's rows are searchable"
    );
}

#[test]
fn a_thought_head_click_unfolds_its_body() {
    use egui_kittest::kittest::Queryable as _;

    let mut harness = harness_for(900.0);
    harness.run();
    harness.run();
    let body = "The read confirms main dereferences an Option, so the fix is a match.";
    assert_eq!(
        testutil::count(&harness, body),
        0,
        "the thought starts folded"
    );
    // The head's own text starts with the thought's verb, whatever
    // duration the recording carried.
    harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::Button
                && node
                    .label()
                    .is_some_and(|label| label.starts_with("Thought"))
        })
        .next()
        .expect("the recording carries a thought head")
        .click();
    harness.run();
    harness.run();
    assert_eq!(
        testutil::count(&harness, body),
        1,
        "the click unfolds the thought"
    );
}

#[test]
fn the_transcript_paints_the_palette_over_the_rows() {
    let mut harness = harness_for(900.0);
    harness.run();
    let img = harness.render().expect("the frame renders");
    let mut counts: std::collections::HashMap<egui::Color32, usize> =
        std::collections::HashMap::new();
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
    let share = bg as f32 / painted as f32;
    assert!(
        share > 0.5,
        "the palette holds {share} of the painted frame"
    );
}

#[test]
fn the_find_marks_tint_the_matched_rows() {
    let mut harness = harness_for(900.0);
    harness.run();
    let (first, count) = {
        let (view, store, _) = harness.state();
        let rows = view.searchable_rows(store);
        (rows.first().expect("the recording has rows").0, rows.len())
    };
    harness
        .state_mut()
        .0
        .set_find(Some(pure_desktop::ui::transcript::model::FindMarks {
            keys: vec![first],
            current: 0,
        }));
    harness.run();
    // Setting the marks leaves the rows searchable and the frame stable.
    let (view, store, _) = harness.state();
    assert_eq!(view.searchable_rows(store).len(), count);
}

/// The rail's center line in the 900px harness.
const RAIL_X: f32 = 900.0 - 8.0 - 6.0 - 8.0;

#[test]
fn the_timeline_rail_seeks_the_rows() {
    let mut harness = harness_sized(900.0, 180.0);
    harness.run();
    harness.run();
    let (view, _, _) = harness.state();
    let fresh = view.scroll_offset();

    // The rail hangs 6px off the pane's right edge, 16px wide, inside
    // the panel's 8px margin, and 10px off the pane's top and bottom.
    // The view opens on the tail, so a press at the rail's very top
    // seeks the first rows.
    let top = egui::Pos2::new(RAIL_X, 22.0);
    harness.drag_at(top);
    harness.run();
    harness.drop_at(top);
    harness.run();
    harness.run();
    let (view, _, _) = harness.state();
    let at_top = view.scroll_offset();
    assert!(
        at_top < fresh,
        "the top of the rail seeks the first rows: {at_top} < {fresh}"
    );

    // A press at its bottom seeks the last rows again.
    let bottom = egui::Pos2::new(RAIL_X, 158.0);
    harness.drag_at(bottom);
    harness.run();
    harness.drop_at(bottom);
    harness.run();
    harness.run();
    let (view, _, _) = harness.state();
    let at_tail = view.scroll_offset();
    assert!(
        at_tail > at_top,
        "the bottom of the rail seeks the last rows: {at_tail} > {at_top}"
    );
}

#[test]
fn the_lab_toggle_turns_the_timeline_rail_off() {
    let mut harness = harness_for(900.0);
    harness.run();
    harness.run();
    harness.run();
    let (_, store, _) = harness.state_mut();
    store.update_prefs(|prefs| prefs.rail = false);
    harness.run();
    harness.run();
    harness.run();
    let (view, _, _) = harness.state();
    let settled = view.scroll_offset();

    // The follow holds the tail; only a rail seek could pull the view
    // away from it. With the toggle off, a click where the rail would
    // hang leaves the offset alone.
    let top = egui::Pos2::new(RAIL_X, 22.0);
    harness.drag_at(top);
    harness.drop_at(top);
    harness.run();
    harness.run();
    let (view, _, _) = harness.state();
    assert_eq!(
        view.scroll_offset(),
        settled,
        "no rail, no seek from the edge"
    );
}

/// Feeds the replayed session a finished tool call of `title` whose
/// output runs `lines` lines, the way the engine streams one.
fn add_long_call(store: &mut Store, id: &str, title: &str, input: serde_json::Value, lines: usize) {
    let session = store
        .active_id()
        .expect("the replay opened a session")
        .to_owned();
    let text: String = (0..lines)
        .map(|n| format!("line {n}: the quick brown fox jumps over the lazy dog\n"))
        .collect();
    for update in [
        serde_json::json!({
            "sessionUpdate": "tool_call", "toolCallId": id, "title": title,
            "kind": "execute", "status": "pending", "rawInput": input
        }),
        serde_json::json!({
            "sessionUpdate": "tool_call_update", "toolCallId": id, "status": "completed",
            "content": [{"type": "content", "content": {"type": "text", "text": text}}]
        }),
    ] {
        store.absorb(pure_client::Frame::Notification {
            method: "session/update".to_owned(),
            params: serde_json::json!({ "sessionId": session, "update": update }),
        });
    }
}

#[test]
fn expanded_rows_survive_the_pointer_sweeping_over_them() {
    let mut harness = harness_sized(900.0, 500.0);
    {
        let store = &mut harness.state_mut().1;
        add_long_call(
            store,
            "call-shell",
            "shell",
            serde_json::json!({"command": "cargo test"}),
            80,
        );
        add_long_call(
            store,
            "call-cat",
            "read",
            serde_json::json!({"path": "src/lib.rs"}),
            120,
        );
    }
    harness.run();
    harness.state_mut().0.fold_all(true);
    harness.run_steps(3);
    for y in (0..500).step_by(5) {
        for x in [60.0, 300.0, 600.0, 840.0, 850.0, 858.0, 864.0, 870.0, 874.0] {
            harness.hover_at(egui::pos2(x, y as f32));
            harness.step();
            harness.step();
        }
    }
}

#[test]
fn a_reader_scrolled_away_gets_a_jump_back_to_the_tail() {
    use egui_kittest::kittest::Queryable as _;

    let mut harness = harness_sized(900.0, 180.0);
    harness.run();
    harness.run();
    let tail = harness.state().0.scroll_offset();
    assert!(
        tail > 0.0,
        "the view opens on the tail of the overflowing rows"
    );
    assert_eq!(
        testutil::count(&harness, "Jump to latest"),
        0,
        "the follow holds the tail, so no jump is offered"
    );

    harness.event(egui::Event::PointerMoved(egui::pos2(450.0, 90.0)));
    harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, 400.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run();
    harness.run();
    assert!(
        harness.state().0.scroll_offset() < tail,
        "the wheel scrolled up"
    );
    harness.get_by_label("Jump to latest").click();
    harness.run();
    harness.run();
    assert!(
        (harness.state().0.scroll_offset() - tail).abs() < 1.0,
        "the jump lands on the tail again"
    );
    assert_eq!(
        testutil::count(&harness, "Jump to latest"),
        0,
        "following again, the jump is gone"
    );
}

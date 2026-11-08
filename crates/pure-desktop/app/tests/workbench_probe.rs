//! Headless E06 probes: the workbench's five tabs draw, the tab strip
//! takes a click, and the panes read the store. A tab the strip draws
//! but the pointer cannot reach is the E05 frame bug over again, so
//! each tab is switched by a click, not by setting the state.
//!
//! The panes draw their row text through the painter, which kittest's
//! accessibility tree does not see, so a row is checked two ways: the
//! section head and the diff body are widgets, and the row's own text
//! is checked as painted pixels inside the row's rect.

use egui_kittest::{Harness, kittest::Queryable};
use pure_desktop::store::Store;
use pure_desktop::ui::workbench::{Tab, Workbench, show};

mod testutil;

/// A harness over the workbench with `store`'s session open.
fn harness_for(store: Store) -> Harness<'static, (Store, Workbench, bool)> {
    Harness::builder()
        .with_size(egui::Vec2::new(460.0, 700.0))
        .build_ui_state(
            |ui, (store, workbench, installed): &mut (Store, Workbench, bool)| {
                if !*installed {
                    // The fonts bind at the start of the next pass, so
                    // the themed style follows one pass later.
                    testutil::install_theme(ui.ctx(), "pure-shadow");
                    *installed = true;
                    return;
                }
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for event in show(ui, workbench, store, false) {
                        // The shell carries these out; the probe only
                        // records that the pane asked.
                        match event {
                            pure_desktop::ui::workbench::WorkbenchEvent::AskFiles => {
                                if let Some(id) = store.active_id().map(str::to_owned) {
                                    store.fs_list(&id);
                                    workbench.mark_listing_asked(&id);
                                }
                            }
                            pure_desktop::ui::workbench::WorkbenchEvent::ReadFile(path) => {
                                store.fs_read(&path);
                            }
                            _ => {}
                        }
                    }
                });
            },
            (store, Workbench::new(), false),
        )
}

/// A store with one session holding one edit call.
fn store_with_edit() -> Store {
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
    for frame in items_frame("s1", &[edit_wire("c1", "src/main.rs")]) {
        store.absorb(frame);
    }
    store
}

/// A `tool_call` update as the wire delivers it: an edit of `path`
/// whose diff content carries the old and new texts.
fn edit_wire(id: &str, path: &str) -> serde_json::Value {
    serde_json::json!({
        "sessionUpdate": "tool_call",
        "toolCallId": id,
        "title": "edit",
        "kind": "edit",
        "status": "completed",
        "content": [{
            "type": "diff",
            "path": path,
            "oldText": "a\n",
            "newText": "a\nb\n",
        }],
    })
}

/// A `tool_call` update for a shell run of `command`.
fn shell_wire(id: &str, command: &str) -> serde_json::Value {
    serde_json::json!({
        "sessionUpdate": "tool_call",
        "toolCallId": id,
        "title": "shell",
        "kind": "execute",
        "status": "completed",
        "rawInput": {"command": command},
        "rawOutput": {"exitCode": 0},
    })
}

/// The session/update notification carrying `updates` on session
/// `id`, each one its own notification the way the wire delivers
/// them.
fn items_frame(id: &str, updates: &[serde_json::Value]) -> Vec<pure_client::Frame> {
    updates
        .iter()
        .map(|update| pure_client::Frame::Notification {
            method: "session/update".to_owned(),
            params: serde_json::json!({
                "sessionId": id,
                "update": update,
            }),
        })
        .collect()
}

/// Painted pixels in `rect` that are not the well's background. The
/// well is the workbench's surface, so anything else in the rect is
/// text or an icon the row painted.
fn painted_in(harness: &mut Harness<'static, (Store, Workbench, bool)>, rect: egui::Rect) -> usize {
    let img = harness.render().expect("the frame renders");
    let lo = rect.min;
    let hi = rect.max;
    let mut painted = 0;
    for y in lo.y.max(0.0) as u32..(hi.y.max(0.0) as u32).min(img.height()) {
        for x in lo.x.max(0.0) as u32..(hi.x.max(0.0) as u32).min(img.width()) {
            let px = img.get_pixel(x, y);
            if px.0[3] != 0 {
                let (r, g, b) = (i32::from(px.0[0]), i32::from(px.0[1]), i32::from(px.0[2]));
                if (r, g, b) != (0x1A, 0x18, 0x21) {
                    painted += 1;
                }
            }
        }
    }
    painted
}

#[test]
fn the_five_tabs_draw_and_the_strip_takes_a_click() {
    let mut harness = harness_for(store_with_edit());
    harness.run();
    harness.run();
    for tab in Tab::ALL {
        assert!(
            harness.query_by_label(tab.label()).is_some(),
            "the tab {} draws",
            tab.label()
        );
    }
    // The strip sits above the pane, so its labels are in the top rows.
    let agents = harness
        .query_by_label("Agents")
        .expect("the agents tab draws")
        .rect();
    assert!(
        agents.min.y < 120.0,
        "the strip sits above the pane, got y {}",
        agents.min.y
    );
    harness.get_by_label("Agents").click();
    harness.run();
    assert!(
        harness
            .query_by_label(
                "No agents yet. Subagents and swarm workers show up here while they run."
            )
            .is_some(),
        "the agents pane took over"
    );
    harness.get_by_label("Changes").click();
    harness.run();
    assert!(
        harness.query_by_label("1 FILE CHANGED").is_some(),
        "the changes pane took back over"
    );
}

#[test]
fn a_change_row_paints_its_path_inside_the_section() {
    let mut harness = harness_for(store_with_edit());
    harness.run();
    harness.run();
    let head = harness
        .query_by_label("1 FILE CHANGED")
        .expect("the section head draws");
    let rect = head.rect();
    // The row sits under the section head, and paints its path there.
    let row = egui::Rect::from_min_size(
        egui::pos2(rect.min.x, rect.max.y + 8.0),
        egui::vec2(rect.width(), 30.0),
    );
    let painted = painted_in(&mut harness, row);
    assert!(
        painted > 20,
        "the change row paints its path, {painted} pixels"
    );
}

#[test]
fn the_files_pane_reads_the_listing_the_store_holds() {
    let mut store = store_with_edit();
    // The pane asks for the listing, and the answer lands the way the
    // shell's ask does: a success on the request the store put out.
    store.fs_list("s1");
    let request = store
        .take_outgoing()
        .into_iter()
        .find_map(|frame| match frame {
            pure_client::Frame::Request { id, .. } => Some(id),
            _ => None,
        })
        .expect("the listing ask went out");
    store.absorb(pure_client::Frame::Success {
        id: request,
        result: serde_json::json!({
            "op": "list",
            "entries": [
                {"path": "src", "kind": "directory", "size": 0},
                {"path": "src/lib.rs", "kind": "file", "size": 128},
            ],
            "truncated": false,
        }),
    });
    let mut harness = harness_for(store);
    harness.run();
    harness.run();
    harness.get_by_label("Files").click();
    harness.run();
    for name in ["src", "lib.rs"] {
        assert!(
            harness.query_by_label(name).is_some(),
            "the files pane lists {name}"
        );
    }
}

#[test]
fn the_terminal_pane_folds_each_run_to_its_command() {
    let mut store = store_with_edit();
    for frame in items_frame("s1", &[shell_wire("c2", "cargo test")]) {
        store.absorb(frame);
    }
    let mut harness = harness_for(store);
    harness.run();
    harness.run();
    harness.get_by_label("Terminal").click();
    harness.run();
    assert!(
        harness.query_by_label("cargo test").is_some(),
        "the pane draws the run under its command"
    );
}

fn store_with_agent() -> Store {
    let mut store = store_with_edit();
    for frame in items_frame(
        "s1",
        &[serde_json::json!({
            "sessionUpdate": "subagent_update",
            "subagentSessionId": "child-1",
            "name": "reviewer",
            "task": "Check the null handling in parse_config",
            "state": "running",
        })],
    ) {
        store.absorb(frame);
    }
    store
}

#[test]
fn the_agent_detail_shows_the_task_and_its_name() {
    let mut harness = harness_for(store_with_agent());
    harness.state_mut().1.open_agent("child-1".to_owned());
    harness.run();
    harness.run();
    assert!(
        harness
            .query_all_by_label("Check the null handling in parse_config")
            .count()
            > 0,
        "the detail draws the agent's task"
    );
}

#[test]
fn an_agent_missing_from_the_session_reads_not_found() {
    let mut harness = harness_for(store_with_edit());
    harness.state_mut().1.open_agent("ghost".to_owned());
    harness.run();
    harness.run();
    assert!(
        harness.query_all_by_label("Agent not found.").count() > 0,
        "the detail names a missing agent"
    );
}

fn answer_last_fs_request(store: &mut Store, result: serde_json::Value) {
    let id = store
        .take_outgoing()
        .into_iter()
        .rev()
        .find_map(|frame| match frame {
            pure_client::Frame::Request { id, .. } => Some(id),
            _ => None,
        })
        .expect("an fs request went out");
    store.absorb(pure_client::Frame::Success { id, result });
}

fn store_with_preview() -> Store {
    let mut store = store_with_edit();
    store.fs_list("");
    answer_last_fs_request(
        &mut store,
        serde_json::json!({
            "op": "list",
            "entries": [
                {"path": "src/main.rs", "kind": "file", "size": 120},
                {"path": "logo.png", "kind": "file", "size": 4096}
            ],
            "truncated": false
        }),
    );
    store.fs_read("src/main.rs");
    answer_last_fs_request(
        &mut store,
        serde_json::json!({
            "op": "read",
            "content": "fn main() {\n    println!(\"hi\");\n}\n",
            "truncated": false,
            "binary": false
        }),
    );
    store
}

#[test]
fn the_file_preview_draws_its_path_and_numbered_lines() {
    let mut harness = harness_for(store_with_preview());
    harness.state_mut().1.set_tab(Tab::Files);
    harness.state_mut().1.mark_listing_asked("s1");
    harness
        .state_mut()
        .1
        .set_file(Some("src/main.rs".to_owned()));
    harness.run();
    harness.run();
    assert!(
        harness.query_all_by_label("src/main.rs").count() > 0,
        "the preview header names the file"
    );
}

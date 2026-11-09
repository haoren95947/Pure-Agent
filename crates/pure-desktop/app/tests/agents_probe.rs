//! The agent card in the real shell while its subagent runs: a click
//! on the card opens the agent's transcript in the workbench, and the
//! transcript follows the child session as it streams.

use std::sync::{Arc, Mutex};

use egui_kittest::Harness;
use pure_desktop::transport::{Event, EventSender, Link, State, Transport};

mod testutil;

/// The parent session the handshake opens.
const PARENT: &str = "01KA5C0D1NG000000000000000";
/// The subagent's session.
const CHILD: &str = "01KA5C0D1NG000000000000001";

/// A transport the probe feeds by hand: it connects at once and hands
/// over whatever frame the probe pushes.
#[derive(Clone, Default)]
struct Scripted {
    /// The shell's inbox, once the shell started the transport.
    events: Arc<Mutex<Option<EventSender>>>,
}

impl Transport for Scripted {
    fn start(&mut self, events: EventSender) {
        let _ = events.try_send(Event::State(State::Connected));
        *self.events.lock().expect("the inbox lock") = Some(events);
    }

    fn send(&self, _frame: pure_client::Frame) {}

    fn close(&self) {}

    fn link(&self) -> Link {
        Link {
            name: "replay",
            detail: "scripted".to_owned(),
        }
    }
}

/// The state a shell harness carries between frames.
struct Rig {
    /// The real window body, built on the harness's own context.
    app: Option<pure_desktop::launch::ShellApp>,
    /// Set once the theme has been installed on a frame that did not
    /// draw.
    installed: bool,
    /// The transport the probe feeds.
    transport: Scripted,
}

/// A harness over the real shell over a hand-fed transport.
fn harness() -> Harness<'static, Rig> {
    Harness::builder()
        .with_size(egui::Vec2::new(1280.0, 800.0))
        .build_ui_state(
            |ui, rig: &mut Rig| {
                if !rig.installed {
                    testutil::install_theme(ui.ctx(), "pure-shadow");
                    rig.installed = true;
                    return;
                }
                let transport = rig.transport.clone();
                let app = rig.app.get_or_insert_with(|| {
                    pure_desktop::launch::ShellApp::new(
                        "/w".to_owned(),
                        true,
                        Box::new(transport),
                        ui.ctx(),
                    )
                });
                eframe::App::ui(app, ui, &mut eframe::Frame::_new_kittest());
            },
            Rig {
                app: None,
                installed: false,
                transport: Scripted::default(),
            },
        )
}

/// Hands the shell `frame` and steps until it has been drawn.
fn push(harness: &mut Harness<'static, Rig>, frame: &serde_json::Value) {
    let frame = pure_client::Frame::parse(frame).expect("a JSON-RPC frame");
    let events = harness
        .state()
        .transport
        .events
        .lock()
        .expect("the inbox lock")
        .clone()
        .expect("the shell started the transport");
    events
        .try_send(Event::Frame(frame))
        .expect("the inbox is open");
    harness.step();
    harness.step();
}

/// Hands the shell one `session/update` of `session`.
fn update(harness: &mut Harness<'static, Rig>, session: &str, update: serde_json::Value) {
    push(
        harness,
        &serde_json::json!({
            "jsonrpc": "2.0", "method": "session/update",
            "params": {"sessionId": session, "update": update}
        }),
    );
}

/// Plays the recorded handshake and session answers, so the parent
/// session is open and active.
fn opened(harness: &mut Harness<'static, Rig>) {
    for _ in 0..4 {
        harness.step();
    }
    let fixture = include_str!("../../pure-client/tests/fixtures/subagents.jsonl");
    for line in fixture.lines().take(2) {
        push(
            harness,
            &serde_json::from_str(line).expect("a fixture line"),
        );
    }
    for _ in 0..20 {
        harness.step();
    }
}

#[test]
fn a_running_agent_opens_in_the_workbench_and_streams_there() {
    let mut harness = harness();
    opened(&mut harness);
    update(
        &mut harness,
        PARENT,
        serde_json::json!({"sessionUpdate": "_pure/turn", "phase": "start"}),
    );
    update(
        &mut harness,
        PARENT,
        serde_json::json!({
            "sessionUpdate": "tool_call", "toolCallId": "call-agent", "title": "agent",
            "kind": "other", "status": "pending",
            "rawInput": {"description": "list files", "prompt": "list"}
        }),
    );
    update(
        &mut harness,
        PARENT,
        serde_json::json!({
            "sessionUpdate": "subagent_update", "subagentSessionId": CHILD,
            "name": "general", "task": "list files", "toolCallId": "call-agent",
            "capabilities": {"cancel": true}
        }),
    );
    update(
        &mut harness,
        CHILD,
        serde_json::json!({"sessionUpdate": "_pure/turn", "phase": "start"}),
    );
    assert_eq!(
        testutil::count(&harness, "Stop"),
        0,
        "the workbench is closed"
    );

    use egui_kittest::kittest::Queryable as _;
    harness.get_by_label("general").click_accesskit();
    for _ in 0..4 {
        harness.step();
    }
    assert_eq!(
        testutil::count(&harness, "Stop"),
        1,
        "the click opened the running agent's detail"
    );

    let reply = "Listed main.rs and lib.rs.";
    update(
        &mut harness,
        CHILD,
        serde_json::json!({
            "sessionUpdate": "agent_message_chunk",
            "content": {"type": "text", "text": reply}
        }),
    );
    for _ in 0..60 {
        harness.step();
        if testutil::count(&harness, reply) > 0 {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
    panic!("the child's reply never reached the agent's transcript");
}

#[test]
fn an_open_agent_holds_the_workbench_in_place_and_lets_it_narrow() {
    let mut harness = harness();
    opened(&mut harness);
    for (session, update_) in [
        (
            PARENT,
            serde_json::json!({"sessionUpdate": "_pure/turn", "phase": "start"}),
        ),
        (
            PARENT,
            serde_json::json!({
                "sessionUpdate": "tool_call", "toolCallId": "call-agent", "title": "agent",
                "kind": "other", "status": "pending",
                "rawInput": {"description": "list files", "prompt": "list"}
            }),
        ),
        (
            PARENT,
            serde_json::json!({
                "sessionUpdate": "subagent_update", "subagentSessionId": CHILD,
                "name": "general", "task": "list files", "toolCallId": "call-agent",
                "capabilities": {"cancel": true}
            }),
        ),
        (
            CHILD,
            serde_json::json!({"sessionUpdate": "_pure/turn", "phase": "start"}),
        ),
        (
            CHILD,
            serde_json::json!({
                "sessionUpdate": "tool_call", "toolCallId": "c1", "title": "bash",
                "kind": "execute", "status": "completed",
                "rawInput": {"command": "find . -name '*.rs' -not -path './target/*' -exec wc -l {} +"}
            }),
        ),
        (
            CHILD,
            serde_json::json!({
                "sessionUpdate": "agent_message_chunk",
                "content": {"type": "text", "text": "Found them in `src/some/deeply/nested/module/path/file.rs`.\n\n```rust\nlet listing = std::fs::read_dir(\"src\").expect(\"the source directory is readable\");\n```\n\n| file | lines |\n| --- | --- |\n| src/main.rs | 120 |\n| src/lib.rs | 80 and a note long enough to run far past the panel's edge, the way an agent's findings table does when one cell carries a paragraph of evidence with paths and line numbers in it, over and over again until the row is wider than any window |"}
            }),
        ),
    ] {
        update(&mut harness, session, update_);
    }
    use egui_kittest::kittest::Queryable as _;
    harness.get_by_label("general").click_accesskit();
    for _ in 0..10 {
        harness.step();
    }
    assert_eq!(
        testutil::count(&harness, "Stop"),
        1,
        "the click opened the agent's detail"
    );
    let panel = |harness: &Harness<'static, Rig>| {
        egui::containers::panel::PanelState::load(&harness.ctx, egui::Id::new("workbench"))
            .expect("the workbench docks at this width")
            .outer_rect
    };
    // A row wider than the panel once fed back into the panel's width
    // each frame, until the panel ran off the window.
    for _ in 0..30 {
        harness.step();
    }
    let opened = panel(&harness);
    assert!(
        opened.right() <= 1280.0 && opened.width() < 470.0,
        "the open agent keeps the panel where it was, got {opened:?}"
    );
    let edge = opened.left_center();
    let far = edge + egui::vec2(600.0, 0.0);
    harness.hover_at(edge);
    harness.step();
    harness.drag_at(edge);
    harness.step();
    harness.hover_at(far);
    harness.step();
    harness.drop_at(far);
    for _ in 0..4 {
        harness.step();
    }
    let width = panel(&harness).width();
    assert!(
        width < 250.0,
        "the agent's transcript lets the panel narrow, got {width}"
    );
}

#[test]
fn the_swarm_pill_lists_its_members_on_a_click() {
    use egui_kittest::kittest::Queryable as _;

    let mut harness = harness();
    opened(&mut harness);
    update(
        &mut harness,
        PARENT,
        serde_json::json!({"sessionUpdate": "_pure/turn", "phase": "start"}),
    );
    update(
        &mut harness,
        PARENT,
        serde_json::json!({
            "sessionUpdate": "plan",
            "entries": [{"content": "Survey", "priority": "medium", "status": "in_progress"}]
        }),
    );
    update(
        &mut harness,
        PARENT,
        serde_json::json!({
            "sessionUpdate": "subagent_update", "subagentSessionId": CHILD,
            "name": "surveyor", "task": "list files"
        }),
    );
    assert_eq!(
        testutil::count(&harness, "running"),
        0,
        "the list starts shut"
    );
    harness.get_by_label("Swarm").click_accesskit();
    harness.step();
    harness.step();
    assert_eq!(
        testutil::count(&harness, "surveyor"),
        1,
        "the click lists the member"
    );
    assert_eq!(testutil::count(&harness, "running"), 1, "with its state");
}

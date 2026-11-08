//! The permission ask, end to end: the real shell over a scripted
//! transport that plays the golden recording and then delivers the
//! engine's live `session/request_permission` frame. The approval card
//! must mount over the dock with the options the agent offered.

use egui_kittest::Harness;
use pure_client::{Frame, RequestId};

mod testutil;

use pure_desktop::transport::{Event, EventSender, Link, State, Transport};

/// The session id the recording's `session/new` answer carries.
const SESSION: &str = "01KA5C0D1NG000000000000000";

/// The recording, then the ask. The playback is the replay
/// transport's, copied: answers wait for the request they answer, so
/// the pump never drops one as unread.
#[derive(Clone, Default)]
struct Scripted {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    asked: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl Scripted {
    fn answer_id(frame: &Frame) -> Option<u64> {
        match frame {
            Frame::Success { id, .. } | Frame::Failure { id, .. } => id.as_number(),
            _ => None,
        }
    }

    fn wait_for(&self, id: u64) {
        while self.asked.load(std::sync::atomic::Ordering::SeqCst) < id
            && !self.stop.load(std::sync::atomic::Ordering::SeqCst)
        {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

impl Transport for Scripted {
    fn start(&mut self, events: EventSender) {
        let transport = self.clone();
        let spawned = std::thread::Builder::new()
            .name("scripted".to_owned())
            .spawn({
                let events = events.clone();
                move || {
                    let _ = events.send_blocking(Event::State(State::Connecting));
                    let _ = events.send_blocking(Event::State(State::Connected));
                    for frame in pure_desktop::transport::replay::transcript() {
                        if transport.stop.load(std::sync::atomic::Ordering::SeqCst) {
                            return;
                        }
                        if let Some(id) = Self::answer_id(&frame) {
                            transport.wait_for(id);
                        }
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        if events.send_blocking(Event::Frame(frame)).is_err() {
                            return;
                        }
                    }
                    // The engine's live ask, captured verbatim from
                    // `pure rpc` 0.3.0 answering a shell prompt; only the
                    // session id is rewritten to the recording's.
                    let mut params = serde_json::json!({
                        "sessionId": SESSION,
                        "toolCall": {
                            "toolCallId": "call_edb79995e1b14f11985920a2",
                            "title": "shell",
                            "kind": "execute",
                            "status": "pending",
                            "rawInput": { "command": "echo permission-probe" }
                        },
                        "options": [
                            { "optionId": "allow", "name": "Allow shell", "kind": "allow_once" },
                            { "optionId": "allow_session", "name": "Allow shell for this session",
                              "kind": "allow_always" },
                            { "optionId": "reject", "name": "Reject shell", "kind": "reject_once" }
                        ]
                    });
                    params["sessionId"] = serde_json::json!(SESSION);
                    let _ = events.send_blocking(Event::Frame(Frame::Request {
                        id: RequestId::Number(101),
                        method: "session/request_permission".into(),
                        params,
                    }));
                }
            });
        if spawned.is_err() {
            let _ = events.send_blocking(Event::State(State::Refused(
                "cannot start the scripted thread".to_owned(),
            )));
        }
    }

    fn send(&self, frame: Frame) {
        if let Frame::Request { id, .. } = &frame
            && let Some(n) = id.as_number()
        {
            self.asked.fetch_max(n, std::sync::atomic::Ordering::SeqCst);
        }
    }

    fn close(&self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    fn link(&self) -> Link {
        Link {
            name: "scripted",
            detail: "recording, then the live ask".to_owned(),
        }
    }
}

/// The state the harness carries between frames.
struct Rig {
    app: Option<pure_desktop::launch::ShellApp>,
    installed: bool,
    /// The last ask count the store reported, for the failure message.
    asks: usize,
}

/// Any node whose text mentions `needle`, walking depth first.
fn mentions(node: &egui_kittest::kittest::AccessKitNode<'_>, needle: &str) -> bool {
    let shown = node.value().into_iter().chain(node.label());
    if shown.into_iter().any(|text| text.contains(needle)) {
        return true;
    }
    node.children().any(|child| mentions(&child, needle))
}

/// Every node's text, walking depth first.
fn collect(node: &egui_kittest::kittest::AccessKitNode<'_>, texts: &mut Vec<String>) {
    if let Some(text) = node.value()
        && !text.trim().is_empty()
    {
        texts.push(text);
    }
    for child in node.children() {
        collect(&child, texts);
    }
}

#[test]
fn the_live_ask_opens_the_approval_card() {
    let mut harness: Harness<'static, Rig> = Harness::builder()
        .with_size(egui::Vec2::new(1252.0, 1317.0))
        .build_ui_state(
            |ui, rig: &mut Rig| {
                if !rig.installed {
                    testutil::install_theme(ui.ctx(), "pure-shadow");
                    rig.installed = true;
                    return;
                }
                let app = rig.app.get_or_insert_with(|| {
                    pure_desktop::launch::ShellApp::new(
                        std::env::temp_dir().to_string_lossy().into_owned(),
                        true,
                        Box::new(Scripted::default()),
                        ui.ctx(),
                    )
                });
                rig.asks = app.shell.store().active_asks().len();
                eframe::App::ui(app, ui, &mut eframe::Frame::_new_kittest());
            },
            Rig {
                app: None,
                installed: false,
                asks: 0,
            },
        );
    // The recording plays at its 100 ms beat; the card mounts when the
    // ask lands after it.
    let mut found = false;
    for _ in 0..3_000 {
        harness.step();
        if mentions(&harness.kittest_state().root(), "Allow shell") {
            found = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let store_asks = harness.state().asks;
    if !found {
        let root = harness.kittest_state().root();
        let mut texts = Vec::new();
        collect(&root, &mut texts);
        println!("store asks: {store_asks}");
        println!("tree texts: {texts:?}");
    }
    assert!(
        found,
        "the approval card must mount with the engine's offered options (store asks: {store_asks})"
    );
    let root = harness.kittest_state().root();
    assert!(mentions(&root, "Reject shell"), "every option mounts");
}

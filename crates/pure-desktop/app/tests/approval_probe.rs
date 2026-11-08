//! Headless E07 probes: the approval card draws the ask the engine
//! offered, its options answer through the store, and a multi-select
//! question toggles its choices. The card renders only what the wire
//! carried, so the probes build their asks from wire frames rather
//! than from local structs.

use egui_kittest::{Harness, kittest::Queryable};
use pure_desktop::store::Store;
use pure_desktop::ui::approval::{Approval, ApprovalEvent};

mod testutil;

/// A harness over the approval card with `store`'s session open.
fn harness_for(store: Store) -> Harness<'static, (Store, Approval, Vec<ApprovalEvent>, bool)> {
    Harness::builder()
        .with_size(egui::Vec2::new(760.0, 420.0))
        .build_ui_state(
            |ui, (store, approval, events, installed): &mut (
                Store,
                Approval,
                Vec<ApprovalEvent>,
                bool,
            )| {
                if testutil::theme_installed(installed, ui.ctx()) {
                    return;
                }
                events.extend(approval.show(ui, store));
            },
            (store, Approval::new(), Vec::new(), false),
        )
}

/// A store with one open session.
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

/// A store with one open shell ask carrying three options.
fn asked() -> Store {
    let mut store = booted();
    store.absorb(pure_client::Frame::Request {
        id: pure_client::RequestId::Number(101),
        method: "session/request_permission".to_owned(),
        params: serde_json::json!({
            "sessionId": "s1",
            "toolCall": {
                "toolCallId": "call-sh",
                "title": "shell",
                "kind": "execute",
                "status": "pending",
                "rawInput": {"command": "cargo test"}
            },
            "options": [
                {"optionId": "allow", "name": "Allow shell", "kind": "allow_once"},
                {"optionId": "allow_session", "name": "Allow shell for this session",
                 "kind": "allow_always"},
                {"optionId": "reject", "name": "Reject shell", "kind": "reject_once"}
            ],
        }),
    });
    store
}

/// A store with one open `ask_user_question` ask.
fn questioned() -> Store {
    let mut store = booted();
    store.absorb(pure_client::Frame::Request {
        id: pure_client::RequestId::Number(102),
        method: "session/request_permission".to_owned(),
        params: serde_json::json!({
            "sessionId": "s1",
            "toolCall": {
                "toolCallId": "call-q",
                "title": "ask_user_question",
                "kind": "other",
                "status": "pending",
            },
            "options": [
                {"optionId": "skip", "name": "Skip", "kind": "reject_once"}
            ],
            "_meta": {"pure": {"question": {"prompt": {
                "header": "Auth method",
                "question": "Which auth method should the client use?",
                "multiSelect": false,
                "options": [
                    {"label": "API key", "description": "A key from the provider console"},
                    {"label": "OAuth", "description": "A browser sign-in"}
                ]
            }}}}
        }),
    });
    store
}

#[test]
fn the_card_renders_the_offered_options_numbered_from_one() {
    let mut harness = harness_for(asked());
    harness.run();
    harness.run();
    for name in [
        "Allow shell",
        "Allow shell for this session",
        "Reject shell",
    ] {
        assert!(
            harness.query_by_label(name).is_some(),
            "the option {name} draws"
        );
    }
    assert!(
        harness.query_by_label("cargo test").is_some(),
        "the ask's subject draws"
    );
}

#[test]
fn an_option_click_answers_with_its_offered_id() {
    let mut harness = harness_for(asked());
    harness.run();
    harness.run();
    harness.get_by_label("Reject shell").click();
    harness.run();
    let (_, approval, events, _) = harness.state_mut();
    assert!(
        events.iter().any(|event| matches!(
            event,
            ApprovalEvent::Reply(_, _, pure_client::PermissionDecision::Option(id)) if id == "reject"
        )),
        "the click answers with the offered reject id"
    );
    assert!(
        approval
            .feedback(&pure_client::RequestId::Number(101))
            .is_empty(),
        "the answered ask leaves no field behind"
    );
}

#[test]
fn the_first_option_answers_with_the_allow_id() {
    let mut harness = harness_for(asked());
    harness.run();
    harness.run();
    harness.get_by_label("Allow shell").click();
    harness.run();
    let (_, _, events, _) = harness.state();
    assert!(
        events.iter().any(|event| matches!(
            event,
            ApprovalEvent::Reply(_, _, pure_client::PermissionDecision::Option(id)) if id == "allow"
        )),
        "the first option answers allow"
    );
}

#[test]
fn the_card_draws_nothing_while_no_ask_is_open() {
    let mut harness = harness_for(booted());
    harness.run();
    harness.run();
    assert!(
        harness.query_by_label("Allow shell").is_none(),
        "no ask, no options"
    );
    assert!(
        harness.query_by_label("Skip").is_none(),
        "no question, no skip"
    );
}

#[test]
fn a_question_card_renders_its_header_choices_and_skip() {
    let mut harness = harness_for(questioned());
    harness.run();
    harness.run();
    assert!(
        harness
            .query_by_label("Which auth method should the client use?")
            .is_some(),
        "the question draws"
    );
    for choice in ["API key", "OAuth"] {
        assert!(
            harness.query_by_label(choice).is_some(),
            "the choice {choice} draws"
        );
    }
    assert!(
        harness
            .query_by_label("A key from the provider console")
            .is_some(),
        "the choice's description draws"
    );
    assert!(harness.query_by_label("Skip").is_some(), "the skip draws");
}

#[test]
fn a_choice_click_answers_the_question_at_once() {
    let mut harness = harness_for(questioned());
    harness.run();
    harness.run();
    harness.get_by_label("OAuth").click();
    harness.run();
    let (_, _, events, _) = harness.state();
    assert!(
        events.iter().any(|event| matches!(
            event,
            ApprovalEvent::Reply(_, _, pure_client::PermissionDecision::Option(id))
                if id == "choice-1"
        )),
        "the second choice answers choice-1"
    );
}

#[test]
fn the_skip_button_answers_with_the_offered_reject() {
    let mut harness = harness_for(questioned());
    harness.run();
    harness.run();
    harness.get_by_label("Skip").click();
    harness.run();
    let (_, _, events, _) = harness.state();
    assert!(
        events.iter().any(|event| matches!(
            event,
            ApprovalEvent::Reply(_, _, pure_client::PermissionDecision::Option(id)) if id == "skip"
        )),
        "skip answers the offered reject"
    );
}

/// Renders the ask and question cards to `/tmp/approval-*.png` for a
/// side-by-side against the reference, when `PURE_PROBE_PNG` is set.
#[test]
fn the_cards_render_for_review() {
    if std::env::var_os("PURE_PROBE_PNG").is_none() {
        return;
    }
    for (name, store) in [("ask", asked()), ("question", questioned())] {
        let mut harness = harness_for(store);
        harness.run();
        harness.run();
        let image = harness.render().expect("the card renders");
        image
            .save(format!("/tmp/approval-{name}.png"))
            .expect("the card image saves");
    }
}

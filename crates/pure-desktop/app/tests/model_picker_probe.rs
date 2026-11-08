//! Headless probes: the model picker lays its rows out without overlap
//! and answers clicks, over a real store whose engine catalog overflows
//! the picker's height.

use egui_kittest::Harness;
use pure_client::wire::ModelEntry;
use pure_client::{Frame, RequestId};
use pure_desktop::store::Store;
use pure_desktop::transport::State;
use pure_desktop::ui::toolbar::{self, Toolbar};

mod testutil;

/// A store whose link is up, whose agent defaults carry the model and
/// thinking options, and whose catalog holds three providers with
/// enough models to overflow the picker.
fn picker_store() -> Store {
    let mut store = Store::new("/w", false);
    store.set_connect(State::Connected);
    store.handshake(false);
    let outgoing = store.take_outgoing();
    let init_id = request_id(&outgoing, "initialize").expect("the handshake asks initialize");
    store.absorb(Frame::Success {
        id: RequestId::Number(init_id),
        result: serde_json::json!({
            "protocolVersion": 1,
            "agentCapabilities": { "steer": true },
            "agentInfo": { "name": "pure", "version": "0.3.0" },
            "_meta": { "pure": { "configOptions": [
                {
                    "id": "model", "name": "Model", "type": "select",
                    "currentValue": "oc-free/big-pickle",
                    "options": [
                        { "value": "oc-free/big-pickle", "name": "Big Pickle" },
                        { "value": "za/zai-glm-5.3-flash", "name": "GLM 5.3 Flash" }
                    ]
                },
                {
                    "id": "thinking", "name": "Thinking", "type": "select",
                    "currentValue": "default",
                    "options": [
                        { "value": "default", "name": "auto" },
                        { "value": "xhigh", "name": "max" }
                    ]
                }
            ] } }
        }),
    });
    let _ = store.take_outgoing();
    store.ask_models();
    let outgoing = store.take_outgoing();
    let models_id =
        request_id(&outgoing, "_pure/models/list").expect("the store asks for the catalog");
    store.absorb(Frame::Success {
        id: RequestId::Number(models_id),
        result: serde_json::json!({
            "providers": [
                { "id": "oc-free", "name": "OC-FREE", "models": catalog("oc-free", 10) },
                { "id": "stepfun", "name": "STEP-FUN", "models": catalog("stepfun", 1) },
                { "id": "pg.z.ai", "name": "PG.Z.AI", "models": catalog("pgz", 2) },
                { "id": "zai-proxy", "name": "ZAI-PROXY", "models": catalog("zai", 1) }
            ]
        }),
    });
    let _ = store.take_outgoing();
    store
}

/// The number id of the `method` request in `outgoing`, if it is there.
fn request_id(outgoing: &[Frame], method: &str) -> Option<u64> {
    outgoing.iter().find_map(|frame| {
        let Frame::Request {
            id, method: sent, ..
        } = frame
        else {
            return None;
        };
        if sent != method {
            return None;
        }
        match id {
            RequestId::Number(n) => Some(*n),
            _ => None,
        }
    })
}

/// `n` catalog entries with real names, contexts and image flags, the
/// shape the engine answers with.
fn catalog(provider: &str, n: usize) -> Vec<ModelEntry> {
    (0..n)
        .map(|ix| ModelEntry {
            id: format!("{provider}/model-{ix}"),
            name: format!("Model {provider} {ix}"),
            context: Some(262_144),
            input_cost: Some(0.2),
            output_cost: Some(1.1),
            thinking: Vec::new(),
            images: ix % 2 == 0,
            released: None,
        })
        .collect()
}

type Pane = (Store, Toolbar, bool, bool);

fn harness(store: Store) -> Harness<'static, Pane> {
    Harness::builder()
        .with_size(egui::Vec2::new(1900.0, 1100.0))
        .build_ui_state(
            |ui, (store, toolbar, opened, installed): &mut Pane| {
                if testutil::theme_installed(installed, ui.ctx()) {
                    return;
                }
                if !*opened {
                    toolbar.toggle_picker(pure_desktop::ui::toolbar::Picker::Model);
                    *opened = true;
                }
                egui::CentralPanel::default().show(ui, |ui| {
                    // The strip the toolbar sits in: the picker window
                    // anchors at the pill and opens rightward, so the
                    // strip stops well short of the canvas edge.
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(1000.0, 40.0), egui::Sense::hover());
                    let mut strip = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(rect)
                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    );
                    toolbar::show(&mut strip, toolbar, store, false, false);
                });
            },
            (store, Toolbar::new(), false, false),
        )
}

#[test]
fn the_model_picker_rows_never_overlap() {
    let store = picker_store();
    let mut harness = harness(store);
    // Frame one binds fonts; frame two draws the toolbar; the click
    // opens the picker; the next frames lay the window out.
    harness.run();
    harness.run();
    let img = harness.render().expect("the picker renders");

    // The surface's shadow is black at 60 percent. A regression once
    // turned `0x00000099` into the opaque blue `#000099`, which bled a
    // violet haze through the card and around its edges: exactly the
    // "lighter purple" the reference never shows. Near-black blue
    // pixels must not exist.
    let bluish = img
        .pixels()
        .filter(|c| c.0[0] < 12 && c.0[1] < 12 && c.0[2] > 100)
        .count();
    assert_eq!(bluish, 0, "the card's shadow must be black, not blue");

    // Every text run in the picker's list must sit in its own band: no
    // two runs of different rows may share rows.
    let root = harness.kittest_state().root();
    let mut runs: Vec<(String, egui::Rect)> = Vec::new();
    walk(&root, &mut runs);
    assert!(
        runs.len() > 20,
        "the picker drew only {} text runs",
        runs.len()
    );
    let mut overlaps: Vec<String> = Vec::new();
    // The list clips past its visible bottom edge, so a row cut off there
    // is in the tree but never painted; only what shows counts.
    let runs: Vec<(String, egui::Rect)> = runs
        .into_iter()
        .filter(|(text, r)| {
            let list_row = text.starts_with("Model ")
                || text.starts_with("model-")
                || text.starts_with('$')
                || text == "262k"
                || text == "OC";
            !(list_row && r.top() >= 470.0)
        })
        .collect();
    let mut sorted = runs.clone();
    sorted.sort_by_key(|(_, r)| r.top().to_bits());
    for (text, r) in &sorted {
        println!(
            "run {:?} y [{:.1},{:.1}] x [{:.1},{:.1}]",
            text,
            r.top(),
            r.bottom(),
            r.left(),
            r.right()
        );
    }
    for (i, (a, ra)) in runs.iter().enumerate() {
        for (b, rb) in runs.iter().skip(i + 1) {
            let dy = (ra.center().y - rb.center().y).abs();
            if a == b || dy >= 4.0 || !ra.intersects(*rb) {
                continue;
            }
            let share = ra.intersect(*rb).height() / ra.height().min(rb.height());
            if share > 0.5 {
                overlaps.push(format!("{a:?} over {b:?} at y {}", ra.center().y));
            }
        }
    }
    assert!(overlaps.is_empty(), "picker rows overlap: {overlaps:#?}");
}

/// Collects every text run's rect, walking the tree depth first.
fn walk(node: &egui_kittest::kittest::AccessKitNode<'_>, runs: &mut Vec<(String, egui::Rect)>) {
    let texty = node.role() == egui::accesskit::Role::TextRun
        || node.role() == egui::accesskit::Role::Label;
    if texty && let Some(boxed) = node.bounding_box() {
        // A label that carries text runs reports its alignment box,
        // wider than its glyphs; the runs themselves are the honest
        // rects, so the label box is skipped when they exist.
        let has_runs = node
            .children()
            .any(|child| child.role() == egui::accesskit::Role::TextRun);
        let tight = node.role() == egui::accesskit::Role::TextRun || !has_runs;
        if tight {
            let rect = egui::Rect::from_x_y_ranges(
                (boxed.x0 as f32)..=(boxed.x1 as f32),
                (boxed.y0 as f32)..=(boxed.y1 as f32),
            );
            if rect.width() > 1.0 {
                let text = node.value().unwrap_or_default();
                runs.push((text, rect));
            }
        }
    }
    for child in node.children() {
        walk(&child, runs);
    }
}

#[test]
fn a_click_outside_the_model_picker_dismisses_it() {
    let store = picker_store();
    let mut harness = harness(store);
    harness.run();
    harness.run();
    assert_eq!(
        harness.state().1.picker(),
        Some(pure_desktop::ui::toolbar::Picker::Model),
        "the picker is open before the click"
    );
    let outside = egui::pos2(60.0, 900.0);
    harness.hover_at(outside);
    harness.drag_at(outside);
    harness.drop_at(outside);
    harness.run();
    harness.run();
    assert_eq!(
        harness.state().1.picker(),
        None,
        "a click outside the card closes it"
    );
}

#[test]
fn the_mode_picker_toggles_open_from_its_pill_state() {
    let store = picker_store();
    let mut harness = harness(store);
    harness
        .state_mut()
        .1
        .toggle_picker(pure_desktop::ui::toolbar::Picker::Mode);
    harness.run();
    harness.run();
    assert_eq!(
        harness.state().1.picker(),
        Some(pure_desktop::ui::toolbar::Picker::Mode),
        "the mode picker is open"
    );
}

//! A frame of the whole shell with the composer mounted, so the
//! composer's placement in the chat column is verified against the
//! layout the window draws.

use egui_kittest::Harness;
use pure_desktop::store::Store;

mod testutil;

#[test]
fn the_composer_sits_under_the_transcript_column() {
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
    let composer = pure_desktop::ui::composer::Composer::new();
    let mut harness = Harness::builder()
        .with_size(egui::Vec2::new(1100.0, 760.0))
        .build_ui_state(
            |ui,
             (store, composer, installed): &mut (
                Store,
                pure_desktop::ui::composer::Composer,
                bool,
            )| {
                if testutil::theme_installed(installed, ui.ctx()) {
                    return;
                }
                // The shell's three panels, the chat column carrying
                // the transcript above and the composer below, the way
                // launch.rs mounts them.
                let sidebar = |ui: &mut egui::Ui| {
                    ui.label("pure");
                };
                let workbench = |ui: &mut egui::Ui| {
                    ui.label("workbench");
                };
                let chat = |ui: &mut egui::Ui| {
                    ui.vertical(|ui| {
                        // The same split launch.rs draws: the
                        // transcript takes the column's remainder,
                        // the dock and the composer sit under it.
                        let composer_h = 132.0;
                        let dock_h = 72.0;
                        let top_h = (ui.available_height() - composer_h - dock_h).max(0.0);
                        let (top_rect, _) = ui.allocate_at_least(
                            egui::vec2(ui.available_width(), top_h),
                            egui::Sense::hover(),
                        );
                        let mut top = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(top_rect)
                                .layout(egui::Layout::top_down(egui::Align::Min)),
                        );
                        top.label("the transcript column");
                        let _ = pure_desktop::ui::dock::show(ui, store);
                        let commands = store.composer_commands().to_vec();
                        composer.show(ui, store, &commands, false, true);
                    });
                };
                pure_desktop::ui::shell::show(
                    ui,
                    &mut pure_desktop::ui::shell::Panels::default(),
                    sidebar,
                    chat,
                    workbench,
                );
            },
            (store, composer, false),
        );
    harness.run();
    harness.run();
    let img = harness.render().expect("the frame renders");
    // The composer's own surface paints a band of its palette token
    // near the bottom of the chat column, under the transcript.
    let (w, h) = (img.width() as usize, img.height() as usize);
    let mut composer_rows = Vec::new();
    for y in 0..h {
        let hits = (0..w)
            .step_by(4)
            .filter(|x| {
                let p = img.get_pixel(*x as u32, y as u32).0;
                p[0] == 0x15 && p[1] == 0x13 && p[2] == 0x1B
            })
            .count();
        if hits > 20 {
            composer_rows.push(y);
        }
    }
    assert!(
        !composer_rows.is_empty(),
        "the composer's surface paints its own token"
    );
    let first = composer_rows[0];
    assert!(
        first > h / 2,
        "the composer sits in the lower half of the column: row {first} of {h}"
    );
}

//! Headless probe for the interface language: with Chinese selected the
//! settings nav, page headings and rows draw their Chinese text.

use egui_kittest::{Harness, kittest::Queryable};
use pure_desktop::i18n::{self, Language};
use pure_desktop::store::Store;
use pure_desktop::ui::settings::{Section, SettingsEvent, SettingsView};

mod testutil;

fn config_answer() -> serde_json::Value {
    serde_json::json!({
        "providers": {"custom": {}},
        "mcp": {"servers": {}},
        "permissions": {"mode": "default", "confine_paths": false, "tools": {}, "mcp": {}},
        "plugins": {"dir": "/plug", "enabled": [], "disabled": [], "capabilities": {}, "config": {}},
        "installedPlugins": []
    })
}

type Probe = (
    Store,
    SettingsView,
    Vec<SettingsEvent>,
    Vec<pure_client::RequestId>,
    bool,
);

fn harness_for(store: Store) -> Harness<'static, Probe> {
    Harness::builder()
        .with_size(egui::Vec2::new(1000.0, 1000.0))
        .build_ui_state(
            |ui, (store, settings, events, answered, installed): &mut Probe| {
                if testutil::theme_installed(installed, ui.ctx()) {
                    return;
                }
                events.extend(settings.show(
                    ui,
                    store,
                    &pure_desktop::ui::settings::UpdateState::default(),
                ));
                if settings.is_open() && settings.section().reads_config() {
                    store.ask_config();
                    let outgoing = store.take_outgoing();
                    let mut answers: Vec<(pure_client::RequestId, serde_json::Value)> = Vec::new();
                    for frame in &outgoing {
                        if let pure_client::Frame::Request { id, method, .. } = frame
                            && method == "_pure/config/get"
                            && !answered.contains(id)
                        {
                            answered.push(id.clone());
                            answers.push((id.clone(), config_answer()));
                        }
                    }
                    for (id, result) in answers {
                        store.absorb(pure_client::Frame::Success { id, result });
                    }
                }
            },
            (store, SettingsView::new(), Vec::new(), Vec::new(), false),
        )
}

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

#[test]
fn the_settings_draw_in_chinese() {
    i18n::set_language(Language::Chinese);
    let mut harness = harness_for(booted());
    harness.state_mut().1.open(Section::General);
    harness.run();
    harness.run();
    // The nav column draws every page name in Chinese.
    for label in ["通用", "代理与会话", "模型提供商", "MCP 服务器", "权限", "插件", "关于"] {
        assert!(
            testutil::count(&harness, label) >= 1,
            "the {label} nav label draws in Chinese"
        );
    }
    // The General page's toggles read Chinese too, and the language
    // picker the page used to carry is gone.
    for label in ["回车发送", "平滑流式输出", "按项目分组会话"] {
        assert!(
            testutil::count(&harness, label) >= 1,
            "the {label} control draws in Chinese"
        );
    }
    assert_eq!(
        testutil::count(&harness, "语言"),
        0,
        "the language picker no longer draws"
    );
    i18n::set_language(Language::System);
}

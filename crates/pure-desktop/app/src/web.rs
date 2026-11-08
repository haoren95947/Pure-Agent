//! The browser entry: boots the shell once `web/boot.js` has drawn
//! its connection form.
//!
//! `boot.js` owns the page: it renders the server URL and token form,
//! reads the `?ws=` developer override, and only then loads this
//! bundle. It hands both values over through `window.__pureConnect`,
//! so the token never rides a URL. This module reads that handoff,
//! dials the WebSocket subprotocol with it, and starts the same shell
//! the native binary runs.
//!
//! Everything target-specific is behind a cfg in [`crate::launch`]:
//! the browser has no local engine, so the setup screen never shows
//! and the release check never runs.

use wasm_bindgen::JsCast as _;

use crate::launch;

/// What `boot.js` hands over once the connection form is submitted.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Handshake {
    /// The `ws://` endpoint of a `pure serve`.
    server: String,
    /// The token the endpoint expects, carried as a subprotocol.
    token: String,
}

/// Reads `window.__pureConnect`, or `None` while the form still shows.
///
/// The handoff is a plain object literal from `boot.js`, so it is read
/// by key through `Reflect` rather than bound to a struct: a missing
/// or non-string key refuses the dial instead of panicking in the
/// browser.
fn handshake() -> Option<Handshake> {
    let value: wasm_bindgen::JsValue = web_sys::window()?
        .get("__pureConnect")
        .map(wasm_bindgen::JsValue::from)?;
    if value.is_null() || value.is_undefined() {
        return None;
    }
    let field = |name: &str| {
        let key: wasm_bindgen::JsValue = name.into();
        js_sys::Reflect::get(&value, &key)
            .ok()
            .filter(|found| !found.is_null() && !found.is_undefined())
            .and_then(|found| found.as_string())
    };
    Some(Handshake {
        server: field("server")?,
        token: field("token")?,
    })
}

/// Starts the shell on `canvas`. The future resolves once the canvas
/// carries the app.
pub async fn start(canvas: web_sys::HtmlCanvasElement) {
    let runner = eframe::WebRunner::new();
    let result = runner
        .start(
            canvas,
            eframe::WebOptions::default(),
            Box::new(move |cc| {
                let prefs = crate::prefs::load();
                crate::theme::install_fonts(&cc.egui_ctx);
                crate::icons::install(&cc.egui_ctx);
                crate::theme::apply_choice(&prefs.theme, &cc.egui_ctx);
                // Until the form is submitted there is no endpoint, so
                // the shell gets the recording: the canvas stays alive
                // and empty, and boot.js keeps owning the page. Once
                // the handoff lands the browser transport dials it.
                let (transport, replay) = match handshake() {
                    Some(handshake) => (
                        Box::new(crate::transport::web::WebTransport::new(
                            handshake.server,
                            handshake.token,
                        )) as Box<dyn crate::transport::Transport>,
                        false,
                    ),
                    None => (
                        Box::new(crate::transport::replay::ReplayTransport::new())
                            as Box<dyn crate::transport::Transport>,
                        true,
                    ),
                };
                Ok(Box::new(
                    launch::ShellApp::new(String::new(), replay, transport, &cc.egui_ctx)
                        .with_prefs(prefs),
                ))
            }),
        )
        .await;
    if let Err(why) = result {
        crate::warn(&format!("the browser runner did not start: {why:?}"));
    }
}

/// The wasm entry the generated glue calls: finds the page canvas by
/// id and starts the shell on it.
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn boot() {
    let Some(canvas) = canvas() else {
        crate::warn("the page has no canvas and none could be created");
        return;
    };
    wasm_bindgen_futures::spawn_local(async move {
        start(canvas).await;
    });
}

/// The canvas the shell draws on: the one `boot.js` left in the page,
/// or one this module creates. The form script clears the body before
/// it loads the bundle, so the bundle owns the canvas it needs. It
/// fills the window, and the runner sizes its buffer to the element.
fn canvas() -> Option<web_sys::HtmlCanvasElement> {
    let document = web_sys::window()?.document()?;
    if let Some(found) = document.get_element_by_id("pure-canvas") {
        return found.dyn_into().ok();
    }
    let element = document.create_element("canvas").ok()?;
    element.set_id("pure-canvas");
    let canvas: web_sys::HtmlCanvasElement = element.dyn_into().ok()?;
    let style = canvas.style();
    let _ = style.set_property("position", "absolute");
    let _ = style.set_property("inset", "0");
    let _ = style.set_property("width", "100%");
    let _ = style.set_property("height", "100%");
    let body = document.body()?;
    let _ = body.append_child(&canvas);
    Some(canvas)
}

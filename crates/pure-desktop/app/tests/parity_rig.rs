//! The visual parity rig: kittest snapshot baselines of the REAL shell
//! at the SPIKE viewport, plus the dominant-color histograms against the
//! palette.
//!
//! The snapshots are the reference the later view epics re-baseline
//! against; the histograms are what a human eyeball used to be for
//! "does the shell still draw the palette". Both themes are checked,
//! because a light palette drift is invisible in the dark one.
//!
//! The rig drives the shipped [`ShellApp`] through `eframe::App::ui`
//! over the replay transport, so a baseline is the real window: the
//! gate banner, the sidebar, the transcript, the composer, the status
//! bar and whichever panel is open. An earlier rig handed
//! `ui::shell::show` three placeholder labels, which meant a baseline
//! could stay green while the app was broken.
//!
//! Two harness facts shape the code below. egui binds a font on the
//! frame after it is installed, so the first frame installs the theme
//! and draws nothing. And the pump thread behind the replay transport
//! asks for a repaint on every event, so the harness is stepped rather
//! than run: `run` stops after four steps and fails, which a window
//! that repaints forever can never satisfy.

use std::collections::HashMap;

use egui_kittest::Harness;

mod testutil;

/// The SPIKE's viewport, the parity rig's reference size.
const SPIKE_VIEWPORT: [f32; 2] = [1252.0, 1317.0];

/// The state a parity harness carries between frames.
struct Rig {
    /// The real window body, built on the harness's own context the
    /// first frame it is drawn, so the fonts it installs are the ones
    /// the harness renders with.
    app: Option<pure_desktop::launch::ShellApp>,
    /// The theme to render.
    theme: String,
    /// Set once the theme has been installed on a frame that did not
    /// draw, which is the frame before the app may lay out.
    installed: bool,
}

/// A harness over the real shell at the SPIKE viewport, in `theme`.
fn harness_for(theme: &str) -> Harness<'static, Rig> {
    Harness::builder()
        .with_size(egui::Vec2::new(SPIKE_VIEWPORT[0], SPIKE_VIEWPORT[1]))
        .build_ui_state(
            |ui, rig: &mut Rig| {
                if !rig.installed {
                    // Frame one binds the fonts and the theme; drawing
                    // here would lay out against the fonts from before.
                    testutil::install_theme(ui.ctx(), &rig.theme);
                    rig.installed = true;
                    return;
                }
                let app = rig.app.get_or_insert_with(|| {
                    // A fixed folder, so the welcome chip names the
                    // same folder on every platform.
                    pure_desktop::launch::ShellApp::new(
                        "/tmp".to_owned(),
                        true,
                        Box::new(pure_desktop::transport::replay::ReplayTransport::new()),
                        ui.ctx(),
                    )
                });
                eframe::App::ui(app, ui, &mut eframe::Frame::_new_kittest());
            },
            Rig {
                app: None,
                theme: theme.to_owned(),
                installed: false,
            },
        )
}

/// The step budget, sized to cover the slowest thing the settle waits
/// on. The transcript paces its reveal over real seconds (a burst is
/// spread over at most 2.5 s), and each step advances it by the time
/// that step actually took, so a budget counted in instant steps would
/// run out before the reveal caught up and record a half-typed frame.
const STEP_BUDGET: usize = 2_000;

/// The pause each step takes, so the settle spans real seconds and the
/// reveal animation has room to finish rather than racing it.
const STEP_PAUSE: std::time::Duration = std::time::Duration::from_millis(5);

/// How many consecutive identical renders count as settled.
///
/// The reveal paces at its slowest rate of 60 characters a second, so a
/// whole character takes about 17 ms to appear and the rendered text
/// only changes once every few steps. Two renders a few steps apart can
/// therefore be identical while the reveal is still typing: the frame
/// pauses between characters. Waiting only for the next identical pair
/// reads that pause as the end and records a half-typed transcript. A
/// window of 40 steps at 5 ms each is 200 ms, well past the slowest
/// character interval, so only a genuinely finished reveal stays still
/// that long.
const STABLE_STEPS: usize = 40;

/// Steps a parity harness until the recording has finished playing and
/// the frame has held still long enough to be the reveal's end, which
/// is the fixed point the snapshot wants.
///
/// The replay transport plays its recording on a background thread with
/// a 100 ms beat per frame, and the transcript paces its reveal over
/// real seconds on top of that, so the rig has to step against a clock
/// it does not control. Two conditions have to hold together:
///
/// - The recording has finished. The last frame is the `end_turn`
///   success that sets the session's `last_stop`, so that field is the
///   "every frame absorbed" marker. Waiting only on stable pixels is not
///   enough: the pump's 100 ms beat means two of the rig's steps often
///   land inside one lull and render identically while the recording is
///   still half played, which is exactly how the rig flaked between the
///   welcome column, the filled transcript and the frame after it.
/// - The frame has held still for a whole character interval. Once the
///   store stops changing, the reveal keeps typing the assistant text
///   out for up to 2.5 s and the gate banner appears, so the frame
///   reaches its resting image only after the reveal is done.
///
/// The step budget is a backstop, so a rig that never settles fails
/// here and the recorded frame carries whatever it reached, which the
/// histogram tests reject.
fn settled(harness: &mut Harness<'static, Rig>) {
    let mut played_out = false;
    let mut previous: Option<Vec<u8>> = None;
    let mut stable = 0;
    let mut stepped = 0;
    while stepped < STEP_BUDGET {
        harness.step();
        std::thread::sleep(STEP_PAUSE);
        stepped += 1;
        if !played_out {
            played_out = harness.state().app.as_ref().is_some_and(|app| {
                let store = app.shell.store();
                store
                    .active_id()
                    .and_then(|id| store.state().session(id))
                    .is_some_and(|session| session.last_stop.is_some())
            });
            // Keep stepping while the recording still has frames to
            // deliver; the pixel check below would pass on the pump's
            // next lull otherwise.
            if !played_out {
                previous = None;
                stable = 0;
                continue;
            }
        }
        let Ok(frame) = harness.render() else {
            continue;
        };
        let bytes = frame.as_raw().as_slice().to_vec();
        if previous.as_deref() == Some(bytes.as_slice()) {
            stable += 1;
            if stable >= STABLE_STEPS {
                return;
            }
        } else {
            stable = 0;
        }
        previous = Some(bytes);
    }
}

/// The painted color histogram.
fn histogram(img: &image::RgbaImage) -> HashMap<egui::Color32, usize> {
    let mut counts: HashMap<egui::Color32, usize> = HashMap::new();
    for px in img.pixels() {
        if px.0[3] == 0 {
            continue;
        }
        let c = egui::Color32::from_rgba_unmultiplied(px.0[0], px.0[1], px.0[2], px.0[3]);
        *counts.entry(c).or_default() += 1;
    }
    counts
}

/// The painted pixel count under `colors`, as a share of the painted
/// frame. The shell paints its surfaces with the palette's own tokens,
/// so a drift away from them shows here.
fn palette_share(img: &image::RgbaImage, colors: &[egui::Color32]) -> f32 {
    let counts = histogram(img);
    let painted: usize = counts.values().sum();
    let hits: usize = colors
        .iter()
        .map(|c| counts.get(c).copied().unwrap_or(0))
        .sum();
    hits as f32 / painted as f32
}

/// The surfaces the shadow palette paints the shell with, including
/// the composer card the welcome stack hosts.
const SHADOW_SURFACES: [egui::Color32; 4] = [
    egui::Color32::from_rgb(0x0F, 0x0E, 0x13),
    egui::Color32::from_rgb(0x0B, 0x0A, 0x0E),
    egui::Color32::from_rgb(0x17, 0x15, 0x1D),
    egui::Color32::from_rgb(0x15, 0x13, 0x1B),
];

/// The surfaces the dawn palette paints the shell with, including
/// the composer card the welcome stack hosts.
const DAWN_SURFACES: [egui::Color32; 4] = [
    egui::Color32::from_rgb(0xF7, 0xF5, 0xF9),
    egui::Color32::from_rgb(0xEE, 0xEB, 0xF2),
    egui::Color32::from_rgb(0xEF, 0xEC, 0xF3),
    egui::Color32::from_rgb(0xFF, 0xFF, 0xFF),
];

/// The surfaces the OpenBitFun light palette paints the shell with.
const BITFUN_LIGHT_SURFACES: [egui::Color32; 3] = [
    egui::Color32::from_rgb(0xFF, 0xFF, 0xFF),
    egui::Color32::from_rgb(0xF8, 0xF8, 0xF9),
    egui::Color32::from_rgb(0xF7, 0xF7, 0xF7),
];

/// The surfaces the OpenBitFun dark palette paints the shell with.
const BITFUN_DARK_SURFACES: [egui::Color32; 2] = [
    egui::Color32::from_rgb(0x1C, 0x1C, 0x1F),
    egui::Color32::from_rgb(0x0E, 0x0E, 0x10),
];

/// The two shells both histograms require, in both themes: the sidebar
/// header's new-session button and the transcript's text runs. Neither
/// is present in a rig that renders placeholders.
fn assert_the_real_shell_drew(harness: &Harness<'_, Rig>) {
    use egui_kittest::kittest::Queryable;
    assert!(
        harness.query_all_by_label("New session").count() == 1,
        "the sidebar drew no new-session button: the rig is not rendering the app"
    );
    // A `Role::Label` node keeps its text in `value`, and the
    // transcript's rows are exactly that, so the runs are read by
    // role rather than by label.
    let runs = harness
        .query_all_by(|node| node.role() == egui::accesskit::Role::TextRun)
        .count();
    assert!(
        runs > 4,
        "the transcript drew {runs} text runs: the rig is not rendering the app"
    );
    // The two ways the shell lies about itself. A chat column that
    // reads the swapped-out store draws the welcome over a live
    // session (its project pill shows), and a row whose trailing pills
    // centre in the measure offer blows every height up so only the
    // tail row lands in the viewport. Both passed a TextRun count
    // once; neither survives these.
    let welcome = harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::Button
                && node.value().is_some_and(|v| v == "tmp")
        })
        .count();
    assert_eq!(
        welcome, 0,
        "the welcome column drew over an open session: the chat column is reading the wrong store"
    );
    assert!(
        harness.query_by_label("Updated todos").is_some(),
        "the transcript drew no todo row: the rows measure past the viewport"
    );
}

#[test]
fn the_shadow_shell_histograms_against_the_palette() {
    let mut harness = harness_for("pure-shadow");
    settled(&mut harness);
    let img = harness.render().expect("the frame renders");
    let share = palette_share(&img, &SHADOW_SURFACES);
    assert!(
        share > 0.9,
        "the palette surfaces hold {share} of the frame"
    );
    assert_the_real_shell_drew(&harness);
}

#[test]
fn the_dawn_shell_histograms_against_the_palette() {
    let mut harness = harness_for("pure-dawn");
    settled(&mut harness);
    let img = harness.render().expect("the frame renders");
    let share = palette_share(&img, &DAWN_SURFACES);
    assert!(
        share > 0.9,
        "the palette surfaces hold {share} of the frame"
    );
    assert_the_real_shell_drew(&harness);
}

/// The snapshot baselines. Re-baseline with
/// `UPDATE_SNAPSHOTS=1 cargo test --test parity_rig`.
#[test]
fn the_shadow_shell_snapshot_baseline() {
    let mut harness = harness_for("pure-shadow");
    settled(&mut harness);
    harness.snapshot("shadow-shell");
}

/// The snapshot baseline for the light palette.
#[test]
fn the_dawn_shell_snapshot_baseline() {
    let mut harness = harness_for("pure-dawn");
    settled(&mut harness);
    harness.snapshot("dawn-shell");
}

#[test]
fn the_bitfun_light_shell_histograms_against_the_palette() {
    let mut harness = harness_for("bitfun-light");
    settled(&mut harness);
    let img = harness.render().expect("the frame renders");
    let share = palette_share(&img, &BITFUN_LIGHT_SURFACES);
    assert!(
        share > 0.9,
        "the palette surfaces hold {share} of the frame"
    );
    assert_the_real_shell_drew(&harness);
}

#[test]
fn the_bitfun_dark_shell_histograms_against_the_palette() {
    let mut harness = harness_for("bitfun-dark");
    settled(&mut harness);
    let img = harness.render().expect("the frame renders");
    let share = palette_share(&img, &BITFUN_DARK_SURFACES);
    assert!(
        share > 0.9,
        "the palette surfaces hold {share} of the frame"
    );
    assert_the_real_shell_drew(&harness);
}

/// The snapshot baseline for the OpenBitFun light palette.
#[test]
fn the_bitfun_light_shell_snapshot_baseline() {
    let mut harness = harness_for("bitfun-light");
    settled(&mut harness);
    harness.snapshot("bitfun-light-shell");
}

/// The snapshot baseline for the OpenBitFun dark palette.
#[test]
fn the_bitfun_dark_shell_snapshot_baseline() {
    let mut harness = harness_for("bitfun-dark");
    settled(&mut harness);
    harness.snapshot("bitfun-dark-shell");
}

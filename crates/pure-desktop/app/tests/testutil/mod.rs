//! The harness the headless probes share: a themed egui context and
//! the one-frame install every probe needs before its widgets draw.
//!
//! The old desktop suite got this from `cx.add_window_view` plus
//! `VisualTestContext`, which installed the fonts and the theme for
//! the window and let a test drive widgets by label. The egui harness
//! replaces that with one helper: [`theme_installed`] installs the
//! fonts, the icons and the shadow theme on the first frame and
//! reports whether the caller should still draw.
//!
//! Every probe reaches this through `mod testutil;`, so the helpers
//! stay out of the shipped crate.

// Each probe uses some of these, not all: one a given probe never
// calls is dead in that binary only.
#![allow(dead_code)]

use egui_kittest::Harness;

/// The probe canvas size. The wide surface keeps a settings dialog or
/// the three-panel shell at its natural width, so a probe reads the
/// layout a user gets.
pub const CANVAS: egui::Vec2 = egui::Vec2::new(1000.0, 760.0);

/// The canvas a narrow widget (a dock, a card, a form) is drawn into.
pub const NARROW: egui::Vec2 = egui::Vec2::new(900.0, 700.0);

/// Installs the fonts, the icons and the shadow theme on the first
/// frame, and keeps provider logos from fetching. Returns `true` while
/// that install is still owed, so the caller returns from its draw
/// without laying anything out.
///
/// egui refuses a family it has no face for, so a probe that drew
/// before this ran panicked on the first `RichText::family`.
pub fn theme_installed(installed: &mut bool, ctx: &egui::Context) -> bool {
    if *installed {
        return false;
    }
    pure_desktop::logos::stay_offline();
    pure_desktop::theme::install_fonts(ctx);
    pure_desktop::icons::install(ctx);
    pure_desktop::theme::apply_choice(
        &pure_desktop::theme::ThemeChoice::Named("pure-shadow".to_owned()),
        ctx,
    );
    *installed = true;
    true
}

/// Installs the fonts, the icons and the named theme on this frame,
/// every time it is called, and keeps provider logos from fetching.
///
/// A measurement probe (row windows, font metrics, panel geometry)
/// lays out on every frame and needs the theme there, not once. The
/// once-per-probe contract is [`theme_installed`].
pub fn install_theme(ctx: &egui::Context, theme: &str) {
    pure_desktop::logos::stay_offline();
    pure_desktop::theme::install_fonts(ctx);
    pure_desktop::icons::install(ctx);
    pure_desktop::theme::apply_choice(
        &pure_desktop::theme::ThemeChoice::Named(theme.to_owned()),
        ctx,
    );
    ctx.global_style_mut(|style| style.visuals.text_cursor.blink = false);
}

/// Whether the replayed recording has played out: the active session
/// holds the stop its last frame carries. The recording runs its own
/// prompt on a wall clock, so a probe that types before this races a
/// turn that starts, runs and ends under it.
pub fn played_out(app: &pure_desktop::launch::ShellApp) -> bool {
    let store = app.shell.store();
    store
        .active_id()
        .and_then(|id| store.state().session(id))
        .is_some_and(|session| session.last_stop.is_some())
}

/// The state a [`harness`] carries: the caller's state, the draw
/// closure and the one-frame theme flag.
pub struct HarnessState<State, Draw> {
    /// The probe's own state.
    pub state: State,
    /// The probe's draw closure.
    pub draw: Draw,
    /// Whether the theme has been installed yet.
    pub installed: bool,
}

/// Builds a harness over `state` whose first frame installs the theme
/// and whose later frames run `draw`.
pub fn harness<State, Draw>(
    size: egui::Vec2,
    state: State,
    draw: Draw,
) -> Harness<'static, HarnessState<State, Draw>>
where
    Draw: FnMut(&mut egui::Ui, &mut State) + 'static,
{
    Harness::builder().with_size(size).build_ui_state(
        move |ui, state: &mut HarnessState<State, Draw>| {
            if theme_installed(&mut state.installed, ui.ctx()) {
                return;
            }
            (state.draw)(ui, &mut state.state);
        },
        HarnessState {
            state,
            draw,
            installed: false,
        },
    )
}

/// Runs two frames, so a probe that has just opened a dialog or a
/// page reads a settled tree.
pub fn settled<State, Draw>(harness: &mut Harness<'static, HarnessState<State, Draw>>) {
    harness.run();
    harness.run();
}

/// How many nodes carry `label`. An AccessKit `Role::Label` node keeps
/// its text in `value`, and kittest's label query already reads that,
/// so a `ui.label` is found by the words it draws.
pub fn count<State>(harness: &Harness<'_, State>, label: &str) -> usize {
    use egui_kittest::kittest::Queryable as _;
    harness.query_all_by_label(label).count()
}

/// Every text field's text, in the order the fields draw. A field's
/// text is its accesskit value, never its label, so a probe reads a
/// field through here rather than by label.
pub fn field_values<State>(harness: &Harness<'_, State>) -> Vec<String> {
    use egui_kittest::kittest::{NodeT as _, Queryable as _};
    harness
        .query_all_by(|node| node.role() == egui::accesskit::Role::TextInput)
        .map(|node| node.accesskit_node().value().unwrap_or_default())
        .collect()
}

/// Whether some text field holds `text`.
pub fn field_holds<State>(harness: &Harness<'_, State>, text: &str) -> bool {
    field_values(harness).iter().any(|value| value == text)
}

/// The first text field on screen, for a probe that types into it. A
/// `TextEdit` takes text only once it has keyboard focus, so the
/// caller clicks the node before it types.
pub fn first_field<'a, State>(harness: &'a Harness<'a, State>) -> Option<egui_kittest::Node<'a>> {
    use egui_kittest::kittest::Queryable as _;
    harness
        .query_all_by(|node| node.role() == egui::accesskit::Role::TextInput)
        .next()
}

#[cfg(test)]
mod tests {
    use super::{CANVAS, NARROW, theme_installed};

    #[test]
    fn the_theme_install_is_owed_once() {
        let ctx = egui::Context::default();
        let mut installed = false;
        assert!(
            theme_installed(&mut installed, &ctx),
            "the first frame owes the install"
        );
        assert!(installed);
        assert!(
            !theme_installed(&mut installed, &ctx),
            "the second frame owes nothing"
        );
    }

    #[test]
    fn both_canvases_fit_a_shell_panel() {
        // A canvas narrower than this cannot show the sidebar and the
        // chat column at once, so a probe would be reading a layout
        // no user gets.
        for (name, size) in [("narrow", NARROW), ("wide", CANVAS)] {
            assert!(size.x >= 640.0, "{name} is {size:?}");
            assert!(size.y >= 480.0, "{name} is {size:?}");
        }
        // The narrow canvas exists for widget probes that only need
        // room for one panel; the wide one is for the dialogs and the
        // three-panel shell. Clippy reads two `const`s as a constant
        // expression, so the widths go through a runtime vector.
        let mut widths: Vec<f32> = [NARROW, CANVAS].iter().map(|size| size.x).collect();
        widths.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(widths.len(), 2, "two canvases are configured");
        assert!(widths[0] < widths[1], "the widths differ: {widths:?}");
    }
}

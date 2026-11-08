//! The shell chrome in the real window: the sidebar's own controls,
//! the narrow-width overlays and the palette over them, each driven
//! through the shipped `ShellApp` over the replay transport.

use egui_kittest::{Harness, kittest::Queryable};

mod testutil;

/// The state a shell harness carries between frames.
struct Rig {
    /// The real window body, built on the harness's own context the
    /// first frame it is drawn.
    app: Option<pure_desktop::launch::ShellApp>,
    /// Set once the theme has been installed on a frame that did not
    /// draw.
    installed: bool,
}

/// A harness over the real shell at `size`.
fn harness(size: egui::Vec2) -> Harness<'static, Rig> {
    Harness::builder().with_size(size).build_ui_state(
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
                    Box::new(pure_desktop::transport::replay::ReplayTransport::new()),
                    ui.ctx(),
                )
            });
            eframe::App::ui(app, ui, &mut eframe::Frame::_new_kittest());
        },
        Rig {
            app: None,
            installed: false,
        },
    )
}

/// Steps until the composer's send button is on screen, then a few
/// frames more so the replayed session settles.
fn booted(harness: &mut Harness<'static, Rig>) {
    for _ in 0..4_000 {
        harness.step();
        if harness.query_by_label("Send").is_some()
            && harness
                .state()
                .app
                .as_ref()
                .is_some_and(testutil::played_out)
        {
            steps(harness, 5);
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("the composer never drew");
}

/// Steps `n` frames.
fn steps(harness: &mut Harness<'static, Rig>, n: usize) {
    for _ in 0..n {
        harness.step();
    }
}

/// The composer's field, a multiline input.
fn composer<'a>(harness: &'a Harness<'static, Rig>) -> egui_kittest::Node<'a> {
    harness
        .query_all_by(|node| node.role() == egui::accesskit::Role::MultilineTextInput)
        .next()
        .expect("the composer draws")
}

/// Whether some node carries `label`.
fn shows(harness: &Harness<'static, Rig>, label: &str) -> bool {
    harness.query_by_label(label).is_some()
}

#[test]
fn the_sidebars_collapse_button_hides_it() {
    let mut harness = harness(egui::vec2(1200.0, 800.0));
    booted(&mut harness);
    assert!(shows(&harness, "Search"), "the sidebar draws while wide");
    harness.get_by_label("Collapse sidebar (Ctrl \\)").click();
    steps(&mut harness, 3);
    assert!(!shows(&harness, "Search"), "the click hides the sidebar");
}

#[test]
fn a_narrow_window_offers_the_sidebar_and_its_rows_take_clicks() {
    let mut harness = harness(egui::vec2(800.0, 600.0));
    booted(&mut harness);
    assert!(
        !shows(&harness, "Search"),
        "a narrow window takes no column"
    );
    harness
        .query_by_label("Show sidebar (Ctrl \\)")
        .expect("a narrow window offers the sidebar")
        .click();
    steps(&mut harness, 3);
    harness
        .query_by_label("Search")
        .expect("the sidebar floats over the content")
        .click();
    steps(&mut harness, 3);
    assert!(
        shows(&harness, "Set goal"),
        "a click inside the floating sidebar reaches its row"
    );
}

#[test]
fn a_sidebar_collapsed_while_wide_still_floats_when_narrow() {
    let mut harness = harness(egui::vec2(1200.0, 800.0));
    booted(&mut harness);
    harness.get_by_label("Collapse sidebar (Ctrl \\)").click();
    steps(&mut harness, 3);
    harness.set_size(egui::vec2(800.0, 600.0));
    steps(&mut harness, 3);
    harness.get_by_label("Show sidebar (Ctrl \\)").click();
    steps(&mut harness, 3);
    assert!(
        shows(&harness, "Search"),
        "the sidebar floats over the content"
    );
}

#[test]
fn the_floating_workbench_spans_the_window_height() {
    let mut harness = harness(egui::vec2(800.0, 600.0));
    booted(&mut harness);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::B);
    steps(&mut harness, 5);
    let rect = harness
        .ctx
        .memory(|memory| memory.area_rect(egui::Id::new("workbench-float")))
        .expect("the workbench floats while narrow");
    assert!(
        rect.height() >= 600.0 - 1.0,
        "the workbench runs top to bottom: {rect:?}"
    );
}

#[test]
fn picking_a_row_in_the_floating_sidebar_closes_it() {
    let mut harness = harness(egui::vec2(800.0, 600.0));
    booted(&mut harness);
    harness.get_by_label("Show sidebar (Ctrl \\)").click();
    steps(&mut harness, 3);
    harness.get_by_label("New session").click();
    steps(&mut harness, 3);
    assert!(
        !shows(&harness, "Search"),
        "the sidebar steps aside once the followed session changes"
    );
}

#[test]
fn the_palette_scrolls_its_keyboard_highlight_into_view() {
    let mut harness = harness(egui::vec2(800.0, 600.0));
    booted(&mut harness);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::K);
    steps(&mut harness, 3);
    // Up from the first row wraps to the last, the session under the
    // fold of the commands.
    harness.key_press(egui::Key::ArrowUp);
    // The scroll animates, so the row settles a few frames on.
    steps(&mut harness, 30);
    let footer = harness.get_by_label("move").rect();
    let row = harness
        .query_all_by_label("Fix the null dereference in main")
        .map(|node| node.rect())
        .max_by(|a, b| a.top().total_cmp(&b.top()))
        .expect("the session row draws");
    assert!(
        row.bottom() <= footer.top(),
        "the highlighted row is above the footer: {row:?} vs {footer:?}"
    );
}

#[test]
fn the_palette_fits_a_small_window() {
    let mut harness = harness(egui::vec2(560.0, 480.0));
    booted(&mut harness);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::K);
    steps(&mut harness, 3);
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(560.0, 480.0));
    for label in ["New session", "move"] {
        let rect = harness
            .query_all_by_label(label)
            .map(|node| node.rect())
            .max_by(|a, b| a.left().total_cmp(&b.left()))
            .expect("the palette draws");
        assert!(screen.contains_rect(rect), "{label} fits: {rect:?}");
    }
}

#[test]
fn renaming_from_the_session_menu_opens_the_field_with_focus() {
    let mut harness = harness(egui::vec2(1200.0, 800.0));
    booted(&mut harness);
    harness.get_by_label("Session actions").click();
    steps(&mut harness, 3);
    // The field takes the focus the frame it first draws, so the
    // accessibility tree never names a focus it does not hold.
    harness.get_by_label("Rename").click();
    steps(&mut harness, 3);
    assert!(
        testutil::field_holds(&harness, "Fix the null dereference in main"),
        "the field opens on the title"
    );
    assert_eq!(
        harness.ctx.memory(|memory| memory.focused()),
        Some(egui::Id::new("topbar-rename-input")),
        "the field holds the focus"
    );
    harness.key_press(egui::Key::Escape);
    steps(&mut harness, 3);
    assert!(
        !testutil::field_holds(&harness, "Fix the null dereference in main"),
        "Esc closes the field"
    );
}

#[test]
fn a_session_menu_action_lands_on_the_live_store() {
    let mut harness = harness(egui::vec2(1200.0, 800.0));
    booted(&mut harness);
    harness.get_by_label("Session actions").click();
    steps(&mut harness, 3);
    harness.get_by_label("Pin").click();
    steps(&mut harness, 3);
    harness.get_by_label("Session actions").click();
    steps(&mut harness, 3);
    assert!(shows(&harness, "Unpin"), "the pin held");
}

#[test]
fn an_idle_window_repaints_to_drop_its_toast() {
    let mut harness = harness(egui::vec2(1200.0, 800.0));
    booted(&mut harness);
    steps(&mut harness, 600);
    harness.get_by_label("Session actions").click();
    steps(&mut harness, 3);
    harness.get_by_label("Archive").click();
    steps(&mut harness, 30);
    assert!(
        harness.query_by_label_contains("Archived").is_some(),
        "the archive toasts"
    );
    let delay = harness.output().viewport_output[&egui::ViewportId::ROOT].repaint_delay;
    assert!(
        delay <= pure_desktop::ui::toasts::TOAST_TTL,
        "the window wakes for the toast's expiry: {delay:?}"
    );
}

#[test]
fn enter_in_the_find_bar_keeps_its_query() {
    let mut harness = harness(egui::vec2(1200.0, 800.0));
    booted(&mut harness);
    steps(&mut harness, 400);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::F);
    steps(&mut harness, 3);
    harness.event(egui::Event::Text("null".to_owned()));
    steps(&mut harness, 3);
    assert!(testutil::field_holds(&harness, "null"), "the query lands");
    harness.key_press(egui::Key::Enter);
    steps(&mut harness, 3);
    assert!(
        testutil::field_holds(&harness, "null"),
        "Enter steps through the matches and keeps the query"
    );
}

#[test]
fn nothing_lays_out_below_the_window() {
    let mut harness = harness(egui::vec2(1200.0, 800.0));
    booted(&mut harness);
    steps(&mut harness, 50);
    use egui_kittest::kittest::NodeT as _;
    let below: Vec<String> = harness
        .query_all_by(|_| true)
        .filter(|node| {
            node.accesskit_node()
                .bounding_box()
                .is_some_and(|rect| rect.y0 >= 800.0)
        })
        .map(|node| {
            let node = node.accesskit_node();
            node.label().or_else(|| node.value()).unwrap_or_default()
        })
        .collect();
    assert!(below.is_empty(), "drawn off screen: {below:?}");
}

#[test]
fn enter_in_the_palette_runs_its_row_and_leaves_the_draft() {
    let mut harness = harness(egui::vec2(1200.0, 800.0));
    booted(&mut harness);
    steps(&mut harness, 400);
    composer(&harness).click();
    steps(&mut harness, 2);
    harness.event(egui::Event::Text("half a thought".to_owned()));
    steps(&mut harness, 2);
    assert_eq!(
        composer(&harness).value().as_deref(),
        Some("half a thought"),
        "the draft lands"
    );
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::K);
    steps(&mut harness, 3);
    harness.event(egui::Event::Text("workbench".to_owned()));
    steps(&mut harness, 2);
    harness.key_press(egui::Key::Enter);
    steps(&mut harness, 3);
    assert_eq!(
        composer(&harness).value().as_deref(),
        Some("half a thought"),
        "the draft stays"
    );
}

#[test]
fn the_docked_workbench_drags_down_to_its_minimum_on_every_tab() {
    let mut harness = harness(egui::vec2(1600.0, 900.0));
    booted(&mut harness);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::B);
    steps(&mut harness, 5);
    for tab in ["Changes", "Files", "Agents", "Terminal", "Browser"] {
        harness
            .query_all_by_label(tab)
            .next()
            .unwrap_or_else(|| panic!("the {tab} tab draws"))
            .click();
        steps(&mut harness, 3);
        let panel = |harness: &Harness<'static, Rig>| {
            egui::containers::panel::PanelState::load(&harness.ctx, egui::Id::new("workbench"))
                .expect("the workbench docks at this width")
                .outer_rect
        };
        let edge = panel(&harness).left_center();
        harness.hover_at(edge);
        steps(&mut harness, 1);
        harness.drag_at(edge);
        steps(&mut harness, 1);
        let far = edge + egui::vec2(600.0, 0.0);
        harness.hover_at(far);
        steps(&mut harness, 1);
        harness.drop_at(far);
        steps(&mut harness, 3);
        let width = panel(&harness).width();
        assert!(
            width < 250.0,
            "the {tab} pane lets the panel narrow, got {width}"
        );
        let close = harness.get_by_label("Close workbench").rect();
        assert!(
            close.right() <= panel(&harness).right(),
            "the close button stays inside the panel: {close:?}"
        );
        let last = harness.get_by_label("Browser").rect();
        assert!(
            last.right() <= close.left(),
            "every tab fits beside the close button: {last:?} {close:?}"
        );
        let edge = panel(&harness).left_center();
        harness.hover_at(edge);
        steps(&mut harness, 1);
        harness.drag_at(edge);
        steps(&mut harness, 1);
        harness.hover_at(edge - egui::vec2(260.0, 0.0));
        steps(&mut harness, 1);
        harness.drop_at(edge - egui::vec2(260.0, 0.0));
        steps(&mut harness, 3);
    }
}

//! The toast stack: the queue the shell feeds, with a TTL and an
//! optional click action.
//!
//! The queue is a plain state machine: [`Toasts::push`] raises a draft
//! stamped with the instant it was raised, [`Toasts::expire`] drops
//! what its TTL has run out on, and [`Toasts::show`] draws the stack
//! top-right with egui widgets.
//!
//! The store's [`Note`]s surface here: the shell drains
//! `store.take_notes()` once a frame and pushes each note through
//! [`Toasts::push_note`], which is the seam the old shell used. A
//! click comes back through [`Toasts::take_actions`] for the shell to
//! carry out, because the store stays on the shell's side of this
//! module's boundary.

use crate::icons::PaintSnapped as _;
use std::time::Duration;

use pure_client::wire::NoticeTone;
use web_time::Instant;

use crate::store::Note;
use crate::theme::{FS_SM, Palette, R_LG};

/// How long a toast stays before it expires on its own.
pub const TOAST_TTL: Duration = Duration::from_secs(4);

/// The most toasts the stack holds; the oldest give way.
pub const TOAST_MAX: usize = 5;

/// A toast card's narrowest width.
const TOAST_MIN_W: f32 = 260.0;
/// A toast card's widest width; longer text wraps.
const TOAST_MAX_W: f32 = 380.0;

/// What a click on a toast does beyond dismissing it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToastAction {
    /// Nothing: the click only dismisses.
    None,
    /// Follow the session the toast is about.
    ActivateSession(String),
    /// Bring an archived session back.
    Restore(String),
    /// Open the releases page in the browser; the update toast's
    /// way in.
    OpenUpdates,
}

/// One toast before it renders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToastDraft {
    /// Severity, as the notice tones carry it.
    pub tone: NoticeTone,
    /// The message text.
    pub text: String,
    /// What a click does beyond dismissing.
    pub action: ToastAction,
}

/// One raised toast: its draft and the instant it was raised, which
/// is what the TTL runs against.
#[derive(Debug, Clone)]
struct Raised {
    /// When the toast was raised.
    at: Instant,
    /// What it shows and what a click does.
    draft: ToastDraft,
}

/// The toast stack: a corner overlay fed by notice tones and local
/// actions. Toasts expire on their own; a click dismisses and runs
/// the toast's action.
#[derive(Debug, Default)]
pub struct Toasts {
    /// The raised toasts, oldest first.
    items: Vec<Raised>,
    /// The actions clicked since the last drain, oldest first.
    fired: Vec<ToastAction>,
}

impl Toasts {
    /// An empty stack.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Raises `draft` at `now`. The stack holds at most [`TOAST_MAX`]
    /// toasts; the oldest give way.
    pub fn push(&mut self, draft: ToastDraft, now: Instant) {
        self.items.push(Raised { at: now, draft });
        if self.items.len() > TOAST_MAX {
            self.items.remove(0);
        }
    }

    /// Raises a store note at `now`. A note that carries an archived
    /// session offers the click that restores it, the way the old
    /// shell mapped it.
    pub fn push_note(&mut self, note: &Note, now: Instant) {
        let action = note
            .undo_archive
            .clone()
            .map_or(ToastAction::None, ToastAction::Restore);
        self.push(
            ToastDraft {
                tone: note.tone,
                text: note.text.clone(),
                action,
            },
            now,
        );
    }

    /// Drops the toasts whose TTL ran out by `now`.
    pub fn expire(&mut self, now: Instant) {
        self.items.retain(|item| !expired(item.at, now));
    }

    /// The toasts in raise order, oldest first.
    #[must_use]
    pub fn items(&self) -> Vec<ToastDraft> {
        self.items.iter().map(|item| item.draft.clone()).collect()
    }

    /// The actions clicked since the last drain, oldest first. A click
    /// always dismisses its toast, so the shell only carries the
    /// action out.
    pub fn take_actions(&mut self) -> Vec<ToastAction> {
        std::mem::take(&mut self.fired)
    }

    /// Draws the stack, top-right, dropping what expired first.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        let now = Instant::now();
        self.expire(now);
        let Some(oldest) = self.items.first() else {
            return;
        };
        // An idle window draws no frame of its own, so it wakes for
        // the oldest toast's expiry or the toast would stay up.
        ui.ctx().request_repaint_after(
            TOAST_TTL.saturating_sub(now.saturating_duration_since(oldest.at)),
        );
        let palette = Palette::active_of(ui.ctx());
        let mut clicked = None;
        egui::Area::new(egui::Id::new("pure-toasts"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -42.0))
            .order(egui::Order::Foreground)
            .interactable(true)
            .movable(false)
            .show(ui.ctx(), |ui| {
                ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                    ui.spacing_mut().item_spacing.y = 8.0;
                    for (index, item) in self.items.iter().enumerate() {
                        if toast_card(ui, &item.draft, palette).clicked() {
                            clicked = Some(index);
                        }
                    }
                });
            });
        if let Some(index) = clicked {
            self.click(index);
        }
    }

    /// A click on the toast at `index`: the action is queued for the
    /// shell and the toast is dismissed either way.
    fn click(&mut self, index: usize) {
        let Some(item) = self.items.get(index) else {
            return;
        };
        self.fired.push(item.draft.action.clone());
        self.items.remove(index);
    }
}

/// One toast: the tone's icon beside the wrapped text on the raised
/// card, between 260 and 380px wide. The whole card answers a click.
fn toast_card(ui: &mut egui::Ui, draft: &ToastDraft, palette: &Palette) -> egui::Response {
    let (icon, color) = tone_look(draft.tone, palette);
    let mut job = egui::text::LayoutJob::simple(
        draft.text.clone(),
        egui::FontId::new(
            FS_SM,
            crate::theme::family(crate::theme::FAMILY_BODY_REGULAR),
        ),
        palette.ink,
        TOAST_MAX_W - 12.0 - 14.0 - 10.0 - 12.0,
    );
    job.wrap.max_width = TOAST_MAX_W - 12.0 - 14.0 - 10.0 - 12.0;
    let text = ui.painter().layout_job(job);
    let width = (12.0 + 14.0 + 10.0 + text.size().x + 12.0).clamp(TOAST_MIN_W, TOAST_MAX_W);
    let height = 10.0 + text.size().y.max(18.0) + 10.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &draft.text));
    let radius = egui::CornerRadius::same(R_LG as u8);
    ui.painter()
        .add(crate::shapes::shadow(rect, radius, &palette.shadow_2));
    ui.painter().rect_filled(rect, radius, palette.raised);
    let border = if response.hovered() {
        palette.line_strong
    } else {
        palette.line
    };
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(1.0, border),
        egui::StrokeKind::Inside,
    );
    crate::icons::themed_image(icon, 14.0, color).paint_snapped(
        ui,
        egui::Rect::from_min_size(rect.min + egui::vec2(12.0, 12.0), egui::vec2(14.0, 14.0)),
    );
    ui.painter().galley(
        rect.min + egui::vec2(12.0 + 14.0 + 10.0, 10.0),
        text,
        palette.ink,
    );
    response
}

/// The icon and color a notice tone renders in, matching the
/// transcript rows.
#[must_use]
fn tone_look(tone: NoticeTone, palette: &Palette) -> (crate::icons::Icon, egui::Color32) {
    match tone {
        NoticeTone::Success => (crate::icons::Icon::CircleCheck, palette.ok),
        NoticeTone::Error => (crate::icons::Icon::CircleX, palette.danger),
        NoticeTone::Warn => (crate::icons::Icon::TriangleAlert, palette.warn),
        NoticeTone::Info => (crate::icons::Icon::Info, palette.accent),
    }
}

/// The toasts the frame's store changes raise, the way the old shell
/// mapped them: an ask answered elsewhere points at its session, and
/// a failed request toasts its error.
#[must_use]
pub fn drafts_for_changes(changes: &[pure_client::Change]) -> Vec<ToastDraft> {
    changes
        .iter()
        .filter_map(|change| match change {
            pure_client::Change::AnsweredElsewhere { id, .. } => Some(ToastDraft {
                tone: NoticeTone::Info,
                text: crate::i18n::t("an approval was answered in another session").to_owned(),
                action: ToastAction::ActivateSession(id.clone()),
            }),
            pure_client::Change::Failed { error, .. } => Some(ToastDraft {
                tone: NoticeTone::Error,
                text: error.message.clone(),
                action: ToastAction::None,
            }),
            _ => None,
        })
        .collect()
}

/// Tracks how many notice items each session held at the last scan,
/// so a frame that adds notices raises its toast once. A session's
/// existing notices are history at first sight and stay quiet, and so
/// are the ones a load replays before the session opens.
#[derive(Debug, Default)]
pub struct NoticeWatch {
    seen: std::collections::HashMap<String, usize>,
}

impl NoticeWatch {
    /// The notices `state` holds beyond the last scan, as toasts.
    pub fn scan(&mut self, state: &pure_client::State) -> Vec<ToastDraft> {
        let mut drafts = Vec::new();
        for (id, session) in &state.sessions {
            let notices: Vec<&pure_client::TranscriptItem> = session
                .items
                .iter()
                .filter(|item| matches!(item, pure_client::TranscriptItem::Notice { .. }))
                .collect();
            let entry = self.seen.entry(id.clone()).or_insert(notices.len());
            if !session.opened {
                *entry = notices.len();
                continue;
            }
            if notices.len() > *entry {
                for item in notices[(*entry)..].iter() {
                    if let pure_client::TranscriptItem::Notice { tone, text } = item {
                        drafts.push(ToastDraft {
                            tone: *tone,
                            text: text.clone(),
                            action: ToastAction::None,
                        });
                    }
                }
                *entry = notices.len();
            }
        }
        drafts
    }
}

/// Whether the toast raised at `at` has expired by `now`. The TTL runs
/// from the raise, the way the old stack's per-toast timer did, and a
/// toast that has been up exactly [`TOAST_TTL`] is gone.
#[must_use]
fn expired(at: Instant, now: Instant) -> bool {
    now.saturating_duration_since(at) >= TOAST_TTL
}

#[cfg(test)]
mod tests {
    use super::{Raised, TOAST_MAX, TOAST_TTL, ToastAction, ToastDraft, Toasts, expired};
    use crate::store::Note;
    use pure_client::wire::NoticeTone;

    fn draft(text: &str, action: ToastAction) -> ToastDraft {
        ToastDraft {
            tone: NoticeTone::Info,
            text: text.to_owned(),
            action,
        }
    }

    /// A toast that has been up for `age`, as the queue holds it.
    fn raised(text: &str, age: Duration) -> Raised {
        let now = Instant::now();
        Raised {
            at: now - age,
            draft: draft(text, ToastAction::None),
        }
    }

    use std::time::Duration;
    use web_time::Instant;

    #[test]
    fn a_push_raises_the_toast_and_the_oldest_gives_way() {
        let now = Instant::now();
        let mut toasts = Toasts::new();
        for index in 0..=TOAST_MAX {
            toasts.push(draft(&format!("toast {index}"), ToastAction::None), now);
        }
        let texts: Vec<String> = toasts.items().into_iter().map(|item| item.text).collect();
        assert_eq!(texts.len(), TOAST_MAX, "the stack holds at most five");
        assert_eq!(texts.first().map(String::as_str), Some("toast 1"));
        assert_eq!(texts.last().map(String::as_str), Some("toast 5"));
    }

    #[test]
    fn the_ttl_expires_a_toast_and_nothing_before_it() {
        let now = Instant::now();
        assert!(!expired(now, now), "a toast just raised is up");
        assert!(!expired(now, now + TOAST_TTL - Duration::from_millis(1)));
        assert!(expired(now, now + TOAST_TTL), "the TTL is inclusive");

        let mut toasts = Toasts::new();
        toasts.push(draft("short lived", ToastAction::None), now);
        toasts.expire(now + TOAST_TTL - Duration::from_millis(1));
        assert_eq!(toasts.items().len(), 1, "one millisecond short of the TTL");
        toasts.expire(now + TOAST_TTL);
        assert!(toasts.items().is_empty(), "the TTL takes it");
    }

    #[test]
    fn expiry_drops_only_the_toasts_whose_ttl_ran_out() {
        let mut toasts = Toasts::new();
        let now = Instant::now();
        toasts.push(draft("old", ToastAction::None), now - TOAST_TTL);
        toasts.push(draft("new", ToastAction::None), now);
        toasts.expire(now + TOAST_TTL - Duration::from_millis(1));
        let texts: Vec<String> = toasts.items().into_iter().map(|item| item.text).collect();
        assert_eq!(texts, ["new"], "the older one expired, the newer one stays");
    }

    #[test]
    fn a_note_raises_its_tone_and_text_and_the_undo_action() {
        let now = Instant::now();
        let mut toasts = Toasts::new();
        toasts.push_note(
            &Note {
                tone: NoticeTone::Success,
                text: "Forked \"Parser\"".to_owned(),
                undo_archive: None,
            },
            now,
        );
        toasts.push_note(
            &Note {
                tone: NoticeTone::Info,
                text: "Archived \"Parser\"; click to undo".to_owned(),
                undo_archive: Some("s1".to_owned()),
            },
            now,
        );
        let items = toasts.items();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].tone, NoticeTone::Success);
        assert_eq!(items[0].action, ToastAction::None);
        assert_eq!(items[1].tone, NoticeTone::Info);
        assert_eq!(items[1].text, "Archived \"Parser\"; click to undo");
        assert_eq!(
            items[1].action,
            ToastAction::Restore("s1".to_owned()),
            "an archive note carries the restore action"
        );
    }

    #[test]
    fn a_click_queues_the_action_and_dismisses_the_toast() {
        let now = Instant::now();
        let mut toasts = Toasts::new();
        toasts.push(draft("context is filling up", ToastAction::None), now);
        toasts.push(
            draft(
                "an approval was answered elsewhere",
                ToastAction::ActivateSession("s2".to_owned()),
            ),
            now,
        );
        assert!(toasts.take_actions().is_empty(), "nothing clicked yet");

        let mut toasts = Toasts {
            items: vec![
                raised("context is filling up", Duration::ZERO),
                raised("an approval was answered elsewhere", Duration::ZERO),
            ],
            fired: Vec::new(),
        };
        // The second toast's action, fired by the click the shell saw.
        toasts
            .fired
            .push(ToastAction::ActivateSession("s2".to_owned()));
        let actions = toasts.take_actions();
        assert_eq!(actions, [ToastAction::ActivateSession("s2".to_owned())]);
        assert!(toasts.take_actions().is_empty(), "the drain takes once");
    }

    #[test]
    fn a_plain_toast_queues_no_action_on_its_click() {
        let mut toasts = Toasts {
            items: vec![raised("context is filling up", Duration::ZERO)],
            fired: Vec::new(),
        };
        toasts.click(0);
        assert_eq!(toasts.take_actions(), [ToastAction::None]);
        assert!(toasts.items().is_empty(), "the click dismissed it");
    }

    #[test]
    fn the_changes_bridge_maps_answered_elsewhere_and_failures() {
        use pure_client::{Change, RequestId};
        let changes = [
            Change::AnsweredElsewhere {
                id: "s1".to_owned(),
                request_id: RequestId::Number(7),
            },
            Change::Failed {
                request: 9,
                error: pure_client::RpcError {
                    code: -32000,
                    message: "the provider refused".to_owned(),
                    data: None,
                },
            },
            Change::Directory,
        ];
        let drafts = super::drafts_for_changes(&changes);
        assert_eq!(drafts.len(), 2, "only the two toast-worthy changes");
        assert_eq!(drafts[0].tone, NoticeTone::Info);
        assert_eq!(
            drafts[0].text,
            "an approval was answered in another session"
        );
        assert_eq!(
            drafts[0].action,
            ToastAction::ActivateSession("s1".to_owned())
        );
        assert_eq!(drafts[1].tone, NoticeTone::Error);
        assert_eq!(drafts[1].text, "the provider refused");
        assert_eq!(drafts[1].action, ToastAction::None);
    }

    #[test]
    fn a_new_notice_toasts_once_and_history_stays_quiet() {
        use pure_client::{Session, State, TranscriptItem};

        let session_with = |notices: usize| Session {
            items: (0..notices)
                .map(|ix| TranscriptItem::Notice {
                    tone: NoticeTone::Warn,
                    text: format!("notice {ix}"),
                })
                .collect(),
            ..Session::default()
        };
        let mut state = State::default();
        state.sessions.insert("s1".to_owned(), session_with(1));
        let mut watch = super::NoticeWatch::default();

        assert!(
            watch.scan(&state).is_empty(),
            "the first sight of a session is history"
        );
        state.sessions.insert("s1".to_owned(), session_with(1));
        assert!(watch.scan(&state).is_empty(), "no growth, no toast");

        let mut grown = session_with(1);
        grown.opened = true;
        grown.items.push(TranscriptItem::Notice {
            tone: NoticeTone::Warn,
            text: "notice 1".to_owned(),
        });
        state.sessions.insert("s1".to_owned(), grown);
        let drafts = watch.scan(&state);
        assert_eq!(drafts.len(), 1, "the notice beyond the last scan toasts");
        assert_eq!(drafts[0].text, "notice 1");
        assert!(watch.scan(&state).is_empty(), "it toasts once");
    }

    #[test]
    fn notices_a_session_receives_before_it_opens_stay_quiet() {
        use pure_client::{Session, State, TranscriptItem};

        let mut state = State::default();
        state.sessions.insert("s1".to_owned(), Session::default());
        let mut watch = super::NoticeWatch::default();
        assert!(watch.scan(&state).is_empty());

        let mut loaded = Session {
            opened: false,
            ..Session::default()
        };
        loaded.items.push(TranscriptItem::Notice {
            tone: NoticeTone::Error,
            text: "replayed from the load".to_owned(),
        });
        state.sessions.insert("s1".to_owned(), loaded.clone());
        assert!(
            watch.scan(&state).is_empty(),
            "a load's replays are not news"
        );

        let mut opened = loaded.clone();
        opened.opened = true;
        state.sessions.insert("s1".to_owned(), opened);
        assert!(watch.scan(&state).is_empty(), "already counted");
    }
}

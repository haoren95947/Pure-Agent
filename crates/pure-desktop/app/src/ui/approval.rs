//! The approval card: the open permission asks and their verdicts.
//!
//! One view over the store, for the shell to mount where the dock
//! renders the approval card; it draws nothing while no ask is open.
//! It shows the asks of the active session and the subagents under
//! it, one at a time, oldest first. The card renders the ask exactly
//! as the agent offered it: the shield glyph, or the swarm glyph when
//! a child asked, the tool title, the asking session's byline, the
//! subject and the remaining input detail the ask carried, and the
//! options in offer order numbered from 1. Option ids and labels are
//! the offered ones; the card never synthesizes either.
//!
//! Answering:
//!
//! - A click on an option button answers with that option's offered
//!   id.
//! - The number keys 1 to 9 answer the oldest open ask, while the card
//!   is the shell's key context and no text field holds the keys.
//! - The feedback field answers with the ask's first reject option
//!   and sends the typed text through the client's feedback channel,
//!   which carries it to the agent under `_meta.pure.planReview`; the
//!   field renders only when the ask offers a reject.
//!
//! An `ask_user_question` ask, one carrying `_meta.pure.question`,
//! renders as a question card instead: the question, its choices with
//! their descriptions, a field for an answer in your own words and a
//! skip button. A choice answers at once; on a question that allows
//! several, choices toggle and `Done` sends them. The whole answer
//! rides `_meta.pure.question.answer`.

use crate::icons::PaintSnapped as _;
use crate::ui::forms::ShowCrisp as _;
use std::collections::{BTreeSet, HashMap};

use pure_client::wire::{PermissionOptionKind, QuestionPrompt};
use pure_client::{PermissionAsk, PermissionDecision, RequestId, SUBJECT_KEYS};
use serde_json::Value;

use crate::store::Store;
use crate::theme::{
    FAMILY_BODY_SEMIBOLD, FAMILY_MONO_REGULAR, FS_2XS, FS_BASE, FS_SM, FS_XS, Palette, R_FULL,
    R_LG, R_MD, SP_4, family,
};

/// The three tones an option button carries: the filled success for
/// the go-ahead answer, the outlined danger for the refuse answers,
/// and the plain hairline for everything in between.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BtnTone {
    /// The filled success button.
    Ok,
    /// The outlined danger button.
    Danger,
    /// The plain hairline button.
    Plain,
}

/// The text one input value renders as, strings verbatim.
#[must_use]
pub fn value_text(value: &Value) -> String {
    match value.as_str() {
        Some(text) => text.to_owned(),
        None => value.to_string(),
    }
}

/// The subject an ask is about: the input's primary argument, or the
/// whole input as the wire carried it. `None` when the ask carries no
/// input at all.
#[must_use]
pub fn subject_of(ask: &PermissionAsk) -> Option<String> {
    let input = ask.tool_call.raw_input.as_ref()?;
    SUBJECT_KEYS
        .iter()
        .find_map(|key| input.get(*key).and_then(Value::as_str))
        .map(str::to_owned)
        .or_else(|| Some(input.to_string()))
}

/// The rest of the input the subject line does not already show, as
/// `key: value` entries. `None` when the subject came from the whole
/// input or nothing remains.
#[must_use]
pub fn reason_of(ask: &PermissionAsk) -> Option<String> {
    let input = ask.tool_call.raw_input.as_ref()?.as_object()?;
    let subject_key = SUBJECT_KEYS
        .iter()
        .find(|key| input.contains_key(**key))
        .copied()?;
    let entries: Vec<String> = input
        .iter()
        .filter(|(key, _)| Some(key.as_str()) != Some(subject_key))
        .map(|(key, value)| format!("{key}: {}", value_text(value)))
        .collect();
    if entries.is_empty() {
        None
    } else {
        Some(entries.join(", "))
    }
}

/// The id of the ask's first reject option: the option the feedback
/// field answers with. `None` when the ask offers no reject, in which
/// case the card offers no feedback field.
#[must_use]
pub fn reject_option_id(ask: &PermissionAsk) -> Option<&str> {
    ask.options
        .iter()
        .find(|option| {
            matches!(
                option.kind,
                PermissionOptionKind::RejectOnce | PermissionOptionKind::RejectAlways
            )
        })
        .map(|option| option.option_id.as_str())
}

/// What a digit key does to the oldest open ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DigitAction {
    /// Toggle the choice at this zero-based index; the question
    /// allows several picks.
    Toggle(usize),
    /// Answer with the option at this zero-based index.
    Answer(usize),
}

/// Decides what the 1-based `digit` does to `ask`. On a multi-select
/// question every offered choice toggles, the last included, bounded
/// by the question's own choices; anything else answers with the
/// named option, bounded by the ask's options. A digit past what is
/// offered does nothing.
#[must_use]
pub fn digit_action(digit: usize, ask: &PermissionAsk) -> Option<DigitAction> {
    let multi = ask.question.as_ref().is_some_and(|q| q.multi_select);
    let len = match ask.question.as_ref() {
        Some(question) if multi => question.options.len(),
        _ => ask.options.len(),
    };
    if digit == 0 || digit > len {
        return None;
    }
    let index = digit - 1;
    if multi {
        Some(DigitAction::Toggle(index))
    } else {
        Some(DigitAction::Answer(index))
    }
}

/// What the card asks the shell to carry out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalEvent {
    /// Answer the ask `request_id` of `session` with this decision.
    Reply(String, RequestId, PermissionDecision),
}

/// The approval card's state: the feedback text of each open ask and
/// the choices toggled on a multi-select question.
#[derive(Debug, Default)]
pub struct Approval {
    /// The feedback field text of each open ask, by request id.
    feedback: HashMap<RequestId, String>,
    /// The choices toggled on a question that allows several, by
    /// request id.
    picks: HashMap<RequestId, BTreeSet<usize>>,
    /// Which ask holds the keys for the digit answers.
    focused: Option<RequestId>,
    /// How many asks were open at the last [`Approval::sync_open`].
    seen: usize,
}

impl Approval {
    /// An approval card with no open ask.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes the keys for ask `request_id`, so its digits answer it.
    pub fn focus(&mut self, request_id: &RequestId) {
        self.focused = Some(request_id.clone());
    }

    /// Hands the keys back, so a digit reaches the shell again.
    pub fn blur(&mut self) {
        self.focused = None;
    }

    /// Records that `open` asks are open now and reports the edge:
    /// `Some(true)` when a first ask opened, `Some(false)` when the
    /// last one closed, `None` otherwise.
    pub fn sync_open(&mut self, open: usize) -> Option<bool> {
        let was = std::mem::replace(&mut self.seen, open);
        match (was, open) {
            (0, 1..) => Some(true),
            (1.., 0) => Some(false),
            _ => None,
        }
    }

    /// What the 1-based `digit` does to the oldest open ask `ask`, or
    /// `None` when this card does not hold the keys for it.
    #[must_use]
    pub fn digit(&self, digit: usize, ask: &PermissionAsk) -> Option<DigitAction> {
        if self.focused.as_ref() != Some(&ask.request_id) {
            return None;
        }
        digit_action(digit, ask)
    }

    /// Turn choice `index` of the question `request_id` on or off.
    pub fn toggle(&mut self, request_id: RequestId, index: usize) {
        let picks = self.picks.entry(request_id).or_default();
        if !picks.remove(&index) {
            picks.insert(index);
        }
    }

    /// The choices toggled on the question `request_id`.
    #[must_use]
    pub fn picks(&self, request_id: &RequestId) -> Vec<usize> {
        self.picks
            .get(request_id)
            .map(|picks| picks.iter().copied().collect())
            .unwrap_or_default()
    }

    /// The feedback text of ask `request_id`.
    #[must_use]
    pub fn feedback(&self, request_id: &RequestId) -> &str {
        self.feedback.get(request_id).map_or("", String::as_str)
    }

    /// Sets the feedback text of ask `request_id`.
    pub fn set_feedback(&mut self, request_id: &RequestId, text: String) {
        if text.is_empty() {
            self.feedback.remove(request_id);
        } else {
            self.feedback.insert(request_id.clone(), text);
        }
    }

    /// Drops the state of ask `request_id`, once it is answered.
    pub fn forget(&mut self, request_id: &RequestId) {
        self.feedback.remove(request_id);
        self.picks.remove(request_id);
        if self.focused.as_ref() == Some(request_id) {
            self.focused = None;
        }
    }

    /// Draws the oldest open ask of `store` and reports what it asked
    /// for. Draws nothing while no ask is open.
    pub fn show(&mut self, ui: &mut egui::Ui, store: &Store) -> Vec<ApprovalEvent> {
        let mut events = Vec::new();
        let Some((session_id, ask)) = store
            .active_asks()
            .into_iter()
            .next()
            .map(|(session_id, ask)| (session_id.to_owned(), ask.clone()))
        else {
            return events;
        };
        let palette = Palette::active_of(ui.ctx());
        let byline = store.state().asker_byline(&session_id);
        let child = byline
            .as_ref()
            .is_some_and(|byline| byline.starts_with("sub agent"));
        let title = ask
            .tool_call
            .title
            .clone()
            .unwrap_or_else(|| ask.tool_call.tool_call_id.clone());
        let (border, fill) = match &ask.question {
            Some(_) => (palette.line, palette.surface),
            None => (palette.ok_bd, palette.surface),
        };
        let mut answer: Option<PermissionDecision> = None;
        let ring = if ask.question.is_some() {
            egui::Shadow::NONE
        } else {
            egui::Shadow {
                offset: [0, 0],
                blur: 0,
                spread: 4,
                color: palette.ok_soft,
            }
        };
        if let Some(question) = &ask.question {
            egui::Frame::NONE
                .fill(fill)
                .stroke(egui::Stroke::new(1.0, border))
                .corner_radius(egui::CornerRadius::same(R_LG as u8))
                .show_crisp(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    self.question_body(
                        ui,
                        store,
                        &session_id,
                        &ask,
                        question,
                        palette,
                        &mut answer,
                        &mut events,
                    );
                });
            if let Some(decision) = answer {
                events.push(ApprovalEvent::Reply(
                    session_id,
                    ask.request_id.clone(),
                    decision,
                ));
                self.forget(&ask.request_id);
            }
            return events;
        }
        egui::Frame::NONE
            .fill(fill)
            .stroke(egui::Stroke::new(1.0, border))
            .corner_radius(egui::CornerRadius::same(R_LG as u8))
            .shadow(ring)
            .inner_margin(egui::Margin::symmetric(14, 12))
            .show_crisp(ui, |ui| {
                ui.set_min_width(ui.available_width());
                // The head: the glyph, the tool title and the byline.
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let glyph = if ask.question.is_some() {
                        crate::icons::Icon::MessageSquare
                    } else if child {
                        crate::icons::Icon::Waypoints
                    } else {
                        crate::icons::Icon::ShieldQuestionMark
                    };
                    let tint = if ask.question.is_some() {
                        palette.accent
                    } else {
                        palette.ok
                    };
                    crate::icons::icon(ui, glyph, 14.0, tint);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&title)
                                .size(FS_BASE)
                                .family(family(FAMILY_BODY_SEMIBOLD))
                                .color(palette.ink_strong),
                        )
                        .truncate(),
                    );
                    if let Some(byline) = &byline {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            egui::Frame::NONE
                                .fill(palette.fill)
                                .corner_radius(egui::CornerRadius::same(R_FULL as u8))
                                .inner_margin(egui::Margin::symmetric(8, 2))
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(byline)
                                            .size(FS_2XS)
                                            .color(palette.muted),
                                    );
                                });
                        });
                    }
                });
                self.ask_body(ui, &ask, palette, &mut answer);
            });
        if let Some(decision) = answer {
            events.push(ApprovalEvent::Reply(
                session_id,
                ask.request_id.clone(),
                decision,
            ));
            self.forget(&ask.request_id);
        }
        events
    }

    /// A plain permission ask: the subject, the reason and the
    /// numbered options.
    fn ask_body(
        &mut self,
        ui: &mut egui::Ui,
        ask: &PermissionAsk,
        palette: &Palette,
        answer: &mut Option<PermissionDecision>,
    ) {
        if let Some(subject) = subject_of(ask) {
            ui.add_space(8.0);
            egui::Frame::NONE
                .fill(palette.deep)
                .corner_radius(egui::CornerRadius::same(R_MD as u8))
                .inner_margin(egui::Margin::symmetric(12, 9))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(
                        egui::RichText::new(subject)
                            .size(12.5)
                            .family(family(FAMILY_MONO_REGULAR))
                            .color(palette.ink_strong),
                    );
                });
        }
        if let Some(reason) = reason_of(ask) {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(reason).size(FS_XS).color(palette.muted));
        }
        ui.add_space(12.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.spacing_mut().item_spacing.y = 8.0;
            for (index, option) in ask.options.iter().enumerate() {
                let number = index + 1;
                let reject = matches!(
                    option.kind,
                    PermissionOptionKind::RejectOnce | PermissionOptionKind::RejectAlways
                );
                let tone = if reject {
                    BtnTone::Danger
                } else if index == 0 {
                    BtnTone::Ok
                } else {
                    BtnTone::Plain
                };
                let (fg, bg, border) = match tone {
                    BtnTone::Ok => (palette.ok_ink, palette.ok, palette.ok),
                    BtnTone::Danger => (
                        palette.danger,
                        egui::Color32::TRANSPARENT,
                        palette.danger_bd,
                    ),
                    BtnTone::Plain => (palette.ink, palette.fill, palette.line),
                };
                let response = option_button(ui, &option.name, number, fg, bg, border);
                if response.clicked() {
                    *answer = Some(PermissionDecision::Option(option.option_id.clone()));
                }
            }
        });
        if reject_option_id(ask).is_some() {
            ui.add_space(12.0);
            let mut text = self.feedback(&ask.request_id).to_owned();
            let field = egui::TextEdit::singleline(&mut text)
                .hint_text(crate::i18n::t("Reject with feedback..."))
                .desired_width(f32::INFINITY)
                .font(egui::FontId::new(
                    FS_SM,
                    family(crate::theme::FAMILY_BODY_REGULAR),
                ))
                .margin(egui::Margin::symmetric(10, 8));
            let response = ui.add(field);
            if response.changed() {
                self.set_feedback(&ask.request_id, text.clone());
            }
            let _ = response;
            let reject = reject_option_id(ask);
            if !text.is_empty()
                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                && let Some(reject) = reject
            {
                *answer = Some(PermissionDecision::Feedback {
                    option_id: reject.to_owned(),
                    feedback: text,
                });
            }
        }
    }

    /// An `ask_user_question` ask, as the reference's card sets it: the
    /// glyph and header chip over the question, a row per choice with
    /// its description, then the own-words field beside Done and Skip.
    #[allow(clippy::too_many_arguments)]
    fn question_body(
        &mut self,
        ui: &mut egui::Ui,
        _store: &Store,
        session_id: &str,
        ask: &PermissionAsk,
        question: &QuestionPrompt,
        palette: &Palette,
        answer: &mut Option<PermissionDecision>,
        events: &mut Vec<ApprovalEvent>,
    ) {
        egui::Frame::NONE
            .inner_margin(egui::Margin {
                left: 14,
                right: 14,
                top: 12,
                bottom: 8,
            })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 4.0;
                let chip = ui.painter().layout_no_wrap(
                    question.header.clone(),
                    egui::FontId::new(FS_2XS, crate::theme::prose_family(ui.ctx())),
                    palette.accent,
                );
                let chip_h = crate::theme::line_h(FS_2XS) + 2.0;
                let (row, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), chip_h.max(14.0)),
                    egui::Sense::hover(),
                );
                let mid = row.center().y;
                crate::icons::themed_image(crate::icons::Icon::MessageSquare, 14.0, palette.accent)
                    .paint_snapped(
                        ui,
                        egui::Rect::from_min_size(
                            egui::pos2(row.left(), mid - 7.0),
                            egui::vec2(14.0, 14.0),
                        ),
                    );
                let pill = egui::Rect::from_min_size(
                    egui::pos2(row.left() + 14.0 + 8.0, mid - chip_h / 2.0),
                    egui::vec2(chip.size().x.ceil() + 16.0, chip_h),
                );
                ui.painter().rect_filled(
                    pill,
                    egui::CornerRadius::same(R_FULL as u8),
                    palette.accent_soft,
                );
                ui.painter()
                    .galley(pill.min + egui::vec2(8.0, 1.0), chip, palette.accent);
                let asked = ui.painter().layout(
                    question.question.clone(),
                    egui::FontId::new(FS_BASE, crate::theme::prose_semibold_family(ui.ctx())),
                    palette.ink_strong,
                    ui.available_width(),
                );
                let (rect, response) = ui.allocate_exact_size(
                    egui::vec2(
                        ui.available_width(),
                        asked.rows.len() as f32 * crate::theme::line_h(FS_BASE),
                    ),
                    egui::Sense::hover(),
                );
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &question.question)
                });
                ui.painter().galley(rect.min, asked, palette.ink_strong);
            });
        let picked = self.picks(&ask.request_id);
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(10, 0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 2.0;
                for (index, choice) in question.options.iter().enumerate() {
                    let on = picked.contains(&index);
                    let clicked = choice_row(
                        ui,
                        index,
                        &choice.label,
                        &choice.description,
                        question.multi_select.then_some(on),
                        palette,
                    );
                    if clicked {
                        if question.multi_select {
                            self.toggle(ask.request_id.clone(), index);
                        } else {
                            *answer = Some(PermissionDecision::Option(format!("choice-{index}")));
                        }
                    }
                }
            });
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(14, 12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let (row, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), crate::ui::forms::FIELD_H.max(26.0)),
                    egui::Sense::hover(),
                );
                let mid = row.center().y;
                let mut right = row.right();
                if let Some(skip) = reject_option_id(ask) {
                    let button = crate::ui::settings_config::btn_sm_at(
                        ui,
                        egui::pos2(right, mid),
                        egui::Align2::RIGHT_CENTER,
                        ui.id().with(("question-skip", &ask.request_id)),
                        crate::i18n::t("Skip"),
                        None,
                        crate::ui::settings_config::BtnTone::Plain,
                        palette,
                    );
                    if button.clicked() {
                        *answer = Some(PermissionDecision::Option(skip.to_owned()));
                    }
                    right = button.rect.left() - SP_4;
                }
                if question.multi_select {
                    let labels: Vec<String> = picked
                        .iter()
                        .filter_map(|index| question.options.get(*index))
                        .map(|choice| choice.label.clone())
                        .collect();
                    let first = picked.first().copied().unwrap_or(0);
                    let done = crate::tr!("Done ({})", labels.len());
                    let button = crate::ui::settings_config::btn_sm_at(
                        ui,
                        egui::pos2(right, mid),
                        egui::Align2::RIGHT_CENTER,
                        ui.id().with(("question-done", &ask.request_id)),
                        &done,
                        None,
                        crate::ui::settings_config::BtnTone::Primary,
                        palette,
                    );
                    if button.clicked() && !labels.is_empty() {
                        *answer = Some(PermissionDecision::Answer {
                            option_id: format!("choice-{first}"),
                            answer: labels,
                        });
                    }
                    right = button.rect.left() - SP_4;
                }
                let field_rect = egui::Rect::from_min_max(
                    row.min,
                    egui::pos2(right.max(row.left() + 180.0), row.max.y),
                );
                let mut text = self.feedback(&ask.request_id).to_owned();
                let response = ui
                    .scope_builder(egui::UiBuilder::new().max_rect(field_rect), |ui| {
                        crate::ui::forms::input_field(
                            ui,
                            &mut text,
                            crate::i18n::t("Answer in your own words..."),
                            false,
                            palette,
                        )
                    })
                    .inner;
                if response.changed() {
                    self.set_feedback(&ask.request_id, text.clone());
                }
                let text = text.trim().to_owned();
                if !text.is_empty()
                    && response.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                {
                    events.push(ApprovalEvent::Reply(
                        session_id.to_owned(),
                        ask.request_id.clone(),
                        PermissionDecision::Answer {
                            option_id: "choice-0".to_owned(),
                            answer: vec![text],
                        },
                    ));
                }
            });
    }
}

/// One choice of a question: the digit cap on the first line, the
/// check circle when several may be picked, the label and its
/// description. Answers whether it was clicked.
fn choice_row(
    ui: &mut egui::Ui,
    index: usize,
    label: &str,
    description: &str,
    picked: Option<bool>,
    palette: &Palette,
) -> bool {
    let label_h = crate::theme::line_h(FS_SM);
    let desc_h = if description.is_empty() {
        0.0
    } else {
        crate::theme::line_h(FS_XS)
    };
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 6.0 + label_h + desc_h + 6.0),
        egui::Sense::click(),
    );
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    let radius = egui::CornerRadius::same(R_MD as u8);
    if picked == Some(true) {
        ui.painter().rect_filled(rect, radius, palette.selected);
    }
    if response.hovered() {
        ui.painter().rect_filled(rect, radius, palette.hover);
    }
    let line_top = rect.top() + 6.0;
    let digit = ui.painter().layout_no_wrap(
        (index + 1).to_string(),
        egui::FontId::new(10.0, family(FAMILY_MONO_REGULAR)),
        palette.muted,
    );
    let cap = egui::Rect::from_min_size(
        egui::pos2(rect.left() + 8.0, line_top),
        egui::vec2(digit.size().x + 10.0, 16.0),
    );
    let faded = palette.muted.gamma_multiply(0.6);
    ui.painter().rect_stroke(
        cap,
        5,
        egui::Stroke::new(1.0, faded),
        egui::StrokeKind::Inside,
    );
    ui.painter()
        .galley(cap.center() - digit.size() / 2.0, digit, faded);
    let mut x = cap.right() + 10.0;
    if let Some(on) = picked {
        let glyph = if on {
            crate::icons::Icon::CircleCheck
        } else {
            crate::icons::Icon::Circle
        };
        let tint = if on { palette.accent } else { palette.faint };
        crate::icons::themed_image(glyph, 14.0, tint).paint_snapped(
            ui,
            egui::Rect::from_min_size(egui::pos2(x, line_top), egui::vec2(14.0, 14.0)),
        );
        x += 14.0 + 10.0;
    }
    let width = rect.right() - 8.0 - x;
    let name = crate::ui::transcript::head::one_line(
        ui,
        label.to_owned(),
        egui::TextFormat::simple(
            egui::FontId::new(FS_SM, crate::theme::prose_family(ui.ctx())),
            palette.ink_strong,
        ),
        width,
    );
    ui.painter()
        .galley(egui::pos2(x, line_top), name, palette.ink_strong);
    if !description.is_empty() {
        let note = crate::ui::transcript::head::one_line(
            ui,
            description.to_owned(),
            egui::TextFormat::simple(
                egui::FontId::new(FS_XS, crate::theme::prose_family(ui.ctx())),
                palette.muted,
            ),
            width,
        );
        let note_rect = egui::Rect::from_min_size(egui::pos2(x, line_top + label_h), note.size());
        ui.interact(
            note_rect,
            ui.id().with(("choice-note", index)),
            egui::Sense::hover(),
        )
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, description));
        ui.painter().galley(note_rect.min, note, palette.muted);
    }
    response.clicked()
}

/// One small option button as the design draws it: 26px tall, an 8px
/// radius, 12px medium text, with the digit key cap at its right.
fn option_button(
    ui: &mut egui::Ui,
    label: &str,
    digit: usize,
    fg: egui::Color32,
    fill: egui::Color32,
    border: egui::Color32,
) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::new(FS_XS, family(crate::theme::FAMILY_BODY_MEDIUM)),
        fg,
    );
    let cap = if digit > 0 { 6.0 + 16.0 } else { 0.0 };
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(9.0 + galley.size().x + cap + 9.0, 26.0),
        egui::Sense::click(),
    );
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    let radius = egui::CornerRadius::same(R_MD as u8);
    ui.painter().rect_filled(rect, radius, fill);
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(1.0, border),
        egui::StrokeKind::Inside,
    );
    let text_w = galley.size().x;
    ui.painter().galley(
        egui::pos2(
            rect.left() + 9.0,
            crate::theme::line_y(rect.center().y, 0.0, &galley),
        ),
        galley,
        fg,
    );
    if digit > 0 {
        key_cap(
            ui,
            egui::pos2(rect.left() + 9.0 + text_w + 6.0 + 8.0, rect.center().y),
            digit,
            fg,
        );
    }
    response
}

/// The digit key cap: a hairline box in the mono family at 0.6
/// opacity, painted inside `center`.
fn key_cap(ui: &mut egui::Ui, center: egui::Pos2, digit: usize, fg: egui::Color32) {
    let painter = ui.painter();
    let size = egui::vec2(16.0, 16.0);
    let rect = egui::Rect::from_center_size(center, size);
    let stroke = egui::Stroke::new(1.0, fg.gamma_multiply(0.6));
    painter.rect_stroke(rect, 5, stroke, egui::StrokeKind::Inside);
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        digit.to_string(),
        egui::FontId::new(10.0, family(FAMILY_MONO_REGULAR)),
        fg.gamma_multiply(0.6),
    );
}

#[cfg(test)]
mod tests {
    use super::{
        Approval, DigitAction, digit_action, reason_of, reject_option_id, subject_of, value_text,
    };
    use pure_client::wire::{PermissionOption, PermissionOptionKind, ToolCallUpdate};
    use pure_client::{PermissionAsk, RequestId};
    use serde_json::json;

    /// An ask with the given raw input and options.
    fn ask(raw_input: Option<serde_json::Value>, options: Vec<PermissionOption>) -> PermissionAsk {
        PermissionAsk {
            request_id: RequestId::Number(1),
            tool_call: ToolCallUpdate {
                tool_call_id: "call-sh".into(),
                title: Some("shell".into()),
                raw_input,
                ..ToolCallUpdate::default()
            },
            options,
            plan: None,
            question: None,
        }
    }

    /// An allow option.
    fn allow(id: &str) -> PermissionOption {
        PermissionOption {
            option_id: id.into(),
            name: "Allow".into(),
            kind: PermissionOptionKind::AllowOnce,
        }
    }

    /// A reject option.
    fn reject(id: &str) -> PermissionOption {
        PermissionOption {
            option_id: id.into(),
            name: "Reject".into(),
            kind: PermissionOptionKind::RejectOnce,
        }
    }

    #[test]
    fn subject_and_reason_come_from_the_delivered_input_only() {
        let shell = ask(Some(json!({"command": "cargo test"})), Vec::new());
        assert_eq!(subject_of(&shell).as_deref(), Some("cargo test"));
        assert_eq!(reason_of(&shell), None, "nothing remains to show");

        let agent = ask(
            Some(json!({"description": "list files", "prompt": "list"})),
            Vec::new(),
        );
        assert_eq!(subject_of(&agent).as_deref(), Some("list files"));
        assert_eq!(reason_of(&agent).as_deref(), Some("prompt: list"));

        let bare = ask(None, Vec::new());
        assert_eq!(subject_of(&bare), None);
        assert_eq!(reason_of(&bare), None);
    }

    #[test]
    fn a_non_string_value_renders_as_its_json() {
        assert_eq!(value_text(&json!(3)), "3");
        assert_eq!(value_text(&json!("three")), "three");
    }

    #[test]
    fn the_feedback_field_targets_the_first_reject_option() {
        let asked = ask(
            None,
            vec![allow("approve"), reject("revise"), reject("reject")],
        );
        assert_eq!(reject_option_id(&asked), Some("revise"));
        let allow_only = ask(None, vec![allow("ok")]);
        assert_eq!(reject_option_id(&allow_only), None);
    }

    #[test]
    fn a_digit_answers_the_option_at_its_index() {
        let asked = ask(
            None,
            vec![allow("approve"), reject("revise"), reject("reject")],
        );
        assert_eq!(digit_action(1, &asked), Some(DigitAction::Answer(0)));
        assert_eq!(digit_action(3, &asked), Some(DigitAction::Answer(2)));
        assert_eq!(digit_action(4, &asked), None, "past what is offered");
        assert_eq!(digit_action(0, &asked), None, "zero is no digit");
    }

    #[test]
    fn a_digit_on_a_multi_select_question_toggles_instead() {
        let mut asked = ask(None, Vec::new());
        asked.question = Some(pure_client::wire::QuestionPrompt {
            header: "Which".into(),
            question: "Pick".into(),
            multi_select: true,
            options: vec![
                pure_client::wire::QuestionChoice {
                    label: "one".into(),
                    description: String::new(),
                },
                pure_client::wire::QuestionChoice {
                    label: "two".into(),
                    description: String::new(),
                },
            ],
        });
        assert_eq!(digit_action(1, &asked), Some(DigitAction::Toggle(0)));
        assert_eq!(digit_action(2, &asked), Some(DigitAction::Toggle(1)));
        assert_eq!(digit_action(3, &asked), None, "past the choices offered");
    }

    #[test]
    fn the_card_answers_a_digit_only_while_it_holds_the_keys() {
        let asked = ask(None, vec![allow("approve"), reject("reject")]);
        let mut card = Approval::new();
        assert_eq!(
            card.digit(1, &asked),
            None,
            "a card that holds no keys answers nothing"
        );
        card.focus(&asked.request_id);
        assert_eq!(card.digit(1, &asked), Some(DigitAction::Answer(0)));
        card.blur();
        assert_eq!(card.digit(1, &asked), None, "the keys went back");
    }

    #[test]
    fn a_toggled_choice_flips_and_the_picks_survive_a_redraw() {
        let request = RequestId::Number(7);
        let mut card = Approval::new();
        assert!(card.picks(&request).is_empty());
        card.toggle(request.clone(), 1);
        assert_eq!(card.picks(&request), vec![1]);
        card.toggle(request.clone(), 1);
        assert!(
            card.picks(&request).is_empty(),
            "the second press takes it off"
        );
        card.toggle(request.clone(), 0);
        card.toggle(request.clone(), 2);
        assert_eq!(card.picks(&request), vec![0, 2], "the set keeps its order");
    }

    #[test]
    fn feedback_text_survives_a_redraw_and_clears_when_emptied() {
        let request = RequestId::Number(9);
        let mut card = Approval::new();
        assert_eq!(card.feedback(&request), "");
        card.set_feedback(&request, "not like that".to_owned());
        assert_eq!(card.feedback(&request), "not like that");
        card.set_feedback(&request, String::new());
        assert_eq!(card.feedback(&request), "");
    }

    #[test]
    fn forgetting_an_ask_drops_its_field_its_picks_and_its_focus() {
        let request = RequestId::Number(11);
        let mut card = Approval::new();
        card.set_feedback(&request, "text".to_owned());
        card.toggle(request.clone(), 0);
        card.focus(&request);
        card.forget(&request);
        assert_eq!(card.feedback(&request), "");
        assert!(card.picks(&request).is_empty());
        assert_eq!(card.digit(1, &ask(None, vec![allow("a")])), None);
    }
}

//! The gate banner.
//!
//! The banner shows when the initialize answer fell short of what the
//! shell gates on, and dismisses for that answer; a reconnect that
//! still falls short brings it back. The connection itself reads in
//! the sidebar's footer, as the reference has it.

use crate::gate::Report;
use crate::theme::{FS_XS, Palette, SP_4, SP_6};

/// The gate banner, when the initialize answer fell short.
/// Dismissible for that answer: the shell calls
/// [`Banner::dismiss`] on the close click and [`Banner::reset`] when a
/// reconnect starts, so a still-short answer shows again.
#[derive(Debug, Clone, Default)]
pub struct Banner {
    /// Whether the user dismissed the current report.
    dismissed: bool,
}

impl Banner {
    /// Takes the dismissal back, for a fresh answer.
    pub fn reset(&mut self) {
        self.dismissed = false;
    }

    /// Dismisses the current report.
    pub fn dismiss(&mut self) {
        self.dismissed = true;
    }

    /// Whether the banner shows now: a report that falls short and no
    /// dismissal against it.
    #[must_use]
    pub fn shows(&self, report: &Report) -> bool {
        !report.is_clean() && !self.dismissed
    }

    /// Draws the banner, when it shows. The close click dismisses.
    pub fn show(&mut self, ui: &mut egui::Ui, report: &Report) {
        if !self.shows(report) {
            return;
        }
        let palette = Palette::active_of(ui.ctx());
        let mut dismiss = false;
        egui::Panel::top("gate-banner")
            .frame(
                egui::Frame::NONE
                    .fill(palette.warn_soft)
                    .inner_margin(egui::Margin::symmetric(SP_6 as i8, SP_4 as i8))
                    .stroke(egui::Stroke::new(1.0, palette.warn_bd)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(crate::i18n::t(
                            "this agent is newer than the shell expects",
                        ))
                        .size(FS_XS)
                        .color(palette.ink_strong),
                    );
                    for line in report.lines() {
                        ui.label(egui::RichText::new(line).size(FS_XS).color(palette.muted));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new(
                                egui::RichText::new(crate::i18n::t("dismiss"))
                                    .size(FS_XS)
                                    .color(palette.muted),
                            ))
                            .clicked()
                        {
                            dismiss = true;
                        }
                    });
                });
            });
        if dismiss {
            self.dismiss();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Banner;
    use crate::gate::{Capability, Report};

    #[test]
    fn a_clean_report_shows_nothing() {
        let banner = Banner::default();
        assert!(!banner.shows(&Report::default()));
    }

    #[test]
    fn a_short_report_shows_until_dismissed() {
        let mut banner = Banner::default();
        let report = Report {
            too_old: Some("0.0.1".to_owned()),
            missing: vec![Capability::CloseSession],
        };
        assert!(banner.shows(&report));
        banner.dismiss();
        assert!(!banner.shows(&report), "the dismissal holds");
    }

    #[test]
    fn a_reconnect_brings_a_short_report_back() {
        let mut banner = Banner::default();
        let report = Report {
            too_old: Some("0.0.1".to_owned()),
            missing: Vec::new(),
        };
        banner.dismiss();
        banner.reset();
        assert!(banner.shows(&report), "a fresh answer shows again");
    }
}

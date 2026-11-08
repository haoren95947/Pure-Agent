//! The boot splash: the nekomata mark, the pure wordmark and a
//! connecting line over the whole window while the first connection is
//! made, faded out once the engine answers the handshake.
//!
//! Only the first connection shows it. A first attempt that fails takes
//! it down at once, so the shell's own connection status, or the setup
//! screen, is what the user sees.

use std::time::Duration;

use web_time::Instant;

use crate::theme::FS_SM;
use crate::transport::State;

/// How long the splash takes to fade out.
const FADE: Duration = Duration::from_millis(300);

/// The wordmark's size, the design's 40px.
const WORDMARK: f32 = 40.0;

/// The mark's height, the design's 44px.
const MARK: f32 = 44.0;

/// Where the boot splash stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Splash {
    /// Up while the first connection is made.
    Showing,
    /// Fading out since this instant.
    Fading(Instant),
    /// Gone for good.
    Gone,
}

impl Splash {
    /// Moves on with the connection: fades once `connect` is up and the
    /// handshake was `answered`, and goes at once when the first
    /// attempt failed.
    pub fn follow(&mut self, connect: &State, answered: bool) {
        match self {
            Self::Showing => match connect {
                State::Connected if answered => *self = Self::Fading(Instant::now()),
                State::Connecting | State::Connected => {}
                State::Refused(_) | State::Reconnecting { .. } | State::Closed => {
                    *self = Self::Gone
                }
            },
            Self::Fading(since) if since.elapsed() >= FADE => *self = Self::Gone,
            Self::Fading(_) | Self::Gone => {}
        }
    }

    /// The splash as it draws now, over the whole window, or nothing
    /// once it is gone. A fading splash asks for the next frame, so the
    /// fade runs without input.
    pub fn show(&self, ui: &mut egui::Ui) {
        let opacity = match self {
            Self::Showing => 1.0,
            Self::Fading(since) => {
                ui.ctx().request_repaint_after(FADE / 30);
                1.0 - (since.elapsed().as_secs_f32() / FADE.as_secs_f32()).min(1.0)
            }
            Self::Gone => return,
        };
        let palette = crate::theme::Palette::active_of(ui.ctx());
        // An area sizes to its content unless told otherwise, which
        // drew the splash as a small box in the window's corner. Pin
        // it to the screen so its fill really covers the window.
        let screen = ui.ctx().content_rect();
        let area = egui::Area::new(egui::Id::new("splash"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min);
        let _ = area.show(ui.ctx(), |ui| {
            ui.set_min_size(screen.size());
            egui::Frame::NONE.fill(palette.bg).show(ui, |ui| {
                ui.set_min_size(screen.size());
                ui.set_opacity(opacity);
                ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                    ui.add_space(screen.height() / 2.0 - 60.0);
                    crate::ui::mark::mark(ui, MARK);
                    ui.add_space(14.0);
                    ui.label(
                        egui::RichText::new("pure")
                            .size(WORDMARK)
                            .color(palette.ink_strong)
                            .family(crate::theme::family(crate::theme::FAMILY_BODY_EXTRABOLD)),
                    );
                    ui.label(
                        egui::RichText::new(crate::i18n::t("Connecting\u{2026}"))
                            .size(FS_SM)
                            .color(palette.muted),
                    );
                });
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::Splash;
    use crate::transport::State;

    #[test]
    fn the_splash_fades_once_the_handshake_answers() {
        let mut splash = Splash::Showing;
        splash.follow(&State::Connecting, false);
        assert_eq!(splash, Splash::Showing);
        splash.follow(&State::Connected, false);
        assert_eq!(splash, Splash::Showing, "up but not answered yet");
        splash.follow(&State::Connected, true);
        assert!(matches!(splash, Splash::Fading(_)));
    }

    #[test]
    fn a_failed_first_attempt_takes_the_splash_down() {
        for state in [
            State::Refused("bad token".into()),
            State::Reconnecting {
                attempt: 1,
                delay: Duration::from_secs(1),
            },
            State::Closed,
        ] {
            let mut splash = Splash::Showing;
            splash.follow(&state, false);
            assert_eq!(splash, Splash::Gone, "{state:?}");
        }
    }
}

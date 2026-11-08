//! The nekomata mark: pure's shadow cat with lantern eyes, drawn from
//! the brand SVG that matches the active theme, and the working
//! indicator, where the lantern circles the cat.

use std::f32::consts::TAU;
use std::time::Duration;

use web_time::Instant;

use crate::assets::{MARK_DAWN, MARK_SHADOW};
use crate::icons::PaintSnapped as _;

/// The art's width over its height, from its 240 by 228 view box.
pub const ASPECT: f32 = 240.0 / 228.0;

/// The working cat's view box: the art plus room for its halo.
const WORKING_VIEW: (f32, f32, f32, f32) = (-20.0, -14.0, 280.0, 268.0);
/// The cat's outline in the brand art's units, shared with the files
/// in `assets/brand/`.
const EARS: &str = "M42 40 Q42 26 54 32 L124 84 L58 138 Z \
                    M214 40 Q214 26 202 32 L132 84 L198 138 Z";
const HEAD: &str = "M20 150 C20 98 66 82 128 82 C190 82 236 98 236 150 \
                    C236 202 190 232 128 232 C66 232 20 202 20 150 Z";
/// The halo's stroke, wide enough to read at 14px.
const HALO_W: f32 = 56.0;
/// One trip of the lantern round the cat.
const ORBIT: Duration = Duration::from_millis(2200);
/// The positions one orbit steps through, each rasterized once.
const ORBIT_STEPS: u32 = 132;

/// The mark's size at `height` pixels tall.
#[must_use]
pub fn size(height: f32) -> egui::Vec2 {
    egui::vec2(height * ASPECT, height)
}

/// Paints the mark filling `rect`, in the art for the context's theme.
pub fn paint(ui: &egui::Ui, rect: egui::Rect) {
    let (uri, bytes) = if ui.ctx().theme() == egui::Theme::Dark {
        ("bytes://pure-mark-shadow.svg", MARK_SHADOW)
    } else {
        ("bytes://pure-mark-dawn.svg", MARK_DAWN)
    };
    egui::Image::new(egui::ImageSource::Bytes {
        uri: uri.into(),
        bytes: bytes.into(),
    })
    .fit_to_exact_size(rect.size())
    .paint_snapped(ui, rect);
}

/// Allocates the mark `height` pixels tall in the layout and paints it.
pub fn mark(ui: &mut egui::Ui, height: f32) {
    let (rect, _) = ui.allocate_exact_size(size(height), egui::Sense::hover());
    paint(ui, rect);
}

/// The working cat as SVG: narrowed eyes, and a lantern halo lit on
/// the side the lantern stands, `turn` of the way round from the top.
fn working_svg(turn: f32, dark: bool) -> String {
    let (body, rim) = if dark {
        ("#1c1924", "#f2a65a")
    } else {
        ("#15121c", "#e08a3a")
    };
    let angle = turn * TAU - TAU / 4.0;
    let (sin, cos) = angle.sin_cos();
    let (cx, cy, r) = (128.0, 132.0, 130.0);
    let eyes: String = [(70.0_f32, true), (156.0, false)]
        .into_iter()
        .map(|(x, left)| {
            let (outer, inner) = if left { (x, x + 30.0) } else { (x + 30.0, x) };
            format!(
                "<path d=\"M{outer} 138 L{inner} 150 L{inner} 162 Q{inner} 176 {mid} 176 \
                 Q{outer} 176 {outer} 162 Z\" fill=\"#f2a65a\" stroke=\"#f2a65a\" \
                 stroke-width=\"8\" stroke-linejoin=\"round\"/>",
                mid = x + 15.0,
            )
        })
        .collect();
    let (vx, vy, vw, vh) = WORKING_VIEW;
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{vx} {vy} {vw} {vh}\">\
         <defs><linearGradient id=\"h\" gradientUnits=\"userSpaceOnUse\" \
         x1=\"{x1:.1}\" y1=\"{y1:.1}\" x2=\"{x2:.1}\" y2=\"{y2:.1}\">\
         <stop offset=\"0.5\" stop-color=\"{rim}\" stop-opacity=\"0\"/>\
         <stop offset=\"1\" stop-color=\"{rim}\"/></linearGradient></defs>\
         <g fill=\"none\" stroke=\"url(#h)\" stroke-width=\"{HALO_W}\" stroke-linejoin=\"round\">\
         <path d=\"{EARS}\"/><path d=\"{HEAD}\"/></g>\
         <g fill=\"{body}\" stroke=\"{body}\" stroke-width=\"8\" stroke-linejoin=\"round\">\
         <path d=\"{EARS}\"/><path d=\"{HEAD}\"/></g>{eyes}</svg>",
        x1 = cx - r * cos,
        y1 = cy - r * sin,
        x2 = cx + r * cos,
        y2 = cy + r * sin,
    )
}

/// The working indicator centered in `rect`, as wide as `rect`: the
/// cat with the lantern circling it on the clock `origin` started.
/// Asks for every frame while it turns.
pub fn working(ui: &egui::Ui, rect: egui::Rect, origin: Instant) {
    ui.ctx().request_repaint();
    let elapsed = origin.elapsed().as_secs_f32() % ORBIT.as_secs_f32();
    let step = (elapsed / ORBIT.as_secs_f32() * ORBIT_STEPS as f32) as u32 % ORBIT_STEPS;
    let dark = ui.ctx().theme() == egui::Theme::Dark;
    let uri = format!(
        "bytes://pure-working-{}-{step}.svg",
        if dark { "shadow" } else { "dawn" }
    );
    let svg = working_svg(step as f32 / ORBIT_STEPS as f32, dark);
    let (_, _, vw, vh) = WORKING_VIEW;
    let at = egui::Rect::from_center_size(
        rect.center(),
        egui::vec2(rect.width(), rect.width() * vh / vw),
    );
    egui::Image::from_bytes(uri, svg.into_bytes())
        .fit_to_exact_size(at.size())
        .paint_snapped(ui, at);
}

#[cfg(test)]
mod tests {
    use super::working_svg;

    #[test]
    fn the_halo_turns_with_the_lantern() {
        let top = working_svg(0.0, true);
        let half = working_svg(0.5, true);
        assert_ne!(top, half, "a half turn moves the halo");
        assert_eq!(top, working_svg(1.0, true), "a full turn comes back");
    }

    #[test]
    fn each_theme_draws_its_own_cat() {
        assert!(working_svg(0.0, true).contains("#1c1924"));
        assert!(working_svg(0.0, false).contains("#15121c"));
    }
}

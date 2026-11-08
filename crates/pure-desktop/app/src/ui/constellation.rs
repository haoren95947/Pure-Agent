//! The swarm constellation: one star per worker, lit by its state and
//! joined in launch order back to the lead.
//!
//! A custom painted field. Stars sit in one to four rows by worker
//! count, with a small jitter seeded by each item so a layout never
//! moves between frames. Running stars twinkle on a clock the caller
//! keeps, so a repaint from a state change continues the animation
//! instead of restarting it. The caller drives the frames with
//! [`egui::Context::request_repaint`], which is what keeps the
//! idle cost at zero: no live member, no scheduled repaint.

use std::time::Duration;
use web_time::Instant;

use crate::theme::Palette;
use crate::ui::agents::Phase;

/// One worker's star.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Star {
    /// The child session id a click opens.
    pub id: String,
    /// The swarm item, which seeds the star's jitter.
    pub item: String,
    /// Where the worker stands.
    pub phase: Phase,
}

/// The star core radius.
const CORE_R: f32 = 4.2;
/// The twinkle halo radius at rest.
const HALO_R: f32 = 10.0;
/// The lead star radius.
const LEAD_R: f32 = 6.0;
/// How near a click must land to a star to open it.
const HIT_R: f32 = 10.0;
/// The lead star's inset from the field's left edge.
const LEAD_X: f32 = 18.0;
/// The row band the stars start in from the field's left edge.
const BAND_X: f32 = 64.0;
/// The width the star band leaves at the right for the jitter.
const BAND_MARGIN: f32 = 96.0;

/// The field height for `n` workers: 26px plus 18px per row.
#[must_use]
pub fn field_height(n: usize) -> f32 {
    26.0 + rows(n) as f32 * 18.0
}

/// The row count `n` workers lay out in: one row to eight workers,
/// then two, three and four.
fn rows(n: usize) -> usize {
    match n {
        0..=8 => 1,
        9..=24 => 2,
        25..=64 => 3,
        _ => 4,
    }
}

/// Where each star sits in a field `width` wide, in launch order, and
/// where the lead sits.
#[must_use]
pub fn layout(stars: &[Star], width: f32) -> (egui::Pos2, Vec<egui::Pos2>) {
    let n = stars.len();
    let rows = rows(n);
    let cols = n.div_ceil(rows).max(1);
    let height = field_height(n);
    let span = (width - BAND_MARGIN).max(0.0);
    let points = stars
        .iter()
        .enumerate()
        .map(|(i, star)| {
            let jx = (i64::from(design_hash(&format!("{}x", star.item)) % 21) - 10) as f32;
            let jy = (i64::from(design_hash(&format!("{}y", star.item)) % 13) - 6) as f32;
            let col = i / rows;
            let row = i % rows;
            let x = BAND_X
                + if cols > 1 {
                    col as f32 * span / (cols - 1) as f32
                } else {
                    span / 2.0
                }
                + jx;
            let y =
                (row as f32 + 0.5) * height / rows as f32 + if rows > 1 { jy } else { jy / 2.0 };
            egui::pos2(x, y)
        })
        .collect();
    (egui::pos2(LEAD_X, height / 2.0), points)
}

/// The color a star's core carries.
fn core_color(phase: Phase, palette: &Palette) -> egui::Color32 {
    phase.color(palette)
}

/// One twinkle's scale and opacity at `t`: 0.6 to 1.25 and back, 0.1
/// to 0.3 and back, over `period`.
#[must_use]
pub fn twinkle(t: Duration, period: Duration) -> (f32, f32) {
    let phase = (t.as_secs_f32() % period.as_secs_f32()) / period.as_secs_f32();
    let wave = (1.0 - (phase * std::f32::consts::TAU).cos()) / 2.0;
    (0.6 + 0.65 * wave, 0.1 + 0.2 * wave)
}

/// What the constellation asked of the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstellationEvent {
    /// The star `id`'s worker was opened.
    Open(String),
}

/// Draws the constellation of `stars` in `rect`, returning what its
/// stars asked for and whether it wants another frame.
///
/// `origin` is the card's clock, so the twinkle continues across
/// repaints. `selected` marks the worker the workbench shows. The
/// painter is the one the caller allocated for the field, so the
/// stars land in the rect the caller reserved.
pub fn show(
    ui: &mut egui::Ui,
    stars: &[Star],
    selected: Option<&str>,
    origin: Instant,
) -> (Vec<ConstellationEvent>, bool) {
    let palette = Palette::active_of(ui.ctx());
    let height = field_height(stars.len());
    let (rect, response) = ui.allocate_at_least(
        egui::vec2(ui.available_width(), height),
        egui::Sense::click(),
    );
    let painter = ui.painter_at(rect);
    let mut events = Vec::new();

    if response.clicked()
        && let Some(at) = response.interact_pointer_pos()
    {
        let (_, points) = layout(stars, rect.width());
        let hit = points.iter().enumerate().position(|(_, p)| {
            let at = at - rect.min;
            let (dx, dy) = (at.x - p.x, at.y - p.y);
            dx * dx + dy * dy <= HIT_R * HIT_R
        });
        if let Some(i) = hit {
            events.push(ConstellationEvent::Open(stars[i].id.clone()));
        }
    }

    let (lead, points) = layout(stars, rect.width());
    let at = |p: egui::Pos2| rect.min + p.to_vec2();
    let now = origin.elapsed();
    let mut animate = false;
    let phases: Vec<Phase> = stars.iter().map(|star| star.phase).collect();
    let per_row = rows(stars.len());
    for (i, p) in points.iter().enumerate() {
        let (from, from_phase) = if i < per_row {
            (lead, Phase::Done)
        } else {
            (points[i - per_row], phases[i - per_row])
        };
        let (color, dashed) = match (from_phase, phases[i]) {
            (_, Phase::Running | Phase::Paused) => (faded(palette.done, 0.55), true),
            (Phase::Done, Phase::Done) => (faded(palette.ok, 0.35), false),
            (_, Phase::Failed) => (faded(palette.danger, 0.4), false),
            _ => (palette.line, false),
        };
        if dashed {
            animate = true;
            // A dashed line is a short segment per dash, so the field
            // animates it as the worker runs.
            let step = 7.0;
            let len = (at(*p) - at(from)).length();
            let mut walked = 0.0;
            while walked < len {
                let a = at(from) + (at(*p) - at(from)) * (walked / len);
                let b = at(from) + (at(*p) - at(from)) * ((walked + 3.0) / len).min(1.0);
                painter.line_segment([a, b], (1.0, color));
                walked += step;
            }
        } else {
            painter.line_segment([at(from), at(*p)], (1.0, color));
        }
    }
    circle(&painter, at(lead), LEAD_R, palette.accent);
    for (i, (star, p)) in stars.iter().zip(&points).enumerate() {
        let center = at(*p);
        let period = match star.phase {
            Phase::Running => Some(Duration::from_millis(1600)),
            Phase::Paused => Some(Duration::from_millis(2600)),
            _ => None,
        };
        if let Some(period) = period {
            animate = true;
            let offset = Duration::from_millis((i as u64 * 263) % 1600);
            let (scale, alpha) = twinkle(now + offset, period);
            circle(
                &painter,
                center,
                HALO_R * scale,
                faded(core_color(star.phase, palette), alpha),
            );
        }
        let core = core_color(star.phase, palette);
        let picked = selected == Some(star.id.as_str());
        let stroke = if picked {
            (palette.ink_strong, 2.2)
        } else {
            (core, 1.4)
        };
        let fill = if matches!(star.phase, Phase::Cancelled) {
            palette.faint
        } else {
            core
        };
        circle_stroked(&painter, center, CORE_R, fill, stroke);
    }
    (events, animate)
}

/// Paints a filled circle.
fn circle(painter: &egui::Painter, center: egui::Pos2, r: f32, fill: egui::Color32) {
    painter.circle_filled(center, r, fill);
}

/// Paints a filled circle with a stroke.
fn circle_stroked(
    painter: &egui::Painter,
    center: egui::Pos2,
    r: f32,
    fill: egui::Color32,
    stroke: (egui::Color32, f32),
) {
    painter.circle(center, r, fill, egui::Stroke::new(stroke.1, stroke.0));
}

/// A color at `alpha` over the field, the way the design fades a
/// connector or a halo.
fn faded(color: egui::Color32, alpha: f32) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(
        color.r(),
        color.g(),
        color.b(),
        (color.a() as f32 * alpha) as u8,
    )
}

/// The design's string hash, so a star's jitter agrees with the web
/// client's.
fn design_hash(text: &str) -> u32 {
    let mut x: i32 = 0;
    for unit in text.encode_utf16() {
        x = x.wrapping_mul(31).wrapping_add(i32::from(unit));
    }
    x.unsigned_abs()
}

/// A star for the tests: `id`, `item` and the phase it stands in.
pub fn star(id: &str, item: &str, phase: Phase) -> Star {
    Star {
        id: id.to_owned(),
        item: item.to_owned(),
        phase,
    }
}

#[cfg(test)]
mod tests {
    use super::{Star, field_height, layout, rows, star, twinkle};
    use crate::ui::agents::Phase;
    use std::time::Duration;

    fn stars(n: usize) -> Vec<Star> {
        (0..n)
            .map(|i| star(&format!("c{i}"), &format!("item-{i}"), Phase::Running))
            .collect()
    }

    #[test]
    fn rows_grow_with_the_worker_count_like_the_design() {
        assert_eq!(
            (rows(8), rows(9), rows(24), rows(25), rows(65)),
            (1, 2, 2, 3, 4)
        );
        assert_eq!(field_height(4), 44.0);
        assert_eq!(field_height(30), 80.0);
    }

    #[test]
    fn a_layout_is_the_same_every_frame_and_keeps_launch_order_left_to_right() {
        let field = stars(6);
        let (lead, first) = layout(&field, 720.0);
        let (_, again) = layout(&field, 720.0);
        assert_eq!(first, again, "the jitter is seeded, not random");
        assert_eq!(lead.x, 18.0);
        assert!(first.windows(2).all(|pair| pair[0].x < pair[1].x));
    }

    #[test]
    fn a_twinkle_breathes_between_its_bounds() {
        let period = Duration::from_millis(1600);
        let (rest, faint) = twinkle(Duration::ZERO, period);
        let (peak, bright) = twinkle(Duration::from_millis(800), period);
        assert!((rest - 0.6).abs() < 1e-4 && (faint - 0.1).abs() < 1e-4);
        assert!((peak - 1.25).abs() < 1e-4 && (bright - 0.3).abs() < 1e-4);
    }

    #[test]
    fn the_lead_sits_before_the_first_star_band() {
        let (lead, points) = layout(&stars(4), 720.0);
        assert!(
            points.iter().all(|p| p.x > lead.x),
            "every star sits right of the lead"
        );
    }
}

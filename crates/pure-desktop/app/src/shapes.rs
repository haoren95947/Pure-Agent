//! Borders and shadows drawn the way GPUI draws them.
//!
//! epaint widens the corners of any stroked rectangle by 0.4 px and
//! fades a shadow linearly across its blur width, where GPUI keeps the
//! radius it was given and blurs with a Gaussian. A plugin rewrites
//! those shapes in each frame's output: stroked rounded rectangles
//! become closed paths, which take the same fill and stroke route
//! through the tessellator, and blurred rectangles become meshes
//! shaded with GPUI's shadow integral.

use egui::emath::GuiRounding as _;
use egui::epaint::{
    CornerRadius, CornerRadiusF32, Mesh, PathShape, PathStroke, Pos2, Rect, RectShape, Shape,
    StrokeKind, Vec2, tessellator::path::rounded_rectangle,
};

use crate::theme::ShadowLayer;

/// Registers the plugin on `ctx`.
pub fn install(ctx: &egui::Context) {
    ctx.add_plugin(GpuiShapes);
}

/// The layers of a drop shadow under a box at `rect` with `radius`
/// corners, bottom layer first.
pub fn shadow(rect: Rect, radius: CornerRadius, layers: &[ShadowLayer]) -> Shape {
    Shape::Vec(
        layers
            .iter()
            .map(|layer| {
                let bounds = rect.translate(layer.offset).expand(layer.spread);
                Shape::Rect(
                    RectShape::filled(bounds, radius, layer.color).with_blur_width(layer.blur),
                )
            })
            .collect(),
    )
}

struct GpuiShapes;

impl egui::Plugin for GpuiShapes {
    fn debug_name(&self) -> &'static str {
        "pure gpui shapes"
    }

    fn output_hook(&mut self, _ctx: &egui::Context, output: &mut egui::FullOutput) {
        let ppp = output.pixels_per_point;
        for clipped in &mut output.shapes {
            rewrite(&mut clipped.shape, ppp);
        }
    }
}

fn rewrite(shape: &mut Shape, ppp: f32) {
    match shape {
        Shape::Vec(shapes) => shapes.iter_mut().for_each(|shape| rewrite(shape, ppp)),
        Shape::Rect(rect) => {
            if let Some(path) = exact_path(rect, ppp) {
                *shape = Shape::Path(path);
            } else if let Some(mesh) = gaussian(rect, ppp) {
                *shape = Shape::Mesh(mesh.into());
            }
        }
        _ => {}
    }
}

/// The path epaint would tessellate for `shape`, minus its corner
/// tweak, or `None` where the rectangle takes another route.
fn exact_path(shape: &RectShape, ppp: f32) -> Option<PathShape> {
    let stroke = shape.stroke;
    if stroke.is_empty()
        || shape.corner_radius == CornerRadius::ZERO
        || shape.brush.is_some()
        || shape.angle != 0.0
        || shape.blur_width != 0.0
        || shape.stroke_kind == StrokeKind::Middle
    {
        return None;
    }
    let mut rect = shape.rect;
    if shape.round_to_pixels.unwrap_or(true) {
        rect = rect.round_to_pixels(ppp);
    }
    let mut radius = CornerRadiusF32::from(shape.corner_radius);
    if shape.stroke_kind == StrokeKind::Outside {
        rect = rect.expand(stroke.width);
        radius += stroke.width;
    }
    // Below this epaint draws the stroke as a solid fill.
    if rect.size().min_elem() <= 2.0 * stroke.width + 0.5 / ppp {
        return None;
    }
    let least = stroke.width + 0.1;
    for (corner, original) in [
        (&mut radius.nw, shape.corner_radius.nw),
        (&mut radius.ne, shape.corner_radius.ne),
        (&mut radius.sw, shape.corner_radius.sw),
        (&mut radius.se, shape.corner_radius.se),
    ] {
        *corner = if original == 0 {
            0.0
        } else {
            corner.max(least)
        };
    }
    let mut points = Vec::new();
    rounded_rectangle(&mut points, rect, radius);
    Some(PathShape {
        points,
        closed: true,
        fill: shape.fill,
        stroke: PathStroke::from(stroke).with_kind(StrokeKind::Inside),
    })
}

/// A blurred rectangle as GPUI shades a drop shadow: `blur_width` is
/// the Gaussian's sigma and the tail runs three sigmas out. The mesh
/// samples the shader's integral on a grid that is dense only where
/// the shadow fades.
fn gaussian(shape: &RectShape, ppp: f32) -> Option<Mesh> {
    let sigma = shape.blur_width;
    if sigma <= 0.0 || !shape.stroke.is_empty() || shape.brush.is_some() || shape.angle != 0.0 {
        return None;
    }
    let bounds = Rect::from_min_max(
        (shape.rect.min.to_vec2() * ppp).floor().to_pos2() / ppp,
        (shape.rect.max.to_vec2() * ppp).ceil().to_pos2() / ppp,
    );
    let mut mesh = Mesh::default();
    if !bounds.is_positive() {
        return Some(mesh);
    }
    let radius = CornerRadiusF32::from(shape.corner_radius);
    let step = (sigma / 6.0).clamp(0.5, 8.0);
    let xs = axis(bounds.min.x, bounds.max.x, sigma, step);
    let ys = axis(bounds.min.y, bounds.max.y, sigma, step);
    let center = bounds.center();
    let half = bounds.size() / 2.0;
    for &y in &ys {
        for &x in &xs {
            let point = Pos2::new(x, y);
            let alpha = shadow_alpha(point - center, half, radius, sigma).clamp(0.0, 1.0);
            mesh.colored_vertex(point, shape.fill.gamma_multiply(alpha));
        }
    }
    let row = xs.len() as u32;
    for j in 0..ys.len() as u32 - 1 {
        for i in 0..row - 1 {
            let at = j * row + i;
            mesh.add_triangle(at, at + 1, at + row);
            mesh.add_triangle(at + 1, at + row + 1, at + row);
        }
    }
    Some(mesh)
}

/// Sample positions along one axis: every `step` through the fade
/// around each edge, and nothing between where the shadow is flat.
fn axis(low: f32, high: f32, sigma: f32, step: f32) -> Vec<f32> {
    let reach = 3.0 * sigma;
    let mut out = Vec::new();
    let mut run = |from: f32, to: f32| {
        let n = ((to - from) / step).ceil().max(1.0) as usize;
        let first = usize::from(!out.is_empty());
        out.extend((first..=n).map(|i| from + (to - from) * i as f32 / n as f32));
    };
    if low + reach >= high - reach {
        run(low - reach, high + reach);
    } else {
        run(low - reach, low + reach);
        run(high - reach, high + reach);
    }
    out
}

/// GPUI's `fs_shadow`: the box's coverage blurred along x in closed
/// form, then integrated along y with four Gaussian-weighted samples.
fn shadow_alpha(p: Vec2, half: Vec2, radius: CornerRadiusF32, sigma: f32) -> f32 {
    let corner = match (p.x < 0.0, p.y < 0.0) {
        (true, true) => radius.nw,
        (true, false) => radius.sw,
        (false, true) => radius.ne,
        (false, false) => radius.se,
    };
    let low = p.y - half.y;
    let high = p.y + half.y;
    let start = (-3.0 * sigma).max(low).min(high);
    let end = (3.0 * sigma).max(low).min(high);
    let step = (end - start) / 4.0;
    let mut y = start + step * 0.5;
    let mut alpha = 0.0;
    for _ in 0..4 {
        alpha += blur_along_x(p.x, p.y - y, sigma, corner, half) * gauss(y, sigma) * step;
        y += step;
    }
    alpha
}

fn blur_along_x(x: f32, y: f32, sigma: f32, corner: f32, half: Vec2) -> f32 {
    let delta = (half.y - corner - y.abs()).min(0.0);
    let curved = half.x - corner + (corner * corner - delta * delta).max(0.0).sqrt();
    let scale = 0.5_f32.sqrt() / sigma;
    let integral = |v: f32| 0.5 + 0.5 * erf(v * scale);
    integral(x + curved) - integral(x - curved)
}

fn gauss(x: f32, sigma: f32) -> f32 {
    (-(x * x) / (2.0 * sigma * sigma)).exp() / ((2.0 * std::f32::consts::PI).sqrt() * sigma)
}

/// The shader's polynomial approximation of the error function.
fn erf(v: f32) -> f32 {
    let s = if v == 0.0 { 0.0 } else { v.signum() };
    let a = v.abs();
    let r1 = 1.0 + (0.278_393 + (0.230_389 + (0.000_972 + 0.078_108 * a) * a) * a) * a;
    let r2 = r1 * r1;
    s - s / (r2 * r2)
}

#[cfg(test)]
mod tests {
    use super::{CornerRadiusF32, Vec2, shadow_alpha};

    #[test]
    fn a_wide_shadow_is_half_strength_on_its_edge_and_full_inside() {
        let half = Vec2::new(480.0, 360.0);
        let radius = CornerRadiusF32::same(16.0);
        let inside = shadow_alpha(Vec2::ZERO, half, radius, 40.0);
        let edge = shadow_alpha(Vec2::new(480.0, 0.0), half, radius, 40.0);
        let far = shadow_alpha(Vec2::new(480.0 + 120.0, 0.0), half, radius, 40.0);
        assert!((inside - 1.0).abs() < 0.01, "{inside}");
        assert!((edge - 0.5).abs() < 0.01, "{edge}");
        assert!(far < 0.01, "{far}");
    }
}

//! Soft UI painting primitives: CSS `box-shadow` (outer and inset) on rounded boxes.

use vello_cpu::RenderContext;
use vello_cpu::kurbo::{BezPath, Rect, RoundedRect, Shape, Vec2};
use vello_cpu::peniko::Fill;

use crate::theme::{Color, Shadow};

/// Path tolerance in logical pixels (the context's transform scales it)
pub const TOLERANCE: f64 = 0.05;

/// vello_cpu cuts a blurred box off at 2.5 σ outside the box
const BLUR_REACH: f64 = 2.5;

pub fn rounded(rect: Rect, radius: f64) -> BezPath {
    RoundedRect::from_rect(rect, radius).to_path(TOLERANCE)
}

fn visible(c: Color, alpha: f32) -> Option<Color> {
    let c = c.multiply_alpha(alpha);
    (c.components[3] > 1.0 / 512.0).then_some(c)
}

/// The outer shadows of `shadows` under a box. Like CSS, the first shadow is on top;
/// the box itself must be painted afterwards (it covers the part of the shadow under it).
/// `alpha` fades the whole set (used to cross-fade between shadow sets).
pub fn outer_shadows(
    ctx: &mut RenderContext,
    rect: Rect,
    radius: f64,
    shadows: &[Shadow],
    alpha: f32,
) {
    for s in shadows.iter().rev().filter(|s| !s.inset) {
        let Some(color) = visible(s.color, alpha) else {
            continue;
        };
        ctx.set_paint(color);
        // spread grows the box and its corner radius
        let r = (rect + Vec2::new(s.x, s.y)).inflate(s.spread, s.spread);
        let radius = (radius + s.spread).max(0.0);
        if s.blur > 0.0 {
            // CSS blur radius = 2 σ
            ctx.fill_blurred_rounded_rect(&r, radius as f32, (s.blur / 2.0) as f32, false);
        } else {
            ctx.fill_path(&rounded(r, radius));
        }
    }
}

/// The inset shadows of `shadows`, clipped to the box (paint them after the box).
/// Spread shrinks the hole (a 1px spread without offset or blur is an inner edge).
pub fn inset_shadows(
    ctx: &mut RenderContext,
    rect: Rect,
    radius: f64,
    shadows: &[Shadow],
    alpha: f32,
) {
    let shadows: Vec<(Shadow, Color)> = shadows
        .iter()
        .rev()
        .filter(|s| s.inset)
        .filter_map(|s| visible(s.color, alpha).map(|c| (*s, c)))
        .collect();
    if shadows.is_empty() {
        return;
    }
    ctx.push_clip_layer(&rounded(rect, radius));
    for (s, color) in shadows {
        ctx.set_paint(color);
        let hole = (rect + Vec2::new(s.x, s.y)).inflate(-s.spread, -s.spread);
        let radius = (radius - s.spread).max(0.0);
        let sigma = s.blur / 2.0;
        // Everything outside the shifted box is solid shadow. With blur, the inverted
        // blurred box covers its own bounds (box + 2.5 σ); outside of that we fill.
        let mut path = rect.inflate(1.0, 1.0).to_path(TOLERANCE);
        if sigma > 0.0 {
            path.extend(
                hole.inflate(BLUR_REACH * sigma, BLUR_REACH * sigma)
                    .to_path(TOLERANCE),
            );
        } else {
            path.extend(rounded(hole, radius));
        }
        ctx.set_fill_rule(Fill::EvenOdd);
        ctx.fill_path(&path);
        ctx.set_fill_rule(Fill::NonZero);
        if sigma > 0.0 {
            ctx.fill_blurred_rounded_rect(&hole, radius as f32, sigma as f32, true);
        }
    }
    ctx.pop_layer();
}

/// Interpolates two shadow lists of the same shape, as a CSS transition does
pub fn lerp_shadows<const N: usize>(a: &[Shadow; N], b: &[Shadow; N], t: f64) -> [Shadow; N] {
    let tf = t as f32;
    std::array::from_fn(|i| {
        let (a, b) = (a[i], b[i]);
        Shadow {
            inset: a.inset,
            x: a.x + (b.x - a.x) * t,
            y: a.y + (b.y - a.y) * t,
            blur: a.blur + (b.blur - a.blur) * t,
            spread: a.spread + (b.spread - a.spread) * t,
            color: a.color.lerp_rect(b.color, tf),
        }
    })
}

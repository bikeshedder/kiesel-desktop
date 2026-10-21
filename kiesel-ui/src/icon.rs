//! Icons: Lucide's outlines (ISC licence, https://lucide.dev), stroked like
//! Lucide's SVGs – a 24-unit box, round caps and joins.

use vello_cpu::RenderContext;
use vello_cpu::kurbo::{Affine, BezPath, Cap, Circle, Join, Point, Rect, Shape, Stroke};

use crate::paint::TOLERANCE;
use crate::theme::Color;

/// One part of an icon, in Lucide's 24×24 box
#[derive(Clone, Copy, Debug)]
pub enum Part {
    Path(&'static str),
    Circle { cx: f64, cy: f64, r: f64 },
}

#[derive(Clone, Copy, Debug)]
pub struct Icon(pub &'static [Part]);

/// Lucide `sun`
pub const SUN: Icon = Icon(&[
    Part::Circle {
        cx: 12.0,
        cy: 12.0,
        r: 4.0,
    },
    Part::Path("M12 2v2"),
    Part::Path("M12 20v2"),
    Part::Path("m4.93 4.93 1.41 1.41"),
    Part::Path("m17.66 17.66 1.41 1.41"),
    Part::Path("M2 12h2"),
    Part::Path("M20 12h2"),
    Part::Path("m6.34 17.66-1.41 1.41"),
    Part::Path("m19.07 4.93-1.41 1.41"),
]);

/// Lucide `moon-star`
pub const MOON_STAR: Icon = Icon(&[
    Part::Path("M18 5h4"),
    Part::Path("M20 3v4"),
    Part::Path(
        "M20.985 12.486a9 9 0 1 1-9.473-9.472c.405-.022.617.46.402.803a6 6 0 0 0 8.268 8.268c.344-.215.825-.004.803.401",
    ),
]);

impl Icon {
    /// The outline in the 24-unit box
    pub fn path(&self) -> BezPath {
        let mut out = BezPath::new();
        for part in self.0 {
            match *part {
                Part::Path(d) => {
                    // the data is ours and known to parse; a bad path would only lose that stroke
                    if let Ok(p) = BezPath::from_svg(d) {
                        out.extend(p);
                    }
                }
                Part::Circle { cx, cy, r } => {
                    out.extend(Circle::new((cx, cy), r).path_elements(TOLERANCE))
                }
            }
        }
        out
    }

    /// Strokes the icon into `rect` (square), `stroke` in the icon's 24-unit box,
    /// turned by `degrees` and scaled by `scale` about its centre
    pub fn draw(
        &self,
        ctx: &mut RenderContext,
        rect: Rect,
        stroke: f64,
        color: Color,
        degrees: f64,
        scale: f64,
    ) {
        let base = *ctx.transform();
        let c = rect.center();
        let fit = rect.width() / 24.0;
        let t = Affine::translate(c.to_vec2())
            * Affine::rotate(degrees.to_radians())
            * Affine::scale(fit * scale)
            * Affine::translate(-Point::new(12.0, 12.0).to_vec2());
        ctx.set_transform(base * t);
        ctx.set_stroke(
            Stroke::new(stroke)
                .with_caps(Cap::Round)
                .with_join(Join::Round),
        );
        ctx.set_paint(color);
        ctx.stroke_path(&self.path());
        ctx.set_transform(base);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lucide_paths_parse_and_fit_the_box() {
        for icon in [SUN, MOON_STAR] {
            for part in icon.0 {
                if let Part::Path(d) = part {
                    assert!(BezPath::from_svg(d).is_ok(), "{d}");
                }
            }
            let b = icon.path().bounding_box();
            assert!(
                b.x0 >= 1.9 && b.y0 >= 1.9 && b.x1 <= 22.1 && b.y1 <= 22.1,
                "{b:?}"
            );
        }
    }
}

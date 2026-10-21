//! Text through parley (shaping, system fonts and fallback) drawn by vello_cpu.

use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontWeight, Layout, LayoutContext,
    PositionedLayoutItem, StyleProperty,
};
use vello_cpu::kurbo::{Affine, Point};
use vello_cpu::{Glyph, RenderContext, Resources};

use crate::theme::Color;

/// Font database and layout scratch space – one per application
pub struct Text {
    fonts: FontContext,
    layouts: LayoutContext<()>,
}

pub struct TextStyle<'a> {
    pub family: &'a str,
    pub size: f32,
    pub weight: f32,
    /// In em, like CSS `letter-spacing: 0.01em`
    pub letter_spacing: f32,
}

impl Default for Text {
    fn default() -> Self {
        Self {
            fonts: FontContext::new(),
            layouts: LayoutContext::new(),
        }
    }
}

impl Text {
    /// One line, no wrapping (`white-space: nowrap`)
    pub fn line(&mut self, text: &str, style: &TextStyle<'_>) -> Layout<()> {
        let mut b = self
            .layouts
            .ranged_builder(&mut self.fonts, text, 1.0, true);
        b.push_default(StyleProperty::FontFamily(FontFamily::Source(
            style.family.into(),
        )));
        b.push_default(StyleProperty::FontSize(style.size));
        b.push_default(StyleProperty::FontWeight(FontWeight::new(style.weight)));
        b.push_default(StyleProperty::LetterSpacing(
            style.letter_spacing * style.size,
        ));
        let mut layout = b.build(text);
        layout.break_all_lines(None);
        layout.align(Alignment::Start, AlignmentOptions::default());
        layout
    }
}

/// Draws `layout` with its top-left corner at `origin`, on top of the context's transform
pub fn draw(
    ctx: &mut RenderContext,
    res: &mut Resources,
    layout: &Layout<()>,
    origin: Point,
    color: Color,
) {
    let base = *ctx.transform();
    ctx.set_transform(base * Affine::translate(origin.to_vec2()));
    ctx.set_paint(color);
    for line in layout.lines() {
        for item in line.items() {
            if let PositionedLayoutItem::GlyphRun(run) = item {
                let glyphs = run.positioned_glyphs().map(|g| Glyph {
                    id: g.id,
                    x: g.x,
                    y: g.y,
                });
                let r = run.run();
                let coords: Vec<i16> = r.normalized_coords().iter().map(|c| c.to_bits()).collect();
                // a glyph that can't be drawn is skipped, not fatal
                let _ = ctx
                    .glyph_run(res, r.font())
                    .font_size(r.font_size())
                    .normalized_coords(&coords)
                    .fill_glyphs(glyphs);
            }
        }
    }
    ctx.set_transform(base);
}

//! Text through parley (shaping, system fonts and fallback) drawn by vello_cpu.

use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontWeight, Layout, LayoutContext,
    LineHeight, PositionedLayoutItem, StyleProperty,
};
use vello_cpu::kurbo::{Affine, Point};
use vello_cpu::{Glyph, RenderContext, Resources};

use crate::theme::{Color, TextStyle};

/// Font database and layout scratch space – one per application
pub struct Text {
    fonts: FontContext,
    layouts: LayoutContext<()>,
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
    /// For parley's editor, which shapes as it edits
    pub(crate) fn contexts(&mut self) -> (&mut FontContext, &mut LayoutContext<()>) {
        (&mut self.fonts, &mut self.layouts)
    }

    /// Shaped text, not yet broken into lines – see [`wrap`]
    pub fn paragraph(&mut self, text: &str, style: &TextStyle) -> Layout<()> {
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
        b.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(
            style.line_height,
        )));
        b.build(text)
    }

    /// One line, no wrapping (`white-space: nowrap`)
    pub fn line(&mut self, text: &str, style: &TextStyle) -> Layout<()> {
        let mut layout = self.paragraph(text, style);
        wrap(&mut layout, None);
        layout
    }
}

/// Breaks `layout` into lines no wider than `width` (`None`: only at hard breaks)
pub fn wrap(layout: &mut Layout<()>, width: Option<f32>) {
    layout.break_all_lines(width);
    layout.align(Alignment::Start, AlignmentOptions::default());
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

//! `Label` – a paragraph of text in one role of the type scale; it wraps at
//! word boundaries to whatever width layout gives it.

use parley::Layout;
use vello_cpu::kurbo::{Rect, Size};

use crate::text::{self, wrap};
use crate::theme::TextRole;
use crate::widget::{Available, Known, LayoutCx, PaintCx, Widget, WidgetId};

pub struct Label {
    id: WidgetId,
    text: String,
    role: TextRole,
    /// Shaped once; broken into lines again only when the width changes
    layout: Option<Layout<()>>,
    broken_at: Option<f32>,
    bounds: Rect,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            id: WidgetId::next(),
            text: text.into(),
            role: TextRole::Body,
            layout: None,
            broken_at: None,
            bounds: Rect::ZERO,
        }
    }

    pub fn role(mut self, role: TextRole) -> Self {
        self.role = role;
        self
    }

    pub fn title(text: impl Into<String>) -> Self {
        Self::new(text).role(TextRole::Title)
    }

    pub fn caption(text: impl Into<String>) -> Self {
        Self::new(text).role(TextRole::Caption)
    }

    fn layout(&mut self, cx: &mut LayoutCx<'_>) -> &mut Layout<()> {
        let style = cx.theme.text_style(self.role);
        self.layout
            .get_or_insert_with(|| cx.text.paragraph(&self.text, &style))
    }

    /// Breaks the text at `width` unless it already is
    fn wrap_at(&mut self, cx: &mut LayoutCx<'_>, width: f32) -> Size {
        let at = self.broken_at;
        let layout = self.layout(cx);
        if at != Some(width) {
            wrap(layout, Some(width));
        }
        let size = Size::new(f64::from(layout.width()), f64::from(layout.height()));
        self.broken_at = Some(width);
        size
    }
}

impl Widget for Label {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn measure(&mut self, cx: &mut LayoutCx<'_>, known: Known, available: Available) -> Size {
        let widths = self.layout(cx).calculate_content_widths();
        let width = known.width.unwrap_or(match available.width {
            taffy::AvailableSpace::MinContent => widths.min,
            taffy::AvailableSpace::MaxContent => widths.max,
            taffy::AvailableSpace::Definite(w) => w.min(widths.max),
        });
        // the box is as wide as offered (block-like); its height follows the lines
        let size = self.wrap_at(cx, width);
        Size::new(
            f64::from(known.width.unwrap_or(size.width as f32)),
            known.height.map_or(size.height, f64::from),
        )
    }

    fn set_bounds(&mut self, cx: &mut LayoutCx<'_>, bounds: Rect) {
        self.bounds = bounds;
        // break at the final width – measuring may have tried others; the
        // half pixel absorbs taffy's rounding, so a line that fit still fits
        self.wrap_at(cx, bounds.width() as f32 + 0.5);
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn paint(&self, cx: &mut PaintCx<'_>) {
        if let Some(layout) = &self.layout {
            text::draw(
                cx.ctx,
                cx.res,
                layout,
                self.bounds.origin(),
                cx.theme.text_color(self.role),
            );
        }
    }

    fn accessibility(&self) -> Option<accesskit::Node> {
        let mut node = accesskit::Node::new(accesskit::Role::Label);
        node.set_value(self.text.clone());
        node.set_bounds(crate::widget::accessible_rect(self.bounds));
        Some(node)
    }
}

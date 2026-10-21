//! `Row` – children side by side with a gap, centred in its bounds (until taffy arrives).

use vello_cpu::kurbo::{Point, Rect, Size};

use crate::widget::{LayoutCx, Widget, WidgetId};

pub struct Row {
    id: WidgetId,
    children: Vec<Box<dyn Widget>>,
    gap: f64,
    bounds: Rect,
}

impl Row {
    pub fn new(gap: f64) -> Self {
        Self {
            id: WidgetId::next(),
            children: Vec::new(),
            gap,
            bounds: Rect::ZERO,
        }
    }

    pub fn child(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }
}

impl Widget for Row {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut [Box<dyn Widget>] {
        &mut self.children
    }

    fn measure(&mut self, cx: &mut LayoutCx<'_>) -> Size {
        let sizes: Vec<Size> = self.children.iter_mut().map(|c| c.measure(cx)).collect();
        let gaps = self.gap * sizes.len().saturating_sub(1) as f64;
        Size::new(
            sizes.iter().map(|s| s.width).sum::<f64>() + gaps,
            sizes.iter().map(|s| s.height).fold(0.0, f64::max),
        )
    }

    fn place(&mut self, cx: &mut LayoutCx<'_>, bounds: Rect) {
        self.bounds = bounds;
        let natural = self.measure(cx);
        let mut x = (bounds.x0 + (bounds.width() - natural.width) / 2.0).round();
        for child in &mut self.children {
            let size = child.measure(cx);
            let y = (bounds.y0 + (bounds.height() - size.height) / 2.0).round();
            child.place(cx, Rect::from_origin_size(Point::new(x, y), size));
            x += size.width + self.gap;
        }
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }
}

//! `Card` – a self-contained object on a calm tonal surface (`--ksl-card-bg`),
//! no border, no shadow: a column with padding `--size-5` and gap `--size-4`.

use vello_cpu::kurbo::Rect;

use crate::paint;
use crate::theme::{Theme, space};
use crate::widget::{LayoutCx, PaintCx, Widget, WidgetId};

pub struct Card {
    id: WidgetId,
    children: Vec<Box<dyn Widget>>,
    max_width: Option<f64>,
    bounds: Rect,
}

impl Card {
    pub fn new() -> Self {
        Self {
            id: WidgetId::next(),
            children: Vec::new(),
            max_width: None,
            bounds: Rect::ZERO,
        }
    }

    pub fn child(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    pub fn max_width(mut self, width: f64) -> Self {
        self.max_width = Some(width);
        self
    }
}

impl Default for Card {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Card {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut [Box<dyn Widget>] {
        &mut self.children
    }

    fn style(&self, _theme: &Theme) -> taffy::Style {
        let pad = taffy::LengthPercentage::length(space(5) as f32);
        let mut style = taffy::Style {
            display: taffy::Display::Flex,
            flex_direction: taffy::FlexDirection::Column,
            gap: taffy::Size {
                width: taffy::LengthPercentage::length(0.0),
                height: taffy::LengthPercentage::length(space(4) as f32),
            },
            padding: taffy::Rect {
                left: pad,
                right: pad,
                top: pad,
                bottom: pad,
            },
            min_size: taffy::Size {
                width: taffy::LengthPercentageAuto::length(0.0),
                height: taffy::LengthPercentageAuto::auto(),
            },
            ..Default::default()
        };
        if let Some(w) = self.max_width {
            style.max_size.width = taffy::LengthPercentageAuto::length(w as f32);
            // a card with a maximum fills up to it, like a block element
            style.size.width = taffy::Dimension::percent(1.0);
        }
        style
    }

    fn set_bounds(&mut self, _cx: &mut LayoutCx<'_>, bounds: Rect) {
        self.bounds = bounds;
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn paint(&self, cx: &mut PaintCx<'_>) {
        cx.ctx.set_paint(cx.theme.card_bg);
        cx.ctx
            .fill_path(&paint::rounded(self.bounds, cx.theme.metrics.radius_lg));
        cx.paint_children(&self.children);
    }
}

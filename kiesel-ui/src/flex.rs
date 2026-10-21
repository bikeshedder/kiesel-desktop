//! `Flex` – Kiesel's layout primitives: [`Flex::stack`] (children below each
//! other, even gap) and [`Flex::cluster`] (a row that wraps: buttons, tags).

use vello_cpu::kurbo::Rect;

use crate::theme::{Theme, space};
use crate::widget::{LayoutCx, Widget, WidgetId};

/// Cross-axis alignment of the children
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
    Stretch,
}

/// Main-axis distribution of the children
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Justify {
    Start,
    Center,
    End,
    Between,
    Around,
}

pub struct Flex {
    id: WidgetId,
    children: Vec<Box<dyn Widget>>,
    style: taffy::Style,
    bounds: Rect,
}

fn px(v: f64) -> taffy::LengthPercentage {
    taffy::LengthPercentage::length(v as f32)
}

impl Flex {
    fn new(style: taffy::Style) -> Self {
        Self {
            id: WidgetId::next(),
            children: Vec::new(),
            style,
            bounds: Rect::ZERO,
        }
    }

    /// Vertical rhythm: children below each other with an even gap (`--size-4`), stretched across
    pub fn stack() -> Self {
        Self::new(taffy::Style {
            display: taffy::Display::Flex,
            flex_direction: taffy::FlexDirection::Column,
            gap: taffy::Size {
                width: px(0.0),
                height: px(space(4)),
            },
            align_items: taffy::AlignItems::STRETCH,
            ..Default::default()
        })
        .min_width_zero()
    }

    /// A row that wraps, items centred, gap `--size-3`
    pub fn cluster() -> Self {
        Self::new(taffy::Style {
            display: taffy::Display::Flex,
            flex_direction: taffy::FlexDirection::Row,
            flex_wrap: taffy::FlexWrap::Wrap,
            gap: taffy::Size {
                width: px(space(3)),
                height: px(space(3)),
            },
            align_items: taffy::AlignItems::CENTER,
            ..Default::default()
        })
        .min_width_zero()
    }

    /// `min-width: 0` – may shrink below its content, so text inside can wrap
    fn min_width_zero(mut self) -> Self {
        self.style.min_size.width = taffy::LengthPercentageAuto::length(0.0);
        self
    }

    pub fn child(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    /// Gap between children, in logical pixels (see [`space`])
    pub fn gap(mut self, gap: f64) -> Self {
        self.style.gap = taffy::Size {
            width: px(gap),
            height: px(gap),
        };
        self
    }

    pub fn align(mut self, align: Align) -> Self {
        self.style.align_items = match align {
            Align::Start => taffy::AlignItems::START,
            Align::Center => taffy::AlignItems::CENTER,
            Align::End => taffy::AlignItems::END,
            Align::Stretch => taffy::AlignItems::STRETCH,
        };
        self
    }

    pub fn justify(mut self, justify: Justify) -> Self {
        self.style.justify_content = match justify {
            Justify::Start => taffy::JustifyContent::START,
            Justify::Center => taffy::JustifyContent::CENTER,
            Justify::End => taffy::JustifyContent::END,
            Justify::Between => taffy::JustifyContent::SPACE_BETWEEN,
            Justify::Around => taffy::JustifyContent::SPACE_AROUND,
        };
        self
    }

    /// Keep everything on one line (clusters)
    pub fn nowrap(mut self) -> Self {
        self.style.flex_wrap = taffy::FlexWrap::NoWrap;
        self
    }

    pub fn padding(mut self, padding: f64) -> Self {
        self.style.padding = taffy::Rect {
            left: px(padding),
            right: px(padding),
            top: px(padding),
            bottom: px(padding),
        };
        self
    }

    pub fn max_width(mut self, width: f64) -> Self {
        self.style.max_size.width = taffy::LengthPercentageAuto::length(width as f32);
        self
    }

    /// Take the free space along the parent's main axis (`flex: 1`)
    pub fn grow(mut self) -> Self {
        self.style.flex_grow = 1.0;
        self
    }
}

impl Widget for Flex {
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
        self.style.clone()
    }

    fn set_bounds(&mut self, _cx: &mut LayoutCx<'_>, bounds: Rect) {
        self.bounds = bounds;
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }
}

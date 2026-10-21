//! Kbd: a keyboard key or shortcut as a small raised key ("Ctrl+K", "⌘K").
//! Give it a [`Shortcut`]'s text to show it the platform's way, in the user's
//! language: `Kbd::new(Shortcut::primary('k').text())`.
//!
//! [`Shortcut`]: crate::Shortcut

use parley::Layout;
use reactive_graph::traits::Track;
use vello_cpu::kurbo::{Point, Rect, Size};

use crate::i18n::{IntoText, TextProp};
use crate::paint;
use crate::reactive::{MountCx, Watch};
use crate::text;
use crate::theme::{ControlSize, Shadow, TextStyle, Theme};
use crate::widget::{Available, Change, Known, LayoutCx, PaintCx, Widget, WidgetId};

pub struct Kbd {
    id: WidgetId,
    text: TextProp,
    size: ControlSize,
    watch: Option<Watch>,
    layout: Option<Layout<()>>,
    /// What it was shaped with – the theme's text size can change
    style: Option<TextStyle>,
    bounds: Rect,
}

/// Height, inline padding, corner radius (at roundness 1) and font size, in px
struct Look {
    height: f64,
    padding: f64,
    radius: f64,
    font: f32,
}

impl Kbd {
    /// A string, or a signal: `Shortcut::text()` follows platform and language
    pub fn new(text: impl IntoText) -> Self {
        Self {
            id: WidgetId::next(),
            text: TextProp::new(text),
            size: ControlSize::Md,
            watch: None,
            layout: None,
            style: None,
            bounds: Rect::ZERO,
        }
    }

    /// Sm in small controls, Lg next to larger text (Xs and Xl are Sm and Lg)
    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    fn look(&self, theme: &Theme) -> Look {
        let roundness = theme.metrics.radius / 6.0;
        let scale = f64::from(theme.metrics.font_size / 16.0);
        let (height, padding, radius, font) = match self.size {
            ControlSize::Xs | ControlSize::Sm => (18.0, 4.0, 4.0, 10.0),
            ControlSize::Md => (22.0, 6.0, 6.0, 11.0),
            ControlSize::Lg | ControlSize::Xl => (26.0, 8.0, 8.0, 12.0),
        };
        Look {
            height: height * scale.max(1.0),
            padding,
            radius: radius * roundness,
            font: font * scale as f32,
        }
    }

    fn text_style(&self, theme: &Theme) -> TextStyle {
        TextStyle {
            // --ksl-font-mono
            family: "ui-monospace, \"DejaVu Sans Mono\", monospace",
            size: self.look(theme).font,
            weight: 600.0,
            letter_spacing: 0.0,
            line_height: 1.25,
        }
    }
}

impl Widget for Kbd {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn mount(&mut self, cx: &mut MountCx<'_>) {
        let text = self.text.signal();
        self.watch = Some(cx.watch(self.id, move || text.track()));
    }

    fn update(&mut self, cx: &mut LayoutCx<'_>) -> Change {
        let restyled = self.style.is_some_and(|s| s != self.text_style(cx.theme));
        if self.text.refresh() || restyled {
            self.layout = None;
            return Change::LAYOUT;
        }
        Change::NONE
    }

    /// At least as wide as high; never wraps
    fn measure(&mut self, cx: &mut LayoutCx<'_>, _known: Known, _available: Available) -> Size {
        let look = self.look(cx.theme);
        let style = self.text_style(cx.theme);
        self.style = Some(style);
        let layout = self
            .layout
            .get_or_insert_with(|| cx.text.line(&self.text.shown, &style));
        let width = (f64::from(layout.width()) + 2.0 * look.padding).max(look.height);
        Size::new(width.ceil(), look.height.max(f64::from(layout.height())))
    }

    fn style(&self, _theme: &Theme) -> taffy::Style {
        taffy::Style {
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn set_bounds(&mut self, _cx: &mut LayoutCx<'_>, bounds: Rect) {
        self.bounds = bounds;
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    /// `1px 1px 3px` reaches 2.75 px right and down (σ = 1.5, cut at 2.5 σ)
    fn ink(&self) -> Rect {
        let b = self.bounds;
        Rect::new(b.x0 - 4.0, b.y0 - 4.0, b.x1 + 6.0, b.y1 + 6.0)
    }

    fn paint(&self, cx: &mut PaintCx<'_>) {
        let theme = cx.theme;
        let look = self.look(theme);
        // box-shadow: -1px -1px 2px var(--ksl-hi), 1px 1px 3px var(--ksl-lo)
        let shadow = |x: f64, blur: f64, color| Shadow {
            inset: false,
            x,
            y: x,
            blur,
            spread: 0.0,
            color,
        };
        paint::outer_shadows(
            cx.ctx,
            self.bounds,
            look.radius,
            &[shadow(-1.0, 2.0, theme.hi), shadow(1.0, 3.0, theme.lo)],
            1.0,
        );
        cx.ctx.set_paint(theme.bg);
        cx.ctx.fill_path(&paint::rounded(self.bounds, look.radius));
        if let Some(layout) = &self.layout {
            let size = Size::new(f64::from(layout.width()), f64::from(layout.height()));
            let c = self.bounds.center();
            let origin = Point::new(c.x - size.width / 2.0, c.y - size.height / 2.0);
            text::draw(cx.ctx, cx.res, layout, origin, theme.text_3);
        }
    }

    fn accessibility(&self) -> Option<accesskit::Node> {
        if self.text.shown.is_empty() {
            return None;
        }
        let mut node = accesskit::Node::new(accesskit::Role::Label);
        node.set_value(self.text.shown.clone());
        node.set_bounds(crate::widget::accessible_rect(self.bounds));
        Some(node)
    }
}

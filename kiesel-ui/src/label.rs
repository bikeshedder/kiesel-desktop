//! `Label` – a paragraph of text in one role of the type scale; it wraps at
//! word boundaries to whatever width layout gives it.

use parley::Layout;
use reactive_graph::traits::{GetUntracked, Track, WithUntracked};
use reactive_graph::wrappers::read::ArcSignal;
use vello_cpu::kurbo::{Rect, Size};

use crate::i18n::IntoText;
use crate::icon::Tone;
use crate::reactive::{MountCx, Watch};
use crate::text::{self, wrap};
use crate::theme::{TextRole, TextStyle};
use crate::widget::{Available, Change, Known, LayoutCx, PaintCx, Widget, WidgetId};

pub struct Label {
    id: WidgetId,
    text: ArcSignal<String>,
    watch: Option<Watch>,
    /// The text as of the last update – what is shaped and on screen
    shown: String,
    role: TextRole,
    /// Another weight than the role's (a switch's label: 500)
    weight: Option<f32>,
    /// Kept from assistive technology (`aria-hidden`): it repeats what the
    /// tree says otherwise
    hidden: bool,
    /// Another colour than the role's – may change (a field's label turns red)
    tone: Option<ArcSignal<Tone>>,
    /// Shaped once; broken into lines again only when the width changes
    layout: Option<Layout<()>>,
    /// What it was shaped with – the theme's text size can change
    style: Option<TextStyle>,
    broken_at: Option<f32>,
    bounds: Rect,
}

impl Label {
    /// A string, or a signal to follow: `t("…")`, `ArcSignal::derive(…)`
    pub fn new(text: impl IntoText) -> Self {
        let text = text.into_text();
        Self {
            id: WidgetId::next(),
            shown: text.with_untracked(String::clone),
            watch: None,
            text,
            role: TextRole::Body,
            tone: None,
            hidden: false,
            weight: None,
            layout: None,
            style: None,
            broken_at: None,
            bounds: Rect::ZERO,
        }
    }

    pub fn role(mut self, role: TextRole) -> Self {
        self.role = role;
        self
    }

    /// Another font weight than the role's: 500 for a little emphasis
    pub fn weight(mut self, weight: f32) -> Self {
        self.weight = Some(weight);
        self
    }

    fn text_style(&self, theme: &crate::theme::Theme) -> TextStyle {
        let mut style = theme.text_style(self.role);
        if let Some(w) = self.weight {
            style.weight = w;
        }
        style
    }

    /// Kept from assistive technology, like `aria-hidden` – a decoration whose
    /// meaning the tree already has (a required field's asterisk)
    pub fn hidden(mut self) -> Self {
        self.hidden = true;
        self
    }

    /// Its colour, instead of the role's: a [`Tone`], or a signal of one
    pub fn tone(mut self, tone: impl Into<ArcSignal<Tone>>) -> Self {
        self.tone = Some(tone.into());
        self
    }

    /// The same as [`Label::new`] – kept for code that reads better with it
    pub fn bound(text: impl IntoText) -> Self {
        Self::new(text)
    }

    pub fn title(text: impl IntoText) -> Self {
        Self::new(text).role(TextRole::Title)
    }

    pub fn caption(text: impl IntoText) -> Self {
        Self::new(text).role(TextRole::Caption)
    }

    fn layout(&mut self, cx: &mut LayoutCx<'_>) -> &mut Layout<()> {
        let style = self.text_style(cx.theme);
        self.style = Some(style);
        self.layout
            .get_or_insert_with(|| cx.text.paragraph(&self.shown, &style))
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

    fn mount(&mut self, cx: &mut MountCx<'_>) {
        let (text, tone) = (self.text.clone(), self.tone.clone());
        self.watch = Some(cx.watch(self.id, move || {
            text.track();
            if let Some(tone) = &tone {
                tone.track();
            }
        }));
    }

    /// New text or another text size: shape again, and its size may change
    fn update(&mut self, cx: &mut LayoutCx<'_>) -> Change {
        let restyled = self.style.is_some_and(|s| s != self.text_style(cx.theme));
        if !restyled && self.text.with_untracked(|t| *t == self.shown) {
            // the tone may have changed: a repaint is cheap
            return if self.tone.is_some() {
                Change::PAINT
            } else {
                Change::NONE
            };
        }
        self.shown = self.text.with_untracked(String::clone);
        self.layout = None;
        self.broken_at = None;
        Change::LAYOUT
    }

    fn measure(&mut self, cx: &mut LayoutCx<'_>, known: Known, available: Available) -> Size {
        if self.shown.is_empty() {
            // like an empty paragraph on the web: no box at all
            return Size::ZERO;
        }
        let widths = self.layout(cx).calculate_content_widths();
        let width = known.width.unwrap_or(match available.width {
            taffy::AvailableSpace::MinContent => widths.min,
            taffy::AvailableSpace::MaxContent => widths.max,
            taffy::AvailableSpace::Definite(w) => w.min(widths.max),
        });
        // the box is as wide as offered (block-like); its height follows the lines.
        // Whole pixels, rounded up: taffy rounds boxes, and a box a fraction
        // narrower than its line would wrap the last word
        let size = self.wrap_at(cx, width);
        Size::new(
            f64::from(known.width.unwrap_or(size.width as f32)).ceil(),
            known.height.map_or(size.height, f64::from).ceil(),
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
                match &self.tone {
                    Some(tone) => tone.get_untracked().color(cx.theme),
                    None => cx.theme.text_color(self.role),
                },
            );
        }
    }

    /// Empty text says nothing: it stays out of the tree
    fn accessibility(&self) -> Option<accesskit::Node> {
        if self.shown.is_empty() || self.hidden {
            return None;
        }
        let mut node = accesskit::Node::new(accesskit::Role::Label);
        node.set_value(self.shown.clone());
        node.set_bounds(crate::widget::accessible_rect(self.bounds));
        Some(node)
    }
}

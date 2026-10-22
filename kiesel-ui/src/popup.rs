//! Popups: panels in a layer above the window's content – the way Kiesel Web
//! does it, so they keep Kiesel's look (rounded, shadowed, animated). They
//! stay inside the window: a panel flips to the other side of its anchor or
//! slides along the edge to fit (`positionPanel` on the web).
//!
//! The shell's root holds the app's content and the open layers, later ones on
//! top; layers lay themselves out ([`Widget::detached`]). A click outside the
//! topmost panel or Escape dismisses it; focus goes back to its owner.

use std::cell::RefCell;
use std::time::Instant;

use reactive_graph::signal::ArcRwSignal;
use reactive_graph::traits::{GetUntracked, Set, Track};
use vello_cpu::kurbo::{Affine, BezPath, Point, Rect, Size, Stroke, Vec2};

use crate::anim::{CubicBezier, EASE};
use crate::i18n::{IntoText, TextProp};
use crate::layout::Layout;
use crate::paint;
use crate::reactive::{MountCx, Watch};
use crate::theme::{Theme, space};
use crate::widget::{Change, LayoutCx, PaintCx, Widget, WidgetId, accessible_rect};

/// Which side of its anchor a panel opens on
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Top,
    Bottom,
    Left,
    Right,
}

/// Where along that side
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub side: Side,
    pub align: Align,
}

impl Placement {
    pub const BOTTOM_START: Self = Self {
        side: Side::Bottom,
        align: Align::Start,
    };
    pub const BOTTOM: Self = Self {
        side: Side::Bottom,
        align: Align::Center,
    };
    pub const TOP: Self = Self {
        side: Side::Top,
        align: Align::Center,
    };
    pub const RIGHT: Self = Self {
        side: Side::Right,
        align: Align::Center,
    };
}

/// What a widget asks for when it opens a popup ([`LayoutCx::open_popup`])
pub struct Popup {
    /// The widget it belongs to: focus returns there, and a press on it while
    /// the popup is open only closes it (it doesn't reopen it)
    pub owner: WidgetId,
    /// What it opens next to, in window coordinates
    pub anchor: Rect,
    pub placement: Placement,
    /// An arrow pointing at the anchor
    pub arrow: bool,
    /// Its name for assistive technology
    pub label: String,
    pub content: Box<dyn Widget>,
    /// Closed by the user (a click outside, Escape) – the owner updates its state
    pub on_dismiss: Option<Box<dyn FnMut()>>,
    /// A list (a select's options): no padding of its own, at most 400 px high,
    /// at least as wide as its anchor
    pub list: bool,
}

pub(crate) enum Command {
    Open(Box<Layer>),
    Close(WidgetId),
}

/// Popups opened and closed during an update, applied by the shell afterwards
#[derive(Default)]
pub(crate) struct PopupQueue(RefCell<Vec<Command>>);

impl PopupQueue {
    pub fn open(&self, popup: Popup) -> WidgetId {
        let layer = Layer::new(popup);
        let id = layer.id;
        self.0.borrow_mut().push(Command::Open(Box::new(layer)));
        id
    }

    pub fn close(&self, id: WidgetId) {
        self.0.borrow_mut().push(Command::Close(id));
    }

    pub fn take(&self) -> Vec<Command> {
        std::mem::take(&mut *self.0.borrow_mut())
    }
}

/// Gap to the anchor, plus room for the arrow; distance kept from the window's edges
const GAP: f64 = 8.0;
const ARROW: f64 = 6.0;
const MARGIN: f64 = 8.0;
/// The arrow keeps this far from the panel's corners
const ARROW_INSET: f64 = 18.0;
/// Opening: 180 ms ease-out-3; closing plays it backwards in 130 ms
const OPEN: f64 = 0.18;
const CLOSE: f64 = 0.13;

/// How far `elapsed` is into an animation of `seconds` (0 … 1)
fn progress(elapsed: std::time::Duration, seconds: f64) -> f64 {
    if seconds <= 0.0 {
        1.0
    } else {
        (elapsed.as_secs_f64() / seconds).min(1.0)
    }
}
const EASE_OUT_3: CubicBezier = CubicBezier::new(0.0, 0.0, 0.3, 1.0);

/// Where a panel of `size` goes next to `anchor` inside `window`: its top-left
/// corner, the side it ended up on, and where the arrow sits along it
pub(crate) fn place(
    anchor: Rect,
    size: Size,
    window: Rect,
    placement: Placement,
    arrow: bool,
) -> (Point, Side, f64) {
    let gap = GAP + if arrow { ARROW } else { 0.0 };
    let (w, h) = (size.width, size.height);
    // beside the anchor without room on either side: below it (or above) instead –
    // a window can't scroll to show the rest, as a page can
    let beside = matches!(placement.side, Side::Left | Side::Right);
    let room_right = window.x1 - MARGIN - (anchor.x1 + gap);
    let room_left = anchor.x0 - gap - (window.x0 + MARGIN);
    if beside && w > room_right && w > room_left {
        return place(
            anchor,
            size,
            window,
            Placement {
                side: Side::Bottom,
                align: Align::Center,
            },
            arrow,
        );
    }
    let clamp = |v: f64, lo: f64, hi: f64| v.min(hi).max(lo);
    let mut side = placement.side;
    let (x, y);
    match side {
        Side::Top | Side::Bottom => {
            let below = anchor.y1 + gap;
            let above = anchor.y0 - h - gap;
            let mut top = if side == Side::Bottom { below } else { above };
            if side == Side::Bottom && top + h > window.y1 - MARGIN && above > window.y0 {
                side = Side::Top;
                top = above;
            } else if side == Side::Top && top < window.y0 + MARGIN {
                side = Side::Bottom;
                top = below;
            }
            let left = match placement.align {
                Align::Start => anchor.x0,
                Align::End => anchor.x1 - w,
                Align::Center => anchor.center().x - w / 2.0,
            };
            x = clamp(left, window.x0 + MARGIN, window.x1 - w - MARGIN);
            y = top;
        }
        Side::Left | Side::Right => {
            let right = anchor.x1 + gap;
            let left = anchor.x0 - w - gap;
            let mut l = if side == Side::Right { right } else { left };
            if side == Side::Right && l + w > window.x1 - MARGIN && left > window.x0 {
                side = Side::Left;
                l = left;
            } else if side == Side::Left && l < window.x0 + MARGIN {
                side = Side::Right;
                l = right;
            }
            let top = match placement.align {
                Align::Start => anchor.y0,
                Align::End => anchor.y1 - h,
                Align::Center => anchor.center().y - h / 2.0,
            };
            x = l;
            y = clamp(top, window.y0 + MARGIN, window.y1 - h - MARGIN);
        }
    }
    let arrow_at = match side {
        Side::Top | Side::Bottom => clamp(anchor.center().x - x, ARROW_INSET, w - ARROW_INSET),
        Side::Left | Side::Right => clamp(anchor.center().y - y, ARROW_INSET, h - ARROW_INSET),
    };
    (Point::new(x.round(), y.round()), side, arrow_at)
}

/// An open popup: Kiesel's floating panel around the content
pub(crate) struct Layer {
    id: WidgetId,
    pub owner: WidgetId,
    anchor: Rect,
    placement: Placement,
    arrow: bool,
    label: String,
    children: Vec<Box<dyn Widget>>,
    on_dismiss: Option<Box<dyn FnMut()>>,
    /// A list: focus moves in however it opened
    pub list: bool,
    /// Focused before it opened: focus goes back there
    pub restore_focus: Option<WidgetId>,
    layout: Layout,
    bounds: Rect,
    /// The room it may take (the window, less a margin)
    room: Size,
    side: Side,
    arrow_at: f64,
    opened: Option<Instant>,
    closing: Option<Instant>,
    /// Opens and closes without animations (reduced motion)
    quick: bool,
}

impl Layer {
    fn new(p: Popup) -> Self {
        Self {
            id: WidgetId::next(),
            owner: p.owner,
            anchor: p.anchor,
            placement: p.placement,
            arrow: p.arrow,
            label: p.label,
            children: vec![p.content],
            on_dismiss: p.on_dismiss,
            list: p.list,
            restore_focus: None,
            layout: Layout::default(),
            bounds: Rect::ZERO,
            room: Size::new(f64::INFINITY, f64::INFINITY),
            side: p.placement.side,
            arrow_at: ARROW_INSET,
            opened: None,
            closing: None,
            quick: false,
        }
    }

    /// Lays the panel out at its natural size and places it next to its anchor
    pub fn lay_out(&mut self, cx: &mut LayoutCx<'_>, window: Rect, dirty: &[WidgetId]) {
        self.room = Size::new(
            window.width() - 2.0 * MARGIN,
            window.height() - 2.0 * MARGIN,
        );
        let mut layout = std::mem::take(&mut self.layout);
        if self.list {
            // a list fits on the side of its anchor with more room – it scrolls
            let below = window.y1 - MARGIN - (self.anchor.y1 + GAP);
            let above = self.anchor.y0 - GAP - (window.y0 + MARGIN);
            self.room.height = below.max(above).max(80.0);
        }
        let size = layout.fit(self, cx, dirty);
        let (origin, side, arrow_at) = place(self.anchor, size, window, self.placement, self.arrow);
        self.side = side;
        self.arrow_at = arrow_at;
        layout.place_at(self, cx, origin);
        self.layout = layout;
    }

    /// Closed by the user: tell the owner, play the closing animation
    pub fn dismiss(&mut self, now: Instant) {
        if let Some(f) = &mut self.on_dismiss {
            f();
        }
        self.close(now);
    }

    pub fn close(&mut self, now: Instant) {
        if self.closing.is_none() {
            self.closing = Some(now);
        }
    }

    pub fn is_closing(&self) -> bool {
        self.closing.is_some()
    }

    /// Closed, and done playing it
    pub fn is_gone(&self, now: Instant) -> bool {
        self.closing
            .is_some_and(|c| now.duration_since(c).as_secs_f64() >= self.close_for())
    }

    /// Shown from `now`; `quick`: without the animations (reduced motion)
    pub fn opened_at(&mut self, now: Instant, quick: bool) {
        self.opened.get_or_insert(now);
        self.quick = quick;
    }

    fn open_for(&self) -> f64 {
        if self.quick { 0.0 } else { OPEN }
    }

    fn close_for(&self) -> f64 {
        if self.quick { 0.0 } else { CLOSE }
    }

    /// How far it has appeared (0 … 1)
    fn shown(&self, now: Instant) -> f64 {
        if let Some(c) = self.closing {
            return 1.0 - EASE.ease(progress(now.duration_since(c), self.close_for()));
        }
        match self.opened {
            Some(o) => EASE_OUT_3.ease(progress(now.duration_since(o), self.open_for())),
            None => 0.0,
        }
    }

    /// The arrow: a 12 px square turned by 45°, half outside the panel
    fn arrow_path(&self) -> Option<(BezPath, BezPath)> {
        if !self.arrow {
            return None;
        }
        let b = self.bounds;
        let (c, facing) = match self.side {
            Side::Bottom => (
                Point::new(b.x0 + self.arrow_at, b.y0),
                [(-1.0, 0.0), (0.0, -1.0), (1.0, 0.0)],
            ),
            Side::Top => (
                Point::new(b.x0 + self.arrow_at, b.y1),
                [(-1.0, 0.0), (0.0, 1.0), (1.0, 0.0)],
            ),
            Side::Right => (
                Point::new(b.x0, b.y0 + self.arrow_at),
                [(0.0, -1.0), (-1.0, 0.0), (0.0, 1.0)],
            ),
            Side::Left => (
                Point::new(b.x1, b.y0 + self.arrow_at),
                [(0.0, -1.0), (1.0, 0.0), (0.0, 1.0)],
            ),
        };
        // half diagonal of a 12 px square
        let r = 12.0 * std::f64::consts::FRAC_1_SQRT_2;
        let p = |(dx, dy): (f64, f64)| c + Vec2::new(dx * r, dy * r);
        let mut fill = BezPath::new();
        fill.move_to(p(facing[0]));
        fill.line_to(p(facing[1]));
        fill.line_to(p(facing[2]));
        fill.close_path();
        let mut edge = BezPath::new();
        edge.move_to(p(facing[0]));
        edge.line_to(p(facing[1]));
        edge.line_to(p(facing[2]));
        Some((fill, edge))
    }
}

impl Widget for Layer {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut [Box<dyn Widget>] {
        &mut self.children
    }

    fn detached(&self) -> bool {
        true
    }

    /// Popover.module.css: padding --size-2, 200–360 px wide, no higher than the window;
    /// a list (OptionList.module.css .popup): no padding, as wide as its anchor
    /// at least, at most 400 px high
    fn style(&self, _theme: &Theme) -> taffy::Style {
        let len = |v: f64| taffy::LengthPercentageAuto::length(v as f32);
        if self.list {
            let wide = self.anchor.width().max(160.0).min(self.room.width);
            return taffy::Style {
                display: taffy::Display::Flex,
                flex_direction: taffy::FlexDirection::Column,
                min_size: taffy::Size {
                    width: len(wide),
                    height: taffy::LengthPercentageAuto::auto(),
                },
                max_size: taffy::Size {
                    width: len(480.0_f64.max(wide).min(self.room.width)),
                    height: len(400.0_f64.min(self.room.height)),
                },
                ..Default::default()
            };
        }
        let pad = taffy::LengthPercentage::length(space(2) as f32);
        taffy::Style {
            display: taffy::Display::Flex,
            flex_direction: taffy::FlexDirection::Column,
            padding: taffy::Rect {
                left: pad,
                right: pad,
                top: pad,
                bottom: pad,
            },
            min_size: taffy::Size {
                width: len(200.0_f64.min(self.room.width)),
                height: taffy::LengthPercentageAuto::auto(),
            },
            max_size: taffy::Size {
                width: len(360.0_f64.min(self.room.width)),
                height: len(self.room.height),
            },
            ..Default::default()
        }
    }

    fn set_bounds(&mut self, _cx: &mut LayoutCx<'_>, bounds: Rect) {
        self.bounds = bounds;
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn clip(&self) -> Option<Rect> {
        Some(self.bounds)
    }

    /// The float shadow reaches far: 24 px down with a 48 px blur
    fn ink(&self) -> Rect {
        let b = self.bounds;
        Rect::new(b.x0 - 64.0, b.y0 - 64.0, b.x1 + 64.0, b.y1 + 96.0)
    }

    fn hit(&self, p: Point) -> bool {
        // a closing panel lets the pointer through
        !self.is_closing() && self.bounds.contains(p)
    }

    fn animating(&self, now: Instant, _theme: &Theme) -> bool {
        match (self.opened, self.closing) {
            (_, Some(c)) => now.duration_since(c).as_secs_f64() < self.close_for() + 0.02,
            (Some(o), None) => now.duration_since(o).as_secs_f64() < self.open_for() + 0.02,
            (None, None) => true,
        }
    }

    fn accessibility(&self) -> Option<accesskit::Node> {
        let mut node = accesskit::Node::new(accesskit::Role::Dialog);
        if !self.label.is_empty() {
            node.set_label(self.label.clone());
        }
        node.set_bounds(accessible_rect(self.bounds));
        Some(node)
    }

    fn paint(&self, cx: &mut PaintCx<'_>) {
        let now = cx.now;
        let shown = self.shown(now);
        if shown <= 0.0 {
            return;
        }
        let theme = cx.theme;
        let r = self.bounds;
        let radius = theme.metrics.radius_lg;
        // the "shift" animation: in from 6 px towards the anchor, from 97 %
        let (dx, dy) = match self.side {
            Side::Bottom => (0.0, -1.0),
            Side::Top => (0.0, 1.0),
            Side::Right => (-1.0, 0.0),
            Side::Left => (1.0, 0.0),
        };
        let origin = match self.side {
            Side::Bottom => Point::new(r.x0 + self.arrow_at, r.y0),
            Side::Top => Point::new(r.x0 + self.arrow_at, r.y1),
            Side::Right => Point::new(r.x0, r.y0 + self.arrow_at),
            Side::Left => Point::new(r.x1, r.y0 + self.arrow_at),
        };
        let base = *cx.ctx.transform();
        let animating = shown < 1.0;
        if animating {
            let k = 1.0 - shown;
            let scale = 0.97 + 0.03 * shown;
            cx.ctx.set_transform(
                base * Affine::translate(Vec2::new(dx * 6.0 * k, dy * 6.0 * k))
                    * Affine::translate(origin.to_vec2())
                    * Affine::scale(scale)
                    * Affine::translate(-origin.to_vec2()),
            );
            cx.ctx.push_opacity_layer(shown as f32);
        }

        paint::outer_shadows(cx.ctx, r, radius, &theme.float, 1.0);
        cx.ctx.set_paint(theme.surface_raised);
        cx.ctx.fill_path(&paint::rounded(r, radius));
        cx.ctx.set_stroke(Stroke::new(1.0));
        cx.ctx.set_paint(theme.rim);
        cx.ctx
            .stroke_path(&paint::rounded(r.inflate(-0.5, -0.5), radius - 0.5));
        if let Some((fill, edge)) = self.arrow_path() {
            // over the border at its base, with the two outer edges in the rim colour
            cx.ctx.set_paint(theme.surface_raised);
            cx.ctx.fill_path(&fill);
            cx.ctx.set_paint(theme.rim);
            cx.ctx.stroke_path(&edge);
        }
        cx.ctx.push_clip_rect(&r);
        let outer = cx.clip;
        cx.clip = outer.intersect(r);
        cx.paint_children(&self.children);
        cx.clip = outer;
        cx.ctx.pop_clip();

        if animating {
            cx.ctx.pop_layer();
        }
        cx.ctx.set_transform(base);
    }
}

/// The shell's root: the app's content, then the open popup layers
pub(crate) struct Root {
    id: WidgetId,
    children: Vec<Box<dyn Widget>>,
    bounds: Rect,
}

impl Root {
    pub fn new(content: Box<dyn Widget>) -> Self {
        Self {
            id: WidgetId::next(),
            children: vec![content],
            bounds: Rect::ZERO,
        }
    }

    pub fn as_ref(&self) -> &dyn Widget {
        self
    }

    pub fn as_mut(&mut self) -> &mut dyn Widget {
        self
    }

    pub fn content_mut(&mut self) -> &mut dyn Widget {
        self.children[0].as_mut()
    }

    pub fn push_layer(&mut self, layer: Box<Layer>) {
        self.children.push(layer);
    }

    /// The open layers, bottom to top
    pub fn layers(&self) -> impl Iterator<Item = &Layer> {
        self.children[1..].iter().map(|c| as_layer(c.as_ref()))
    }

    pub fn layers_mut(&mut self) -> impl Iterator<Item = &mut Layer> {
        self.children[1..]
            .iter_mut()
            .map(|c| as_layer_mut(c.as_mut()))
    }

    pub fn layer_mut(&mut self, id: WidgetId) -> Option<&mut Layer> {
        self.layers_mut().find(|l| l.id == id)
    }

    /// The topmost layer that isn't closing
    pub fn top_layer(&self) -> Option<&Layer> {
        self.layers().filter(|l| !l.is_closing()).last()
    }

    /// Takes out the layers that finished closing; returns their inks
    pub fn remove_gone(&mut self, now: Instant) -> Vec<Rect> {
        let mut inks = Vec::new();
        let mut i = 1;
        while i < self.children.len() {
            let gone = as_layer(self.children[i].as_ref()).is_gone(now);
            if gone {
                inks.push(self.children[i].ink());
                self.children.remove(i);
            } else {
                i += 1;
            }
        }
        inks
    }
}

/// Every child after the first is a `Layer` – only `Root::push_layer` adds them
fn as_layer(w: &dyn Widget) -> &Layer {
    w.as_any().downcast_ref().expect("a layer")
}

fn as_layer_mut(w: &mut dyn Widget) -> &mut Layer {
    w.as_any_mut().downcast_mut().expect("a layer")
}

impl Widget for Root {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut [Box<dyn Widget>] {
        &mut self.children
    }

    fn set_bounds(&mut self, _cx: &mut LayoutCx<'_>, bounds: Rect) {
        self.bounds = bounds;
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }
}

type MakeContent = Box<dyn FnMut() -> Box<dyn Widget>>;

/// `Popover` – a panel next to its trigger while `open` is true: the trigger
/// (a button that toggles `open`, say) stays in the layout, the panel floats
/// above. A click outside or Escape sets `open` back to false.
pub struct Popover {
    id: WidgetId,
    open: ArcRwSignal<bool>,
    watch: Option<Watch>,
    children: Vec<Box<dyn Widget>>,
    make: MakeContent,
    placement: Placement,
    arrow: bool,
    label: TextProp,
    layer: Option<WidgetId>,
    bounds: Rect,
}

impl Popover {
    pub fn new<W: Widget + 'static>(
        open: ArcRwSignal<bool>,
        trigger: impl Widget + 'static,
        mut content: impl FnMut() -> W + 'static,
    ) -> Self {
        Self {
            id: WidgetId::next(),
            open,
            watch: None,
            children: vec![Box::new(trigger)],
            make: Box::new(move || Box::new(content())),
            placement: Placement::BOTTOM_START,
            arrow: false,
            label: TextProp::default(),
            layer: None,
            bounds: Rect::ZERO,
        }
    }

    pub fn placement(mut self, placement: Placement) -> Self {
        self.placement = placement;
        self
    }

    /// An arrow pointing at the trigger
    pub fn arrow(mut self) -> Self {
        self.arrow = true;
        self
    }

    /// The panel's name for assistive technology (the text at opening)
    pub fn label(mut self, label: impl IntoText) -> Self {
        self.label = TextProp::new(label);
        self
    }
}

impl Widget for Popover {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut [Box<dyn Widget>] {
        &mut self.children
    }

    fn layout_transparent(&self) -> bool {
        true
    }

    fn mount(&mut self, cx: &mut MountCx<'_>) {
        let open = self.open.clone();
        self.watch = Some(cx.watch(self.id, move || open.track()));
    }

    fn update(&mut self, cx: &mut LayoutCx<'_>) -> Change {
        match (self.open.get_untracked(), self.layer) {
            (true, None) => {
                let trigger = &self.children[0];
                let open = self.open.clone();
                self.layer = Some(cx.open_popup(Popup {
                    owner: trigger.id(),
                    anchor: trigger.bounds(),
                    placement: self.placement,
                    arrow: self.arrow,
                    label: {
                        self.label.refresh();
                        self.label.shown.clone()
                    },
                    content: (self.make)(),
                    on_dismiss: Some(Box::new(move || open.set(false))),
                    list: false,
                }));
            }
            (false, Some(layer)) => {
                cx.close_popup(layer);
                self.layer = None;
            }
            _ => {}
        }
        Change::NONE
    }

    fn set_bounds(&mut self, _cx: &mut LayoutCx<'_>, bounds: Rect) {
        self.bounds = bounds;
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Rect = Rect::new(0.0, 0.0, 600.0, 400.0);

    #[test]
    fn beside_without_room_on_either_side_goes_below() {
        let anchor = Rect::new(200.0, 50.0, 260.0, 90.0);
        let (p, side, _) = place(
            anchor,
            Size::new(360.0, 80.0),
            WINDOW,
            Placement::RIGHT,
            true,
        );
        assert_eq!(side, Side::Bottom);
        assert!(
            p.x >= 8.0 && p.x + 360.0 <= 592.0,
            "inside the window: {p:?}"
        );
    }

    #[test]
    fn below_and_aligned_when_there_is_room() {
        let anchor = Rect::new(100.0, 50.0, 180.0, 90.0);
        let (p, side, _) = place(
            anchor,
            Size::new(220.0, 100.0),
            WINDOW,
            Placement::BOTTOM_START,
            false,
        );
        assert_eq!((p, side), (Point::new(100.0, 98.0), Side::Bottom));
    }

    #[test]
    fn flips_above_when_the_bottom_is_too_close() {
        let anchor = Rect::new(100.0, 330.0, 180.0, 370.0);
        let (p, side, _) = place(
            anchor,
            Size::new(220.0, 100.0),
            WINDOW,
            Placement::BOTTOM_START,
            true,
        );
        assert_eq!(side, Side::Top);
        assert_eq!(p.y, 330.0 - 100.0 - 14.0, "gap 8 + arrow 6");
    }

    #[test]
    fn slides_along_the_edge_and_keeps_the_arrow_at_the_anchor() {
        let anchor = Rect::new(560.0, 50.0, 590.0, 90.0);
        let (p, _, arrow) = place(
            anchor,
            Size::new(220.0, 100.0),
            WINDOW,
            Placement::BOTTOM,
            true,
        );
        assert_eq!(p.x, 600.0 - 220.0 - 8.0, "kept 8 px off the right edge");
        assert_eq!(
            arrow,
            220.0 - ARROW_INSET,
            "pointing at the anchor, away from the corner"
        );
    }
}

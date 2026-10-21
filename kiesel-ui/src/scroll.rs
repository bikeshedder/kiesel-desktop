//! `Scroll` – a vertical scroll view around one child.
//!
//! - Touchpad: the content follows the fingers 1:1; when they lift it glides
//!   on with momentum (exponential decay, τ = 325 ms) – Wayland compositors
//!   leave that to the app, as GTK does it.
//! - Wheel: a notch scrolls `viewport^(2/3)` (GTK's step), animated in 150 ms.
//! - Keys the focused widget doesn't use: PageUp/PageDown, arrows, Home/End.
//! - Focus moving from the keyboard, or assistive technology, reveals widgets.
//! - The scrollbar is an overlay in Kiesel's colours (`--ksl-field-edge` on no
//!   track): thin while scrolling, wider under the pointer, draggable; it fades
//!   out a second after the last scroll.
//!
//! Scrolling moves the children without measuring anything ([`Change::PLACE`]),
//! and only the viewport is redrawn.

use std::collections::VecDeque;
use std::time::Instant;

use vello_cpu::kurbo::{Point, Rect, RoundedRect, Shape, Vec2};
use winit::cursor::CursorIcon;
use winit::keyboard::{Key, NamedKey};

use crate::anim::{EASE, Transition};
use crate::paint::TOLERANCE;
use crate::theme::{Theme, mix};
use crate::widget::{
    Change, Event, EventCx, LayoutCx, PaintCx, ScrollDelta, Widget, WidgetId, accessible_rect,
};

/// Wheel and key scrolling glide this long
const GLIDE: f64 = 0.15;
/// Momentum's time constant: the speed falls to 1/e in this time
const TAU: f64 = 0.325;
/// Below this speed (px/s) momentum stops; above this a lift starts it
const MIN_SPEED: f64 = 30.0;
const FLING: f64 = 120.0;
/// Touchpad samples within this window give the speed at lift-off
const SAMPLE_WINDOW: f64 = 0.1;
/// The scrollbar stays this long after the last scroll, then fades out
const BAR_LINGER: f64 = 1.0;
const BAR_FADE: f64 = 0.3;
/// The strip at the right edge where the pointer meets the scrollbar
const BAR_STRIP: f64 = 14.0;
const ARROW_STEP: f64 = 40.0;

#[derive(Clone, Copy, Debug)]
enum Motion {
    Still,
    /// Towards `to`, eased (wheel, keys)
    Glide {
        from: f64,
        to: f64,
        start: Instant,
    },
    /// After the fingers lift
    Momentum {
        from: f64,
        speed: f64,
        start: Instant,
    },
}

pub struct Scroll {
    id: WidgetId,
    children: Vec<Box<dyn Widget>>,
    bounds: Rect,
    style: taffy::Style,
    offset: f64,
    /// The offset the children were last placed with
    placed: f64,
    motion: Motion,
    /// Recent touchpad movement: (when, how far)
    samples: VecDeque<(Instant, f64)>,
    /// The last time it scrolled (the scrollbar shows from here)
    active: Option<Instant>,
    over_bar: bool,
    /// Dragging the thumb: where on it the pointer took hold
    grab: Option<f64>,
    wide: Transition,
    repaint: bool,
}

impl Scroll {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            id: WidgetId::next(),
            children: vec![Box::new(child)],
            bounds: Rect::ZERO,
            style: taffy::Style {
                display: taffy::Display::Block,
                overflow: taffy::Point {
                    x: taffy::Overflow::Visible,
                    y: taffy::Overflow::Scroll,
                },
                scrollbar_width: 0.0,
                // it may be smaller than its content – that's the point
                min_size: taffy::Size {
                    width: taffy::LengthPercentageAuto::length(0.0),
                    height: taffy::LengthPercentageAuto::length(0.0),
                },
                ..Default::default()
            },
            offset: 0.0,
            placed: 0.0,
            motion: Motion::Still,
            samples: VecDeque::new(),
            active: None,
            over_bar: false,
            grab: None,
            wide: Transition::new(0.0),
            repaint: false,
        }
    }

    /// Take the free space along the parent's column (`flex: 1`)
    pub fn grow(mut self) -> Self {
        self.style.flex_grow = 1.0;
        self
    }

    pub fn max_height(mut self, px: f64) -> Self {
        self.style.max_size.height = taffy::LengthPercentageAuto::length(px as f32);
        self
    }

    fn content_height(&self) -> f64 {
        self.children.first().map_or(0.0, |c| c.bounds().height())
    }

    fn max_offset(&self) -> f64 {
        (self.content_height() - self.bounds.height()).max(0.0)
    }

    fn scrollable(&self) -> bool {
        self.max_offset() > 0.5
    }

    /// Where the offset is at `now`, and whether the motion is over
    fn offset_at(&self, now: Instant) -> (f64, bool) {
        let max = self.max_offset();
        match self.motion {
            Motion::Still => (self.offset, true),
            Motion::Glide { from, to, start } => {
                let t = now.duration_since(start).as_secs_f64() / GLIDE;
                if t >= 1.0 {
                    (to, true)
                } else {
                    (from + (to - from) * EASE.ease(t), false)
                }
            }
            Motion::Momentum { from, speed, start } => {
                let t = now.duration_since(start).as_secs_f64();
                let decay = (-t / TAU).exp();
                let at = (from + speed * TAU * (1.0 - decay)).clamp(0.0, max);
                // it stops when slow, or at the edge it is moving towards
                let at_edge = (speed > 0.0 && at >= max) || (speed < 0.0 && at <= 0.0);
                let done = (speed * decay).abs() < MIN_SPEED || at_edge;
                (at, done)
            }
        }
    }

    /// Settles the motion at `now` into `offset`
    fn settle(&mut self, now: Instant) {
        let (at, done) = self.offset_at(now);
        self.offset = at.clamp(0.0, self.max_offset());
        if done {
            self.motion = Motion::Still;
        }
    }

    fn glide_to(&mut self, to: f64, now: Instant) {
        self.settle(now);
        // successive wheel notches add up to where the last glide was going
        let base = match self.motion {
            Motion::Glide { to, .. } => to,
            _ => self.offset,
        };
        let to = (to - self.offset + base).clamp(0.0, self.max_offset());
        self.motion = Motion::Glide {
            from: self.offset,
            to,
            start: now,
        };
        self.touch(now);
    }

    fn jump_to(&mut self, to: f64, now: Instant) {
        self.motion = Motion::Still;
        self.offset = to.clamp(0.0, self.max_offset());
        self.touch(now);
    }

    /// It scrolled: the scrollbar shows (again)
    fn touch(&mut self, now: Instant) {
        self.active = Some(now);
        self.repaint = true;
    }

    /// The scrollbar's track and thumb
    fn bar(&self) -> (Rect, Rect) {
        let track = Rect::new(
            self.bounds.x1 - BAR_STRIP,
            self.bounds.y0 + 2.0,
            self.bounds.x1 - 2.0,
            self.bounds.y1 - 2.0,
        );
        let h = track.height();
        let thumb_h =
            (h * self.bounds.height() / self.content_height().max(1.0)).clamp(24.0_f64.min(h), h);
        let y = track.y0 + (h - thumb_h) * self.offset / self.max_offset().max(1.0);
        (track, Rect::new(track.x0, y, track.x1, y + thumb_h))
    }

    /// How much of the scrollbar shows at `now` (0 … 1)
    fn bar_visibility(&self, now: Instant) -> f64 {
        if self.over_bar || self.grab.is_some() {
            return 1.0;
        }
        let Some(active) = self.active else {
            return 0.0;
        };
        let idle = now.duration_since(active).as_secs_f64() - BAR_LINGER;
        (1.0 - idle / BAR_FADE).clamp(0.0, 1.0)
    }

    fn page(&self) -> f64 {
        (self.bounds.height() - ARROW_STEP).max(ARROW_STEP)
    }

    fn drag_to(&mut self, y: f64, grab: f64, now: Instant) {
        let (track, thumb) = self.bar();
        let room = (track.height() - thumb.height()).max(1.0);
        self.jump_to((y - grab - track.y0) / room * self.max_offset(), now);
    }
}

impl Widget for Scroll {
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

    fn content_offset(&self) -> Vec2 {
        Vec2::new(0.0, self.placed)
    }

    fn clip(&self) -> Option<Rect> {
        Some(self.bounds)
    }

    fn children_placed(&mut self) -> bool {
        // the content may have shrunk while scrolled down
        let clamped = self.offset.clamp(0.0, self.max_offset());
        if clamped != self.placed {
            self.offset = clamped;
            self.placed = clamped;
            self.motion = Motion::Still;
            return true;
        }
        false
    }

    fn claims(&self, p: Point) -> bool {
        self.scrollable() && p.x >= self.bounds.x1 - BAR_STRIP
    }

    fn cursor(&self) -> CursorIcon {
        CursorIcon::Default
    }

    fn reveal(&mut self, rect: Rect) -> bool {
        let view = self.bounds;
        let mut to = self.offset;
        if rect.y1 > view.y1 {
            to += rect.y1 - view.y1;
        }
        if rect.y0 < view.y0 {
            // the start wins if it doesn't fit
            to -= view.y0 - rect.y0;
        }
        let to = to.clamp(0.0, self.max_offset());
        if (to - self.offset).abs() < 0.5 {
            return false;
        }
        self.jump_to(to, Instant::now());
        // the shell places the children right away
        self.placed = self.offset;
        true
    }

    fn update(&mut self, cx: &mut LayoutCx<'_>) -> Change {
        self.settle(cx.now);
        let repaint = std::mem::take(&mut self.repaint);
        // events and motions move `offset`; the children follow when placed
        if (self.offset - self.placed).abs() > 0.01 {
            self.placed = self.offset;
            // moving counts as scrolling: the bar stays until a second after it stops
            self.active = Some(cx.now);
            Change::PLACE
        } else if repaint {
            Change::PAINT
        } else {
            Change::NONE
        }
    }

    fn animating(&self, now: Instant, theme: &Theme) -> bool {
        let fading = self.active.is_some_and(|a| {
            let t = now.duration_since(a).as_secs_f64();
            (BAR_LINGER..BAR_LINGER + BAR_FADE + 0.05).contains(&t)
        });
        !matches!(self.motion, Motion::Still)
            || fading
            || self.wide.animating(now, theme.metrics.duration)
    }

    fn wake_at(&self, now: Instant) -> Option<Instant> {
        // the fade starts after the linger, with no frames until then
        let a = self.active?;
        let start = a + std::time::Duration::from_secs_f64(BAR_LINGER);
        (start > now && !self.over_bar && self.grab.is_none()).then_some(start)
    }

    fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) {
        let now = cx.now;
        match event {
            Event::Scroll(delta) if self.scrollable() => {
                cx.handled = true;
                match *delta {
                    ScrollDelta::Lines(d) => {
                        let step = self.bounds.height().powf(2.0 / 3.0);
                        self.glide_to(self.offset + d.y * step, now);
                    }
                    ScrollDelta::Pixels(d) => {
                        self.settle(now);
                        self.motion = Motion::Still;
                        self.offset = (self.offset + d.y).clamp(0.0, self.max_offset());
                        self.samples.push_back((now, d.y));
                        while self.samples.front().is_some_and(|(t, _)| {
                            now.duration_since(*t).as_secs_f64() > SAMPLE_WINDOW
                        }) {
                            self.samples.pop_front();
                        }
                        self.touch(now);
                    }
                    ScrollDelta::Lifted => {
                        let span = self
                            .samples
                            .front()
                            .map_or(0.0, |(t, _)| now.duration_since(*t).as_secs_f64())
                            .max(1.0 / 120.0);
                        let speed = self.samples.iter().map(|(_, d)| d).sum::<f64>() / span;
                        self.samples.clear();
                        if speed.abs() > FLING {
                            self.motion = Motion::Momentum {
                                from: self.offset,
                                speed,
                                start: now,
                            };
                            self.touch(now);
                        }
                    }
                }
            }
            Event::KeyDown { key, .. } if self.scrollable() => {
                let to = match key {
                    Key::Named(NamedKey::PageDown) => Some(self.offset + self.page()),
                    Key::Named(NamedKey::PageUp) => Some(self.offset - self.page()),
                    Key::Named(NamedKey::ArrowDown) => Some(self.offset + ARROW_STEP),
                    Key::Named(NamedKey::ArrowUp) => Some(self.offset - ARROW_STEP),
                    Key::Named(NamedKey::Home) => Some(0.0),
                    Key::Named(NamedKey::End) => Some(self.max_offset()),
                    _ => None,
                };
                if let Some(to) = to {
                    cx.handled = true;
                    self.glide_to(to, now);
                }
            }
            Event::PointerEnter | Event::PointerMove(_) | Event::PointerLeave => {
                let over = match event {
                    Event::PointerMove(p) => self.claims(*p),
                    Event::PointerEnter => true,
                    _ => false,
                };
                if let (Event::PointerMove(p), Some(grab)) = (event, self.grab) {
                    self.drag_to(p.y, grab, now);
                }
                if over != self.over_bar {
                    self.over_bar = over;
                    if !over {
                        // fade from now, not from the last scroll
                        self.active = Some(now);
                    }
                    self.repaint = true;
                }
            }
            Event::PointerDown(p) if self.claims(*p) => {
                let (_, thumb) = self.bar();
                if thumb.inflate(0.0, 2.0).contains(*p) {
                    self.grab = Some(p.y - thumb.y0);
                } else {
                    // on the track: a page towards the pointer
                    let page = if p.y < thumb.y0 {
                        -self.page()
                    } else {
                        self.page()
                    };
                    self.glide_to(self.offset + page, now);
                }
                self.repaint = true;
            }
            Event::PointerUp(_) if self.grab.is_some() => {
                self.grab = None;
                self.active = Some(now);
                self.repaint = true;
            }
            _ => {}
        }
        let on = |b: bool| f64::from(u8::from(b));
        self.repaint |= self.wide.set(
            on(self.over_bar || self.grab.is_some()),
            now,
            cx.theme.metrics.duration,
        );
    }

    fn accessibility(&self) -> Option<accesskit::Node> {
        let mut node = accesskit::Node::new(accesskit::Role::ScrollView);
        node.set_bounds(accessible_rect(self.bounds));
        node.set_clips_children();
        node.set_scroll_y(self.offset);
        node.set_scroll_y_min(0.0);
        node.set_scroll_y_max(self.max_offset());
        Some(node)
    }

    fn paint(&self, cx: &mut PaintCx<'_>) {
        // a non-isolated clip: each drawing is masked, no layer to composite (a
        // clip layer cost half of a 4K scroll frame)
        cx.ctx.push_clip_rect(&self.bounds);
        let outer = cx.clip;
        cx.clip = outer.intersect(self.bounds);
        cx.paint_children(&self.children);
        cx.clip = outer;
        cx.ctx.pop_clip();

        let vis = self.bar_visibility(cx.now) as f32;
        if vis <= 0.0 || !self.scrollable() {
            return;
        }
        let (_, thumb) = self.bar();
        let theme = cx.theme;
        let wide = self.wide.value(cx.now, theme.metrics.duration);
        // thin (4 px) while just showing, 8 px under the pointer
        let w = 4.0 + 4.0 * wide;
        let thumb = Rect::new(thumb.x1 - w, thumb.y0, thumb.x1, thumb.y1);
        let color = mix(theme.field_shadow[0].color, theme.text_3, wide as f32 * 0.6);
        cx.ctx.set_paint(color.multiply_alpha(vis));
        cx.ctx
            .fill_path(&RoundedRect::from_rect(thumb, w / 2.0).to_path(TOLERANCE));
    }
}

#[cfg(test)]
mod tests {
    use reactive_graph::signal::ArcRwSignal;
    use reactive_graph::traits::Set;
    use vello_cpu::kurbo::Size;

    use super::*;
    use crate::widget::tests::lay_out;
    use crate::{Button, Flex, For};

    fn list(n: u32) -> (Flex, ArcRwSignal<Vec<u32>>) {
        let items = ArcRwSignal::new((0..n).collect::<Vec<_>>());
        let rows = For::new(items.clone(), |i: &u32| *i, |i| Button::new(i.to_string()));
        (
            Flex::stack().child(Scroll::new(Flex::stack().gap(8.0).child(rows)).grow()),
            items,
        )
    }

    fn scroll(root: &mut Flex) -> &mut Scroll {
        let s = root.children_mut()[0].as_mut() as *mut dyn Widget as *mut Scroll;
        // SAFETY: the first child is the Scroll built in `list`
        unsafe { &mut *s }
    }

    #[test]
    fn the_content_is_taller_than_the_view() {
        let (mut root, _) = list(50);
        lay_out(&mut root, Size::new(300.0, 400.0));
        let s = scroll(&mut root);
        assert_eq!(s.bounds().height(), 400.0);
        assert_eq!(s.content_height(), 50.0 * 48.0 - 8.0);
        assert_eq!(s.max_offset(), 50.0 * 48.0 - 8.0 - 400.0);
    }

    #[test]
    fn reveal_moves_as_little_as_needed() {
        let (mut root, _) = list(50);
        lay_out(&mut root, Size::new(300.0, 400.0));
        let s = scroll(&mut root);
        // a row below the view comes in at the bottom edge
        assert!(s.reveal(Rect::new(0.0, 500.0, 100.0, 540.0)));
        assert_eq!(s.offset, 140.0);
        // already in view: nothing moves
        assert!(!s.reveal(Rect::new(0.0, 100.0, 100.0, 140.0)));
        // above the view: in at the top edge
        assert!(s.reveal(Rect::new(0.0, -60.0, 100.0, -20.0)));
        assert_eq!(s.offset, 80.0);
    }

    #[test]
    fn shrinking_content_pulls_the_offset_back() {
        let (mut root, items) = list(50);
        lay_out(&mut root, Size::new(300.0, 400.0));
        let s = scroll(&mut root);
        s.offset = s.max_offset();
        s.placed = s.offset;
        // five rows (232 px) fit the 400 px view: nothing to scroll any more
        items.set((0..5).collect());
        lay_out(&mut root, Size::new(300.0, 400.0));
        let s = scroll(&mut root);
        assert_eq!(s.offset, 0.0);
        // the first row is placed at the top again
        let first = s.children()[0].children()[0].children()[0].bounds();
        assert_eq!(first.y0, 0.0);
    }

    #[test]
    fn momentum_stays_in_bounds_and_comes_to_rest() {
        let (mut root, _) = list(50);
        lay_out(&mut root, Size::new(300.0, 400.0));
        let s = scroll(&mut root);
        let start = Instant::now();
        s.motion = Motion::Momentum {
            from: 0.0,
            speed: 3000.0,
            start,
        };
        let mut last = 0.0;
        let mut rested = None;
        for ms in (0..4000).step_by(16) {
            let (at, done) = s.offset_at(start + std::time::Duration::from_millis(ms));
            assert!(
                at >= last && at <= s.max_offset(),
                "monotone and within bounds"
            );
            last = at;
            if done {
                rested = Some(ms);
                break;
            }
        }
        // 3000 px/s · τ = 975 px at most, reached as the speed falls below 30 px/s
        assert!((last - 975.0).abs() < 15.0, "{last}");
        assert!(rested.is_some_and(|ms| ms < 2000), "{rested:?}");
    }
}

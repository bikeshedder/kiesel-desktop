//! `Switch` – an on/off toggle: a round knob in a sunken pill
//! (see `kiesel/src/components/Switch`).
//!
//! The knob moves with the spring easing and stretches while pressed. A
//! press-and-release on it, Space (on release, like a checkbox) or the
//! assistive technology's click toggles. Like the web component it is
//! controlled: it shows `checked` and reports the wish to change it.

use std::time::Instant;

use reactive_graph::traits::{GetUntracked, Track};
use reactive_graph::wrappers::read::ArcSignal;
use vello_cpu::kurbo::{Point, Rect, RoundedRect, Shape, Size, Stroke, Vec2};
use vello_cpu::peniko::Gradient;
use winit::cursor::CursorIcon;
use winit::keyboard::Key;

use crate::anim::{Easing, Transition};
use crate::paint::{self, TOLERANCE};
use crate::reactive::{MountCx, Watch};
use crate::theme::{ControlSize, Shadow, Theme};
use crate::widget::{
    Available, Change, Event, EventCx, Known, LayoutCx, PaintCx, Widget, WidgetId, accessible_rect,
};

/// Track height, width, knob inset, shadow depth
fn dims(size: ControlSize) -> (f64, f64, f64, f64) {
    match size {
        ControlSize::Xs => (16.0, 30.0, 2.0, 1.0),
        ControlSize::Sm => (22.0, 40.0, 2.0, 1.5),
        ControlSize::Md => (28.0, 54.0, 3.0, 2.0),
        ControlSize::Lg => (36.0, 72.0, 4.0, 3.0),
        ControlSize::Xl => (64.0, 150.0, 5.0, 5.0),
    }
}

/// The knob slides in 380 ms with the spring, stretches in 200 ms, the ring fades like others
const SLIDE: f64 = 0.38;
const STRETCH: f64 = 0.2;

type OnChange = Box<dyn FnMut(bool)>;

pub struct Switch {
    id: WidgetId,
    checked: ArcSignal<bool>,
    watch: Option<Watch>,
    shown: bool,
    on_change: Option<OnChange>,
    label: String,
    size: ControlSize,
    bounds: Rect,
    hovered: bool,
    pointer_armed: bool,
    key_armed: bool,
    focused: bool,
    focus_visible: bool,
    slide: Transition,
    stretch: Transition,
    ring: Transition,
    repaint: bool,
}

impl Switch {
    pub fn new(checked: impl Into<ArcSignal<bool>>) -> Self {
        let checked = checked.into();
        let shown = checked.get_untracked();
        Self {
            id: WidgetId::next(),
            checked,
            watch: None,
            shown,
            on_change: None,
            label: String::new(),
            size: ControlSize::Md,
            bounds: Rect::ZERO,
            hovered: false,
            pointer_armed: false,
            key_armed: false,
            focused: false,
            focus_visible: false,
            slide: Transition::new(f64::from(u8::from(shown))).with_easing(Easing::SPRING),
            stretch: Transition::new(0.0),
            ring: Transition::new(0.0),
            repaint: false,
        }
    }

    /// Asked to change; the owner of `checked` decides
    pub fn on_change(mut self, f: impl FnMut(bool) + 'static) -> Self {
        self.on_change = Some(Box::new(f));
        self
    }

    /// Its accessible name
    pub fn label(mut self, text: impl Into<String>) -> Self {
        self.label = text.into();
        self
    }

    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    fn toggle(&mut self) {
        let to = !self.shown;
        if let Some(f) = &mut self.on_change {
            f(to);
        }
    }

    fn sync(&mut self, now: Instant, theme: &Theme) {
        let on = |b: bool| f64::from(u8::from(b));
        let pressed = (self.hovered && self.pointer_armed) || self.key_armed;
        let mut changed = self.slide.set(on(self.shown), now, SLIDE);
        changed |= self.stretch.set(on(pressed), now, STRETCH);
        changed |= self.ring.set(
            on(self.focused && self.focus_visible),
            now,
            theme.metrics.duration,
        );
        self.repaint |= changed;
    }
}

impl Widget for Switch {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn mount(&mut self, cx: &mut MountCx<'_>) {
        let checked = self.checked.clone();
        self.watch = Some(cx.watch(self.id, move || checked.track()));
    }

    fn update(&mut self, cx: &mut LayoutCx<'_>) -> Change {
        let checked = self.checked.get_untracked();
        if checked != self.shown {
            self.shown = checked;
            self.sync(cx.now, cx.theme);
        }
        if std::mem::take(&mut self.repaint) {
            Change::PAINT
        } else {
            Change::NONE
        }
    }

    fn style(&self, _theme: &Theme) -> taffy::Style {
        taffy::Style {
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn measure(&mut self, _cx: &mut LayoutCx<'_>, _known: Known, _available: Available) -> Size {
        let (h, w, _, _) = dims(self.size);
        Size::new(w, h)
    }

    fn set_bounds(&mut self, _cx: &mut LayoutCx<'_>, bounds: Rect) {
        self.bounds = bounds;
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    /// The ring sits 3 px out and is 3 px wide; the knob's shadow and overshoot reach further
    fn ink(&self) -> Rect {
        let (_, _, _, d) = dims(self.size);
        self.bounds.inflate(8.0 + d * 3.0, 8.0 + d * 3.0)
    }

    fn hit(&self, p: Point) -> bool {
        RoundedRect::from_rect(self.bounds, self.bounds.height() / 2.0).contains(p)
    }

    fn focusable(&self) -> bool {
        true
    }

    fn cursor(&self) -> CursorIcon {
        CursorIcon::Pointer
    }

    fn animating(&self, now: Instant, theme: &Theme) -> bool {
        self.slide.animating(now, SLIDE)
            || self.stretch.animating(now, STRETCH)
            || self.ring.animating(now, theme.metrics.duration)
    }

    fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) {
        let space = |k: &Key| matches!(k, Key::Character(c) if c == " ");
        // Space toggles; other keys go on to the ancestors
        if let Event::KeyDown { key, .. } | Event::KeyUp { key } = event
            && space(key)
        {
            cx.handled = true;
        }
        match event {
            Event::PointerEnter => self.hovered = true,
            Event::PointerLeave => self.hovered = false,
            Event::PointerMove(p) => self.hovered = self.hit(*p),
            Event::PointerDown(p) => self.pointer_armed = self.hit(*p),
            Event::PointerUp(p) => {
                let click = self.pointer_armed && self.hit(*p);
                self.pointer_armed = false;
                if click {
                    self.toggle();
                }
            }
            Event::KeyDown {
                key, repeat: false, ..
            } if space(key) => self.key_armed = true,
            Event::KeyUp { key } if space(key) && self.key_armed => {
                self.key_armed = false;
                self.toggle();
            }
            Event::Focus { focused, visible } => {
                self.focused = *focused;
                self.focus_visible = *visible;
                if !focused {
                    self.key_armed = false;
                }
            }
            Event::Activate => self.toggle(),
            _ => {}
        }
        self.sync(cx.now, cx.theme);
    }

    fn accessibility(&self) -> Option<accesskit::Node> {
        let mut node = accesskit::Node::new(accesskit::Role::Switch);
        if !self.label.is_empty() {
            node.set_label(self.label.clone());
        }
        node.set_toggled(if self.shown {
            accesskit::Toggled::True
        } else {
            accesskit::Toggled::False
        });
        node.set_bounds(accessible_rect(self.bounds));
        node.add_action(accesskit::Action::Click);
        node.add_action(accesskit::Action::Focus);
        Some(node)
    }

    fn paint(&self, cx: &mut PaintCx<'_>) {
        let (ctx, theme, now) = (&mut *cx.ctx, cx.theme, cx.now);
        let (h, w, pad, d) = dims(self.size);
        let track = self.bounds;
        let radius = h / 2.0;

        // focus: a 3 px outline, 3 px off the track
        let ring = self.ring.value(now, theme.metrics.duration) as f32;
        if ring > 0.0 {
            ctx.set_stroke(Stroke::new(3.0));
            ctx.set_paint(theme.accent.multiply_alpha(0.45 * ring));
            ctx.stroke_path(&paint::rounded(track.inflate(4.5, 4.5), radius + 4.5));
        }

        let shade = |x: f64, y: f64, blur: f64, spread: f64, inset: bool, color| Shadow {
            inset,
            x,
            y,
            blur,
            spread,
            color,
        };
        let track_shadows = [
            shade(d, d, d * 2.6, 0.0, true, theme.lo_strong),
            shade(-d, -d, d * 2.4, 0.0, true, theme.hi),
            shade(0.0, 0.0, 0.0, d * 0.6, false, theme.rim),
            shade(-d * 0.6, -d * 0.6, d * 1.4, 0.0, false, theme.hi),
        ];
        paint::outer_shadows(ctx, track, radius, &track_shadows, 1.0);
        ctx.set_paint(theme.surface_sunken);
        ctx.fill_path(&paint::rounded(track, radius));
        paint::inset_shadows(ctx, track, radius, &track_shadows, 1.0);

        // the knob: slides (overshooting with the spring) and stretches towards the middle while pressed
        let pos = self.slide.value(now, SLIDE);
        let extra = h * 0.18 * self.stretch.value(now, STRETCH);
        let k = h - 2.0 * pad;
        let x = track.x0 + pad + pos * (w - h - extra);
        let knob = Rect::new(x, track.y0 + pad, x + k + extra, track.y0 + pad + k);
        let knob_shadows = [
            shade(d, d * 1.4, d * 3.0, 0.0, false, theme.lo_strong),
            shade(-d * 0.4, -d * 0.4, d * 1.2, 0.0, false, theme.hi),
            shade(
                1.0,
                1.0,
                1.0,
                0.0,
                true,
                crate::theme::Color::new([1.0, 1.0, 1.0, 0.35]),
            ),
        ];
        paint::outer_shadows(ctx, knob, k / 2.0, &knob_shadows, 1.0);
        // CSS linear-gradient(145deg, …): the line runs through the centre at 145° from "up"
        let a = 145f64.to_radians();
        let dir = Vec2::new(a.sin(), -a.cos());
        let half = (knob.width() * a.sin().abs() + knob.height() * a.cos().abs()) / 2.0;
        let c = knob.center();
        ctx.set_paint(
            Gradient::new_linear(c - dir * half, c + dir * half).with_stops([
                (0.0, theme.knob[0]),
                (0.55, theme.knob[1]),
                (1.0, theme.knob[2]),
            ]),
        );
        ctx.fill_path(&RoundedRect::from_rect(knob, k / 2.0).to_path(TOLERANCE));
        paint::inset_shadows(ctx, knob, k / 2.0, &knob_shadows, 1.0);
    }
}

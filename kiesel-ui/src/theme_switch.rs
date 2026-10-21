//! `ThemeSwitch` – sun ☀ [ switch ] ☾, the reference control of Kiesel
//! (see `kiesel/src/components/ThemeSwitch`), and `IconToggle`, its icon buttons.
//!
//! The icons and the switch show the scheme on screen and choose light or
//! dark; choosing makes it the user's mode (no longer following the system).
//! A pressed icon is a touch darker; it spins in (sun) or rocks (moon) with
//! the spring when it becomes pressed.

use std::time::Instant;

use reactive_graph::traits::{Get, GetUntracked, Set, Track};
use reactive_graph::wrappers::read::ArcSignal;
use vello_cpu::kurbo::{Point, Rect, Size, Stroke};
use winit::cursor::CursorIcon;
use winit::keyboard::{Key, NamedKey};

use crate::anim::{Easing, Transition};
use crate::icon::{self, Icon};
use crate::reactive::{MountCx, Watch};
use crate::switch::Switch;
use crate::theme::{ControlSize, Scheme, Theme, mix, space};
use crate::theming::{ThemeMode, Theming};
use crate::widget::{
    Available, Change, Event, EventCx, Known, LayoutCx, PaintCx, Widget, WidgetId, accessible_rect,
};

/// How a pressed icon arrives: from a quarter turn back (sun) or a rock (moon)
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Arrival {
    SpinIn,
    Rock,
}

const COLOUR: f64 = 0.3;
const ARRIVE: f64 = 0.6;

/// An icon that is a toggle button (`aria-pressed`): no knob, just the icon
pub struct IconToggle {
    id: WidgetId,
    icon: Icon,
    label: String,
    pressed: ArcSignal<bool>,
    watch: Option<Watch>,
    shown: bool,
    on_press: Option<Box<dyn FnMut()>>,
    arrival: Arrival,
    /// Icon edge length and stroke width (in the icon's 24-unit box)
    px: f64,
    stroke: f64,
    bounds: Rect,
    hovered: bool,
    armed: bool,
    focused: bool,
    focus_visible: bool,
    hover: Transition,
    pressed_look: Transition,
    ring: Transition,
    /// When it last became pressed (the arrival plays from here)
    arrived: Option<Instant>,
    repaint: bool,
}

impl IconToggle {
    pub fn new(icon: Icon, label: impl Into<String>, pressed: impl Into<ArcSignal<bool>>) -> Self {
        let pressed = pressed.into();
        let shown = pressed.get_untracked();
        Self {
            id: WidgetId::next(),
            icon,
            label: label.into(),
            pressed,
            watch: None,
            shown,
            on_press: None,
            arrival: Arrival::SpinIn,
            px: 22.0,
            stroke: 1.75,
            bounds: Rect::ZERO,
            hovered: false,
            armed: false,
            focused: false,
            focus_visible: false,
            hover: Transition::new(0.0),
            pressed_look: Transition::new(f64::from(u8::from(shown))),
            ring: Transition::new(0.0),
            arrived: None,
            repaint: false,
        }
    }

    pub fn arrival(mut self, arrival: Arrival) -> Self {
        self.arrival = arrival;
        self
    }

    /// Activated while not pressed (pressing a pressed one does nothing, as on the web)
    pub fn on_press(mut self, f: impl FnMut() + 'static) -> Self {
        self.on_press = Some(Box::new(f));
        self
    }

    pub fn icon_size(mut self, px: f64, stroke: f64) -> Self {
        self.px = px;
        self.stroke = stroke;
        self
    }

    fn activate(&mut self) {
        if !self.shown
            && let Some(f) = &mut self.on_press
        {
            f();
        }
    }

    fn sync(&mut self, now: Instant) {
        let on = |b: bool| f64::from(u8::from(b));
        let mut changed = self.hover.set(on(self.hovered), now, COLOUR);
        changed |= self.pressed_look.set(on(self.shown), now, COLOUR);
        changed |= self
            .ring
            .set(on(self.focused && self.focus_visible), now, COLOUR);
        self.repaint |= changed;
    }
}

impl Widget for IconToggle {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn mount(&mut self, cx: &mut MountCx<'_>) {
        let pressed = self.pressed.clone();
        self.watch = Some(cx.watch(self.id, move || pressed.track()));
    }

    fn update(&mut self, cx: &mut LayoutCx<'_>) -> Change {
        let pressed = self.pressed.get_untracked();
        if pressed != self.shown {
            self.shown = pressed;
            if pressed {
                self.arrived = Some(cx.now);
            }
            self.sync(cx.now);
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
        Size::new(self.px, self.px)
    }

    fn set_bounds(&mut self, _cx: &mut LayoutCx<'_>, bounds: Rect) {
        self.bounds = bounds;
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    /// Turning, the icon's corners swing out a little; the ring adds 6 px
    fn ink(&self) -> Rect {
        self.bounds
            .inflate(self.px * 0.25 + 6.0, self.px * 0.25 + 6.0)
    }

    fn hit(&self, p: Point) -> bool {
        // round, like the web's border-radius: 50%
        self.bounds.center().distance(p) <= self.px / 2.0
    }

    fn focusable(&self) -> bool {
        true
    }

    fn cursor(&self) -> CursorIcon {
        CursorIcon::Pointer
    }

    fn animating(&self, now: Instant, _theme: &Theme) -> bool {
        self.hover.animating(now, COLOUR)
            || self.pressed_look.animating(now, COLOUR)
            || self.ring.animating(now, COLOUR)
            || self
                .arrived
                .is_some_and(|t| now.duration_since(t).as_secs_f64() < ARRIVE)
    }

    fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) {
        let space = |k: &Key| matches!(k, Key::Character(c) if c == " ");
        match event {
            Event::PointerEnter => self.hovered = true,
            Event::PointerLeave => self.hovered = false,
            Event::PointerMove(p) => self.hovered = self.hit(*p),
            Event::PointerDown(p) => self.armed = self.hit(*p),
            Event::PointerUp(p) => {
                let click = self.armed && self.hit(*p);
                self.armed = false;
                if click {
                    self.activate();
                }
            }
            Event::KeyUp { key } if space(key) => self.activate(),
            Event::KeyDown {
                key: Key::Named(NamedKey::Enter),
                repeat: false,
                ..
            } => self.activate(),
            Event::Focus { focused, visible } => {
                self.focused = *focused;
                self.focus_visible = *visible;
            }
            Event::Activate => self.activate(),
            _ => {}
        }
        self.sync(cx.now);
    }

    fn accessibility(&self) -> Option<accesskit::Node> {
        let mut node = accesskit::Node::new(accesskit::Role::Button);
        node.set_label(self.label.clone());
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
        let ring = self.ring.value(now, COLOUR) as f32;
        if ring > 0.0 {
            ctx.set_stroke(Stroke::new(3.0));
            ctx.set_paint(theme.accent.multiply_alpha(0.45 * ring));
            ctx.stroke_path(&paint_circle(self.bounds.center(), self.px / 2.0 + 4.5));
        }
        // --ksl-icon; hover: --ksl-text-2; pressed: 60 % icon, 40 % text-1
        let hovered = mix(
            theme.icon,
            theme.text_2,
            self.hover.value(now, COLOUR) as f32,
        );
        let color = mix(
            hovered,
            mix(theme.icon, theme.text_1, 0.4),
            self.pressed_look.value(now, COLOUR) as f32,
        );
        let (from_deg, from_scale) = match self.arrival {
            Arrival::SpinIn => (-90.0, 0.8),
            Arrival::Rock => (25.0, 0.85),
        };
        let (deg, scale) = match self.arrived {
            Some(t) if now.duration_since(t).as_secs_f64() < ARRIVE => {
                let p = Easing::SPRING.ease(now.duration_since(t).as_secs_f64() / ARRIVE);
                (from_deg * (1.0 - p), from_scale + (1.0 - from_scale) * p)
            }
            _ => (0.0, 1.0),
        };
        self.icon
            .draw(ctx, self.bounds, self.stroke, color, deg, scale);
    }
}

fn paint_circle(c: Point, r: f64) -> vello_cpu::kurbo::BezPath {
    use vello_cpu::kurbo::Shape;
    vello_cpu::kurbo::Circle::new(c, r).to_path(crate::paint::TOLERANCE)
}

/// Sun ☀ [ switch ] ☾ for a [`Theming`]
pub struct ThemeSwitch {
    id: WidgetId,
    children: Vec<Box<dyn Widget>>,
    gap: f64,
    bounds: Rect,
}

impl ThemeSwitch {
    pub fn new(theming: &Theming) -> Self {
        Self::sized(theming, ControlSize::Md)
    }

    pub fn sized(theming: &Theming, size: ControlSize) -> Self {
        let scheme = theming.scheme();
        let is = |s: Scheme| {
            let scheme = scheme.clone();
            ArcSignal::derive(move || scheme.get() == s)
        };
        let choose = |s: Scheme| {
            let mode = theming.mode();
            move || {
                mode.set(match s {
                    Scheme::Light => ThemeMode::Light,
                    Scheme::Dark => ThemeMode::Dark,
                })
            }
        };
        // icon size and gap per size, as in ThemeSwitch.module.css
        let (px, gap) = match size {
            ControlSize::Xs => (14.0, space(1)),
            ControlSize::Sm => (18.0, space(2)),
            ControlSize::Md => (22.0, space(2)),
            ControlSize::Lg => (30.0, space(3)),
            ControlSize::Xl => (64.0, space(5)),
        };
        let stroke = if size == ControlSize::Xl { 1.5 } else { 1.75 };
        let mode = theming.mode();
        let children: Vec<Box<dyn Widget>> = vec![
            Box::new(
                IconToggle::new(icon::SUN, "Light", is(Scheme::Light))
                    .icon_size(px, stroke)
                    .on_press(choose(Scheme::Light)),
            ),
            Box::new(
                Switch::new(is(Scheme::Dark))
                    .label("Dark")
                    .size(size)
                    .on_change(move |dark| {
                        mode.set(if dark {
                            ThemeMode::Dark
                        } else {
                            ThemeMode::Light
                        })
                    }),
            ),
            Box::new(
                IconToggle::new(icon::MOON_STAR, "Dark", is(Scheme::Dark))
                    .arrival(Arrival::Rock)
                    .icon_size(px, stroke)
                    .on_press(choose(Scheme::Dark)),
            ),
        ];
        Self {
            id: WidgetId::next(),
            children,
            gap,
            bounds: Rect::ZERO,
        }
    }
}

impl Widget for ThemeSwitch {
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
        let gap = taffy::LengthPercentage::length(self.gap as f32);
        taffy::Style {
            display: taffy::Display::Flex,
            align_items: taffy::AlignItems::CENTER,
            gap: taffy::Size {
                width: gap,
                height: gap,
            },
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

    fn accessibility(&self) -> Option<accesskit::Node> {
        let mut node = accesskit::Node::new(accesskit::Role::Group);
        node.set_label("Theme");
        node.set_bounds(accessible_rect(self.bounds));
        Some(node)
    }
}

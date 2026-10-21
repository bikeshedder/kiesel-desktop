//! `Button` – the default variant, size md, with `loading` (see `kiesel/src/components/Button`).
//!
//! Activation follows the browser's `<button>`: a pointer press and release on
//! it, Space (on release, pressed look while held), Enter (on press), or the
//! assistive technology's click. Focus from the keyboard shows the focus ring
//! (`:focus-visible`), focus from a pointer press doesn't.

use std::time::Instant;

use parley::Layout;
use reactive_graph::traits::{GetUntracked, Track};
use reactive_graph::wrappers::read::ArcSignal;
use vello_cpu::RenderContext;
use vello_cpu::kurbo::{Affine, Arc, Cap, Point, Rect, RoundedRect, Shape, Size, Stroke, Vec2};
use vello_cpu::peniko::Gradient;
use winit::cursor::CursorIcon;
use winit::keyboard::{Key, NamedKey};

use crate::anim::Transition;
use crate::paint::{self, TOLERANCE};
use crate::reactive::{MountCx, Watch};
use crate::text;
use crate::theme::{Color, Theme, mix};
use crate::widget::{
    Available, Change, Event, EventCx, Known, LayoutCx, PaintCx, Widget, WidgetId, accessible_rect,
};

/// The spinner box is 1.25em (the icon's place), the spinner itself 1.05em
const SPINNER_BOX_EM: f64 = 1.25;
const SPINNER_EM: f64 = 1.05;
/// One turn every 900 ms, linear
const SPIN_PERIOD: f64 = 0.9;

pub struct Button {
    id: WidgetId,
    label: String,
    /// What assistive technology announces, if not the label
    accessible_name: Option<String>,
    layout: Option<Layout<()>>,
    bounds: Rect,
    radius: f64,
    loading: ArcSignal<bool>,
    watch: Option<Watch>,
    /// `loading` as of the last update – what is on screen
    shown_loading: bool,
    spin_start: Instant,
    on_click: Option<Box<dyn FnMut()>>,
    hovered: bool,
    /// Pointer went down on the button and hasn't been released yet
    pointer_armed: bool,
    /// Space is held while focused
    key_armed: bool,
    focused: bool,
    focus_visible: bool,
    /// Its look changed since the last update
    repaint: bool,
    hover: Transition,
    press: Transition,
    ring: Transition,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            id: WidgetId::next(),
            label: label.into(),
            accessible_name: None,
            layout: None,
            bounds: Rect::ZERO,
            radius: 0.0,
            loading: ArcSignal::stored(false),
            watch: None,
            shown_loading: false,
            spin_start: Instant::now(),
            on_click: None,
            hovered: false,
            pointer_armed: false,
            key_armed: false,
            focused: false,
            focus_visible: false,
            repaint: false,
            hover: Transition::new(0.0),
            press: Transition::new(0.0),
            ring: Transition::new(0.0),
        }
    }

    /// Busy: a spinner before the label; the label stays
    /// A value, a signal, or `ArcSignal::derive(…)`
    pub fn loading(mut self, loading: impl Into<ArcSignal<bool>>) -> Self {
        self.loading = loading.into();
        self
    }

    /// A name for assistive technology when the label alone is ambiguous – many
    /// "Remove" buttons in a list each say what they remove (`aria-label`)
    pub fn accessible_name(mut self, name: impl Into<String>) -> Self {
        self.accessible_name = Some(name.into());
        self
    }

    pub fn on_click(mut self, f: impl FnMut() + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }

    fn activate(&mut self) {
        if let Some(f) = &mut self.on_click {
            f();
        }
    }

    fn hit_point(&self, p: Point) -> bool {
        RoundedRect::from_rect(self.bounds, self.radius).contains(p)
    }

    /// Moves the transitions towards the current state. While loading the
    /// button is busy, not interactive-looking: no hover or press visuals.
    fn sync(&mut self, now: Instant, theme: &Theme) {
        let d = theme.metrics.duration;
        let live = !self.shown_loading;
        let pressed = (self.hovered && self.pointer_armed) || self.key_armed;
        let target = |on: bool| f64::from(u8::from(on));
        let mut changed = self.hover.set(target(live && self.hovered), now, d);
        changed |= self.press.set(target(live && pressed), now, d);
        changed |= self
            .ring
            .set(target(self.focused && self.focus_visible), now, d);
        self.repaint |= changed;
    }
}

impl Widget for Button {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn mount(&mut self, cx: &mut MountCx<'_>) {
        let loading = self.loading.clone();
        self.watch = Some(cx.watch(self.id, move || loading.track()));
    }

    /// Picks up `loading` (it changes the width) and repaints after events
    fn update(&mut self, cx: &mut LayoutCx<'_>) -> Change {
        let mut change = Change::NONE;
        let loading = self.loading.get_untracked();
        if loading != self.shown_loading {
            if loading {
                self.spin_start = cx.now;
            }
            self.shown_loading = loading;
            self.sync(cx.now, cx.theme);
            change = Change::LAYOUT;
        }
        if std::mem::take(&mut self.repaint) {
            change |= Change::PAINT;
        }
        change
    }

    /// Inline padding + [spinner + gap] + label; at least `--ksl-control` high.
    /// Never narrower: a button doesn't wrap or shrink.
    fn measure(&mut self, cx: &mut LayoutCx<'_>, _known: Known, _available: Available) -> Size {
        let m = &cx.theme.metrics;
        let style = cx.theme.control_text();
        let layout = self
            .layout
            .get_or_insert_with(|| cx.text.line(&self.label, &style));
        let mut content = f64::from(layout.width());
        if self.shown_loading {
            content += SPINNER_BOX_EM * f64::from(m.font_size) + m.gap;
        }
        Size::new(
            content + 2.0 * m.padding_x,
            m.control.max(f64::from(layout.height())),
        )
    }

    fn style(&self, _theme: &Theme) -> taffy::Style {
        taffy::Style {
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn set_bounds(&mut self, cx: &mut LayoutCx<'_>, bounds: Rect) {
        self.bounds = bounds;
        self.radius = cx.theme.metrics.radius;
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    /// The farthest shadow is the hover one, `3px 5px 10px` (σ = 5, cut off at
    /// 2.5 σ): 9.5 px left, 7.5 up, 15.5 right, 17.5 down; the ring adds 3 all round
    fn ink(&self) -> Rect {
        let b = self.bounds;
        Rect::new(b.x0 - 10.0, b.y0 - 8.0, b.x1 + 16.0, b.y1 + 18.0)
    }

    fn hit(&self, p: Point) -> bool {
        self.hit_point(p)
    }

    fn focusable(&self) -> bool {
        true
    }

    fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) {
        // Space and Enter press it; other keys go on to the ancestors
        if let Event::KeyDown { key, .. } | Event::KeyUp { key } = event
            && (is_space(key) || matches!(key, Key::Named(NamedKey::Enter)))
        {
            cx.handled = true;
        }
        match event {
            Event::PointerEnter => self.hovered = true,
            Event::PointerLeave => self.hovered = false,
            Event::PointerMove(p) => self.hovered = self.hit_point(*p),
            Event::PointerDown(p) => self.pointer_armed = self.hit_point(*p),
            Event::PointerUp(p) => {
                let click = self.pointer_armed && self.hit_point(*p);
                self.pointer_armed = false;
                if click {
                    self.activate();
                }
            }
            Event::KeyDown {
                key, repeat: false, ..
            } if is_space(key) => self.key_armed = true,
            Event::KeyUp { key } if is_space(key) && self.key_armed => {
                self.key_armed = false;
                self.activate();
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Enter),
                repeat: false,
                ..
            } => self.activate(),
            Event::Focus { focused, visible } => {
                self.focused = *focused;
                self.focus_visible = *visible;
                if !focused {
                    self.key_armed = false;
                }
            }
            Event::Activate => self.activate(),
            _ => {}
        }
        self.sync(cx.now, cx.theme);
    }

    fn cursor(&self) -> CursorIcon {
        match (self.hovered, self.shown_loading) {
            (true, true) => CursorIcon::Progress,
            (true, false) => CursorIcon::Pointer,
            _ => CursorIcon::Default,
        }
    }

    fn animating(&self, now: Instant, theme: &Theme) -> bool {
        let d = theme.metrics.duration;
        self.shown_loading
            || self.hover.animating(now, d)
            || self.press.animating(now, d)
            || self.ring.animating(now, d)
    }

    fn accessibility(&self) -> Option<accesskit::Node> {
        let mut node = accesskit::Node::new(accesskit::Role::Button);
        node.set_label(
            self.accessible_name
                .clone()
                .unwrap_or_else(|| self.label.clone()),
        );
        node.set_bounds(accessible_rect(self.bounds));
        node.add_action(accesskit::Action::Click);
        node.add_action(accesskit::Action::Focus);
        if self.shown_loading {
            node.set_busy();
        }
        Some(node)
    }

    fn paint(&self, cx: &mut PaintCx<'_>) {
        let (ctx, theme, now) = (&mut *cx.ctx, cx.theme, cx.now);
        let m = &theme.metrics;
        let (h, p) = (
            self.hover.value(now, m.duration),
            self.press.value(now, m.duration),
        );
        let (r, radius) = (self.bounds, m.radius);

        // focus ring: the last (lowest) of the button's box shadows
        paint::outer_shadows(
            ctx,
            r,
            radius,
            &[theme.focus_ring],
            self.ring.value(now, m.duration) as f32,
        );

        // shadows: rest ↔ hover move with the hover transition, the pressed
        // (inset) set fades in as the raised one fades out
        let raised = paint::lerp_shadows(&theme.control_shadow, &theme.control_shadow_hover, h);
        let (up, down) = ((1.0 - p) as f32, p as f32);
        paint::outer_shadows(ctx, r, radius, &raised, up);

        let [top, bottom] = [0, 1].map(|i| {
            let c = theme.control_bg[i].lerp_rect(theme.control_bg_hover[i], h as f32);
            mix(c, theme.surface_sunken, down)
        });
        ctx.set_paint(Gradient::new_linear((r.x0, r.y0), (r.x0, r.y1)).with_stops([top, bottom]));
        ctx.fill_path(&paint::rounded(r, radius));

        paint::inset_shadows(ctx, r, radius, &raised, up);
        paint::inset_shadows(ctx, r, radius, &theme.control_shadow_pressed, down);

        // content: [spinner][gap]label, centred; pressed it sinks a little
        let Some(layout) = &self.layout else { return };
        let em = f64::from(m.font_size);
        let spinner_w = if self.shown_loading {
            SPINNER_BOX_EM * em + m.gap
        } else {
            0.0
        };
        let content = Size::new(
            spinner_w + f64::from(layout.width()),
            f64::from(layout.height()),
        );
        let origin = r.center() - content.to_vec2() / 2.0;
        let centre = r.center();
        let base = *ctx.transform();
        let sink = Affine::translate(centre.to_vec2() + Vec2::new(0.0, 0.5 * p))
            * Affine::scale(1.0 - 0.015 * p)
            * Affine::translate(-centre.to_vec2());
        ctx.set_transform(base * sink);

        let fg = mix(theme.text_1, theme.accent_2, down);
        if self.shown_loading {
            let box_size = SPINNER_BOX_EM * em;
            let c = Point::new(origin.x + box_size / 2.0, centre.y);
            let turn = (now.duration_since(self.spin_start).as_secs_f64() / SPIN_PERIOD).fract();
            spinner(ctx, c, SPINNER_EM * em, turn, fg);
        }
        text::draw(
            ctx,
            cx.res,
            layout,
            Point::new(origin.x + spinner_w, origin.y),
            fg,
        );
        ctx.set_transform(base);
    }
}

/// winit reports Space as the character " ", not as a named key
fn is_space(key: &Key) -> bool {
    matches!(key, Key::Character(c) if c == " ")
}

/// Kiesel's Spinner: a 24-unit viewBox, circle r = 9, stroke 3; a faint track
/// and an 18-unit arc with round caps, turning by `turn` (0..1)
fn spinner(ctx: &mut RenderContext, centre: Point, size: f64, turn: f64, color: Color) {
    use std::f64::consts::TAU;
    let k = size / 24.0;
    let radius = 9.0 * k;
    ctx.set_stroke(Stroke::new(3.0 * k).with_caps(Cap::Round));
    let circle = |start: f64, sweep: f64| {
        Arc::new(centre, (radius, radius), start, sweep, 0.0).into_path(TOLERANCE)
    };
    ctx.set_paint(color.multiply_alpha(0.25));
    ctx.stroke_path(&circle(0.0, TAU));
    ctx.set_paint(color);
    ctx.stroke_path(&circle(turn * TAU, 18.0 / 9.0));
}

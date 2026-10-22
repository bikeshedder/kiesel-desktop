//! `Slider` – a number within a range: a crisp sunken track, filled with the
//! accent up to a round knob (see `kiesel/src/components/Slider`).
//!
//! A press on the track moves the knob there, dragging follows; the knob
//! swells a little while held. The arrows step (Left/Down less, Right/Up
//! more), PageUp/PageDown move a tenth of the range, Home/End go to the ends.
//! Put it in a [`Field`](crate::Field) for a label, hint and error; `show_value`
//! writes the value at the track's end. Like the web component it is
//! controlled: it shows `value` and reports the wish to change it. For
//! assistive technology it is a slider with value, limits and step.

use std::time::Instant;

use parley::Layout;
use reactive_graph::traits::{GetUntracked, Track};
use reactive_graph::wrappers::read::ArcSignal;
use vello_cpu::kurbo::{Point, Rect, Size};
use vello_cpu::peniko::Gradient;
use winit::cursor::CursorIcon;
use winit::keyboard::{Key, NamedKey};

use crate::anim::{Easing, Transition};
use crate::field::FieldLink;
use crate::i18n::{IntoText, TextProp};
use crate::paint;
use crate::reactive::{MountCx, Watch};
use crate::text;
use crate::theme::{Color, ControlSize, Shadow, TextRole, Theme, mix, space};
use crate::widget::{
    Available, Change, Event, EventCx, Known, LayoutCx, PaintCx, Widget, WidgetId, accessible_rect,
};

/// The knob swells in 200 ms with the spring
const SWELL: f64 = 0.2;

/// Track height and knob diameter
fn dims(size: ControlSize) -> (f64, f64) {
    match size {
        ControlSize::Xs | ControlSize::Sm => (6.0, 18.0),
        ControlSize::Md => (8.0, 22.0),
        ControlSize::Lg | ControlSize::Xl => (10.0, 28.0),
    }
}

type OnChange = Box<dyn FnMut(f64)>;
type Format = Box<dyn Fn(f64) -> String>;

pub struct Slider {
    id: WidgetId,
    value: ArcSignal<f64>,
    min: f64,
    max: f64,
    step: f64,
    size: ControlSize,
    on_change: Option<OnChange>,
    format: Option<Format>,
    label: Option<TextProp>,
    field: Option<FieldLink>,
    watch: Option<Watch>,
    /// `value` as of the last update
    shown: f64,
    /// The written value, and the room it takes (the widest of min and max)
    value_layout: Option<Layout<()>>,
    value_room: f64,
    invalid: bool,
    described: String,
    bounds: Rect,
    hovered: bool,
    dragging: bool,
    focused: bool,
    focus_visible: bool,
    swell: Transition,
    ring: Transition,
    repaint: bool,
}

impl Slider {
    /// Shows `value`, 0 to 100 in steps of 1 unless told otherwise
    pub fn new(value: impl Into<ArcSignal<f64>>) -> Self {
        let value = value.into();
        let shown = value.get_untracked();
        Self {
            id: WidgetId::next(),
            value,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            size: ControlSize::Md,
            on_change: None,
            format: None,
            label: None,
            field: None,
            watch: None,
            shown,
            value_layout: None,
            value_room: 0.0,
            invalid: false,
            described: String::new(),
            bounds: Rect::ZERO,
            hovered: false,
            dragging: false,
            focused: false,
            focus_visible: false,
            swell: Transition::new(0.0).with_easing(Easing::SPRING),
            ring: Transition::new(0.0),
            repaint: false,
        }
    }

    /// The range, ends included (default 0 to 100)
    pub fn range(mut self, min: f64, max: f64) -> Self {
        (self.min, self.max) = (min.min(max), max.max(min));
        self
    }

    /// The smallest change (default 1); its decimals are the value's
    pub fn step(mut self, step: f64) -> Self {
        self.step = step.abs().max(f64::EPSILON);
        self
    }

    /// Sm, Md (default), Lg: track and knob
    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    /// Asked to change; the owner of `value` decides
    pub fn on_change(mut self, f: impl FnMut(f64) + 'static) -> Self {
        self.on_change = Some(Box::new(f));
        self
    }

    /// Writes the value at the track's end – "40 %", "1.5 s"
    pub fn show_value(mut self, format: impl Fn(f64) -> String + 'static) -> Self {
        self.format = Some(Box::new(format));
        self
    }

    /// Its accessible name without a visible label; otherwise put it in a `Field`
    pub fn label(mut self, text: impl IntoText) -> Self {
        self.label = Some(TextProp::new(text));
        self
    }

    /// Snapped to the step, within the range
    fn snap(&self, v: f64) -> f64 {
        let steps = ((v - self.min) / self.step).round();
        let v = self.min + steps * self.step;
        let decimals = format!("{}", self.step)
            .split('.')
            .nth(1)
            .map_or(0, str::len)
            .min(9) as i32;
        let k = 10f64.powi(decimals);
        ((v * k).round() / k).clamp(self.min, self.max)
    }

    fn change(&mut self, to: f64) {
        let to = self.snap(to);
        if to != self.shown
            && let Some(f) = &mut self.on_change
        {
            f(to);
        }
    }

    /// The track: the widget's width less the written value, centred
    fn track(&self) -> Rect {
        let (h, _) = dims(self.size);
        let b = self.bounds;
        let end = if self.format.is_some() {
            self.value_room + space(3)
        } else {
            0.0
        };
        let y = b.center().y - h / 2.0;
        Rect::new(b.x0, y, b.x1 - end, y + h)
    }

    /// Where the knob's centre can go: half a knob in from the track's ends
    fn travel(&self) -> (f64, f64) {
        let (_, knob) = dims(self.size);
        let t = self.track();
        (
            t.x0 + knob / 2.0,
            (t.x1 - knob / 2.0).max(t.x0 + knob / 2.0),
        )
    }

    /// Where along the track `v` is (0 … 1)
    fn fraction(&self, v: f64) -> f64 {
        if self.max > self.min {
            ((v - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    fn value_at(&self, x: f64) -> f64 {
        let (a, b) = self.travel();
        let f = ((x - a) / (b - a).max(1.0)).clamp(0.0, 1.0);
        self.min + f * (self.max - self.min)
    }

    fn knob_rect(&self, v: f64) -> Rect {
        let (_, knob) = dims(self.size);
        let (a, b) = self.travel();
        let x = a + self.fraction(v) * (b - a);
        Rect::from_center_size((x, self.track().center().y), (knob, knob))
    }

    fn sync(&mut self, now: Instant, theme: &Theme) {
        let on = |b: bool| f64::from(u8::from(b));
        let mut changed = self.swell.set(on(self.dragging), now, theme.motion(SWELL));
        changed |= self.ring.set(
            on(self.focused && self.focus_visible),
            now,
            theme.metrics.duration,
        );
        self.repaint |= changed;
    }

    fn text(&self, v: f64) -> String {
        self.format
            .as_ref()
            .map_or_else(|| format!("{v}"), |f| f(v))
    }
}

impl Widget for Slider {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn link_field(&mut self, link: FieldLink) {
        self.field = Some(link);
    }

    fn mount(&mut self, cx: &mut MountCx<'_>) {
        let value = self.value.clone();
        let label = self.label.as_ref().map(TextProp::signal);
        let field = self.field.clone();
        self.watch = Some(cx.watch(self.id, move || {
            value.track();
            if let Some(l) = &label {
                l.track();
            }
            if let Some(f) = &field {
                f.invalid.track();
                f.description.track();
            }
        }));
    }

    fn update(&mut self, cx: &mut LayoutCx<'_>) -> Change {
        let mut change = Change::NONE;
        let v = self.value.get_untracked();
        if v != self.shown || self.value_layout.is_none() {
            self.shown = v;
            if self.format.is_some() {
                let style = cx.theme.text_style(TextRole::FieldLabel);
                let room = [self.min, self.max]
                    .map(|e| f64::from(cx.text.line(&self.text(e), &style).width()))
                    .into_iter()
                    .fold(0.0, f64::max);
                self.value_layout = Some(cx.text.line(&self.text(v), &style));
                if room != self.value_room {
                    self.value_room = room;
                    change = Change::LAYOUT;
                }
            }
            self.repaint = true;
        }
        if let Some(f) = &self.field {
            let state = (f.invalid.get_untracked(), f.description.get_untracked());
            if (state.0, &state.1) != (self.invalid, &self.described) {
                (self.invalid, self.described) = state;
                self.repaint = true;
            }
        }
        if let Some(l) = &mut self.label {
            self.repaint |= l.refresh();
        }
        if std::mem::take(&mut self.repaint) {
            change |= Change::PAINT;
        }
        change
    }

    /// As wide as offered, 240 px on its own; as high as the knob
    fn measure(&mut self, _cx: &mut LayoutCx<'_>, known: Known, available: Available) -> Size {
        let width = known.width.unwrap_or(match available.width {
            taffy::AvailableSpace::MinContent => 120.0,
            taffy::AvailableSpace::MaxContent => 240.0,
            taffy::AvailableSpace::Definite(w) => w,
        });
        Size::new(f64::from(width), dims(self.size).1 + 6.0)
    }

    fn style(&self, _theme: &Theme) -> taffy::Style {
        taffy::Style {
            min_size: taffy::Size {
                width: taffy::LengthPercentageAuto::length(0.0),
                height: taffy::LengthPercentageAuto::auto(),
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

    /// The knob's shadow, its swelling and the focus ring reach out
    fn ink(&self) -> Rect {
        self.bounds.inflate(10.0, 10.0)
    }

    fn focusable(&self) -> bool {
        true
    }

    fn cursor(&self) -> CursorIcon {
        CursorIcon::Pointer
    }

    fn animating(&self, now: Instant, theme: &Theme) -> bool {
        self.swell.animating(now, theme.motion(SWELL))
            || self.ring.animating(now, theme.metrics.duration)
    }

    fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) {
        let range = self.max - self.min;
        let page = (range / 10.0).max(self.step);
        match event {
            Event::PointerEnter => self.hovered = true,
            Event::PointerLeave => self.hovered = false,
            Event::PointerDown(p) => {
                self.dragging = true;
                self.change(self.value_at(p.x));
            }
            Event::PointerMove(p) if self.dragging => self.change(self.value_at(p.x)),
            Event::PointerUp(_) => self.dragging = false,
            Event::KeyDown { key, .. } => {
                let to = match key {
                    Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowDown) => self.shown - self.step,
                    Key::Named(NamedKey::ArrowRight | NamedKey::ArrowUp) => self.shown + self.step,
                    Key::Named(NamedKey::PageDown) => self.shown - page,
                    Key::Named(NamedKey::PageUp) => self.shown + page,
                    Key::Named(NamedKey::Home) => self.min,
                    Key::Named(NamedKey::End) => self.max,
                    _ => {
                        self.sync(cx.now, cx.theme);
                        return;
                    }
                };
                cx.handled = true;
                self.change(to);
            }
            Event::Step(dir) => self.change(self.shown + f64::from(*dir) * self.step),
            Event::SetValue(v) => {
                if let Some(n) = crate::number_field::parse(v) {
                    self.change(n);
                }
            }
            Event::Focus { focused, visible } => {
                self.focused = *focused;
                self.focus_visible = *visible;
                if !focused {
                    self.dragging = false;
                }
            }
            _ => {}
        }
        self.sync(cx.now, cx.theme);
    }

    fn accessibility(&self) -> Option<accesskit::Node> {
        let mut node = accesskit::Node::new(accesskit::Role::Slider);
        if let Some(l) = self.label.as_ref().filter(|l| !l.shown.is_empty()) {
            node.set_label(l.shown.clone());
        }
        if let Some(f) = &self.field {
            node.push_labelled_by(f.label.node());
            if f.required {
                node.set_required();
            }
        }
        if self.invalid {
            node.set_invalid(accesskit::Invalid::True);
        }
        if !self.described.is_empty() {
            node.set_description(self.described.clone());
        }
        node.set_numeric_value(self.shown);
        node.set_min_numeric_value(self.min);
        node.set_max_numeric_value(self.max);
        node.set_numeric_value_step(self.step);
        node.set_value(self.text(self.shown));
        node.set_bounds(accessible_rect(self.bounds));
        for a in [
            accesskit::Action::Focus,
            accesskit::Action::Increment,
            accesskit::Action::Decrement,
            accesskit::Action::SetValue,
        ] {
            node.add_action(a);
        }
        Some(node)
    }

    fn paint(&self, cx: &mut PaintCx<'_>) {
        let (ctx, theme, now) = (&mut *cx.ctx, cx.theme, cx.now);
        let t = self.track();
        let radius = t.height() / 2.0;
        // the track: a crisp well (red edged with an error)
        let well = if self.invalid {
            &theme.field_shadow_invalid
        } else {
            &theme.field_shadow
        };
        paint::outer_shadows(ctx, t, radius, well, 1.0);
        ctx.set_paint(theme.field_bg);
        ctx.fill_path(&paint::rounded(t, radius));
        // the fill up to the knob: the accent, lighter at its top
        let knob = self.knob_rect(self.shown);
        let fill = Rect::new(t.x0, t.y0, knob.center().x, t.y1);
        if fill.width() > 0.5 {
            ctx.set_paint(
                Gradient::new_linear((0.0, t.y0), (0.0, t.y1))
                    .with_stops([mix(theme.accent, Color::WHITE, 0.25), theme.accent]),
            );
            ctx.fill_path(&paint::rounded(fill, radius));
        }
        paint::inset_shadows(ctx, t, radius, well, 1.0);

        // the knob: --ksl-knob, --ksl-knob-shadow-sm, swelling 8 % while held
        let k = Rect::from_center_size(
            knob.center(),
            (knob.size().to_vec2() * (1.0 + 0.08 * self.swell.value(now, theme.motion(SWELL))))
                .to_size(),
        );
        let ring = self.ring.value(now, theme.metrics.duration) as f32;
        paint::outer_shadows(ctx, k, k.width() / 2.0, &[theme.focus_ring], ring);
        let shadows = [
            Shadow {
                inset: true,
                x: 0.0,
                y: 1.0,
                blur: 0.0,
                spread: 0.0,
                color: theme.field_shadow[2].color,
            },
            Shadow {
                inset: false,
                x: 0.0,
                y: 0.0,
                blur: 0.0,
                spread: 0.5,
                color: theme.field_shadow[0].color,
            },
            Shadow {
                inset: false,
                x: 0.0,
                y: 1.0,
                blur: 2.0,
                spread: 0.0,
                color: theme.lo_strong,
            },
            Shadow {
                inset: false,
                x: 0.0,
                y: 2.0,
                blur: 5.0,
                spread: 0.0,
                color: theme.lo_soft,
            },
        ];
        let r = k.width() / 2.0;
        paint::outer_shadows(ctx, k, r, &shadows, 1.0);
        // linear-gradient(145deg, …)
        let a = 145f64.to_radians();
        let dir = vello_cpu::kurbo::Vec2::new(a.sin(), -a.cos());
        let half = (k.width() * a.sin().abs() + k.height() * a.cos().abs()) / 2.0;
        let c = k.center();
        ctx.set_paint(
            Gradient::new_linear(c - dir * half, c + dir * half).with_stops([
                (0.0, theme.knob[0]),
                (0.55, theme.knob[1]),
                (1.0, theme.knob[2]),
            ]),
        );
        ctx.fill_path(&paint::rounded(k, r));
        paint::inset_shadows(ctx, k, r, &shadows, 1.0);

        // the written value, at the end
        if let Some(layout) = &self.value_layout {
            let w = f64::from(layout.width());
            let p = Point::new(
                self.bounds.x1 - w,
                self.bounds.center().y - f64::from(layout.height()) / 2.0,
            );
            text::draw(ctx, cx.res, layout, p, theme.text_1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_snap_to_the_step_within_the_range() {
        let s = Slider::new(0.0).range(0.0, 1.0).step(0.1);
        assert_eq!(s.snap(0.26), 0.3);
        assert_eq!(s.snap(0.1 + 0.2), 0.3);
        assert_eq!(s.snap(1.4), 1.0);
        assert_eq!(s.snap(-3.0), 0.0);
        let s = Slider::new(0.0).range(10.0, 20.0).step(5.0);
        assert_eq!(s.snap(13.0), 15.0);
    }
}

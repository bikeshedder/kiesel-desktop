//! On/off controls with a label: [`Checkbox`], [`Switch`] and the rows of a
//! [`RadioGroup`](crate::RadioGroup) – one widget, [`Toggle`], in three looks
//! (see `kiesel/src/components/Checkbox`, `Switch` and `Radio`).
//!
//! The whole row – indicator, label, description – is one control: a press
//! anywhere on it toggles, Space toggles (on release, pressed look while held),
//! and so does the assistive technology's click. The label and description are
//! its name and description for assistive technology. Like the web components
//! they are controlled: they show `checked` and report the wish to change it.
//!
//! A checkbox fills with the accent and shows a tick (a dash when
//! indeterminate); a radio's knob springs in; a switch's knob slides and
//! stretches while pressed. With an error the indicator gets a red edge and the
//! label turns red.

use std::time::Instant;

use reactive_graph::traits::{Get, GetUntracked, Track, With};
use reactive_graph::wrappers::read::ArcSignal;
use vello_cpu::RenderContext;
use vello_cpu::kurbo::{Affine, Circle, Point, Rect, RoundedRect, Shape, Size, Stroke, Vec2};
use vello_cpu::peniko::Gradient;
use winit::cursor::CursorIcon;
use winit::keyboard::Key;

use crate::anim::{Easing, Transition};
use crate::control::Show;
use crate::flex::{Align, Flex};
use crate::i18n::{IntoText, TextProp};
use crate::icon::{Icon, Tone};
use crate::label::Label;
use crate::lucide;
use crate::paint::{self, TOLERANCE};
use crate::reactive::{MountCx, Watch};
use crate::theme::{Color, ControlSize, Shadow, TextRole, Theme, mix, space};
use crate::widget::{Change, Event, EventCx, LayoutCx, PaintCx, Widget, WidgetId, accessible_rect};

/// Which control it is
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Checkbox,
    Switch,
    Radio,
}

/// A checkbox fills in 200 ms, its tick springs in over 260 ms; a radio's knob
/// springs in 280 ms; a switch's knob slides in 380 ms and stretches in 200 ms
const FILL: f64 = 0.2;
const TICK: f64 = 0.26;
const DOT: f64 = 0.28;
const SLIDE: f64 = 0.38;
const STRETCH: f64 = 0.2;

/// Switch: track height, width, knob inset, shadow depth
fn switch_dims(size: ControlSize) -> (f64, f64, f64, f64) {
    match size {
        ControlSize::Xs => (16.0, 30.0, 2.0, 1.0),
        ControlSize::Sm => (22.0, 40.0, 2.0, 1.5),
        ControlSize::Md => (28.0, 54.0, 3.0, 2.0),
        ControlSize::Lg => (36.0, 72.0, 4.0, 3.0),
        ControlSize::Xl => (64.0, 150.0, 5.0, 5.0),
    }
}

/// Checkbox and radio: box (or dot) edge and tick size
fn box_dims(size: ControlSize) -> (f64, f64) {
    match size {
        ControlSize::Xs | ControlSize::Sm => (18.0, 12.0),
        ControlSize::Md => (22.0, 14.0),
        ControlSize::Lg | ControlSize::Xl => (26.0, 17.0),
    }
}

type OnChange = Box<dyn FnMut(bool)>;

/// A checkbox, switch or radio with its label – made by [`Checkbox::new`],
/// [`Switch::new`] or a [`RadioGroup`](crate::RadioGroup)
pub struct Toggle {
    id: WidgetId,
    kind: Kind,
    size: ControlSize,
    checked: ArcSignal<bool>,
    /// Checkbox: neither on nor off (some of a group)
    mixed: Option<ArcSignal<bool>>,
    watch: Option<Watch>,
    /// `checked` and `mixed` as of the last update – what is on screen
    shown: bool,
    shown_mixed: bool,
    on_change: Option<OnChange>,
    label: TextProp,
    description: TextProp,
    /// A name for assistive technology without a visible label
    accessible_name: Option<TextProp>,
    /// Its own error (a consent that must be given)
    error: Option<ArcSignal<String>>,
    /// Its group's error state (a radio, a checkbox of a group)
    group_invalid: Option<ArcSignal<bool>>,
    /// Some error, as of the last update
    invalid: bool,
    /// What assistive technology reads after the name: the error, or else the description
    described: String,
    /// The label before the control, the control at the end (a settings row)
    label_first: bool,
    /// A radio: the first of its group, and whether any of the group is checked
    /// – only the checked one (or else the first) is in the Tab order
    roving: Option<(bool, ArcSignal<bool>)>,
    tab_stop: bool,
    /// Until mounted: not yet made
    built: bool,
    /// The text: label, description, error
    children: Vec<Box<dyn Widget>>,
    bounds: Rect,
    hovered: bool,
    pointer_armed: bool,
    key_armed: bool,
    focused: bool,
    focus_visible: bool,
    /// Checkbox fill, radio dot, switch knob position: 0 off … 1 on
    check: Transition,
    /// The checkbox's tick
    tick: Transition,
    /// The switch's knob stretching while pressed
    press: Transition,
    ring: Transition,
    repaint: bool,
}

impl Toggle {
    fn new(kind: Kind, checked: ArcSignal<bool>) -> Self {
        let shown = checked.get_untracked();
        let on = f64::from(u8::from(shown));
        let check = match kind {
            Kind::Checkbox => Transition::new(on),
            Kind::Switch | Kind::Radio => Transition::new(on).with_easing(Easing::SPRING),
        };
        Self {
            id: WidgetId::next(),
            kind,
            size: ControlSize::Md,
            checked,
            mixed: None,
            watch: None,
            shown,
            shown_mixed: false,
            on_change: None,
            label: TextProp::default(),
            description: TextProp::default(),
            accessible_name: None,
            error: None,
            group_invalid: None,
            invalid: false,
            described: String::new(),
            label_first: false,
            roving: None,
            tab_stop: true,
            built: false,
            children: Vec::new(),
            bounds: Rect::ZERO,
            hovered: false,
            pointer_armed: false,
            key_armed: false,
            focused: false,
            focus_visible: false,
            check,
            tick: Transition::new(on).with_easing(Easing::SPRING),
            press: Transition::new(0.0),
            ring: Transition::new(0.0),
            repaint: false,
        }
    }

    /// Asked to change: `true` to turn it on (a radio: to choose it); the owner
    /// of `checked` decides
    pub fn on_change(mut self, f: impl FnMut(bool) + 'static) -> Self {
        self.on_change = Some(Box::new(f));
        self
    }

    /// The text next to it – also its name for assistive technology
    pub fn label(mut self, text: impl IntoText) -> Self {
        self.label = TextProp::new(text);
        self
    }

    /// A second, quieter line under the label
    pub fn description(mut self, text: impl IntoText) -> Self {
        self.description = TextProp::new(text);
        self
    }

    /// A name for assistive technology, for one without a visible label
    pub fn accessible_name(mut self, text: impl IntoText) -> Self {
        self.accessible_name = Some(TextProp::new(text));
        self
    }

    /// Xs and Sm, Md (default), Lg and Xl
    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    /// An error under the label, empty while all is fine – a consent that must
    /// be given: red edge, red label, the message with a warning icon
    pub fn error(mut self, error: impl IntoText) -> Self {
        self.error = Some(error.into_text());
        self
    }

    /// The label first, the control at the end of the row – a settings row
    pub fn label_first(mut self) -> Self {
        self.label_first = true;
        self
    }

    pub(crate) fn group_invalid(mut self, invalid: ArcSignal<bool>) -> Self {
        self.group_invalid = Some(invalid);
        self
    }

    pub(crate) fn roving(mut self, first: bool, any_checked: ArcSignal<bool>) -> Self {
        self.roving = Some((first, any_checked));
        self
    }

    fn indicator(&self) -> Size {
        match self.kind {
            Kind::Switch => {
                let (h, w, _, _) = switch_dims(self.size);
                Size::new(w, h)
            }
            Kind::Checkbox | Kind::Radio => {
                let (d, _) = box_dims(self.size);
                Size::new(d, d)
            }
        }
    }

    fn gap(&self) -> f64 {
        match (self.kind, self.size) {
            (Kind::Switch, _) => space(3),
            (_, ControlSize::Xs | ControlSize::Sm) => space(2),
            _ => space(3),
        }
    }

    fn label_role(&self) -> TextRole {
        match self.size {
            ControlSize::Xs | ControlSize::Sm => TextRole::Small,
            ControlSize::Md => TextRole::Body,
            ControlSize::Lg | ControlSize::Xl => TextRole::Large,
        }
    }

    fn has_text(&self) -> bool {
        !self.children.is_empty()
    }

    /// The label (red with an error), the description, the error
    fn build(&mut self) {
        let own_error = self.error.clone();
        let own_invalid = own_error
            .clone()
            .map(|e| ArcSignal::derive(move || e.with(|e| !e.is_empty())));
        let invalid = match (own_invalid.clone(), self.group_invalid.clone()) {
            (Some(a), Some(b)) => Some(ArcSignal::derive(move || a.get() || b.get())),
            (a, b) => a.or(b),
        };
        let has_label = !self.label.shown.is_empty();
        if !has_label && self.error.is_none() {
            return;
        }
        let mut column = Flex::stack().gap(2.0).align(Align::Stretch);
        if has_label {
            let mut label = Label::new(self.label.signal())
                .role(self.label_role())
                .hidden()
                .tone(match invalid.clone() {
                    Some(invalid) => ArcSignal::derive(move || {
                        if invalid.get() {
                            Tone::Danger
                        } else {
                            Tone::Text(TextRole::Title)
                        }
                    }),
                    None => ArcSignal::stored(Tone::Text(TextRole::Title)),
                });
            if self.kind == Kind::Switch {
                label = label.weight(500.0);
            }
            column = column.child(label);
        }
        if !self.description.shown.is_empty() {
            let role = match self.size {
                ControlSize::Lg | ControlSize::Xl => TextRole::Body,
                _ => TextRole::Caption,
            };
            column = column.child(
                Label::new(self.description.signal())
                    .role(role)
                    .tone(Tone::Text(TextRole::Caption))
                    .hidden(),
            );
        }
        if let (Some(error), Some(invalid)) = (own_error, own_invalid) {
            column = column.child(Show::new(invalid, move || {
                Flex::cluster()
                    .gap(space(1))
                    .align(Align::Center)
                    .child(
                        Icon::new(lucide::TRIANGLE_ALERT)
                            .size(14.0)
                            .tone(Tone::Danger),
                    )
                    .child(Label::new(error.clone()).role(TextRole::Error).hidden())
            }));
        }
        if self.label_first {
            column = column.grow();
        }
        self.children.push(Box::new(column));
    }

    fn toggle(&mut self) {
        let to = match self.kind {
            Kind::Radio if self.shown => return,
            Kind::Radio => true,
            _ => self.shown_mixed || !self.shown,
        };
        if let Some(f) = &mut self.on_change {
            f(to);
        }
    }

    fn sync(&mut self, now: Instant, theme: &Theme) {
        let on = |b: bool| f64::from(u8::from(b));
        let lit = on(self.shown || self.shown_mixed);
        let pressed = (self.hovered && self.pointer_armed) || self.key_armed;
        let duration = match self.kind {
            Kind::Checkbox => FILL,
            Kind::Radio => DOT,
            Kind::Switch => SLIDE,
        };
        let mut changed = self.check.set(lit, now, theme.motion(duration));
        changed |= self.tick.set(lit, now, theme.motion(TICK));
        changed |= self.press.set(on(pressed), now, theme.motion(STRETCH));
        changed |= self.ring.set(
            on(self.focused && self.focus_visible),
            now,
            theme.metrics.duration,
        );
        self.repaint |= changed;
    }

    /// Where the indicator is: at the start (the end, label first), level with
    /// the first line (a switch: in the middle)
    fn indicator_rect(&self, theme: &Theme) -> Rect {
        let size = self.indicator();
        let b = self.bounds;
        let x = if self.label_first {
            b.x1 - size.width
        } else {
            b.x0
        };
        let y = match self.kind {
            Kind::Switch => b.center().y - size.height / 2.0,
            _ => b.y0 + ((self.line(theme) - size.height) / 2.0).max(0.0),
        };
        Rect::from_origin_size((x, y), size)
    }

    /// The height of the label's first line
    fn line(&self, theme: &Theme) -> f64 {
        let style = theme.text_style(self.label_role());
        f64::from(style.size * style.line_height)
    }
}

/// A single option, on or off – a consent, a setting
pub struct Checkbox;

impl Checkbox {
    /// Shows `checked`; say what to do on a change with [`Toggle::on_change`]
    #[allow(clippy::new_ret_no_self)]
    pub fn new(checked: impl Into<ArcSignal<bool>>) -> Toggle {
        Toggle::new(Kind::Checkbox, checked.into())
    }
}

impl Toggle {
    /// A checkbox neither on nor off: some of the things it stands for are
    /// ("select all" of a group)
    pub fn indeterminate(mut self, mixed: impl Into<ArcSignal<bool>>) -> Self {
        self.mixed = Some(mixed.into());
        self
    }
}

/// A setting that takes effect at once: a knob in a sunken pill
pub struct Switch;

impl Switch {
    /// Shows `checked`; say what to do on a change with [`Toggle::on_change`]
    #[allow(clippy::new_ret_no_self)]
    pub fn new(checked: impl Into<ArcSignal<bool>>) -> Toggle {
        Toggle::new(Kind::Switch, checked.into())
    }
}

pub(crate) fn radio(checked: ArcSignal<bool>) -> Toggle {
    Toggle::new(Kind::Radio, checked)
}

impl Widget for Toggle {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut [Box<dyn Widget>] {
        &mut self.children
    }

    fn mount(&mut self, cx: &mut MountCx<'_>) {
        if !std::mem::replace(&mut self.built, true) {
            self.build();
        }
        let checked = self.checked.clone();
        let mixed = self.mixed.clone();
        let roving = self.roving.as_ref().map(|(_, any)| any.clone());
        let (label, description) = (self.label.signal(), self.description.signal());
        let name = self.accessible_name.as_ref().map(TextProp::signal);
        let (error, group) = (self.error.clone(), self.group_invalid.clone());
        self.watch = Some(cx.watch(self.id, move || {
            checked.track();
            for s in [&mixed, &group].into_iter().flatten() {
                s.track();
            }
            if let Some(any) = &roving {
                any.track();
            }
            label.track();
            description.track();
            for s in [&name, &error].into_iter().flatten() {
                s.track();
            }
        }));
    }

    fn update(&mut self, cx: &mut LayoutCx<'_>) -> Change {
        let checked = self.checked.get_untracked();
        let mixed = self.mixed.as_ref().is_some_and(GetUntracked::get_untracked);
        if (checked, mixed) != (self.shown, self.shown_mixed) {
            (self.shown, self.shown_mixed) = (checked, mixed);
            self.sync(cx.now, cx.theme);
        }
        self.tab_stop = match &self.roving {
            Some((first, any)) => self.shown || (*first && !any.get_untracked()),
            None => true,
        };
        let error = self
            .error
            .as_ref()
            .map(GetUntracked::get_untracked)
            .unwrap_or_default();
        let invalid = !error.is_empty()
            || self
                .group_invalid
                .as_ref()
                .is_some_and(GetUntracked::get_untracked);
        self.repaint |= self.label.refresh();
        self.repaint |= self.description.refresh();
        if let Some(name) = &mut self.accessible_name {
            self.repaint |= name.refresh();
        }
        let described = if error.is_empty() {
            self.description.shown.clone()
        } else {
            error
        };
        if (invalid, &described) != (self.invalid, &self.described) {
            (self.invalid, self.described) = (invalid, described);
            self.repaint = true;
        }
        if std::mem::take(&mut self.repaint) {
            Change::PAINT
        } else {
            Change::NONE
        }
    }

    /// A row: room for the indicator, then the text; at least as high as the
    /// indicator, the text level with it
    fn style(&self, theme: &Theme) -> taffy::Style {
        let size = self.indicator();
        let reserve = if self.has_text() {
            size.width + self.gap()
        } else {
            size.width
        };
        let px = |v: f64| taffy::LengthPercentage::length(v as f32);
        let top = match self.kind {
            Kind::Switch => 0.0,
            _ => ((size.height - self.line(theme)) / 2.0).max(0.0),
        };
        taffy::Style {
            display: taffy::Display::Flex,
            flex_direction: taffy::FlexDirection::Row,
            align_items: match self.kind {
                Kind::Switch => taffy::AlignItems::CENTER,
                _ => taffy::AlignItems::START,
            },
            flex_shrink: 0.0,
            padding: taffy::Rect {
                left: px(if self.label_first { 0.0 } else { reserve }),
                right: px(if self.label_first { reserve } else { 0.0 }),
                top: px(top),
                bottom: px(0.0),
            },
            min_size: taffy::Size {
                width: taffy::LengthPercentageAuto::length(0.0),
                height: taffy::LengthPercentageAuto::length(size.height as f32),
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

    /// The ring and the knob's shadow reach past the row
    fn ink(&self) -> Rect {
        let d = match self.kind {
            Kind::Switch => switch_dims(self.size).3,
            _ => 1.0,
        };
        self.bounds.inflate(8.0 + d * 3.0, 8.0 + d * 3.0)
    }

    fn hit(&self, p: Point) -> bool {
        self.bounds.contains(p)
    }

    /// The label is part of the control: presses on it are the control's
    fn claims(&self, _p: Point) -> bool {
        true
    }

    fn focusable(&self) -> bool {
        true
    }

    fn tab_stop(&self) -> bool {
        self.tab_stop
    }

    fn cursor(&self) -> CursorIcon {
        CursorIcon::Pointer
    }

    fn animating(&self, now: Instant, theme: &Theme) -> bool {
        let duration = match self.kind {
            Kind::Checkbox => FILL,
            Kind::Radio => DOT,
            Kind::Switch => SLIDE,
        };
        self.check.animating(now, theme.motion(duration))
            || self.tick.animating(now, theme.motion(TICK))
            || self.press.animating(now, theme.motion(STRETCH))
            || self.ring.animating(now, theme.metrics.duration)
    }

    fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) {
        let space = |k: &Key| matches!(k, Key::Character(c) if c == " ");
        // Space toggles; other keys go on to the ancestors (arrows to a radio group)
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
        let mut node = accesskit::Node::new(match self.kind {
            Kind::Checkbox => accesskit::Role::CheckBox,
            Kind::Switch => accesskit::Role::Switch,
            Kind::Radio => accesskit::Role::RadioButton,
        });
        let name = self.accessible_name.as_ref().unwrap_or(&self.label);
        if !name.shown.is_empty() {
            node.set_label(name.shown.clone());
        }
        if !self.described.is_empty() {
            node.set_description(self.described.clone());
        }
        node.set_toggled(match (self.shown_mixed, self.shown) {
            (true, _) => accesskit::Toggled::Mixed,
            (false, true) => accesskit::Toggled::True,
            (false, false) => accesskit::Toggled::False,
        });
        if self.invalid {
            node.set_invalid(accesskit::Invalid::True);
        }
        node.set_bounds(accessible_rect(self.bounds));
        node.add_action(accesskit::Action::Click);
        node.add_action(accesskit::Action::Focus);
        Some(node)
    }

    fn paint(&self, cx: &mut PaintCx<'_>) {
        let (now, theme) = (cx.now, cx.theme);
        let r = self.indicator_rect(theme);
        let ring = self.ring.value(now, theme.metrics.duration) as f32;
        let ctx = &mut *cx.ctx;
        match self.kind {
            Kind::Checkbox => {
                let fill = self.check.value(now, theme.motion(FILL));
                let tick = self.tick.value(now, theme.motion(TICK));
                paint_checkbox(
                    ctx,
                    theme,
                    r,
                    self.size,
                    fill,
                    tick,
                    self.shown_mixed,
                    self.invalid,
                    ring,
                );
            }
            Kind::Radio => {
                let dot = self.check.value(now, theme.motion(DOT));
                paint_radio(ctx, theme, r, self.size, dot, self.invalid, ring);
            }
            Kind::Switch => {
                let pos = self.check.value(now, theme.motion(SLIDE));
                let press = self.press.value(now, theme.motion(STRETCH));
                paint_switch(ctx, theme, r, self.size, pos, press, self.invalid, ring);
            }
        }
        cx.paint_children(&self.children);
    }
}

fn shade(x: f64, y: f64, blur: f64, spread: f64, inset: bool, color: Color) -> Shadow {
    Shadow {
        inset,
        x,
        y,
        blur,
        spread,
        color,
    }
}

/// The focus outline: 3 px wide, 2 px off the shape (`outline-offset: 2px`)
fn focus_outline(ctx: &mut RenderContext, theme: &Theme, r: Rect, radius: f64, ring: f32) {
    if ring > 0.0 {
        ctx.set_stroke(Stroke::new(3.0));
        ctx.set_paint(theme.accent.multiply_alpha(0.45 * ring));
        ctx.stroke_path(&paint::rounded(r.inflate(3.5, 3.5), radius + 3.5));
    }
}

/// CSS `linear-gradient(145deg, …)` over `r`: start and end of the line
fn line_145(r: Rect) -> (Point, Point) {
    let a = 145f64.to_radians();
    let dir = Vec2::new(a.sin(), -a.cos());
    let half = (r.width() * a.sin().abs() + r.height() * a.cos().abs()) / 2.0;
    let c = r.center();
    (c - dir * half, c + dir * half)
}

/// The round knob of switches and radios: `--ksl-knob`
fn knob(ctx: &mut RenderContext, theme: &Theme, r: Rect, shadows: &[Shadow]) {
    let radius = r.width().min(r.height()) / 2.0;
    paint::outer_shadows(ctx, r, radius, shadows, 1.0);
    let (a, b) = line_145(r);
    ctx.set_paint(Gradient::new_linear(a, b).with_stops([
        (0.0, theme.knob[0]),
        (0.55, theme.knob[1]),
        (1.0, theme.knob[2]),
    ]));
    ctx.fill_path(&RoundedRect::from_rect(r, radius).to_path(TOLERANCE));
    paint::inset_shadows(ctx, r, radius, shadows, 1.0);
}

/// A well that fills with the accent (`fill`), and the tick (or dash) springing in
#[allow(clippy::too_many_arguments)]
fn paint_checkbox(
    ctx: &mut RenderContext,
    theme: &Theme,
    r: Rect,
    size: ControlSize,
    fill: f64,
    tick: f64,
    mixed: bool,
    invalid: bool,
    ring: f32,
) {
    let radius = match size {
        ControlSize::Lg | ControlSize::Xl => theme.metrics.radius,
        _ => theme.metrics.radius_sm,
    };
    focus_outline(ctx, theme, r, radius, ring);
    // the empty well
    let well = if invalid {
        &theme.field_shadow_invalid
    } else {
        &theme.field_shadow
    };
    paint::outer_shadows(ctx, r, radius, well, 1.0);
    ctx.set_paint(theme.field_bg);
    ctx.fill_path(&paint::rounded(r, radius));
    paint::inset_shadows(ctx, r, radius, well, 1.0);
    // filled: the accent from light to deep, a light top edge, a short shadow
    let a = fill.clamp(0.0, 1.0) as f32;
    if a > 0.0 {
        let filled = [
            shade(0.0, 1.0, 0.0, 0.0, true, Color::new([1.0, 1.0, 1.0, 0.28])),
            shade(
                0.0,
                1.0,
                2.0,
                0.0,
                false,
                mix(theme.accent_fill_2, Color::BLACK, 0.4),
            ),
            theme.field_shadow[2],
        ];
        paint::outer_shadows(ctx, r, radius, &filled, a);
        let (p0, p1) = line_145(r);
        let light = mix(theme.accent_fill, Color::WHITE, 0.15);
        ctx.set_paint(Gradient::new_linear(p0, p1).with_stops([
            (0.0, light.multiply_alpha(a)),
            (1.0, theme.accent_fill_2.multiply_alpha(a)),
        ]));
        ctx.fill_path(&paint::rounded(r, radius));
        paint::inset_shadows(ctx, r, radius, &filled, a);
    }
    // the tick: fades in over the first 160 ms, grows from 40 % with the spring
    let (_, mark) = box_dims(size);
    let opacity = (tick * TICK / 0.16).clamp(0.0, 1.0) as f32;
    if opacity > 0.0 {
        let scale = 0.4 + 0.6 * tick;
        let icon = if mixed { lucide::MINUS } else { lucide::CHECK };
        let square = Rect::from_center_size(r.center(), (mark, mark));
        icon.draw(
            ctx,
            square,
            3.0,
            theme.on_accent_fill.multiply_alpha(opacity),
            0.0,
            scale,
        );
    }
}

/// A round well and, chosen, the theme switch in small: a knob in the accent
/// colour springing up in it (10, 12 or 14 px – an even gap to the well)
fn paint_radio(
    ctx: &mut RenderContext,
    theme: &Theme,
    r: Rect,
    size: ControlSize,
    dot: f64,
    invalid: bool,
    ring: f32,
) {
    let radius = r.width() / 2.0;
    focus_outline(ctx, theme, r, radius, ring);
    let well = if invalid {
        &theme.field_shadow_invalid
    } else {
        &theme.field_shadow
    };
    paint::outer_shadows(ctx, r, radius, well, 1.0);
    ctx.set_paint(theme.field_bg);
    ctx.fill_path(&paint::rounded(r, radius));
    paint::inset_shadows(ctx, r, radius, well, 1.0);

    // the knob scales about the well's centre from nothing (overshooting with
    // the spring); its light and highlight are laid over the well's box
    let s = dot.max(0.0);
    let knob_r = match size {
        ControlSize::Xs | ControlSize::Sm => 5.0,
        ControlSize::Md => 6.0,
        ControlSize::Lg | ControlSize::Xl => 7.0,
    };
    if knob_r * s < 0.25 {
        return;
    }
    let (c, d) = (r.center(), r.width());
    let base = *ctx.transform();
    ctx.set_transform(
        base * Affine::translate(c.to_vec2()) * Affine::scale(s) * Affine::translate(-c.to_vec2()),
    );
    let circle = Circle::new(c, knob_r).to_path(TOLERANCE);
    // linear-gradient(160deg, accent-fill lightened 30 %, accent-fill-2 70 %)
    let a = 160f64.to_radians();
    let dir = Vec2::new(a.sin(), -a.cos());
    let half = (d * a.sin().abs() + d * a.cos().abs()) / 2.0;
    ctx.set_paint(
        Gradient::new_linear(c - dir * half, c + dir * half).with_stops([
            (0.3, mix(theme.accent_fill, Color::WHITE, 0.15)),
            (0.7, theme.accent_fill_2),
        ]),
    );
    ctx.fill_path(&circle);
    // radial-gradient(circle at 50% 30%, white 30 %, transparent 45 %): the
    // light on its top, reaching 45 % of the way to the well's farthest corner
    let top = Point::new(c.x, r.y0 + 0.3 * d);
    let reach = Vec2::new(d / 2.0, 0.7 * d).hypot();
    ctx.set_paint(Gradient::new_radial(top, reach as f32).with_stops([
        (0.0, Color::new([1.0, 1.0, 1.0, 0.3])),
        (0.45, Color::new([1.0, 1.0, 1.0, 0.0])),
    ]));
    ctx.fill_path(&circle);
    ctx.set_transform(base);
}

/// The sunken pill and the knob sliding (`pos`, overshooting with the spring)
/// and stretching towards the middle while pressed
#[allow(clippy::too_many_arguments)]
fn paint_switch(
    ctx: &mut RenderContext,
    theme: &Theme,
    track: Rect,
    size: ControlSize,
    pos: f64,
    press: f64,
    invalid: bool,
    ring: f32,
) {
    let (h, w, pad, d) = switch_dims(size);
    let radius = h / 2.0;
    if ring > 0.0 {
        // a 3 px outline, 3 px off the track
        ctx.set_stroke(Stroke::new(3.0));
        ctx.set_paint(theme.accent.multiply_alpha(0.45 * ring));
        ctx.stroke_path(&paint::rounded(track.inflate(4.5, 4.5), radius + 4.5));
    }
    let edge = if invalid {
        shade(0.0, 0.0, 0.0, 1.0, false, theme.danger)
    } else {
        shade(0.0, 0.0, 0.0, d * 0.6, false, theme.rim)
    };
    let track_shadows = [
        shade(d, d, d * 2.6, 0.0, true, theme.lo_strong),
        shade(-d, -d, d * 2.4, 0.0, true, theme.hi),
        edge,
        shade(-d * 0.6, -d * 0.6, d * 1.4, 0.0, false, theme.hi),
    ];
    paint::outer_shadows(ctx, track, radius, &track_shadows, 1.0);
    ctx.set_paint(theme.surface_sunken);
    ctx.fill_path(&paint::rounded(track, radius));
    paint::inset_shadows(ctx, track, radius, &track_shadows, 1.0);

    let extra = h * 0.18 * press;
    let k = h - 2.0 * pad;
    let x = track.x0 + pad + pos * (w - h - extra);
    let r = Rect::new(x, track.y0 + pad, x + k + extra, track.y0 + pad + k);
    knob(
        ctx,
        theme,
        r,
        &[
            shade(d, d * 1.4, d * 3.0, 0.0, false, theme.lo_strong),
            shade(-d * 0.4, -d * 0.4, d * 1.2, 0.0, false, theme.hi),
            shade(1.0, 1.0, 1.0, 0.0, true, Color::new([1.0, 1.0, 1.0, 0.35])),
        ],
    );
}

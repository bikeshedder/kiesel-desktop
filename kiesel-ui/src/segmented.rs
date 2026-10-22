//! Buttons in a sunken track: [`SegmentedControl`] – one of a few options, a
//! raised knob sliding to the chosen one – and [`ToggleGroup`] – several at
//! once, every chosen option its own knob (see `kiesel/src/components/
//! SegmentedControl` and `ToggleGroup`).
//!
//! The well is drawn around the outside of the buttons, so a knob lines up
//! with a plain button next to the track. A segmented control is a radio group
//! for assistive technology: only the chosen option is a Tab stop, the arrow
//! keys choose and move focus. Its `plain` variant is for switching views –
//! navigation, not a setting: no track, the current option a flat accent tint.
//!
//! Put either in a [`Field`](crate::Field) for a label above it, a hint and an
//! error, or give it an [`accessible_name`](SegmentedControl::accessible_name)
//! in a toolbar.

use std::rc::Rc;
use std::time::Instant;

use parley::Layout;
use reactive_graph::traits::{Get, GetUntracked, Track, With, WithUntracked};
use reactive_graph::wrappers::read::ArcSignal;
use vello_cpu::kurbo::{Point, Rect, Size, Stroke};
use vello_cpu::peniko::Gradient;
use winit::cursor::CursorIcon;
use winit::keyboard::{Key, NamedKey};

use crate::anim::{Easing, Transition};
use crate::choice_group::Group;
use crate::field::FieldLink;
use crate::i18n::{IntoText, TextProp};
use crate::icon::{IconNode, STROKE};
use crate::paint;
use crate::reactive::{MountCx, Watch};
use crate::text;
use crate::theme::{ControlSize, TextStyle, Theme, mix, space};
use crate::widget::{
    Available, Change, Event, EventCx, Known, LayoutCx, PaintCx, Widget, WidgetId, accessible_rect,
};

/// The knob slides with the spring in 320 ms
const SLIDE: f64 = 0.32;
/// `--ksl-well-pad`: the well reaches this far around the buttons
const WELL_PAD: f64 = 3.0;

/// One option: a label, an icon, or both (an icon alone needs a name)
pub struct Segment<T> {
    value: T,
    label: Option<ArcSignal<String>>,
    icon: Option<IconNode>,
    name: Option<ArcSignal<String>>,
}

impl<T> Segment<T> {
    pub fn new(value: T, label: impl IntoText) -> Self {
        Self {
            value,
            label: Some(label.into_text()),
            icon: None,
            name: None,
        }
    }

    /// An option that only shows `icon`; `name` says what it is
    pub fn icon_only(value: T, icon: IconNode, name: impl IntoText) -> Self {
        Self {
            value,
            label: None,
            icon: Some(icon),
            name: Some(name.into_text()),
        }
    }

    /// An icon before the label
    pub fn icon(mut self, icon: IconNode) -> Self {
        self.icon = Some(icon);
        self
    }
}

/// How an option looks
#[derive(Clone, Copy, PartialEq, Eq)]
enum Look {
    /// In a segmented control's track: the container draws the sliding knob
    Track,
    /// In a toggle group's track: chosen, it is a knob itself
    Knob,
    /// The plain variant: chosen, a flat accent tint
    Plain,
    /// The plain variant as a list, one option a row, its content at the start
    List,
}

/// The height of a control of this size (`--ksl-control-sm` … `-lg`), and of
/// a plain option
fn height(theme: &Theme, size: ControlSize, look: Look) -> f64 {
    let scale = f64::from((theme.metrics.font_size / 16.0).max(1.0));
    match (look, size) {
        (Look::Plain | Look::List, ControlSize::Xs | ControlSize::Sm) => 24.0 * scale,
        (Look::Plain | Look::List, ControlSize::Md) => 28.0 * scale,
        (Look::Plain | Look::List, _) => 36.0 * scale,
        (_, ControlSize::Xs | ControlSize::Sm) => theme.metrics.control * 0.8,
        (_, ControlSize::Md) => theme.metrics.control,
        (_, _) => theme.metrics.control * 1.2,
    }
}

fn radius(theme: &Theme, size: ControlSize) -> f64 {
    match size {
        ControlSize::Xs | ControlSize::Sm => theme.metrics.radius_sm,
        ControlSize::Md => theme.metrics.radius,
        _ => 8.0 * theme.metrics.radius / 6.0,
    }
}

/// One option's button
struct SegmentButton {
    id: WidgetId,
    label: Option<TextProp>,
    icon: Option<IconNode>,
    name: Option<TextProp>,
    size: ControlSize,
    look: Look,
    chosen: ArcSignal<bool>,
    /// Nothing is chosen and this is the default that holds
    implied: ArcSignal<bool>,
    /// Only the chosen (or else the first) is a Tab stop – not in a toggle group
    roving: Option<(bool, ArcSignal<bool>)>,
    press: Rc<dyn Fn()>,
    watch: Option<Watch>,
    layout: Option<Layout<()>>,
    style: Option<TextStyle>,
    shown: bool,
    shown_implied: bool,
    tab_stop: bool,
    bounds: Rect,
    hovered: bool,
    pointer_armed: bool,
    key_armed: bool,
    focused: bool,
    focus_visible: bool,
    hover: Transition,
    lit: Transition,
    ring: Transition,
    repaint: bool,
}

impl SegmentButton {
    fn text_style(&self, theme: &Theme) -> TextStyle {
        let mut style = theme.control_text();
        style.size = match self.size {
            ControlSize::Xs | ControlSize::Sm => 12.0,
            ControlSize::Md => 16.0,
            _ => 17.6,
        } * theme.metrics.font_size
            / 16.0;
        style.weight = if matches!(self.look, Look::Plain | Look::List) && !self.shown {
            500.0
        } else {
            600.0
        };
        style
    }

    fn padding(&self) -> f64 {
        match (self.look, self.size) {
            (Look::Plain | Look::List, _) => space(2),
            (_, ControlSize::Xs | ControlSize::Sm) => space(3),
            (_, ControlSize::Md) => space(4),
            _ => space(5),
        }
    }

    fn icon_size(&self, theme: &Theme) -> f64 {
        1.25 * f64::from(self.text_style(theme).size)
    }

    fn sync(&mut self, now: Instant, theme: &Theme) {
        let on = |b: bool| f64::from(u8::from(b));
        let d = theme.metrics.duration;
        let mut changed = self.hover.set(on(self.hovered), now, d);
        changed |= self.lit.set(on(self.shown), now, d);
        changed |= self
            .ring
            .set(on(self.focused && self.focus_visible), now, d);
        self.repaint |= changed;
    }

    fn activate(&mut self) {
        (self.press)();
    }
}

impl Widget for SegmentButton {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn mount(&mut self, cx: &mut MountCx<'_>) {
        let (chosen, implied) = (self.chosen.clone(), self.implied.clone());
        let any = self.roving.as_ref().map(|(_, any)| any.clone());
        let label = self.label.as_ref().map(TextProp::signal);
        let name = self.name.as_ref().map(TextProp::signal);
        self.watch = Some(cx.watch(self.id, move || {
            chosen.track();
            implied.track();
            for s in [&any].into_iter().flatten() {
                s.track();
            }
            for s in [&label, &name].into_iter().flatten() {
                s.track();
            }
        }));
    }

    fn update(&mut self, cx: &mut LayoutCx<'_>) -> Change {
        let mut change = Change::NONE;
        let (chosen, implied) = (self.chosen.get_untracked(), self.implied.get_untracked());
        if (chosen, implied) != (self.shown, self.shown_implied) {
            if matches!(self.look, Look::Plain | Look::List) && chosen != self.shown {
                // bolder when chosen: the width may change
                self.layout = None;
                change = Change::LAYOUT;
            }
            (self.shown, self.shown_implied) = (chosen, implied);
            self.sync(cx.now, cx.theme);
        }
        self.tab_stop = match &self.roving {
            Some((first, any)) => self.shown || (*first && !any.get_untracked()),
            None => true,
        };
        let restyled = self.style.is_some_and(|s| s != self.text_style(cx.theme));
        if self.label.as_mut().is_some_and(TextProp::refresh) || restyled {
            self.layout = None;
            change = Change::LAYOUT;
        }
        if let Some(name) = &mut self.name {
            self.repaint |= name.refresh();
        }
        if std::mem::take(&mut self.repaint) {
            change |= Change::PAINT;
        }
        change
    }

    /// Padding, [icon + gap] label; a toggle group's option at least square
    fn measure(&mut self, cx: &mut LayoutCx<'_>, _known: Known, _available: Available) -> Size {
        let style = self.text_style(cx.theme);
        self.style = Some(style);
        let mut width = 2.0 * self.padding();
        if self.icon.is_some() {
            width += self.icon_size(cx.theme);
        }
        if let Some(label) = &self.label {
            let layout = self
                .layout
                .get_or_insert_with(|| cx.text.line(&label.shown, &style));
            width += f64::from(layout.width());
            if self.icon.is_some() {
                width += space(2);
            }
        }
        let h = height(cx.theme, self.size, self.look);
        if self.look == Look::Knob {
            width = width.max(h);
        }
        Size::new(width.ceil(), h)
    }

    fn style(&self, _theme: &Theme) -> taffy::Style {
        taffy::Style {
            // a toggle group's options share the room the track gets; short of
            // room, labels are clipped rather than run into each other
            flex_grow: if self.look == Look::Knob { 1.0 } else { 0.0 },
            min_size: taffy::Size {
                width: if self.look == Look::Knob {
                    taffy::LengthPercentageAuto::auto()
                } else {
                    taffy::LengthPercentageAuto::length(0.0)
                },
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

    fn ink(&self) -> Rect {
        self.bounds.inflate(8.0, 8.0)
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
        let d = theme.metrics.duration;
        self.hover.animating(now, d) || self.lit.animating(now, d) || self.ring.animating(now, d)
    }

    fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) {
        let space = |k: &Key| matches!(k, Key::Character(c) if c == " ");
        let enter = |k: &Key| matches!(k, Key::Named(NamedKey::Enter));
        // Space and Enter press it; other keys go on (arrows to the control)
        if let Event::KeyDown { key, .. } | Event::KeyUp { key } = event
            && (space(key) || enter(key))
        {
            cx.handled = true;
        }
        match event {
            Event::PointerEnter => self.hovered = true,
            Event::PointerLeave => self.hovered = false,
            Event::PointerMove(p) => self.hovered = self.bounds.contains(*p),
            Event::PointerDown(p) => self.pointer_armed = self.bounds.contains(*p),
            Event::PointerUp(p) => {
                let click = self.pointer_armed && self.bounds.contains(*p);
                self.pointer_armed = false;
                if click {
                    self.activate();
                }
            }
            Event::KeyDown {
                key, repeat: false, ..
            } if space(key) => self.key_armed = true,
            Event::KeyUp { key } if space(key) && self.key_armed => {
                self.key_armed = false;
                self.activate();
            }
            Event::KeyDown {
                key, repeat: false, ..
            } if enter(key) => self.activate(),
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

    fn accessibility(&self) -> Option<accesskit::Node> {
        let mut node = accesskit::Node::new(match self.look {
            Look::Knob => accesskit::Role::Button,
            _ => accesskit::Role::RadioButton,
        });
        let name = self.name.as_ref().or(self.label.as_ref());
        if let Some(n) = name.filter(|n| !n.shown.is_empty()) {
            node.set_label(n.shown.clone());
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
        let d = theme.metrics.duration;
        let (r, rad) = (self.bounds, radius(theme, self.size));
        let (hover, lit) = (
            self.hover.value(now, d) as f32,
            self.lit.value(now, d) as f32,
        );
        let plain = matches!(self.look, Look::Plain | Look::List);

        paint::outer_shadows(
            ctx,
            r,
            if plain { theme.metrics.radius_sm } else { rad },
            &[theme.focus_ring],
            self.ring.value(now, d) as f32,
        );
        match self.look {
            Look::Knob if lit > 0.0 => knob(ctx, theme, r, rad, lit),
            Look::Plain | Look::List => {
                // hover: a faint tint; chosen: the accent's soft tint
                let rad = theme.metrics.radius_sm;
                let tint = if self.shown_implied {
                    theme.hover_bg
                } else {
                    mix(theme.hover_bg.multiply_alpha(hover), theme.accent_soft, lit)
                };
                if tint.components[3] > 0.002 {
                    ctx.set_paint(tint);
                    ctx.fill_path(&paint::rounded(r, rad));
                }
            }
            _ => {}
        }

        // --ksl-text-3-sunken, --ksl-text-2 under the pointer, --ksl-text-1 chosen
        // (and for the default that holds); plain: the accent text chosen
        let rest = mix(theme.text_3_sunken, theme.text_2, hover);
        let fg = match (self.look, self.shown_implied) {
            (_, true) => theme.text_1,
            (Look::Plain | Look::List, false) => mix(rest, theme.accent_2, lit),
            _ => mix(rest, theme.text_1, lit),
        };
        let icon = self.icon.map(|_| self.icon_size(theme)).unwrap_or(0.0);
        let label = self.layout.as_ref().map_or(0.0, |l| f64::from(l.width()));
        let gap = if icon > 0.0 && label > 0.0 {
            space(2)
        } else {
            0.0
        };
        let room = (r.width() - 2.0 * self.padding()).max(0.0);
        let width = (icon + gap + label).min(room);
        let mut x = if self.look == Look::List {
            r.x0 + self.padding()
        } else {
            r.center().x - width / 2.0
        };
        // short of room: clipped to the space inside the padding
        let pad = self.padding().min(r.width() / 2.0);
        ctx.push_clip_rect(&Rect::new(r.x0 + pad, r.y0, r.x1 - pad, r.y1));
        if let Some(node) = self.icon {
            let square = Rect::from_center_size((x + icon / 2.0, r.center().y), (icon, icon));
            node.draw(ctx, square, STROKE, fg, 0.0, 1.0);
            x += icon + gap;
        }
        if let Some(layout) = &self.layout {
            let y = r.center().y - f64::from(layout.height()) / 2.0;
            text::draw(ctx, cx.res, layout, Point::new(x, y), fg);
        }
        ctx.pop_clip();
    }
}

/// The raised knob of buttons (`--ksl-control-bg`, `--ksl-control-shadow`), faded by `alpha`
fn knob(ctx: &mut vello_cpu::RenderContext, theme: &Theme, r: Rect, rad: f64, alpha: f32) {
    paint::outer_shadows(ctx, r, rad, &theme.control_shadow, alpha);
    let [top, bottom] = theme.control_bg.map(|c| c.multiply_alpha(alpha));
    ctx.set_paint(Gradient::new_linear((r.x0, r.y0), (r.x0, r.y1)).with_stops([top, bottom]));
    ctx.fill_path(&paint::rounded(r, rad));
    paint::inset_shadows(ctx, r, rad, &theme.control_shadow, alpha);
}

/// The well around a track's buttons: `--ksl-field-bg`, `--ksl-field-shadow`
fn well(ctx: &mut vello_cpu::RenderContext, theme: &Theme, track: Rect, rad: f64, invalid: bool) {
    let r = track.inflate(WELL_PAD, WELL_PAD);
    let rad = rad + WELL_PAD;
    let shadows = if invalid {
        &theme.field_shadow_invalid
    } else {
        &theme.field_shadow
    };
    paint::outer_shadows(ctx, r, rad, shadows, 1.0);
    ctx.set_paint(theme.field_bg);
    ctx.fill_path(&paint::rounded(r, rad));
    paint::inset_shadows(ctx, r, rad, shadows, 1.0);
}

type Handler<V> = Rc<std::cell::RefCell<Option<Box<dyn FnMut(V)>>>>;

/// One of a few options in a sunken track; the chosen one a raised knob that
/// slides there. For more options or longer labels use a radio group.
pub struct SegmentedControl<T: Clone + PartialEq + Send + Sync + 'static> {
    group: Group,
    value: ArcSignal<T>,
    options: Vec<Segment<T>>,
    default: Option<T>,
    plain: bool,
    vertical: bool,
    name: Option<TextProp>,
    on_change: Handler<T>,
    on_clear: Option<Rc<dyn Fn()>>,
    /// Which option the knob is on, and whether it is the default (a ghost)
    at: ArcSignal<Option<(usize, bool)>>,
    shown: Option<(usize, bool)>,
    slide: Transition,
    watch: Option<Watch>,
}

impl<T: Clone + PartialEq + Send + Sync + 'static> SegmentedControl<T> {
    /// Shows `value`; options with [`SegmentedControl::option`], what to do on
    /// a choice with [`SegmentedControl::on_change`]. A `value` none of the
    /// options has chooses none (or shows the [`default`](Self::default)).
    pub fn new(value: impl Into<ArcSignal<T>>) -> Self {
        Self {
            group: Group::new(),
            value: value.into(),
            options: Vec::new(),
            default: None,
            plain: false,
            vertical: false,
            name: None,
            on_change: Rc::new(std::cell::RefCell::new(None)),
            on_clear: None,
            at: ArcSignal::stored(None),
            shown: None,
            slide: Transition::new(0.0).with_easing(Easing::SPRING),
            watch: None,
        }
    }

    pub fn option(mut self, segment: Segment<T>) -> Self {
        self.options.push(segment);
        self
    }

    /// Asked to choose another option; the owner of `value` decides
    pub fn on_change(self, f: impl FnMut(T) + 'static) -> Self {
        *self.on_change.borrow_mut() = Some(Box::new(f));
        self
    }

    /// What applies while none is chosen – a flat "ghost" knob on it: nothing
    /// was chosen, this is what holds
    pub fn default(mut self, value: T) -> Self {
        self.default = Some(value);
        self
    }

    /// Makes the value optional: a press on the chosen option calls this (set
    /// the value to one none of the options has) and the default shows again
    pub fn on_clear(mut self, f: impl Fn() + 'static) -> Self {
        self.on_clear = Some(Rc::new(f));
        self
    }

    /// Text options, the current one a flat accent tint – for switching views,
    /// where it is navigation rather than a setting
    pub fn plain(mut self) -> Self {
        self.plain = true;
        self
    }

    /// The plain variant as a list: one option a row, as wide as the list –
    /// the pages of a sidebar
    pub fn vertical(mut self) -> Self {
        self.plain = true;
        self.vertical = true;
        self
    }

    /// Sm, Md (default), Lg – the heights of buttons of these sizes
    pub fn size(mut self, size: ControlSize) -> Self {
        self.group.size = size;
        self
    }

    /// Its name for assistive technology, without a visible label (in a
    /// toolbar); otherwise put it in a `Field`
    pub fn accessible_name(mut self, name: impl IntoText) -> Self {
        self.name = Some(TextProp::new(name));
        self
    }

    fn build(&mut self) {
        let values: Vec<T> = self.options.iter().map(|o| o.value.clone()).collect();
        let (value, default) = (self.value.clone(), self.default.clone());
        let at = ArcSignal::derive(move || {
            value.with(|v| match values.iter().position(|o| o == v) {
                Some(i) => Some((i, false)),
                None => default
                    .as_ref()
                    .and_then(|d| values.iter().position(|o| o == d))
                    .map(|i| (i, true)),
            })
        });
        let any = ArcSignal::derive({
            let at = at.clone();
            move || at.get().is_some_and(|(_, implied)| !implied)
        });
        let look = match (self.vertical, self.plain) {
            (true, _) => Look::List,
            (false, true) => Look::Plain,
            (false, false) => Look::Track,
        };
        for (i, o) in self.options.iter().enumerate() {
            let chosen = ArcSignal::derive({
                let at = at.clone();
                move || at.get() == Some((i, false))
            });
            let implied = ArcSignal::derive({
                let at = at.clone();
                move || at.get() == Some((i, true))
            });
            let press: Rc<dyn Fn()> = {
                let (handler, clear) = (self.on_change.clone(), self.on_clear.clone());
                let (value, mine) = (self.value.clone(), o.value.clone());
                Rc::new(move || {
                    if value.with_untracked(|v| *v == mine) {
                        if let Some(clear) = &clear {
                            clear();
                        }
                    } else if let Some(f) = &mut *handler.borrow_mut() {
                        f(mine.clone());
                    }
                })
            };
            self.group.children.push(Box::new(option_widget(
                o,
                self.group.size,
                look,
                chosen,
                implied,
                Some((i == 0, any.clone())),
                press,
            )));
        }
        self.at = at;
    }

    /// Arrows choose the next or previous option, wrapping, and move focus there
    fn key(&mut self, cx: &mut EventCx<'_>, event: &Event) {
        let Event::KeyDown { key, .. } = event else {
            return;
        };
        let step: isize = match key {
            Key::Named(NamedKey::ArrowRight | NamedKey::ArrowDown) => 1,
            Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowUp) => -1,
            _ => return,
        };
        let n = self.options.len() as isize;
        if n == 0 {
            return;
        }
        cx.handled = true;
        let next = match self.at.get_untracked() {
            Some((i, false)) => (i as isize + step).rem_euclid(n),
            _ if step > 0 => 0,
            _ => n - 1,
        } as usize;
        if let Some(f) = &mut *self.on_change.borrow_mut() {
            f(self.options[next].value.clone());
        }
        cx.request_focus(self.group.children[next].id());
    }

    /// Where the knob is: between the options, at `pos` (in options)
    fn knob_rect(&self, pos: f64) -> Option<Rect> {
        let n = self.group.children.len();
        if n == 0 {
            return None;
        }
        let i = (pos.floor().max(0.0) as usize).min(n - 1);
        let j = (i + 1).min(n - 1);
        let (a, b) = (
            self.group.children[i].bounds(),
            self.group.children[j].bounds(),
        );
        let t = (pos - i as f64).clamp(-0.5, 1.5);
        let lerp = |x: f64, y: f64| x + (y - x) * t;
        Some(Rect::new(lerp(a.x0, b.x0), a.y0, lerp(a.x1, b.x1), a.y1))
    }
}

impl<T: Clone + PartialEq + Send + Sync + 'static> Widget for SegmentedControl<T> {
    fn id(&self) -> WidgetId {
        self.group.id
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.group.children
    }

    fn children_mut(&mut self) -> &mut [Box<dyn Widget>] {
        &mut self.group.children
    }

    fn link_field(&mut self, link: FieldLink) {
        self.group.field = Some(link);
    }

    fn mount(&mut self, cx: &mut MountCx<'_>) {
        if self.group.children.is_empty() {
            self.build();
            self.shown = self.at.get_untracked();
            self.slide = Transition::new(self.shown.map_or(0.0, |(i, _)| i as f64))
                .with_easing(Easing::SPRING);
        }
        self.group.mount(cx);
        let (at, name) = (self.at.clone(), self.name.as_ref().map(TextProp::signal));
        self.watch = Some(cx.watch(self.group.id, move || {
            at.track();
            if let Some(n) = &name {
                n.track();
            }
        }));
    }

    fn update(&mut self, cx: &mut LayoutCx<'_>) -> Change {
        let mut change = self.group.update();
        let at = self.at.get_untracked();
        if at != self.shown {
            // the knob slides from where it is; it appears where it lands
            if let (Some((i, _)), Some(_)) = (at, self.shown) {
                self.slide.set(i as f64, cx.now, cx.theme.motion(SLIDE));
            } else if let Some((i, _)) = at {
                self.slide = Transition::new(i as f64).with_easing(Easing::SPRING);
            }
            self.shown = at;
            change |= Change::PAINT;
        }
        if self.name.as_mut().is_some_and(TextProp::refresh) {
            change |= Change::PAINT;
        }
        change
    }

    /// Equal columns (`repeat(n, minmax(0, 1fr))`), room for the well at the sides
    fn style(&self, _theme: &Theme) -> taffy::Style {
        let n = self.group.children.len().max(1);
        let pad = taffy::LengthPercentage::length(if self.plain { 0.0 } else { WELL_PAD as f32 });
        let gap = taffy::LengthPercentage::length(if self.plain { 2.0 } else { 0.0 });
        taffy::Style {
            display: taffy::Display::Grid,
            // a list: one column as wide as the list, a row each
            grid_template_columns: if self.vertical {
                vec![taffy::style_helpers::flex(1.0)]
            } else {
                (0..n).map(|_| taffy::style_helpers::flex(1.0)).collect()
            },
            gap: taffy::Size {
                width: gap,
                height: gap,
            },
            padding: taffy::Rect {
                left: pad,
                right: pad,
                top: taffy::LengthPercentage::length(0.0),
                bottom: taffy::LengthPercentage::length(0.0),
            },
            align_self: Some(if self.vertical {
                taffy::AlignSelf::STRETCH
            } else {
                taffy::AlignSelf::START
            }),
            justify_self: Some(if self.vertical {
                taffy::JustifySelf::STRETCH
            } else {
                taffy::JustifySelf::START
            }),
            min_size: taffy::Size {
                width: taffy::LengthPercentageAuto::length(0.0),
                height: taffy::LengthPercentageAuto::auto(),
            },
            flex_shrink: 1.0,
            ..Default::default()
        }
    }

    fn set_bounds(&mut self, _cx: &mut LayoutCx<'_>, bounds: Rect) {
        self.group.bounds = bounds;
    }

    fn bounds(&self) -> Rect {
        self.group.bounds
    }

    /// The well reaches out above and below; the knob's shadow further
    fn ink(&self) -> Rect {
        self.group.bounds.inflate(12.0, 14.0)
    }

    fn animating(&self, now: Instant, theme: &Theme) -> bool {
        self.slide.animating(now, theme.motion(SLIDE))
    }

    fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) {
        self.key(cx, event);
    }

    fn accessibility(&self) -> Option<accesskit::Node> {
        let mut node = self.group.accessibility(accesskit::Role::RadioGroup);
        if let Some(n) = self.name.as_ref().filter(|n| !n.shown.is_empty()) {
            node.set_label(n.shown.clone());
        }
        Some(node)
    }

    fn paint(&self, cx: &mut PaintCx<'_>) {
        let theme = cx.theme;
        let rad = radius(theme, self.group.size);
        if !self.plain {
            let b = self.group.bounds;
            let track = Rect::new(b.x0 + WELL_PAD, b.y0, b.x1 - WELL_PAD, b.y1);
            well(cx.ctx, theme, track, rad, self.group.invalid);
            let pos = self.slide.value(cx.now, theme.motion(SLIDE));
            if let (Some((_, implied)), Some(r)) = (self.shown, self.knob_rect(pos)) {
                if implied {
                    // the default that holds: half the knob's fill, no lift, a dashed edge
                    let [top, bottom] = theme.control_bg.map(|c| c.multiply_alpha(0.55));
                    cx.ctx.set_paint(
                        Gradient::new_linear((r.x0, r.y0), (r.x0, r.y1)).with_stops([top, bottom]),
                    );
                    cx.ctx.fill_path(&paint::rounded(r, rad));
                    cx.ctx
                        .set_stroke(Stroke::new(1.0).with_dashes(0.0, [4.0, 3.0]));
                    cx.ctx.set_paint(theme.text_3.multiply_alpha(0.75));
                    cx.ctx
                        .stroke_path(&paint::rounded(r.inset(-0.5), rad - 0.5));
                } else {
                    knob(cx.ctx, theme, r, rad, 1.0);
                }
            }
        }
        cx.paint_children(&self.group.children);
    }
}

#[allow(clippy::too_many_arguments)]
fn option_widget<T>(
    o: &Segment<T>,
    size: ControlSize,
    look: Look,
    chosen: ArcSignal<bool>,
    implied: ArcSignal<bool>,
    roving: Option<(bool, ArcSignal<bool>)>,
    press: Rc<dyn Fn()>,
) -> SegmentButton {
    let shown = chosen.get_untracked();
    let on = f64::from(u8::from(shown));
    SegmentButton {
        id: WidgetId::next(),
        label: o.label.clone().map(TextProp::new),
        icon: o.icon,
        name: o.name.clone().map(TextProp::new),
        size,
        look,
        shown,
        shown_implied: implied.get_untracked(),
        chosen,
        implied,
        roving,
        press,
        watch: None,
        layout: None,
        style: None,
        tab_stop: true,
        bounds: Rect::ZERO,
        hovered: false,
        pointer_armed: false,
        key_armed: false,
        focused: false,
        focus_visible: false,
        hover: Transition::new(0.0),
        lit: Transition::new(on),
        ring: Transition::new(0.0),
        repaint: false,
    }
}

/// Several of a few options at once: a sunken track of toggle buttons, every
/// chosen one its own raised knob. For one option use a segmented control.
pub struct ToggleGroup<T: Clone + PartialEq + Send + Sync + 'static> {
    group: Group,
    value: ArcSignal<Vec<T>>,
    options: Vec<Segment<T>>,
    name: Option<TextProp>,
    on_change: Handler<Vec<T>>,
}

impl<T: Clone + PartialEq + Send + Sync + 'static> ToggleGroup<T> {
    /// Shows the options in `value` as chosen; changes with [`ToggleGroup::on_change`]
    pub fn new(value: impl Into<ArcSignal<Vec<T>>>) -> Self {
        Self {
            group: Group::new(),
            value: value.into(),
            options: Vec::new(),
            name: None,
            on_change: Rc::new(std::cell::RefCell::new(None)),
        }
    }

    pub fn option(mut self, segment: Segment<T>) -> Self {
        self.options.push(segment);
        self
    }

    /// Asked to change: the values that should be chosen, in the options' order
    pub fn on_change(self, f: impl FnMut(Vec<T>) + 'static) -> Self {
        *self.on_change.borrow_mut() = Some(Box::new(f));
        self
    }

    /// Sm, Md (default), Lg
    pub fn size(mut self, size: ControlSize) -> Self {
        self.group.size = size;
        self
    }

    /// Its name for assistive technology, without a visible label
    pub fn accessible_name(mut self, name: impl IntoText) -> Self {
        self.name = Some(TextProp::new(name));
        self
    }

    fn build(&mut self) {
        let all: Vec<T> = self.options.iter().map(|o| o.value.clone()).collect();
        for o in &self.options {
            let chosen = ArcSignal::derive({
                let (value, mine) = (self.value.clone(), o.value.clone());
                move || value.with(|v| v.contains(&mine))
            });
            let press: Rc<dyn Fn()> = {
                let (handler, value, mine, all) = (
                    self.on_change.clone(),
                    self.value.clone(),
                    o.value.clone(),
                    all.clone(),
                );
                Rc::new(move || {
                    let next = value.with_untracked(|v| {
                        let on = !v.contains(&mine);
                        all.iter()
                            .filter(|x| if **x == mine { on } else { v.contains(x) })
                            .cloned()
                            .collect()
                    });
                    if let Some(f) = &mut *handler.borrow_mut() {
                        f(next);
                    }
                })
            };
            self.group.children.push(Box::new(option_widget(
                o,
                self.group.size,
                Look::Knob,
                chosen,
                ArcSignal::stored(false),
                None,
                press,
            )));
        }
    }
}

impl<T: Clone + PartialEq + Send + Sync + 'static> Widget for ToggleGroup<T> {
    fn id(&self) -> WidgetId {
        self.group.id
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.group.children
    }

    fn children_mut(&mut self) -> &mut [Box<dyn Widget>] {
        &mut self.group.children
    }

    fn link_field(&mut self, link: FieldLink) {
        self.group.field = Some(link);
    }

    fn mount(&mut self, cx: &mut MountCx<'_>) {
        if self.group.children.is_empty() {
            self.build();
        }
        self.group.mount(cx);
    }

    fn update(&mut self, _cx: &mut LayoutCx<'_>) -> Change {
        let mut change = self.group.update();
        if self.name.as_mut().is_some_and(TextProp::refresh) {
            change |= Change::PAINT;
        }
        change
    }

    /// Options side by side, 2 px apart, sharing the track; room for the well at the sides
    fn style(&self, _theme: &Theme) -> taffy::Style {
        let pad = taffy::LengthPercentage::length(WELL_PAD as f32);
        let gap = taffy::LengthPercentage::length(2.0);
        taffy::Style {
            display: taffy::Display::Flex,
            flex_direction: taffy::FlexDirection::Row,
            gap: taffy::Size {
                width: gap,
                height: gap,
            },
            padding: taffy::Rect {
                left: pad,
                right: pad,
                top: taffy::LengthPercentage::length(0.0),
                bottom: taffy::LengthPercentage::length(0.0),
            },
            align_self: Some(taffy::AlignSelf::START),
            min_size: taffy::Size {
                width: taffy::LengthPercentageAuto::length(0.0),
                height: taffy::LengthPercentageAuto::auto(),
            },
            ..Default::default()
        }
    }

    fn set_bounds(&mut self, _cx: &mut LayoutCx<'_>, bounds: Rect) {
        self.group.bounds = bounds;
    }

    fn bounds(&self) -> Rect {
        self.group.bounds
    }

    fn ink(&self) -> Rect {
        self.group.bounds.inflate(12.0, 14.0)
    }

    fn accessibility(&self) -> Option<accesskit::Node> {
        let mut node = self.group.accessibility(accesskit::Role::Group);
        if let Some(n) = self.name.as_ref().filter(|n| !n.shown.is_empty()) {
            node.set_label(n.shown.clone());
        }
        Some(node)
    }

    fn paint(&self, cx: &mut PaintCx<'_>) {
        let b = self.group.bounds;
        let track = Rect::new(b.x0 + WELL_PAD, b.y0, b.x1 - WELL_PAD, b.y1);
        well(
            cx.ctx,
            cx.theme,
            track,
            radius(cx.theme, self.group.size),
            self.group.invalid,
        );
        cx.paint_children(&self.group.children);
    }
}

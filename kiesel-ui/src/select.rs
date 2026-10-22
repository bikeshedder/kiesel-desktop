//! `Select` – a raised trigger that opens a list of options below it: one
//! value, or several (see `kiesel/src/components/Select`).
//!
//! A click, Down/Up, Enter or Space opens the list; focus moves onto the
//! chosen option (or the search field), the arrows and a typed letter move
//! through the options, Enter, Space or a click chooses – one value closes
//! the list and focus goes back to the trigger; several keep it open and
//! tick. Escape or a click outside closes it. `searchable` puts a search field
//! on top: typing filters and marks the matches, Enter takes the first, Down
//! goes into the list. `on_clear` (one value) or `multiple` add "Clear" at the
//! list's foot. Put it in a [`Field`](crate::Field) for a label, hint and error.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use parley::Layout;
use reactive_graph::signal::ArcRwSignal;
use reactive_graph::traits::{Get, GetUntracked, Set, Track, Update, WithUntracked};
use reactive_graph::wrappers::read::ArcSignal;
use vello_cpu::kurbo::{Point, Rect, Size};
use vello_cpu::peniko::Gradient;
use winit::cursor::CursorIcon;
use winit::keyboard::{Key, NamedKey};

use crate::anim::Transition;
use crate::button::{Button, Variant};
use crate::control::Show;
use crate::field::FieldLink;
use crate::flex::{Align, Flex, Justify};
use crate::i18n::{self, IntoText, TextProp, t};
use crate::icon::{IconNode, STROKE};
use crate::label::Label;
use crate::lucide;
use crate::option_list::{IsSelected, ListSetup, OnTop, OptionList, RowIds, SelectOption, rows};
use crate::paint;
use crate::popup::{Placement, Popup};
use crate::reactive::{MountCx, Watch};
use crate::shortcut::KeyPress;
use crate::text;
use crate::text_field::TextField;
use crate::theme::{ControlSize, TextRole, TextStyle, Theme, mix, space};
use crate::widget::{
    Available, Change, Event, EventCx, Known, LayoutCx, PaintCx, Widget, WidgetId, accessible_rect,
};

type OnOne<T> = Rc<RefCell<Option<Box<dyn FnMut(T)>>>>;
type OnMany<T> = Rc<RefCell<Option<Box<dyn FnMut(Vec<T>)>>>>;

/// One value or several
enum Mode<T: Send + Sync + 'static> {
    One(ArcSignal<Option<T>>, OnOne<T>),
    Many(ArcSignal<Vec<T>>, OnMany<T>),
}

pub struct Select<T: Clone + PartialEq + Send + Sync + 'static> {
    id: WidgetId,
    options: Vec<SelectOption<T>>,
    /// The options, shared with the open list
    shared: Option<Arc<Vec<SelectOption<T>>>>,
    mode: Mode<T>,
    placeholder: TextProp,
    searchable: bool,
    on_clear: Option<Rc<dyn Fn()>>,
    size: ControlSize,
    name: Option<TextProp>,
    field: Option<FieldLink>,
    open: ArcRwSignal<bool>,
    query: ArcRwSignal<String>,
    layer: Option<WidgetId>,
    watch: Option<Watch>,
    /// What the trigger shows, and its shaped text
    shown: String,
    shown_icon: Option<IconNode>,
    layout: Option<Layout<()>>,
    style: Option<TextStyle>,
    invalid: bool,
    described: String,
    bounds: Rect,
    hovered: bool,
    focused: bool,
    focus_visible: bool,
    hover: Transition,
    flip: Transition,
    ring: Transition,
    repaint: bool,
}

impl<T: Clone + PartialEq + Send + Sync + 'static> Select<T> {
    fn with_mode(mode: Mode<T>) -> Self {
        Self {
            id: WidgetId::next(),
            options: Vec::new(),
            shared: None,
            mode,
            placeholder: TextProp::new(t("kiesel.select.placeholder")),
            searchable: false,
            on_clear: None,
            size: ControlSize::Md,
            name: None,
            field: None,
            open: ArcRwSignal::new(false),
            query: ArcRwSignal::new(String::new()),
            layer: None,
            watch: None,
            shown: String::new(),
            shown_icon: None,
            layout: None,
            style: None,
            invalid: false,
            described: String::new(),
            bounds: Rect::ZERO,
            hovered: false,
            focused: false,
            focus_visible: false,
            hover: Transition::new(0.0),
            flip: Transition::new(0.0),
            ring: Transition::new(0.0),
            repaint: false,
        }
    }

    /// One value: shows `value` (`None`: the placeholder); a choice is reported
    /// to [`Select::on_change`]
    pub fn new(value: impl Into<ArcSignal<Option<T>>>) -> Self {
        Self::with_mode(Mode::One(value.into(), Rc::new(RefCell::new(None))))
    }

    /// Several values: shows those in `values`; the list stays open and ticks,
    /// changes are reported to [`Select::on_change_many`]
    pub fn multiple(values: impl Into<ArcSignal<Vec<T>>>) -> Self {
        Self::with_mode(Mode::Many(values.into(), Rc::new(RefCell::new(None))))
    }

    pub fn option(mut self, option: SelectOption<T>) -> Self {
        self.options.push(option);
        self
    }

    pub fn options(mut self, options: impl IntoIterator<Item = SelectOption<T>>) -> Self {
        self.options.extend(options);
        self
    }

    /// One value chosen; the owner of the value decides
    pub fn on_change(self, f: impl FnMut(T) + 'static) -> Self {
        if let Mode::One(_, on) = &self.mode {
            *on.borrow_mut() = Some(Box::new(f));
        }
        self
    }

    /// Several: the values that should be chosen, in the options' order
    pub fn on_change_many(self, f: impl FnMut(Vec<T>) + 'static) -> Self {
        if let Mode::Many(_, on) = &self.mode {
            *on.borrow_mut() = Some(Box::new(f));
        }
        self
    }

    /// Shown while nothing is chosen (default "Select…")
    pub fn placeholder(mut self, text: impl IntoText) -> Self {
        self.placeholder = TextProp::new(text);
        self
    }

    /// A search field on top of the list – long lists: countries, time zones
    pub fn searchable(mut self) -> Self {
        self.searchable = true;
        self
    }

    /// Makes one value optional: "Clear" at the list's foot calls this
    pub fn on_clear(mut self, f: impl Fn() + 'static) -> Self {
        self.on_clear = Some(Rc::new(f));
        self
    }

    /// Sm, Md (default), Lg – the heights of buttons and fields of that size
    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    /// Its name for assistive technology without a visible label; otherwise
    /// put it in a `Field`
    pub fn accessible_name(mut self, name: impl IntoText) -> Self {
        self.name = Some(TextProp::new(name));
        self
    }

    fn chosen(&self) -> Vec<T> {
        match &self.mode {
            Mode::One(v, _) => v.get_untracked().into_iter().collect(),
            Mode::Many(v, _) => v.get_untracked(),
        }
    }

    /// The trigger's text: the chosen labels, or the placeholder (empty)
    fn chosen_text(&self) -> (String, Option<IconNode>) {
        let chosen = self.chosen();
        let picked: Vec<&SelectOption<T>> = self
            .options
            .iter()
            .filter(|o| chosen.contains(&o.value))
            .collect();
        let text = picked
            .iter()
            .map(|o| o.label.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let icon = match (&self.mode, picked.first()) {
            (Mode::One(..), Some(o)) => o.icon,
            _ => None,
        };
        (text, icon)
    }

    fn text_style(&self, theme: &Theme) -> TextStyle {
        let (_, _, size, _) = crate::button::dims(theme, self.size);
        let mut style = theme.text_style(TextRole::Body);
        style.size = size;
        style.weight = if self.shown.is_empty() { 400.0 } else { 500.0 };
        style
    }

    fn sync(&mut self, now: Instant, theme: &Theme) {
        let on = |b: bool| f64::from(u8::from(b));
        let d = theme.metrics.duration;
        let mut changed = self.hover.set(on(self.hovered), now, d);
        changed |= self.flip.set(on(self.open.get_untracked()), now, d);
        changed |= self
            .ring
            .set(on(self.focused && self.focus_visible), now, d);
        self.repaint |= changed;
    }

    /// The list's panel: [search field] the options [count, Clear]
    fn panel(&self) -> Flex {
        let options = self.shared.clone().expect("mounted");
        let ids: RowIds = Rc::default();
        let (open, query) = (self.open.clone(), self.query.clone());
        let multiple = matches!(self.mode, Mode::Many(..));
        // what a choice does: one value closes the list, several tick
        let choose: Rc<dyn Fn(T)> = match &self.mode {
            Mode::One(value, on) => {
                let (value, on, open) = (value.clone(), on.clone(), open.clone());
                Rc::new(move |v: T| {
                    if value.with_untracked(|cur| cur.as_ref() != Some(&v))
                        && let Some(f) = &mut *on.borrow_mut()
                    {
                        f(v);
                    }
                    open.set(false);
                })
            }
            Mode::Many(values, on) => {
                let (values, on, options) = (values.clone(), on.clone(), options.clone());
                Rc::new(move |v: T| {
                    let next = values.with_untracked(|cur| {
                        let on = !cur.contains(&v);
                        options
                            .iter()
                            .filter(|o| {
                                if o.value == v {
                                    on
                                } else {
                                    cur.contains(&o.value)
                                }
                            })
                            .map(|o| o.value.clone())
                            .collect::<Vec<_>>()
                    });
                    if let Some(f) = &mut *on.borrow_mut() {
                        f(next);
                    }
                })
            }
        };
        let (selection, selected): (ArcSignal<Vec<T>>, IsSelected<T>) = match &self.mode {
            Mode::One(v, _) => {
                let s = v.clone();
                (
                    ArcSignal::derive({
                        let v = v.clone();
                        move || v.get().into_iter().collect()
                    }),
                    Arc::new(move |x: &T| s.with_untracked(|c| c.as_ref() == Some(x))),
                )
            }
            Mode::Many(v, _) => {
                let s = v.clone();
                (
                    v.clone(),
                    Arc::new(move |x: &T| s.with_untracked(|c| c.contains(x))),
                )
            }
        };

        let mut panel = Flex::stack().gap(0.0);
        let mut search_id = None;
        if self.searchable {
            let mut search = TextField::new()
                .value(query.clone())
                .placeholder(t("kiesel.common.search"))
                .label(t("kiesel.select.searchOptions"))
                .grow();
            // Enter takes the first match, Down goes into the list
            search.on_key = Some(Box::new({
                let (options, query, ids, choose) =
                    (options.clone(), query.clone(), ids.clone(), choose.clone());
                move |press: &KeyPress, cx: &mut EventCx<'_>| {
                    let first = || {
                        rows(&options, &query.get_untracked())
                            .into_iter()
                            .find_map(|r| match r {
                                crate::option_list::Row::Option(i) if !options[i].disabled => {
                                    Some(i)
                                }
                                _ => None,
                            })
                    };
                    match press.key {
                        Key::Named(NamedKey::Enter) => {
                            if let Some(i) = first() {
                                choose(options[i].value.clone());
                            }
                            true
                        }
                        Key::Named(NamedKey::ArrowDown) => {
                            if let Some(id) = first().and_then(|i| ids.borrow().get(&i).copied()) {
                                cx.request_focus(id);
                            }
                            true
                        }
                        _ => false,
                    }
                }
            }));
            search_id = Some(search.id());
            panel = panel.child(Flex::stack().padding(space(2)).child(search));
        }
        let on_top: Option<OnTop> = search_id.map(|id| {
            let query = query.clone();
            Rc::new(move |cx: &mut EventCx<'_>, typed: Option<String>| {
                if let Some(t) = typed {
                    query.update(|q| q.push_str(&t));
                }
                cx.request_focus(id);
            }) as OnTop
        });
        let label = self
            .name
            .as_ref()
            .map(|n| n.shown.clone())
            .unwrap_or_default();
        panel = panel.child(OptionList::new(
            ListSetup {
                options: options.clone(),
                query: query.clone().into(),
                selected,
                selection: selection.clone(),
                choose: choose.clone(),
                prefer_selected: !self.searchable,
                active: None,
                owner: None,
                on_top,
                label,
                multiple,
            },
            ids,
        ));

        // the foot: how many match (searching), Clear (a value to clear)
        let clear: Option<Rc<dyn Fn()>> = match &self.mode {
            Mode::One(_, _) => self.on_clear.clone().map(|f| {
                let open = open.clone();
                Rc::new(move || {
                    f();
                    open.set(false);
                }) as Rc<dyn Fn()>
            }),
            Mode::Many(_, on) => {
                let on = on.clone();
                Some(Rc::new(move || {
                    if let Some(f) = &mut *on.borrow_mut() {
                        f(Vec::new());
                    }
                }) as Rc<dyn Fn()>)
            }
        };
        let mut foot = Flex::cluster()
            .nowrap()
            .justify(Justify::Between)
            .align(Align::Center)
            .padding(space(1));
        let mut any = false;
        if self.searchable {
            let (options, query) = (options.clone(), query.clone());
            foot = foot.child(
                Label::new(ArcSignal::derive(move || {
                    let q = query.get();
                    let n = rows(&options, &q)
                        .iter()
                        .filter(|r| matches!(r, crate::option_list::Row::Option(_)))
                        .count();
                    if q.trim().is_empty() {
                        String::new()
                    } else if n == 0 {
                        i18n::format(
                            &i18n::current(),
                            "kiesel.common.noResults",
                            &[("query", q.trim().to_string().into())],
                        )
                    } else {
                        i18n::format(
                            &i18n::current(),
                            "kiesel.common.xOfY",
                            &[("n", n.into()), ("total", options.len().into())],
                        )
                    }
                }))
                .role(TextRole::Caption),
            );
            any = true;
        }
        if let Some(clear) = clear {
            let has = ArcSignal::derive(move || !selection.get().is_empty());
            foot = foot.child(Show::new(has, move || {
                let clear = clear.clone();
                Button::new(t("kiesel.select.clear"))
                    .icon(lucide::X)
                    .variant(Variant::Ghost)
                    .size(ControlSize::Sm)
                    .on_click(move || clear())
            }));
            any = true;
        }
        if any {
            panel = panel.child(foot);
        }
        panel
    }
}

impl<T: Clone + PartialEq + Send + Sync + 'static> Widget for Select<T> {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn link_field(&mut self, link: FieldLink) {
        self.field = Some(link);
    }

    fn mount(&mut self, cx: &mut MountCx<'_>) {
        self.shared = Some(Arc::new(self.options.clone()));
        let open = self.open.clone();
        let value: ArcSignal<Vec<T>> = match &self.mode {
            Mode::One(v, _) => {
                let v = v.clone();
                ArcSignal::derive(move || v.get().into_iter().collect())
            }
            Mode::Many(v, _) => v.clone(),
        };
        let placeholder = self.placeholder.signal();
        let name = self.name.as_ref().map(TextProp::signal);
        let field = self.field.clone();
        self.watch = Some(cx.watch(self.id, move || {
            open.track();
            value.track();
            placeholder.track();
            if let Some(n) = &name {
                n.track();
            }
            if let Some(f) = &field {
                f.invalid.track();
                f.description.track();
            }
        }));
    }

    fn update(&mut self, cx: &mut LayoutCx<'_>) -> Change {
        let mut change = Change::NONE;
        // the list: opened and closed with `open`
        match (self.open.get_untracked(), self.layer) {
            (true, None) => {
                self.query.set(String::new());
                let open = self.open.clone();
                let label = match &self.name {
                    Some(n) => n.shown.clone(),
                    None => self.placeholder.shown.clone(),
                };
                self.layer = Some(cx.open_popup(Popup {
                    owner: self.id,
                    anchor: self.bounds,
                    placement: Placement::BOTTOM_START,
                    arrow: false,
                    label,
                    content: Box::new(self.panel()),
                    on_dismiss: Some(Box::new(move || open.set(false))),
                    list: true,
                    keep_focus: false,
                }));
                self.sync(cx.now, cx.theme);
            }
            (false, Some(layer)) => {
                cx.close_popup(layer);
                self.layer = None;
                self.sync(cx.now, cx.theme);
            }
            _ => {}
        }
        let (text, icon) = self.chosen_text();
        self.placeholder.refresh();
        let style = self.text_style(cx.theme);
        if text != self.shown || self.layout.is_none() || self.style.is_some_and(|s| s != style) {
            self.shown = text;
            self.shown_icon = icon;
            self.layout = None;
            change = Change::LAYOUT;
        }
        if let Some(n) = &mut self.name {
            self.repaint |= n.refresh();
        }
        if let Some(f) = &self.field {
            let state = (f.invalid.get_untracked(), f.description.get_untracked());
            if (state.0, &state.1) != (self.invalid, &self.described) {
                (self.invalid, self.described) = state;
                self.repaint = true;
            }
        }
        if std::mem::take(&mut self.repaint) {
            change |= Change::PAINT;
        }
        change
    }

    /// Padding, [icon] the text, the chevron; at least 160 px, as wide as offered
    fn measure(&mut self, cx: &mut LayoutCx<'_>, known: Known, available: Available) -> Size {
        let (h, _, _, _) = crate::button::dims(cx.theme, self.size);
        let style = self.text_style(cx.theme);
        self.style = Some(style);
        let shown = if self.shown.is_empty() {
            self.placeholder.shown.clone()
        } else {
            self.shown.clone()
        };
        let layout = self
            .layout
            .get_or_insert_with(|| cx.text.line(&shown, &style));
        let icon = if self.shown_icon.is_some() {
            20.0 + space(2)
        } else {
            0.0
        };
        let natural = 2.0 * space(3) + icon + f64::from(layout.width()) + space(2) + 20.0;
        let width = known.width.map(f64::from).unwrap_or(match available.width {
            taffy::AvailableSpace::Definite(w) => f64::from(w).max(160.0).min(natural.max(160.0)),
            _ => natural.max(160.0),
        });
        Size::new(width.ceil(), h)
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

    fn ink(&self) -> Rect {
        let b = self.bounds;
        Rect::new(b.x0 - 10.0, b.y0 - 8.0, b.x1 + 16.0, b.y1 + 18.0)
    }

    fn focusable(&self) -> bool {
        true
    }

    fn cursor(&self) -> CursorIcon {
        CursorIcon::Pointer
    }

    fn animating(&self, now: Instant, theme: &Theme) -> bool {
        let d = theme.metrics.duration;
        self.hover.animating(now, d) || self.flip.animating(now, d) || self.ring.animating(now, d)
    }

    fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) {
        let toggle = |s: &mut Self| s.open.update(|o| *o = !*o);
        match event {
            Event::PointerEnter => self.hovered = true,
            Event::PointerLeave => self.hovered = false,
            Event::PointerDown(_) => toggle(self),
            Event::KeyDown { key, .. } => match key {
                Key::Named(NamedKey::ArrowDown | NamedKey::ArrowUp | NamedKey::Enter) => {
                    cx.handled = true;
                    self.open.set(true);
                }
                Key::Character(c) if c == " " => {
                    cx.handled = true;
                    self.open.set(true);
                }
                // several: Backspace takes the last one off
                Key::Named(NamedKey::Backspace | NamedKey::Delete) => {
                    if let Mode::Many(values, on) = &self.mode {
                        let mut next = values.get_untracked();
                        if next.pop().is_some() {
                            cx.handled = true;
                            if let Some(f) = &mut *on.borrow_mut() {
                                f(next);
                            }
                        }
                    }
                }
                _ => {}
            },
            Event::Activate => toggle(self),
            Event::Focus { focused, visible } => {
                self.focused = *focused;
                self.focus_visible = *visible;
            }
            _ => {}
        }
        self.sync(cx.now, cx.theme);
    }

    fn accessibility(&self) -> Option<accesskit::Node> {
        let mut node = accesskit::Node::new(accesskit::Role::ComboBox);
        if let Some(n) = self.name.as_ref().filter(|n| !n.shown.is_empty()) {
            node.set_label(n.shown.clone());
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
        node.set_value(if self.shown.is_empty() {
            self.placeholder.shown.clone()
        } else {
            self.shown.clone()
        });
        node.set_expanded(self.layer.is_some());
        node.set_has_popup(accesskit::HasPopup::Listbox);
        node.set_bounds(accessible_rect(self.bounds));
        node.add_action(accesskit::Action::Click);
        node.add_action(accesskit::Action::Focus);
        Some(node)
    }

    fn paint(&self, cx: &mut PaintCx<'_>) {
        let (ctx, theme, now) = (&mut *cx.ctx, cx.theme, cx.now);
        let d = theme.metrics.duration;
        let (h, open) = (self.hover.value(now, d), self.flip.value(now, d));
        let r = self.bounds;
        let radius = crate::button::dims(theme, self.size).3;
        paint::outer_shadows(
            ctx,
            r,
            radius,
            &[theme.focus_ring],
            self.ring.value(now, d) as f32,
        );
        // the knob; open, it is pushed in (and red-edged with an error)
        let raised = paint::lerp_shadows(&theme.control_shadow, &theme.control_shadow_hover, h);
        let (up, down) = ((1.0 - open) as f32, open as f32);
        paint::outer_shadows(ctx, r, radius, &raised, up);
        let [top, bottom] = [0, 1].map(|i| {
            let c = theme.control_bg[i].lerp_rect(theme.control_bg_hover[i], h as f32);
            mix(c, theme.surface_sunken, down)
        });
        ctx.set_paint(Gradient::new_linear((r.x0, r.y0), (r.x0, r.y1)).with_stops([top, bottom]));
        ctx.fill_path(&paint::rounded(r, radius));
        paint::inset_shadows(ctx, r, radius, &raised, up);
        paint::inset_shadows(ctx, r, radius, &theme.control_shadow_pressed, down);
        if self.invalid {
            paint::inset_shadows(ctx, r, radius, &theme.field_shadow_invalid[..1], 1.0);
        }

        let mut x = r.x0 + space(3);
        if let Some(icon) = self.shown_icon {
            let sq = Rect::from_center_size((x + 10.0, r.center().y), (20.0, 20.0));
            icon.draw(ctx, sq, STROKE, theme.icon, 0.0, 1.0);
            x += 20.0 + space(2);
        }
        let chevron = Rect::from_center_size((r.x1 - space(3) - 10.0, r.center().y), (20.0, 20.0));
        if let Some(layout) = &self.layout {
            let color = if self.shown.is_empty() {
                theme.text_3
            } else {
                theme.text_1
            };
            ctx.push_clip_rect(&Rect::new(x, r.y0, chevron.x0 - space(1), r.y1));
            let y = r.center().y - f64::from(layout.height()) / 2.0;
            text::draw(ctx, cx.res, layout, Point::new(x, y), color);
            ctx.pop_clip();
        }
        lucide::CHEVRON_DOWN.draw(ctx, chevron, STROKE, theme.icon, 180.0 * open, 1.0);
    }
}

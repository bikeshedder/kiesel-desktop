//! The list under a [`Select`](crate::Select) (and later the combobox): options
//! with icons and descriptions, group headings, the search's match marked,
//! keyboard (arrows, Home/End, typing a letter jumps there) – see
//! `kiesel/src/components/Select/OptionList`.
//!
//! The options take the focus, one at a time (the list in a popup of its
//! own). The host owns the popup, the search field and what a choice means.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use parley::Layout;
use parley::editing::{Cursor, Selection};
use reactive_graph::traits::{Get, GetUntracked, Track};
use reactive_graph::wrappers::read::ArcSignal;
use vello_cpu::kurbo::{Point, Rect, Size};
use winit::cursor::CursorIcon;
use winit::keyboard::{Key, NamedKey};

use crate::control::For;
use crate::flex::Flex;
use crate::icon::{IconNode, STROKE};
use crate::lucide;
use crate::paint;
use crate::reactive::{MountCx, Watch};
use crate::scroll::Scroll;
use crate::text;
use crate::theme::{Shadow, TextRole, TextStyle, Theme, space};
use crate::widget::{
    Available, Change, Event, EventCx, Known, LayoutCx, PaintCx, Widget, WidgetId, accessible_rect,
};

/// One option of a select: its value and label, optionally an icon, a
/// description, a group heading to be listed under, words the search finds it by
#[derive(Clone, Debug)]
pub struct SelectOption<T> {
    pub value: T,
    pub label: String,
    pub description: Option<String>,
    pub icon: Option<IconNode>,
    pub group: Option<String>,
    pub keywords: Option<String>,
    pub disabled: bool,
}

impl<T> SelectOption<T> {
    pub fn new(value: T, label: impl Into<String>) -> Self {
        Self {
            value,
            label: label.into(),
            description: None,
            icon: None,
            group: None,
            keywords: None,
            disabled: false,
        }
    }

    /// A quieter second line
    pub fn description(mut self, text: impl Into<String>) -> Self {
        self.description = Some(text.into());
        self
    }

    pub fn icon(mut self, icon: IconNode) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Listed under this heading, with the others of the same group
    pub fn group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }

    /// What the search finds it by besides what it shows – a code, a number
    pub fn keywords(mut self, words: impl Into<String>) -> Self {
        self.keywords = Some(words.into());
        self
    }

    /// Shown, but can't be chosen
    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }

    /// Found by `query` (lower case, already trimmed)
    pub(crate) fn matches(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let all = [
            Some(&self.label),
            self.description.as_ref(),
            self.keywords.as_ref(),
            self.group.as_ref(),
        ];
        all.into_iter()
            .flatten()
            .any(|s| s.to_lowercase().contains(query))
    }
}

/// A row of the list: a heading, or the option at this index
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Row {
    Group(String, bool),
    Option(usize),
}

/// The rows for `query`: the matching options, headings before each group
pub(crate) fn rows<T>(options: &[SelectOption<T>], query: &str) -> Vec<Row> {
    let q = query.trim().to_lowercase();
    let mut out = Vec::new();
    let mut last: Option<&String> = None;
    for (i, o) in options.iter().enumerate().filter(|(_, o)| o.matches(&q)) {
        if let Some(g) = &o.group
            && last != Some(g)
        {
            out.push(Row::Group(g.clone(), out.is_empty()));
        }
        last = o.group.as_ref();
        out.push(Row::Option(i));
    }
    out
}

type Choose = Rc<dyn Fn()>;

/// One option's row: [icon] label (the match marked) and description, a tick when chosen
pub(crate) struct OptionRow {
    id: WidgetId,
    label: String,
    description: Option<String>,
    icon: Option<IconNode>,
    disabled: bool,
    selected: ArcSignal<bool>,
    query: ArcSignal<String>,
    /// Focus goes here when the list opens (the chosen one, without a search field)
    prefer: bool,
    /// A combobox's list: the row takes no focus, the field shows it highlighted
    highlight: Option<ArcSignal<bool>>,
    shown_highlight: bool,
    choose: Choose,
    watch: Option<Watch>,
    shown_selected: bool,
    shown_query: String,
    label_layout: Option<Layout<()>>,
    desc_layout: Option<Layout<()>>,
    style: Option<TextStyle>,
    bounds: Rect,
    hovered: bool,
    focused: bool,
    armed: bool,
}

impl OptionRow {
    fn styles(&self, theme: &Theme) -> (TextStyle, TextStyle) {
        let mut label = theme.text_style(TextRole::Body);
        label.weight = if self.shown_selected { 600.0 } else { 400.0 };
        (label, theme.text_style(TextRole::Caption))
    }

    /// Where the label starts and the tick sits, inside the row
    fn icon_room(&self) -> f64 {
        if self.icon.is_some() {
            20.0 + space(3)
        } else {
            0.0
        }
    }

    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    pub(crate) fn enabled(&self) -> bool {
        !self.disabled
    }
}

const TICK: f64 = 16.0;

impl Widget for OptionRow {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn mount(&mut self, cx: &mut MountCx<'_>) {
        let (selected, query) = (self.selected.clone(), self.query.clone());
        let highlight = self.highlight.clone();
        self.watch = Some(cx.watch(self.id, move || {
            selected.track();
            query.track();
            if let Some(h) = &highlight {
                h.track();
            }
        }));
    }

    fn update(&mut self, cx: &mut LayoutCx<'_>) -> Change {
        let mut change = Change::NONE;
        let selected = self.selected.get_untracked();
        let query = self.query.get_untracked();
        let style = self.styles(cx.theme).0;
        if selected != self.shown_selected || self.style.is_some_and(|s| s != style) {
            // bolder when chosen: shaped again
            self.shown_selected = selected;
            self.label_layout = None;
            self.desc_layout = None;
            change = Change::LAYOUT;
        }
        if query != self.shown_query {
            self.shown_query = query;
            change |= Change::PAINT;
        }
        let highlight = self
            .highlight
            .as_ref()
            .is_some_and(GetUntracked::get_untracked);
        if highlight != self.shown_highlight {
            self.shown_highlight = highlight;
            change |= Change::PAINT;
        }
        change
    }

    fn measure(&mut self, cx: &mut LayoutCx<'_>, known: Known, _available: Available) -> Size {
        let (label_style, desc_style) = self.styles(cx.theme);
        self.style = Some(label_style);
        let label = self
            .label_layout
            .get_or_insert_with(|| cx.text.line(&self.label, &label_style));
        let (mut w, mut h) = (f64::from(label.width()), f64::from(label.height()));
        if let Some(d) = &self.description {
            let desc = self
                .desc_layout
                .get_or_insert_with(|| cx.text.line(d, &desc_style));
            w = w.max(f64::from(desc.width()));
            h += f64::from(desc.height());
        }
        let width = 2.0 * space(3) + self.icon_room() + w + space(3) + TICK;
        Size::new(
            known.width.map_or(width.ceil(), f64::from),
            (h + 2.0 * space(1)).max(36.0),
        )
    }

    fn set_bounds(&mut self, _cx: &mut LayoutCx<'_>, bounds: Rect) {
        self.bounds = bounds;
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn focusable(&self) -> bool {
        !self.disabled && self.highlight.is_none()
    }

    fn prefers_focus(&self) -> bool {
        self.prefer && self.shown_selected
    }

    fn cursor(&self) -> CursorIcon {
        if self.disabled {
            CursorIcon::NotAllowed
        } else {
            CursorIcon::Pointer
        }
    }

    fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) {
        let choose = |row: &Self| {
            if !row.disabled {
                (row.choose)();
            }
        };
        match event {
            Event::PointerEnter => self.hovered = true,
            Event::PointerLeave => self.hovered = false,
            Event::PointerDown(_) => self.armed = true,
            Event::PointerUp(p) => {
                if std::mem::take(&mut self.armed) && self.bounds.contains(*p) {
                    choose(self);
                }
            }
            Event::KeyDown { key, .. }
                if matches!(key, Key::Named(NamedKey::Enter))
                    || matches!(key, Key::Character(c) if c == " ") =>
            {
                cx.handled = true;
                choose(self);
            }
            Event::Activate => choose(self),
            Event::Focus { focused, .. } => self.focused = *focused,
            _ => {}
        }
        let _ = cx;
    }

    fn accessibility(&self) -> Option<accesskit::Node> {
        let mut node = accesskit::Node::new(accesskit::Role::ListBoxOption);
        node.set_label(self.label.clone());
        if let Some(d) = &self.description {
            node.set_description(d.clone());
        }
        node.set_selected(self.shown_selected);
        if self.disabled {
            node.set_disabled();
        } else {
            node.add_action(accesskit::Action::Click);
            node.add_action(accesskit::Action::Focus);
        }
        node.set_bounds(accessible_rect(self.bounds));
        Some(node)
    }

    fn paint(&self, cx: &mut PaintCx<'_>) {
        let (ctx, theme) = (&mut *cx.ctx, cx.theme);
        let r = self.bounds;
        let alpha = if self.disabled { 0.45 } else { 1.0 };
        // under the pointer, or focused: pressed into the list (--ksl-inset-sm)
        if (self.hovered || self.focused || self.shown_highlight) && !self.disabled {
            let radius = theme.metrics.radius;
            let inset = |x: f64, color| Shadow {
                inset: true,
                x,
                y: x,
                blur: 5.0,
                spread: 0.0,
                color,
            };
            let shadows = [inset(2.0, theme.lo), inset(-2.0, theme.hi)];
            ctx.set_paint(theme.surface_sunken);
            ctx.fill_path(&paint::rounded(r, radius));
            paint::inset_shadows(ctx, r, radius, &shadows, 1.0);
        }
        let mut x = r.x0 + space(3);
        if let Some(icon) = self.icon {
            let square = Rect::from_center_size((x + 10.0, r.center().y), (20.0, 20.0));
            icon.draw(
                ctx,
                square,
                STROKE,
                theme.icon.multiply_alpha(alpha),
                0.0,
                1.0,
            );
            x += self.icon_room();
        }
        let Some(label) = &self.label_layout else {
            return;
        };
        let desc_h = self
            .desc_layout
            .as_ref()
            .map_or(0.0, |d| f64::from(d.height()));
        let top = r.center().y - (f64::from(label.height()) + desc_h) / 2.0;
        let origin = Point::new(x, top);
        // the search's match, marked
        let q = self.shown_query.trim().to_lowercase();
        if !q.is_empty()
            && let Some(start) = self.label.to_lowercase().find(&q)
        {
            let end = (start + q.len()).min(self.label.len());
            if self.label.is_char_boundary(start) && self.label.is_char_boundary(end) {
                let sel = Selection::new(
                    Cursor::from_byte_index(label, start, parley::Affinity::Downstream),
                    Cursor::from_byte_index(label, end, parley::Affinity::Downstream),
                );
                ctx.set_paint(theme.accent.multiply_alpha(0.25));
                for (b, _) in sel.geometry(label) {
                    let b = Rect::new(b.x0, b.y0 + 2.0, b.x1, b.y1 - 2.0) + origin.to_vec2();
                    ctx.fill_path(&paint::rounded(b, 3.0));
                }
            }
        }
        text::draw(
            ctx,
            cx.res,
            label,
            origin,
            theme.text_1.multiply_alpha(alpha),
        );
        if let Some(d) = &self.desc_layout {
            let p = Point::new(x, top + f64::from(label.height()));
            text::draw(ctx, cx.res, d, p, theme.text_3.multiply_alpha(alpha));
        }
        if self.shown_selected {
            let square =
                Rect::from_center_size((r.x1 - space(3) - TICK / 2.0, r.center().y), (TICK, TICK));
            lucide::CHECK.draw(ctx, square, 2.0, theme.accent, 0.0, 1.0);
        }
    }
}

/// A group heading: small capitals, a hairline above (not above the first)
struct GroupRow {
    id: WidgetId,
    text: String,
    first: bool,
    layout: Option<Layout<()>>,
    bounds: Rect,
}

impl Widget for GroupRow {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn measure(&mut self, cx: &mut LayoutCx<'_>, known: Known, _available: Available) -> Size {
        let mut style = cx.theme.text_style(TextRole::Caption);
        style.size *= 11.0 / 12.0;
        style.weight = 700.0;
        style.letter_spacing = 0.08;
        let layout = self
            .layout
            .get_or_insert_with(|| cx.text.line(&self.text.to_uppercase(), &style));
        let top = if self.first {
            space(2)
        } else {
            space(1) + space(3)
        };
        Size::new(
            known
                .width
                .map_or(f64::from(layout.width()) + 2.0 * space(3), f64::from),
            f64::from(layout.height()) + top + space(1),
        )
    }

    fn set_bounds(&mut self, _cx: &mut LayoutCx<'_>, bounds: Rect) {
        self.bounds = bounds;
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn paint(&self, cx: &mut PaintCx<'_>) {
        let r = self.bounds;
        if !self.first {
            cx.ctx.set_paint(cx.theme.hairline);
            cx.ctx.fill_rect(&Rect::new(
                r.x0,
                r.y0 + space(1),
                r.x1,
                r.y0 + space(1) + 1.0,
            ));
        }
        if let Some(layout) = &self.layout {
            let y = r.y1 - space(1) - f64::from(layout.height());
            text::draw(
                cx.ctx,
                cx.res,
                layout,
                Point::new(r.x0 + space(3), y),
                cx.theme.text_3,
            );
        }
    }
}

/// Up on the first option, or a letter typed (`Some`): back to the host's search field
pub(crate) type OnTop = Rc<dyn Fn(&mut EventCx<'_>, Option<String>)>;
/// Whether a value is chosen
pub(crate) type IsSelected<T> = Arc<dyn Fn(&T) -> bool + Send + Sync>;

/// What a host hands an [`OptionList`]
pub(crate) struct ListSetup<T: Send + Sync + 'static> {
    pub options: Arc<Vec<SelectOption<T>>>,
    pub query: ArcSignal<String>,
    /// Whether the option at this index is chosen
    pub selected: IsSelected<T>,
    /// The values chosen, tracked – the ticks follow it
    pub selection: ArcSignal<Vec<T>>,
    pub choose: Rc<dyn Fn(T)>,
    /// Focus the chosen option when the list opens (without a search field)
    pub prefer_selected: bool,
    /// A combobox: the highlighted option's index; the rows take no focus,
    /// presses keep it in the field (`owner`)
    pub active: Option<ArcSignal<Option<usize>>>,
    pub owner: Option<WidgetId>,
    /// Up on the first option, or a letter typed: back to the search field
    pub on_top: Option<OnTop>,
    pub label: String,
    pub multiple: bool,
}

/// The rows' ids by option index, so the host can focus the first one
pub(crate) type RowIds = Rc<RefCell<HashMap<usize, WidgetId>>>;

/// The options in a scroll view: rows made for the current query
pub(crate) struct OptionList {
    id: WidgetId,
    children: Vec<Box<dyn Widget>>,
    on_top: Option<OnTop>,
    owner: Option<WidgetId>,
    label: String,
    multiple: bool,
    bounds: Rect,
}

impl OptionList {
    pub fn new<T: Clone + PartialEq + Send + Sync + 'static>(s: ListSetup<T>, ids: RowIds) -> Self {
        let rows_signal = ArcSignal::derive({
            let (options, query) = (s.options.clone(), s.query.clone());
            move || rows(&options, &query.get())
        });
        let options = s.options.clone();
        let (query, selection, selected, choose) = (
            s.query.clone(),
            s.selection.clone(),
            s.selected.clone(),
            s.choose.clone(),
        );
        let prefer = s.prefer_selected;
        let active = s.active.clone();
        let rows = For::boxed(
            rows_signal,
            |r: &Row| match r {
                // first or not, a heading is another (the line above it)
                Row::Group(g, first) => format!("g:{g}:{first}"),
                Row::Option(i) => format!("o:{i}"),
            },
            move |r: &Row| -> Box<dyn Widget> {
                match r {
                    Row::Group(text, first) => Box::new(GroupRow {
                        id: WidgetId::next(),
                        text: text.clone(),
                        first: *first,
                        layout: None,
                        bounds: Rect::ZERO,
                    }),
                    Row::Option(i) => {
                        let o = &options[*i];
                        let value = o.value.clone();
                        let is = ArcSignal::derive({
                            let (selection, selected, value) =
                                (selection.clone(), selected.clone(), value.clone());
                            move || {
                                selection.track();
                                selected(&value)
                            }
                        });
                        let choose = choose.clone();
                        let row = OptionRow {
                            id: WidgetId::next(),
                            label: o.label.clone(),
                            description: o.description.clone(),
                            icon: o.icon,
                            disabled: o.disabled,
                            shown_selected: is.get_untracked(),
                            selected: is,
                            query: query.clone(),
                            prefer,
                            highlight: active.clone().map(|a| {
                                let i = *i;
                                ArcSignal::derive(move || a.get() == Some(i))
                            }),
                            shown_highlight: false,
                            choose: Rc::new(move || choose(value.clone())),
                            watch: None,
                            shown_query: String::new(),
                            label_layout: None,
                            desc_layout: None,
                            style: None,
                            bounds: Rect::ZERO,
                            hovered: false,
                            focused: false,
                            armed: false,
                        };
                        ids.borrow_mut().insert(*i, row.id);
                        Box::new(row)
                    }
                }
            },
        );
        let list = Flex::stack().gap(2.0).padding(space(2)).child(rows);
        Self {
            id: WidgetId::next(),
            children: vec![Box::new(Scroll::new(list).grow())],
            on_top: s.on_top,
            owner: s.owner,
            label: s.label,
            multiple: s.multiple,
            bounds: Rect::ZERO,
        }
    }

    /// The option rows, in list order
    fn options(&self) -> Vec<&OptionRow> {
        let mut out = Vec::new();
        crate::widget::walk(self, &mut |w| {
            if let Some(row) = w.as_any().downcast_ref::<OptionRow>() {
                out.push(row);
            }
        });
        out
    }
}

impl Widget for OptionList {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut [Box<dyn Widget>] {
        &mut self.children
    }

    /// Takes the room the popup has, scrolling the rest
    fn style(&self, _theme: &Theme) -> taffy::Style {
        taffy::Style {
            display: taffy::Display::Flex,
            flex_direction: taffy::FlexDirection::Column,
            flex_grow: 1.0,
            flex_shrink: 1.0,
            min_size: taffy::Size {
                width: taffy::LengthPercentageAuto::length(0.0),
                height: taffy::LengthPercentageAuto::length(0.0),
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

    /// A combobox's list: a press on it keeps the focus in the field
    fn press_focuses(&self, _p: Point) -> Option<WidgetId> {
        self.owner
    }

    /// Keys from the focused option: arrows, Home/End, a letter jumps to the
    /// next option starting with it (or goes on to the search field)
    fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) {
        let Event::KeyDown { key, text, .. } = event else {
            return;
        };
        let rows = self.options();
        let enabled: Vec<&OptionRow> = rows.iter().copied().filter(|r| r.enabled()).collect();
        let n = enabled.len();
        if n == 0 {
            return;
        }
        let at = cx
            .origin
            .and_then(|o| enabled.iter().position(|r| r.id() == o));
        let to = match key {
            Key::Named(NamedKey::ArrowDown) => at.map_or(0, |i| (i + 1) % n),
            Key::Named(NamedKey::ArrowUp) => match at {
                Some(0) | None if self.on_top.is_some() => {
                    cx.handled = true;
                    if let Some(f) = self.on_top.clone() {
                        f(cx, None);
                    }
                    return;
                }
                Some(i) => (i + n - 1) % n,
                None => n - 1,
            },
            Key::Named(NamedKey::Home) => 0,
            Key::Named(NamedKey::End) => n - 1,
            _ => {
                let typed = text
                    .as_deref()
                    .filter(|t| !t.chars().any(char::is_control) && !cx.modifiers.control_key());
                let Some(typed) = typed else { return };
                cx.handled = true;
                // a search field above: the letters go on there
                if let Some(f) = self.on_top.clone() {
                    f(cx, Some(typed.to_string()));
                    return;
                }
                let k = typed.to_lowercase();
                let start = at.map_or(0, |i| i + 1);
                let hit = (0..n)
                    .map(|j| (start + j) % n)
                    .find(|&j| enabled[j].label().to_lowercase().starts_with(&k));
                match hit {
                    Some(j) => j,
                    None => return,
                }
            }
        };
        cx.handled = true;
        cx.request_focus(enabled[to].id());
    }

    fn accessibility(&self) -> Option<accesskit::Node> {
        let mut node = accesskit::Node::new(accesskit::Role::ListBox);
        if !self.label.is_empty() {
            node.set_label(self.label.clone());
        }
        if self.multiple {
            node.set_multiselectable();
        }
        node.set_bounds(accessible_rect(self.bounds));
        Some(node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_filter_and_head_their_groups() {
        let options = vec![
            SelectOption::new(1, "Apple").group("Fruit"),
            SelectOption::new(2, "Banana")
                .group("Fruit")
                .keywords("yellow"),
            SelectOption::new(3, "Carrot").group("Vegetables"),
        ];
        assert_eq!(
            rows(&options, ""),
            [
                Row::Group("Fruit".into(), true),
                Row::Option(0),
                Row::Option(1),
                Row::Group("Vegetables".into(), false),
                Row::Option(2),
            ]
        );
        assert_eq!(
            rows(&options, " YEL "),
            [Row::Group("Fruit".into(), true), Row::Option(1)],
            "keywords count, case and spaces don't"
        );
        assert_eq!(
            rows(&options, "vegetables"),
            [Row::Group("Vegetables".into(), true), Row::Option(2)],
            "the group's name finds its options"
        );
    }
}

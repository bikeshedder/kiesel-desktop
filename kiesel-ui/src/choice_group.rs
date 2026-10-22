//! Groups of options: [`RadioGroup`] – one choice of a few visible options –
//! and [`CheckboxGroup`] – several independent choices about one thing (see
//! `kiesel/src/components/Radio` and `CheckboxGroup`).
//!
//! Put a group in a [`Field`](crate::Field) for its legend, hint and error:
//! the field names the group, and an error marks every option red. In a radio
//! group only the chosen option is in the Tab order; the arrow keys choose the
//! next or previous one, as on every desktop.

use std::cell::RefCell;
use std::rc::Rc;

use reactive_graph::traits::{Get, GetUntracked, Track, With, WithUntracked};
use reactive_graph::wrappers::read::ArcSignal;
use vello_cpu::kurbo::Rect;
use winit::keyboard::{Key, NamedKey};

use crate::field::FieldLink;
use crate::i18n::{IntoText, t};
use crate::reactive::{MountCx, Watch};
use crate::theme::{ControlSize, Theme, space};
use crate::toggle::{self, Checkbox, Toggle};
use crate::widget::{Change, Event, EventCx, LayoutCx, Widget, WidgetId, accessible_rect};

/// One option of a group: its value, label and (optionally) a description
pub struct Choice<T> {
    value: T,
    label: ArcSignal<String>,
    description: Option<ArcSignal<String>>,
}

impl<T> Choice<T> {
    pub fn new(value: T, label: impl IntoText) -> Self {
        Self {
            value,
            label: label.into_text(),
            description: None,
        }
    }

    /// A quieter line under the label
    pub fn description(mut self, text: impl IntoText) -> Self {
        self.description = Some(text.into_text());
        self
    }
}

type Handler<V> = Rc<RefCell<Option<Box<dyn FnMut(V)>>>>;

/// What both groups share: the rows, the field's link, the layout
struct Group {
    id: WidgetId,
    children: Vec<Box<dyn Widget>>,
    size: ControlSize,
    horizontal: bool,
    field: Option<FieldLink>,
    watch: Option<Watch>,
    /// The field's state as of the last update
    invalid: bool,
    described: String,
    bounds: Rect,
}

impl Group {
    fn new() -> Self {
        Self {
            id: WidgetId::next(),
            children: Vec::new(),
            size: ControlSize::Md,
            horizontal: false,
            field: None,
            watch: None,
            invalid: false,
            described: String::new(),
            bounds: Rect::ZERO,
        }
    }

    fn invalid_signal(&self) -> ArcSignal<bool> {
        self.field
            .as_ref()
            .map_or_else(|| ArcSignal::stored(false), |f| f.invalid.clone())
    }

    fn mount(&mut self, cx: &mut MountCx<'_>) {
        if let Some(f) = self.field.clone() {
            self.watch = Some(cx.watch(self.id, move || {
                f.invalid.track();
                f.description.track();
            }));
        }
    }

    fn update(&mut self) -> Change {
        let Some(f) = &self.field else {
            return Change::NONE;
        };
        let state = (f.invalid.get_untracked(), f.description.get_untracked());
        if (state.0, &state.1) == (self.invalid, &self.described) {
            return Change::NONE;
        }
        (self.invalid, self.described) = state;
        Change::PAINT
    }

    /// Options below each other, or in a row that wraps
    fn style(&self) -> taffy::Style {
        let small = matches!(self.size, ControlSize::Xs | ControlSize::Sm);
        let gap = match (self.horizontal, small) {
            (true, true) => space(4),
            (true, false) => space(5),
            (false, true) => space(2),
            (false, false) => space(3),
        };
        let gap = taffy::LengthPercentage::length(gap as f32);
        taffy::Style {
            display: taffy::Display::Flex,
            flex_direction: if self.horizontal {
                taffy::FlexDirection::Row
            } else {
                taffy::FlexDirection::Column
            },
            flex_wrap: taffy::FlexWrap::Wrap,
            gap: taffy::Size {
                width: gap,
                height: gap,
            },
            align_items: taffy::AlignItems::START,
            min_size: taffy::Size {
                width: taffy::LengthPercentageAuto::length(0.0),
                height: taffy::LengthPercentageAuto::auto(),
            },
            ..Default::default()
        }
    }

    fn accessibility(&self, role: accesskit::Role) -> accesskit::Node {
        let mut node = accesskit::Node::new(role);
        if let Some(f) = &self.field {
            node.push_labelled_by(f.label.node());
            if f.required {
                node.set_required();
            }
        }
        if !self.described.is_empty() {
            node.set_description(self.described.clone());
        }
        if self.invalid {
            node.set_invalid(accesskit::Invalid::True);
        }
        node.set_bounds(accessible_rect(self.bounds));
        node
    }
}

macro_rules! group_widget {
    ($name:ident, $role:expr) => {
        impl<T: Clone + PartialEq + Send + Sync + 'static> Widget for $name<T> {
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
                self.group.update()
            }

            fn style(&self, _theme: &Theme) -> taffy::Style {
                self.group.style()
            }

            fn set_bounds(&mut self, _cx: &mut LayoutCx<'_>, bounds: Rect) {
                self.group.bounds = bounds;
            }

            fn bounds(&self) -> Rect {
                self.group.bounds
            }

            fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) {
                self.key(cx, event);
            }

            fn accessibility(&self) -> Option<accesskit::Node> {
                Some(self.group.accessibility($role))
            }
        }
    };
}

/// One choice of a few visible options, each with an optional description.
/// For many options use a select, for 2–5 short ones a segmented control.
pub struct RadioGroup<T: Clone + PartialEq + Send + Sync + 'static> {
    group: Group,
    value: ArcSignal<T>,
    options: Vec<Choice<T>>,
    on_change: Handler<T>,
}

impl<T: Clone + PartialEq + Send + Sync + 'static> RadioGroup<T> {
    /// Shows `value`; options with [`RadioGroup::option`], what to do on a
    /// choice with [`RadioGroup::on_change`]. A `value` none of the options has
    /// chooses none.
    pub fn new(value: impl Into<ArcSignal<T>>) -> Self {
        Self {
            group: Group::new(),
            value: value.into(),
            options: Vec::new(),
            on_change: Rc::new(RefCell::new(None)),
        }
    }

    pub fn option(mut self, choice: Choice<T>) -> Self {
        self.options.push(choice);
        self
    }

    /// Asked to choose another option; the owner of `value` decides
    pub fn on_change(self, f: impl FnMut(T) + 'static) -> Self {
        *self.on_change.borrow_mut() = Some(Box::new(f));
        self
    }

    /// Options side by side, wrapping
    pub fn horizontal(mut self) -> Self {
        self.group.horizontal = true;
        self
    }

    /// Dot and label size: Sm, Md (default), Lg
    pub fn size(mut self, size: ControlSize) -> Self {
        self.group.size = size;
        self
    }

    fn choose(&self, value: T) {
        if let Some(f) = &mut *self.on_change.borrow_mut() {
            f(value);
        }
    }

    fn build(&mut self) {
        let values: Vec<T> = self.options.iter().map(|o| o.value.clone()).collect();
        let any = ArcSignal::derive({
            let value = self.value.clone();
            move || value.with(|v| values.contains(v))
        });
        let invalid = self.group.invalid_signal();
        for (i, o) in self.options.iter().enumerate() {
            let checked = ArcSignal::derive({
                let (value, mine) = (self.value.clone(), o.value.clone());
                move || value.with(|v| *v == mine)
            });
            let (handler, mine) = (self.on_change.clone(), o.value.clone());
            let mut row = toggle::radio(checked)
                .label(o.label.clone())
                .size(self.group.size)
                .group_invalid(invalid.clone())
                .roving(i == 0, any.clone())
                .on_change(move |_| {
                    if let Some(f) = &mut *handler.borrow_mut() {
                        f(mine.clone());
                    }
                });
            if let Some(d) = &o.description {
                row = row.description(d.clone());
            }
            self.group.children.push(Box::new(row));
        }
    }

    /// Arrows choose the next or previous option – and move focus there
    fn key(&mut self, cx: &mut EventCx<'_>, event: &Event) {
        let Event::KeyDown { key, .. } = event else {
            return;
        };
        let step: isize = match key {
            Key::Named(NamedKey::ArrowDown | NamedKey::ArrowRight) => 1,
            Key::Named(NamedKey::ArrowUp | NamedKey::ArrowLeft) => -1,
            _ => return,
        };
        let n = self.options.len() as isize;
        if n == 0 {
            return;
        }
        cx.handled = true;
        let at = self
            .value
            .with_untracked(|v| self.options.iter().position(|o| o.value == *v));
        // with none chosen, focus is on the first option: move on from there
        let next = (at.unwrap_or(0) as isize + step).rem_euclid(n) as usize;
        self.choose(self.options[next].value.clone());
        cx.request_focus(self.group.children[next].id());
    }
}

group_widget!(RadioGroup, accesskit::Role::RadioGroup);

/// Several independent choices about one thing – notifications to receive,
/// permissions of a person. Optionally a "select all" box above them.
pub struct CheckboxGroup<T: Clone + PartialEq + Send + Sync + 'static> {
    group: Group,
    value: ArcSignal<Vec<T>>,
    options: Vec<Choice<T>>,
    select_all: Option<ArcSignal<String>>,
    on_change: Handler<Vec<T>>,
}

impl<T: Clone + PartialEq + Send + Sync + 'static> CheckboxGroup<T> {
    /// Shows the options in `value` as checked; options with
    /// [`CheckboxGroup::option`], changes with [`CheckboxGroup::on_change`]
    pub fn new(value: impl Into<ArcSignal<Vec<T>>>) -> Self {
        Self {
            group: Group::new(),
            value: value.into(),
            options: Vec::new(),
            select_all: None,
            on_change: Rc::new(RefCell::new(None)),
        }
    }

    pub fn option(mut self, choice: Choice<T>) -> Self {
        self.options.push(choice);
        self
    }

    /// Asked to change: the values that should be checked, in the options' order
    pub fn on_change(self, f: impl FnMut(Vec<T>) + 'static) -> Self {
        *self.on_change.borrow_mut() = Some(Box::new(f));
        self
    }

    /// A box above the options that checks all of them – indeterminate when
    /// some are ("Select all")
    pub fn select_all(mut self) -> Self {
        self.select_all = Some(t("kiesel.checkboxGroup.all"));
        self
    }

    /// Box and label size: Sm, Md (default), Lg
    pub fn size(mut self, size: ControlSize) -> Self {
        self.group.size = size;
        self
    }

    /// Options side by side, wrapping
    pub fn horizontal(mut self) -> Self {
        self.group.horizontal = true;
        self
    }

    fn build(&mut self) {
        let all: Vec<T> = self.options.iter().map(|o| o.value.clone()).collect();
        let invalid = self.group.invalid_signal();
        if let Some(label) = self.select_all.clone() {
            let (value, everything) = (self.value.clone(), all.clone());
            let count = ArcSignal::derive(move || {
                value.with(|v| everything.iter().filter(|o| v.contains(o)).count())
            });
            let n = all.len();
            let (every, some) = (count.clone(), count);
            let handler = self.on_change.clone();
            let all = all.clone();
            let row = Checkbox::new(ArcSignal::derive(move || n > 0 && every.get() == n))
                .indeterminate(ArcSignal::derive(move || {
                    let c = some.get();
                    c > 0 && c < n
                }))
                .label(label)
                .size(self.group.size)
                .group_invalid(invalid.clone())
                .on_change(move |on| {
                    if let Some(f) = &mut *handler.borrow_mut() {
                        f(if on { all.clone() } else { Vec::new() });
                    }
                });
            self.group.children.push(Box::new(row));
        }
        for o in &self.options {
            let checked = ArcSignal::derive({
                let (value, mine) = (self.value.clone(), o.value.clone());
                move || value.with(|v| v.contains(&mine))
            });
            let (handler, value, mine) =
                (self.on_change.clone(), self.value.clone(), o.value.clone());
            let all = all.clone();
            let mut row: Toggle = Checkbox::new(checked)
                .label(o.label.clone())
                .size(self.group.size)
                .group_invalid(invalid.clone())
                .on_change(move |on| {
                    let next = value.with_untracked(|v| toggled(&all, v, &mine, on));
                    if let Some(f) = &mut *handler.borrow_mut() {
                        f(next);
                    }
                });
            if let Some(d) = &o.description {
                row = row.description(d.clone());
            }
            self.group.children.push(Box::new(row));
        }
    }

    fn key(&mut self, _cx: &mut EventCx<'_>, _event: &Event) {}
}

group_widget!(CheckboxGroup, accesskit::Role::Group);

/// The values checked after `item` turned `on`, in the options' order
fn toggled<T: Clone + PartialEq>(all: &[T], checked: &[T], item: &T, on: bool) -> Vec<T> {
    all.iter()
        .filter(|x| if *x == item { on } else { checked.contains(x) })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::toggled;

    #[test]
    fn checking_keeps_the_options_order_and_drops_unknown_values() {
        let all = ["a", "b", "c"];
        assert_eq!(toggled(&all, &["c"], &"a", true), ["a", "c"]);
        assert_eq!(toggled(&all, &["a", "c"], &"a", false), ["c"]);
        assert_eq!(toggled(&all, &["c", "x"], &"b", true), ["b", "c"]);
    }
}

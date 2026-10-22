//! `ComboBox` – a text field with suggestions: the text stays free, the list
//! under it shows the options that match what is typed (see Kiesel's
//! `TextField` with `options`).
//!
//! The list shows while there is text with matches, or after Down; the field
//! keeps the focus. Up and Down move the highlight (assistive technology
//! follows it as the active option), Enter or a click takes it – its label
//! becomes the text and [`ComboBox::on_choose`] hears its value. Escape, a
//! click elsewhere or leaving the field close the list.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use reactive_graph::signal::ArcRwSignal;
use reactive_graph::traits::{GetUntracked, Set, Track};
use reactive_graph::wrappers::read::ArcSignal;
use vello_cpu::kurbo::Rect;
use winit::keyboard::{Key, NamedKey};

use crate::field::FieldLink;
use crate::flex::Flex;
use crate::i18n::{IntoText, TextProp};
use crate::option_list::{ListSetup, OptionList, Row, RowIds, SelectOption, rows};
use crate::popup::{Placement, Popup};
use crate::reactive::{MountCx, Watch};
use crate::shortcut::KeyPress;
use crate::text_field::{Combo, TextField};
use crate::theme::Theme;
use crate::widget::{Change, EventCx, LayoutCx, Widget, WidgetId};

type OnChoose<T> = Rc<RefCell<Option<Box<dyn FnMut(T)>>>>;

pub struct ComboBox<T: Clone + PartialEq + Send + Sync + 'static> {
    id: WidgetId,
    text: ArcRwSignal<String>,
    options: Vec<SelectOption<T>>,
    shared: Option<Arc<Vec<SelectOption<T>>>>,
    on_choose: OnChoose<T>,
    placeholder: Option<TextProp>,
    label: Option<TextProp>,
    grow: bool,
    field: Option<FieldLink>,
    /// The text field, made when mounted
    children: Vec<Box<dyn Widget>>,
    /// The list is asked for (typed into, or Down)
    wanted: ArcRwSignal<bool>,
    /// The highlighted option's index
    active: ArcRwSignal<Option<usize>>,
    open: Rc<Cell<bool>>,
    ids: RowIds,
    layer: Option<WidgetId>,
    watch: Option<Watch>,
    bounds: Rect,
}

/// The enabled options matching `query`, in list order (their indexes)
fn matching<T>(options: &[SelectOption<T>], query: &str) -> Vec<usize> {
    rows(options, query)
        .into_iter()
        .filter_map(|r| match r {
            Row::Option(i) if !options[i].disabled => Some(i),
            _ => None,
        })
        .collect()
}

impl<T: Clone + PartialEq + Send + Sync + 'static> ComboBox<T> {
    /// The text, both ways: typing sets it, a chosen option's label too
    pub fn new(text: ArcRwSignal<String>) -> Self {
        Self {
            id: WidgetId::next(),
            text,
            options: Vec::new(),
            shared: None,
            on_choose: Rc::new(RefCell::new(None)),
            placeholder: None,
            label: None,
            grow: false,
            field: None,
            children: Vec::new(),
            wanted: ArcRwSignal::new(false),
            active: ArcRwSignal::new(None),
            open: Rc::new(Cell::new(false)),
            ids: Rc::default(),
            layer: None,
            watch: None,
            bounds: Rect::ZERO,
        }
    }

    pub fn option(mut self, option: SelectOption<T>) -> Self {
        self.options.push(option);
        self
    }

    pub fn options(mut self, options: impl IntoIterator<Item = SelectOption<T>>) -> Self {
        self.options.extend(options);
        self
    }

    /// An option was taken (its label is the text already)
    pub fn on_choose(self, f: impl FnMut(T) + 'static) -> Self {
        *self.on_choose.borrow_mut() = Some(Box::new(f));
        self
    }

    pub fn placeholder(mut self, text: impl IntoText) -> Self {
        self.placeholder = Some(TextProp::new(text));
        self
    }

    /// Its accessible name without a visible label; otherwise put it in a `Field`
    pub fn label(mut self, text: impl IntoText) -> Self {
        self.label = Some(TextProp::new(text));
        self
    }

    pub fn grow(mut self) -> Self {
        self.grow = true;
        self
    }

    /// Takes the option at `i`: its label becomes the text
    fn chooser(&self) -> Rc<dyn Fn(usize)> {
        let options = self.shared.clone().expect("mounted");
        let (text, on, wanted, active) = (
            self.text.clone(),
            self.on_choose.clone(),
            self.wanted.clone(),
            self.active.clone(),
        );
        Rc::new(move |i: usize| {
            let o = &options[i];
            text.set(o.label.clone());
            if let Some(f) = &mut *on.borrow_mut() {
                f(o.value.clone());
            }
            wanted.set(false);
            active.set(None);
        })
    }

    fn build(&mut self) {
        let options = self.shared.clone().expect("mounted");
        let choose = self.chooser();
        let mut field = TextField::new().value(self.text.clone());
        if self.grow {
            field = field.grow();
        }
        if let Some(p) = self.placeholder.take() {
            field = field.placeholder(p.signal());
        }
        if let Some(l) = self.label.take() {
            field = field.label(l.signal());
        }
        field.on_input = Some(Box::new({
            let (wanted, active) = (self.wanted.clone(), self.active.clone());
            move |typed: &str| {
                // another text: the list follows it, the highlight starts over
                wanted.set(!typed.trim().is_empty());
                active.set(None);
            }
        }));
        field.on_key = Some(Box::new({
            let (wanted, active, open, text) = (
                self.wanted.clone(),
                self.active.clone(),
                self.open.clone(),
                self.text.clone(),
            );
            let options = options.clone();
            move |press: &KeyPress, _cx: &mut EventCx<'_>| {
                let key = &press.key;
                if !open.get() {
                    // Down asks for the list
                    if matches!(key, Key::Named(NamedKey::ArrowDown)) {
                        wanted.set(true);
                        return true;
                    }
                    return false;
                }
                let hits = matching(&options, &text.get_untracked());
                let n = hits.len();
                let at = active
                    .get_untracked()
                    .and_then(|a| hits.iter().position(|&i| i == a));
                let to = match key {
                    Key::Named(NamedKey::ArrowDown) if n > 0 => at.map_or(0, |p| (p + 1) % n),
                    Key::Named(NamedKey::ArrowUp) if n > 0 => at.map_or(n - 1, |p| (p + n - 1) % n),
                    Key::Named(NamedKey::Enter) => {
                        return match active.get_untracked() {
                            Some(i) => {
                                choose(i);
                                true
                            }
                            None => false,
                        };
                    }
                    _ => return false,
                };
                active.set(Some(hits[to]));
                true
            }
        }));
        field.on_blur = Some(Box::new({
            let wanted = self.wanted.clone();
            move || wanted.set(false)
        }));
        field.combo = Some(Box::new({
            let (open, active, ids) = (self.open.clone(), self.active.clone(), self.ids.clone());
            move || Combo {
                expanded: open.get(),
                active: active
                    .get_untracked()
                    .filter(|_| open.get())
                    .and_then(|i| ids.borrow().get(&i).copied()),
            }
        }));
        if let Some(link) = self.field.take() {
            field.link_field(link);
        }
        self.children = vec![Box::new(field)];
    }

    /// The list under the field
    fn panel(&self) -> Flex {
        let options = self.shared.clone().expect("mounted");
        let choose = self.chooser();
        let values: Vec<T> = options.iter().map(|o| o.value.clone()).collect();
        Flex::stack().child(OptionList::new(
            ListSetup {
                options: options.clone(),
                query: self.text.clone().into(),
                selected: Arc::new(|_: &T| false),
                selection: ArcSignal::stored(Vec::new()),
                choose: Rc::new(move |v: T| {
                    if let Some(i) = values.iter().position(|x| *x == v) {
                        choose(i);
                    }
                }),
                prefer_selected: false,
                on_top: None,
                label: String::new(),
                multiple: false,
                active: Some(self.active.clone().into()),
                owner: self.children.first().map(|c| c.id()),
            },
            self.ids.clone(),
        ))
    }
}

impl<T: Clone + PartialEq + Send + Sync + 'static> Widget for ComboBox<T> {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut [Box<dyn Widget>] {
        &mut self.children
    }

    /// Its text field is laid out in its place
    fn layout_transparent(&self) -> bool {
        true
    }

    fn link_field(&mut self, link: FieldLink) {
        self.field = Some(link);
    }

    fn mount(&mut self, cx: &mut MountCx<'_>) {
        if self.children.is_empty() {
            self.shared = Some(Arc::new(self.options.clone()));
            self.build();
        }
        let (text, wanted) = (self.text.clone(), self.wanted.clone());
        self.watch = Some(cx.watch(self.id, move || {
            text.track();
            wanted.track();
        }));
    }

    /// The list shows while it is wanted and has matches
    fn update(&mut self, cx: &mut LayoutCx<'_>) -> Change {
        let options = self.shared.clone().expect("mounted");
        let show = self.wanted.get_untracked()
            && !matching(&options, &self.text.get_untracked()).is_empty();
        match (show, self.layer) {
            (true, None) => {
                let field = &self.children[0];
                let wanted = self.wanted.clone();
                self.layer = Some(cx.open_popup(Popup {
                    owner: field.id(),
                    anchor: field.bounds(),
                    placement: Placement::BOTTOM_START,
                    arrow: false,
                    label: String::new(),
                    content: Box::new(self.panel()),
                    on_dismiss: Some(Box::new(move || wanted.set(false))),
                    list: true,
                    keep_focus: true,
                }));
                self.open.set(true);
            }
            (false, Some(layer)) => {
                cx.close_popup(layer);
                self.layer = None;
                self.open.set(false);
                self.active.set(None);
            }
            _ => {}
        }
        Change::NONE
    }

    fn style(&self, _theme: &Theme) -> taffy::Style {
        taffy::Style::default()
    }

    fn set_bounds(&mut self, _cx: &mut LayoutCx<'_>, bounds: Rect) {
        self.bounds = bounds;
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_skip_headings_and_disabled_options() {
        let options = vec![
            SelectOption::new(1, "Berlin").group("Germany"),
            SelectOption::new(2, "Bern").group("Switzerland").disabled(),
            SelectOption::new(3, "Bergen").group("Norway"),
        ];
        assert_eq!(matching(&options, "ber"), [0, 2]);
        assert_eq!(matching(&options, "zz"), Vec::<usize>::new());
    }
}

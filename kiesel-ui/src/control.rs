//! Control flow, as in SolidJS: [`Show`] a widget while a condition holds,
//! [`For`] one widget per item of a list. Both are layout-transparent – their
//! widgets are laid out by the container around them, like CSS `display: contents`.
//!
//! Widgets made here live as long as their condition or item: `Show` makes a
//! fresh one each time the condition turns true and drops it when it turns
//! false; `For` keeps an item's widget – and its state: focus, typed text –
//! while the item's key stays in the list, and drops it when the key goes.
//! Dropped widgets stop watching their signals; focus moves on (see the shell).

use std::collections::HashMap;
use std::hash::Hash;

use reactive_graph::traits::{GetUntracked, Track, WithUntracked};
use reactive_graph::wrappers::read::ArcSignal;
use vello_cpu::kurbo::Rect;

use crate::reactive::{MountCx, Watch};
use crate::widget::{Change, LayoutCx, Widget, WidgetId};

type Make = Box<dyn FnMut() -> Box<dyn Widget>>;

/// `when` true: the widget from `make`; false: the `fallback`'s, or nothing
pub struct Show {
    id: WidgetId,
    when: ArcSignal<bool>,
    watch: Option<Watch>,
    make: Make,
    fallback: Option<Make>,
    /// What is shown: `None` before the first update
    shown: Option<bool>,
    children: Vec<Box<dyn Widget>>,
    bounds: Rect,
}

impl Show {
    pub fn new<W: Widget + 'static>(
        when: impl Into<ArcSignal<bool>>,
        mut make: impl FnMut() -> W + 'static,
    ) -> Self {
        Self {
            id: WidgetId::next(),
            when: when.into(),
            watch: None,
            make: Box::new(move || Box::new(make())),
            fallback: None,
            shown: None,
            children: Vec::new(),
            bounds: Rect::ZERO,
        }
    }

    /// Shown while `when` is false
    pub fn fallback<W: Widget + 'static>(mut self, mut make: impl FnMut() -> W + 'static) -> Self {
        self.fallback = Some(Box::new(move || Box::new(make())));
        self
    }
}

impl Widget for Show {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut [Box<dyn Widget>] {
        &mut self.children
    }

    fn layout_transparent(&self) -> bool {
        true
    }

    fn mount(&mut self, cx: &mut MountCx<'_>) {
        let when = self.when.clone();
        self.watch = Some(cx.watch(self.id, move || when.track()));
    }

    fn update(&mut self, cx: &mut LayoutCx<'_>) -> Change {
        let when = self.when.get_untracked();
        if self.shown == Some(when) {
            return Change::NONE;
        }
        self.shown = Some(when);
        // the old widget goes (and with it its watches); a new one comes
        self.children.clear();
        let make = if when {
            Some(&mut self.make)
        } else {
            self.fallback.as_mut()
        };
        if let Some(make) = make {
            let mut w = make();
            cx.mount(w.as_mut());
            self.children.push(w);
        }
        Change::LAYOUT
    }

    fn set_bounds(&mut self, _cx: &mut LayoutCx<'_>, bounds: Rect) {
        self.bounds = bounds;
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }
}

type MakeItem<T> = Box<dyn FnMut(&T) -> Box<dyn Widget>>;

/// One widget per item of `each`, keyed: an item's widget stays while its key
/// does, moving with it when the order changes.
///
/// The widget is made once per key, from the item as it was then. Parts of an
/// item that change while it stays in the list belong in signals inside it.
pub struct For<T: Send + Sync + 'static, K> {
    id: WidgetId,
    each: ArcSignal<Vec<T>>,
    watch: Option<Watch>,
    key: Box<dyn Fn(&T) -> K>,
    make: MakeItem<T>,
    keys: Vec<K>,
    children: Vec<Box<dyn Widget>>,
    started: bool,
    bounds: Rect,
}

impl<T: Send + Sync + 'static, K: Eq + Hash + Clone + 'static> For<T, K> {
    pub fn new<W: Widget + 'static>(
        each: impl Into<ArcSignal<Vec<T>>>,
        key: impl Fn(&T) -> K + 'static,
        mut make: impl FnMut(&T) -> W + 'static,
    ) -> Self {
        Self {
            id: WidgetId::next(),
            each: each.into(),
            watch: None,
            key: Box::new(key),
            make: Box::new(move |t| Box::new(make(t))),
            keys: Vec::new(),
            children: Vec::new(),
            started: false,
            bounds: Rect::ZERO,
        }
    }
}

impl<T: Send + Sync + 'static, K: Eq + Hash + Clone + 'static> Widget for For<T, K> {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut [Box<dyn Widget>] {
        &mut self.children
    }

    fn layout_transparent(&self) -> bool {
        true
    }

    fn mount(&mut self, cx: &mut MountCx<'_>) {
        let each = self.each.clone();
        self.watch = Some(cx.watch(self.id, move || each.track()));
    }

    fn update(&mut self, cx: &mut LayoutCx<'_>) -> Change {
        let keys: Vec<K> = self
            .each
            .with_untracked(|items| items.iter().map(|t| (self.key)(t)).collect());
        if self.started && keys == self.keys {
            return Change::NONE;
        }
        self.started = true;
        // keep the widgets of keys that stay, in the new order; make the rest
        let mut old: HashMap<K, Box<dyn Widget>> =
            self.keys.drain(..).zip(self.children.drain(..)).collect();
        let mut fresh = Vec::new();
        let make = &mut self.make;
        let children: Vec<Box<dyn Widget>> = self.each.with_untracked(|items| {
            items
                .iter()
                .zip(&keys)
                .enumerate()
                .map(|(i, (item, k))| {
                    old.remove(k).unwrap_or_else(|| {
                        fresh.push(i);
                        make(item)
                    })
                })
                .collect()
        });
        self.children = children;
        self.keys = keys;
        // what is left in `old` is dropped here – with its watches
        drop(old);
        for i in fresh {
            cx.mount(self.children[i].as_mut());
        }
        Change::LAYOUT
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
    use reactive_graph::signal::ArcRwSignal;
    use reactive_graph::traits::{Set, Update};
    use vello_cpu::kurbo::Size;

    use super::*;
    use crate::widget::tests::lay_out;
    use crate::widget::{find, focus_order};
    use crate::{Button, Flex, Label};

    fn ids(w: &dyn Widget) -> Vec<WidgetId> {
        w.children().iter().map(|c| c.id()).collect()
    }

    #[test]
    fn for_keeps_the_widgets_of_staying_keys() {
        let list = ArcRwSignal::new(vec!["a", "b", "c"]);
        let mut each = For::new(list.clone(), |s: &&str| *s, |s| Button::new(*s));
        lay_out(&mut each, Size::new(300.0, 300.0));
        let before = ids(&each);
        assert_eq!(before.len(), 3);

        list.set(vec!["c", "a", "d"]);
        lay_out(&mut each, Size::new(300.0, 300.0));
        let after = ids(&each);
        assert_eq!(after[0], before[2], "c moved to the front, same widget");
        assert_eq!(after[1], before[0], "a stayed");
        assert!(!before.contains(&after[2]), "d is new");

        list.update(|l| l.clear());
        lay_out(&mut each, Size::new(300.0, 300.0));
        assert!(each.children().is_empty());
    }

    #[test]
    fn for_and_show_lay_out_in_their_parent() {
        let list = ArcRwSignal::new(vec![1, 2]);
        let more = ArcRwSignal::new(false);
        let mut stack = Flex::stack()
            .child(Label::new("top"))
            .child(For::new(
                list.clone(),
                |n: &i32| *n,
                |n| Button::new(n.to_string()),
            ))
            .child(Show::new(more.clone(), || Label::new("more")).fallback(|| Label::new("less")));
        lay_out(&mut stack, Size::new(300.0, 400.0));
        // all leaves stack below each other, as direct children of the stack would
        let mut ys = Vec::new();
        crate::widget::walk(&stack, &mut |w| {
            if w.children().is_empty() {
                ys.push(w.bounds().y0);
            }
        });
        assert_eq!(ys.len(), 4, "top, 1, 2, less");
        assert!(ys.windows(2).all(|p| p[1] > p[0]), "{ys:?}");
        // the For covers its buttons
        let for_ = &stack.children()[1];
        assert_eq!(for_.bounds().y0, for_.children()[0].bounds().y0);
        assert_eq!(for_.bounds().y1, for_.children()[1].bounds().y1);

        more.set(true);
        list.set(vec![1, 2, 3]);
        lay_out(&mut stack, Size::new(300.0, 400.0));
        assert_eq!(focus_order(&stack).len(), 3, "a third button");
        let show = &stack.children()[2];
        let shown = show.children()[0].id();
        assert!(find(&stack, shown).is_some());
    }
}

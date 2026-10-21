//! The retained widget tree: identity, layout, painting, events, accessibility.
//!
//! Widgets own their children. The shell routes input: pointer events go to the
//! deepest widget under the pointer (or the one that captured it on press),
//! keys to the focused widget. Every widget's id doubles as its AccessKit node id.

use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use vello_cpu::kurbo::{Point, Rect, Size};
use vello_cpu::{RenderContext, Resources};
use winit::cursor::CursorIcon;
use winit::keyboard::Key;

use crate::text::Text;
use crate::theme::Theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WidgetId(NonZeroU64);

/// Id 1 is the window's own accessibility node
static NEXT_ID: AtomicU64 = AtomicU64::new(2);

impl WidgetId {
    pub(crate) const WINDOW: Self = Self(NonZeroU64::MIN);

    pub fn next() -> Self {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        Self(NonZeroU64::new(id).expect("widget ids start at 2"))
    }

    pub fn node(self) -> accesskit::NodeId {
        accesskit::NodeId(self.0.get())
    }

    pub(crate) fn from_node(node: accesskit::NodeId) -> Option<Self> {
        NonZeroU64::new(node.0).map(Self)
    }
}

#[derive(Clone, Debug)]
pub enum Event {
    PointerEnter,
    PointerLeave,
    PointerMove(Point),
    PointerDown(Point),
    PointerUp(Point),
    KeyDown {
        key: Key,
        repeat: bool,
    },
    KeyUp {
        key: Key,
    },
    /// Focus came or went; `visible` is `:focus-visible` (keyboard or assistive technology)
    Focus {
        focused: bool,
        visible: bool,
    },
    /// Assistive technology asks for the default action (a click)
    Activate,
}

pub struct LayoutCx<'a> {
    pub theme: &'a Theme,
    pub text: &'a mut Text,
    pub now: Instant,
}

pub struct PaintCx<'a> {
    pub ctx: &'a mut RenderContext,
    pub res: &'a mut Resources,
    pub theme: &'a Theme,
    pub now: Instant,
}

pub struct EventCx<'a> {
    pub theme: &'a Theme,
    pub now: Instant,
}

pub trait Widget {
    fn id(&self) -> WidgetId;

    fn children(&self) -> &[Box<dyn Widget>] {
        &[]
    }

    fn children_mut(&mut self) -> &mut [Box<dyn Widget>] {
        &mut []
    }

    /// Natural size
    fn measure(&mut self, cx: &mut LayoutCx<'_>) -> Size;

    /// Takes the given bounds (logical pixels, window coordinates); containers place their children
    fn place(&mut self, cx: &mut LayoutCx<'_>, bounds: Rect);

    fn bounds(&self) -> Rect;

    fn paint(&self, cx: &mut PaintCx<'_>) {
        for child in self.children() {
            child.paint(cx);
        }
    }

    fn hit(&self, p: Point) -> bool {
        self.bounds().contains(p)
    }

    fn focusable(&self) -> bool {
        false
    }

    fn event(&mut self, _cx: &mut EventCx<'_>, _event: &Event) {}

    fn cursor(&self) -> CursorIcon {
        CursorIcon::Default
    }

    /// True while the widget needs frames (transitions, spinners)
    fn animating(&self, _now: Instant, _theme: &Theme) -> bool {
        false
    }

    /// Its accessibility node, or `None` to stay out of the tree (its exposed
    /// descendants then belong to the nearest exposed ancestor)
    fn accessibility(&self) -> Option<accesskit::Node> {
        None
    }
}

/// Preorder walk over a tree
pub fn walk<'a>(w: &'a dyn Widget, f: &mut impl FnMut(&'a dyn Widget)) {
    f(w);
    for child in w.children() {
        walk(child.as_ref(), f);
    }
}

pub fn find_mut(w: &mut dyn Widget, id: WidgetId) -> Option<&mut dyn Widget> {
    if w.id() == id {
        return Some(w);
    }
    w.children_mut()
        .iter_mut()
        .find_map(|c| find_mut(c.as_mut(), id))
}

pub fn find(w: &dyn Widget, id: WidgetId) -> Option<&dyn Widget> {
    if w.id() == id {
        return Some(w);
    }
    w.children().iter().find_map(|c| find(c.as_ref(), id))
}

/// The deepest widget under `p`; later siblings are on top
pub fn hit_test(w: &dyn Widget, p: Point) -> Option<WidgetId> {
    if !w.hit(p) {
        return None;
    }
    w.children()
        .iter()
        .rev()
        .find_map(|c| hit_test(c.as_ref(), p))
        .or(Some(w.id()))
}

/// Focusable widgets in tab order (document order)
pub fn focus_order(w: &dyn Widget) -> Vec<WidgetId> {
    let mut ids = Vec::new();
    walk(w, &mut |w| {
        if w.focusable() {
            ids.push(w.id());
        }
    });
    ids
}

/// Accessibility nodes of the exposed widgets under `w`, appended to `out`;
/// returns the ids of the top-most exposed ones (the parent's children)
pub(crate) fn accessibility_nodes(
    w: &dyn Widget,
    out: &mut Vec<(accesskit::NodeId, accesskit::Node)>,
) -> Vec<accesskit::NodeId> {
    let below: Vec<accesskit::NodeId> = w
        .children()
        .iter()
        .flat_map(|c| accessibility_nodes(c.as_ref(), out))
        .collect();
    match w.accessibility() {
        Some(mut node) => {
            node.set_children(below);
            out.push((w.id().node(), node));
            vec![w.id().node()]
        }
        None => below,
    }
}

pub(crate) fn accessible_rect(r: Rect) -> accesskit::Rect {
    accesskit::Rect {
        x0: r.x0,
        y0: r.y0,
        x1: r.x1,
        y1: r.y1,
    }
}

pub(crate) fn any_animating(w: &dyn Widget, now: Instant, theme: &Theme) -> bool {
    w.animating(now, theme)
        || w.children()
            .iter()
            .any(|c| any_animating(c.as_ref(), now, theme))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Appearance, Scheme};
    use crate::{Button, Row};

    fn laid_out() -> (Row, WidgetId, WidgetId) {
        let (a, b) = (Button::new("One"), Button::new("Two"));
        let (ia, ib) = (a.id(), b.id());
        let mut row = Row::new(10.0).child(a).child(b);
        let theme = Theme::new(Scheme::Light, &Appearance::default());
        let mut text = Text::default();
        let mut cx = LayoutCx {
            theme: &theme,
            text: &mut text,
            now: Instant::now(),
        };
        row.place(&mut cx, Rect::new(0.0, 0.0, 400.0, 100.0));
        (row, ia, ib)
    }

    #[test]
    fn hit_test_finds_the_deepest_widget() {
        let (row, a, b) = laid_out();
        let ca = find(&row, a).unwrap().bounds().center();
        let cb = find(&row, b).unwrap().bounds().center();
        assert_eq!(hit_test(&row, ca), Some(a));
        assert_eq!(hit_test(&row, cb), Some(b));
        // between the buttons: the row itself
        assert_eq!(hit_test(&row, Point::new(200.0, 2.0)), Some(row.id()));
        assert_eq!(hit_test(&row, Point::new(-5.0, 50.0)), None);
    }

    #[test]
    fn focus_order_is_document_order() {
        let (row, a, b) = laid_out();
        assert_eq!(focus_order(&row), vec![a, b]);
    }

    #[test]
    fn accessibility_skips_unexposed_containers() {
        let (row, a, b) = laid_out();
        let mut nodes = Vec::new();
        let top = accessibility_nodes(&row, &mut nodes);
        assert_eq!(top, vec![a.node(), b.node()]);
        assert_eq!(nodes.len(), 2);
        assert!(
            nodes
                .iter()
                .all(|(_, n)| n.role() == accesskit::Role::Button)
        );
    }
}

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
use winit::keyboard::{Key, ModifiersState};

use crate::clipboard::Clipboard;
use crate::reactive::MountCx;
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
        /// What the key types, if anything (layout, dead keys and compose applied)
        text: Option<String>,
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
    /// Assistive technology sets the value (text fields)
    SetValue(String),
    /// Assistive technology moves the caret or selection (text fields)
    SetTextSelection(accesskit::TextSelection),
    /// Input method: composition in progress, its result, or text to remove
    Ime(Ime),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ime {
    /// Text being composed; `cursor` is a byte range in it. Empty: composition ended.
    Preedit {
        text: String,
        cursor: Option<(usize, usize)>,
    },
    Commit(String),
    /// Remove this many bytes before and after the caret
    DeleteSurrounding {
        before: usize,
        after: usize,
    },
}

pub struct LayoutCx<'a> {
    pub theme: &'a Theme,
    pub text: &'a mut Text,
    pub now: Instant,
    pub(crate) inbox: &'a std::sync::Arc<crate::reactive::Inbox>,
}

impl LayoutCx<'_> {
    /// Brings a widget created at runtime (and its children) to life: watches
    /// its signals and lets it read their values – call it before adding the
    /// widget to the tree
    pub fn mount(&mut self, w: &mut dyn Widget) {
        mount_all(w, &mut MountCx { inbox: self.inbox });
        let (mut relayout, mut repaint) = (Vec::new(), Vec::new());
        update_all(w, self, &mut relayout, &mut repaint);
    }
}

pub struct PaintCx<'a> {
    pub ctx: &'a mut RenderContext,
    pub res: &'a mut Resources,
    pub theme: &'a Theme,
    pub now: Instant,
    /// The region being redrawn (logical); widgets whose ink misses it can be skipped
    pub clip: Rect,
}

impl PaintCx<'_> {
    /// Paints the children whose ink reaches the redrawn region
    pub fn paint_children(&mut self, children: &[Box<dyn Widget>]) {
        for child in children {
            if child.ink().overlaps(self.clip) {
                child.paint(self);
            }
        }
    }
}

pub struct EventCx<'a> {
    pub theme: &'a Theme,
    pub now: Instant,
    pub text: &'a mut Text,
    pub clipboard: &'a mut Clipboard,
    pub modifiers: ModifiersState,
    /// 1, 2, 3… for single, double, triple clicks (with `PointerDown`)
    pub clicks: u8,
}

/// What changed since the last update – reported by [`Widget::update`]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Change {
    /// Its size may have changed: measure again
    pub layout: bool,
    /// It looks different: redraw its ink
    pub paint: bool,
}

impl Change {
    pub const NONE: Self = Self {
        layout: false,
        paint: false,
    };
    pub const PAINT: Self = Self {
        layout: false,
        paint: true,
    };
    pub const LAYOUT: Self = Self {
        layout: true,
        paint: true,
    };
}

impl std::ops::BitOr for Change {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self {
            layout: self.layout || rhs.layout,
            paint: self.paint || rhs.paint,
        }
    }
}

impl std::ops::BitOrAssign for Change {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = *self | rhs;
    }
}

/// Constraints for [`Widget::measure`], straight from taffy
pub type Known = taffy::Size<Option<f32>>;
pub type Available = taffy::Size<taffy::AvailableSpace>;

pub trait Widget {
    fn id(&self) -> WidgetId;

    fn children(&self) -> &[Box<dyn Widget>] {
        &[]
    }

    fn children_mut(&mut self) -> &mut [Box<dyn Widget>] {
        &mut []
    }

    /// Takes no box of its own in layout: its children are laid out as its
    /// parent's (CSS `display: contents`) – `Show`, `For`
    fn layout_transparent(&self) -> bool {
        false
    }

    /// Its box in layout: display, direction, gaps, padding, size limits (taffy)
    fn style(&self, _theme: &Theme) -> taffy::Style {
        taffy::Style::default()
    }

    /// Leaves only (containers are sized by taffy from their children):
    /// the size for the given constraints
    fn measure(&mut self, _cx: &mut LayoutCx<'_>, _known: Known, _available: Available) -> Size {
        Size::ZERO
    }

    /// The result of layout, in window coordinates (logical pixels)
    fn set_bounds(&mut self, cx: &mut LayoutCx<'_>, bounds: Rect);

    fn bounds(&self) -> Rect;

    /// Everything it paints, shadows and rings included (for redrawing only what changed)
    fn ink(&self) -> Rect {
        self.bounds()
    }

    /// Once, before the first layout: watch the signals it shows (see [`MountCx::watch`])
    fn mount(&mut self, _cx: &mut MountCx<'_>) {}

    /// Called when a watched signal changed or the widget got an event (and once
    /// at the start): read signals untracked, report what changed
    fn update(&mut self, _cx: &mut LayoutCx<'_>) -> Change {
        Change::NONE
    }

    fn paint(&self, cx: &mut PaintCx<'_>) {
        cx.paint_children(self.children());
    }

    fn hit(&self, p: Point) -> bool {
        self.bounds().contains(p)
    }

    fn focusable(&self) -> bool {
        false
    }

    /// Takes text input: the shell turns the input method on while it is focused
    fn accepts_text(&self) -> bool {
        false
    }

    /// Where text is being entered (the caret or the composition), for the input
    /// method's candidate window
    fn ime_area(&self) -> Option<Rect> {
        None
    }

    /// When it next needs a frame without any input (a caret blinking)
    fn wake_at(&self, _now: Instant) -> Option<Instant> {
        None
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

    /// Nodes of its own below its accessibility node (a text field's text runs)
    fn accessibility_children(&self) -> Vec<(accesskit::NodeId, accesskit::Node)> {
        Vec::new()
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
            let own = w.accessibility_children();
            let mut below = below;
            below.extend(own.iter().map(|(id, _)| *id));
            out.extend(own);
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

/// The ink of every widget that is animating
pub(crate) fn animating_inks(w: &dyn Widget, now: Instant, theme: &Theme, out: &mut Vec<Rect>) {
    if w.animating(now, theme) {
        out.push(w.ink());
    }
    for child in w.children() {
        animating_inks(child.as_ref(), now, theme, out);
    }
}

/// Runs [`Widget::update`] everywhere; returns the ids that need layout and the inks to redraw
pub(crate) fn update_all(
    w: &mut dyn Widget,
    cx: &mut LayoutCx<'_>,
    relayout: &mut Vec<WidgetId>,
    repaint: &mut Vec<Rect>,
) {
    let change = w.update(cx);
    if change.layout {
        relayout.push(w.id());
    }
    if change.paint {
        repaint.push(w.ink());
    }
    for child in w.children_mut() {
        update_all(child.as_mut(), cx, relayout, repaint);
    }
}

/// The earliest [`Widget::wake_at`] in the tree, and the inks of the widgets that asked
pub(crate) fn wakes(
    w: &dyn Widget,
    now: Instant,
    next: &mut Option<Instant>,
    inks: &mut Vec<Rect>,
) {
    if let Some(t) = w.wake_at(now) {
        inks.push(w.ink());
        if next.is_none_or(|n| t < n) {
            *next = Some(t);
        }
    }
    for child in w.children() {
        wakes(child.as_ref(), now, next, inks);
    }
}

pub(crate) fn mount_all(w: &mut dyn Widget, cx: &mut MountCx<'_>) {
    w.mount(cx);
    for child in w.children_mut() {
        mount_all(child.as_mut(), cx);
    }
}

/// Every widget's ink, by id
pub(crate) fn inks(w: &dyn Widget, out: &mut std::collections::HashMap<WidgetId, Rect>) {
    out.insert(w.id(), w.ink());
    for child in w.children() {
        inks(child.as_ref(), out);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::layout::Layout;
    use crate::theme::{Appearance, Scheme};
    use crate::{Button, Card, Flex, Justify, Label};

    /// Lays `root` out in a window of `size`
    pub(crate) fn lay_out(root: &mut dyn Widget, size: Size) {
        let theme = Theme::new(Scheme::Light, &Appearance::default());
        let mut text = Text::default();
        let inbox = crate::reactive::Inbox::new(|| {});
        let mut cx = LayoutCx {
            theme: &theme,
            text: &mut text,
            now: Instant::now(),
            inbox: &inbox,
        };
        let (mut relayout, mut repaint) = (Vec::new(), Vec::new());
        update_all(root, &mut cx, &mut relayout, &mut repaint);
        Layout::default().run(root, &mut cx, size, &relayout);
    }

    fn two_buttons() -> (Flex, WidgetId, WidgetId) {
        let (a, b) = (Button::new("One"), Button::new("Two"));
        let (ia, ib) = (a.id(), b.id());
        let mut row = Flex::cluster().justify(Justify::Center).child(a).child(b);
        lay_out(&mut row, Size::new(400.0, 100.0));
        (row, ia, ib)
    }

    #[test]
    fn hit_test_finds_the_deepest_widget() {
        let (row, a, b) = two_buttons();
        let ca = find(&row, a).unwrap().bounds().center();
        let cb = find(&row, b).unwrap().bounds().center();
        assert_eq!(hit_test(&row, ca), Some(a));
        assert_eq!(hit_test(&row, cb), Some(b));
        // above the buttons: the row itself
        assert_eq!(hit_test(&row, Point::new(200.0, 2.0)), Some(row.id()));
        assert_eq!(hit_test(&row, Point::new(-5.0, 50.0)), None);
    }

    #[test]
    fn focus_order_is_document_order() {
        let (row, a, b) = two_buttons();
        assert_eq!(focus_order(&row), vec![a, b]);
    }

    #[test]
    fn accessibility_skips_unexposed_containers() {
        let (row, a, b) = two_buttons();
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

    #[test]
    fn cluster_centres_and_gaps() {
        let (row, a, b) = two_buttons();
        let (ra, rb) = (
            find(&row, a).unwrap().bounds(),
            find(&row, b).unwrap().bounds(),
        );
        assert_eq!(rb.x0 - ra.x1, 16.0, "gap --size-3");
        assert!(
            ((ra.x0 - 0.0) - (400.0 - rb.x1)).abs() <= 1.0,
            "centred: {ra:?} {rb:?}"
        );
        assert_eq!(ra.height(), 40.0, "--ksl-control");
    }

    #[test]
    fn a_centred_label_keeps_its_line() {
        // a fractional text width must not lose its last word to rounding
        let label = Label::caption("Following the system (dark)");
        let id = label.id();
        let mut card = Card::new().max_width(420.0).child(
            Flex::stack()
                .align(crate::Align::Center)
                .child(Button::new("x"))
                .child(label),
        );
        lay_out(&mut card, Size::new(520.0, 400.0));
        assert_eq!(
            find(&card, id).unwrap().bounds().height(),
            18.0,
            "one line of 12 px × 1.5"
        );
    }

    #[test]
    fn labels_wrap_to_the_card() {
        let text =
            "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor.";
        let label = Label::new(text);
        let id = label.id();
        let mut card = Card::new().max_width(300.0).child(label);
        lay_out(&mut card, Size::new(800.0, 600.0));
        assert_eq!(card.bounds().width(), 300.0, "max-width");
        let l = find(&card, id).unwrap().bounds();
        assert_eq!(l.width(), 300.0 - 2.0 * 24.0, "padding --size-5");
        assert!(
            l.height() >= 3.0 * 24.0,
            "16px × 1.5 per line, several lines: {l:?}"
        );

        let mut wide = Card::new().child(Label::new(text));
        lay_out(&mut wide, Size::new(2000.0, 600.0));
        let one_line = wide.children()[0].bounds().height();
        assert!(one_line < 30.0, "one line when there is room: {one_line}");
    }
}

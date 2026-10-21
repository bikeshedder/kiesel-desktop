//! Layout through taffy: a persistent taffy tree mirrors the widget tree.
//!
//! Each pass syncs styles and children (cheap when nothing changed), marks the
//! nodes of widgets that reported a layout change dirty, and lets taffy
//! compute – it re-measures only dirty leaves, so unchanged text isn't shaped
//! or broken again. The results go back to the widgets as window coordinates.
//!
//! Layout-transparent widgets (`Show`, `For` – like CSS `display: contents`)
//! get no node of their own: their children's nodes join the parent's, and
//! their bounds are the union of their children's.

use std::collections::{HashMap, HashSet};

use taffy::{AvailableSpace, NodeId, TaffyTree};
use vello_cpu::kurbo::{Point, Rect, Size};

use crate::widget::{self, LayoutCx, Widget, WidgetId};

/// How the root is sized
#[derive(Clone, Copy)]
enum Fit {
    /// It fills the window
    Fill(Size),
    /// As wide and high as its content
    Natural,
    /// This wide, as high as its content
    Width(f32),
}

pub(crate) struct Layout {
    tree: TaffyTree<WidgetId>,
    nodes: HashMap<WidgetId, NodeId>,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            tree: TaffyTree::new(),
            nodes: HashMap::new(),
        }
    }
}

impl Layout {
    /// Lays out `root` to fill `size`; `dirty` lists widgets whose own size may have changed
    pub fn run(
        &mut self,
        root: &mut dyn Widget,
        cx: &mut LayoutCx<'_>,
        size: Size,
        dirty: &[WidgetId],
    ) {
        self.compute(root, cx, Fit::Fill(size), dirty);
        self.apply(root, cx, Point::ZERO);
    }

    /// Lays out `root` at its natural size (as wide and high as its content, within
    /// its own size limits) without placing it; see [`Layout::place_at`]
    pub fn fit(
        &mut self,
        root: &mut dyn Widget,
        cx: &mut LayoutCx<'_>,
        dirty: &[WidgetId],
    ) -> Size {
        let node = self.compute(root, cx, Fit::Natural, dirty);
        let l = self.tree.layout(node).expect("laid out");
        let width = l.size.width;
        // Text wraps only once the width is known: lay out again at that width,
        // or a paragraph that wraps keeps the height of one line
        let node = self.compute(root, cx, Fit::Width(width), &[]);
        let l = self.tree.layout(node).expect("laid out");
        Size::new(f64::from(l.size.width), f64::from(l.size.height))
    }

    /// Places `root` (laid out by [`Layout::fit`]) with its top-left corner at `origin`
    pub fn place_at(&self, root: &mut dyn Widget, cx: &mut LayoutCx<'_>, origin: Point) {
        self.apply(root, cx, origin);
    }

    fn compute(
        &mut self,
        root: &mut dyn Widget,
        cx: &mut LayoutCx<'_>,
        fit: Fit,
        dirty: &[WidgetId],
    ) -> NodeId {
        let mut seen = HashSet::new();
        let root_node = self.sync_node(root, cx, &mut seen, matches!(fit, Fit::Fill(_)));
        for id in dirty {
            if let Some(&node) = self.nodes.get(id) {
                self.tree.mark_dirty(node).expect("node exists");
            }
        }
        // widgets that left the tree take their nodes with them
        self.nodes.retain(|id, node| {
            let keep = seen.contains(id);
            if !keep {
                let _ = self.tree.remove(*node);
            }
            keep
        });

        let available = match fit {
            Fit::Fill(size) => taffy::Size {
                width: AvailableSpace::Definite(size.width as f32),
                height: AvailableSpace::Definite(size.height as f32),
            },
            Fit::Natural => taffy::Size {
                width: AvailableSpace::MaxContent,
                height: AvailableSpace::MaxContent,
            },
            Fit::Width(w) => taffy::Size {
                width: AvailableSpace::Definite(w),
                height: AvailableSpace::MaxContent,
            },
        };
        self.tree
            .compute_layout_with_measure(root_node, available, |input, _, id, style| {
                let Some(id) = id.copied() else {
                    return taffy::LayoutOutput::HIDDEN;
                };
                taffy::compute_leaf_layout(
                    input,
                    style,
                    |_, _| 0.0,
                    |known, available| {
                        let Some(w) = widget::find_mut(root, id) else {
                            return taffy::Size::ZERO;
                        };
                        let s = w.measure(cx, known, available);
                        taffy::Size {
                            width: s.width as f32,
                            height: s.height as f32,
                        }
                    },
                )
            })
            .expect("layout");
        root_node
    }

    /// The nodes `w` puts into its parent: its own, or its children's if it is transparent
    fn sync(
        &mut self,
        w: &dyn Widget,
        cx: &LayoutCx<'_>,
        seen: &mut HashSet<WidgetId>,
    ) -> Vec<NodeId> {
        if w.detached() {
            // popup layers lay themselves out
            return Vec::new();
        }
        if w.layout_transparent() {
            return w
                .children()
                .iter()
                .flat_map(|c| self.sync(c.as_ref(), cx, seen))
                .collect();
        }
        vec![self.sync_node(w, cx, seen, false)]
    }

    fn sync_node(
        &mut self,
        w: &dyn Widget,
        cx: &LayoutCx<'_>,
        seen: &mut HashSet<WidgetId>,
        is_root: bool,
    ) -> NodeId {
        let children: Vec<NodeId> = w
            .children()
            .iter()
            .flat_map(|c| self.sync(c.as_ref(), cx, seen))
            .collect();
        let mut style = w.style(cx.theme);
        if is_root {
            // the root fills the window
            style.size = taffy::Size {
                width: taffy::Dimension::percent(1.0),
                height: taffy::Dimension::percent(1.0),
            };
        }
        let id = w.id();
        seen.insert(id);
        let node = match self.nodes.get(&id) {
            Some(&node) => {
                if self.tree.style(node).is_ok_and(|s| *s != style) {
                    self.tree.set_style(node, style).expect("node exists");
                }
                node
            }
            None => {
                let node = self
                    .tree
                    .new_leaf_with_context(style, id)
                    .expect("new node");
                self.nodes.insert(id, node);
                node
            }
        };
        if self.tree.children(node).is_ok_and(|c| c != children) {
            self.tree
                .set_children(node, &children)
                .expect("node exists");
        }
        node
    }

    /// Places the widgets again from the last layout – after scrolling, which
    /// moves children without changing any size
    pub fn place(&self, root: &mut dyn Widget, cx: &mut LayoutCx<'_>) {
        self.apply(root, cx, Point::ZERO);
    }

    fn apply(&self, w: &mut dyn Widget, cx: &mut LayoutCx<'_>, parent: Point) {
        if w.layout_transparent() {
            // the children are placed in the parent; this one covers them
            let mut union: Option<Rect> = None;
            for child in w.children_mut() {
                self.apply(child.as_mut(), cx, parent);
                let b = child.bounds();
                union = Some(union.map_or(b, |u| u.union(b)));
            }
            w.set_bounds(
                cx,
                union.unwrap_or(Rect::from_origin_size(parent, Size::ZERO)),
            );
            return;
        }
        let Some(&node) = self.nodes.get(&w.id()) else {
            return;
        };
        let l = self.tree.layout(node).expect("laid out");
        let origin = parent + (f64::from(l.location.x), f64::from(l.location.y));
        let size = Size::new(f64::from(l.size.width), f64::from(l.size.height));
        w.set_bounds(cx, Rect::from_origin_size(origin, size));
        // a scrolled widget moves its children up and left
        let inner = origin - w.content_offset();
        for child in w.children_mut() {
            self.apply(child.as_mut(), cx, inner);
        }
        if w.children_placed() {
            let inner = origin - w.content_offset();
            for child in w.children_mut() {
                self.apply(child.as_mut(), cx, inner);
            }
        }
    }
}

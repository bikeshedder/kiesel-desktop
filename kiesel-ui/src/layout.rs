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
        let mut seen = HashSet::new();
        let root_node = self.sync_node(root, cx, &mut seen, true);
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

        let available = taffy::Size {
            width: AvailableSpace::Definite(size.width as f32),
            height: AvailableSpace::Definite(size.height as f32),
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
        self.apply(root, cx, Point::ZERO);
    }

    /// The nodes `w` puts into its parent: its own, or its children's if it is transparent
    fn sync(
        &mut self,
        w: &dyn Widget,
        cx: &LayoutCx<'_>,
        seen: &mut HashSet<WidgetId>,
    ) -> Vec<NodeId> {
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
        for child in w.children_mut() {
            self.apply(child.as_mut(), cx, origin);
        }
    }
}

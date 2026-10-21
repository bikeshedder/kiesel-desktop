//! Signals reach the widgets that read them – the SolidJS model, on reactive_graph.
//!
//! When the shell starts, every widget is mounted: it [`watch`](MountCx::watch)es
//! the signals it shows. A watch is an immediate effect; when one of its signals
//! changes – on any thread – it queues the widget and wakes the event loop. The
//! shell then calls [`Widget::update`](crate::Widget::update) on exactly the
//! queued widgets, which read their signals untracked and report what changed.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use reactive_graph::effect::ImmediateEffect;

use crate::widget::WidgetId;

/// Widgets whose signals changed, and how to wake the event loop
pub(crate) struct Inbox {
    queue: Mutex<Vec<WidgetId>>,
    wake: Box<dyn Fn() + Send + Sync>,
}

impl Inbox {
    /// `wake` gets the event loop going (winit's proxy); called from any thread
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Arc<Self> {
        Arc::new(Self {
            queue: Mutex::new(Vec::new()),
            wake: Box::new(wake),
        })
    }

    fn push(&self, id: WidgetId) {
        let mut queue = self.queue.lock().unwrap();
        if !queue.contains(&id) {
            queue.push(id);
        }
        drop(queue);
        (self.wake)();
    }

    pub fn take(&self) -> Vec<WidgetId> {
        std::mem::take(&mut *self.queue.lock().unwrap())
    }
}

/// What a widget gets when it is mounted
pub struct MountCx<'a> {
    pub(crate) inbox: &'a Arc<Inbox>,
}

/// A running watch; dropping it stops it. Keep it in the widget.
#[must_use = "a watch stops when dropped"]
pub struct Watch(#[allow(dead_code)] ImmediateEffect);

impl MountCx<'_> {
    /// Runs `track` now and again whenever a signal it reads changes; every
    /// change after the first run queues `id` for an update
    pub fn watch(&self, id: WidgetId, track: impl Fn() + Send + Sync + 'static) -> Watch {
        let inbox = Arc::clone(self.inbox);
        let first = AtomicBool::new(true);
        Watch(ImmediateEffect::new_isomorphic(move || {
            track();
            if !first.swap(false, Ordering::AcqRel) {
                inbox.push(id);
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use reactive_graph::signal::ArcRwSignal;
    use reactive_graph::traits::Set;
    use vello_cpu::kurbo::Size;

    use super::*;
    use crate::widget::tests::lay_out;
    use crate::widget::{self, Widget};
    use crate::{Button, Flex, Label};

    fn inbox() -> (Arc<Inbox>, Arc<AtomicUsize>) {
        let wakes = Arc::new(AtomicUsize::new(0));
        let w = Arc::clone(&wakes);
        (
            Inbox::new(move || {
                w.fetch_add(1, Ordering::SeqCst);
            }),
            wakes,
        )
    }

    #[test]
    fn a_signal_queues_exactly_its_widgets() {
        let (inbox, wakes) = inbox();
        let (text, loading) = (ArcRwSignal::new("a".to_string()), ArcRwSignal::new(false));
        let (label, button, other) = (
            Label::bound(text.clone()),
            Button::new("B").loading(loading.clone()),
            Label::new("x"),
        );
        let (l, b) = (label.id(), button.id());
        let mut root = Flex::stack().child(label).child(button).child(other);
        widget::mount_all(&mut root, &mut MountCx { inbox: &inbox });
        lay_out(&mut root, Size::new(300.0, 200.0));
        assert!(inbox.take().is_empty(), "mounting reports nothing");

        text.set("b".into());
        assert_eq!(inbox.take(), vec![l]);
        loading.set(true);
        text.set("c".into());
        loading.set(false);
        assert_eq!(
            inbox.take(),
            vec![b, l],
            "each widget once, in order of first change"
        );
        assert_eq!(wakes.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn signals_set_on_other_threads_arrive() {
        let (inbox, wakes) = inbox();
        let text = ArcRwSignal::new(String::new());
        let mut label = Label::bound(text.clone());
        widget::mount_all(&mut label, &mut MountCx { inbox: &inbox });
        std::thread::spawn(move || text.set("from a thread".into()))
            .join()
            .unwrap();
        assert_eq!(inbox.take(), vec![label.id()]);
        assert_eq!(wakes.load(Ordering::SeqCst), 1);
    }
}

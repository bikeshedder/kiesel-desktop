//! Accessibility through AccessKit, without accesskit_winit (it pins winit 0.30).
//!
//! AccessKit calls its handlers on its own thread. They only queue what was
//! asked and wake the event loop; the shell answers from there – building the
//! tree when assistive technology first shows up, then on every change.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use accesskit::{ActionRequest, TreeUpdate};
use winit::event_loop::EventLoopProxy;

#[derive(Default)]
struct Inbox {
    requests: Mutex<Vec<ActionRequest>>,
    /// AT asked for the tree; the next update must be a full one
    activated: AtomicBool,
}

pub(crate) struct Accessibility {
    #[cfg(target_os = "linux")]
    adapter: accesskit_unix::Adapter,
    inbox: Arc<Inbox>,
}

#[cfg(target_os = "linux")]
struct Handler {
    inbox: Arc<Inbox>,
    proxy: EventLoopProxy,
}

#[cfg(target_os = "linux")]
impl accesskit::ActivationHandler for Handler {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        self.inbox.activated.store(true, Ordering::Release);
        self.proxy.wake_up();
        None
    }
}

#[cfg(target_os = "linux")]
impl accesskit::ActionHandler for Handler {
    fn do_action(&mut self, request: ActionRequest) {
        self.inbox.requests.lock().unwrap().push(request);
        self.proxy.wake_up();
    }
}

#[cfg(target_os = "linux")]
impl accesskit::DeactivationHandler for Handler {
    fn deactivate_accessibility(&mut self) {}
}

impl Accessibility {
    pub fn new(proxy: EventLoopProxy) -> Self {
        let inbox = Arc::new(Inbox::default());
        #[cfg(target_os = "linux")]
        {
            let handler = || Handler {
                inbox: Arc::clone(&inbox),
                proxy: proxy.clone(),
            };
            let adapter = accesskit_unix::Adapter::new(handler(), handler(), handler());
            Self { adapter, inbox }
        }
        #[cfg(not(target_os = "linux"))]
        {
            // Windows and macOS adapters come with those platforms
            let _ = proxy;
            Self { inbox }
        }
    }

    /// Applies an update – only built while assistive technology listens
    pub fn update(&mut self, build: impl FnOnce() -> TreeUpdate) {
        #[cfg(target_os = "linux")]
        self.adapter.update_if_active(build);
        #[cfg(not(target_os = "linux"))]
        let _ = build;
    }

    pub fn window_focused(&mut self, focused: bool) {
        #[cfg(target_os = "linux")]
        self.adapter.update_window_focus_state(focused);
        #[cfg(not(target_os = "linux"))]
        let _ = focused;
    }

    /// Requests queued since the last call
    pub fn take_requests(&self) -> Vec<ActionRequest> {
        self.inbox.activated.store(false, Ordering::Release);
        std::mem::take(&mut *self.inbox.requests.lock().unwrap())
    }
}

//! The system clipboard: on Wayland through the window's own connection
//! (smithay-clipboard), elsewhere through arboard.

use raw_window_handle::RawDisplayHandle;

pub struct Clipboard {
    inner: Inner,
}

enum Inner {
    #[cfg(all(unix, not(target_os = "macos")))]
    Wayland(smithay_clipboard::Clipboard),
    Arboard(arboard::Clipboard),
    /// No clipboard (tests, or it failed to open): copy and paste do nothing
    None,
}

impl Clipboard {
    pub(crate) fn none() -> Self {
        Self { inner: Inner::None }
    }

    pub(crate) fn new(display: RawDisplayHandle) -> Self {
        #[cfg(all(unix, not(target_os = "macos")))]
        if let RawDisplayHandle::Wayland(handle) = display {
            // SAFETY: the display outlives the clipboard – the shell drops it with the window
            let clipboard = unsafe { smithay_clipboard::Clipboard::new(handle.display.as_ptr()) };
            return Self {
                inner: Inner::Wayland(clipboard),
            };
        }
        let _ = display;
        match arboard::Clipboard::new() {
            Ok(c) => Self {
                inner: Inner::Arboard(c),
            },
            Err(_) => Self::none(),
        }
    }

    pub fn get(&mut self) -> Option<String> {
        match &mut self.inner {
            #[cfg(all(unix, not(target_os = "macos")))]
            Inner::Wayland(c) => c.load().ok(),
            Inner::Arboard(c) => c.get_text().ok(),
            Inner::None => None,
        }
    }

    pub fn set(&mut self, text: &str) {
        match &mut self.inner {
            #[cfg(all(unix, not(target_os = "macos")))]
            Inner::Wayland(c) => c.store(text),
            Inner::Arboard(c) => {
                let _ = c.set_text(text);
            }
            Inner::None => {}
        }
    }
}

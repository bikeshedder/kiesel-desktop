//! Two Kiesel buttons, centred in the window, sharing one `loading` signal:
//! "Save changes" toggles it, "Reset" clears it. Tab / Shift+Tab move focus,
//! Space and Enter press; assistive technology sees both buttons.
//!
//!   kiesel-headless start --scale 2
//!   KIESEL_PAGE=button kiesel-headless run cargo run --example gallery

use crate::App;
use kiesel_ui::{Align, ArcRwSignal, Button, Flex, Justify, Set, Update, space};

pub fn page(_app: &App) -> Flex {
    let saving = ArcRwSignal::new(false);
    let buttons = Flex::cluster()
        .gap(space(2) + 4.0)
        .justify(Justify::Center)
        .child(Button::new("Reset").on_click({
            let saving = saving.clone();
            move || saving.set(false)
        }))
        .child(
            Button::new("Save changes")
                .loading(saving.clone())
                .on_click(move || saving.update(|s| *s = !*s)),
        );
    Flex::stack()
        .justify(Justify::Center)
        .align(Align::Center)
        .padding(space(3))
        .child(buttons)
}

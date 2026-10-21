//! Two Kiesel buttons, centred in the window, sharing one `loading` signal:
//! "Save changes" toggles it, "Reset" clears it. Tab / Shift+Tab move focus,
//! Space and Enter press; assistive technology sees both buttons.
//!
//!   kiesel-headless start --scale 2
//!   kiesel-headless run cargo run --example button

use kiesel_ui::kurbo::Size;
use kiesel_ui::{Align, ArcRwSignal, Button, Flex, Justify, Options, Set, Update, space};

fn main() -> Result<(), Box<dyn std::error::Error>> {
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
    let root = Flex::stack()
        .justify(Justify::Center)
        .align(Align::Center)
        .padding(space(3))
        .child(buttons);
    kiesel_ui::run(
        Options {
            title: "Kiesel – Button".into(),
            size: Size::new(480.0, 300.0),
            appearance: Default::default(),
        },
        root,
    )
}

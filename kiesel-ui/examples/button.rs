//! Two Kiesel buttons, centred in the window, sharing one `loading` signal:
//! "Save changes" toggles it, "Reset" clears it. Tab / Shift+Tab move focus,
//! Space and Enter press; assistive technology sees both buttons.
//!
//!   kiesel-headless start --scale 2
//!   kiesel-headless run cargo run --example button

use kiesel_ui::kurbo::Size;
use kiesel_ui::{ArcRwSignal, Button, Options, Row, Set, Update};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let saving = ArcRwSignal::new(false);
    let row = Row::new(12.0)
        .child(Button::new("Reset").on_click({
            let saving = saving.clone();
            move || saving.set(false)
        }))
        .child(
            Button::new("Save changes")
                .loading(saving.clone())
                .on_click({
                    let saving = saving.clone();
                    move || saving.update(|s| *s = !*s)
                }),
        );
    kiesel_ui::run(
        Options {
            title: "Kiesel – Button".into(),
            size: Size::new(480.0, 300.0),
            appearance: Default::default(),
        },
        row,
    )
}

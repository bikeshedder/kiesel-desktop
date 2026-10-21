//! A card as a confirmation dialog: title, a paragraph that wraps with the
//! window, and its buttons at the end of a row that wraps too.
//!
//! "Save changes" saves on a background thread: the button shows `loading`
//! until the thread is done (2 s), then the caption says so – both through
//! signals, set from that thread.
//!
//!   kiesel-headless start --scale 2
//!   kiesel-headless run cargo run --example dialog

use std::time::Duration;

use kiesel_ui::kurbo::Size;
use kiesel_ui::{
    Align, ArcRwSignal, Button, Card, Flex, GetUntracked, Justify, Label, Options, Set, space,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let saving = ArcRwSignal::new(false);
    let status = ArcRwSignal::new(String::from("Last saved 14 minutes ago"));

    let save = {
        let (saving, status) = (saving.clone(), status.clone());
        move || {
            if saving.get_untracked() {
                return;
            }
            saving.set(true);
            let (saving, status) = (saving.clone(), status.clone());
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(2));
                status.set("Saved just now".into());
                saving.set(false);
            });
        }
    };
    let reset = {
        let status = status.clone();
        move || status.set("Changes reset".into())
    };

    let card = Card::new()
        .max_width(440.0)
        .child(
            Flex::stack()
                .gap(space(2))
                .child(Label::title("Unsaved changes"))
                .child(Label::new(
                    "You changed the invoice period and two time entries. Save them now, or \
                     reset the form to what was stored – your edits are gone then.",
                )),
        )
        .child(Label::bound(status).role(kiesel_ui::TextRole::Caption))
        .child(
            Flex::cluster()
                .justify(Justify::End)
                .child(Button::new("Reset").on_click(reset))
                .child(Button::new("Save changes").loading(saving).on_click(save)),
        );
    let root = Flex::stack()
        .justify(Justify::Center)
        .align(Align::Center)
        .padding(space(5))
        .child(card);
    kiesel_ui::run(
        Options {
            title: "Kiesel – Dialog".into(),
            size: Size::new(560.0, 400.0),
            appearance: Default::default(),
        },
        root,
    )
}

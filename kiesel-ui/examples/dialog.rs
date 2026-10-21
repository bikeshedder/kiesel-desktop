//! A card as a confirmation dialog: title, a paragraph that wraps with the
//! window, and its buttons at the end of a row that wraps too.
//!
//!   kiesel-headless start --scale 2
//!   kiesel-headless run cargo run --example dialog

use kiesel_ui::kurbo::Size;
use kiesel_ui::{
    Align, ArcRwSignal, Button, Card, Flex, Justify, Label, Options, Set, Update, space,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let saving = ArcRwSignal::new(false);
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
        .child(Label::caption("Last saved 14 minutes ago"))
        .child(
            Flex::cluster()
                .justify(Justify::End)
                .child(Button::new("Reset").on_click({
                    let saving = saving.clone();
                    move || saving.set(false)
                }))
                .child(
                    Button::new("Save changes")
                        .loading(saving.clone())
                        .on_click(move || saving.update(|s| *s = !*s)),
                ),
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

//! Numbers and passwords. A number field steps with −/+ (held, they repeat)
//! and Up/Down (Shift: ten), clamps to its limits when left and writes numbers
//! the language's way. A password hides behind bullets until the eye shows it;
//! hidden, it can't be copied, and Caps Lock gets a note.
//!
//!   kiesel-headless start --scale 2
//!   KIESEL_PAGE=inputs kiesel-headless run cargo run --example gallery

use crate::App;
use kiesel_ui::{
    Align, ArcRwSignal, ArcSignal, Card, Field, Flex, Get, Justify, Label, NumberField,
    PasswordField, TextRole, space,
};

pub fn page(_app: &App) -> Flex {
    let quantity = ArcRwSignal::new(Some(2.0));
    let width = ArcRwSignal::new(Some(12.5));
    let password = ArcRwSignal::new(String::new());
    let again = ArcRwSignal::new(String::new());

    let summary = ArcSignal::derive({
        let (quantity, width) = (quantity.clone(), width.clone());
        move || {
            let show = |v: Option<f64>| v.map_or("–".to_string(), kiesel_ui::number_field::format);
            format!("{} × {} px", show(quantity.get()), show(width.get()))
        }
    });
    let mismatch = ArcSignal::derive({
        let (password, again) = (password.clone(), again.clone());
        move || {
            if !again.get().is_empty() && again.get() != password.get() {
                "The passwords differ".to_string()
            } else {
                String::new()
            }
        }
    });

    let card = Card::new()
        .max_width(440.0)
        .child(Label::title("Numbers and passwords"))
        .child(
            Flex::cluster()
                .gap(space(4))
                .align(Align::Start)
                .child(
                    Field::new("Quantity", NumberField::new(quantity).min(1.0).max(10.0))
                        .hint("1 to 10"),
                )
                .child(Field::new(
                    "Width",
                    NumberField::new(width).min(0.0).step(0.5).unit("px"),
                )),
        )
        .child(Label::new(summary).role(TextRole::Caption))
        .child(
            Field::new("Password", PasswordField::new().value(password).grow())
                .hint("At least 8 characters"),
        )
        .child(
            Field::new("Repeat it", PasswordField::confirm().value(again).grow()).error(mismatch),
        )
        .child(Flex::cluster().justify(Justify::End));
    Flex::stack()
        .justify(Justify::Center)
        .align(Align::Center)
        .padding(space(5))
        .child(card)
}

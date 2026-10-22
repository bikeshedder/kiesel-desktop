//! Fields: a label above the control, the error or else the hint below it.
//! The label names the control for screen readers and focuses it when clicked;
//! an error turns label and edge red and marks the control invalid.
//!
//! "Name" is required: its error shows once "Create account" was pressed.
//! "Email" checks as you type.
//!
//!   kiesel-headless start --scale 2
//!   KIESEL_PAGE=field kiesel-headless run cargo run --example gallery

use crate::App;
use kiesel_ui::{
    Align, ArcRwSignal, ArcSignal, Button, Card, Field, Flex, Get, Justify, Label, Set, TextField,
    TextRole, space,
};

pub fn page(_app: &App) -> Flex {
    let (name, email) = (
        ArcRwSignal::new(String::new()),
        ArcRwSignal::new(String::new()),
    );
    let submitted = ArcRwSignal::new(false);
    let created = ArcRwSignal::new(String::new());

    let name_error = ArcSignal::derive({
        let (name, submitted) = (name.clone(), submitted.clone());
        move || {
            if submitted.get() && name.get().trim().is_empty() {
                "Please enter your name".into()
            } else {
                String::new()
            }
        }
    });
    let email_error = ArcSignal::derive({
        let email = email.clone();
        move || {
            let e = email.get();
            if !e.is_empty() && !e.contains('@') {
                "That isn't an email address".into()
            } else {
                String::new()
            }
        }
    });

    let create = {
        let (name, submitted, created) = (name.clone(), submitted.clone(), created.clone());
        let (name_error, email_error) = (name_error.clone(), email_error.clone());
        move || {
            submitted.set(true);
            if name_error.get().is_empty() && email_error.get().is_empty() {
                created.set(format!("Created an account for {}", name.get().trim()));
            } else {
                created.set(String::new());
            }
        }
    };

    let card = Card::new()
        .max_width(420.0)
        .child(Label::title("Create an account"))
        .child(
            Field::new("Name", TextField::new().value(name.clone()).grow())
                .required()
                .error(name_error),
        )
        .child(
            Field::new(
                "Email",
                TextField::new()
                    .value(email.clone())
                    .placeholder("ada@example.com")
                    .grow(),
            )
            .hint("We never share it")
            .error(email_error),
        )
        .child(
            Flex::cluster()
                .justify(Justify::Between)
                .align(Align::Center)
                .child(Label::new(created).role(TextRole::Caption))
                .child(Button::new("Create account").on_click(create)),
        );
    Flex::stack()
        .justify(Justify::Center)
        .align(Align::Center)
        .padding(space(5))
        .child(card)
}

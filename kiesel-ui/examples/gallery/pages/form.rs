//! A small form: a text field bound to a signal, a caption derived from it,
//! Enter (or "Create") submits.
//!
//!   kiesel-headless start --scale 2
//!   KIESEL_PAGE=form kiesel-headless run cargo run --example gallery

use crate::App;
use kiesel_ui::{
    Align, ArcRwSignal, ArcSignal, Button, Card, Flex, Get, GetUntracked, Justify, Label, Set,
    TextField, TextRole, space,
};

pub fn page(_app: &App) -> Flex {
    let name = ArcRwSignal::new(String::new());
    let created = ArcRwSignal::new(None::<String>);

    let hint = ArcSignal::derive({
        let (name, created) = (name.clone(), created.clone());
        move || match (created.get(), name.get()) {
            (Some(c), _) => format!("Created “{c}”"),
            (None, n) if n.trim().is_empty() => "Give the project a name".to_string(),
            (None, n) => format!("Will be created as “{}”", n.trim()),
        }
    });
    let create = {
        let (name, created) = (name.clone(), created.clone());
        move || {
            let n = name.get_untracked();
            if !n.trim().is_empty() {
                created.set(Some(n.trim().to_string()));
            }
        }
    };

    let card = Card::new()
        .max_width(420.0)
        .child(Label::title("New project"))
        .child(
            Flex::stack()
                .gap(space(2))
                .child(Label::caption("Name"))
                .child(
                    TextField::new()
                        .label("Name")
                        .value(name.clone())
                        .placeholder("e.g. Website relaunch")
                        .on_submit({
                            let create = create.clone();
                            move |_| create()
                        }),
                )
                .child(Label::bound(hint).role(TextRole::Caption)),
        )
        .child(
            Flex::cluster()
                .justify(Justify::End)
                .child(Button::new("Clear").on_click({
                    let (name, created) = (name.clone(), created.clone());
                    move || {
                        name.set(String::new());
                        created.set(None);
                    }
                }))
                .child(Button::new("Create").on_click(create)),
        );
    Flex::stack()
        .justify(Justify::Center)
        .align(Align::Center)
        .padding(space(5))
        .child(card)
}

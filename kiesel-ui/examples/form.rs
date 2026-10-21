//! A small form: a text field bound to a signal, a caption derived from it,
//! Enter (or "Create") submits.
//!
//!   kiesel-headless start --scale 2
//!   kiesel-headless run cargo run --example form

use kiesel_ui::kurbo::Size;
use kiesel_ui::{
    Align, ArcRwSignal, ArcSignal, Button, Card, Flex, Get, GetUntracked, Justify, Label, Options,
    Set, TextField, TextRole, space,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
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
    let root = Flex::stack()
        .justify(Justify::Center)
        .align(Align::Center)
        .padding(space(5))
        .child(card);
    kiesel_ui::run(
        Options {
            title: "Kiesel – Form".into(),
            size: Size::new(520.0, 360.0),
            appearance: Default::default(),
        },
        root,
    )
}

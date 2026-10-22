//! A long list in a scroll view: wheel, touchpad (with momentum), PageUp/
//! PageDown, Home/End, Tab scrolls the focused row into view, the overlay
//! scrollbar can be dragged.
//!
//!   kiesel-headless start --scale 2
//!   KIESEL_PAGE=list kiesel-headless run cargo run --example gallery

use crate::App;
use kiesel_ui::{
    Align, ArcRwSignal, Button, Card, Flex, For, Justify, Label, Scroll, Set, TextRole,
    ThemeSwitch, space,
};

pub fn page(app: &App) -> Flex {
    let theming = app.theming.clone();
    let opened = ArcRwSignal::new(String::from("Nothing opened yet"));
    let items: Vec<u32> = (1..=200).collect();

    let rows = For::new(items, |n: &u32| *n, {
        let opened = opened.clone();
        move |n| {
            let (opened, n) = (opened.clone(), *n);
            Flex::cluster()
                .nowrap()
                .justify(Justify::Between)
                .child(Label::new(format!("Item {n}")))
                .child(
                    Button::new("Open")
                        .accessible_name(format!("Open item {n}"))
                        .on_click(move || opened.set(format!("Opened item {n}"))),
                )
        }
    });

    let card = Card::new()
        .max_width(440.0)
        .grow()
        .child(
            Flex::cluster()
                .nowrap()
                .justify(Justify::Between)
                .child(Label::title("Items"))
                .child(ThemeSwitch::new(&theming)),
        )
        .child(Scroll::new(Flex::stack().gap(space(2)).padding(space(1)).child(rows)).grow())
        .child(Label::bound(opened).role(TextRole::Caption));
    Flex::stack()
        .align(Align::Center)
        .padding(space(5))
        .child(card)
}

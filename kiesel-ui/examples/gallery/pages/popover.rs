//! Popovers: panels in a layer above the content, next to their trigger. A
//! click outside or Escape closes them; one opened from the keyboard takes
//! focus, and Tab goes round inside it. Near the window's bottom edge a panel
//! opens above its trigger instead.
//!
//!   kiesel-headless start --scale 2
//!   KIESEL_PAGE=popover kiesel-headless run cargo run --example gallery

use crate::App;
use kiesel_ui::{
    Align, ArcRwSignal, Button, Card, Flex, Justify, Label, Placement, Popover, Set, TextField,
    TextRole, Update, space,
};

/// A button that opens and closes `open`
fn toggle(label: &str, open: &ArcRwSignal<bool>) -> Button {
    let open = open.clone();
    Button::new(label).on_click(move || open.update(|o| *o = !*o))
}

pub fn page(_app: &App) -> Flex {
    let (account, info, more) = (
        ArcRwSignal::new(false),
        ArcRwSignal::new(false),
        ArcRwSignal::new(false),
    );
    let name = ArcRwSignal::new(String::from("Ada Lovelace"));

    let account_panel = {
        let (account, name) = (account.clone(), name.clone());
        move || {
            let close = account.clone();
            Flex::stack()
                .gap(space(2))
                .padding(space(2))
                .child(Label::title("Account"))
                .child(TextField::new().label("Display name").value(name.clone()))
                .child(
                    Flex::cluster()
                        .justify(Justify::End)
                        .child(Button::new("Done").on_click(move || close.set(false))),
                )
        }
    };
    let info_panel = || {
        Flex::stack().padding(space(2)).child(Label::new(
            "Popovers float above the window's content. They flip and slide to stay inside it.",
        ))
    };
    let more_panel = || {
        Flex::stack()
            .gap(space(1))
            .padding(space(2))
            .child(Label::caption(
                "Opened near the bottom edge – so above the button",
            ))
            .child(Button::new("Archive"))
            .child(Button::new("Delete"))
    };

    let card = Card::new()
        .max_width(440.0)
        .child(Label::title("Popovers"))
        .child(
            Flex::cluster()
                .child(
                    Popover::new(account.clone(), toggle("Account", &account), account_panel)
                        .label("Account"),
                )
                .child(
                    Popover::new(info.clone(), toggle("Info", &info), info_panel)
                        .placement(Placement::RIGHT)
                        .arrow()
                        .label("Info"),
                ),
        )
        .child(
            Label::new("Click outside a panel or press Escape to close it.").role(TextRole::Body),
        );
    Flex::stack()
        .justify(Justify::Between)
        .align(Align::Center)
        .padding(space(5))
        .child(card)
        .child(Popover::new(more.clone(), toggle("More", &more), more_panel).label("More"))
}

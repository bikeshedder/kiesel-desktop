//! Button styles: five variants (how prominent), colours (what it means),
//! three sizes, round and icon-only, a toggle (pressed), disabled, loading –
//! and groups of buttons, attached into one body or spaced.
//!
//!   kiesel-headless start --scale 2
//!   KIESEL_PAGE=variants kiesel-headless run cargo run --example gallery

use crate::App;
use crate::app_icons;
use kiesel_ui::{
    Align, ArcRwSignal, ArcSignal, Button, ButtonColor, ButtonGroup, Card, ControlSize, Flex, Get,
    Label, Set, Switch, TextRole, Update, Variant, space,
};

fn row(title: &str) -> Flex {
    Flex::cluster()
        .gap(space(3))
        .align(Align::Center)
        .child(Label::new(title.to_string()).role(TextRole::Caption))
}

pub fn page(_app: &App) -> Flex {
    let bold = ArcRwSignal::new(true);
    let off = ArcRwSignal::new(false);
    let busy = ArcRwSignal::new(false);
    let align = ArcRwSignal::new("left");

    let variants = [
        (Variant::Default, "Default"),
        (Variant::Primary, "Primary"),
        (Variant::Soft, "Soft"),
        (Variant::Outline, "Outline"),
        (Variant::Ghost, "Ghost"),
    ];
    let mut plain = row("Variants");
    for (v, name) in variants {
        plain = plain.child(Button::new(name).variant(v));
    }
    let mut danger = row("Danger");
    for (v, name) in variants {
        danger = danger.child(
            Button::new(name)
                .variant(v)
                .color(ButtonColor::Danger)
                .accessible_name(format!("{name}, danger")),
        );
    }
    let colours = row("Colours")
        .child(Button::new("Save").primary().color(ButtonColor::Success))
        .child(
            Button::new("Check")
                .variant(Variant::Soft)
                .color(ButtonColor::Warning),
        )
        .child(
            Button::new("Details")
                .variant(Variant::Outline)
                .color(ButtonColor::Info),
        )
        .child(
            Button::new("Like")
                .icon(app_icons::HEART)
                .variant(Variant::Ghost)
                .color(ButtonColor::Custom(kiesel_ui::Color::from_rgb8(
                    0xd6, 0x33, 0x6c,
                ))),
        );
    let sizes = row("Sizes")
        .child(Button::new("Small").size(ControlSize::Sm))
        .child(Button::new("Medium"))
        .child(Button::new("Large").size(ControlSize::Lg))
        .child(
            Button::new("Settings")
                .icon(app_icons::SETTINGS)
                .icon_only()
                .size(ControlSize::Sm),
        )
        .child(
            Button::new("Settings")
                .icon(app_icons::SETTINGS)
                .icon_only()
                .round(),
        )
        .child(Button::new("Round").round().primary());

    let states = row("States")
        .child(
            Button::new("Bold")
                .icon(app_icons::BOLD)
                .icon_only()
                .pressed(bold.clone())
                .on_click(move || bold.update(|b| *b = !*b)),
        )
        .child(
            Button::new("Delete")
                .icon(app_icons::TRASH_2)
                .color(ButtonColor::Danger)
                .disabled(off.clone()),
        )
        .child(Button::new("Publish").primary().disabled(off.clone()))
        .child(
            Button::new("Upload")
                .icon(app_icons::DOWNLOAD)
                .loading(busy.clone())
                .on_click({
                    let busy = busy.clone();
                    move || busy.update(|b| *b = !*b)
                }),
        )
        .child(
            Switch::new(off.clone())
                .label("Disabled")
                .on_change(move |v| off.set(v)),
        );

    let pick = |value: &'static str, icon, name: &'static str| {
        let (align, current) = (align.clone(), align.clone());
        Button::new(name)
            .icon(icon)
            .icon_only()
            .pressed(ArcSignal::derive(move || current.get() == value))
            .on_click(move || align.set(value))
    };
    let groups = row("Groups")
        .child(
            ButtonGroup::attached()
                .label("Alignment")
                .button(pick("left", app_icons::ALIGN_LEFT, "Align left"))
                .button(pick("center", app_icons::ALIGN_CENTER, "Centre"))
                .button(pick("right", app_icons::ALIGN_RIGHT, "Align right")),
        )
        .child(
            ButtonGroup::attached()
                .size(ControlSize::Sm)
                .button(Button::new("Cut"))
                .button(Button::new("Copy"))
                .button(Button::new("Paste")),
        )
        .child(
            ButtonGroup::spaced()
                .button(Button::new("Cancel"))
                .button(Button::new("Apply").primary()),
        );

    let card = Card::new()
        .max_width(760.0)
        .child(Label::title("Button styles"))
        .child(plain)
        .child(danger)
        .child(colours)
        .child(sizes)
        .child(states)
        .child(groups);
    Flex::stack()
        .align(Align::Center)
        .padding(space(5))
        .child(card)
}

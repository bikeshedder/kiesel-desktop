//! Icons: Lucide's, taken into the app by `tools/lucide-icons.py` (see
//! `app_icons.rs`), next to the toolkit's own (`kiesel_ui::lucide`). Buttons
//! show them before or after the label, or alone (named for screen readers);
//! while loading, the spinner takes the leading icon's place.
//!
//!   kiesel-headless start --scale 2
//!   KIESEL_PAGE=icons kiesel-headless run cargo run --example gallery

use crate::App;
use kiesel_ui::{
    Align, ArcRwSignal, Button, Card, Flex, Icon, Justify, Label, TextRole, Tone, Update, lucide,
    space,
};

pub fn page(_app: &App) -> Flex {
    let saving = ArcRwSignal::new(false);
    let toggle = |s: &ArcRwSignal<bool>| {
        let s = s.clone();
        move || s.update(|v| *v = !*v)
    };

    let card = Card::new()
        .max_width(460.0)
        .child(Label::title("Icons"))
        .child(
            Flex::cluster()
                .gap(space(3))
                .align(Align::Center)
                .child(Icon::new(crate::app_icons::BELL))
                .child(Icon::new(crate::app_icons::STAR).tone(Tone::Accent))
                .child(Icon::new(lucide::INFO).tone(Tone::Text(TextRole::Body)))
                .child(
                    Icon::new(lucide::CIRCLE_CHECK)
                        .size(28.0)
                        .tone(Tone::Accent)
                        .label("All saved"),
                ),
        )
        .child(
            Flex::cluster()
                .gap(space(3))
                .child(
                    Button::new("Save")
                        .icon(crate::app_icons::SAVE)
                        .loading(saving.clone())
                        .on_click(toggle(&saving)),
                )
                .child(Button::new("Download").icon_end(crate::app_icons::DOWNLOAD))
                .child(Button::new("Next").icon_end(lucide::CHEVRON_RIGHT)),
        )
        .child(
            Flex::cluster()
                .gap(space(3))
                .align(Align::Center)
                .child(
                    Button::new("Settings")
                        .icon(crate::app_icons::SETTINGS)
                        .icon_only(),
                )
                .child(
                    Button::new("Delete")
                        .icon(crate::app_icons::TRASH_2)
                        .icon_only()
                        .loading(saving.clone()),
                )
                .child(Label::new("Click Save: both show a spinner").role(TextRole::Caption)),
        );
    Flex::stack()
        .justify(Justify::Center)
        .align(Align::Center)
        .padding(space(5))
        .child(card)
}

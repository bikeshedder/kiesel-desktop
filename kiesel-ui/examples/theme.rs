//! Light, dark, or the system's preference: a `ThemeSwitch` on a `Theming`
//! that remembers the choice (in ~/.config/de.kiesel.ThemeDemo/kiesel.conf).
//!
//!   kiesel-headless start --scale 2
//!   kiesel-headless portal dark          # what the "system" prefers
//!   kiesel-headless run cargo run --example theme

use kiesel_ui::kurbo::Size;
use kiesel_ui::{
    Align, ArcSignal, Button, Card, ControlSize, Flex, Get, Justify, Label, Options, Scheme, Set,
    TextField, TextRole, ThemeMode, ThemeSwitch, Theming, space,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let theming = Theming::new("de.kiesel.ThemeDemo");

    let status = ArcSignal::derive({
        let (mode, system) = (theming.mode(), theming.system());
        move || {
            let name = |s: Scheme| if s == Scheme::Dark { "dark" } else { "light" };
            match (mode.get(), system.get()) {
                (ThemeMode::System, Some(s)) => format!("Following the system ({})", name(s)),
                (ThemeMode::System, None) => "Following the system (no preference: light)".into(),
                (ThemeMode::Light, _) => "Light, chosen here".into(),
                (ThemeMode::Dark, _) => "Dark, chosen here".into(),
            }
        }
    });

    let card =
        Card::new()
            .max_width(420.0)
            .child(Label::title("Appearance"))
            .child(
                Flex::stack()
                    .gap(space(3))
                    .align(Align::Center)
                    .child(ThemeSwitch::sized(&theming, ControlSize::Lg))
                    .child(Label::bound(status).role(TextRole::Caption)),
            )
            .child(
                TextField::new()
                    .label("Note")
                    .placeholder("Anything – watch it fade"),
            )
            .child(Flex::cluster().justify(Justify::End).child(
                Button::new("Follow system").on_click({
                    let mode = theming.mode();
                    move || mode.set(ThemeMode::System)
                }),
            ));
    let root = Flex::stack()
        .justify(Justify::Center)
        .align(Align::Center)
        .padding(space(5))
        .child(card);
    kiesel_ui::run(
        Options {
            title: "Kiesel – Theme".into(),
            size: Size::new(520.0, 400.0),
            theming,
            ..Default::default()
        },
        root,
    )
}

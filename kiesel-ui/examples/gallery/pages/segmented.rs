//! Segmented controls and toggle groups: buttons in a sunken track.
//!
//! - Theme: System · Light · Dark next to the ThemeSwitch – the knob slides,
//!   the arrow keys choose
//! - Density: optional – nothing chosen, a ghost knob shows the default that
//!   holds; a press on the chosen option clears it again
//! - Format: a toggle group – every chosen option its own knob
//! - View: the plain variant, for navigation – a flat accent tint
//!
//!   kiesel-headless start --scale 2
//!   KIESEL_PAGE=segmented kiesel-headless run cargo run --example gallery

use crate::App;
use kiesel_ui::{
    Align, ArcRwSignal, ArcSignal, Card, ControlSize, Field, Flex, Get, Justify, Label, Segment,
    SegmentedControl, Set, TextRole, ThemeMode, ThemeSwitch, ToggleGroup, space,
};

/// Sets `s` to what was chosen
fn set<T: Send + Sync + 'static>(s: &ArcRwSignal<T>) -> impl FnMut(T) + 'static {
    let s = s.clone();
    move |v| s.set(v)
}

pub fn page(app: &App) -> Flex {
    let theming = app.theming.clone();
    let density = ArcRwSignal::new(None::<&'static str>);
    let format = ArcRwSignal::new(vec!["bold"]);
    let view = ArcRwSignal::new("overview");

    let mode = theming.mode();
    let card = Card::new()
        .max_width(540.0)
        .child(Label::title("Appearance"))
        .child(
            Flex::cluster()
                .gap(space(4))
                .align(Align::End)
                .child(
                    Field::new(
                        "Theme",
                        SegmentedControl::new(mode.clone())
                            .option(Segment::new(ThemeMode::System, "System"))
                            .option(Segment::new(ThemeMode::Light, "Light"))
                            .option(Segment::new(ThemeMode::Dark, "Dark"))
                            .on_change(set(&mode)),
                    )
                    .hint("System follows the desktop"),
                )
                .child(ThemeSwitch::new(&theming)),
        )
        .child(
            Field::new(
                "Density",
                SegmentedControl::new(density.clone())
                    .option(Segment::new(Some("compact"), "Compact"))
                    .option(Segment::new(Some("comfortable"), "Comfortable"))
                    .option(Segment::new(Some("spacious"), "Spacious"))
                    .default(Some("comfortable"))
                    .on_change(set(&density))
                    .on_clear({
                        let density = density.clone();
                        move || density.set(None)
                    }),
            )
            .hint(ArcSignal::derive({
                let density = density.clone();
                move || match density.get() {
                    Some(d) => format!("Chosen: {d} – press it again for the default"),
                    None => "Nothing chosen: the default holds".into(),
                }
            })),
        )
        .child(Field::new(
            "Format",
            ToggleGroup::new(format.clone())
                .option(Segment::icon_only("bold", crate::app_icons::BOLD, "Bold"))
                .option(Segment::icon_only(
                    "italic",
                    crate::app_icons::ITALIC,
                    "Italic",
                ))
                .option(Segment::icon_only(
                    "underline",
                    crate::app_icons::UNDERLINE,
                    "Underline",
                ))
                .on_change(set(&format)),
        ))
        .child(
            Flex::cluster()
                .justify(Justify::Between)
                .align(Align::Center)
                .child(
                    SegmentedControl::new(view.clone())
                        .plain()
                        .accessible_name("View")
                        .option(Segment::new("overview", "Overview"))
                        .option(Segment::new("activity", "Activity"))
                        .option(Segment::new("settings", "Settings"))
                        .on_change(set(&view)),
                )
                .child(
                    Label::new(ArcSignal::derive({
                        let view = view.clone();
                        move || format!("Showing {}", view.get())
                    }))
                    .role(TextRole::Caption),
                ),
        )
        .child(
            Flex::cluster()
                .gap(space(4))
                .align(Align::Center)
                .child(
                    SegmentedControl::new(view.clone())
                        .size(ControlSize::Sm)
                        .accessible_name("View, small")
                        .option(Segment::icon_only(
                            "overview",
                            crate::app_icons::LAYOUT_GRID,
                            "Grid",
                        ))
                        .option(Segment::icon_only(
                            "activity",
                            crate::app_icons::LIST,
                            "List",
                        ))
                        .on_change(set(&view)),
                )
                .child(
                    SegmentedControl::new(view.clone())
                        .size(ControlSize::Lg)
                        .accessible_name("View, large")
                        .option(
                            Segment::new("overview", "Grid").icon(crate::app_icons::LAYOUT_GRID),
                        )
                        .option(Segment::new("activity", "List").icon(crate::app_icons::LIST))
                        .on_change(set(&view)),
                ),
        );
    Flex::stack()
        .justify(Justify::Center)
        .align(Align::Center)
        .padding(space(5))
        .child(card)
}

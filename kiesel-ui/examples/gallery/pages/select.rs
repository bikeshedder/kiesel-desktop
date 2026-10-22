//! Selects: a raised trigger opens the options below it. The arrows and a
//! typed letter move, Enter or a click chooses; a searchable one filters as you
//! type and marks the matches; several values keep the list open and tick.
//!
//!   kiesel-headless start --scale 2
//!   KIESEL_PAGE=select kiesel-headless run cargo run --example gallery

use crate::App;
use crate::app_icons;
use kiesel_ui::{
    Align, ArcRwSignal, ArcSignal, Card, ComboBox, ControlSize, Field, Flex, Get, Justify, Label,
    Select, SelectOption, Set, TextRole, space,
};

const COUNTRIES: &[(&str, &str, &str)] = &[
    ("at", "Austria", "Europe"),
    ("be", "Belgium", "Europe"),
    ("de", "Germany", "Europe"),
    ("dk", "Denmark", "Europe"),
    ("es", "Spain", "Europe"),
    ("fr", "France", "Europe"),
    ("it", "Italy", "Europe"),
    ("nl", "Netherlands", "Europe"),
    ("ch", "Switzerland", "Europe"),
    ("ar", "Argentina", "Americas"),
    ("br", "Brazil", "Americas"),
    ("ca", "Canada", "Americas"),
    ("mx", "Mexico", "Americas"),
    ("us", "United States", "Americas"),
    ("cn", "China", "Asia"),
    ("in", "India", "Asia"),
    ("jp", "Japan", "Asia"),
    ("kr", "South Korea", "Asia"),
];

const CITIES: &[(&str, &str, &str)] = &[
    ("ber", "Berlin", "Germany"),
    ("ham", "Hamburg", "Germany"),
    ("muc", "Munich", "Germany"),
    ("vie", "Vienna", "Austria"),
    ("grz", "Graz", "Austria"),
    ("zrh", "Zurich", "Switzerland"),
    ("brn", "Bern", "Switzerland"),
    ("bgn", "Bergen", "Norway"),
];

pub fn page(_app: &App) -> Flex {
    let status = ArcRwSignal::new(Some("active"));
    let country = ArcRwSignal::new(None::<&'static str>);
    let channels = ArcRwSignal::new(vec!["email"]);
    let city = ArcRwSignal::new(String::new());
    let picked = ArcRwSignal::new(None::<&'static str>);

    let summary = ArcSignal::derive({
        let (status, country, channels) = (status.clone(), country.clone(), channels.clone());
        move || {
            format!(
                "{} · {} · {}",
                status.get().unwrap_or("no status"),
                country.get().unwrap_or("no country"),
                channels.get().join("+")
            )
        }
    });

    let card = Card::new()
        .max_width(460.0)
        .child(Label::title("Selects"))
        .child(Field::new(
            "Status",
            Select::new(status.clone())
                .option(
                    SelectOption::new("active", "Active")
                        .icon(app_icons::CIRCLE_DOT)
                        .description("Visible to everyone"),
                )
                .option(
                    SelectOption::new("paused", "Paused")
                        .icon(app_icons::CIRCLE_PAUSE)
                        .description("Hidden until resumed"),
                )
                .option(SelectOption::new("done", "Done").icon(app_icons::CIRCLE_CHECK_BIG))
                .option(
                    SelectOption::new("archived", "Archived")
                        .icon(app_icons::ARCHIVE)
                        .disabled(),
                )
                .on_change({
                    let status = status.clone();
                    move |v| status.set(Some(v))
                })
                .on_clear({
                    let status = status.clone();
                    move || status.set(None)
                }),
        ))
        .child(
            Field::new(
                "Country",
                Select::new(country.clone())
                    .placeholder("Choose a country…")
                    .searchable()
                    .options(COUNTRIES.iter().map(|&(code, name, region)| {
                        SelectOption::new(code, name).group(region).keywords(code)
                    }))
                    .on_change({
                        let country = country.clone();
                        move |v| country.set(Some(v))
                    }),
            )
            .hint("Type to search – a country code works too"),
        )
        .child(
            Field::new(
                "City",
                ComboBox::new(city.clone())
                    .placeholder("Start typing…")
                    .options(
                        CITIES
                            .iter()
                            .map(|&(id, name, country)| SelectOption::new(id, name).group(country)),
                    )
                    .on_choose({
                        let picked = picked.clone();
                        move |v| picked.set(Some(v))
                    }),
            )
            .hint(ArcSignal::derive(move || match picked.get() {
                Some(id) => format!("Suggestion taken: {id}"),
                None => "Free text, or take a suggestion".into(),
            })),
        )
        .child(Field::new(
            "Notify via",
            Select::multiple(channels.clone())
                .options([
                    SelectOption::new("email", "Email"),
                    SelectOption::new("push", "Push"),
                    SelectOption::new("sms", "Text message"),
                    SelectOption::new("chat", "Chat"),
                ])
                .on_change_many({
                    let channels = channels.clone();
                    move |v| channels.set(v)
                }),
        ))
        .child(
            Flex::cluster()
                .justify(Justify::Between)
                .align(Align::Center)
                .child(Label::new(summary).role(TextRole::Caption))
                .child(
                    Select::new(status.clone())
                        .size(ControlSize::Sm)
                        .accessible_name("Status, small")
                        .options([
                            SelectOption::new("active", "Active"),
                            SelectOption::new("paused", "Paused"),
                            SelectOption::new("done", "Done"),
                        ])
                        .on_change(move |v| status.set(Some(v))),
                ),
        );
    Flex::stack()
        .justify(Justify::Center)
        .align(Align::Center)
        .padding(space(5))
        .child(card)
}

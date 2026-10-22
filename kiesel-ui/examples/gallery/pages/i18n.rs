//! Messages in English and German: Kiesel's catalogs, the app's own, plurals,
//! lists and numbers – switching the language updates every text (and what
//! screen readers hear) without restarting.
//!
//!   kiesel-headless start --scale 2
//!   KIESEL_LANG=de kiesel-headless run cargo run --example i18n

use crate::App;
use kiesel_ui::i18n::{self, add_catalog};
use kiesel_ui::{
    Align, ArcRwSignal, Button, Card, Flex, Get, Justify, Label, Set, TextRole, ThemeSwitch,
    Update, space, t, t_with,
};

const NAMES: [&str; 4] = ["Ann", "Bo", "Cy", "Dee"];

/// This page's messages, in English and German
pub fn catalogs() {
    add_catalog(
        "en",
        "i18n",
        r#"{ "title": "Languages", "more": "One more", "fewer": "One fewer" }"#,
    );
    add_catalog(
        "de",
        "i18n",
        r#"{ "title": "Sprachen", "more": "Einer mehr", "fewer": "Einer weniger" }"#,
    );
}

pub fn page(app: &App) -> Flex {
    // the app's own messages, next to Kiesel's

    let theming = app.theming.clone();
    let n = ArcRwSignal::new(1_usize);
    let language = i18n::language();

    let results = t_with("kiesel.filter.results", {
        let n = n.clone();
        move || vec![("n", n.get().into())]
    });
    let typing = t_with("kiesel.typingIndicator.typing", {
        let n = n.clone();
        move || {
            let names: Vec<String> = NAMES.iter().take(n.get()).map(|s| s.to_string()).collect();
            vec![("count", n.get().into()), ("names", names.into())]
        }
    });
    let of = t_with("kiesel.common.xOfY", {
        let n = n.clone();
        move || {
            vec![
                ("n", (n.get() * 1000).into()),
                ("total", 1_234_567_i64.into()),
            ]
        }
    });

    let pick = |lang: &'static str, label: &'static str| {
        let language = language.clone();
        Button::new(label).on_click(move || language.set(lang.to_string()))
    };
    let card = Card::new()
        .max_width(440.0)
        .child(
            Flex::cluster()
                .nowrap()
                .justify(Justify::Between)
                .child(Label::title(t("i18n.title")))
                .child(ThemeSwitch::new(&theming)),
        )
        .child(
            Flex::cluster()
                .child(pick("en", "English"))
                .child(pick("de", "Deutsch")),
        )
        .child(
            Flex::stack()
                .gap(space(1))
                .child(Label::new(results))
                .child(Label::new(typing))
                .child(Label::new(of).role(TextRole::Caption)),
        )
        .child(
            Flex::cluster()
                .justify(Justify::End)
                .child(Button::new(t("i18n.fewer")).on_click({
                    let n = n.clone();
                    move || n.update(|n| *n = n.saturating_sub(1))
                }))
                .child(Button::new(t("i18n.more")).on_click(move || n.update(|n| *n += 1)))
                .child(Button::new(t("kiesel.common.cancel"))),
        );
    Flex::stack()
        .justify(Justify::Center)
        .align(Align::Center)
        .padding(space(5))
        .child(card)
}

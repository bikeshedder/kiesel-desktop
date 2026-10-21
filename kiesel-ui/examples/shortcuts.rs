//! Shortcuts: written once with "Primary" (Ctrl, ⌘ on macOS), shown the
//! platform's way in `Kbd`, in the user's language.
//!
//! - Ctrl+S presses Save from anywhere, also while typing in the note
//!   (the field keeps the keys it uses itself: Ctrl+A selects its text)
//! - Ctrl+E exports – but while the "More" popover is open, its Archive
//!   button has Ctrl+E: the topmost popup comes first
//! - Ctrl+L, an app shortcut, switches between English and German
//!
//!   kiesel-headless start --scale 2
//!   kiesel-headless run cargo run --example shortcuts

use kiesel_ui::i18n::{self, add_catalog};
use kiesel_ui::kurbo::Size;
use kiesel_ui::{
    Align, ArcRwSignal, Button, Card, Flex, Get, Justify, Kbd, Label, Options, Popover, Set,
    Shortcut, Shortcuts, TextField, TextRole, Update, WithUntracked, space, t, t_with,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    add_catalog(
        "en",
        "demo",
        r#"{ "title": "Shortcuts", "save": "Save", "export": "Export", "more": "More",
             "archive": "Archive", "note": "Note",
             "last": ".input {$n :number} .match $n 0 {{Nothing yet}} * {{{$what} ({$n})}}",
             "saved": "Saved", "exported": "Exported", "archived": "Archived",
             "hint": "Switch the language" }"#,
    );
    add_catalog(
        "de",
        "demo",
        r#"{ "title": "Tastenkürzel", "save": "Speichern", "export": "Exportieren", "more": "Mehr",
             "archive": "Archivieren", "note": "Notiz",
             "last": ".input {$n :number} .match $n 0 {{Noch nichts}} * {{{$what} ({$n})}}",
             "saved": "Gespeichert", "exported": "Exportiert", "archived": "Archiviert",
             "hint": "Sprache wechseln" }"#,
    );

    // what happened last, and how many things happened
    let last = ArcRwSignal::new((0_usize, "demo.saved"));
    let did = |what: &'static str| {
        let last = last.clone();
        move || last.update(|(n, w)| (*n, *w) = (*n + 1, what))
    };
    let status = t_with("demo.last", {
        let (last, language) = (last.clone(), i18n::language());
        move || {
            let (n, what) = last.get();
            vec![
                ("n", n.into()),
                ("what", i18n::format(&language.get(), what, &[]).into()),
            ]
        }
    });

    let more = ArcRwSignal::new(false);
    let more_panel = {
        let archived = did("demo.archived");
        move || {
            let archived = archived.clone();
            Flex::stack().padding(space(2)).child(
                Flex::cluster()
                    .gap(space(2))
                    .align(Align::Center)
                    .child(
                        Button::new(t("demo.archive"))
                            .shortcut("Primary+E")
                            .on_click(archived),
                    )
                    .child(Kbd::new(Shortcut::primary('e').text())),
            )
        }
    };

    let row = |button: Button, shortcut: &str| {
        Flex::cluster()
            .gap(space(2))
            .align(Align::Center)
            .child(button.shortcut(shortcut))
            .child(Kbd::new(shortcut.parse::<Shortcut>().unwrap().text()))
    };

    let card = Card::new()
        .max_width(440.0)
        .child(Label::title(t("demo.title")))
        .child(Label::new(status).role(TextRole::Caption))
        .child(TextField::new().label(t("demo.note")).grow())
        .child(row(
            Button::new(t("demo.save")).on_click(did("demo.saved")),
            "Primary+S",
        ))
        .child(row(
            Button::new(t("demo.export")).on_click(did("demo.exported")),
            "Primary+E",
        ))
        .child(
            Flex::cluster()
                .justify(Justify::Between)
                .align(Align::Center)
                .child(
                    Popover::new(
                        more.clone(),
                        Button::new(t("demo.more")).on_click({
                            let more = more.clone();
                            move || more.update(|o| *o = !*o)
                        }),
                        more_panel,
                    )
                    .label(t("demo.more")),
                )
                .child(
                    Flex::cluster()
                        .gap(space(2))
                        .align(Align::Center)
                        .child(Label::new(t("demo.hint")).role(TextRole::Caption))
                        .child(Kbd::new(Shortcut::primary('l').text())),
                ),
        );

    let language = i18n::language();
    let shortcuts = Shortcuts::new().on("Primary+L", move || {
        let next = if language.with_untracked(|l| l.starts_with("de")) {
            "en"
        } else {
            "de"
        };
        language.set(next.into());
    });

    let root = Flex::stack()
        .justify(Justify::Center)
        .align(Align::Center)
        .padding(space(5))
        .child(card);
    kiesel_ui::run(
        Options {
            title: "Kiesel – Shortcuts".into(),
            size: Size::new(520.0, 460.0),
            shortcuts,
            ..Default::default()
        },
        root,
    )
}

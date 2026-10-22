//! The gallery: every Kiesel Desktop example in one application – a list of
//! pages on the left, the chosen page on the right. New examples become pages
//! here (`pages/<id>.rs`, listed in [`PAGES`]).
//!
//!   kiesel-headless start --scale 2
//!   kiesel-headless run cargo run --example gallery
//!
//! `KIESEL_PAGE=<id>` opens one page on its own, filling the window – how the
//! headless tests (`tests/<id>-demo.sh`) see it:
//!
//!   KIESEL_PAGE=choices kiesel-headless run cargo run --example gallery

mod app_icons;
mod pages;

use kiesel_ui::kurbo::Size;
use kiesel_ui::{
    Align, ArcRwSignal, ArcSignal, Flex, Get, Label, Options, Segment, SegmentedControl, Set,
    Shortcuts, Show, TextRole, Theming, With, i18n, space,
};

/// What the pages share
#[derive(Clone)]
pub struct App {
    /// Light, dark or the system's – one choice for the whole gallery
    pub theming: Theming,
}

/// A page: its id (for `KIESEL_PAGE`), its name in the list, what it shows
pub struct Page {
    pub id: &'static str,
    pub title: &'static str,
    pub make: fn(&App) -> Flex,
}

pub const PAGES: &[Page] = &[
    Page {
        id: "button",
        title: "Buttons",
        make: pages::button::page,
    },
    Page {
        id: "icons",
        title: "Icons",
        make: pages::icons::page,
    },
    Page {
        id: "form",
        title: "Text input",
        make: pages::form::page,
    },
    Page {
        id: "field",
        title: "Fields",
        make: pages::field::page,
    },
    Page {
        id: "choices",
        title: "Choices",
        make: pages::choices::page,
    },
    Page {
        id: "segmented",
        title: "Segmented",
        make: pages::segmented::page,
    },
    Page {
        id: "dialog",
        title: "Wrapping text",
        make: pages::dialog::page,
    },
    Page {
        id: "popover",
        title: "Popovers",
        make: pages::popover::page,
    },
    Page {
        id: "list",
        title: "Long list",
        make: pages::list::page,
    },
    Page {
        id: "todo",
        title: "Tasks",
        make: pages::todo::page,
    },
    Page {
        id: "theme",
        title: "Theme",
        make: pages::theme::page,
    },
    Page {
        id: "i18n",
        title: "Languages",
        make: pages::i18n::page,
    },
    Page {
        id: "shortcuts",
        title: "Shortcuts",
        make: pages::shortcuts::page,
    },
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = App {
        theming: Theming::new("de.kiesel.Gallery"),
    };
    pages::catalogs();

    // Ctrl+L switches between English and German, wherever focus is
    let language = i18n::language();
    let shortcuts = Shortcuts::new().on("Primary+L", move || {
        let next = if language.with(|l| l.starts_with("de")) {
            "en"
        } else {
            "de"
        };
        language.set(next.into());
    });

    let alone = std::env::var("KIESEL_PAGE").ok();
    let root = match alone.as_deref() {
        Some(id) => {
            let page = PAGES
                .iter()
                .find(|p| p.id == id)
                .ok_or_else(|| format!("no page '{id}' (pages: {})", ids()))?;
            (page.make)(&app)
        }
        None => gallery(&app),
    };
    kiesel_ui::run(
        Options {
            title: "Kiesel Gallery".into(),
            size: Size::new(960.0, 720.0),
            theming: app.theming.clone(),
            shortcuts,
            ..Default::default()
        },
        root,
    )
}

fn ids() -> String {
    PAGES.iter().map(|p| p.id).collect::<Vec<_>>().join(", ")
}

/// The list of pages, and the chosen page next to it
fn gallery(app: &App) -> Flex {
    let current = ArcRwSignal::new(PAGES[0].id);
    let mut nav = SegmentedControl::new(current.clone())
        .vertical()
        .accessible_name("Pages")
        .on_change({
            let current = current.clone();
            move |id| current.set(id)
        });
    for p in PAGES {
        nav = nav.option(Segment::new(p.id, p.title));
    }
    let side = Flex::stack()
        .gap(space(3))
        .padding(space(4))
        .child(Label::new("Kiesel Gallery").role(TextRole::Title))
        .child(nav)
        .child(
            Label::new(ArcSignal::derive({
                let current = current.clone();
                move || format!("{} of {} pages", position(current.get()), PAGES.len())
            }))
            .role(TextRole::Caption),
        );

    // one page at a time: made when chosen, dropped when another is
    let mut area = Flex::stack().grow();
    for p in PAGES {
        let (app, make) = (app.clone(), p.make);
        let shown = ArcSignal::derive({
            let (current, id) = (current.clone(), p.id);
            move || current.get() == id
        });
        area = area.child(Show::new(shown, move || make(&app).grow()));
    }
    Flex::cluster()
        .nowrap()
        .gap(0.0)
        .align(Align::Stretch)
        .child(side)
        .child(area)
}

fn position(id: &str) -> usize {
    PAGES.iter().position(|p| p.id == id).map_or(0, |i| i + 1)
}

//! Textareas: several lines, wrapped, scrolling to keep the caret in view.
//! Up and Down move by lines, PageUp/PageDown by what shows, Enter starts a
//! line, Ctrl+Enter sends. The second one holds code: monospace, unwrapped.
//!
//!   kiesel-headless start --scale 2
//!   KIESEL_PAGE=textarea kiesel-headless run cargo run --example gallery

use crate::App;
use kiesel_ui::{
    Align, ArcRwSignal, ArcSignal, Button, Card, Field, Flex, Get, Justify, Label, Set, TextRole,
    Textarea, space,
};

const LIMIT: usize = 280;

pub fn page(_app: &App) -> Flex {
    let message = ArcRwSignal::new(String::new());
    let config = ArcRwSignal::new(String::from(
        "{\n  \"name\": \"Example\",\n  \"theme\": \"system\",\n  \"tags\": [\"one\", \"two\"]\n}",
    ));
    let sent = ArcRwSignal::new(String::new());

    let count = ArcSignal::derive({
        let message = message.clone();
        move || message.get().chars().count()
    });
    let error = ArcSignal::derive({
        let count = count.clone();
        move || {
            let n = count.get();
            if n > LIMIT {
                format!("{} characters too many", n - LIMIT)
            } else {
                String::new()
            }
        }
    });
    let send = {
        let (message, sent, count) = (message.clone(), sent.clone(), count.clone());
        move || {
            let lines = message.get().lines().count();
            if count.get() <= LIMIT {
                sent.set(format!("Sent {lines} line(s)"));
            }
        }
    };

    let card = Card::new()
        .max_width(480.0)
        .child(Label::title("Write a message"))
        .child(
            Field::new(
                "Message",
                Textarea::new()
                    .value(message.clone())
                    .placeholder("A few lines – Ctrl+Enter sends")
                    .on_submit({
                        let send = send.clone();
                        move |_| send()
                    }),
            )
            .hint(ArcSignal::derive(move || {
                format!("{} of {LIMIT}", count.get())
            }))
            .error(error),
        )
        .child(Field::new(
            "Settings (JSON)",
            Textarea::new().rows(4).mono().value(config),
        ))
        .child(
            Flex::cluster()
                .justify(Justify::Between)
                .align(Align::Center)
                .child(Label::new(sent).role(TextRole::Caption))
                .child(Button::new("Send").on_click(send)),
        );
    Flex::stack()
        .justify(Justify::Center)
        .align(Align::Center)
        .padding(space(5))
        .child(card)
}

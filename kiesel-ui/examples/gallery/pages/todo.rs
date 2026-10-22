//! A task list: widgets come and go at runtime. `For` keeps one row per task
//! (keyed by id), `Show` the empty state; removing the focused "Remove" moves
//! focus to the next one.
//!
//!   kiesel-headless start --scale 2
//!   KIESEL_PAGE=todo kiesel-headless run cargo run --example gallery

use std::sync::atomic::{AtomicU64, Ordering};

use crate::App;
use kiesel_ui::{
    Align, ArcRwSignal, ArcSignal, Button, Card, Flex, For, GetUntracked, Justify, Label, Set,
    Show, TextField, TextRole, Update, With, space,
};

#[derive(Clone)]
struct Task {
    id: u64,
    title: String,
}

pub fn page(_app: &App) -> Flex {
    let tasks = ArcRwSignal::new(Vec::<Task>::new());
    let draft = ArcRwSignal::new(String::new());
    let next_id = std::sync::Arc::new(AtomicU64::new(1));

    let add = {
        let (tasks, draft) = (tasks.clone(), draft.clone());
        move || {
            let title = draft.get_untracked().trim().to_string();
            if title.is_empty() {
                return;
            }
            let id = next_id.fetch_add(1, Ordering::Relaxed);
            tasks.update(|t| t.push(Task { id, title }));
            draft.set(String::new());
        }
    };
    let empty = ArcSignal::derive({
        let tasks = tasks.clone();
        move || tasks.with(Vec::is_empty)
    });
    let count = ArcSignal::derive({
        let tasks = tasks.clone();
        move || match tasks.with(Vec::len) {
            0 => String::new(),
            1 => "1 task".to_string(),
            n => format!("{n} tasks"),
        }
    });

    let rows = For::new(
        tasks.clone(),
        |t: &Task| t.id,
        move |t| {
            let (tasks, id) = (tasks.clone(), t.id);
            Flex::cluster()
                .nowrap()
                .justify(Justify::Between)
                .child(Label::new(t.title.clone()))
                .child(
                    Button::new("Remove")
                        .accessible_name(format!("Remove “{}”", t.title))
                        .on_click(move || tasks.update(|all| all.retain(|t| t.id != id))),
                )
        },
    );

    let card = Card::new()
        .max_width(440.0)
        .child(Label::title("Tasks"))
        .child(
            Flex::cluster()
                .nowrap()
                .child(
                    TextField::new()
                        .label("New task")
                        .grow()
                        .placeholder("What needs doing?")
                        .value(draft)
                        .on_submit({
                            let add = add.clone();
                            move |_| add()
                        }),
                )
                .child(Button::new("Add").on_click(add)),
        )
        .child(Show::new(empty, || {
            Label::caption("Nothing to do – add a task above")
        }))
        .child(Flex::stack().gap(space(2)).child(rows))
        .child(Label::bound(count).role(TextRole::Caption));
    Flex::stack()
        .justify(Justify::Center)
        .align(Align::Center)
        .padding(space(5))
        .child(card)
}

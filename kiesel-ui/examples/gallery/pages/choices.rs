//! Checkboxes, radios and switches: a plan to choose (radios – the arrow keys
//! move the choice), notifications (a checkbox group with "Select all"), a
//! settings row (a switch at the end) and a consent that "Continue" insists on.
//!
//!   kiesel-headless start --scale 2
//!   KIESEL_PAGE=choices kiesel-headless run cargo run --example gallery

use crate::App;
use kiesel_ui::{
    Align, ArcRwSignal, ArcSignal, Button, Card, Checkbox, CheckboxGroup, Choice, ControlSize,
    Field, Flex, Get, Justify, Label, RadioGroup, Scroll, Set, Switch, TextRole, space,
};

pub fn page(_app: &App) -> Flex {
    let plan = ArcRwSignal::new(String::new());
    let channels = ArcRwSignal::new(vec!["email".to_string()]);
    let updates = ArcRwSignal::new(true);
    let terms = ArcRwSignal::new(false);
    let submitted = ArcRwSignal::new(false);
    let status = ArcRwSignal::new(String::new());

    let plan_error = ArcSignal::derive({
        let (plan, submitted) = (plan.clone(), submitted.clone());
        move || {
            if submitted.get() && plan.get().is_empty() {
                "Choose a plan".into()
            } else {
                String::new()
            }
        }
    });
    let terms_error = ArcSignal::derive({
        let (terms, submitted) = (terms.clone(), submitted.clone());
        move || {
            if submitted.get() && !terms.get() {
                "Please accept the terms to continue".into()
            } else {
                String::new()
            }
        }
    });

    let set = |s: &ArcRwSignal<String>| {
        let s = s.clone();
        move |v: String| s.set(v)
    };
    let card = Card::new()
        .max_width(460.0)
        .child(Label::title("Subscription"))
        .child(
            Field::new(
                "Plan",
                RadioGroup::new(plan.clone())
                    .option(Choice::new("monthly".to_string(), "Monthly"))
                    .option(
                        Choice::new("yearly".to_string(), "Yearly").description("Two months free"),
                    )
                    .option(Choice::new("team".to_string(), "Team"))
                    .on_change(set(&plan)),
            )
            .required()
            .error(plan_error),
        )
        .child(
            Field::new(
                "Notifications",
                CheckboxGroup::new(channels.clone())
                    .select_all()
                    .option(Choice::new("email".to_string(), "Email"))
                    .option(Choice::new("push".to_string(), "Push"))
                    .option(
                        Choice::new("sms".to_string(), "Text message")
                            .description("Only for security alerts"),
                    )
                    .on_change({
                        let channels = channels.clone();
                        move |v| channels.set(v)
                    }),
            )
            .hint("Change them any time"),
        )
        .child(
            Switch::new(updates.clone())
                .label("Automatic updates")
                .description("Installs updates in the background")
                .label_first()
                .on_change({
                    let updates = updates.clone();
                    move |on| updates.set(on)
                }),
        )
        .child(
            Checkbox::new(terms.clone())
                .label("I accept the terms")
                .error(terms_error)
                .on_change({
                    let terms = terms.clone();
                    move |on| terms.set(on)
                }),
        )
        .child(
            Flex::cluster()
                .gap(space(3))
                .align(Align::Center)
                .child(
                    Checkbox::new(true)
                        .size(ControlSize::Sm)
                        .accessible_name("Small"),
                )
                .child(Checkbox::new(true).accessible_name("Medium"))
                .child(
                    Checkbox::new(true)
                        .size(ControlSize::Lg)
                        .accessible_name("Large"),
                )
                .child(
                    Switch::new(true)
                        .size(ControlSize::Sm)
                        .accessible_name("Small switch"),
                )
                .child(
                    Switch::new(false)
                        .size(ControlSize::Lg)
                        .accessible_name("Large switch"),
                ),
        )
        .child(
            Flex::cluster()
                .justify(Justify::Between)
                .align(Align::Center)
                .child(Label::new(status.clone()).role(TextRole::Caption))
                .child(Button::new("Continue").on_click({
                    let (plan, terms, submitted, status) = (
                        plan.clone(),
                        terms.clone(),
                        submitted.clone(),
                        status.clone(),
                    );
                    move || {
                        submitted.set(true);
                        let ok = !plan.get().is_empty() && terms.get();
                        status.set(if ok {
                            format!("Subscribed: {}", plan.get())
                        } else {
                            String::new()
                        });
                    }
                })),
        );
    Flex::stack().child(
        Scroll::new(
            Flex::stack()
                .justify(Justify::Center)
                .align(Align::Center)
                .padding(space(5))
                .child(card),
        )
        .grow(),
    )
}

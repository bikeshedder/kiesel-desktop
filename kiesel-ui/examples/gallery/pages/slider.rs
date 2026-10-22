//! Sliders: a press on the track jumps there, dragging follows; arrows step,
//! PageUp/PageDown move a tenth, Home/End go to the ends. The value can be
//! written at the end; a field gives the label, hint and error.
//!
//!   kiesel-headless start --scale 2
//!   KIESEL_PAGE=slider kiesel-headless run cargo run --example gallery

use crate::App;
use kiesel_ui::{
    Align, ArcRwSignal, ArcSignal, Card, ControlSize, Field, Flex, Get, Justify, Label, Set,
    Slider, TextRole, space,
};

fn set(s: &ArcRwSignal<f64>) -> impl FnMut(f64) + 'static {
    let s = s.clone();
    move |v| s.set(v)
}

pub fn page(_app: &App) -> Flex {
    let volume = ArcRwSignal::new(40.0);
    let brightness = ArcRwSignal::new(70.0);
    let temperature = ArcRwSignal::new(21.5);

    let too_bright = ArcSignal::derive({
        let brightness = brightness.clone();
        move || {
            if brightness.get() > 90.0 {
                "Above 90 % wears the screen".to_string()
            } else {
                String::new()
            }
        }
    });

    let card = Card::new()
        .max_width(440.0)
        .child(Label::title("Sliders"))
        .child(Field::new(
            "Volume",
            Slider::new(volume.clone())
                .show_value(|v| format!("{v} %"))
                .on_change(set(&volume)),
        ))
        .child(
            Field::new(
                "Brightness",
                Slider::new(brightness.clone())
                    .step(5.0)
                    .show_value(|v| format!("{v} %"))
                    .on_change(set(&brightness)),
            )
            .hint("In steps of 5 %")
            .error(too_bright),
        )
        .child(Field::new(
            "Temperature",
            Slider::new(temperature.clone())
                .range(16.0, 28.0)
                .step(0.5)
                .show_value(|v| format!("{} °C", kiesel_ui::number_field::format(v)))
                .on_change(set(&temperature)),
        ))
        .child(
            Flex::cluster()
                .gap(space(4))
                .align(Align::Center)
                .child(Label::new("Sizes").role(TextRole::Caption))
                .child(
                    Slider::new(volume.clone())
                        .size(ControlSize::Sm)
                        .label("Volume, small")
                        .on_change(set(&volume)),
                )
                .child(
                    Slider::new(volume.clone())
                        .size(ControlSize::Lg)
                        .label("Volume, large")
                        .on_change(set(&volume)),
                ),
        );
    Flex::stack()
        .justify(Justify::Center)
        .align(Align::Center)
        .padding(space(5))
        .child(card)
}

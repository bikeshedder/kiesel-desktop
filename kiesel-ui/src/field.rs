//! `Field` – a label, a control, and under it the error or else the hint (see
//! `kiesel/src/components/TextField`, `Field` and `FieldMessage`).
//!
//! The field names its control for assistive technology: the label labels it,
//! the message describes it, an error marks it invalid ([`FieldLink`]). A
//! press on the label focuses the control. Invalid, the label turns red, the
//! control gets a red edge and the message a warning icon.

use reactive_graph::traits::{Get, With};
use reactive_graph::wrappers::read::ArcSignal;
use vello_cpu::kurbo::{Point, Rect};

use crate::control::Show;
use crate::flex::{Align, Flex};
use crate::i18n::{IntoText, TextProp};
use crate::icon::{Icon, Tone};
use crate::label::Label;
use crate::lucide;
use crate::theme::{TextRole, Theme, space};
use crate::widget::{Widget, WidgetId};

/// What a control inside a [`Field`] learns about it (see [`Widget::link_field`])
#[derive(Clone)]
pub struct FieldLink {
    /// The label's id: the control is labelled by it
    pub label: WidgetId,
    /// The error, or else the hint: describes the control ("" for none)
    pub description: ArcSignal<String>,
    /// There is an error
    pub invalid: ArcSignal<bool>,
    /// Needs a value (an asterisk after the label)
    pub required: bool,
}

pub struct Field {
    id: WidgetId,
    label: TextProp,
    hint: Option<TextProp>,
    error: Option<ArcSignal<String>>,
    required: bool,
    /// Until mounted: the control, waiting for the hint, error and required
    control: Option<Box<dyn Widget>>,
    control_id: WidgetId,
    /// Label row, control, message
    children: Vec<Box<dyn Widget>>,
    bounds: Rect,
}

impl Field {
    /// `label` names `control` – a `TextField`, later any form control
    pub fn new(label: impl IntoText, control: impl Widget + 'static) -> Self {
        Self {
            id: WidgetId::next(),
            label: TextProp::new(label),
            hint: None,
            error: None,
            required: false,
            control_id: control.id(),
            control: Some(Box::new(control)),
            children: Vec::new(),
            bounds: Rect::ZERO,
        }
    }

    /// A line under the control that helps filling it in (hidden by an error)
    pub fn hint(mut self, hint: impl IntoText) -> Self {
        self.hint = Some(TextProp::new(hint));
        self
    }

    /// The error: a signal, empty while the value is fine – e.g.
    /// `ArcSignal::derive(move || if name.get().is_empty() { "Required".into() } else { String::new() })`
    pub fn error(mut self, error: impl IntoText) -> Self {
        self.error = Some(error.into_text());
        self
    }

    /// A value is needed: an asterisk after the label, "required" for assistive technology
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// The label row, the control (told about the field) and the message
    fn build(&mut self, mut control: Box<dyn Widget>) {
        let error = self
            .error
            .clone()
            .unwrap_or_else(|| ArcSignal::stored(String::new()));
        let invalid = ArcSignal::derive({
            let error = error.clone();
            move || error.with(|e| !e.is_empty())
        });
        let hint = self.hint.as_ref().map(TextProp::signal);
        let description = ArcSignal::derive({
            let (error, invalid, hint) = (error.clone(), invalid.clone(), hint.clone());
            move || match (&hint, invalid.get()) {
                (_, true) => error.get(),
                (Some(h), false) => h.get(),
                (None, false) => String::new(),
            }
        });

        let label = Label::new(self.label.signal())
            .role(TextRole::FieldLabel)
            .tone(ArcSignal::derive({
                let invalid = invalid.clone();
                move || {
                    if invalid.get() {
                        Tone::Danger
                    } else {
                        Tone::Text(TextRole::FieldLabel)
                    }
                }
            }));
        let label_id = label.id();
        let mut row = Flex::cluster()
            .gap(2.0)
            .align(Align::Start)
            .padding_start(space(1))
            .child(label);
        if self.required {
            row = row.child(
                Label::new("*")
                    .role(TextRole::FieldLabel)
                    .tone(Tone::Danger)
                    .hidden(),
            );
        }

        control.link_field(FieldLink {
            label: label_id,
            description,
            invalid: invalid.clone(),
            required: self.required,
        });

        let message = Show::new(invalid, move || {
            Flex::cluster()
                .gap(space(1))
                .align(Align::Start)
                .padding_start(space(1))
                .child(
                    Icon::new(lucide::TRIANGLE_ALERT)
                        .size(14.0)
                        .tone(Tone::Danger),
                )
                .child(Label::new(error.clone()).role(TextRole::Error))
        })
        .fallback(move || {
            Flex::cluster().padding_start(space(1)).child(
                Label::new(
                    hint.clone()
                        .unwrap_or_else(|| ArcSignal::stored(String::new())),
                )
                .role(TextRole::Caption),
            )
        });

        self.children = vec![Box::new(row), control, Box::new(message)];
    }
}

impl Widget for Field {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut [Box<dyn Widget>] {
        &mut self.children
    }

    /// Before its children: they are made here, then mounted
    fn mount(&mut self, _cx: &mut crate::reactive::MountCx<'_>) {
        if let Some(control) = self.control.take() {
            self.build(control);
        }
    }

    /// Label, control and message below each other, `--size-2` apart, stretched
    fn style(&self, _theme: &Theme) -> taffy::Style {
        let gap = taffy::LengthPercentage::length(space(2) as f32);
        taffy::Style {
            display: taffy::Display::Flex,
            flex_direction: taffy::FlexDirection::Column,
            gap: taffy::Size {
                width: gap,
                height: gap,
            },
            align_items: taffy::AlignItems::STRETCH,
            min_size: taffy::Size {
                width: taffy::LengthPercentageAuto::length(0.0),
                height: taffy::LengthPercentageAuto::auto(),
            },
            ..Default::default()
        }
    }

    fn set_bounds(&mut self, _cx: &mut crate::widget::LayoutCx<'_>, bounds: Rect) {
        self.bounds = bounds;
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    /// A press on the label focuses the control (`<label for>`)
    fn press_focuses(&self, p: Point) -> Option<WidgetId> {
        let row = self.children.first()?.bounds();
        row.contains(p).then_some(self.control_id)
    }
}

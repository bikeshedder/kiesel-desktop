//! `PasswordField` – a text field whose text is hidden behind bullets, with a
//! button that shows it (see `kiesel/src/components/PasswordField`).
//!
//! Hidden, the text can't be copied or cut, takes no input method and doesn't
//! tell its words apart (Ctrl+arrows go to its ends); assistive technology
//! reads bullets. The eye button shows it in plain text (crossed out while
//! visible); the caret and selection stay. While hidden, a note says when
//! Caps Lock seems to be on.

use reactive_graph::signal::ArcRwSignal;
use reactive_graph::traits::{Get, Set, Update};
use reactive_graph::wrappers::read::ArcSignal;

use crate::button::{Button, Variant};
use crate::control::Show;
use crate::i18n::t;
use crate::icon::Tone;
use crate::label::Label;
use crate::lucide;
use crate::text_field::TextField;
use crate::theme::{ControlSize, TextRole};

pub struct PasswordField;

impl PasswordField {
    /// A password being entered, with the eye button to show it
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> TextField {
        Self::make(true)
    }

    /// A field that follows another one – the confirmation under a new
    /// password: hidden, without an eye of its own
    pub fn confirm() -> TextField {
        Self::make(false)
    }

    fn make(eye: bool) -> TextField {
        let visible = ArcRwSignal::new(false);
        let caps = ArcRwSignal::new(false);
        let mut field = TextField::new();
        field.mask = Some(ArcSignal::derive({
            let visible = visible.clone();
            move || !visible.get()
        }));
        field.on_caps = Some(Box::new({
            let caps = caps.clone();
            move |on| caps.set(on)
        }));
        // Caps Lock, while hidden: what is typed may not be what is meant
        let warn = ArcSignal::derive({
            let (caps, visible) = (caps.clone(), visible.clone());
            move || caps.get() && !visible.get()
        });
        field.trailing.push(Box::new(Show::new(warn, || {
            Label::new(t("kiesel.passwordField.capsLock"))
                .role(TextRole::Caption)
                .tone(Tone::Text(TextRole::Caption))
        })));
        if eye {
            let (show, hide) = (
                t("kiesel.passwordField.show"),
                t("kiesel.passwordField.hide"),
            );
            let name = ArcSignal::derive({
                let visible = visible.clone();
                move || {
                    if visible.get() {
                        hide.get()
                    } else {
                        show.get()
                    }
                }
            });
            let mut button = Button::new(name)
                .variant(Variant::Ghost)
                .size(ControlSize::Sm)
                .round()
                .icon(lucide::EYE)
                .icon_only()
                .pressed(visible.clone())
                .on_click(move || visible.update(|v| *v = !*v));
            button.icon_pressed = Some(lucide::EYE_OFF);
            field.trailing.push(Box::new(button));
        }
        field
    }
}

//! One Kiesel button, centred in the window; pressing it toggles `loading`.
//!
//!   kiesel-headless start --scale 2
//!   kiesel-headless run cargo run --example button

use std::time::Instant;

use kiesel_ui::kurbo::{Point, Rect, Size};
use kiesel_ui::shell::Pointer;
use kiesel_ui::{Button, CursorIcon, Cx, Options, RenderContext, Resources, Theme, View};

struct Demo {
    button: Button,
    size: Size,
}

impl Demo {
    fn place(&mut self, cx: &mut Cx<'_>) {
        let natural = self.button.measure(cx.text, cx.theme);
        let origin = Point::new(
            (self.size.width - natural.width) / 2.0,
            (self.size.height - natural.height) / 2.0,
        );
        self.button
            .set_bounds(Rect::from_origin_size(origin.round(), natural));
    }
}

impl View for Demo {
    fn layout(&mut self, cx: &mut Cx<'_>, size: Size) {
        self.size = size;
        self.place(cx);
    }

    fn paint(&self, ctx: &mut RenderContext, res: &mut Resources, theme: &Theme, now: Instant) {
        self.button.paint(ctx, res, theme, now);
    }

    fn pointer(&mut self, cx: &mut Cx<'_>, event: Pointer) -> bool {
        match event {
            Pointer::Move(p) => self.button.pointer_move(Some(p), cx.now, cx.theme),
            Pointer::Leave => self.button.pointer_move(None, cx.now, cx.theme),
            Pointer::Down(p) => self.button.pointer_down(p, cx.now, cx.theme),
            Pointer::Up(p) => {
                if self.button.pointer_up(p, cx.now, cx.theme) {
                    let loading = !self.button.loading();
                    self.button.set_loading(loading, cx.now, cx.theme);
                    return true; // the spinner makes the button wider
                }
            }
        }
        false
    }

    fn cursor(&self) -> CursorIcon {
        self.button.cursor()
    }

    fn animating(&self, theme: &Theme, now: Instant) -> bool {
        self.button.animating(now, theme)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let demo = Demo {
        button: Button::new("Save changes"),
        size: Size::ZERO,
    };
    kiesel_ui::run(
        Options {
            title: "Kiesel – Button".into(),
            size: Size::new(480.0, 300.0),
            appearance: Default::default(),
        },
        demo,
    )
}

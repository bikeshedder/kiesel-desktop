//! `Button` – the default variant, size md, with `loading` (see `kiesel/src/components/Button`).

use std::time::Instant;

use parley::Layout;
use vello_cpu::kurbo::{Affine, Arc, Cap, Point, Rect, RoundedRect, Shape, Size, Stroke, Vec2};
use vello_cpu::peniko::Gradient;
use vello_cpu::{RenderContext, Resources};
use winit::cursor::CursorIcon;

use crate::anim::Transition;
use crate::paint::{self, TOLERANCE};
use crate::text::{self, Text, TextStyle};
use crate::theme::{Theme, mix};

/// The spinner box is 1.25em (the icon's place), the spinner itself 1.05em
const SPINNER_BOX_EM: f64 = 1.25;
const SPINNER_EM: f64 = 1.05;
/// One turn every 900 ms, linear
const SPIN_PERIOD: f64 = 0.9;

pub struct Button {
    label: String,
    layout: Option<Layout<()>>,
    bounds: Rect,
    loading: bool,
    spin_start: Instant,
    hovered: bool,
    /// Pointer went down on the button and hasn't been released yet
    armed: bool,
    hover: Transition,
    press: Transition,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            layout: None,
            bounds: Rect::ZERO,
            loading: false,
            spin_start: Instant::now(),
            hovered: false,
            armed: false,
            hover: Transition::new(0.0),
            press: Transition::new(0.0),
        }
    }

    pub fn loading(&self) -> bool {
        self.loading
    }

    pub fn set_loading(&mut self, loading: bool, now: Instant, theme: &Theme) {
        if loading && !self.loading {
            self.spin_start = now;
        }
        self.loading = loading;
        self.sync(now, theme);
    }

    /// Natural size: inline padding + [spinner + gap] + label; at least `--ksl-control` high
    pub fn measure(&mut self, text: &mut Text, theme: &Theme) -> Size {
        let m = &theme.metrics;
        let layout = self.layout.get_or_insert_with(|| {
            text.line(
                &self.label,
                &TextStyle {
                    family: m.font_family,
                    size: m.font_size,
                    weight: m.font_weight,
                    letter_spacing: 0.01,
                },
            )
        });
        let mut content = f64::from(layout.width());
        if self.loading {
            content += SPINNER_BOX_EM * f64::from(m.font_size) + m.gap;
        }
        Size::new(
            content + 2.0 * m.padding_x,
            m.control.max(f64::from(layout.height())),
        )
    }

    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    pub fn bounds(&self) -> Rect {
        self.bounds
    }

    fn hit(&self, p: Point, theme: &Theme) -> bool {
        RoundedRect::from_rect(self.bounds, theme.metrics.radius).contains(p)
    }

    /// While loading the button is busy, not interactive-looking: no hover or press visuals
    fn sync(&mut self, now: Instant, theme: &Theme) {
        let d = theme.metrics.duration;
        let live = !self.loading;
        self.hover
            .set(f64::from(u8::from(live && self.hovered)), now, d);
        self.press.set(
            f64::from(u8::from(live && self.hovered && self.armed)),
            now,
            d,
        );
    }

    pub fn pointer_move(&mut self, p: Option<Point>, now: Instant, theme: &Theme) {
        self.hovered = p.is_some_and(|p| self.hit(p, theme));
        self.sync(now, theme);
    }

    pub fn pointer_down(&mut self, p: Point, now: Instant, theme: &Theme) {
        self.hovered = self.hit(p, theme);
        self.armed = self.hovered;
        self.sync(now, theme);
    }

    /// True when this completes a click (down and up on the button)
    pub fn pointer_up(&mut self, p: Point, now: Instant, theme: &Theme) -> bool {
        self.hovered = self.hit(p, theme);
        let clicked = self.armed && self.hovered;
        self.armed = false;
        self.sync(now, theme);
        clicked
    }

    pub fn cursor(&self) -> CursorIcon {
        match (self.hovered, self.loading) {
            (true, true) => CursorIcon::Progress,
            (true, false) => CursorIcon::Pointer,
            _ => CursorIcon::Default,
        }
    }

    pub fn animating(&self, now: Instant, theme: &Theme) -> bool {
        let d = theme.metrics.duration;
        self.loading || self.hover.animating(now, d) || self.press.animating(now, d)
    }

    pub fn paint(&self, ctx: &mut RenderContext, res: &mut Resources, theme: &Theme, now: Instant) {
        let m = &theme.metrics;
        let (h, p) = (
            self.hover.value(now, m.duration),
            self.press.value(now, m.duration),
        );
        let (r, radius) = (self.bounds, m.radius);

        // shadows: rest ↔ hover move with the hover transition, the pressed
        // (inset) set fades in as the raised one fades out
        let raised = paint::lerp_shadows(&theme.control_shadow, &theme.control_shadow_hover, h);
        let (up, down) = ((1.0 - p) as f32, p as f32);
        paint::outer_shadows(ctx, r, radius, &raised, up);

        let [top, bottom] = [0, 1].map(|i| {
            let c = theme.control_bg[i].lerp_rect(theme.control_bg_hover[i], h as f32);
            mix(c, theme.surface_sunken, down)
        });
        ctx.set_paint(Gradient::new_linear((r.x0, r.y0), (r.x0, r.y1)).with_stops([top, bottom]));
        ctx.fill_path(&paint::rounded(r, radius));

        paint::inset_shadows(ctx, r, radius, &raised, up);
        paint::inset_shadows(ctx, r, radius, &theme.control_shadow_pressed, down);

        // content: [spinner][gap]label, centred; pressed it sinks a little
        let Some(layout) = &self.layout else { return };
        let em = f64::from(m.font_size);
        let spinner_w = if self.loading {
            SPINNER_BOX_EM * em + m.gap
        } else {
            0.0
        };
        let content = Size::new(
            spinner_w + f64::from(layout.width()),
            f64::from(layout.height()),
        );
        let origin = r.center() - content.to_vec2() / 2.0;
        let centre = r.center();
        let base = *ctx.transform();
        let sink = Affine::translate(centre.to_vec2() + Vec2::new(0.0, 0.5 * p))
            * Affine::scale(1.0 - 0.015 * p)
            * Affine::translate(-centre.to_vec2());
        ctx.set_transform(base * sink);

        let fg = mix(theme.text_1, theme.accent_2, down);
        if self.loading {
            let box_size = SPINNER_BOX_EM * em;
            let c = Point::new(origin.x + box_size / 2.0, centre.y);
            let turn = (now.duration_since(self.spin_start).as_secs_f64() / SPIN_PERIOD).fract();
            spinner(ctx, c, SPINNER_EM * em, turn, fg);
        }
        text::draw(
            ctx,
            res,
            layout,
            Point::new(origin.x + spinner_w, origin.y),
            fg,
        );
        ctx.set_transform(base);
    }
}

/// Kiesel's Spinner: a 24-unit viewBox, circle r = 9, stroke 3; a faint track
/// and an 18-unit arc with round caps, turning by `turn` (0..1)
fn spinner(
    ctx: &mut RenderContext,
    centre: Point,
    size: f64,
    turn: f64,
    color: crate::theme::Color,
) {
    use std::f64::consts::TAU;
    let k = size / 24.0;
    let radius = 9.0 * k;
    ctx.set_stroke(Stroke::new(3.0 * k).with_caps(Cap::Round));
    let circle = |start: f64, sweep: f64| {
        Arc::new(centre, (radius, radius), start, sweep, 0.0).into_path(TOLERANCE)
    };
    ctx.set_paint(color.multiply_alpha(0.25));
    ctx.stroke_path(&circle(0.0, TAU));
    ctx.set_paint(color);
    ctx.stroke_path(&circle(turn * TAU, 18.0 / 9.0));
}

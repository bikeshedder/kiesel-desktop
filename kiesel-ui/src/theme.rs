//! Design tokens – the native counterpart of `kiesel/src/styles/theme.css`.
//!
//! Colours are resolved once from the appearance parameters (accent colour,
//! neutral hue and tint, depth) at the default contrast (`--ksl-contrast: 1`),
//! so the oklch values below are the CSS formulas with `c-mid = 1`.

use vello_cpu::peniko::color::{AlphaColor, Oklch, Srgb};

pub type Color = AlphaColor<Srgb>;

/// The appearance parameters on `:root` (`--ksl-accent-color`, `--ksl-roundness`, …)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Appearance {
    pub accent_color: Color,
    pub roundness: f32,
    pub density: f32,
    pub depth: f32,
    pub neutral_hue: f32,
    pub tint: f32,
    /// `--ksl-contrast`: 0 low, 1 medium, 2 high – values in between blend
    pub contrast: f32,
    /// Text sizes × this (the desktop's large-text setting)
    pub text_scale: f32,
    /// Transitions and animations take no time (the desktop's reduced motion)
    pub reduce_motion: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            accent_color: Color::from_rgb8(0x70, 0x7e, 0xd4),
            roundness: 1.0,
            density: 1.0,
            depth: 1.0,
            neutral_hue: 260.0,
            tint: 0.45,
            contrast: 1.0,
            text_scale: 1.0,
            reduce_motion: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scheme {
    Light,
    Dark,
}

/// One box shadow, as in CSS: offset, blur radius, spread, colour; `inset` draws inside the box
#[derive(Clone, Copy, Debug)]
pub struct Shadow {
    pub inset: bool,
    pub x: f64,
    pub y: f64,
    pub blur: f64,
    pub spread: f64,
    pub color: Color,
}

const fn outer(x: f64, y: f64, blur: f64, color: Color) -> Shadow {
    Shadow {
        inset: false,
        x,
        y,
        blur,
        spread: 0.0,
        color,
    }
}
const fn inset(x: f64, y: f64, blur: f64, color: Color) -> Shadow {
    Shadow {
        inset: true,
        x,
        y,
        blur,
        spread: 0.0,
        color,
    }
}

/// Resolved tokens for one scheme
#[derive(Clone, Debug)]
pub struct Theme {
    pub scheme: Scheme,
    pub bg: Color,
    pub surface_sunken: Color,
    /// `--ksl-card-bg`: the tonal surface of cards
    pub card_bg: Color,
    pub text_1: Color,
    pub text_2: Color,
    pub text_3: Color,
    pub accent: Color,
    pub accent_2: Color,
    /// `--ksl-control-bg`: a vertical gradient (top, bottom)
    pub control_bg: [Color; 2],
    pub control_bg_hover: [Color; 2],
    pub control_shadow: [Shadow; 4],
    pub control_shadow_hover: [Shadow; 4],
    pub control_shadow_pressed: [Shadow; 2],
    /// `--ksl-focus-ring`: 0 0 0 3px, the accent at 45 %
    pub focus_ring: Shadow,
    /// `--ksl-accent-soft`: selections, soft tints
    pub accent_soft: Color,
    /// `--ksl-field-bg`: the well of text fields
    pub field_bg: Color,
    /// `--ksl-field-shadow`: a 1px inner edge, an inner shade at the top, a sheen below
    pub field_shadow: [Shadow; 3],
    /// The edge while the pointer is over the field (`--ksl-text-3` at 55 %)
    pub field_shadow_hover: [Shadow; 3],
    /// `--ksl-field-shadow-focus`: the edge in the accent, a 3px ring at 22 %
    pub field_shadow_focus: [Shadow; 3],
    /// `--ksl-field-shadow-invalid`: the edge in `--ksl-danger`
    pub field_shadow_invalid: [Shadow; 3],
    /// Focused and invalid: the edge and a 3px ring in the danger colour
    pub field_shadow_invalid_focus: [Shadow; 3],
    /// `--ksl-danger`: errors, destructive actions
    pub danger: Color,
    /// `--ksl-on-accent`: text on the accent
    pub on_accent: Color,
    /// `--ksl-accent-fill`, `--ksl-accent-fill-2`: the accent filling a small mark
    /// (a ticked box, a chosen radio's knob), its light top and dark bottom –
    /// deeper than the accent in the dark, whose light text accent would glare
    pub accent_fill: Color,
    pub accent_fill_2: Color,
    /// `--ksl-on-accent-fill`: what sits on it (the tick)
    pub on_accent_fill: Color,
    /// `--ksl-icon`: icons that aren't text
    pub icon: Color,
    /// `--ksl-hi`, `--ksl-lo`, `--ksl-lo-strong`: the light and the dark of Soft UI depth
    pub hi: Color,
    pub lo: Color,
    pub lo_strong: Color,
    /// `--ksl-rim`: a fine light edge around wells
    pub rim: Color,
    /// `--ksl-knob`: the 145° gradient of round knobs (0 %, 55 %, 100 %)
    pub knob: [Color; 3],
    /// `--ksl-surface-raised`: popovers and dialogs
    pub surface_raised: Color,
    /// `--ksl-float`: what floats above the page (popovers, dialogs)
    pub float: [Shadow; 3],
    /// The desktop asks for fewer animations: see [`Theme::motion`]
    pub reduce_motion: bool,
    pub metrics: Metrics,
}

/// Sizes in logical pixels (1rem = 16px)
#[derive(Clone, Copy, Debug)]
pub struct Metrics {
    /// `--ksl-control`
    pub control: f64,
    /// `--ksl-radius`
    /// `--ksl-radius-sm`: small controls (a checkbox)
    pub radius_sm: f64,
    pub radius: f64,
    /// `--ksl-radius-lg` (cards)
    pub radius_lg: f64,
    /// `--size-4`, the button's inline padding
    pub padding_x: f64,
    /// `--size-2`
    pub gap: f64,
    /// `--font-size-1`
    pub font_size: f32,
    /// `--font-weight-6`
    pub font_weight: f32,
    /// `--ksl-font`
    pub font_family: &'static str,
    /// `--ksl-duration`, in seconds
    pub duration: f64,
}

/// A field's shadows: `inset 0 0 0 1px edge, inset 0 1.5px 1.5px inner, last`
fn field(edge: Color, inner: Color, last: Shadow) -> [Shadow; 3] {
    [
        Shadow {
            spread: 1.0,
            ..inset(0.0, 0.0, 0.0, edge)
        },
        inset(0.0, 1.5, 1.5, inner),
        last,
    ]
}

fn oklch(l: f32, c: f32, h: f32, a: f32) -> Color {
    AlphaColor::<Oklch>::new([l, c, h, a]).convert::<Srgb>()
}

fn white(a: f32) -> Color {
    Color::new([1.0, 1.0, 1.0, a])
}

impl Theme {
    pub fn new(scheme: Scheme, ap: &Appearance) -> Self {
        let (hue, t, d) = (ap.neutral_hue, ap.tint, ap.depth);
        let n = |l: f32, c: f32| oklch(l, c * t, hue, 1.0);
        let na = |l: f32, c: f32, a: f32| oklch(l, c * t, hue, a * d);
        // --ksl-c-low/mid/high: the contrast's weights between Kiesel's three levels
        let k = ap.contrast.clamp(0.0, 2.0);
        let (cl, ch) = ((1.0 - k).max(0.0), (k - 1.0).max(0.0));
        // --ksl-danger: lighter with less contrast (light), one red in the dark
        let danger = match scheme {
            Scheme::Light => mix(
                Color::from_rgb8(0xc8, 0x43, 0x5d),
                Color::from_rgb8(0xd6, 0x5a, 0x70),
                cl,
            ),
            Scheme::Dark => Color::from_rgb8(0xef, 0x83, 0x96),
        };
        let cm = 1.0 - cl - ch;
        let c3 = |low: f32, mid: f32, high: f32| low * cl + mid * cm + high * ch;
        // the accent keeps hue and chroma of --ksl-accent-color, at each theme's lightness
        let [_, ac, ah, _] = ap.accent_color.convert::<Oklch>().components;

        let (
            bg,
            sunken,
            card_bg,
            text_1,
            text_2,
            text_3,
            accent,
            accent_2,
            hi,
            lo_strong,
            lo_soft,
            sheen,
            bg_top,
            bg_bottom,
            hover_top,
            hover_bottom,
            field_bg,
            field_edge,
            field_inner,
            icon,
            rim,
            knob,
            lo,
            raised,
        ) = match scheme {
            Scheme::Light => (
                n(0.949, 0.0058),
                n(0.921, 0.0087),
                n(0.970, 0.0041),
                n(c3(0.374, 0.310, 0.250), 0.0360),
                n(c3(0.530, 0.470, 0.410), 0.0350),
                n(c3(0.733, 0.585, 0.500), 0.0300),
                oklch(c3(0.62, 0.57, 0.52), ac.min(0.18), ah, 1.0),
                oklch(c3(0.54, 0.50, 0.46), (ac * 1.08).min(0.19), ah, 1.0),
                white(0.95 * d),
                na(0.599, 0.0360, 0.55),
                na(0.701, 0.0363, 0.32),
                white(0.85),
                Color::from_rgb8(0xfd, 0xfd, 0xfe),
                n(0.962, 0.0054),
                Color::from_rgb8(0xff, 0xff, 0xff),
                n(0.973, 0.0041),
                n(0.930, 0.0087),
                oklch(0.374, 0.0353 * t, hue, c3(0.16, 0.24, 0.32)),
                oklch(0.374, 0.0353 * t, hue, 0.10),
                n(c3(0.783, 0.600, 0.500), 0.0280),
                white(0.7),
                [
                    Color::from_rgb8(0xff, 0xff, 0xff),
                    n(0.955, 0.0058),
                    n(0.924, 0.0087),
                ],
                na(0.739, 0.0285, 0.55),
                n(0.961, 0.0041),
            ),
            Scheme::Dark => {
                // in dark, the surfaces darken with contrast too
                let lo_l = c3(0.177, 0.120, 0.070);
                let lo_c = c3(0.0230, 0.0150, 0.0100);
                let deep_l = c3(0.142, 0.090, 0.050);
                let deep_c = c3(0.0155, 0.0100, 0.0060);
                (
                    n(c3(0.359, 0.275, 0.215), c3(0.0357, 0.0240, 0.0190)),
                    n(c3(0.305, 0.230, 0.175), c3(0.0313, 0.0200, 0.0160)),
                    n(c3(0.375, 0.295, 0.235), c3(0.0370, 0.0250, 0.0200)),
                    n(c3(0.913, 0.920, 0.960), c3(0.0139, 0.0120, 0.0080)),
                    n(c3(0.746, 0.780, 0.840), c3(0.0264, 0.0200, 0.0160)),
                    n(c3(0.589, 0.640, 0.720), c3(0.0329, 0.0260, 0.0220)),
                    oklch(0.76, (ac * 0.85).min(0.15), ah, 1.0),
                    oklch(0.81, (ac * 0.77).min(0.14), ah, 1.0),
                    white(0.07 * d),
                    na(deep_l, deep_c, 0.7),
                    na(lo_l, lo_c, 0.38),
                    white(0.09),
                    n(c3(0.427, 0.360, 0.300), c3(0.0409, 0.0300, 0.0250)),
                    n(c3(0.387, 0.322, 0.265), c3(0.0384, 0.0280, 0.0230)),
                    n(c3(0.450, 0.385, 0.325), c3(0.0420, 0.0320, 0.0270)),
                    n(c3(0.406, 0.343, 0.285), c3(0.0395, 0.0300, 0.0250)),
                    n(c3(0.313, 0.240, 0.185), c3(0.0328, 0.0210, 0.0170)),
                    oklch(deep_l, deep_c * t, hue, 0.55),
                    oklch(deep_l, deep_c * t, hue, 0.35),
                    n(c3(0.609, 0.660, 0.740), c3(0.0326, 0.0260, 0.0220)),
                    white(0.05),
                    [n(0.695, 0.0298), n(0.646, 0.0304), n(0.600, 0.0324)],
                    na(lo_l, lo_c, 0.55),
                    n(c3(0.379, 0.300, 0.240), c3(0.0385, 0.0260, 0.0210)),
                )
            }
        };

        Self {
            scheme,
            bg,
            surface_sunken: sunken,
            card_bg,
            text_1,
            text_2,
            text_3,
            accent,
            accent_2,
            control_bg: [bg_top, bg_bottom],
            control_bg_hover: [hover_top, hover_bottom],
            control_shadow: [
                inset(0.0, 1.0, 0.0, sheen),
                outer(0.0, 1.0, 2.0, lo_strong),
                outer(2.0, 3.0, 6.0, lo_soft),
                outer(-1.0, -1.0, 2.0, hi),
            ],
            control_shadow_hover: [
                inset(0.0, 1.0, 0.0, sheen),
                outer(0.0, 1.0, 2.0, lo_strong),
                outer(3.0, 5.0, 10.0, lo_soft),
                outer(-2.0, -2.0, 4.0, hi),
            ],
            control_shadow_pressed: [inset(2.0, 2.0, 4.0, lo_strong), inset(-1.0, -1.0, 2.0, hi)],
            focus_ring: Shadow {
                spread: 3.0,
                ..outer(0.0, 0.0, 0.0, accent.multiply_alpha(0.45))
            },
            accent_soft: mix(bg, accent, if scheme == Scheme::Dark { 0.18 } else { 0.16 }),
            field_bg,
            field_shadow: field(field_edge, field_inner, outer(0.0, 1.0, 0.0, sheen)),
            field_shadow_hover: field(
                text_3.multiply_alpha(0.55),
                field_inner,
                outer(0.0, 1.0, 0.0, sheen),
            ),
            field_shadow_focus: field(
                accent,
                field_inner,
                Shadow {
                    spread: 3.0,
                    ..outer(0.0, 0.0, 0.0, accent.multiply_alpha(0.22))
                },
            ),
            field_shadow_invalid: field(danger, field_inner, outer(0.0, 1.0, 0.0, sheen)),
            field_shadow_invalid_focus: field(
                danger,
                field_inner,
                Shadow {
                    spread: 3.0,
                    ..outer(0.0, 0.0, 0.0, danger.multiply_alpha(0.22))
                },
            ),
            danger,
            on_accent: match scheme {
                Scheme::Light => white(1.0),
                Scheme::Dark => n(0.278, 0.0356),
            },
            accent_fill: match scheme {
                Scheme::Light => accent,
                Scheme::Dark => oklch(0.62, (ac * 0.95).min(0.16), ah, 1.0),
            },
            accent_fill_2: match scheme {
                Scheme::Light => accent_2,
                Scheme::Dark => oklch(0.52, ac.min(0.16), ah, 1.0),
            },
            on_accent_fill: match scheme {
                Scheme::Light => white(1.0),
                Scheme::Dark => oklch(0.985, 0.0, 0.0, 1.0),
            },
            icon,
            hi,
            lo,
            lo_strong,
            rim,
            knob,
            surface_raised: raised,
            reduce_motion: ap.reduce_motion,
            float: [
                Shadow {
                    spread: -16.0,
                    ..outer(0.0, 24.0, 48.0, lo_strong)
                },
                outer(-8.0, -8.0, 24.0, hi),
                outer(8.0, 8.0, 24.0, lo),
            ],
            metrics: Metrics {
                // larger text makes controls taller, as on GTK (Kiesel Web's are px)
                control: 40.0 * f64::from(ap.density) * f64::from(ap.text_scale.max(1.0)),
                radius: 6.0 * f64::from(ap.roundness),
                radius_lg: 12.0 * f64::from(ap.roundness),
                radius_sm: 5.0 * f64::from(ap.roundness),
                padding_x: 20.0,
                gap: 8.0,
                font_size: 16.0 * ap.text_scale,
                font_weight: 600.0,
                font_family: "Inter, system-ui, sans-serif",
                duration: if ap.reduce_motion { 0.0 } else { 0.180 },
            },
        }
    }
}

impl Theme {
    /// Between two themes, colours mixed in oklab – for the cross-fade when the
    /// scheme changes. Sizes come from `b`; the scheme flips half-way.
    pub fn mix(a: &Theme, b: &Theme, t: f32) -> Theme {
        use crate::paint::lerp_shadows as sh;
        let c = |x: Color, y: Color| mix(x, y, t);
        let t64 = f64::from(t);
        Theme {
            scheme: if t < 0.5 { a.scheme } else { b.scheme },
            bg: c(a.bg, b.bg),
            surface_sunken: c(a.surface_sunken, b.surface_sunken),
            card_bg: c(a.card_bg, b.card_bg),
            text_1: c(a.text_1, b.text_1),
            text_2: c(a.text_2, b.text_2),
            text_3: c(a.text_3, b.text_3),
            accent: c(a.accent, b.accent),
            accent_2: c(a.accent_2, b.accent_2),
            control_bg: [
                c(a.control_bg[0], b.control_bg[0]),
                c(a.control_bg[1], b.control_bg[1]),
            ],
            control_bg_hover: [
                c(a.control_bg_hover[0], b.control_bg_hover[0]),
                c(a.control_bg_hover[1], b.control_bg_hover[1]),
            ],
            control_shadow: sh(&a.control_shadow, &b.control_shadow, t64),
            control_shadow_hover: sh(&a.control_shadow_hover, &b.control_shadow_hover, t64),
            control_shadow_pressed: sh(&a.control_shadow_pressed, &b.control_shadow_pressed, t64),
            focus_ring: sh(&[a.focus_ring], &[b.focus_ring], t64)[0],
            accent_soft: c(a.accent_soft, b.accent_soft),
            field_bg: c(a.field_bg, b.field_bg),
            field_shadow: sh(&a.field_shadow, &b.field_shadow, t64),
            field_shadow_hover: sh(&a.field_shadow_hover, &b.field_shadow_hover, t64),
            field_shadow_focus: sh(&a.field_shadow_focus, &b.field_shadow_focus, t64),
            field_shadow_invalid: sh(&a.field_shadow_invalid, &b.field_shadow_invalid, t64),
            field_shadow_invalid_focus: sh(
                &a.field_shadow_invalid_focus,
                &b.field_shadow_invalid_focus,
                t64,
            ),
            danger: c(a.danger, b.danger),
            on_accent: c(a.on_accent, b.on_accent),
            accent_fill: c(a.accent_fill, b.accent_fill),
            accent_fill_2: c(a.accent_fill_2, b.accent_fill_2),
            on_accent_fill: c(a.on_accent_fill, b.on_accent_fill),
            icon: c(a.icon, b.icon),
            hi: c(a.hi, b.hi),
            lo: c(a.lo, b.lo),
            lo_strong: c(a.lo_strong, b.lo_strong),
            rim: c(a.rim, b.rim),
            knob: [
                c(a.knob[0], b.knob[0]),
                c(a.knob[1], b.knob[1]),
                c(a.knob[2], b.knob[2]),
            ],
            surface_raised: c(a.surface_raised, b.surface_raised),
            reduce_motion: b.reduce_motion,
            float: sh(&a.float, &b.float, t64),
            metrics: b.metrics,
        }
    }
}

/// The Open Props spacing steps used by Kiesel (`--size-1` … `--size-10`), in logical pixels
pub fn space(step: u8) -> f64 {
    const STEPS: [f64; 11] = [
        0.0, 4.0, 8.0, 16.0, 20.0, 24.0, 28.0, 32.0, 48.0, 64.0, 80.0,
    ];
    STEPS[usize::from(step.min(10))]
}

/// Control sizes as in Kiesel (`xs` … `xl`)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ControlSize {
    Xs,
    Sm,
    #[default]
    Md,
    Lg,
    Xl,
}

/// Text roles of the type scale
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextRole {
    /// Body text: `--font-size-1`, line height 1.5, `--ksl-text-2` (as in a card's body)
    Body,
    /// Headings: `--font-size-2`, weight 600, line height 1.25, `--ksl-text-1`
    Title,
    /// Captions and subtitles: `--font-size-0`, `--ksl-text-3`
    Caption,
    /// A field's label: `--font-size-0`, weight 600, `--ksl-text-2`
    FieldLabel,
    /// Small controls' text: `--font-size-0`, `--ksl-text-2`
    Small,
    /// Large controls' text: `--font-size-2`, `--ksl-text-2`
    Large,
    /// A field's error: `--font-size-0`, weight 500, `--ksl-danger`
    Error,
}

/// Font settings of one piece of text
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub family: &'static str,
    pub size: f32,
    pub weight: f32,
    /// In em, like CSS `letter-spacing: 0.01em`
    pub letter_spacing: f32,
    /// Relative to the font size, like CSS `line-height: 1.5`
    pub line_height: f32,
}

impl Theme {
    /// How long an animation of `seconds` takes: no time with reduced motion
    /// (like Kiesel Web's 1 ms transitions)
    pub fn motion(&self, seconds: f64) -> f64 {
        if self.reduce_motion { 0.0 } else { seconds }
    }

    pub fn text_style(&self, role: TextRole) -> TextStyle {
        let family = self.metrics.font_family;
        let (size, weight, line_height) = match role {
            TextRole::Body => (16.0, 400.0, 1.5),
            TextRole::Title => (17.6, 600.0, 1.25),
            TextRole::Caption => (12.0, 400.0, 1.5),
            TextRole::FieldLabel => (12.0, 600.0, 1.5),
            TextRole::Small => (12.0, 400.0, 1.5),
            TextRole::Large => (17.6, 400.0, 1.5),
            TextRole::Error => (12.0, 500.0, 1.5),
        };
        // the desktop's text scale (font_size is 16 × it)
        let scale = self.metrics.font_size / 16.0;
        TextStyle {
            family,
            size: size * scale,
            weight,
            letter_spacing: 0.0,
            line_height,
        }
    }

    pub fn text_color(&self, role: TextRole) -> Color {
        match role {
            TextRole::Body => self.text_2,
            TextRole::Title => self.text_1,
            TextRole::Caption => self.text_3,
            TextRole::FieldLabel | TextRole::Small | TextRole::Large => self.text_2,
            TextRole::Error => self.danger,
        }
    }

    /// The button label: `--font-size-1`, weight 600, letter-spacing 0.01em
    pub fn control_text(&self) -> TextStyle {
        let m = &self.metrics;
        TextStyle {
            family: m.font_family,
            size: m.font_size,
            weight: m.font_weight,
            letter_spacing: 0.01,
            line_height: 1.5,
        }
    }
}

/// Mixes two colours in oklab, like CSS `color-mix(in oklab, a, b t)`
pub fn mix(a: Color, b: Color, t: f32) -> Color {
    if t <= 0.0 {
        return a;
    }
    if t >= 1.0 {
        return b;
    }
    use vello_cpu::peniko::color::Oklab;
    let (a, b) = (a.convert::<Oklab>(), b.convert::<Oklab>());
    a.lerp_rect(b, t).convert::<Srgb>()
}

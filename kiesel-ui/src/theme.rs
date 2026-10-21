//! Design tokens – the native counterpart of `kiesel/src/styles/theme.css`.
//!
//! Colours are resolved once from the appearance parameters (accent colour,
//! neutral hue and tint, depth) at the default contrast (`--ksl-contrast: 1`),
//! so the oklch values below are the CSS formulas with `c-mid = 1`.

use vello_cpu::peniko::color::{AlphaColor, Oklch, Srgb};

pub type Color = AlphaColor<Srgb>;

/// The appearance parameters on `:root` (`--ksl-accent-color`, `--ksl-roundness`, …)
#[derive(Clone, Copy, Debug)]
pub struct Appearance {
    pub accent_color: Color,
    pub roundness: f32,
    pub density: f32,
    pub depth: f32,
    pub neutral_hue: f32,
    pub tint: f32,
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
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scheme {
    Light,
    Dark,
}

/// One box shadow, as in CSS: offset, blur radius, colour; `inset` draws inside the box
#[derive(Clone, Copy, Debug)]
pub struct Shadow {
    pub inset: bool,
    pub x: f64,
    pub y: f64,
    pub blur: f64,
    pub color: Color,
}

const fn outer(x: f64, y: f64, blur: f64, color: Color) -> Shadow {
    Shadow {
        inset: false,
        x,
        y,
        blur,
        color,
    }
}
const fn inset(x: f64, y: f64, blur: f64, color: Color) -> Shadow {
    Shadow {
        inset: true,
        x,
        y,
        blur,
        color,
    }
}

/// Resolved tokens for one scheme
#[derive(Clone, Debug)]
pub struct Theme {
    pub scheme: Scheme,
    pub bg: Color,
    pub surface_sunken: Color,
    pub text_1: Color,
    pub text_3: Color,
    pub accent: Color,
    pub accent_2: Color,
    /// `--ksl-control-bg`: a vertical gradient (top, bottom)
    pub control_bg: [Color; 2],
    pub control_bg_hover: [Color; 2],
    pub control_shadow: [Shadow; 4],
    pub control_shadow_hover: [Shadow; 4],
    pub control_shadow_pressed: [Shadow; 2],
    pub metrics: Metrics,
}

/// Sizes in logical pixels (1rem = 16px)
#[derive(Clone, Copy, Debug)]
pub struct Metrics {
    /// `--ksl-control`
    pub control: f64,
    /// `--ksl-radius`
    pub radius: f64,
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
        // the accent keeps hue and chroma of --ksl-accent-color, at each theme's lightness
        let [_, ac, ah, _] = ap.accent_color.convert::<Oklch>().components;

        let (
            bg,
            sunken,
            text_1,
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
        ) = match scheme {
            Scheme::Light => (
                n(0.949, 0.0058),
                n(0.921, 0.0087),
                n(0.310, 0.0360),
                n(0.585, 0.0300),
                oklch(0.57, ac.min(0.18), ah, 1.0),
                oklch(0.50, (ac * 1.08).min(0.19), ah, 1.0),
                white(0.95 * d),
                na(0.599, 0.0360, 0.55),
                na(0.701, 0.0363, 0.32),
                white(0.85),
                Color::from_rgb8(0xfd, 0xfd, 0xfe),
                n(0.962, 0.0054),
                Color::from_rgb8(0xff, 0xff, 0xff),
                n(0.973, 0.0041),
            ),
            Scheme::Dark => (
                n(0.275, 0.0240),
                n(0.230, 0.0200),
                n(0.920, 0.0120),
                n(0.640, 0.0260),
                oklch(0.76, (ac * 0.85).min(0.15), ah, 1.0),
                oklch(0.81, (ac * 0.77).min(0.14), ah, 1.0),
                white(0.07 * d),
                na(0.090, 0.0100, 0.7),
                na(0.120, 0.0150, 0.38),
                white(0.09),
                n(0.360, 0.0300),
                n(0.322, 0.0280),
                n(0.385, 0.0320),
                n(0.343, 0.0300),
            ),
        };

        Self {
            scheme,
            bg,
            surface_sunken: sunken,
            text_1,
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
            metrics: Metrics {
                control: 40.0 * f64::from(ap.density),
                radius: 6.0 * f64::from(ap.roundness),
                padding_x: 20.0,
                gap: 8.0,
                font_size: 16.0,
                font_weight: 600.0,
                font_family: "Inter, system-ui, sans-serif",
                duration: 0.180,
            },
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

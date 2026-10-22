//! `Button` – in five variants (how prominent), three sizes, with a colour
//! (what it means), `loading`, `pressed` and `disabled` (see
//! `kiesel/src/components/Button`). [`ButtonGroup`] attaches buttons into one
//! raised body, or spaces them.
//!
//! Activation follows the browser's `<button>`: a pointer press and release on
//! it, Space (on release, pressed look while held), Enter (on press), or the
//! assistive technology's click. Focus from the keyboard shows the focus ring
//! (`:focus-visible`), focus from a pointer press doesn't. A disabled button
//! does nothing and leaves the Tab order.

use std::time::Instant;

use parley::Layout;
use reactive_graph::traits::{GetUntracked, Track};
use reactive_graph::wrappers::read::ArcSignal;
use vello_cpu::RenderContext;
use vello_cpu::color::{AlphaColor, Oklch, Srgb};
use vello_cpu::kurbo::{
    Affine, Arc, Cap, Point, Rect, RoundedRect, RoundedRectRadii, Shape, Size, Stroke, Vec2,
};
use vello_cpu::peniko::Gradient;
use winit::cursor::CursorIcon;
use winit::keyboard::{Key, NamedKey};

use crate::anim::Transition;
use crate::i18n::{IntoText, TextProp};
use crate::icon::{IconNode, STROKE};
use crate::paint::{self, TOLERANCE};
use crate::reactive::{MountCx, Watch};
use crate::shortcut::{IntoShortcut, Shortcut};
use crate::text;
use crate::theme::{Color, ControlSize, Scheme, Shadow, TextStyle, Theme, mix, space};
use crate::widget::{
    Available, Change, Event, EventCx, Known, LayoutCx, PaintCx, Widget, WidgetId, accessible_rect,
};

/// The spinner box is 1.25em (the icon's place), the spinner itself 1.05em
const SPINNER_BOX_EM: f64 = 1.25;
const SPINNER_EM: f64 = 1.05;
/// Icons are 1.25em, 1.3em on icon-only buttons
const ICON_EM: f64 = 1.25;
const ICON_ONLY_EM: f64 = 1.3;
/// One turn every 900 ms, linear
const SPIN_PERIOD: f64 = 0.9;

/// How prominent a button is – its colour is a separate axis ([`ButtonColor`])
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Variant {
    /// Raised, in the surface's tone
    #[default]
    Default,
    /// Raised, filled with the accent (or its colour): the main action
    Primary,
    /// Flat, a tint of its colour (grey without one)
    Soft,
    /// Flat, a thin edge in its colour
    Outline,
    /// No body until the pointer is over it: quiet actions, toolbars
    Ghost,
}

/// What a button's colour means – or any colour; its lightness follows the
/// theme like the accent's
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ButtonColor {
    Accent,
    Danger,
    Success,
    Warning,
    Info,
    Custom(Color),
}

/// Where a button sits in an attached group: only the group's outer corners are round
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Segment {
    pub first: bool,
    pub last: bool,
    pub vertical: bool,
}

/// Height, inline padding, type size and corner radius of a size
pub(crate) fn dims(theme: &Theme, size: ControlSize) -> (f64, f64, f32, f64) {
    let m = &theme.metrics;
    let scale = m.font_size / 16.0;
    match size {
        ControlSize::Xs | ControlSize::Sm => (m.control * 0.8, space(3), 12.0 * scale, m.radius_sm),
        ControlSize::Md => (m.control, space(4), 16.0 * scale, m.radius),
        ControlSize::Lg | ControlSize::Xl => (
            m.control * 1.2,
            space(6),
            17.6 * scale,
            m.radius * 8.0 / 6.0,
        ),
    }
}

/// A button's paint in one variant and colour: backgrounds (top, middle,
/// bottom) and shadows at rest, under the pointer and pressed; label colours
struct Palette {
    bg: [Color; 3],
    bg_hover: [Color; 3],
    bg_active: [Color; 3],
    shadow: Vec<Shadow>,
    shadow_hover: Vec<Shadow>,
    shadow_active: Vec<Shadow>,
    fg: Color,
    fg_active: Color,
}

fn shade(inset: bool, x: f64, y: f64, blur: f64, spread: f64, color: Color) -> Shadow {
    Shadow {
        inset,
        x,
        y,
        blur,
        spread,
        color,
    }
}

const WHITE: Color = Color::WHITE;
const BLACK: Color = Color::BLACK;
const CLEAR: Color = Color::TRANSPARENT;

/// The accent pair of a colour: `--ksl-accent` and `--ksl-accent-2` re-derived
/// as Kiesel's `.colored` does – the colour's hue and chroma at the theme's lightness
fn accents(theme: &Theme, color: Option<ButtonColor>) -> (Color, Color) {
    let base = match color {
        None | Some(ButtonColor::Accent) => return (theme.accent, theme.accent_2),
        Some(ButtonColor::Danger) => theme.danger,
        Some(ButtonColor::Success) => theme.success,
        Some(ButtonColor::Warning) => theme.warning,
        Some(ButtonColor::Info) => theme.info,
        Some(ButtonColor::Custom(c)) => c,
    };
    let [_, c, h, _] = base.convert::<Oklch>().components;
    let dark = f32::from(u8::from(theme.scheme == Scheme::Dark));
    let at = |l: f32| AlphaColor::<Oklch>::new([l, c.min(0.2), h, 1.0]).convert::<Srgb>();
    (at(0.62 + 0.14 * dark), at(0.54 + 0.27 * dark))
}

pub struct Button {
    id: WidgetId,
    label: TextProp,
    /// What assistive technology announces, if not the label
    accessible_name: Option<TextProp>,
    layout: Option<Layout<()>>,
    /// What the label was shaped with – the theme's text size can change
    style: Option<TextStyle>,
    bounds: Rect,
    variant: Variant,
    color: Option<ButtonColor>,
    size: ControlSize,
    /// A height of its own instead of its size's (steppers inside a field)
    pub(crate) height: Option<f64>,
    round: bool,
    /// In an attached group: no body of its own
    pub(crate) segment: Option<Segment>,
    loading: ArcSignal<bool>,
    pressed: Option<ArcSignal<bool>>,
    disabled: ArcSignal<bool>,
    watch: Option<Watch>,
    /// `loading`, `pressed`, `disabled` as of the last update – what is on screen
    shown_loading: bool,
    shown_pressed: bool,
    shown_disabled: bool,
    spin_start: Instant,
    on_click: Option<Box<dyn FnMut()>>,
    /// Before and after the label
    icon: Option<IconNode>,
    icon_end: Option<IconNode>,
    /// Square, the icon alone; the label only names it for assistive technology
    icon_only: bool,
    /// Not in the Tab order (steppers: the field's own keys do the same)
    pub(crate) skip_tab: bool,
    /// Press it from anywhere in the window
    shortcuts: Vec<Shortcut>,
    hovered: bool,
    /// Pointer went down on the button and hasn't been released yet
    pointer_armed: bool,
    /// Space is held while focused
    key_armed: bool,
    focused: bool,
    focus_visible: bool,
    /// Its look changed since the last update
    repaint: bool,
    hover: Transition,
    press: Transition,
    ring: Transition,
}

impl Button {
    /// `label`: a string, or a signal – `t("…")` follows the language
    pub fn new(label: impl IntoText) -> Self {
        Self {
            id: WidgetId::next(),
            label: TextProp::new(label),
            accessible_name: None,
            layout: None,
            style: None,
            bounds: Rect::ZERO,
            variant: Variant::Default,
            color: None,
            size: ControlSize::Md,
            height: None,
            round: false,
            segment: None,
            loading: ArcSignal::stored(false),
            pressed: None,
            disabled: ArcSignal::stored(false),
            watch: None,
            shown_loading: false,
            shown_pressed: false,
            shown_disabled: false,
            spin_start: Instant::now(),
            shortcuts: Vec::new(),
            on_click: None,
            icon: None,
            icon_end: None,
            icon_only: false,
            skip_tab: false,
            hovered: false,
            pointer_armed: false,
            key_armed: false,
            focused: false,
            focus_visible: false,
            repaint: false,
            hover: Transition::new(0.0),
            press: Transition::new(0.0),
            ring: Transition::new(0.0),
        }
    }

    /// How prominent: default, primary, soft, outline, ghost
    pub fn variant(mut self, variant: Variant) -> Self {
        self.variant = variant;
        self
    }

    /// `Button::new(…).primary()` – the main action
    pub fn primary(self) -> Self {
        self.variant(Variant::Primary)
    }

    /// What it means (danger, success …) or any colour: primary is filled
    /// with it, the others colour label and icon
    pub fn color(mut self, color: ButtonColor) -> Self {
        self.color = Some(color);
        self
    }

    /// Sm, Md (default), Lg – the heights of fields of the same size
    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    /// Fully round: a pill, or a circle when icon-only
    pub fn round(mut self) -> Self {
        self.round = true;
        self
    }

    /// An icon before the label: `.icon(lucide::PLUS)` (the spinner takes its place while loading)
    pub fn icon(mut self, icon: IconNode) -> Self {
        self.icon = Some(icon);
        self
    }

    /// An icon after the label
    pub fn icon_end(mut self, icon: IconNode) -> Self {
        self.icon_end = Some(icon);
        self
    }

    /// Square, showing only its icon; the label still names it for assistive
    /// technology – make it say what the button does ("Delete", not "Trash")
    pub fn icon_only(mut self) -> Self {
        self.icon_only = true;
        self
    }

    /// Busy: a spinner before the label; the label stays
    /// A value, a signal, or `ArcSignal::derive(…)`
    pub fn loading(mut self, loading: impl Into<ArcSignal<bool>>) -> Self {
        self.loading = loading.into();
        self
    }

    /// A toggle button: pushed in while this is true (and so announced)
    pub fn pressed(mut self, pressed: impl Into<ArcSignal<bool>>) -> Self {
        self.pressed = Some(pressed.into());
        self
    }

    /// Not available: faded, does nothing, out of the Tab order
    pub fn disabled(mut self, disabled: impl Into<ArcSignal<bool>>) -> Self {
        self.disabled = disabled.into();
        self
    }

    /// A name for assistive technology when the label alone is ambiguous – many
    /// "Remove" buttons in a list each say what they remove (`aria-label`)
    pub fn accessible_name(mut self, name: impl IntoText) -> Self {
        self.accessible_name = Some(TextProp::new(name));
        self
    }

    /// Clicks it from anywhere in the window – unless the focused widget uses
    /// the keys itself (Ctrl+A in a text field). "Primary" is Ctrl, or ⌘ on macOS.
    pub fn shortcut(mut self, shortcut: impl IntoShortcut) -> Self {
        self.shortcuts.push(shortcut.into_shortcut());
        self
    }

    pub fn on_click(mut self, f: impl FnMut() + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }

    /// What it shows, left to right, and how wide each is
    fn items(&self, em: f64, label: f64) -> Vec<(Item, f64)> {
        let lead = if self.icon_only {
            ICON_ONLY_EM
        } else {
            ICON_EM
        };
        let mut items = Vec::new();
        if self.shown_loading {
            let size = if self.icon_only {
                ICON_ONLY_EM
            } else {
                SPINNER_BOX_EM
            };
            items.push((Item::Spinner, size * em));
        } else if let Some(icon) = self.icon {
            items.push((Item::Icon(icon), lead * em));
        }
        if !self.icon_only {
            items.push((Item::Label, label));
            if let Some(icon) = self.icon_end {
                items.push((Item::Icon(icon), ICON_EM * em));
            }
        }
        items
    }

    fn activate(&mut self) {
        if self.shown_disabled {
            return;
        }
        if let Some(f) = &mut self.on_click {
            f();
        }
    }

    fn radius(&self, theme: &Theme) -> f64 {
        if self.round {
            self.bounds.height() / 2.0
        } else {
            dims(theme, self.size).3
        }
    }

    /// The body's outline: in an attached group only the group's outer corners are round
    fn shape(&self, theme: &Theme) -> RoundedRect {
        let r = self.radius(theme);
        let radii = match self.segment {
            None => RoundedRectRadii::from_single_radius(r),
            Some(s) => {
                let (start, end) = (if s.first { r } else { 0.0 }, if s.last { r } else { 0.0 });
                if s.vertical {
                    RoundedRectRadii::new(start, start, end, end)
                } else {
                    RoundedRectRadii::new(start, end, end, start)
                }
            }
        };
        RoundedRect::from_rect(self.bounds, radii)
    }

    fn hit_point(&self, p: Point) -> bool {
        let r = if self.round {
            self.bounds.height() / 2.0
        } else {
            4.0
        };
        RoundedRect::from_rect(self.bounds, r).contains(p)
    }

    /// Moves the transitions towards the current state. Loading or disabled,
    /// the button isn't interactive-looking: no hover or press visuals.
    fn sync(&mut self, now: Instant, theme: &Theme) {
        let d = theme.metrics.duration;
        let live = !self.shown_loading && !self.shown_disabled;
        let pressed = (self.hovered && self.pointer_armed) || self.key_armed || self.shown_pressed;
        let target = |on: bool| f64::from(u8::from(on));
        let mut changed = self.hover.set(target(live && self.hovered), now, d);
        changed |= self.press.set(target(live && pressed), now, d);
        changed |= self
            .ring
            .set(target(self.focused && self.focus_visible), now, d);
        self.repaint |= changed;
    }

    /// The label: `--font-size-1` (sm `-0`, lg `-2`), weight 600, letter-spacing 0.01em
    fn text_style(&self, theme: &Theme) -> TextStyle {
        let mut style = theme.control_text();
        style.size = dims(theme, self.size).2;
        style
    }

    /// The paint of its variant and colour
    fn palette(&self, theme: &Theme) -> Palette {
        let (accent, accent_2) = accents(theme, self.color);
        let colored = self.color.is_some();
        let sunken = [theme.surface_sunken; 3];
        let grad = |[a, b]: [Color; 2]| [a, mix(a, b, 0.45), b];
        // label: --ksl-text-1 (ghost --ksl-text-2), an icon alone --ksl-icon;
        // coloured buttons in --ksl-accent-2; pressed in --ksl-accent-2
        let plain_fg = match (self.variant, self.icon_only) {
            (_, true) => theme.icon,
            (Variant::Ghost, false) => theme.text_2,
            _ => theme.text_1,
        };
        let fg = if colored { accent_2 } else { plain_fg };
        if self.segment.is_some() {
            // attached: the group is the body; a pushed-in segment sinks into it
            return Palette {
                bg: [CLEAR; 3],
                bg_hover: [theme.hover_bg; 3],
                bg_active: sunken,
                shadow: Vec::new(),
                shadow_hover: Vec::new(),
                shadow_active: theme.control_shadow_pressed.to_vec(),
                fg: if colored { accent_2 } else { theme.text_1 },
                fg_active: accent_2,
            };
        }
        match self.variant {
            Variant::Default => Palette {
                bg: grad(theme.control_bg),
                bg_hover: grad(theme.control_bg_hover),
                bg_active: sunken,
                shadow: theme.control_shadow.to_vec(),
                shadow_hover: theme.control_shadow_hover.to_vec(),
                shadow_active: theme.control_shadow_pressed.to_vec(),
                fg,
                fg_active: accent_2,
            },
            Variant::Primary => {
                let glow = |a: f32| accent.multiply_alpha(a * theme.depth);
                let rim = mix(accent, BLACK, 0.35);
                Palette {
                    bg: [
                        mix(accent, WHITE, 0.26),
                        mix(accent, WHITE, 0.06),
                        mix(accent, BLACK, 0.16),
                    ],
                    bg_hover: [
                        mix(accent, WHITE, 0.34),
                        mix(accent, WHITE, 0.12),
                        mix(accent, BLACK, 0.10),
                    ],
                    bg_active: grad([mix(accent, BLACK, 0.16), mix(accent, WHITE, 0.06)]),
                    shadow: vec![
                        shade(true, 0.0, 1.0, 0.0, 0.0, WHITE.multiply_alpha(0.45)),
                        shade(true, 0.0, -1.0, 0.0, 0.0, rim),
                        shade(false, 0.0, 1.0, 2.0, 0.0, theme.lo_strong),
                        shade(false, 2.0, 3.0, 6.0, 0.0, theme.lo_soft),
                        shade(false, 1.0, 3.0, 10.0, 0.0, glow(0.28)),
                        shade(false, -1.0, -1.0, 2.0, 0.0, theme.hi),
                    ],
                    shadow_hover: vec![
                        shade(true, 0.0, 1.0, 0.0, 0.0, WHITE.multiply_alpha(0.5)),
                        shade(true, 0.0, -1.0, 0.0, 0.0, rim),
                        shade(false, 0.0, 1.0, 2.0, 0.0, theme.lo_strong),
                        shade(false, 3.0, 5.0, 10.0, 0.0, theme.lo_soft),
                        shade(false, 2.0, 5.0, 14.0, 0.0, glow(0.34)),
                        shade(false, -2.0, -2.0, 4.0, 0.0, theme.hi),
                    ],
                    shadow_active: vec![
                        shade(true, 2.0, 2.0, 5.0, 0.0, mix(accent_2, BLACK, 0.4)),
                        shade(true, -1.0, -1.0, 2.0, 0.0, mix(accent, WHITE, 0.4)),
                    ],
                    fg: theme.on_accent,
                    fg_active: theme.on_accent,
                }
            }
            Variant::Ghost => Palette {
                bg: [CLEAR; 3],
                bg_hover: [theme.surface_sunken.multiply_alpha(0.7); 3],
                bg_active: sunken,
                shadow: Vec::new(),
                shadow_hover: Vec::new(),
                shadow_active: theme.control_shadow_pressed.to_vec(),
                fg,
                fg_active: accent_2,
            },
            Variant::Soft => {
                let tint = if colored { accent } else { theme.text_3 };
                let on = |t: f32| [mix(theme.bg, tint, t); 3];
                Palette {
                    bg: on(0.15),
                    bg_hover: on(0.22),
                    bg_active: on(0.28),
                    shadow: Vec::new(),
                    shadow_hover: Vec::new(),
                    shadow_active: vec![shade(true, 0.0, 1.0, 2.0, 0.0, theme.lo_soft)],
                    fg,
                    fg_active: accent_2,
                }
            }
            Variant::Outline => {
                let tint = if colored { accent } else { theme.text_3 };
                let edge = shade(true, 0.0, 0.0, 0.0, 1.0, tint.multiply_alpha(0.55));
                Palette {
                    bg: [CLEAR; 3],
                    bg_hover: [tint.multiply_alpha(0.09); 3],
                    bg_active: [tint.multiply_alpha(0.15); 3],
                    shadow: vec![edge],
                    shadow_hover: vec![edge],
                    shadow_active: vec![edge, shade(true, 0.0, 1.0, 2.0, 0.0, theme.lo_soft)],
                    fg,
                    fg_active: accent_2,
                }
            }
        }
    }
}

impl Widget for Button {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn mount(&mut self, cx: &mut MountCx<'_>) {
        let (loading, disabled) = (self.loading.clone(), self.disabled.clone());
        let pressed = self.pressed.clone();
        let label = self.label.signal();
        let name = self.accessible_name.as_ref().map(TextProp::signal);
        self.watch = Some(cx.watch(self.id, move || {
            loading.track();
            disabled.track();
            if let Some(p) = &pressed {
                p.track();
            }
            label.track();
            if let Some(n) = &name {
                n.track();
            }
        }));
    }

    /// Picks up `loading` (it changes the width), `pressed`, `disabled`, and
    /// repaints after events
    fn update(&mut self, cx: &mut LayoutCx<'_>) -> Change {
        let mut change = Change::NONE;
        let loading = self.loading.get_untracked();
        if loading != self.shown_loading {
            if loading {
                self.spin_start = cx.now;
            }
            self.shown_loading = loading;
            self.sync(cx.now, cx.theme);
            change = Change::LAYOUT;
        }
        let pressed = self
            .pressed
            .as_ref()
            .is_some_and(GetUntracked::get_untracked);
        let disabled = self.disabled.get_untracked();
        if (pressed, disabled) != (self.shown_pressed, self.shown_disabled) {
            (self.shown_pressed, self.shown_disabled) = (pressed, disabled);
            if disabled {
                (self.pointer_armed, self.key_armed) = (false, false);
            }
            self.sync(cx.now, cx.theme);
            self.repaint = true;
        }
        let restyled = self.style.is_some_and(|s| s != self.text_style(cx.theme));
        if self.label.refresh() || restyled {
            // another text (another language) or size: shape it again; the width may change
            self.layout = None;
            change = Change::LAYOUT;
        }
        if self.accessible_name.as_mut().is_some_and(TextProp::refresh) {
            change |= Change::PAINT;
        }
        if std::mem::take(&mut self.repaint) {
            change |= Change::PAINT;
        }
        change
    }

    /// Inline padding + [spinner or icon + gap] + label [+ gap + icon]; at least
    /// as high as its size, and square when icon-only.
    /// Never narrower: a button doesn't wrap or shrink.
    fn measure(&mut self, cx: &mut LayoutCx<'_>, _known: Known, _available: Available) -> Size {
        let (h, px, font, _) = dims(cx.theme, self.size);
        let h = self.height.unwrap_or(h);
        let style = self.text_style(cx.theme);
        self.style = Some(style);
        let layout = self
            .layout
            .get_or_insert_with(|| cx.text.line(&self.label.shown, &style));
        if self.icon_only {
            return Size::new(h, h);
        }
        let (label_w, label_h) = (f64::from(layout.width()), f64::from(layout.height()));
        let items = self.items(f64::from(font), label_w);
        let gap = cx.theme.metrics.gap;
        let content =
            items.iter().map(|(_, w)| w).sum::<f64>() + gap * items.len().saturating_sub(1) as f64;
        Size::new(content + 2.0 * px, h.max(label_h))
    }

    fn style(&self, _theme: &Theme) -> taffy::Style {
        taffy::Style {
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn set_bounds(&mut self, _cx: &mut LayoutCx<'_>, bounds: Rect) {
        self.bounds = bounds;
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    /// The farthest shadow is the primary hover glow, `2px 5px 14px` (σ = 7,
    /// cut off at 2.5 σ): 15.5 px left, 12.5 up, 19.5 right, 22.5 down
    fn ink(&self) -> Rect {
        let b = self.bounds;
        Rect::new(b.x0 - 16.0, b.y0 - 13.0, b.x1 + 20.0, b.y1 + 23.0)
    }

    fn hit(&self, p: Point) -> bool {
        self.hit_point(p)
    }

    fn focusable(&self) -> bool {
        !self.shown_disabled
    }

    fn tab_stop(&self) -> bool {
        !self.shown_disabled && !self.skip_tab
    }

    fn shortcuts(&self) -> &[Shortcut] {
        if self.shown_disabled {
            &[]
        } else {
            &self.shortcuts
        }
    }

    fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) {
        // Space and Enter press it; other keys go on to the ancestors
        if let Event::KeyDown { key, .. } | Event::KeyUp { key } = event
            && (is_space(key) || matches!(key, Key::Named(NamedKey::Enter)))
        {
            cx.handled = true;
        }
        match event {
            Event::PointerEnter => self.hovered = true,
            Event::PointerLeave => self.hovered = false,
            Event::PointerMove(p) => self.hovered = self.hit_point(*p),
            Event::PointerDown(p) => {
                self.pointer_armed = !self.shown_disabled && self.hit_point(*p);
            }
            Event::PointerUp(p) => {
                let click = self.pointer_armed && self.hit_point(*p);
                self.pointer_armed = false;
                if click {
                    self.activate();
                }
            }
            Event::KeyDown {
                key, repeat: false, ..
            } if is_space(key) && !self.shown_disabled => self.key_armed = true,
            Event::KeyUp { key } if is_space(key) && self.key_armed => {
                self.key_armed = false;
                self.activate();
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Enter),
                repeat: false,
                ..
            } => self.activate(),
            Event::Focus { focused, visible } => {
                self.focused = *focused;
                self.focus_visible = *visible;
                if !focused {
                    self.key_armed = false;
                }
            }
            Event::Activate | Event::Shortcut(_) => self.activate(),
            _ => {}
        }
        self.sync(cx.now, cx.theme);
    }

    fn cursor(&self) -> CursorIcon {
        match (self.hovered, self.shown_loading, self.shown_disabled) {
            (true, _, true) => CursorIcon::NotAllowed,
            (true, true, false) => CursorIcon::Progress,
            (true, false, false) => CursorIcon::Pointer,
            _ => CursorIcon::Default,
        }
    }

    fn animating(&self, now: Instant, theme: &Theme) -> bool {
        let d = theme.metrics.duration;
        self.shown_loading
            || self.hover.animating(now, d)
            || self.press.animating(now, d)
            || self.ring.animating(now, d)
    }

    fn accessibility(&self) -> Option<accesskit::Node> {
        let mut node = accesskit::Node::new(accesskit::Role::Button);
        let name = self.accessible_name.as_ref().unwrap_or(&self.label);
        node.set_label(name.shown.clone());
        node.set_bounds(accessible_rect(self.bounds));
        if self.shown_disabled {
            node.set_disabled();
        } else {
            node.add_action(accesskit::Action::Click);
            node.add_action(accesskit::Action::Focus);
        }
        if self.pressed.is_some() {
            node.set_toggled(if self.shown_pressed {
                accesskit::Toggled::True
            } else {
                accesskit::Toggled::False
            });
        }
        if self.shown_loading {
            node.set_busy();
        }
        if let Some(s) = self.shortcuts.first() {
            node.set_keyboard_shortcut(s.label());
        }
        Some(node)
    }

    fn paint(&self, cx: &mut PaintCx<'_>) {
        let (ctx, theme, now) = (&mut *cx.ctx, cx.theme, cx.now);
        let m = &theme.metrics;
        let (h, p) = (
            self.hover.value(now, m.duration),
            self.press.value(now, m.duration),
        );
        let r = self.bounds;
        let radius = self.radius(theme);
        let shape = self.shape(theme).to_path(TOLERANCE);
        let mut pal = self.palette(theme);
        // disabled: faded, the surface's tone with a faint edge, muted text
        let alpha: f32 = if self.shown_disabled { 0.6 } else { 1.0 };
        if self.shown_disabled && self.segment.is_none() {
            let flat = matches!(
                self.variant,
                Variant::Ghost | Variant::Soft | Variant::Outline
            );
            pal.bg = [if flat { CLEAR } else { theme.bg }; 3];
            pal.shadow = if self.variant == Variant::Ghost {
                Vec::new()
            } else {
                vec![shade(false, 0.0, 0.0, 0.0, 1.0, theme.lo_soft)]
            };
            pal.shadow_hover = pal.shadow.clone();
            pal.fg = theme.text_3;
        }

        // focus ring: the last (lowest) of the button's box shadows; in a
        // group an inner ring, as its neighbours touch it
        let ring = self.ring.value(now, m.duration) as f32;
        if self.segment.is_some() {
            if ring > 0.0 {
                ctx.set_stroke(Stroke::new(2.0));
                ctx.set_paint(theme.accent.multiply_alpha(0.55 * ring));
                ctx.stroke_path(
                    &RoundedRect::from_rect(r.inset(-1.0), radius - 1.0).to_path(TOLERANCE),
                );
            }
        } else {
            paint::outer_shadows(ctx, r, radius, &[theme.focus_ring], ring);
        }

        // rest ↔ hover move with the hover transition (shadow lists of the same
        // length), the pressed set fades in as the raised one fades out
        let raised: Vec<Shadow> = pal
            .shadow
            .iter()
            .zip(&pal.shadow_hover)
            .map(|(a, b)| paint::lerp_shadows(&[*a], &[*b], h)[0])
            .collect();
        let (up, down) = ((1.0 - p) as f32, p as f32);
        paint::outer_shadows(ctx, r, radius, &raised, up * alpha);
        let stops: [Color; 3] = std::array::from_fn(|i| {
            let c = pal.bg[i].lerp_rect(pal.bg_hover[i], h as f32);
            mix(c, pal.bg_active[i], down).multiply_alpha(alpha)
        });
        if stops.iter().any(|c| c.components[3] > 0.002) {
            ctx.set_paint(
                Gradient::new_linear((r.x0, r.y0), (r.x0, r.y1)).with_stops([
                    (0.0, stops[0]),
                    (0.45, stops[1]),
                    (1.0, stops[2]),
                ]),
            );
            ctx.fill_path(&shape);
        }
        paint::inset_shadows(ctx, r, radius, &raised, up * alpha);
        paint::inset_shadows(ctx, r, radius, &pal.shadow_active, down * alpha);

        // content: [spinner or icon] label [icon], centred; pressed it sinks a little
        let Some(layout) = &self.layout else { return };
        let em = f64::from(dims(theme, self.size).2);
        let items = self.items(em, f64::from(layout.width()));
        let width = items.iter().map(|(_, w)| w).sum::<f64>()
            + m.gap * items.len().saturating_sub(1) as f64;
        let centre = r.center();
        let base = *ctx.transform();
        let sink = Affine::translate(centre.to_vec2() + Vec2::new(0.0, 0.5 * p))
            * Affine::scale(1.0 - 0.015 * p)
            * Affine::translate(-centre.to_vec2());
        ctx.set_transform(base * sink);

        let fg = mix(pal.fg, pal.fg_active, down).multiply_alpha(alpha);
        let mut x = centre.x - width / 2.0;
        for (item, w) in items {
            match item {
                Item::Spinner => {
                    let c = Point::new(x + w / 2.0, centre.y);
                    let turn =
                        (now.duration_since(self.spin_start).as_secs_f64() / SPIN_PERIOD).fract();
                    let size = if self.icon_only { w } else { SPINNER_EM * em };
                    spinner(ctx, c, size, turn, fg);
                }
                Item::Icon(icon) => {
                    let square = Rect::from_center_size((x + w / 2.0, centre.y), (w, w));
                    icon.draw(ctx, square, STROKE, fg, 0.0, 1.0);
                }
                Item::Label => {
                    let y = centre.y - f64::from(layout.height()) / 2.0;
                    text::draw(ctx, cx.res, layout, Point::new(x, y), fg);
                }
            }
            x += w + m.gap;
        }
        ctx.set_transform(base);
    }
}

/// A group of buttons: attached into one raised body with hairlines between
/// them (only its outer corners round), or spaced apart
pub struct ButtonGroup {
    id: WidgetId,
    children: Vec<Box<dyn Widget>>,
    attached: bool,
    vertical: bool,
    size: ControlSize,
    label: Option<TextProp>,
    watch: Option<Watch>,
    bounds: Rect,
}

impl ButtonGroup {
    /// Attached: one body, segments edge to edge
    pub fn attached() -> Self {
        Self {
            id: WidgetId::next(),
            children: Vec::new(),
            attached: true,
            vertical: false,
            size: ControlSize::Md,
            label: None,
            watch: None,
            bounds: Rect::ZERO,
        }
    }

    /// Each button raised on its own, a gap between them
    pub fn spaced() -> Self {
        Self {
            attached: false,
            ..Self::attached()
        }
    }

    /// Below each other
    pub fn vertical(mut self) -> Self {
        self.vertical = true;
        self.mark_segments();
        self
    }

    /// The size of its buttons, and of its corners
    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    /// What the buttons are about, for assistive technology ("Text alignment")
    pub fn label(mut self, label: impl IntoText) -> Self {
        self.label = Some(TextProp::new(label));
        self
    }

    pub fn button(mut self, button: Button) -> Self {
        self.children.push(Box::new(button.size(self.size)));
        self.mark_segments();
        self
    }

    fn mark_segments(&mut self) {
        let n = self.children.len();
        for (i, child) in self.children.iter_mut().enumerate() {
            if let Some(b) = child.as_any_mut().downcast_mut::<Button>() {
                b.segment = self.attached.then_some(Segment {
                    first: i == 0,
                    last: i + 1 == n,
                    vertical: self.vertical,
                });
            }
        }
    }
}

impl Widget for ButtonGroup {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut [Box<dyn Widget>] {
        &mut self.children
    }

    fn mount(&mut self, cx: &mut MountCx<'_>) {
        if let Some(label) = &self.label {
            let label = label.signal();
            self.watch = Some(cx.watch(self.id, move || label.track()));
        }
    }

    fn update(&mut self, _cx: &mut LayoutCx<'_>) -> Change {
        if self.label.as_mut().is_some_and(TextProp::refresh) {
            Change::PAINT
        } else {
            Change::NONE
        }
    }

    fn style(&self, _theme: &Theme) -> taffy::Style {
        let gap = match (self.attached, self.size) {
            (true, _) => 0.0,
            (false, ControlSize::Lg | ControlSize::Xl) => space(3),
            (false, _) => space(2),
        };
        let gap = taffy::LengthPercentage::length(gap as f32);
        taffy::Style {
            display: taffy::Display::Flex,
            flex_direction: if self.vertical {
                taffy::FlexDirection::Column
            } else {
                taffy::FlexDirection::Row
            },
            align_items: taffy::AlignItems::STRETCH,
            align_self: Some(taffy::AlignSelf::START),
            gap: taffy::Size {
                width: gap,
                height: gap,
            },
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn set_bounds(&mut self, _cx: &mut LayoutCx<'_>, bounds: Rect) {
        self.bounds = bounds;
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn ink(&self) -> Rect {
        let b = self.bounds;
        Rect::new(b.x0 - 10.0, b.y0 - 8.0, b.x1 + 16.0, b.y1 + 18.0)
    }

    fn accessibility(&self) -> Option<accesskit::Node> {
        let mut node = accesskit::Node::new(accesskit::Role::Group);
        if let Some(l) = self.label.as_ref().filter(|l| !l.shown.is_empty()) {
            node.set_label(l.shown.clone());
        }
        node.set_bounds(accessible_rect(self.bounds));
        Some(node)
    }

    /// Attached: the body, its segments on it, hairlines between them
    fn paint(&self, cx: &mut PaintCx<'_>) {
        if self.attached && !self.children.is_empty() {
            let (ctx, theme) = (&mut *cx.ctx, cx.theme);
            let (r, radius) = (self.bounds, dims(theme, self.size).3);
            paint::outer_shadows(ctx, r, radius, &theme.control_shadow, 1.0);
            let [top, bottom] = theme.control_bg;
            ctx.set_paint(
                Gradient::new_linear((r.x0, r.y0), (r.x0, r.y1)).with_stops([top, bottom]),
            );
            ctx.fill_path(&paint::rounded(r, radius));
            paint::inset_shadows(ctx, r, radius, &theme.control_shadow, 1.0);
        }
        cx.paint_children(&self.children);
        if self.attached {
            let ctx = &mut *cx.ctx;
            ctx.set_paint(cx.theme.hairline);
            for c in self.children.iter().skip(1) {
                let b = c.bounds();
                let line = if self.vertical {
                    Rect::new(b.x0, b.y0, b.x1, b.y0 + 1.0)
                } else {
                    Rect::new(b.x0, b.y0, b.x0 + 1.0, b.y1)
                };
                ctx.fill_rect(&line);
            }
        }
    }
}

/// One piece of a button's content
#[derive(Clone, Copy)]
enum Item {
    Spinner,
    Icon(IconNode),
    Label,
}

/// winit reports Space as the character " ", not as a named key
fn is_space(key: &Key) -> bool {
    matches!(key, Key::Character(c) if c == " ")
}

/// Kiesel's Spinner: a 24-unit viewBox, circle r = 9, stroke 3; a faint track
/// and an 18-unit arc with round caps, turning by `turn` (0..1)
fn spinner(ctx: &mut RenderContext, centre: Point, size: f64, turn: f64, color: Color) {
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

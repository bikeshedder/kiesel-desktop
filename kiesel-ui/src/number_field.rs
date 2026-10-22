//! `NumberField` – a number in a text field, with −/+ steppers in the well
//! (see `kiesel/src/components/NumberField`).
//!
//! The value is `Option<f64>` (empty: `None`), a signal both ways. Typing
//! sets it as soon as the text is a number; Up and Down step (Shift: ten
//! steps); the steppers step once on a press and repeat while held, faster
//! the longer – they take no focus, the field's own keys do the same. Leaving
//! the field clamps the value to `min`/`max`. Numbers are written the
//! language's way (a decimal comma in German). For assistive technology it is
//! a spin button: value, limits and step, Increment and Decrement.

use std::rc::Rc;

use reactive_graph::signal::ArcRwSignal;
use reactive_graph::traits::{Get, GetUntracked, Set, Track};
use reactive_graph::wrappers::read::ArcSignal;
use vello_cpu::kurbo::Rect;
use winit::keyboard::{Key, NamedKey};

use crate::button::{Button, ButtonGroup};
use crate::field::FieldLink;
use crate::i18n::{self, IntoText, TextProp, t};
use crate::icon::Tone;
use crate::label::Label;
use crate::lucide;
use crate::reactive::{MountCx, Watch};
use crate::shortcut::KeyPress;
use crate::text_field::{Spin, TextField};
use crate::theme::{ControlSize, Theme};
use crate::widget::{Change, LayoutCx, Widget, WidgetId};

/// Writes a number the current language's way: no grouping, up to six decimals
pub fn format(n: f64) -> String {
    format_in(&i18n::current(), n)
}

fn format_in(lang: &str, n: f64) -> String {
    let mut s = format!("{:.6}", n);
    if s.contains('.') {
        s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    if s == "-0" {
        s = "0".into();
    }
    if lang.starts_with("de") {
        s = s.replace('.', ",");
    }
    s.replace('-', "−")
}

/// Reads a number written the current language's way; `None` if it isn't one
pub fn parse(text: &str) -> Option<f64> {
    parse_in(&i18n::current(), text)
}

fn parse_in(lang: &str, text: &str) -> Option<f64> {
    let t: String = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| if c == '−' { '-' } else { c })
        .collect();
    let t = if lang.starts_with("de") {
        t.replace('.', "").replace(',', ".")
    } else {
        t.replace(',', "")
    };
    t.parse::<f64>().ok().filter(|n| n.is_finite())
}

/// The rules a number field keeps
#[derive(Clone, Copy)]
struct Rules {
    min: Option<f64>,
    max: Option<f64>,
    step: f64,
}

impl Rules {
    fn clamp(&self, n: f64) -> f64 {
        n.max(self.min.unwrap_or(f64::NEG_INFINITY))
            .min(self.max.unwrap_or(f64::INFINITY))
    }

    /// To the step's decimals: 0.1 + 0.2 is 0.3
    fn round(&self, n: f64) -> f64 {
        let decimals = format!("{}", self.step)
            .split('.')
            .nth(1)
            .map_or(0, str::len)
            .min(9) as i32;
        let k = 10f64.powi(decimals);
        (n * k).round() / k
    }

    /// From `value` (or `min`, or 0) by `steps` steps, rounded and clamped
    fn stepped(&self, value: Option<f64>, steps: f64) -> f64 {
        let base = value.or(self.min).unwrap_or(0.0);
        self.clamp(self.round(base + steps * self.step))
    }
}

pub struct NumberField {
    id: WidgetId,
    value: ArcRwSignal<Option<f64>>,
    rules: Rules,
    unit: Option<TextProp>,
    placeholder: Option<TextProp>,
    label: Option<TextProp>,
    grow: bool,
    field: Option<FieldLink>,
    /// The text field, made when mounted
    children: Vec<Box<dyn Widget>>,
    /// What the field shows: the value written out, or what is being typed
    text: ArcRwSignal<String>,
    watch: Option<Watch>,
    bounds: Rect,
}

impl NumberField {
    /// Shows `value` (`None`: empty) and sets it as you type and step
    pub fn new(value: ArcRwSignal<Option<f64>>) -> Self {
        Self {
            id: WidgetId::next(),
            text: ArcRwSignal::new(value.get_untracked().map(format).unwrap_or_default()),
            value,
            rules: Rules {
                min: None,
                max: None,
                step: 1.0,
            },
            unit: None,
            placeholder: None,
            label: None,
            grow: false,
            field: None,
            children: Vec::new(),
            watch: None,
            bounds: Rect::ZERO,
        }
    }

    pub fn min(mut self, min: f64) -> Self {
        self.rules.min = Some(min);
        self
    }

    pub fn max(mut self, max: f64) -> Self {
        self.rules.max = Some(max);
        self
    }

    /// What −/+ and Up/Down add (default 1); its decimals are the value's
    pub fn step(mut self, step: f64) -> Self {
        self.rules.step = step.abs().max(f64::EPSILON);
        self
    }

    /// A unit after the number: "px", "%", "kg"
    pub fn unit(mut self, unit: impl IntoText) -> Self {
        self.unit = Some(TextProp::new(unit));
        self
    }

    pub fn placeholder(mut self, text: impl IntoText) -> Self {
        self.placeholder = Some(TextProp::new(text));
        self
    }

    /// Its accessible name without a visible label; otherwise put it in a `Field`
    pub fn label(mut self, text: impl IntoText) -> Self {
        self.label = Some(TextProp::new(text));
        self
    }

    /// Take the free space along the parent's row
    pub fn grow(mut self) -> Self {
        self.grow = true;
        self
    }

    fn build(&mut self) {
        let (value, text, rules) = (self.value.clone(), self.text.clone(), self.rules);
        // step the value; the text follows
        let step: Rc<dyn Fn(f64)> = {
            let (value, text) = (value.clone(), text.clone());
            Rc::new(move |steps| {
                let next = rules.stepped(value.get_untracked(), steps);
                if Some(next) != value.get_untracked() {
                    value.set(Some(next));
                }
                text.set(format(next));
            })
        };

        let mut field = TextField::new().value(text.clone());
        field.align_end = true;
        if self.grow {
            field = field.grow();
        }
        if let Some(p) = self.placeholder.take() {
            field = field.placeholder(p.signal());
        }
        if let Some(l) = self.label.take() {
            field = field.label(l.signal());
        }
        field.on_input = Some(Box::new({
            let value = value.clone();
            move |typed: &str| {
                // a number, or nothing at all, is the value; "1," on the way to "1,5" is neither
                let n = parse(typed);
                if n.is_some() || typed.trim().is_empty() {
                    value.set(n);
                }
            }
        }));
        field.on_key = Some(Box::new({
            let step = step.clone();
            move |press: &KeyPress| {
                let dir = match press.key {
                    Key::Named(NamedKey::ArrowUp) => 1.0,
                    Key::Named(NamedKey::ArrowDown) => -1.0,
                    _ => return false,
                };
                let ten = if press.mods.shift_key() { 10.0 } else { 1.0 };
                step(dir * ten);
                true
            }
        }));
        field.on_step = Some(Box::new({
            let step = step.clone();
            move |dir| step(f64::from(dir))
        }));
        field.on_blur = Some(Box::new({
            let (value, text) = (value.clone(), text.clone());
            move || {
                if let Some(v) = value.get_untracked() {
                    let c = rules.clamp(v);
                    if c != v {
                        value.set(Some(c));
                    }
                    text.set(format(c));
                }
            }
        }));
        field.spin = Some(Box::new({
            let value = value.clone();
            move || Spin {
                value: value.get_untracked(),
                min: rules.min,
                max: rules.max,
                step: rules.step,
            }
        }));
        if let Some(link) = self.field.take() {
            field.link_field(link);
        }

        if let Some(unit) = self.unit.take() {
            field.trailing.push(Box::new(
                Label::new(unit.signal()).tone(Tone::Text(crate::theme::TextRole::Caption)),
            ));
        }
        let at = |limit: Option<f64>, beyond: fn(f64, f64) -> bool| {
            let value = value.clone();
            ArcSignal::derive(
                move || matches!((value.get(), limit), (Some(v), Some(l)) if beyond(v, l)),
            )
        };
        let stepper = |name: &'static str, icon, dir: f64, disabled: ArcSignal<bool>| {
            let step = step.clone();
            let mut b = Button::new(t(name))
                .icon(icon)
                .icon_only()
                .disabled(disabled)
                .on_click(move || step(dir));
            (b.unfocusable, b.repeat) = (true, true);
            b
        };
        // −/+ in the well, 4 px in from its edge: Sm buttons are the field's height less 8 px
        let steppers = ButtonGroup::attached()
            .size(ControlSize::Sm)
            .button(stepper(
                "kiesel.numberField.decrement",
                lucide::MINUS,
                -1.0,
                at(rules.min, |v, l| v <= l),
            ))
            .button(stepper(
                "kiesel.numberField.increment",
                lucide::PLUS,
                1.0,
                at(rules.max, |v, l| v >= l),
            ));
        field.trailing.push(Box::new(steppers));
        self.children = vec![Box::new(field)];
    }
}

impl Widget for NumberField {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut [Box<dyn Widget>] {
        &mut self.children
    }

    /// Its text field is laid out in its place
    fn layout_transparent(&self) -> bool {
        true
    }

    fn link_field(&mut self, link: FieldLink) {
        self.field = Some(link);
    }

    fn mount(&mut self, cx: &mut MountCx<'_>) {
        if self.children.is_empty() {
            self.build();
        }
        // the value, and the language: another language writes numbers anew
        let (value, language) = (self.value.clone(), i18n::language());
        self.watch = Some(cx.watch(self.id, move || {
            value.track();
            language.track();
        }));
    }

    /// A value set from outside: the text shows it (unless it already does –
    /// "1," while typing "1,5" stays)
    fn update(&mut self, _cx: &mut LayoutCx<'_>) -> Change {
        let v = self.value.get_untracked();
        let shown = parse(&self.text.get_untracked());
        if shown != v {
            self.text.set(v.map(format).unwrap_or_default());
        }
        Change::NONE
    }

    fn style(&self, _theme: &Theme) -> taffy::Style {
        taffy::Style::default()
    }

    fn set_bounds(&mut self, _cx: &mut LayoutCx<'_>, bounds: Rect) {
        self.bounds = bounds;
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_written_and_read_the_languages_way() {
        assert_eq!(format_in("en", 1.5), "1.5");
        assert_eq!(format_in("de", 1.5), "1,5");
        assert_eq!(format_in("en", -2.0), "−2");
        assert_eq!(format_in("en", 0.1 + 0.2), "0.3");
        assert_eq!(parse_in("de", "1.234,5"), Some(1234.5));
        assert_eq!(parse_in("en", "1,234.5"), Some(1234.5));
        assert_eq!(parse_in("en", "−3"), Some(-3.0));
        assert_eq!(parse_in("en", "1.5x"), None);
        assert_eq!(parse_in("en", ""), None);
    }

    #[test]
    fn steps_round_to_the_step_and_stay_within_the_limits() {
        let rules = Rules {
            min: Some(0.0),
            max: Some(1.0),
            step: 0.1,
        };
        assert_eq!(rules.stepped(Some(0.2), 1.0), 0.3);
        assert_eq!(rules.stepped(Some(0.95), 1.0), 1.0);
        assert_eq!(rules.stepped(None, 1.0), 0.1, "from min");
        assert_eq!(rules.stepped(Some(0.3), -10.0), 0.0);
    }
}

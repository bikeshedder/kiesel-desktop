//! Keyboard shortcuts: one keymap with platform modifiers. A shortcut is written
//! once with `Primary` (Ctrl, ⌘ on macOS) and matched against key presses on any
//! keyboard layout; its label reads like the platform's ("Ctrl+Shift+S", "⇧⌘S").
//!
//! Who gets a shortcut: the focused widget and its ancestors first (a text field
//! keeps Ctrl+A), then widgets that registered it ([`Widget::shortcuts`]), the
//! topmost popup's before the window's, then the app's ([`Shortcuts`] in
//! `Options`).
//!
//! [`Widget::shortcuts`]: crate::Widget::shortcuts

use std::fmt;

use reactive_graph::traits::Get;
use reactive_graph::wrappers::read::ArcSignal;
#[cfg(test)]
use winit::keyboard::KeyCode;
use winit::keyboard::{Key, ModifiersState, NamedKey, PhysicalKey};

use crate::i18n;

const MAC: bool = cfg!(target_os = "macos");

/// The modifier keys of a shortcut, in platform terms
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Mods(u8);

impl Mods {
    pub const NONE: Mods = Mods(0);
    /// Ctrl; ⌘ (Command) on macOS – what menus and editing shortcuts use
    pub const PRIMARY: Mods = Mods(1);
    pub const SHIFT: Mods = Mods(2);
    /// Alt; ⌥ (Option) on macOS
    pub const ALT: Mods = Mods(4);
    /// The Control key ⌃ on macOS; elsewhere the same as [`Mods::PRIMARY`]
    pub const CONTROL: Mods = Mods(8);

    pub const fn union(self, other: Mods) -> Mods {
        Mods(self.0 | other.0)
    }

    pub const fn contains(self, other: Mods) -> bool {
        self.0 & other.0 == other.0
    }

    /// The modifiers held, the way shortcuts name them
    pub fn held(state: ModifiersState) -> Mods {
        Self::from_state(state, MAC)
    }

    fn from_state(state: ModifiersState, mac: bool) -> Mods {
        let mut m = Mods::NONE;
        if state.shift_key() {
            m = m | Mods::SHIFT;
        }
        if state.alt_key() {
            m = m | Mods::ALT;
        }
        if mac {
            if state.meta_key() {
                m = m | Mods::PRIMARY;
            }
            if state.control_key() {
                m = m | Mods::CONTROL;
            }
        } else if state.control_key() {
            m = m | Mods::PRIMARY;
        }
        m
    }

    /// Off macOS, Control is Primary
    fn normal(self, mac: bool) -> Mods {
        if !mac && self.contains(Mods::CONTROL) {
            Mods(self.0 & !Mods::CONTROL.0) | Mods::PRIMARY
        } else {
            self
        }
    }
}

impl std::ops::BitOr for Mods {
    type Output = Mods;
    fn bitor(self, rhs: Mods) -> Mods {
        self.union(rhs)
    }
}

/// The key of a shortcut
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Trigger {
    /// A character key, lower case: `'s'`, `'+'`, `'1'`
    Char(char),
    /// Enter, Escape, F2, Delete, arrows…
    Named(NamedKey),
}

/// A key with modifiers: `Shortcut::primary('s')`, `"Primary+Shift+Z".parse()`
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Shortcut {
    pub mods: Mods,
    pub key: Trigger,
}

/// A key press, as shortcuts see it
#[derive(Clone, Debug, PartialEq)]
pub struct KeyPress {
    /// With the layout and Shift applied ("Z", "+")
    pub key: Key,
    /// The same key without modifiers ("z", "=")
    pub plain: Key,
    /// Where it is on the keyboard, for layouts without Latin letters
    pub code: PhysicalKey,
    pub mods: ModifiersState,
}

impl Shortcut {
    pub fn new(mods: Mods, key: Trigger) -> Self {
        let key = match key {
            Trigger::Char(c) => Trigger::Char(lower(c)),
            k => k,
        };
        Self { mods, key }
    }

    /// Ctrl+`c` (⌘ on macOS)
    pub fn primary(c: char) -> Self {
        Self::new(Mods::PRIMARY, Trigger::Char(c))
    }

    /// A key on its own: `Shortcut::key(NamedKey::F2)`
    pub fn key(key: NamedKey) -> Self {
        Self::new(Mods::NONE, Trigger::Named(key))
    }

    pub fn with(mut self, mods: Mods) -> Self {
        self.mods = self.mods | mods;
        self
    }

    pub fn copy() -> Self {
        Self::primary('c')
    }
    pub fn cut() -> Self {
        Self::primary('x')
    }
    pub fn paste() -> Self {
        Self::primary('v')
    }
    pub fn select_all() -> Self {
        Self::primary('a')
    }
    pub fn undo() -> Self {
        Self::primary('z')
    }
    pub fn redo() -> Self {
        Self::primary('z').with(Mods::SHIFT)
    }

    /// Whether this key press is the shortcut
    pub fn matches(&self, press: &KeyPress) -> bool {
        self.matches_on(press, MAC)
    }

    fn matches_on(&self, press: &KeyPress, mac: bool) -> bool {
        let held = Mods::from_state(press.mods, mac);
        let want = self.mods.normal(mac);
        match &self.key {
            Trigger::Named(n) => held == want && matches!(&press.key, Key::Named(k) if k == n),
            Trigger::Char(c) => {
                // the key itself, without Shift and Option ("z" for ⇧⌘Z)
                if held == want && char_of(&press.plain).is_some_and(|p| lower(p) == *c) {
                    return true;
                }
                // a character typed with Shift ("+" on a US keyboard is Shift+=)
                if !c.is_alphabetic()
                    && held == want | Mods::SHIFT
                    && char_of(&press.key) == Some(*c)
                {
                    return true;
                }
                // Cyrillic, Greek…: Ctrl+C is where C is on a US keyboard
                held == want
                    && char_of(&press.plain).is_some_and(|p| !p.is_ascii())
                    && us_char(press.code) == Some(*c)
            }
        }
    }

    /// The platform's way of writing it, in the current language
    pub fn label(&self) -> String {
        self.label_on(&i18n::current(), MAC)
    }

    /// The label, following the language: for `Kbd::new` or a menu item
    pub fn text(&self) -> ArcSignal<String> {
        let (me, language) = (self.clone(), i18n::language());
        ArcSignal::derive(move || me.label_on(&language.get(), MAC))
    }

    fn label_on(&self, lang: &str, mac: bool) -> String {
        let m = self.mods.normal(mac);
        let name = |key: &str| i18n::format(lang, key, &[]);
        let key = match &self.key {
            Trigger::Char(' ') => name("kiesel.keys.space"),
            Trigger::Char(c) => c.to_uppercase().collect(),
            Trigger::Named(n) => named_label(n, mac).map_or_else(
                || {
                    named_key(n)
                        .map(|k| name(&format!("kiesel.keys.{k}")))
                        .unwrap_or_else(|| format!("{n:?}"))
                },
                str::to_owned,
            ),
        };
        if mac {
            // Apple's order: ⌃⌥⇧⌘, then the key, without separators
            let mut s = String::new();
            for (bit, sym) in [
                (Mods::CONTROL, "⌃"),
                (Mods::ALT, "⌥"),
                (Mods::SHIFT, "⇧"),
                (Mods::PRIMARY, "⌘"),
            ] {
                if m.contains(bit) {
                    s.push_str(sym);
                }
            }
            s + &key
        } else {
            let mut parts = Vec::new();
            for (bit, key) in [
                (Mods::PRIMARY, "kiesel.keys.ctrl"),
                (Mods::ALT, "kiesel.keys.alt"),
                (Mods::SHIFT, "kiesel.keys.shift"),
            ] {
                if m.contains(bit) {
                    parts.push(name(key));
                }
            }
            parts.push(key);
            parts.join("+")
        }
    }
}

impl fmt::Display for Shortcut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label())
    }
}

/// "Primary+Shift+Z", "Ctrl+S" (Primary), "Alt+F4", "F2", "Escape", "Primary++"
impl std::str::FromStr for Shortcut {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let bad = || format!("not a shortcut: {s:?}");
        // the key is after the last '+', unless the key is '+' itself
        let (mods_part, key) = match s.strip_suffix("++") {
            Some(rest) => (rest, "+"),
            None if s == "+" => ("", "+"),
            None => s.rsplit_once('+').unwrap_or(("", s)),
        };
        let mut mods = Mods::NONE;
        for m in mods_part.split('+').filter(|m| !m.is_empty()) {
            mods = mods
                | match m.to_ascii_lowercase().as_str() {
                    "primary" | "mod" | "ctrl" | "cmd" | "command" => Mods::PRIMARY,
                    "shift" => Mods::SHIFT,
                    "alt" | "option" | "opt" => Mods::ALT,
                    "control" => Mods::CONTROL,
                    _ => return Err(bad()),
                };
        }
        let mut chars = key.chars();
        let trigger = match (chars.next(), chars.next()) {
            (Some(c), None) => Trigger::Char(c),
            // winit reports Space as a character
            _ if key.eq_ignore_ascii_case("space") => Trigger::Char(' '),
            _ => Trigger::Named(parse_named(key).ok_or_else(bad)?),
        };
        Ok(Self::new(mods, trigger))
    }
}

/// Shortcuts of the app as a whole: `Shortcuts::new().on("Primary+S", save)`
#[derive(Default)]
pub struct Shortcuts {
    pub(crate) entries: Vec<(Shortcut, Box<dyn FnMut()>)>,
}

impl Shortcuts {
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs `f` on `shortcut` – a [`Shortcut`] or a string like "Primary+S"
    pub fn on(mut self, shortcut: impl IntoShortcut, f: impl FnMut() + 'static) -> Self {
        self.entries.push((shortcut.into_shortcut(), Box::new(f)));
        self
    }

    /// Runs the first matching one; true if there was one
    pub(crate) fn run(&mut self, press: &KeyPress) -> bool {
        match self.entries.iter_mut().find(|(s, _)| s.matches(press)) {
            Some((_, f)) => {
                f();
                true
            }
            None => false,
        }
    }
}

/// A [`Shortcut`], or a string to parse (panics if it isn't one – a typo in the code)
pub trait IntoShortcut {
    fn into_shortcut(self) -> Shortcut;
}

impl IntoShortcut for Shortcut {
    fn into_shortcut(self) -> Shortcut {
        self
    }
}

impl IntoShortcut for &str {
    fn into_shortcut(self) -> Shortcut {
        self.parse().unwrap_or_else(|e| panic!("{e}"))
    }
}

fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

fn char_of(key: &Key) -> Option<char> {
    match key {
        Key::Character(s) => {
            let mut chars = s.chars();
            chars.next().filter(|_| chars.next().is_none())
        }
        _ => None,
    }
}

/// The character a key has on a US keyboard (letters and digits)
fn us_char(code: PhysicalKey) -> Option<char> {
    let PhysicalKey::Code(code) = code else {
        return None;
    };
    let name = format!("{code:?}");
    let c = name
        .strip_prefix("Key")
        .or_else(|| name.strip_prefix("Digit"))?;
    let mut chars = c.chars();
    let c = chars.next().filter(|_| chars.next().is_none())?;
    Some(c.to_ascii_lowercase())
}

/// Keys written as symbols everywhere, or on macOS
fn named_label(key: &NamedKey, mac: bool) -> Option<&'static str> {
    Some(match key {
        NamedKey::ArrowLeft => "←",
        NamedKey::ArrowRight => "→",
        NamedKey::ArrowUp => "↑",
        NamedKey::ArrowDown => "↓",
        NamedKey::Enter if mac => "↩",
        NamedKey::Escape if mac => "⎋",
        NamedKey::Backspace if mac => "⌫",
        NamedKey::Delete if mac => "⌦",
        NamedKey::Tab if mac => "⇥",
        NamedKey::PageUp if mac => "⇞",
        NamedKey::PageDown if mac => "⇟",
        NamedKey::Home if mac => "↖",
        NamedKey::End if mac => "↘",
        NamedKey::F1 => "F1",
        NamedKey::F2 => "F2",
        NamedKey::F3 => "F3",
        NamedKey::F4 => "F4",
        NamedKey::F5 => "F5",
        NamedKey::F6 => "F6",
        NamedKey::F7 => "F7",
        NamedKey::F8 => "F8",
        NamedKey::F9 => "F9",
        NamedKey::F10 => "F10",
        NamedKey::F11 => "F11",
        NamedKey::F12 => "F12",
        _ => return None,
    })
}

/// The catalog key (`kiesel.keys.<name>`) of a key written as a word
fn named_key(key: &NamedKey) -> Option<&'static str> {
    Some(match key {
        NamedKey::Enter => "enter",
        NamedKey::Escape => "escape",
        NamedKey::Backspace => "backspace",
        NamedKey::Delete => "delete",
        NamedKey::Tab => "tab",
        NamedKey::PageUp => "pageUp",
        NamedKey::PageDown => "pageDown",
        NamedKey::Home => "home",
        NamedKey::End => "end",
        NamedKey::Insert => "insert",
        _ => return None,
    })
}

fn parse_named(s: &str) -> Option<NamedKey> {
    let key = match s.to_ascii_lowercase().as_str() {
        "enter" | "return" => NamedKey::Enter,
        "escape" | "esc" => NamedKey::Escape,
        "backspace" => NamedKey::Backspace,
        "delete" | "del" => NamedKey::Delete,
        "tab" => NamedKey::Tab,
        "pageup" => NamedKey::PageUp,
        "pagedown" => NamedKey::PageDown,
        "home" => NamedKey::Home,
        "end" => NamedKey::End,
        "insert" => NamedKey::Insert,
        "left" | "arrowleft" => NamedKey::ArrowLeft,
        "right" | "arrowright" => NamedKey::ArrowRight,
        "up" | "arrowup" => NamedKey::ArrowUp,
        "down" | "arrowdown" => NamedKey::ArrowDown,
        f => {
            let n: u8 = f.strip_prefix('f')?.parse().ok()?;
            [
                NamedKey::F1,
                NamedKey::F2,
                NamedKey::F3,
                NamedKey::F4,
                NamedKey::F5,
                NamedKey::F6,
                NamedKey::F7,
                NamedKey::F8,
                NamedKey::F9,
                NamedKey::F10,
                NamedKey::F11,
                NamedKey::F12,
            ]
            .get(usize::from(n).checked_sub(1)?)
            .copied()?
        }
    };
    Some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(key: &str, plain: &str, code: KeyCode, mods: ModifiersState) -> KeyPress {
        KeyPress {
            key: Key::Character(key.into()),
            plain: Key::Character(plain.into()),
            code: PhysicalKey::Code(code),
            mods,
        }
    }

    const CTRL: ModifiersState = ModifiersState::CONTROL;
    const META: ModifiersState = ModifiersState::META;

    #[test]
    fn primary_is_ctrl_or_command() {
        let s = Shortcut::primary('s');
        let ctrl_s = press("s", "s", KeyCode::KeyS, CTRL);
        let cmd_s = press("s", "s", KeyCode::KeyS, META);
        assert!(s.matches_on(&ctrl_s, false));
        assert!(!s.matches_on(&cmd_s, false));
        assert!(s.matches_on(&cmd_s, true));
        assert!(!s.matches_on(&ctrl_s, true), "Ctrl isn't ⌘ on macOS");
        assert!(!s.matches_on(
            &press("s", "s", KeyCode::KeyS, CTRL | ModifiersState::SHIFT),
            false
        ));
    }

    #[test]
    fn shift_counts_for_letters_and_makes_symbols() {
        let redo = Shortcut::redo();
        let shift = CTRL | ModifiersState::SHIFT;
        assert!(redo.matches_on(&press("Z", "z", KeyCode::KeyZ, shift), false));
        assert!(!Shortcut::undo().matches_on(&press("Z", "z", KeyCode::KeyZ, shift), false));
        // Ctrl++ on a US keyboard: Shift and =
        let zoom: Shortcut = "Primary++".parse().unwrap();
        assert!(zoom.matches_on(&press("+", "=", KeyCode::Equal, shift), false));
        // on a German keyboard + has its own key
        assert!(zoom.matches_on(&press("+", "+", KeyCode::BracketRight, CTRL), false));
    }

    #[test]
    fn other_scripts_use_the_us_positions() {
        // Ctrl+C on a Russian layout: the key says "с"
        let c = press("с", "с", KeyCode::KeyC, CTRL);
        assert!(Shortcut::copy().matches_on(&c, false));
        assert!(!Shortcut::paste().matches_on(&c, false));
        // a German layout keeps its own letters: Z is where Y is on US keyboards
        let z = press("z", "z", KeyCode::KeyY, CTRL);
        assert!(Shortcut::undo().matches_on(&z, false));
    }

    #[test]
    fn named_keys() {
        let f2: Shortcut = "F2".parse().unwrap();
        let p = KeyPress {
            key: Key::Named(NamedKey::F2),
            plain: Key::Named(NamedKey::F2),
            code: PhysicalKey::Code(KeyCode::F2),
            mods: ModifiersState::empty(),
        };
        assert!(f2.matches_on(&p, false));
        assert_eq!("Alt+F4".parse::<Shortcut>().unwrap().mods, Mods::ALT);
        assert!("Hyper+X".parse::<Shortcut>().is_err());
        assert!("Primary+Nope".parse::<Shortcut>().is_err());
    }

    #[test]
    fn labels_follow_the_platform_and_language() {
        let save_as: Shortcut = "Primary+Shift+S".parse().unwrap();
        assert_eq!(save_as.label_on("en", false), "Ctrl+Shift+S");
        assert_eq!(save_as.label_on("de", false), "Strg+Umschalt+S");
        assert_eq!(save_as.label_on("en", true), "⇧⌘S");
        let del: Shortcut = "Primary+Delete".parse().unwrap();
        assert_eq!(del.label_on("de", false), "Strg+Entf");
        assert_eq!(del.label_on("de", true), "⌘⌦");
        assert_eq!(Shortcut::key(NamedKey::F2).label_on("en", false), "F2");
        let ctrl_tab: Shortcut = "Control+Tab".parse().unwrap();
        assert_eq!(ctrl_tab.label_on("en", true), "⌃⇥");
        assert_eq!(ctrl_tab.label_on("en", false), "Ctrl+Tab");
    }
}

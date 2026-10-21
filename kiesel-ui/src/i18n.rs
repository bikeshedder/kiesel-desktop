//! Messages in the user's language – Kiesel Web's catalogs, as they are.
//!
//! Catalogs are flat JSON objects of MessageFormat 2 messages, one per
//! language and area (`locales/<lang>/kiesel.json`, keys `kiesel.<key>`);
//! apps add their own with [`add_catalog`]. [`t`] gives a message as a signal
//! that follows the [`language`], so switching it updates exactly the widgets
//! that show text.
//!
//! Formatting covers what the catalogs use: `{$x}`, `{$x :number}`,
//! `{$x :list}`, `{$x :string}`, `.input` declarations and `.match` over one or
//! more selectors, with exact keys (`2`, `true`), plural categories (`one`) and
//! `*`. Plural rules, numbers and lists follow English and German (Kiesel's
//! languages); others fall back to English rules for now.
//!
//! The language comes from `KIESEL_LANG`, else `LC_ALL`, `LC_MESSAGES`,
//! `LANGUAGE`, `LANG` – the first that has a catalog (`de_AT.UTF-8` → `de`),
//! else English.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use reactive_graph::signal::ArcRwSignal;
use reactive_graph::traits::{Get, GetUntracked};
use reactive_graph::wrappers::read::ArcSignal;

const FALLBACK: &str = "en";

struct State {
    language: ArcRwSignal<String>,
    /// language → key → message
    catalogs: Mutex<HashMap<String, HashMap<String, String>>>,
}

fn state() -> &'static State {
    static STATE: OnceLock<State> = OnceLock::new();
    STATE.get_or_init(|| {
        let mut catalogs = HashMap::new();
        // Kiesel Web's catalogs as they are, then the desktop's own few messages
        for (lang, json) in [
            ("en", include_str!("../locales/en/kiesel.json")),
            ("de", include_str!("../locales/de/kiesel.json")),
            ("en", include_str!("../locales/en/desktop.json")),
            ("de", include_str!("../locales/de/desktop.json")),
        ] {
            let entries = catalogs
                .entry(lang.to_string())
                .or_insert_with(HashMap::new);
            for (k, v) in parse_catalog(json) {
                entries.insert(format!("kiesel.{k}"), v);
            }
        }
        let language = detect(&catalogs);
        State {
            language: ArcRwSignal::new(language),
            catalogs: Mutex::new(catalogs),
        }
    })
}

/// The language messages are in (a BCP 47 tag like `de` or `en`); set it to switch
pub fn language() -> ArcRwSignal<String> {
    state().language.clone()
}

/// The languages there are catalogs for
pub fn languages() -> Vec<String> {
    let mut l: Vec<String> = state().catalogs.lock().unwrap().keys().cloned().collect();
    l.sort();
    l
}

/// Adds an app's messages: `json` is a flat object of MessageFormat 2 messages,
/// stored under `<prefix>.<key>` (an empty prefix keeps the keys as they are).
/// Later catalogs win over earlier ones, so an app can reword Kiesel's messages.
pub fn add_catalog(lang: &str, prefix: &str, json: &str) {
    let mut catalogs = state().catalogs.lock().unwrap();
    let entries = catalogs.entry(lang.to_string()).or_default();
    for (k, v) in parse_catalog(json) {
        let key = if prefix.is_empty() {
            k
        } else {
            format!("{prefix}.{k}")
        };
        entries.insert(key, v);
    }
}

/// A value for a message's `{$name}`
#[derive(Clone, Debug, PartialEq)]
pub enum Arg {
    Number(f64),
    Text(String),
    List(Vec<String>),
}

impl From<f64> for Arg {
    fn from(v: f64) -> Self {
        Arg::Number(v)
    }
}
impl From<i64> for Arg {
    fn from(v: i64) -> Self {
        Arg::Number(v as f64)
    }
}
impl From<usize> for Arg {
    fn from(v: usize) -> Self {
        Arg::Number(v as f64)
    }
}
impl From<&str> for Arg {
    fn from(v: &str) -> Self {
        Arg::Text(v.to_string())
    }
}
impl From<String> for Arg {
    fn from(v: String) -> Self {
        Arg::Text(v)
    }
}
impl From<bool> for Arg {
    fn from(v: bool) -> Self {
        Arg::Text(v.to_string())
    }
}
impl From<Vec<String>> for Arg {
    fn from(v: Vec<String>) -> Self {
        Arg::List(v)
    }
}

pub type Args = Vec<(&'static str, Arg)>;

/// The message `key` in the current language, following it
pub fn t(key: &'static str) -> ArcSignal<String> {
    let language = language();
    ArcSignal::derive(move || format(&language.get(), key, &[]))
}

/// The message `key` with arguments – `args` may read signals; the text follows them
pub fn t_with(
    key: &'static str,
    args: impl Fn() -> Args + Send + Sync + 'static,
) -> ArcSignal<String> {
    let language = language();
    ArcSignal::derive(move || format(&language.get(), key, &args()))
}

/// The message `key` in `lang` (falling back to English, then to the key itself)
pub fn format(lang: &str, key: &str, args: &[(&str, Arg)]) -> String {
    let catalogs = state().catalogs.lock().unwrap();
    let message = [lang, base(lang), FALLBACK]
        .iter()
        .find_map(|l| catalogs.get(*l).and_then(|c| c.get(key)))
        .cloned();
    drop(catalogs);
    match message {
        Some(m) => Message::parse(&m).format(lang, args),
        None => key.to_string(),
    }
}

/// The language now, without following it (for code that formats once)
pub fn current() -> String {
    state().language.get_untracked()
}

fn base(tag: &str) -> &str {
    tag.split(['-', '_']).next().unwrap_or(tag)
}

fn detect(catalogs: &HashMap<String, HashMap<String, String>>) -> String {
    let wanted = ["KIESEL_LANG", "LC_ALL", "LC_MESSAGES", "LANGUAGE", "LANG"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .flat_map(|v| v.split(':').map(str::to_string).collect::<Vec<_>>())
        .map(|v| v.split(['.', '@']).next().unwrap_or("").replace('_', "-"))
        .filter(|v| !v.is_empty() && v != "C" && v != "POSIX");
    for tag in wanted {
        if catalogs.contains_key(&tag) {
            return tag;
        }
        let b = base(&tag).to_string();
        if catalogs.contains_key(&b) {
            return b;
        }
    }
    FALLBACK.to_string()
}

// ---------- catalogs ----------

/// A flat JSON object of strings – what the catalogs are
fn parse_catalog(json: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut chars = json.chars().peekable();
    let mut strings = Vec::new();
    while let Some(c) = chars.next() {
        if c != '"' {
            continue;
        }
        let mut s = String::new();
        while let Some(c) = chars.next() {
            match c {
                '"' => break,
                '\\' => match chars.next() {
                    Some('n') => s.push('\n'),
                    Some('t') => s.push('\t'),
                    Some('u') => {
                        let hex: String = chars.by_ref().take(4).collect();
                        if let Some(ch) =
                            u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32)
                        {
                            s.push(ch);
                        }
                    }
                    Some(other) => s.push(other),
                    None => {}
                },
                c => s.push(c),
            }
        }
        strings.push(s);
        if strings.len() == 2 {
            let v = strings.pop().unwrap();
            let k = strings.pop().unwrap();
            out.push((k, v));
        }
    }
    out
}

// ---------- MessageFormat 2 (the subset the catalogs use) ----------

#[derive(Debug, PartialEq)]
enum Part {
    Text(String),
    /// `{$name}` or `{$name :function}`
    Var(String, Option<String>),
}

#[derive(Debug)]
struct Message {
    /// `.input {$n :number}`: how a variable is to be read
    inputs: HashMap<String, String>,
    selectors: Vec<String>,
    /// keys per variant (empty for a simple message), and its pattern
    variants: Vec<(Vec<String>, Vec<Part>)>,
}

impl Message {
    fn parse(src: &str) -> Message {
        let src = src.trim();
        let mut inputs = HashMap::new();
        if !src.starts_with('.') {
            return Message {
                inputs,
                selectors: Vec::new(),
                variants: vec![(Vec::new(), pattern(src))],
            };
        }
        let mut rest = src;
        // declarations
        while let Some(r) = rest.strip_prefix(".input") {
            let r = r.trim_start();
            let end = r.find('}').map_or(r.len(), |i| i + 1);
            let (name, func) = placeholder(&r[1..end - 1]);
            if let Some(f) = func {
                inputs.insert(name, f);
            }
            rest = r[end..].trim_start();
        }
        let mut selectors = Vec::new();
        let mut variants = Vec::new();
        if let Some(r) = rest.strip_prefix(".match") {
            let mut r = r.trim_start();
            while let Some(after) = r.strip_prefix('$') {
                let end = after.find(char::is_whitespace).unwrap_or(after.len());
                selectors.push(after[..end].to_string());
                r = after[end..].trim_start();
            }
            // variants: keys, then {{pattern}}
            while !r.is_empty() {
                let open = r.find("{{").unwrap_or(r.len());
                let keys: Vec<String> = r[..open].split_whitespace().map(str::to_string).collect();
                let Some(close) = find_close(&r[open..]) else {
                    break;
                };
                let body = &r[open + 2..open + close];
                variants.push((keys, pattern(body)));
                r = r[open + close + 2..].trim_start();
            }
        } else if let Some(open) = rest.find("{{") {
            // a declared message without .match: its quoted pattern
            let close = find_close(&rest[open..]).unwrap_or(rest.len() - open);
            variants.push((Vec::new(), pattern(&rest[open + 2..open + close])));
        }
        Message {
            inputs,
            selectors,
            variants,
        }
    }

    fn format(&self, lang: &str, args: &[(&str, Arg)]) -> String {
        let arg = |name: &str| args.iter().find(|(n, _)| *n == name).map(|(_, a)| a);
        let chosen = if self.selectors.is_empty() {
            self.variants.first()
        } else {
            // every key must match; exact values beat plural categories beat `*`,
            // compared selector by selector (MF2's preference order)
            self.variants
                .iter()
                .filter_map(|v| {
                    let ranks: Option<Vec<u8>> = self
                        .selectors
                        .iter()
                        .zip(&v.0)
                        .map(|(sel, key)| rank(lang, arg(sel), key))
                        .collect();
                    ranks.map(|r| (r, v))
                })
                .max_by(|a, b| a.0.cmp(&b.0))
                .map(|(_, v)| v)
        };
        let Some((_, parts)) = chosen else {
            return String::new();
        };
        let mut out = String::new();
        for part in parts {
            match part {
                Part::Text(t) => out.push_str(t),
                Part::Var(name, func) => {
                    let func = func
                        .as_deref()
                        .or(self.inputs.get(name).map(String::as_str));
                    match arg(name) {
                        Some(a) => out.push_str(&show(lang, a, func)),
                        None => {
                            out.push('{');
                            out.push_str(name);
                            out.push('}');
                        }
                    }
                }
            }
        }
        out
    }
}

/// How well `key` matches the value: 2 exact, 1 plural category, 0 `*`; None: no match
fn rank(lang: &str, value: Option<&Arg>, key: &str) -> Option<u8> {
    if key == "*" {
        return Some(0);
    }
    match value {
        Some(Arg::Number(n)) => {
            if key.parse::<f64>().is_ok_and(|k| k == *n) {
                Some(2)
            } else if plural(lang, *n) == key {
                Some(1)
            } else {
                None
            }
        }
        Some(Arg::Text(t)) => (t == key).then_some(2),
        Some(Arg::List(l)) => (l.len().to_string() == key).then_some(2),
        None => None,
    }
}

/// CLDR plural category; English and German: "one" for exactly 1, else "other"
fn plural(_lang: &str, n: f64) -> &'static str {
    if n == 1.0 { "one" } else { "other" }
}

fn show(lang: &str, a: &Arg, func: Option<&str>) -> String {
    match (a, func) {
        (Arg::Number(n), _) => number(lang, *n),
        (Arg::List(items), _) => list(lang, items),
        (Arg::Text(t), _) => t.clone(),
    }
}

/// Grouped digits: 1,234.5 (English), 1.234,5 (German)
pub fn number(lang: &str, n: f64) -> String {
    let (group, decimal) = match base(lang) {
        "de" => ('.', ','),
        _ => (',', '.'),
    };
    let negative = n < 0.0;
    let s = format!("{}", n.abs());
    let (int, frac) = s.split_once('.').unwrap_or((&s, ""));
    let mut out = String::new();
    for (i, d) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            out.push(group);
        }
        out.push(d);
    }
    if !frac.is_empty() {
        out.push(decimal);
        out.push_str(frac);
    }
    if negative { format!("-{out}") } else { out }
}

/// "Ann, Bo, and Cy" (English), "Ann, Bo und Cy" (German)
pub fn list(lang: &str, items: &[String]) -> String {
    let (and, serial) = match base(lang) {
        "de" => ("und", false),
        _ => ("and", true),
    };
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} {and} {b}"),
        [init @ .., last] => {
            let comma = if serial { "," } else { "" };
            format!("{}{comma} {and} {last}", init.join(", "))
        }
    }
}

/// Where the `}}` closing a `{{` at the start of `s` is (placeholders `{…}` inside)
fn find_close(s: &str) -> Option<usize> {
    let mut depth = 0;
    let bytes = s.as_bytes();
    let mut i = 2;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' if depth > 0 => depth -= 1,
            b'}' if bytes.get(i + 1) == Some(&b'}') => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

/// `$name :function options` → name, function
fn placeholder(inner: &str) -> (String, Option<String>) {
    let mut words = inner.split_whitespace();
    let name = words
        .next()
        .unwrap_or("")
        .trim_start_matches('$')
        .to_string();
    let func = words
        .next()
        .filter(|w| w.starts_with(':'))
        .map(|w| w[1..].to_string());
    (name, func)
}

fn pattern(src: &str) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut text = String::new();
    let mut chars = src.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c == '{' {
            let end = src[i..].find('}').map_or(src.len(), |e| i + e);
            let inner = &src[i + 1..end];
            if inner.trim_start().starts_with('$') {
                if !text.is_empty() {
                    parts.push(Part::Text(std::mem::take(&mut text)));
                }
                let (name, func) = placeholder(inner);
                parts.push(Part::Var(name, func));
            } else {
                text.push_str(inner.trim().trim_matches('|'));
            }
            while chars.peek().is_some_and(|(j, _)| *j <= end) {
                chars.next();
            }
        } else if c == '\\' {
            if let Some((_, n)) = chars.next() {
                text.push(n);
            }
        } else {
            text.push(c);
        }
    }
    if !text.is_empty() {
        parts.push(Part::Text(text));
    }
    parts
}

/// A widget's text: what it follows, and what it shows (as of the last update)
pub(crate) struct TextProp {
    signal: ArcSignal<String>,
    pub shown: String,
}

impl TextProp {
    pub fn new(text: impl IntoText) -> Self {
        let signal = text.into_text();
        let shown = signal.get_untracked();
        Self { signal, shown }
    }

    /// For a widget's watch: subscribes to the text
    pub fn signal(&self) -> ArcSignal<String> {
        self.signal.clone()
    }

    /// Takes the current text; true if it changed
    pub fn refresh(&mut self) -> bool {
        let now = self.signal.get_untracked();
        if now == self.shown {
            return false;
        }
        self.shown = now;
        true
    }
}

impl Default for TextProp {
    fn default() -> Self {
        Self::new("")
    }
}

/// What a widget shows as text: a string, or a signal (a translation, say)
pub trait IntoText {
    fn into_text(self) -> ArcSignal<String>;
}

impl IntoText for &str {
    fn into_text(self) -> ArcSignal<String> {
        ArcSignal::stored(self.to_string())
    }
}
impl IntoText for String {
    fn into_text(self) -> ArcSignal<String> {
        ArcSignal::stored(self)
    }
}
impl IntoText for ArcSignal<String> {
    fn into_text(self) -> ArcSignal<String> {
        self
    }
}
impl IntoText for ArcRwSignal<String> {
    fn into_text(self) -> ArcSignal<String> {
        self.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(src: &str, lang: &str, args: &[(&str, Arg)]) -> String {
        Message::parse(src).format(lang, args)
    }

    #[test]
    fn simple_messages_and_placeholders() {
        assert_eq!(fmt("Close", "en", &[]), "Close");
        assert_eq!(
            fmt(
                "No results for “{$query}”",
                "en",
                &[("query", "kie".into())]
            ),
            "No results for “kie”"
        );
        assert_eq!(
            fmt(
                "{$n} of {$total}",
                "de",
                &[("n", 3_i64.into()), ("total", 1200_i64.into())]
            ),
            "3 of 1.200"
        );
    }

    #[test]
    fn plurals_pick_one_or_other() {
        let m = ".input {$n :number} .match $n one {{1 result}} * {{{$n} results}}";
        assert_eq!(fmt(m, "en", &[("n", 1_i64.into())]), "1 result");
        assert_eq!(fmt(m, "en", &[("n", 0_i64.into())]), "0 results");
        assert_eq!(fmt(m, "en", &[("n", 1234_i64.into())]), "1,234 results");
    }

    #[test]
    fn exact_keys_beat_categories_and_two_selectors_work() {
        let typing = ".input {$count :number} .match $count one {{{$names :list} is typing…}} 2 {{{$names :list} are typing…}} * {{Several people are typing…}}";
        let names = |n: &[&str]| Arg::List(n.iter().map(|s| s.to_string()).collect());
        assert_eq!(
            fmt(
                typing,
                "en",
                &[("count", 2_i64.into()), ("names", names(&["Ann", "Bo"]))]
            ),
            "Ann and Bo are typing…"
        );
        assert_eq!(
            fmt(
                typing,
                "en",
                &[("count", 5_i64.into()), ("names", names(&[]))]
            ),
            "Several people are typing…"
        );
        let react = ".input {$count :number} .input {$mine :string} .match $count $mine one true {{{$emoji} 1 reaction, including yours}} one * {{{$emoji} 1 reaction}} * true {{{$emoji} {$count} reactions, including yours}} * * {{{$emoji} {$count} reactions}}";
        let args = |c: i64, mine: bool| {
            vec![
                ("count", c.into()),
                ("mine", mine.into()),
                ("emoji", "👍".into()),
            ]
        };
        assert_eq!(
            fmt(react, "en", &args(1, true)),
            "👍 1 reaction, including yours"
        );
        assert_eq!(fmt(react, "en", &args(3, false)), "👍 3 reactions");
    }

    #[test]
    fn lists_and_numbers_per_language() {
        let three = ["Ann", "Bo", "Cy"].map(String::from);
        assert_eq!(list("en", &three), "Ann, Bo, and Cy");
        assert_eq!(list("de", &three), "Ann, Bo und Cy");
        assert_eq!(number("de-AT", 1234567.5), "1.234.567,5");
        assert_eq!(number("en", -1000.0), "-1,000");
    }

    #[test]
    fn every_catalog_message_parses_and_formats() {
        for lang in ["en", "de"] {
            let json = if lang == "en" {
                include_str!("../locales/en/kiesel.json")
            } else {
                include_str!("../locales/de/kiesel.json")
            };
            let entries = parse_catalog(json);
            assert!(entries.len() > 300, "{lang}: {}", entries.len());
            for (k, v) in entries {
                let m = Message::parse(&v);
                assert!(!m.variants.is_empty(), "{lang} {k}: no pattern");
                let out = m.format(lang, &[("n", 2_i64.into()), ("count", 1_i64.into())]);
                assert!(
                    !out.contains("{{") && !out.contains(".match"),
                    "{lang} {k}: {out}"
                );
            }
        }
    }

    #[test]
    fn the_catalogs_have_the_same_keys() {
        let en: Vec<String> = parse_catalog(include_str!("../locales/en/kiesel.json"))
            .into_iter()
            .map(|e| e.0)
            .collect();
        let de: Vec<String> = parse_catalog(include_str!("../locales/de/kiesel.json"))
            .into_iter()
            .map(|e| e.0)
            .collect();
        assert_eq!(en, de);
        assert_eq!(
            format("de", "kiesel.themeSwitch.dark", &[]),
            "Dunkles Design"
        );
        assert_eq!(
            format("fr", "kiesel.themeSwitch.dark", &[]),
            "Dark theme",
            "unknown languages fall back to English"
        );
    }
}

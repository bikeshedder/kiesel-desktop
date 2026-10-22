//! `TextField` – a single line of editable text in Kiesel's field well, and
//! [`Textarea`] – several lines (see `kiesel/src/components/TextField`).
//!
//! Editing is parley's `PlainEditor`: caret and selection by pointer (click,
//! drag, double click for a word, triple click for all, Shift+click extends)
//! and keyboard (arrows, Home/End, with Shift to select and Ctrl by word;
//! Backspace/Delete, Ctrl for words; Ctrl+A/C/X/V), and input method
//! composition shown underlined. Keyboard focus selects everything, like a
//! browser's input. The caret blinks for 10 s after the last change (GTK's
//! habit), then stays – an idle field costs no frames.
//!
//! The value is a signal both ways: typing sets it, setting it replaces the text.
//!
//! A textarea wraps its lines (a `mono` one, for code, doesn't) and scrolls
//! vertically to keep the caret in view, by wheel and touchpad too. Up and
//! Down move by lines, PageUp and PageDown by what is visible, Home and End to
//! the ends of the line (with Ctrl of the text); Enter starts a new line and
//! Ctrl+Enter submits. Assistive technology reads it line by line.

use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use parley::Affinity;
use parley::editing::{Cursor, PlainEditor, Selection};
use parley::{FontFamily, FontWeight, LineHeight, StyleProperty};
use reactive_graph::signal::ArcRwSignal;
use reactive_graph::traits::{GetUntracked, Set, Track, WithUntracked};
use reactive_graph::wrappers::read::ArcSignal;
use vello_cpu::kurbo::{Point, Rect, Size};
use winit::cursor::CursorIcon;
use winit::keyboard::{Key, NamedKey};

use crate::anim::Transition;
use crate::field::FieldLink;
use crate::i18n::{IntoText, TextProp};
use crate::paint;
use crate::reactive::{MountCx, Watch};
use crate::shortcut::{KeyPress, Mods, Shortcut};
use crate::text::{self, Text};
use crate::theme::{TextRole, TextStyle, Theme, space};
use crate::widget::{
    Available, Change, Event, EventCx, Ime, Known, LayoutCx, PaintCx, ScrollDelta, Widget,
    WidgetId, accessible_rect,
};

/// Called with the text when Enter is pressed
type OnSubmit = Box<dyn FnMut(&str)>;
/// Hears the text typed (not set from outside)
type OnInput = Box<dyn FnMut(&str)>;
/// Sees a key before the field: true if it used it (a number field's Up/Down)
type OnKey = Box<dyn FnMut(&KeyPress, &mut EventCx<'_>) -> bool>;

/// A combobox, for assistive technology: whether its list shows, and the
/// option highlighted in it
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Combo {
    pub expanded: bool,
    pub active: Option<WidgetId>,
}

/// A number field, for assistive technology: a spin button
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Spin {
    pub value: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: f64,
}

const BLINK: Duration = Duration::from_millis(500);
const BLINK_FOR: Duration = Duration::from_secs(10);
const CARET_WIDTH: f32 = 1.5;
/// What a hidden password shows for each character
const BULLET: char = '•';

/// A textarea: how many lines show, and whether it holds code
#[derive(Clone, Copy)]
struct Lines {
    rows: usize,
    /// Monospace, lines not wrapped – JSON, CSS, a config
    mono: bool,
}

/// `--ksl-font-mono`
const MONO: &str = "ui-monospace, \"DejaVu Sans Mono\", monospace";

/// Several lines of editable text, with the same states as a [`TextField`]
/// (put it in a [`Field`](crate::Field) for a label, hint and error)
pub struct Textarea;

impl Textarea {
    /// Five lines high; more scroll. [`TextField::rows`] and [`TextField::mono`] change that.
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> TextField {
        let mut field = TextField::new();
        field.lines = Some(Lines {
            rows: 5,
            mono: false,
        });
        field
    }
}

/// Editing commands with platform shortcuts
#[derive(Clone, Copy, PartialEq, Eq)]
enum Edit {
    SelectAll,
    Copy,
    Cut,
    Paste,
}

pub struct TextField {
    id: WidgetId,
    /// Ids of its text-run nodes for assistive technology (one per 255 characters)
    run_ids: Vec<WidgetId>,
    value: ArcRwSignal<String>,
    watch: Option<Watch>,
    placeholder: TextProp,
    label: TextProp,
    /// Inside a `Field`: its label, message and error state
    field: Option<FieldLink>,
    /// The field's error state as of the last update
    invalid: bool,
    description: String,
    grow: bool,
    placeholder_layout: Option<parley::Layout<()>>,
    /// What the editor was set up with – the theme's text size can change
    style: Option<TextStyle>,
    editor: PlainEditor<()>,
    /// The editor holds `value` as of the last sync (no composition)
    synced: String,
    on_submit: Option<OnSubmit>,
    bounds: Rect,
    /// A textarea's rows and font; `None`: one line
    lines: Option<Lines>,
    /// Widgets at the end of the well: steppers, a unit, a button
    pub(crate) trailing: Vec<Box<dyn Widget>>,
    /// The text at the end of the line (numbers)
    pub(crate) align_end: bool,
    pub(crate) on_key: Option<OnKey>,
    /// Focus left (a number field clamps its value)
    pub(crate) on_blur: Option<Box<dyn FnMut()>>,
    /// The text changed by typing (not from outside)
    pub(crate) on_input: Option<OnInput>,
    /// Assistive technology stepped the value
    pub(crate) on_step: Option<Box<dyn FnMut(i8)>>,
    /// A number field: what a spin button reports
    pub(crate) spin: Option<Box<dyn Fn() -> Spin>>,
    /// A combobox: what it reports
    pub(crate) combo: Option<Box<dyn Fn() -> Combo>>,
    /// A password: hidden behind bullets while this is true
    pub(crate) mask: Option<ArcSignal<bool>>,
    shown_mask: bool,
    /// The bullets shown for a hidden password, and their layout
    bullet_text: String,
    bullets: Option<parley::Layout<()>>,
    /// Caps Lock seems on (a hidden password warns about it)
    pub(crate) on_caps: Option<Box<dyn FnMut(bool)>>,
    caps: bool,
    /// How far the text is scrolled left to keep the caret in view
    scroll: f64,
    /// How far a textarea's text is scrolled up
    scroll_y: f64,
    /// Line height of its text (from the theme, at layout)
    line: f64,
    hovered: bool,
    focused: bool,
    dragging: bool,
    /// The caret blinks from here (reset by every edit and move)
    blink_from: Instant,
    hover: Transition,
    focus: Transition,
    repaint: bool,
}

impl TextField {
    pub fn new() -> Self {
        Self {
            id: WidgetId::next(),
            run_ids: vec![WidgetId::next()],
            value: ArcRwSignal::new(String::new()),
            watch: None,
            placeholder: TextProp::default(),
            label: TextProp::default(),
            field: None,
            invalid: false,
            description: String::new(),
            grow: false,
            placeholder_layout: None,
            style: None,
            editor: PlainEditor::new(16.0),
            synced: String::new(),
            on_submit: None,
            bounds: Rect::ZERO,
            lines: None,
            trailing: Vec::new(),
            align_end: false,
            on_key: None,
            on_blur: None,
            on_input: None,
            on_step: None,
            spin: None,
            combo: None,
            mask: None,
            shown_mask: false,
            bullet_text: String::new(),
            bullets: None,
            on_caps: None,
            caps: false,
            scroll: 0.0,
            scroll_y: 0.0,
            line: 24.0,
            hovered: false,
            focused: false,
            dragging: false,
            blink_from: Instant::now(),
            hover: Transition::new(0.0),
            focus: Transition::new(0.0),
            repaint: false,
        }
    }

    /// The text, both ways: typing sets the signal, setting the signal replaces the text
    pub fn value(mut self, value: ArcRwSignal<String>) -> Self {
        self.value = value;
        self
    }

    pub fn placeholder(mut self, text: impl IntoText) -> Self {
        self.placeholder = TextProp::new(text);
        self
    }

    /// Its accessible name, without a visible label – a search box in a toolbar.
    /// With a visible label, put it in a [`Field`](crate::Field) instead.
    pub fn label(mut self, text: impl IntoText) -> Self {
        self.label = TextProp::new(text);
        self
    }

    /// Take the free space along the parent's row (`flex: 1`)
    pub fn grow(mut self) -> Self {
        self.grow = true;
        self
    }

    /// A textarea's height in lines (default 5); more lines scroll
    pub fn rows(mut self, rows: usize) -> Self {
        if let Some(l) = &mut self.lines {
            l.rows = rows.max(1);
        }
        self
    }

    /// A textarea for code: a monospace font, lines not wrapped
    pub fn mono(mut self) -> Self {
        if let Some(l) = &mut self.lines {
            l.mono = true;
        }
        self
    }

    /// Enter pressed (Ctrl+Enter in a textarea, where Enter starts a line)
    pub fn on_submit(mut self, f: impl FnMut(&str) + 'static) -> Self {
        self.on_submit = Some(Box::new(f));
        self
    }

    /// Where the text starts, in window coordinates: the line centred in the
    /// well (at its end when aligned so); a textarea's first line at the top, scrolled
    fn text_origin(&self) -> Point {
        let y = match self.lines {
            Some(_) => self.bounds.y0 + space(2) - self.scroll_y,
            None => self.bounds.y0 + (self.bounds.height() - self.line) / 2.0,
        };
        let view = self.viewport();
        let end = if self.align_end {
            let width = self
                .editor
                .try_layout()
                .map_or(0.0, |l| f64::from(l.width()));
            (view.width() - width - f64::from(CARET_WIDTH)).max(0.0)
        } else {
            0.0
        };
        Point::new(view.x0 + end - self.scroll, y)
    }

    /// The text box the field shows: inside its padding, before what trails it
    fn viewport(&self) -> Rect {
        let pad_y = if self.lines.is_some() { space(2) } else { 0.0 };
        let mut view = self.bounds.inflate(-space(3), -pad_y);
        if let Some(start) = trailing_start(&self.trailing) {
            view.x1 = (start - space(2)).max(view.x0);
        }
        view
    }

    /// The layout drawn: the text, or a hidden password's bullets
    fn shown_layout(&self) -> Option<&parley::Layout<()>> {
        if self.shown_mask {
            self.bullets.as_ref()
        } else {
            self.editor.try_layout()
        }
    }

    /// The text drawn
    fn shown_text(&self) -> &str {
        if self.shown_mask {
            &self.bullet_text
        } else {
            self.editor.raw_text()
        }
    }

    /// A byte offset in the text, in the text drawn (a bullet a character)
    fn to_shown(&self, real: usize) -> usize {
        if self.shown_mask {
            let real = real.min(self.editor.raw_text().len());
            self.editor.raw_text()[..real].chars().count() * BULLET.len_utf8()
        } else {
            real
        }
    }

    /// A byte offset in the text drawn, in the text
    fn to_real(&self, shown: usize) -> usize {
        if self.shown_mask {
            let text = self.editor.raw_text();
            text.char_indices()
                .nth(shown / BULLET.len_utf8())
                .map_or(text.len(), |(i, _)| i)
        } else {
            shown
        }
    }

    /// The caret's box in the layout drawn
    fn caret_box(&self) -> Option<Rect> {
        if !self.shown_mask {
            let a = self.editor.ime_cursor_area();
            return Some(Rect::new(a.x0, a.y0, a.x1, a.y1));
        }
        let layout = self.bullets.as_ref()?;
        let at = self.to_shown(self.editor.raw_selection().focus().index());
        let b =
            Cursor::from_byte_index(layout, at, Affinity::Downstream).geometry(layout, CARET_WIDTH);
        Some(Rect::new(b.x0, b.y0, b.x1, b.y1))
    }

    /// Lines wrap: a textarea, unless it holds code
    fn wraps(&self) -> bool {
        self.lines.is_some_and(|l| !l.mono)
    }

    /// The font: the body text's, a textarea for code monospace at 0.9 em
    fn text_style(&self, theme: &Theme) -> TextStyle {
        let mut style = theme.text_style(TextRole::Body);
        if self.lines.is_some_and(|l| l.mono) {
            style.family = MONO;
            style.size *= 0.9;
        }
        style
    }

    /// How far a textarea's text can scroll up
    fn max_scroll_y(&self) -> f64 {
        let height = self
            .editor
            .try_layout()
            .map_or(0.0, |l| f64::from(l.height()));
        (height - self.viewport().height()).max(0.0)
    }

    /// The text without the composition in progress
    fn plain_text(&self) -> String {
        self.editor.text().to_string()
    }

    /// Lays out the editor and scrolls so the caret (or the composition) is visible
    fn refresh(&mut self, text: &mut Text) {
        let (fonts, layouts) = text.contexts();
        self.editor.driver(fonts, layouts).refresh_layout();
        if self.shown_mask {
            let n = self.editor.raw_text().chars().count();
            if self.bullets.is_none() || self.bullet_text.chars().count() != n {
                self.bullet_text = BULLET.to_string().repeat(n);
                // the editor's font (set at the first update)
                let style = self.style.unwrap_or(TextStyle {
                    family: "system-ui",
                    size: 16.0,
                    weight: 400.0,
                    letter_spacing: 0.0,
                    line_height: 1.5,
                });
                self.bullets = Some(text.line(&self.bullet_text, &style));
            }
        }
        let Some(layout) = self.shown_layout() else {
            return;
        };
        let width = f64::from(layout.width());
        let Some(area) = self.caret_box() else {
            return;
        };
        if self.wraps() {
            self.scroll = 0.0;
        } else {
            let caret = area.x1;
            let view = self.viewport().width() - f64::from(CARET_WIDTH);
            if caret - self.scroll > view {
                self.scroll = caret - view;
            } else if caret < self.scroll {
                self.scroll = caret;
            }
            // no empty space after the text while scrolled
            self.scroll = self.scroll.min((width - view).max(0.0)).max(0.0);
        }
        if self.lines.is_some() {
            let view = self.viewport().height();
            // the caret is followed only while it shows: a textarea filled from
            // outside starts at its top
            if self.focused && area.y1 - self.scroll_y > view {
                self.scroll_y = area.y1 - view;
            } else if self.focused && area.y0 < self.scroll_y {
                self.scroll_y = area.y0;
            }
            self.scroll_y = self.scroll_y.clamp(0.0, self.max_scroll_y());
        }
    }

    /// After an edit: publish the value, restart the blink, repaint
    fn edited(&mut self, cx: &mut EventCx<'_>) {
        self.refresh(cx.text);
        let text = self.plain_text();
        if text != self.synced {
            self.synced = text.clone();
            if let Some(f) = &mut self.on_input {
                f(&text);
            }
            self.value.set(text);
        }
        self.blink_from = cx.now;
        self.repaint = true;
    }

    fn sync(&mut self, now: Instant, theme: &Theme) {
        let d = theme.metrics.duration;
        let on = |b: bool| f64::from(u8::from(b));
        let mut changed = self.hover.set(on(self.hovered && !self.focused), now, d);
        changed |= self.focus.set(on(self.focused), now, d);
        self.repaint |= changed;
    }

    /// A key while focused; true if it was handled
    fn key(&mut self, cx: &mut EventCx<'_>, press: &KeyPress, typed: Option<&str>) -> bool {
        let (shift, ctrl) = (cx.modifiers.shift_key(), cx.modifiers.control_key());
        let edit = [
            (Shortcut::select_all(), Edit::SelectAll),
            (Shortcut::copy(), Edit::Copy),
            (Shortcut::cut(), Edit::Cut),
            (Shortcut::paste(), Edit::Paste),
        ]
        .into_iter()
        .find_map(|(s, e)| s.matches(press).then_some(e));
        if let Some(edit) = edit {
            self.edit(cx, edit);
            return true;
        }
        let key = &press.key;
        let multiline = self.lines.is_some();
        // a hidden password doesn't tell its words apart: Ctrl moves to its ends
        let (masked, ctrl) = (self.shown_mask, ctrl && !self.shown_mask);
        let to_end = cx.modifiers.control_key() && masked;
        // PageUp/PageDown: as many lines as show, less one
        let page = ((self.viewport().height() / self.line).floor() as usize)
            .saturating_sub(1)
            .max(1);
        let (fonts, layouts) = cx.text.contexts();
        let mut d = self.editor.driver(fonts, layouts);
        match key {
            Key::Named(NamedKey::ArrowUp) if multiline && shift => d.select_up(),
            Key::Named(NamedKey::ArrowUp) if multiline => d.move_up(),
            Key::Named(NamedKey::ArrowDown) if multiline && shift => d.select_down(),
            Key::Named(NamedKey::ArrowDown) if multiline => d.move_down(),
            Key::Named(NamedKey::PageUp) if multiline => {
                for _ in 0..page {
                    if shift { d.select_up() } else { d.move_up() }
                }
            }
            Key::Named(NamedKey::PageDown) if multiline => {
                for _ in 0..page {
                    if shift {
                        d.select_down()
                    } else {
                        d.move_down()
                    }
                }
            }
            // a textarea's Home and End: the line's ends; with Ctrl the text's
            Key::Named(NamedKey::Home) if multiline && !ctrl && shift => d.select_to_line_start(),
            Key::Named(NamedKey::Home) if multiline && !ctrl => d.move_to_line_start(),
            Key::Named(NamedKey::End) if multiline && !ctrl && shift => d.select_to_line_end(),
            Key::Named(NamedKey::End) if multiline && !ctrl => d.move_to_line_end(),
            Key::Named(NamedKey::Enter) if multiline && !ctrl => {
                d.insert_or_replace_selection("\n")
            }
            Key::Named(NamedKey::ArrowLeft) if to_end && shift => d.select_to_text_start(),
            Key::Named(NamedKey::ArrowLeft) if to_end => d.move_to_text_start(),
            Key::Named(NamedKey::ArrowRight) if to_end && shift => d.select_to_text_end(),
            Key::Named(NamedKey::ArrowRight) if to_end => d.move_to_text_end(),
            Key::Named(NamedKey::ArrowLeft) => match (shift, ctrl) {
                (true, true) => d.select_word_left(),
                (true, false) => d.select_left(),
                (false, true) => d.move_word_left(),
                (false, false) => d.move_left(),
            },
            Key::Named(NamedKey::ArrowRight) => match (shift, ctrl) {
                (true, true) => d.select_word_right(),
                (true, false) => d.select_right(),
                (false, true) => d.move_word_right(),
                (false, false) => d.move_right(),
            },
            Key::Named(NamedKey::Home) if shift => d.select_to_text_start(),
            Key::Named(NamedKey::Home) => d.move_to_text_start(),
            Key::Named(NamedKey::End) if shift => d.select_to_text_end(),
            Key::Named(NamedKey::End) => d.move_to_text_end(),
            Key::Named(NamedKey::Backspace) if ctrl => d.backdelete_word(),
            Key::Named(NamedKey::Backspace) => d.backdelete(),
            Key::Named(NamedKey::Delete) if ctrl => d.delete_word(),
            Key::Named(NamedKey::Delete) => d.delete(),
            Key::Named(NamedKey::Enter) => {
                let text = self.plain_text();
                if let Some(f) = &mut self.on_submit {
                    f(&text);
                }
                return true;
            }
            _ if !Mods::held(cx.modifiers).contains(Mods::PRIMARY)
                && !ctrl
                && !cx.modifiers.alt_key() =>
            {
                let Some(typed) = typed.filter(|t| !t.chars().any(char::is_control)) else {
                    return false;
                };
                d.insert_or_replace_selection(typed);
            }
            _ => return false,
        }
        true
    }

    /// Caps Lock, as far as a key tells: a letter in the other case than
    /// Shift asks for, or the Caps Lock key itself
    fn guess_caps(&mut self, press: &KeyPress) {
        let caps = match &press.key {
            Key::Named(NamedKey::CapsLock) => !self.caps,
            Key::Character(c) => match c.chars().next().filter(|c| c.is_alphabetic()) {
                Some(c) if c.is_uppercase() != c.is_lowercase() => {
                    c.is_uppercase() != press.mods.shift_key()
                }
                _ => self.caps,
            },
            _ => self.caps,
        };
        if caps != self.caps {
            self.caps = caps;
            if let Some(f) = &mut self.on_caps {
                f(caps);
            }
        }
    }

    /// Select all and the clipboard
    fn edit(&mut self, cx: &mut EventCx<'_>, edit: Edit) {
        let (fonts, layouts) = cx.text.contexts();
        let mut d = self.editor.driver(fonts, layouts);
        match edit {
            Edit::SelectAll => d.select_all(),
            // a hidden password stays where it is
            Edit::Copy | Edit::Cut if self.shown_mask => {}
            Edit::Copy | Edit::Cut => {
                if let Some(sel) = self.editor.selected_text().map(str::to_owned) {
                    cx.clipboard.set(&sel);
                    if edit == Edit::Cut {
                        let (fonts, layouts) = cx.text.contexts();
                        self.editor.driver(fonts, layouts).delete_selection();
                    }
                }
            }
            Edit::Paste => {
                if let Some(pasted) = cx.clipboard.get() {
                    // one line: line breaks become spaces; a textarea keeps them, as \n
                    let pasted = if self.lines.is_some() {
                        pasted.replace("\r\n", "\n").replace('\r', "\n")
                    } else {
                        pasted.replace(['\r', '\n'], " ")
                    };
                    d.insert_or_replace_selection(&pasted);
                }
            }
        }
    }

    /// Editor coordinates of a window point
    fn to_editor(&self, p: Point) -> (f32, f32) {
        let o = self.text_origin();
        ((p.x - o.x) as f32, (p.y - o.y) as f32)
    }
}

/// A text run may hold this many characters (its word starts are bytes)
const MAX_RUN: usize = 255;

/// One character of the shown text – the smallest selectable unit, a parley cluster
struct Char {
    /// Byte range in the text
    start: usize,
    len: usize,
    /// Offset from the start of the line, and advance
    x: f32,
    width: f32,
    space: bool,
    /// Its line, and the line's top and bottom in the layout
    line: usize,
    top: f32,
    bottom: f32,
}

impl TextField {
    /// The characters of the text in text order – without a composition in
    /// progress, which isn't text yet (GTK doesn't expose it either)
    fn characters(&self) -> Vec<Char> {
        let Some(layout) = self.shown_layout() else {
            return Vec::new();
        };
        let compose = self.editor.raw_compose().clone().unwrap_or(0..0);
        let compose = self.to_shown(compose.start)..self.to_shown(compose.end);
        let mut chars = Vec::new();
        for (n, line) in layout.lines().enumerate() {
            let m = line.metrics();
            let mut x = 0.0;
            for run in line.runs() {
                for c in run.visual_clusters() {
                    let range = c.text_range();
                    if !range.is_empty() && !compose.contains(&range.start) {
                        chars.push(Char {
                            start: range.start,
                            len: range.len(),
                            x,
                            width: c.advance(),
                            space: c.is_space_or_nbsp(),
                            line: n,
                            top: m.block_min_coord,
                            bottom: m.block_max_coord,
                        });
                    }
                    x += c.advance();
                }
            }
        }
        chars.sort_by_key(|c| c.start);
        chars
    }

    /// The text position (run and character) of a byte offset
    fn position_of(&self, byte: usize) -> accesskit::TextPosition {
        let byte = self.to_shown(byte);
        let chars = self.characters();
        let runs = runs(&chars);
        let i = chars
            .iter()
            .position(|c| c.start >= byte)
            .unwrap_or(chars.len());
        // the end of the text is the end of the last run, not the start of a new one
        let (run, index) = match runs.iter().position(|r| r.contains(&i)) {
            Some(n) => (n, i - runs[n].start),
            None => (runs.len() - 1, runs.last().map_or(0, |r| r.len())),
        };
        accesskit::TextPosition {
            node: self.run_ids.get(run).unwrap_or(&self.run_ids[0]).node(),
            character_index: index,
        }
    }

    /// The byte offset of a text position, if it is one of ours
    fn byte_at(&self, pos: &accesskit::TextPosition) -> Option<usize> {
        let run = self.run_ids.iter().position(|id| id.node() == pos.node)?;
        let chars = self.characters();
        let i = runs(&chars).get(run)?.start + pos.character_index;
        Some(
            chars
                .get(i)
                .map_or(self.editor.raw_text().len(), |c| self.to_real(c.start)),
        )
    }
}

/// Where the widgets at the end of a field begin: the leftmost one that is
/// laid out (a `Show` takes no box – its widget does, or nothing)
fn trailing_start(widgets: &[Box<dyn Widget>]) -> Option<f64> {
    widgets
        .iter()
        .filter_map(|w| {
            if w.layout_transparent() {
                trailing_start(w.children())
            } else {
                Some(w.bounds().x0)
            }
        })
        .reduce(f64::min)
}

/// The text runs: a line each (of at most 255 characters – longer lines take
/// several), as index ranges into the characters; one empty run without text
fn runs(chars: &[Char]) -> Vec<std::ops::Range<usize>> {
    let mut out: Vec<std::ops::Range<usize>> = Vec::new();
    for (i, c) in chars.iter().enumerate() {
        match out.last_mut() {
            Some(r) if chars[r.start].line == c.line && r.len() < MAX_RUN => r.end = i + 1,
            _ => out.push(i..i + 1),
        }
    }
    if out.is_empty() {
        out.push(0..0);
    }
    out
}

impl Default for TextField {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for TextField {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn mount(&mut self, cx: &mut MountCx<'_>) {
        let value = self.value.clone();
        let (label, placeholder) = (self.label.signal(), self.placeholder.signal());
        let (field, mask) = (self.field.clone(), self.mask.clone());
        self.watch = Some(cx.watch(self.id, move || {
            value.track();
            label.track();
            placeholder.track();
            if let Some(m) = &mask {
                m.track();
            }
            if let Some(f) = &field {
                f.invalid.track();
                f.description.track();
            }
        }));
    }

    fn link_field(&mut self, link: FieldLink) {
        self.field = Some(link);
    }

    /// Picks up a value set from outside (not by typing), and repaints after events
    fn update(&mut self, cx: &mut LayoutCx<'_>) -> Change {
        let style = self.text_style(cx.theme);
        let mut change = Change::NONE;
        if self.style != Some(style) {
            // first update, or another text size: font settings, then the value
            if self.style.is_some() {
                change = Change::LAYOUT;
            }
            self.style = Some(style);
            let styles = self.editor.edit_styles();
            styles.insert(StyleProperty::FontFamily(FontFamily::Source(
                style.family.into(),
            )));
            styles.insert(StyleProperty::FontSize(style.size));
            styles.insert(StyleProperty::FontWeight(FontWeight::new(style.weight)));
            styles.insert(StyleProperty::LineHeight(LineHeight::FontSizeRelative(
                style.line_height,
            )));
            self.synced.clear();
            self.placeholder_layout = None;
            self.repaint = true;
        }
        // the placeholder: shaped at first, and again when its text changes
        if self.placeholder.refresh()
            || (self.placeholder_layout.is_none() && !self.placeholder.shown.is_empty())
        {
            self.placeholder_layout = (!self.placeholder.shown.is_empty())
                .then(|| cx.text.line(&self.placeholder.shown, &style));
            self.repaint = true;
        }
        self.repaint |= self.label.refresh();
        if let Some(f) = &self.field {
            let (invalid, description) = (f.invalid.get_untracked(), f.description.get_untracked());
            if (invalid, &description) != (self.invalid, &self.description) {
                (self.invalid, self.description) = (invalid, description);
                self.repaint = true;
            }
        }
        let masked = self.mask.as_ref().is_some_and(GetUntracked::get_untracked);
        if masked != self.shown_mask || (masked && self.bullets.is_none()) {
            // shown or hidden: the same text, drawn again; the caret stays
            self.shown_mask = masked;
            self.bullets = None;
            self.refresh(cx.text);
            self.repaint = true;
        }
        if self.value.with_untracked(|v| *v != self.synced) {
            self.synced = self.value.with_untracked(String::clone);
            self.editor.set_text(&self.synced);
            let (fonts, layouts) = cx.text.contexts();
            self.editor.driver(fonts, layouts).move_to_text_end();
            self.repaint = true;
        }
        let (fonts, layouts) = cx.text.contexts();
        self.editor.driver(fonts, layouts).refresh_layout();
        let runs = runs(&self.characters()).len();
        while self.run_ids.len() < runs {
            self.run_ids.push(WidgetId::next());
        }
        if std::mem::take(&mut self.repaint) {
            change |= Change::PAINT;
        }
        change
    }

    /// A leaf – with trailing widgets a row that keeps them at its end, 4 px
    /// in from the edge (the steppers' inset), as high as a control
    fn style(&self, theme: &Theme) -> taffy::Style {
        let mut style = taffy::Style {
            min_size: taffy::Size {
                width: taffy::LengthPercentageAuto::length(0.0),
                height: taffy::LengthPercentageAuto::auto(),
            },
            flex_grow: if self.grow { 1.0 } else { 0.0 },
            ..Default::default()
        };
        if !self.trailing.is_empty() {
            let px = |v: f64| taffy::LengthPercentage::length(v as f32);
            style.display = taffy::Display::Flex;
            style.flex_direction = taffy::FlexDirection::Row;
            style.justify_content = taffy::JustifyContent::END;
            style.align_items = taffy::AlignItems::CENTER;
            style.gap = taffy::Size {
                width: px(space(1)),
                height: px(0.0),
            };
            style.padding = taffy::Rect {
                left: px(0.0),
                right: px(4.0),
                top: px(0.0),
                bottom: px(0.0),
            };
            // room for some text before what trails it
            style.min_size = taffy::Size {
                width: taffy::LengthPercentageAuto::length(160.0),
                height: taffy::LengthPercentageAuto::length(theme.metrics.control as f32),
            };
        }
        style
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.trailing
    }

    fn children_mut(&mut self) -> &mut [Box<dyn Widget>] {
        &mut self.trailing
    }

    /// A press on a trailing widget that takes no focus (a stepper) keeps it here
    fn press_focuses(&self, _p: Point) -> Option<WidgetId> {
        Some(self.id)
    }

    /// As wide as offered (like a block-level input), 240 px on its own;
    /// `--ksl-control` high – a textarea its rows and padding
    fn measure(&mut self, cx: &mut LayoutCx<'_>, known: Known, available: Available) -> Size {
        let width = known.width.unwrap_or(match available.width {
            taffy::AvailableSpace::MinContent => 80.0,
            taffy::AvailableSpace::MaxContent => 240.0,
            taffy::AvailableSpace::Definite(w) => w,
        });
        let height = match self.lines {
            Some(l) => {
                let style = self.text_style(cx.theme);
                l.rows as f64 * f64::from(style.size * style.line_height) + 2.0 * space(2)
            }
            None => cx.theme.metrics.control,
        };
        Size::new(f64::from(width), height)
    }

    fn set_bounds(&mut self, cx: &mut LayoutCx<'_>, bounds: Rect) {
        self.bounds = bounds;
        let style = self.text_style(cx.theme);
        self.line = f64::from(style.size * style.line_height);
        // a textarea's lines break at the width it shows
        let width = self.wraps().then(|| self.viewport().width() as f32);
        self.editor.set_width(width);
        self.refresh(cx.text);
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    /// The focus ring reaches 3 px out, the sheen 1 px below
    fn ink(&self) -> Rect {
        self.bounds.inflate(4.0, 4.0)
    }

    fn focusable(&self) -> bool {
        true
    }

    /// A hidden password takes no input method (as on GTK): keys only
    fn accepts_text(&self) -> bool {
        !self.shown_mask
    }

    fn cursor(&self) -> CursorIcon {
        CursorIcon::Text
    }

    fn ime_area(&self) -> Option<Rect> {
        let r = self.caret_box()? + self.text_origin().to_vec2();
        Some(r.intersect(self.bounds))
    }

    fn wake_at(&self, now: Instant) -> Option<Instant> {
        if !self.focused || now.duration_since(self.blink_from) >= BLINK_FOR {
            return None;
        }
        let phases = now.duration_since(self.blink_from).as_nanos() / BLINK.as_nanos() + 1;
        Some(self.blink_from + BLINK * phases as u32)
    }

    fn animating(&self, now: Instant, theme: &Theme) -> bool {
        let d = theme.metrics.duration;
        self.hover.animating(now, d) || self.focus.animating(now, d)
    }

    fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) {
        match event {
            Event::PointerEnter => self.hovered = true,
            Event::PointerLeave => self.hovered = false,
            Event::PointerDown(p) if self.shown_mask => {
                // on the bullets: their character is the text's
                let (x, y) = self.to_editor(*p);
                let at = self
                    .bullets
                    .as_ref()
                    .map_or(0, |l| self.to_real(Cursor::from_point(l, x, y).index()));
                let anchor = self.editor.raw_selection().anchor().index();
                let (fonts, layouts) = cx.text.contexts();
                let mut d = self.editor.driver(fonts, layouts);
                match cx.clicks {
                    1 if cx.modifiers.shift_key() => d.select_byte_range(anchor, at),
                    1 => d.select_byte_range(at, at),
                    _ => d.select_all(),
                }
                self.dragging = cx.clicks == 1;
                self.edited(cx);
            }
            Event::PointerMove(p) if self.dragging && self.shown_mask => {
                let (x, y) = self.to_editor(*p);
                let at = self
                    .bullets
                    .as_ref()
                    .map_or(0, |l| self.to_real(Cursor::from_point(l, x, y).index()));
                let anchor = self.editor.raw_selection().anchor().index();
                let (fonts, layouts) = cx.text.contexts();
                self.editor
                    .driver(fonts, layouts)
                    .select_byte_range(anchor, at);
                self.edited(cx);
            }
            Event::PointerDown(p) => {
                let (x, y) = self.to_editor(*p);
                let (fonts, layouts) = cx.text.contexts();
                let mut d = self.editor.driver(fonts, layouts);
                match cx.clicks {
                    1 if cx.modifiers.shift_key() => d.shift_click_extension(x, y),
                    1 => d.move_to_point(x, y),
                    2 => d.select_word_at_point(x, y),
                    _ => d.select_all(),
                }
                self.dragging = cx.clicks == 1;
                self.edited(cx);
            }
            Event::PointerMove(p) if self.dragging => {
                let (x, y) = self.to_editor(*p);
                let (fonts, layouts) = cx.text.contexts();
                self.editor
                    .driver(fonts, layouts)
                    .extend_selection_to_point(x, y);
                self.edited(cx);
            }
            Event::PointerUp(_) => self.dragging = false,
            // a textarea scrolls its lines; at its ends the scroll goes on to the page
            Event::Scroll(delta) if self.lines.is_some() => {
                let dy = match *delta {
                    ScrollDelta::Lines(v) => v.y * self.line * 3.0,
                    ScrollDelta::Pixels(v) => v.y,
                    ScrollDelta::Lifted => 0.0,
                };
                let to = (self.scroll_y + dy).clamp(0.0, self.max_scroll_y());
                if to != self.scroll_y {
                    self.scroll_y = to;
                    self.repaint = true;
                    cx.handled = true;
                }
            }
            Event::Focus { focused, visible } => {
                let gained = *focused && !self.focused;
                let left = !*focused && self.focused;
                self.focused = *focused;
                let (fonts, layouts) = cx.text.contexts();
                let mut d = self.editor.driver(fonts, layouts);
                if gained && *visible {
                    // from the keyboard: everything selected, ready to be replaced
                    d.select_all();
                }
                if !focused {
                    d.clear_compose();
                }
                self.edited(cx);
                if left && let Some(f) = &mut self.on_blur {
                    f();
                }
                if left
                    && std::mem::take(&mut self.caps)
                    && let Some(f) = &mut self.on_caps
                {
                    f(false);
                }
            }
            Event::KeyDown { text, .. } => {
                let press = cx.press(event).expect("a key press");
                if self.shown_mask {
                    self.guess_caps(&press);
                }
                if self.on_key.as_mut().is_some_and(|f| f(&press, cx)) {
                    cx.handled = true;
                } else if self.key(cx, &press, text.as_deref()) {
                    cx.handled = true;
                    self.edited(cx);
                }
            }
            Event::Ime(ime) => {
                let (fonts, layouts) = cx.text.contexts();
                let mut d = self.editor.driver(fonts, layouts);
                match ime {
                    Ime::Preedit { text, .. } if text.is_empty() => d.clear_compose(),
                    Ime::Preedit { text, cursor } => d.set_compose(text, *cursor),
                    Ime::Commit(text) => {
                        d.clear_compose();
                        d.insert_or_replace_selection(text);
                    }
                    Ime::DeleteSurrounding { before, after } => {
                        if let Some(n) = NonZeroUsize::new(*before) {
                            d.delete_bytes_before_selection(n);
                        }
                        if let Some(n) = NonZeroUsize::new(*after) {
                            d.delete_bytes_after_selection(n);
                        }
                    }
                }
                self.edited(cx);
            }
            Event::SetTextSelection(sel) => {
                let (anchor, focus) = (self.byte_at(&sel.anchor), self.byte_at(&sel.focus));
                if let (Some(anchor), Some(focus)) = (anchor, focus) {
                    let (fonts, layouts) = cx.text.contexts();
                    self.editor
                        .driver(fonts, layouts)
                        .select_byte_range(anchor, focus);
                    self.edited(cx);
                }
            }
            Event::Step(dir) => {
                if let Some(f) = &mut self.on_step {
                    f(*dir);
                }
            }
            Event::SetValue(v) => {
                let (fonts, layouts) = cx.text.contexts();
                let mut d = self.editor.driver(fonts, layouts);
                d.select_all();
                d.insert_or_replace_selection(v);
                self.edited(cx);
            }
            _ => {}
        }
        self.sync(cx.now, cx.theme);
    }

    fn accessibility(&self) -> Option<accesskit::Node> {
        let mut node = accesskit::Node::new(match (&self.spin, self.lines) {
            _ if self.mask.is_some() => accesskit::Role::PasswordInput,
            _ if self.combo.is_some() => accesskit::Role::EditableComboBox,
            (Some(_), _) => accesskit::Role::SpinButton,
            (None, Some(_)) => accesskit::Role::MultilineTextInput,
            (None, None) => accesskit::Role::TextInput,
        });
        if let Some(combo) = &self.combo {
            let c = combo();
            node.set_expanded(c.expanded);
            node.set_has_popup(accesskit::HasPopup::Listbox);
            if let Some(a) = c.active {
                node.set_active_descendant(a.node());
            }
        }
        if let Some(spin) = &self.spin {
            let s = spin();
            if let Some(v) = s.value {
                node.set_numeric_value(v);
            }
            if let Some(m) = s.min {
                node.set_min_numeric_value(m);
            }
            if let Some(m) = s.max {
                node.set_max_numeric_value(m);
            }
            node.set_numeric_value_step(s.step);
            node.add_action(accesskit::Action::Increment);
            node.add_action(accesskit::Action::Decrement);
        }
        // a hidden password reads as its bullets
        node.set_value(if self.shown_mask {
            self.bullet_text.clone()
        } else {
            self.synced.clone()
        });
        if !self.label.shown.is_empty() {
            node.set_label(self.label.shown.clone());
        }
        if let Some(f) = &self.field {
            node.push_labelled_by(f.label.node());
            if f.required {
                node.set_required();
            }
            if self.invalid {
                node.set_invalid(accesskit::Invalid::True);
            }
            if !self.description.is_empty() {
                node.set_description(self.description.clone());
            }
        }
        if !self.placeholder.shown.is_empty() {
            node.set_placeholder(self.placeholder.shown.clone());
        }
        node.set_bounds(accessible_rect(self.bounds));
        node.add_action(accesskit::Action::Focus);
        node.add_action(accesskit::Action::SetValue);
        node.add_action(accesskit::Action::SetTextSelection);
        let sel = self.editor.raw_selection();
        node.set_text_selection(accesskit::TextSelection {
            anchor: self.position_of(sel.anchor().index()),
            focus: self.position_of(sel.focus().index()),
        });
        Some(node)
    }

    /// The shown text as text runs: what screen readers read, and where each
    /// character is (for caret tracking and magnifiers)
    fn accessibility_children(&self) -> Vec<(accesskit::NodeId, accesskit::Node)> {
        let chars = self.characters();
        let text = self.shown_text();
        let o = self.text_origin();
        let mut prev_space = true;
        // an empty text: one empty run where the first line is
        let empty_line = (0.0, self.line as f32);
        runs(&chars)
            .into_iter()
            .zip(&self.run_ids)
            .map(|(range, id)| {
                let chunk = &chars[range];
                let mut node = accesskit::Node::new(accesskit::Role::TextRun);
                let value: String = chunk
                    .iter()
                    .map(|c| &text[c.start..c.start + c.len])
                    .collect();
                node.set_value(value);
                let x0 = chunk.first().map_or(0.0, |c| c.x);
                let x1 = chunk.last().map_or(0.0, |c| c.x + c.width);
                let (top, bottom) = chunk.first().map_or(empty_line, |c| (c.top, c.bottom));
                node.set_bounds(accessible_rect(Rect::new(
                    o.x + f64::from(x0),
                    o.y + f64::from(top),
                    o.x + f64::from(x1),
                    o.y + f64::from(bottom),
                )));
                node.set_character_lengths(chunk.iter().map(|c| c.len as u8).collect::<Vec<u8>>());
                node.set_character_positions(chunk.iter().map(|c| c.x - x0).collect::<Vec<f32>>());
                node.set_character_widths(chunk.iter().map(|c| c.width).collect::<Vec<f32>>());
                // a word starts after whitespace; trailing whitespace belongs to the word before
                let mut starts = Vec::new();
                for (i, c) in chunk.iter().enumerate() {
                    if !c.space && prev_space {
                        starts.push(i as u8);
                    }
                    prev_space = c.space;
                }
                node.set_word_starts(starts);
                (id.node(), node)
            })
            .collect()
    }

    fn paint(&self, cx: &mut PaintCx<'_>) {
        let (ctx, theme, now) = (&mut *cx.ctx, cx.theme, cx.now);
        let d = theme.metrics.duration;
        let (h, f) = (self.hover.value(now, d), self.focus.value(now, d));
        // invalid: a red edge that hovering doesn't change, a red ring when focused
        let (rest, focused) = if self.invalid {
            (
                theme.field_shadow_invalid,
                &theme.field_shadow_invalid_focus,
            )
        } else {
            (
                paint::lerp_shadows(&theme.field_shadow, &theme.field_shadow_hover, h),
                &theme.field_shadow_focus,
            )
        };
        let shadows = paint::lerp_shadows(&rest, focused, f);
        let (r, radius) = (self.bounds, theme.metrics.radius);
        paint::outer_shadows(ctx, r, radius, &shadows, 1.0);
        ctx.set_paint(theme.field_bg);
        ctx.fill_path(&paint::rounded(r, radius));
        paint::inset_shadows(ctx, r, radius, &shadows, 1.0);

        let Some(layout) = self.shown_layout() else {
            cx.paint_children(&self.trailing);
            return;
        };
        let origin = self.text_origin();
        ctx.push_clip_rect(&self.viewport());
        let at = |b: parley::BoundingBox| Rect::new(b.x0, b.y0, b.x1, b.y1) + origin.to_vec2();
        if self.focused {
            ctx.set_paint(theme.accent_soft);
            let sel = self.editor.raw_selection();
            let shown = Selection::new(
                Cursor::from_byte_index(
                    layout,
                    self.to_shown(sel.anchor().index()),
                    Affinity::Downstream,
                ),
                Cursor::from_byte_index(
                    layout,
                    self.to_shown(sel.focus().index()),
                    Affinity::Downstream,
                ),
            );
            for (b, _) in shown.geometry(layout) {
                ctx.fill_rect(&at(b));
            }
        }
        let empty = self.editor.raw_text().is_empty();
        match (&self.placeholder_layout, empty) {
            (Some(p), true) => text::draw(ctx, cx.res, p, origin, theme.text_3),
            _ => text::draw(ctx, cx.res, layout, origin, theme.text_1),
        }
        // the composition, underlined
        if let Some(range) = self.editor.raw_compose() {
            let sel = Selection::new(
                Cursor::from_byte_index(layout, range.start, Affinity::Downstream),
                Cursor::from_byte_index(layout, range.end, Affinity::Upstream),
            );
            ctx.set_paint(theme.text_1);
            for (b, _) in sel.geometry(layout) {
                let b = at(b);
                ctx.fill_rect(&Rect::new(b.x0, b.y1 - 2.0, b.x1, b.y1 - 1.0));
            }
        }
        let since = now.duration_since(self.blink_from);
        let caret_on =
            since >= BLINK_FOR || (since.as_millis() / BLINK.as_millis()).is_multiple_of(2);
        let caret = if self.shown_mask {
            self.caret_box()
        } else {
            self.editor
                .cursor_geometry(CARET_WIDTH)
                .map(|b| Rect::new(b.x0, b.y0, b.x1, b.y1))
        };
        if self.focused
            && caret_on
            && let Some(b) = caret
        {
            ctx.set_paint(theme.text_1);
            ctx.fill_rect(&(b + origin.to_vec2()));
        }
        ctx.pop_clip();
        // a textarea with more lines than it shows: a thin thumb at its end
        let max = self.max_scroll_y();
        if max > 0.0 {
            let view = self.viewport();
            let track = Rect::new(r.x1 - 6.0, view.y0, r.x1 - 3.0, view.y1);
            let h = (track.height() * view.height() / (view.height() + max)).max(16.0);
            let y = track.y0 + (track.height() - h) * self.scroll_y / max;
            ctx.set_paint(theme.text_3.multiply_alpha(0.45));
            ctx.fill_path(&paint::rounded(
                Rect::new(track.x0, y, track.x1, y + h),
                1.5,
            ));
        }
        cx.paint_children(&self.trailing);
    }
}

#[cfg(test)]
mod tests {
    use reactive_graph::traits::GetUntracked;
    use winit::keyboard::{ModifiersState, NativeKeyCode, PhysicalKey};

    use super::*;
    use crate::clipboard::Clipboard;
    use crate::theme::{Appearance, Scheme};
    use crate::widget::tests::lay_out;

    struct Harness {
        field: TextField,
        value: ArcRwSignal<String>,
        theme: Theme,
        text: Text,
        clipboard: Clipboard,
    }

    impl Harness {
        fn new(initial: &str) -> Self {
            Self::with(TextField::new(), initial, 40.0)
        }

        fn textarea(initial: &str) -> Self {
            Self::with(Textarea::new().rows(3), initial, 88.0)
        }

        fn with(field: TextField, initial: &str, height: f64) -> Self {
            let value = ArcRwSignal::new(initial.to_string());
            let mut field = field.value(value.clone());
            lay_out(&mut field, Size::new(300.0, height));
            let mut h = Self {
                field,
                value,
                theme: Theme::new(Scheme::Light, &Appearance::default()),
                text: Text::default(),
                clipboard: Clipboard::none(),
            };
            h.update();
            h.send(
                Event::Focus {
                    focused: true,
                    visible: false,
                },
                ModifiersState::empty(),
            );
            h
        }

        fn update(&mut self) {
            let inbox = crate::reactive::Inbox::new(|| {});
            let popups = crate::popup::PopupQueue::default();
            let mut cx = LayoutCx {
                theme: &self.theme,
                text: &mut self.text,
                now: Instant::now(),
                inbox: &inbox,
                popups: &popups,
            };
            self.field.update(&mut cx);
        }

        fn send(&mut self, event: Event, modifiers: ModifiersState) {
            let mut cx = EventCx {
                theme: &self.theme,
                now: Instant::now(),
                text: &mut self.text,
                clipboard: &mut self.clipboard,
                modifiers,
                clicks: 1,
                handled: false,
                focus_request: None,
                origin: None,
            };
            self.field.event(&mut cx, &event);
        }

        fn key(&mut self, key: Key, text: Option<&str>, modifiers: ModifiersState) {
            let text = text.map(str::to_string);
            self.send(
                Event::KeyDown {
                    plain: key.clone(),
                    key,
                    code: PhysicalKey::Unidentified(NativeKeyCode::Unidentified),
                    repeat: false,
                    text,
                },
                modifiers,
            );
        }

        fn typ(&mut self, s: &str) {
            for c in s.chars() {
                self.key(
                    Key::Character(c.to_string().into()),
                    Some(&c.to_string()),
                    ModifiersState::empty(),
                );
            }
        }

        fn named(&mut self, k: NamedKey, modifiers: ModifiersState) {
            self.key(Key::Named(k), None, modifiers);
        }
    }

    #[test]
    fn typing_and_editing_keys_publish_the_value() {
        let mut h = Harness::new("");
        h.typ("hello world");
        assert_eq!(h.value.get_untracked(), "hello world");
        h.named(NamedKey::Backspace, ModifiersState::CONTROL);
        assert_eq!(h.value.get_untracked(), "hello ");
        h.named(NamedKey::Home, ModifiersState::empty());
        h.named(NamedKey::Delete, ModifiersState::empty());
        assert_eq!(h.value.get_untracked(), "ello ");
        h.named(NamedKey::End, ModifiersState::SHIFT);
        h.typ("X");
        assert_eq!(
            h.value.get_untracked(),
            "X",
            "a selection is replaced by typing"
        );
        h.key(
            Key::Character("a".into()),
            Some("a"),
            ModifiersState::CONTROL,
        );
        h.typ("new");
        assert_eq!(h.value.get_untracked(), "new", "Ctrl+A selects everything");
    }

    #[test]
    fn control_characters_and_line_breaks_stay_out() {
        let mut h = Harness::new("a");
        h.key(
            Key::Named(NamedKey::Tab),
            Some("\t"),
            ModifiersState::empty(),
        );
        h.key(
            Key::Named(NamedKey::Enter),
            Some("\r"),
            ModifiersState::empty(),
        );
        assert_eq!(h.value.get_untracked(), "a");
    }

    #[test]
    fn a_value_set_from_outside_replaces_the_text() {
        let mut h = Harness::new("old");
        h.value.set("from outside".into());
        h.update();
        h.typ("!");
        assert_eq!(
            h.value.get_untracked(),
            "from outside!",
            "the caret goes to the end"
        );
    }

    #[test]
    fn composition_counts_only_once_committed() {
        let mut h = Harness::new("a");
        h.send(
            Event::Ime(Ime::Preedit {
                text: "かな".into(),
                cursor: Some((6, 6)),
            }),
            ModifiersState::empty(),
        );
        assert_eq!(h.value.get_untracked(), "a", "preedit is not the value yet");
        assert!(h.field.editor.is_composing());
        h.send(
            Event::Ime(Ime::Commit("仮名".into())),
            ModifiersState::empty(),
        );
        assert_eq!(h.value.get_untracked(), "a仮名");
        assert!(!h.field.editor.is_composing());
        h.send(
            Event::Ime(Ime::DeleteSurrounding {
                before: 3,
                after: 0,
            }),
            ModifiersState::empty(),
        );
        assert_eq!(h.value.get_untracked(), "a仮");
    }

    #[test]
    fn text_runs_cover_the_text_in_chunks_of_255() {
        let long: String = "word ".repeat(120); // 600 characters
        let mut h = Harness::new(&long);
        h.update();
        let runs = h.field.accessibility_children();
        assert_eq!(runs.len(), 3);
        let mut joined = String::new();
        for (_, n) in &runs {
            assert_eq!(n.role(), accesskit::Role::TextRun);
            let v = n.value().unwrap();
            let lengths: usize = n.character_lengths().iter().map(|&l| usize::from(l)).sum();
            assert_eq!(
                lengths,
                v.len(),
                "character lengths add up to the run's bytes"
            );
            assert!(n.character_lengths().len() <= MAX_RUN);
            joined.push_str(v);
        }
        assert_eq!(joined, long);
        assert_eq!(runs[0].1.word_starts()[..3], [0, 5, 10]);

        // the caret at the end sits at the end of the last run
        let node = h.field.accessibility().unwrap();
        let sel = node.text_selection().unwrap();
        assert_eq!(sel.focus.node, runs[2].0);
        assert_eq!(sel.focus.character_index, 600 - 2 * MAX_RUN);
        // and a position maps back to its byte
        let pos = accesskit::TextPosition {
            node: runs[1].0,
            character_index: 5,
        };
        assert_eq!(h.field.byte_at(&pos), Some(MAX_RUN + 5));
    }

    #[test]
    fn a_textarea_takes_line_breaks_and_moves_by_lines() {
        let submitted = std::rc::Rc::new(std::cell::Cell::new(false));
        let mut h = Harness::textarea("");
        let flag = submitted.clone();
        h.field.on_submit = Some(Box::new(move |_| flag.set(true)));
        h.typ("one");
        h.named(NamedKey::Enter, ModifiersState::empty());
        h.typ("two");
        assert_eq!(h.value.get_untracked(), "one\ntwo");
        assert!(!submitted.get(), "Enter starts a line");
        h.named(NamedKey::Enter, ModifiersState::CONTROL);
        assert!(submitted.get(), "Ctrl+Enter submits");

        // Up goes to the line above, Home and End stay on the line
        h.named(NamedKey::ArrowUp, ModifiersState::empty());
        h.named(NamedKey::End, ModifiersState::empty());
        h.typ("!");
        assert_eq!(h.value.get_untracked(), "one!\ntwo");
        h.named(NamedKey::ArrowDown, ModifiersState::empty());
        h.named(NamedKey::Home, ModifiersState::empty());
        h.typ(">");
        assert_eq!(h.value.get_untracked(), "one!\n>two");

        // pasted line breaks stay, as \n
        h.clipboard = Clipboard::fixed("a\r\nb");
        h.key(
            Key::Character("v".into()),
            Some("v"),
            ModifiersState::CONTROL,
        );
        assert_eq!(h.value.get_untracked(), "one!\n>a\nbtwo");
    }

    #[test]
    fn a_textarea_reads_line_by_line() {
        let mut h = Harness::textarea("first line\nsecond");
        h.update();
        let runs = h.field.accessibility_children();
        assert_eq!(runs.len(), 2, "a run per line");
        let joined: String = runs.iter().map(|(_, n)| n.value().unwrap()).collect();
        assert_eq!(joined, "first line\nsecond");
        let (a, b) = (runs[0].1.bounds().unwrap(), runs[1].1.bounds().unwrap());
        assert!(b.y0 >= a.y1 - 0.5, "the second line is below the first");
        assert_eq!(
            h.field.accessibility().unwrap().role(),
            accesskit::Role::MultilineTextInput
        );
    }
}

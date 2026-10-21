//! `TextField` – a single line of editable text in Kiesel's field well
//! (see `kiesel/src/components/TextField`).
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

use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use parley::Affinity;
use parley::editing::{Cursor, PlainEditor, Selection};
use parley::{FontFamily, FontWeight, LineHeight, StyleProperty};
use reactive_graph::signal::ArcRwSignal;
use reactive_graph::traits::{Set, Track, WithUntracked};
use vello_cpu::kurbo::{Point, Rect, Size};
use winit::cursor::CursorIcon;
use winit::keyboard::{Key, NamedKey};

use crate::anim::Transition;
use crate::i18n::{IntoText, TextProp};
use crate::paint;
use crate::reactive::{MountCx, Watch};
use crate::text::{self, Text};
use crate::theme::{TextRole, Theme, space};
use crate::widget::{
    Available, Change, Event, EventCx, Ime, Known, LayoutCx, PaintCx, Widget, WidgetId,
    accessible_rect,
};

/// Called with the text when Enter is pressed
type OnSubmit = Box<dyn FnMut(&str)>;

const BLINK: Duration = Duration::from_millis(500);
const BLINK_FOR: Duration = Duration::from_secs(10);
const CARET_WIDTH: f32 = 1.5;

pub struct TextField {
    id: WidgetId,
    /// Ids of its text-run nodes for assistive technology (one per 255 characters)
    run_ids: Vec<WidgetId>,
    value: ArcRwSignal<String>,
    watch: Option<Watch>,
    placeholder: TextProp,
    label: TextProp,
    grow: bool,
    placeholder_layout: Option<parley::Layout<()>>,
    editor: PlainEditor<()>,
    /// The editor holds `value` as of the last sync (no composition)
    synced: String,
    on_submit: Option<OnSubmit>,
    bounds: Rect,
    /// How far the text is scrolled left to keep the caret in view
    scroll: f64,
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
            grow: false,
            placeholder_layout: None,
            editor: PlainEditor::new(16.0),
            synced: String::new(),
            on_submit: None,
            bounds: Rect::ZERO,
            scroll: 0.0,
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

    /// Its accessible name – what a screen reader announces. (A `Field` that
    /// shows a label and names its control comes later; until then, set both.)
    pub fn label(mut self, text: impl IntoText) -> Self {
        self.label = TextProp::new(text);
        self
    }

    /// Take the free space along the parent's row (`flex: 1`)
    pub fn grow(mut self) -> Self {
        self.grow = true;
        self
    }

    /// Enter pressed
    pub fn on_submit(mut self, f: impl FnMut(&str) + 'static) -> Self {
        self.on_submit = Some(Box::new(f));
        self
    }

    /// Where the text starts, in window coordinates: the line centred in the well
    fn text_origin(&self) -> Point {
        Point::new(
            self.bounds.x0 + space(3) - self.scroll,
            self.bounds.y0 + (self.bounds.height() - self.line) / 2.0,
        )
    }

    /// The text box the field shows (inside its inline padding)
    fn viewport(&self) -> Rect {
        self.bounds.inflate(-space(3), 0.0)
    }

    /// The text without the composition in progress
    fn plain_text(&self) -> String {
        self.editor.text().to_string()
    }

    /// Lays out the editor and scrolls so the caret (or the composition) is visible
    fn refresh(&mut self, text: &mut Text) {
        let (fonts, layouts) = text.contexts();
        self.editor.driver(fonts, layouts).refresh_layout();
        let Some(layout) = self.editor.try_layout() else {
            return;
        };
        let width = f64::from(layout.width());
        let caret = self.editor.ime_cursor_area().x1;
        let view = self.viewport().width() - f64::from(CARET_WIDTH);
        if caret - self.scroll > view {
            self.scroll = caret - view;
        } else if caret < self.scroll {
            self.scroll = caret;
        }
        // no empty space after the text while scrolled
        self.scroll = self.scroll.min((width - view).max(0.0)).max(0.0);
    }

    /// After an edit: publish the value, restart the blink, repaint
    fn edited(&mut self, cx: &mut EventCx<'_>) {
        self.refresh(cx.text);
        let text = self.plain_text();
        if text != self.synced {
            self.synced = text.clone();
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
    fn key(&mut self, cx: &mut EventCx<'_>, key: &Key, typed: Option<&str>) -> bool {
        let (shift, ctrl) = (cx.modifiers.shift_key(), cx.modifiers.control_key());
        let (fonts, layouts) = cx.text.contexts();
        let mut d = self.editor.driver(fonts, layouts);
        match key {
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
            Key::Character(c) if ctrl => match c.as_str() {
                "a" => d.select_all(),
                "c" | "x" => {
                    if let Some(sel) = self.editor.selected_text().map(str::to_owned) {
                        cx.clipboard.set(&sel);
                        if c == "x" {
                            let (fonts, layouts) = cx.text.contexts();
                            self.editor.driver(fonts, layouts).delete_selection();
                        }
                    }
                }
                "v" => {
                    if let Some(pasted) = cx.clipboard.get() {
                        // one line: line breaks become spaces
                        let pasted = pasted.replace(['\r', '\n'], " ");
                        let (fonts, layouts) = cx.text.contexts();
                        self.editor
                            .driver(fonts, layouts)
                            .insert_or_replace_selection(&pasted);
                    }
                }
                _ => return false,
            },
            _ if !ctrl && !cx.modifiers.alt_key() => {
                let Some(typed) = typed.filter(|t| !t.chars().any(char::is_control)) else {
                    return false;
                };
                d.insert_or_replace_selection(typed);
            }
            _ => return false,
        }
        true
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
}

impl TextField {
    /// The characters of the text in text order – without a composition in
    /// progress, which isn't text yet (GTK doesn't expose it either)
    fn characters(&self) -> Vec<Char> {
        let Some(layout) = self.editor.try_layout() else {
            return Vec::new();
        };
        let compose = self.editor.raw_compose().clone().unwrap_or(0..0);
        let mut chars = Vec::new();
        for line in layout.lines() {
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
        let chars = self.characters();
        let i = chars
            .iter()
            .position(|c| c.start >= byte)
            .unwrap_or(chars.len());
        // the end of the text is the end of the last run, not the start of a new one
        let (run, index) = if i == chars.len() && i > 0 {
            ((i - 1) / MAX_RUN, (i - 1) % MAX_RUN + 1)
        } else {
            (i / MAX_RUN, i % MAX_RUN)
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
        let i = run * MAX_RUN + pos.character_index;
        Some(
            chars
                .get(i)
                .map_or(self.editor.raw_text().len(), |c| c.start),
        )
    }
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
        self.watch = Some(cx.watch(self.id, move || {
            value.track();
            label.track();
            placeholder.track();
        }));
    }

    /// Picks up a value set from outside (not by typing), and repaints after events
    fn update(&mut self, cx: &mut LayoutCx<'_>) -> Change {
        if self.editor.try_layout().is_none() {
            // first update: font settings, then the initial value
            let style = cx.theme.text_style(TextRole::Body);
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
        }
        // the placeholder: shaped at first, and again when its text changes
        if self.placeholder.refresh()
            || (self.placeholder_layout.is_none() && !self.placeholder.shown.is_empty())
        {
            let style = cx.theme.text_style(TextRole::Body);
            self.placeholder_layout = (!self.placeholder.shown.is_empty())
                .then(|| cx.text.line(&self.placeholder.shown, &style));
            self.repaint = true;
        }
        self.repaint |= self.label.refresh();
        if self.value.with_untracked(|v| *v != self.synced) {
            self.synced = self.value.with_untracked(String::clone);
            self.editor.set_text(&self.synced);
            let (fonts, layouts) = cx.text.contexts();
            self.editor.driver(fonts, layouts).move_to_text_end();
            self.repaint = true;
        }
        let (fonts, layouts) = cx.text.contexts();
        self.editor.driver(fonts, layouts).refresh_layout();
        let runs = self.characters().len().div_ceil(MAX_RUN).max(1);
        while self.run_ids.len() < runs {
            self.run_ids.push(WidgetId::next());
        }
        if std::mem::take(&mut self.repaint) {
            Change::PAINT
        } else {
            Change::NONE
        }
    }

    fn style(&self, _theme: &Theme) -> taffy::Style {
        taffy::Style {
            min_size: taffy::Size {
                width: taffy::LengthPercentageAuto::length(0.0),
                height: taffy::LengthPercentageAuto::auto(),
            },
            flex_grow: if self.grow { 1.0 } else { 0.0 },
            ..Default::default()
        }
    }

    /// As wide as offered (like a block-level input), 240 px on its own; `--ksl-control` high
    fn measure(&mut self, cx: &mut LayoutCx<'_>, known: Known, available: Available) -> Size {
        let width = known.width.unwrap_or(match available.width {
            taffy::AvailableSpace::MinContent => 80.0,
            taffy::AvailableSpace::MaxContent => 240.0,
            taffy::AvailableSpace::Definite(w) => w,
        });
        Size::new(f64::from(width), cx.theme.metrics.control)
    }

    fn set_bounds(&mut self, cx: &mut LayoutCx<'_>, bounds: Rect) {
        self.bounds = bounds;
        let style = cx.theme.text_style(TextRole::Body);
        self.line = f64::from(style.size * style.line_height);
        let (fonts, layouts) = cx.text.contexts();
        self.editor.driver(fonts, layouts).refresh_layout();
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

    fn accepts_text(&self) -> bool {
        true
    }

    fn cursor(&self) -> CursorIcon {
        CursorIcon::Text
    }

    fn ime_area(&self) -> Option<Rect> {
        let a = self.editor.ime_cursor_area();
        let r = Rect::new(a.x0, a.y0, a.x1, a.y1) + self.text_origin().to_vec2();
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
            Event::Focus { focused, visible } => {
                let gained = *focused && !self.focused;
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
            }
            Event::KeyDown { key, text, .. } => {
                if self.key(cx, key, text.as_deref()) {
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
        let mut node = accesskit::Node::new(accesskit::Role::TextInput);
        node.set_value(self.synced.clone());
        if !self.label.shown.is_empty() {
            node.set_label(self.label.shown.clone());
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
        let text = self.editor.raw_text();
        let o = self.text_origin();
        let mut prev_space = true;
        let chunks: Vec<&[Char]> = if chars.is_empty() {
            vec![&[]]
        } else {
            chars.chunks(MAX_RUN).collect()
        };
        chunks
            .into_iter()
            .zip(&self.run_ids)
            .map(|(chunk, id)| {
                let mut node = accesskit::Node::new(accesskit::Role::TextRun);
                let value: String = chunk
                    .iter()
                    .map(|c| &text[c.start..c.start + c.len])
                    .collect();
                node.set_value(value);
                let x0 = chunk.first().map_or(0.0, |c| c.x);
                let x1 = chunk.last().map_or(0.0, |c| c.x + c.width);
                node.set_bounds(accessible_rect(Rect::new(
                    o.x + f64::from(x0),
                    o.y,
                    o.x + f64::from(x1),
                    o.y + self.line,
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
        let shadows = paint::lerp_shadows(
            &paint::lerp_shadows(&theme.field_shadow, &theme.field_shadow_hover, h),
            &theme.field_shadow_focus,
            f,
        );
        let (r, radius) = (self.bounds, theme.metrics.radius);
        paint::outer_shadows(ctx, r, radius, &shadows, 1.0);
        ctx.set_paint(theme.field_bg);
        ctx.fill_path(&paint::rounded(r, radius));
        paint::inset_shadows(ctx, r, radius, &shadows, 1.0);

        let Some(layout) = self.editor.try_layout() else {
            return;
        };
        let origin = self.text_origin();
        ctx.push_clip_rect(&self.viewport());
        let at = |b: parley::BoundingBox| Rect::new(b.x0, b.y0, b.x1, b.y1) + origin.to_vec2();
        if self.focused {
            ctx.set_paint(theme.accent_soft);
            for (b, _) in self.editor.selection_geometry() {
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
        if self.focused
            && caret_on
            && let Some(b) = self.editor.cursor_geometry(CARET_WIDTH)
        {
            ctx.set_paint(theme.text_1);
            ctx.fill_rect(&at(b));
        }
        ctx.pop_clip();
    }
}

#[cfg(test)]
mod tests {
    use reactive_graph::traits::GetUntracked;
    use winit::keyboard::ModifiersState;

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
            let value = ArcRwSignal::new(initial.to_string());
            let mut field = TextField::new().value(value.clone());
            lay_out(&mut field, Size::new(300.0, 40.0));
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
            };
            self.field.event(&mut cx, &event);
        }

        fn key(&mut self, key: Key, text: Option<&str>, modifiers: ModifiersState) {
            let text = text.map(str::to_string);
            self.send(
                Event::KeyDown {
                    key,
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
}

//! The window shell: winit window, softbuffer surface, vello_cpu rendering,
//! layout, input routing, focus and accessibility.
//!
//! State reaches the widgets through signals (see [`crate::reactive`]): a change
//! queues the widgets that show it, and only those – and widgets that got
//! events – are asked what changed ([`Widget::update`]).
//!
//! Frames are drawn on demand and only where something changed. Widgets report
//! changes from [`Widget::update`] (and layout moves them); the shell collects
//! their inks into a damage rectangle and redraws just that: a small region is
//! rendered into its own pixmap and copied into softbuffer's buffer, a large one
//! straight into the buffer. Since softbuffer cycles buffers, the region also
//! covers what changed in the frames since this buffer was last shown (its age).
//! While something animates, each frame asks for the next one, paced by the
//! compositor's frame callbacks; one more frame follows the end of an animation
//! so transitions settle on their final value.
//!
//! Input: the pointer goes to the deepest widget under it; a press captures it
//! until release (only the captured widget counts as hovered meanwhile). Keys
//! go to the focused widget; Tab and Shift+Tab move focus in document order.
//! Focus from the keyboard or assistive technology is visible (`:focus-visible`),
//! focus from a pointer press is not – until the next key press.

use std::collections::{HashMap, VecDeque};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Instant;

use raw_window_handle::HasDisplayHandle;
use softbuffer::{Context, Surface};
use vello_cpu::kurbo::{Affine, Point, Rect, Size};
use vello_cpu::{Pixmap, RenderContext, Resources};
use winit::application::ApplicationHandler;
use winit::cursor::{Cursor, CursorIcon};
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::event::{
    ButtonSource, ElementState, Ime as WinitIme, KeyEvent, MouseButton, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, OwnedDisplayHandle};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{
    ImeCapabilities, ImeEnableRequest, ImeRequest, ImeRequestData, Theme as WinitTheme, Window,
    WindowAttributes, WindowId,
};

use crate::a11y::Accessibility;
use crate::clipboard::Clipboard;
use crate::layout::Layout;
use crate::reactive::{Inbox, MountCx};
use crate::text::Text;
use crate::theme::{Appearance, Scheme, Theme};
use crate::widget::{
    self, Event, EventCx, Ime, LayoutCx, PaintCx, Widget, WidgetId, accessibility_nodes,
};

pub struct Options {
    pub title: String,
    pub size: Size,
    pub appearance: Appearance,
}

/// Regions above this share of the window are rendered straight into the buffer
const PARTIAL_MAX: f64 = 0.5;
/// Damage of this many past frames is kept for buffer ages
const HISTORY: usize = 4;

pub fn run(
    options: Options,
    root: impl Widget + 'static,
) -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    let proxy = event_loop.create_proxy();
    let inbox = Inbox::new(move || proxy.wake_up());
    let mut root: Box<dyn Widget> = Box::new(root);
    widget::mount_all(root.as_mut(), &mut MountCx { inbox: &inbox });
    // KIESEL_THEME=light|dark overrides the system preference (tests, screenshots)
    let forced = match std::env::var("KIESEL_THEME").as_deref() {
        Ok("dark") => Some(Scheme::Dark),
        Ok("light") => Some(Scheme::Light),
        _ => None,
    };
    let scheme = forced.unwrap_or(Scheme::Light);
    let app = Shell {
        theme: Theme::new(scheme, &options.appearance),
        forced,
        options,
        root,
        inbox,
        touched: Vec::new(),
        update_everything: true,
        text: Text::default(),
        layout: Layout::default(),
        needs_layout: true,
        window: None,
        render: None,
        partial: None,
        res: Resources::new(),
        damage: None,
        full_damage: true,
        history: VecDeque::new(),
        buffer_size: (0, 0),
        animating: Vec::new(),
        cursor: CursorIcon::Default,
        stats: std::env::var_os("KIESEL_STATS").is_some(),
        // KIESEL_FULL_REDRAW=1 redraws everything every frame (to compare against)
        always_full: std::env::var_os("KIESEL_FULL_REDRAW").is_some(),
        pointer: None,
        hovered: None,
        captured: None,
        focused: None,
        focus_visible: false,
        modifiers: ModifiersState::empty(),
        last_press: None,
        clicks: 1,
        clipboard: Clipboard::none(),
        wake: None,
        ime_area: None,
        a11y: None,
    };
    event_loop.run_app(app)?;
    Ok(())
}

struct Shell {
    options: Options,
    theme: Theme,
    forced: Option<Scheme>,
    root: Box<dyn Widget>,
    inbox: Arc<Inbox>,
    /// Widgets that got events since the last update
    touched: Vec<WidgetId>,
    /// Ask every widget, not just the touched and queued ones (start, theme change)
    update_everything: bool,
    text: Text,
    layout: Layout,
    /// Everything must be laid out again (size, scale or theme changed)
    needs_layout: bool,
    window: Option<Surface<OwnedDisplayHandle, Box<dyn Window>>>,
    /// Context for frames rendered straight into the buffer
    render: Option<RenderContext>,
    /// Context and pixmap for small regions
    partial: Option<(RenderContext, Pixmap)>,
    res: Resources,
    /// What changed since the last frame (logical)
    damage: Option<Rect>,
    full_damage: bool,
    /// Damage of the last frames (pixels), newest last
    history: VecDeque<Rect>,
    buffer_size: (u16, u16),
    /// Inks of the widgets that animated in the last frame
    animating: Vec<Rect>,
    cursor: CursorIcon,
    stats: bool,
    always_full: bool,
    /// Last pointer position (logical)
    pointer: Option<Point>,
    hovered: Option<WidgetId>,
    captured: Option<WidgetId>,
    focused: Option<WidgetId>,
    focus_visible: bool,
    modifiers: ModifiersState,
    /// When and where the last press was (for double and triple clicks)
    last_press: Option<(Instant, Point)>,
    clicks: u8,
    clipboard: Clipboard,
    /// The time the loop sleeps until (see `about_to_wait`)
    wake: Option<Instant>,
    /// The input method is on, with its candidate window at this area
    ime_area: Option<Rect>,
    a11y: Option<Accessibility>,
}

/// Presses this close in time and space count as one double or triple click (GTK's defaults)
const MULTI_CLICK_TIME: std::time::Duration = std::time::Duration::from_millis(400);
const MULTI_CLICK_DISTANCE: f64 = 5.0;

impl Shell {
    fn scale(&self) -> f64 {
        self.window
            .as_ref()
            .map_or(1.0, |s| s.window().scale_factor())
    }

    fn logical_size(&self) -> Size {
        let Some(s) = &self.window else {
            return self.options.size;
        };
        let px = s.window().surface_size();
        Size::new(f64::from(px.width), f64::from(px.height)) / self.scale()
    }

    fn request_redraw(&self) {
        if let Some(s) = &self.window {
            s.window().request_redraw();
        }
    }

    fn add_damage(&mut self, r: Rect) {
        if r.is_zero_area() {
            return;
        }
        self.damage = Some(self.damage.map_or(r, |d| d.union(r)));
    }

    /// Everything changes: new size, scale or theme
    fn invalidate_all(&mut self) {
        self.needs_layout = true;
        self.full_damage = true;
        self.update_everything = true;
    }

    /// Sends `event` to the widget `id`; false if it no longer exists
    fn send(&mut self, id: WidgetId, event: Event) -> bool {
        let mut cx = EventCx {
            theme: &self.theme,
            now: Instant::now(),
            text: &mut self.text,
            clipboard: &mut self.clipboard,
            modifiers: self.modifiers,
            clicks: self.clicks,
        };
        match widget::find_mut(self.root.as_mut(), id) {
            Some(w) => {
                w.event(&mut cx, &event);
                if !self.touched.contains(&id) {
                    self.touched.push(id);
                }
                true
            }
            None => false,
        }
    }

    /// Asks the widgets that got events or whose signals changed what changed;
    /// returns the ids needing layout
    fn collect_updates(&mut self) -> Vec<WidgetId> {
        let (mut relayout, mut repaint) = (Vec::new(), Vec::new());
        let mut cx = LayoutCx {
            theme: &self.theme,
            text: &mut self.text,
            now: Instant::now(),
        };
        let mut ids = std::mem::take(&mut self.touched);
        ids.extend(self.inbox.take());
        if std::mem::take(&mut self.update_everything) {
            widget::update_all(self.root.as_mut(), &mut cx, &mut relayout, &mut repaint);
        } else {
            for id in ids {
                let Some(w) = widget::find_mut(self.root.as_mut(), id) else {
                    continue;
                };
                let change = w.update(&mut cx);
                if change.layout {
                    relayout.push(id);
                }
                if change.paint {
                    repaint.push(w.ink());
                }
            }
        }
        for r in repaint {
            self.add_damage(r);
        }
        relayout
    }

    /// Lays out again; whatever moved or resized is damaged where it was and where it is
    fn relayout(&mut self, dirty: &[WidgetId]) {
        let (mut before, mut after) = (HashMap::new(), HashMap::new());
        widget::inks(self.root.as_ref(), &mut before);
        let size = self.logical_size();
        let mut cx = LayoutCx {
            theme: &self.theme,
            text: &mut self.text,
            now: Instant::now(),
        };
        self.layout.run(self.root.as_mut(), &mut cx, size, dirty);
        widget::inks(self.root.as_ref(), &mut after);
        for (id, ink) in &after {
            let old = before.remove(id);
            if old != Some(*ink) {
                self.add_damage(*ink);
                if let Some(old) = old {
                    self.add_damage(old);
                }
            }
        }
        for gone in before.into_values() {
            self.add_damage(gone);
        }
        self.needs_layout = false;
    }

    /// After input or a size change: pick up changes (signals, widget state),
    /// lay out if needed, update hover, cursor and the accessibility tree,
    /// and ask for a frame if anything needs drawing
    fn changed(&mut self) {
        let dirty = self.collect_updates();
        if self.needs_layout || !dirty.is_empty() {
            self.relayout(&dirty);
        }
        if let Some(p) = self.pointer {
            // the widget under a resting pointer can change with layout
            self.update_hover(p);
            self.collect_updates();
        }
        let cursor = self
            .hovered
            .and_then(|id| widget::find(self.root.as_ref(), id))
            .map_or(CursorIcon::Default, |w| w.cursor());
        if cursor != self.cursor
            && let Some(s) = &self.window
        {
            self.cursor = cursor;
            s.window().set_cursor(Cursor::Icon(cursor));
        }
        self.update_accessibility();
        self.update_ime();
        if self.damage.is_some() || self.full_damage {
            self.request_redraw();
        }
    }

    /// The input method is on while a widget that takes text is focused, its
    /// candidate window next to the caret
    fn update_ime(&mut self) {
        let Some(s) = &self.window else { return };
        let area = self
            .focused
            .and_then(|id| widget::find(self.root.as_ref(), id))
            .filter(|w| w.accepts_text())
            .map(|w| w.ime_area().unwrap_or(w.bounds()));
        if area == self.ime_area {
            return;
        }
        let window = s.window();
        match area {
            Some(a) => {
                let data = ImeRequestData::default().with_cursor_area(
                    LogicalPosition::new(a.x0, a.y0).into(),
                    LogicalSize::new(a.width().max(1.0), a.height().max(1.0)).into(),
                );
                let request = if self.ime_area.is_some() {
                    ImeRequest::Update(data)
                } else {
                    let caps = ImeCapabilities::new().with_cursor_area();
                    match ImeEnableRequest::new(caps, data) {
                        Some(r) => ImeRequest::Enable(r),
                        None => return,
                    }
                };
                // without an input method there is nothing to update; typing still works
                let _ = window.request_ime_update(request);
            }
            None => {
                let _ = window.request_ime_update(ImeRequest::Disable);
            }
        }
        self.ime_area = area;
    }

    fn set_focus(&mut self, id: Option<WidgetId>, visible: bool) {
        if id == self.focused && visible == self.focus_visible {
            return;
        }
        if let Some(old) = self.focused
            && Some(old) != id
        {
            self.send(
                old,
                Event::Focus {
                    focused: false,
                    visible: false,
                },
            );
        }
        self.focused = id;
        self.focus_visible = visible && id.is_some();
        if let Some(new) = id
            && !self.send(
                new,
                Event::Focus {
                    focused: true,
                    visible,
                },
            )
        {
            self.focused = None;
        }
    }

    /// Tab order, wrapping around
    fn move_focus(&mut self, backwards: bool) {
        let order = widget::focus_order(self.root.as_ref());
        if order.is_empty() {
            return;
        }
        let pos = self
            .focused
            .and_then(|f| order.iter().position(|&id| id == f));
        let next = match (pos, backwards) {
            (None, false) => 0,
            (None, true) => order.len() - 1,
            (Some(i), false) => (i + 1) % order.len(),
            (Some(i), true) => (i + order.len() - 1) % order.len(),
        };
        self.set_focus(Some(order[next]), true);
    }

    fn update_hover(&mut self, p: Point) {
        let mut target = widget::hit_test(self.root.as_ref(), p);
        if self.captured.is_some() && target != self.captured {
            target = None;
        }
        if target != self.hovered {
            if let Some(old) = self.hovered {
                self.send(old, Event::PointerLeave);
            }
            self.hovered = target;
            if let Some(new) = target {
                self.send(new, Event::PointerEnter);
            }
        }
    }

    fn pointer_move(&mut self, p: Point) {
        self.pointer = Some(p);
        self.update_hover(p);
        if let Some(target) = self.captured.or(self.hovered) {
            self.send(target, Event::PointerMove(p));
        }
        self.changed();
    }

    fn pointer_leave(&mut self) {
        self.pointer = None;
        if self.captured.is_none()
            && let Some(old) = self.hovered.take()
        {
            self.send(old, Event::PointerLeave);
        }
        self.changed();
    }

    fn pointer_down(&mut self, p: Point) {
        let now = Instant::now();
        let repeat = self.last_press.is_some_and(|(t, at)| {
            now.duration_since(t) <= MULTI_CLICK_TIME && at.distance(p) <= MULTI_CLICK_DISTANCE
        });
        self.clicks = if repeat { self.clicks % 3 + 1 } else { 1 };
        self.last_press = Some((now, p));
        self.pointer = Some(p);
        self.update_hover(p);
        let target = self.hovered;
        self.captured = target;
        // a press focuses what it hits (if focusable), or nothing – without a ring
        let focusable = target
            .and_then(|id| widget::find(self.root.as_ref(), id))
            .is_some_and(|w| w.focusable());
        self.set_focus(target.filter(|_| focusable), false);
        if let Some(target) = target {
            self.send(target, Event::PointerDown(p));
        }
        self.changed();
    }

    fn pointer_up(&mut self, p: Point) {
        if let Some(target) = self.captured.take().or(self.hovered) {
            self.send(target, Event::PointerUp(p));
        }
        self.update_hover(p);
        self.changed();
    }

    fn key(&mut self, event: KeyEvent) {
        let down = event.state == ElementState::Pressed;
        if down && matches!(event.logical_key, Key::Named(NamedKey::Tab)) {
            self.move_focus(self.modifiers.shift_key());
            self.changed();
            return;
        }
        let Some(focused) = self.focused else { return };
        if down
            && !matches!(event.logical_key, Key::Named(k) if is_modifier(k))
            && !self.focus_visible
        {
            // keyboard interaction makes the focus visible (the :focus-visible heuristic)
            self.focus_visible = true;
            self.send(
                focused,
                Event::Focus {
                    focused: true,
                    visible: true,
                },
            );
        }
        let ev = if down {
            Event::KeyDown {
                key: event.logical_key,
                repeat: event.repeat,
                text: event.text.map(|t| t.to_string()),
            }
        } else {
            Event::KeyUp {
                key: event.logical_key,
            }
        };
        self.send(focused, ev);
        self.changed();
    }

    fn accessibility_action(&mut self, request: accesskit::ActionRequest) {
        let Some(id) = WidgetId::from_node(request.target_node) else {
            return;
        };
        match request.action {
            accesskit::Action::Click => {
                self.send(id, Event::Activate);
            }
            accesskit::Action::SetTextSelection => {
                if let Some(accesskit::ActionData::SetTextSelection(sel)) = request.data {
                    self.send(id, Event::SetTextSelection(sel));
                }
            }
            accesskit::Action::SetValue => {
                if let Some(accesskit::ActionData::Value(v)) = request.data {
                    self.send(id, Event::SetValue(v.into()));
                }
            }
            accesskit::Action::Focus => {
                let focusable = widget::find(self.root.as_ref(), id).is_some_and(|w| w.focusable());
                if focusable {
                    self.set_focus(Some(id), true);
                }
            }
            _ => return,
        }
        self.changed();
    }

    fn update_accessibility(&mut self) {
        let scale = self.scale();
        let Some(a11y) = &mut self.a11y else { return };
        let (root, title, focus) = (self.root.as_ref(), &self.options.title, self.focused);
        a11y.update(|| {
            let mut nodes = Vec::new();
            let children = accessibility_nodes(root, &mut nodes);
            let mut window = accesskit::Node::new(accesskit::Role::Window);
            window.set_label(title.clone());
            window.set_children(children);
            // node bounds are logical; the window maps them to pixels
            window.set_transform(accesskit::Affine::scale(scale));
            nodes.push((WidgetId::WINDOW.node(), window));
            accesskit::TreeUpdate {
                nodes,
                tree: Some(accesskit::TreeInfo {
                    root: WidgetId::WINDOW.node(),
                    toolkit_name: Some("Kiesel".into()),
                    toolkit_version: Some(env!("CARGO_PKG_VERSION").into()),
                }),
                tree_id: accesskit::TreeId::ROOT,
                focus: focus.unwrap_or(WidgetId::WINDOW).node(),
            }
        });
    }

    fn set_scheme(&mut self, theme: Option<WinitTheme>) {
        let scheme = self.forced.unwrap_or(match theme {
            Some(WinitTheme::Dark) => Scheme::Dark,
            _ => Scheme::Light,
        });
        if scheme != self.theme.scheme {
            self.theme = Theme::new(scheme, &self.options.appearance);
            self.invalidate_all();
        }
    }

    fn redraw(&mut self) {
        let start = Instant::now();
        let now = Instant::now();
        // animating widgets need their ink redrawn – and those that animated
        // last frame once more, so a transition that just ended shows its end
        let mut animating = Vec::new();
        widget::animating_inks(self.root.as_ref(), now, &self.theme, &mut animating);
        for r in std::mem::replace(&mut self.animating, animating.clone()) {
            self.add_damage(r);
        }
        for &r in &animating {
            self.add_damage(r);
        }

        let scale = self.scale();
        let Some(surface) = self.window.as_mut() else {
            return;
        };
        let px = surface.window().surface_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(px.width), NonZeroU32::new(px.height)) else {
            return;
        };
        let (w16, h16) = (
            u16::try_from(px.width).unwrap_or(u16::MAX),
            u16::try_from(px.height).unwrap_or(u16::MAX),
        );
        surface.resize(w, h).expect("resizing the surface");
        let whole = Rect::new(0.0, 0.0, f64::from(w16), f64::from(h16));
        if self.buffer_size != (w16, h16) {
            // the buffers were resized: earlier damage means nothing any more
            self.buffer_size = (w16, h16);
            self.history.clear();
            self.full_damage = true;
        }

        // this frame's damage in pixels: outward to whole pixels, plus one for antialiasing
        let frame = if self.full_damage || self.always_full {
            Some(whole)
        } else {
            self.damage.map(|d| {
                let d = d.scale_from_origin(scale).inflate(1.0, 1.0);
                Rect::new(d.x0.floor(), d.y0.floor(), d.x1.ceil(), d.y1.ceil()).intersect(whole)
            })
        };
        self.damage = None;
        self.full_damage = false;
        let Some(frame) = frame.filter(|f| f.area() > 0.0) else {
            if !animating.is_empty() || !self.animating.is_empty() {
                surface.window().request_redraw();
            }
            return;
        };

        surface.window().pre_present_notify();
        let mut buffer = surface.buffer_mut().expect("softbuffer buffer");
        // the buffer still shows the frame from `age` presents ago: redraw what changed since
        let age = usize::from(buffer.age());
        let region = if age == 0 || age - 1 > self.history.len() {
            whole
        } else {
            self.history
                .iter()
                .rev()
                .take(age - 1)
                .fold(frame, |r, h| r.union(*h))
                .intersect(whole)
        };
        self.history.push_back(frame);
        while self.history.len() > HISTORY {
            self.history.pop_front();
        }

        let partial = region.area() <= whole.area() * PARTIAL_MAX;
        let logical_region = region.scale_from_origin(1.0 / scale);
        let theme = &self.theme;
        let paint =
            |ctx: &mut RenderContext, res: &mut Resources, root: &dyn Widget, offset: Point| {
                ctx.reset();
                ctx.set_transform(Affine::translate(-offset.to_vec2()) * Affine::scale(scale));
                ctx.set_paint(theme.bg);
                ctx.fill_rect(&logical_region);
                root.paint(&mut PaintCx {
                    ctx,
                    res,
                    theme,
                    now,
                    clip: logical_region,
                });
                ctx.flush();
            };
        if partial {
            let (rw, rh) = (region.width() as u16, region.height() as u16);
            let (ctx, pixmap) = self
                .partial
                .get_or_insert_with(|| (RenderContext::new(rw, rh), Pixmap::new(rw, rh)));
            if ctx.width() != rw || ctx.height() != rh {
                ctx.reset_and_resize(rw, rh);
                pixmap.resize(rw, rh);
            }
            paint(ctx, &mut self.res, self.root.as_ref(), region.origin());
            ctx.render(&mut *pixmap, &mut self.res);
            // copy into the buffer: RGBA bytes → 0RGB words
            let src = pixmap.data_as_u8_slice();
            let (x0, y0, stride) = (region.x0 as usize, region.y0 as usize, usize::from(w16));
            for (row, line) in src.chunks_exact(usize::from(rw) * 4).enumerate() {
                let dst = &mut buffer[(y0 + row) * stride + x0..][..usize::from(rw)];
                for (out, px) in dst.iter_mut().zip(line.as_chunks::<4>().0) {
                    *out = u32::from(px[0]) << 16 | u32::from(px[1]) << 8 | u32::from(px[2]);
                }
            }
        } else {
            let ctx = match &mut self.render {
                Some(ctx) if ctx.width() == w16 && ctx.height() == h16 => ctx,
                slot => slot.insert(RenderContext::new(w16, h16)),
            };
            paint(ctx, &mut self.res, self.root.as_ref(), Point::ZERO);
            let bytes: &mut [u8] = bytemuck::cast_slice_mut(&mut buffer[..]);
            let len = usize::from(w16) * usize::from(h16) * 4;
            let target =
                vello_cpu::PixmapMut::new(w16, h16, &mut bytes[..len]).expect("buffer size");
            ctx.render(target, &mut self.res);
            for px in bytes[..len].as_chunks_mut::<4>().0 {
                px.swap(0, 2);
            }
        }
        let damage = softbuffer::Rect {
            x: frame.x0 as u32,
            y: frame.y0 as u32,
            width: NonZeroU32::new(frame.width() as u32).expect("non-empty"),
            height: NonZeroU32::new(frame.height() as u32).expect("non-empty"),
        };
        buffer.present_with_damage(&[damage]).expect("presenting");

        if self.stats {
            eprintln!(
                "frame {}x{} @{scale}: {} {}x{} ({:.1} %, age {age}) {:.2} ms",
                px.width,
                px.height,
                if partial { "partial" } else { "full" },
                region.width(),
                region.height(),
                100.0 * region.area() / whole.area(),
                start.elapsed().as_secs_f64() * 1e3
            );
        }
        if !animating.is_empty() || !self.animating.is_empty() {
            surface.window().request_redraw();
        }
    }
}

fn is_modifier(key: NamedKey) -> bool {
    matches!(
        key,
        NamedKey::Shift | NamedKey::Control | NamedKey::Alt | NamedKey::Meta | NamedKey::AltGraph
    )
}

impl ApplicationHandler for Shell {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        let attrs = WindowAttributes::default()
            .with_title(self.options.title.clone())
            .with_surface_size(winit::dpi::LogicalSize::new(
                self.options.size.width,
                self.options.size.height,
            ))
            .with_min_surface_size(winit::dpi::LogicalSize::new(160.0, 100.0));
        let window = event_loop
            .create_window(attrs)
            .expect("creating the window");
        let theme = window.theme();
        let context = Context::new(event_loop.owned_display_handle()).expect("softbuffer context");
        self.window = Some(Surface::new(&context, window).expect("softbuffer surface"));
        if let Ok(display) = event_loop.owned_display_handle().display_handle() {
            self.clipboard = Clipboard::new(display.as_raw());
        }
        self.a11y = Some(Accessibility::new(event_loop.create_proxy()));
        self.set_scheme(theme);
        self.invalidate_all();
        self.changed();
    }

    /// Before the loop sleeps: if the time we woke up for has come, redraw the
    /// widgets that asked for it (a blinking caret); then sleep until the next one
    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        let now = Instant::now();
        let (mut next, mut inks) = (None, Vec::new());
        widget::wakes(self.root.as_ref(), now, &mut next, &mut inks);
        if self.wake.is_some_and(|t| t <= now) {
            for r in inks {
                self.add_damage(r);
            }
            self.request_redraw();
        }
        self.wake = next;
        event_loop.set_control_flow(next.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }

    fn proxy_wake_up(&mut self, _: &dyn ActiveEventLoop) {
        let requests = self
            .a11y
            .as_ref()
            .map(Accessibility::take_requests)
            .unwrap_or_default();
        for request in requests {
            self.accessibility_action(request);
        }
        // signals changed (perhaps on another thread)
        self.changed();
    }

    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let scale = self.scale();
        let logical = |p: winit::dpi::PhysicalPosition<f64>| Point::new(p.x / scale, p.y / scale);
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::SurfaceResized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                self.invalidate_all();
                self.changed();
            }
            WindowEvent::ThemeChanged(theme) => {
                self.set_scheme(Some(theme));
                self.changed();
            }
            WindowEvent::Focused(focused) => {
                if let Some(a11y) = &mut self.a11y {
                    a11y.window_focused(focused);
                }
            }
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::Ime(ime) => {
                let ime = match ime {
                    WinitIme::Preedit(text, cursor) => Ime::Preedit { text, cursor },
                    WinitIme::Commit(text) => Ime::Commit(text),
                    WinitIme::DeleteSurrounding {
                        before_bytes,
                        after_bytes,
                    } => Ime::DeleteSurrounding {
                        before: before_bytes,
                        after: after_bytes,
                    },
                    _ => return,
                };
                if let Some(focused) = self.focused {
                    self.send(focused, Event::Ime(ime));
                    self.changed();
                }
            }
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } => self.key(event),
            WindowEvent::PointerMoved { position, .. }
            | WindowEvent::PointerEntered { position, .. } => self.pointer_move(logical(position)),
            WindowEvent::PointerLeft { .. } => self.pointer_leave(),
            WindowEvent::PointerButton {
                state,
                position,
                button,
                ..
            } => {
                let primary = button.clone().mouse_button() == Some(MouseButton::Left)
                    || matches!(button, ButtonSource::Touch { .. });
                if primary {
                    let p = logical(position);
                    if state == ElementState::Pressed {
                        self.pointer_down(p);
                    } else {
                        self.pointer_up(p);
                    }
                }
            }
            _ => {}
        }
    }
}

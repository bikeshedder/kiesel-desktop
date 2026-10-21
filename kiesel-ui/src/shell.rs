//! The window shell: winit window, softbuffer surface, vello_cpu rendering,
//! input routing, focus and accessibility.
//!
//! vello_cpu renders straight into softbuffer's buffer (no second full-size
//! pixmap) and the channels are swapped in place: RGBA bytes → 0RGB words.
//! Frames are only drawn on demand – while something animates, each frame asks
//! for the next one, paced by the compositor's frame callbacks.
//!
//! Input: the pointer goes to the deepest widget under it; a press captures it
//! until release (only the captured widget counts as hovered meanwhile). Keys
//! go to the focused widget; Tab and Shift+Tab move focus in document order.
//! Focus from the keyboard or assistive technology is visible (`:focus-visible`),
//! focus from a pointer press is not – until the next key press.

use std::num::NonZeroU32;
use std::time::Instant;

use softbuffer::{Context, Surface};
use vello_cpu::kurbo::{Affine, Point, Rect, Size};
use vello_cpu::{RenderContext, Resources};
use winit::application::ApplicationHandler;
use winit::cursor::{Cursor, CursorIcon};
use winit::event::{ButtonSource, ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, OwnedDisplayHandle};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Theme as WinitTheme, Window, WindowAttributes, WindowId};

use crate::a11y::Accessibility;
use crate::text::Text;
use crate::theme::{Appearance, Scheme, Theme};
use crate::widget::{
    self, Event, EventCx, LayoutCx, PaintCx, Widget, WidgetId, accessibility_nodes,
};

pub struct Options {
    pub title: String,
    pub size: Size,
    pub appearance: Appearance,
}

pub fn run(
    options: Options,
    root: impl Widget + 'static,
) -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
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
        root: Box::new(root),
        text: Text::default(),
        window: None,
        render: None,
        res: Resources::new(),
        cursor: CursorIcon::Default,
        stats: std::env::var_os("KIESEL_STATS").is_some(),
        pointer: None,
        hovered: None,
        captured: None,
        focused: None,
        focus_visible: false,
        modifiers: ModifiersState::empty(),
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
    text: Text,
    window: Option<Surface<OwnedDisplayHandle, Box<dyn Window>>>,
    render: Option<RenderContext>,
    res: Resources,
    cursor: CursorIcon,
    stats: bool,
    /// Last pointer position (logical)
    pointer: Option<Point>,
    hovered: Option<WidgetId>,
    captured: Option<WidgetId>,
    focused: Option<WidgetId>,
    focus_visible: bool,
    modifiers: ModifiersState,
    a11y: Option<Accessibility>,
}

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

    fn layout(&mut self) {
        let bounds = Rect::from_origin_size(Point::ZERO, self.logical_size());
        let mut cx = LayoutCx {
            theme: &self.theme,
            text: &mut self.text,
            now: Instant::now(),
        };
        self.root.place(&mut cx, bounds);
    }

    /// Sends `event` to the widget `id`; false if it no longer exists
    fn send(&mut self, id: WidgetId, event: Event) -> bool {
        let mut cx = EventCx {
            theme: &self.theme,
            now: Instant::now(),
        };
        match widget::find_mut(self.root.as_mut(), id) {
            Some(w) => {
                w.event(&mut cx, &event);
                true
            }
            None => false,
        }
    }

    /// After input: state may have changed anywhere (signals), so lay out,
    /// update the cursor and the accessibility tree, and draw
    fn changed(&mut self) {
        self.layout();
        if let Some(p) = self.pointer {
            // the widget under a resting pointer can change with layout
            self.update_hover(p);
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
        self.request_redraw();
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
            self.layout();
        }
    }

    fn redraw(&mut self) {
        let start = Instant::now();
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

        let ctx = match &mut self.render {
            Some(ctx) if ctx.width() == w16 && ctx.height() == h16 => ctx,
            slot => slot.insert(RenderContext::new(w16, h16)),
        };
        ctx.reset();
        ctx.set_transform(Affine::scale(scale));
        ctx.set_paint(self.theme.bg);
        ctx.fill_rect(&Rect::from_origin_size(
            Point::ZERO,
            Size::new(f64::from(w16), f64::from(h16)) / scale,
        ));
        let now = Instant::now();
        self.root.paint(&mut PaintCx {
            ctx,
            res: &mut self.res,
            theme: &self.theme,
            now,
        });
        ctx.flush();

        surface.window().pre_present_notify();
        let mut buffer = surface.buffer_mut().expect("softbuffer buffer");
        let bytes: &mut [u8] = bytemuck::cast_slice_mut(&mut buffer[..]);
        let len = usize::from(w16) * usize::from(h16) * 4;
        let target = vello_cpu::PixmapMut::new(w16, h16, &mut bytes[..len]).expect("buffer size");
        ctx.render(target, &mut self.res);
        for px in bytes[..len].as_chunks_mut::<4>().0 {
            px.swap(0, 2);
        }
        buffer.present().expect("presenting");

        if self.stats {
            eprintln!(
                "frame {}x{} @{scale}: {:.2} ms",
                px.width,
                px.height,
                start.elapsed().as_secs_f64() * 1e3
            );
        }
        if widget::any_animating(self.root.as_ref(), Instant::now(), &self.theme) {
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
        self.a11y = Some(Accessibility::new(event_loop.create_proxy()));
        self.set_scheme(theme);
        self.changed();
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
        // a first request for the tree is answered here as well
        self.update_accessibility();
    }

    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let scale = self.scale();
        let logical = |p: winit::dpi::PhysicalPosition<f64>| Point::new(p.x / scale, p.y / scale);
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::SurfaceResized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                self.changed()
            }
            WindowEvent::ThemeChanged(theme) => {
                self.set_scheme(Some(theme));
                self.request_redraw();
            }
            WindowEvent::Focused(focused) => {
                if let Some(a11y) = &mut self.a11y {
                    a11y.window_focused(focused);
                }
            }
            WindowEvent::RedrawRequested => self.redraw(),
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

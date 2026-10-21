//! The window shell: winit window, softbuffer surface, vello_cpu rendering.
//!
//! vello_cpu renders straight into softbuffer's buffer (no second full-size
//! pixmap) and the channels are swapped in place: RGBA bytes → 0RGB words.
//! Frames are only drawn on demand – while something animates, each frame asks
//! for the next one, paced by the compositor's frame callbacks.

use std::num::NonZeroU32;
use std::time::Instant;

use softbuffer::{Context, Surface};
use vello_cpu::kurbo::{Affine, Point, Rect, Size};
use vello_cpu::{RenderContext, Resources};
use winit::application::ApplicationHandler;
use winit::cursor::{Cursor, CursorIcon};
use winit::event::{ButtonSource, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, OwnedDisplayHandle};
use winit::window::{Theme as WinitTheme, Window, WindowAttributes, WindowId};

use crate::text::Text;
use crate::theme::{Appearance, Scheme, Theme};

/// Pointer input in logical coordinates
#[derive(Clone, Copy, Debug)]
pub enum Pointer {
    Move(Point),
    Leave,
    Down(Point),
    Up(Point),
}

/// What the shell hands a view
pub struct Cx<'a> {
    pub theme: &'a Theme,
    pub text: &'a mut Text,
    pub now: Instant,
}

/// The root of a window. (A real widget tree comes next; this is the seam.)
pub trait View {
    /// Logical window size changed, or the view asked for layout
    fn layout(&mut self, cx: &mut Cx<'_>, size: Size);
    fn paint(&self, ctx: &mut RenderContext, res: &mut Resources, theme: &Theme, now: Instant);
    /// Returns true when the view needs layout again (its size changed)
    fn pointer(&mut self, cx: &mut Cx<'_>, event: Pointer) -> bool;
    fn cursor(&self) -> CursorIcon;
    fn animating(&self, theme: &Theme, now: Instant) -> bool;
}

pub struct Options {
    pub title: String,
    pub size: Size,
    pub appearance: Appearance,
}

pub fn run(options: Options, view: impl View + 'static) -> Result<(), Box<dyn std::error::Error>> {
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
        view,
        text: Text::default(),
        window: None,
        render: None,
        res: Resources::new(),
        cursor: CursorIcon::Default,
        stats: std::env::var_os("KIESEL_STATS").is_some(),
    };
    event_loop.run_app(app)?;
    Ok(())
}

struct Shell<V> {
    options: Options,
    theme: Theme,
    forced: Option<Scheme>,
    view: V,
    text: Text,
    window: Option<Surface<OwnedDisplayHandle, Box<dyn Window>>>,
    render: Option<RenderContext>,
    res: Resources,
    cursor: CursorIcon,
    stats: bool,
}

impl<V: View> Shell<V> {
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

    fn layout(&mut self) {
        let size = self.logical_size();
        let mut cx = Cx {
            theme: &self.theme,
            text: &mut self.text,
            now: Instant::now(),
        };
        self.view.layout(&mut cx, size);
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
        self.view.paint(ctx, &mut self.res, &self.theme, now);
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
        if self.view.animating(&self.theme, Instant::now()) {
            surface.window().request_redraw();
        }
    }

    fn pointer(&mut self, event: Pointer) {
        let mut cx = Cx {
            theme: &self.theme,
            text: &mut self.text,
            now: Instant::now(),
        };
        if self.view.pointer(&mut cx, event) {
            self.layout();
        }
        let Some(surface) = &self.window else { return };
        let cursor = self.view.cursor();
        if cursor != self.cursor {
            self.cursor = cursor;
            surface.window().set_cursor(Cursor::Icon(cursor));
        }
        surface.window().request_redraw();
    }
}

impl<V: View> ApplicationHandler for Shell<V> {
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
        self.set_scheme(theme);
        self.layout();
    }

    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let scale = self.scale();
        let logical = |p: winit::dpi::PhysicalPosition<f64>| Point::new(p.x / scale, p.y / scale);
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::SurfaceResized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                self.layout();
                if let Some(s) = &self.window {
                    s.window().request_redraw();
                }
            }
            WindowEvent::ThemeChanged(theme) => {
                self.set_scheme(Some(theme));
                if let Some(s) = &self.window {
                    s.window().request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::PointerMoved { position, .. }
            | WindowEvent::PointerEntered { position, .. } => {
                self.pointer(Pointer::Move(logical(position)));
            }
            WindowEvent::PointerLeft { .. } => self.pointer(Pointer::Leave),
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
                    self.pointer(if state == ElementState::Pressed {
                        Pointer::Down(p)
                    } else {
                        Pointer::Up(p)
                    });
                }
            }
            _ => {}
        }
    }
}

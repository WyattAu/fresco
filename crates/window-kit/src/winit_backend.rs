//! winit backend (default feature `winit`): real windows, software
//! present through softbuffer.
//!
//! Runtime only; CI exercises the facade headless via `backend::fake`.
//! Pixel policy: premultiplied RGBA8 in, opaque XRGB out (alpha ignored;
//! backend policy documented on `Present`).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window as WinitWindow, WindowId as WinitWindowId};

use crate::RunnerError;
use crate::event::{Event, KeyCharState, LogicalPoint, LogicalSize, PhysicalSize, WindowId};
use crate::window::{Present, PresentError, Window, WindowAttrs};

type SharedWindow = Arc<WinitWindow>;

/// winit-backed runner. Windows registered at construction are created
/// when the platform resumes the application.
pub struct WinitRunner {
    event_loop: EventLoop<()>,
    pending: Vec<WindowAttrs>,
}

struct WinitWindowEntry {
    id: WindowId,
    window: Option<SharedWindow>,
    closed: Arc<AtomicBool>,
    surface: Option<SoftSurface>,
    scale: f64,
    logical: LogicalSize,
    last_cursor: LogicalPoint,
}

struct SoftSurface {
    // The context must outlive the surface; it is kept, never read.
    #[allow(dead_code)]
    context: softbuffer::Context<SharedWindow>,
    surface: softbuffer::Surface<SharedWindow, SharedWindow>,
    width: u32,
    height: u32,
}

struct WinitApp<'a> {
    sink: Option<&'a mut dyn crate::window::EventSink>,
    pending: Vec<WindowAttrs>,
    entries: Vec<WinitWindowEntry>,
    next_ordinal: u64,
}

impl WinitRunner {
    /// Create the platform event loop and register windows to open at
    /// resume.
    pub fn new(windows: Vec<WindowAttrs>) -> Result<Self, RunnerError> {
        let event_loop = EventLoop::new().map_err(|_| RunnerError::Init)?;
        Ok(WinitRunner {
            event_loop,
            pending: windows,
        })
    }

    /// Run until exit.
    pub fn run(self, sink: &mut dyn crate::window::EventSink) -> Result<(), RunnerError> {
        let WinitRunner {
            event_loop,
            pending,
        } = self;
        let mut app = WinitApp {
            sink: Some(sink),
            pending,
            entries: Vec::new(),
            next_ordinal: 0,
        };
        event_loop.run_app(&mut app).map_err(|_| RunnerError::Loop)
    }
}

impl WinitApp<'_> {
    fn deliver(&mut self, entry_index: usize, event: &Event) {
        let Some(entry) = self.entries.get_mut(entry_index) else {
            return;
        };
        let Some(surface) = entry.surface.as_mut() else {
            return;
        };
        let present: &mut dyn Present = surface;
        let Some(window) = entry.window.clone() else {
            return;
        };
        let mut facade = WinitWindowFacade {
            window,
            id: entry.id,
            closed: entry.closed.clone(),
        };
        if let Some(sink) = self.sink.as_deref_mut() {
            sink.event(&mut facade, present, event);
        }
    }
}

struct WinitWindowFacade {
    window: SharedWindow,
    id: WindowId,
    closed: Arc<AtomicBool>,
}

impl Window for WinitWindowFacade {
    fn id(&self) -> WindowId {
        self.id
    }

    fn logical_size(&self) -> LogicalSize {
        let size = self.window.inner_size();
        let scale = self.window.scale_factor();
        LogicalSize {
            width: f64::from(size.width) / scale,
            height: f64::from(size.height) / scale,
        }
    }

    fn physical_size(&self) -> PhysicalSize {
        let size = self.window.inner_size();
        PhysicalSize {
            width: size.width,
            height: size.height,
        }
    }

    fn scale(&self) -> f64 {
        self.window.scale_factor()
    }

    fn request_redraw(&mut self) {
        self.window.request_redraw();
    }

    fn set_title(&mut self, title: &str) {
        self.window.set_title(title);
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }
}

impl Present for SoftSurface {
    fn present(
        &mut self,
        _id: WindowId,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> Result<(), PresentError> {
        let expected = usize::try_from(width)
            .ok()
            .and_then(|w| w.checked_mul(height as usize))
            .and_then(|px| px.checked_mul(4))
            .unwrap_or(usize::MAX);
        if pixels.len() != expected {
            return Err(PresentError::SizeMismatch {
                expected,
                got: pixels.len(),
            });
        }
        let (Some(w), Some(h)) = (
            core::num::NonZeroU32::new(width),
            core::num::NonZeroU32::new(height),
        ) else {
            return Err(PresentError::Backend);
        };
        if self.width != width || self.height != height {
            self.surface
                .resize(w, h)
                .map_err(|_| PresentError::Backend)?;
            self.width = width;
            self.height = height;
        }
        let mut buffer = self
            .surface
            .buffer_mut()
            .map_err(|_| PresentError::Backend)?;
        for (chunk, out) in pixels.as_chunks::<4>().0.iter().zip(buffer.iter_mut()) {
            *out = (u32::from(chunk[0]) << 16) | (u32::from(chunk[1]) << 8) | u32::from(chunk[2]);
        }
        buffer.present().map_err(|_| PresentError::Backend)
    }
}

impl ApplicationHandler for WinitApp<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let pending = std::mem::take(&mut self.pending);
        for attrs in pending {
            let mut wattrs = winit::window::WindowAttributes::default()
                .with_title(attrs.title.clone())
                .with_resizable(attrs.resizable);
            if let Some(logical) = attrs.logical_size {
                wattrs = wattrs
                    .with_inner_size(winit::dpi::LogicalSize::new(logical.width, logical.height));
            }
            let Ok(wwin) = event_loop.create_window(wattrs) else {
                continue;
            };
            let scale = wwin.scale_factor();
            let inner = wwin.inner_size();
            let shared: SharedWindow = Arc::new(wwin);
            let context = match softbuffer::Context::new(shared.clone()) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let surface = match softbuffer::Surface::new(&context, shared.clone()) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let id = WindowId(self.next_ordinal);
            self.next_ordinal += 1;
            self.entries.push(WinitWindowEntry {
                id,
                window: Some(shared),
                closed: Arc::new(AtomicBool::new(false)),
                surface: Some(SoftSurface {
                    context,
                    surface,
                    width: inner.width,
                    height: inner.height,
                }),
                scale,
                logical: LogicalSize {
                    width: f64::from(inner.width) / scale,
                    height: f64::from(inner.height) / scale,
                },
                last_cursor: LogicalPoint { x: 0.0, y: 0.0 },
            });
            // Tell the engine the initial size.
            let event = Event::Resized(PhysicalSize {
                width: inner.width,
                height: inner.height,
            });
            self.deliver(self.entries.len() - 1, &event);
        }
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        wid: WinitWindowId,
        event: WindowEvent,
    ) {
        let Some(index) = self
            .entries
            .iter_mut()
            .position(|e| e.window.as_ref().map(|w| w.id() == wid).unwrap_or(false))
        else {
            return;
        };
        let scale = self
            .entries
            .get(index)
            .map(|e| e.scale)
            .unwrap_or(1.0)
            .max(1e-9);
        let facade_event = match event {
            WindowEvent::RedrawRequested => Event::RedrawRequested,
            WindowEvent::Resized(size) => {
                if let Some(entry) = self.entries.get_mut(index) {
                    entry.logical = LogicalSize {
                        width: f64::from(size.width) / scale,
                        height: f64::from(size.height) / scale,
                    };
                }
                Event::Resized(PhysicalSize {
                    width: size.width,
                    height: size.height,
                })
            }
            WindowEvent::CloseRequested => Event::CloseRequested,
            WindowEvent::Focused(focused) => Event::Focused(focused),
            WindowEvent::CursorMoved { position, .. } => {
                let at = LogicalPoint {
                    x: position.x / scale,
                    y: position.y / scale,
                };
                if let Some(e) = self.entries.get_mut(index) {
                    e.last_cursor = at;
                }
                Event::CursorMoved(at)
            }
            WindowEvent::MouseInput { state, button, .. } => Event::MouseInput {
                pressed: state == ElementState::Pressed,
                button: match button {
                    MouseButton::Left => 0,
                    MouseButton::Right => 1,
                    MouseButton::Middle => 2,
                    _ => 3,
                },
                at: self
                    .entries
                    .get(index)
                    .map(|e| e.last_cursor)
                    .unwrap_or(LogicalPoint { x: 0.0, y: 0.0 }),
            },
            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    winit::event::MouseScrollDelta::LineDelta(x, y) => {
                        (f64::from(x) * 16.0, f64::from(y) * 16.0)
                    }
                    winit::event::MouseScrollDelta::PixelDelta(p) => (p.x / scale, p.y / scale),
                };
                Event::Scrolled {
                    delta: LogicalPoint { x: dx, y: dy },
                }
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => {
                if is_synthetic {
                    return;
                }
                let state = match event.state {
                    ElementState::Pressed => KeyCharState::Pressed,
                    ElementState::Released => KeyCharState::Released,
                };
                let Some(text) = event.text.as_ref().and_then(|t| t.chars().next()) else {
                    return;
                };
                Event::KeyChar { ch: text, state }
            }
            _ => return,
        };
        if matches!(facade_event, Event::CloseRequested)
            && let Some(e) = self.entries.get(index)
        {
            e.closed.store(true, Ordering::Relaxed);
        }
        self.deliver(index, &facade_event);
    }
}

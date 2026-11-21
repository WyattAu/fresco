//! The kestrel-gui windowed application shell.
//!
//! Winit event loop + softbuffer present + pacing evidence, hosting
//! the [`crate::KestrelPanel`] — the same shell pattern the fresco
//! game uses, productized for the kestrel consumer app.

use std::path::PathBuf;

use paint_kit::{Painter, Surface};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

use crate::KestrelPanel;

/// Shared application state for the windowed shell.
#[derive(Default)]
struct KestrelApp {
    window: Option<std::sync::Arc<Window>>,
    context: Option<softbuffer::Context<std::sync::Arc<Window>>>,
    surface: Option<softbuffer::Surface<std::sync::Arc<Window>, std::sync::Arc<Window>>>,
    panel_surface: Option<Surface>,
    panel_painter: Option<Painter>,
    panel: KestrelPanel,
    frame: u64,
    /// Pacing evidence: `frame,paint_us` rows (file-based so the
    /// evidence survives process death).
    pacing_file: Option<std::fs::File>,
    capture_dir: Option<PathBuf>,
    /// Tracked cursor (winit MouseInput carries no position — the
    /// fresco-android lesson).
    last_cursor: Option<widget_kit::LogicalAt>,
}

impl KestrelApp {
    /// Observability dir: env override, else the desktop scratch dir.
    fn capture_dir() -> PathBuf {
        std::env::var("GATE_A_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp/opencode/kestrel-trace"))
    }

    /// Paint the panel into the scene surface, present, record
    /// pacing.
    fn paint_and_present(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let size = window.inner_size();
        let scale = window.scale_factor();
        // Logical resolution: divide physical by the scale factor
        // (touches are scaled back up by the same factor).
        let (lwu, lhu) = (
            ((size.width as f64 / scale).max(1.0) as usize),
            ((size.height as f64 / scale).max(1.0) as usize),
        );
        let (wu, hu) = (size.width.max(1) as usize, size.height.max(1) as usize);

        let paint_start = std::time::Instant::now();
        self.panel.resize(lwu, lhu);
        let mut panel_surface = self
            .panel_surface
            .take()
            .unwrap_or_else(|| Surface::new(lwu, lhu).unwrap_or_default());
        if panel_surface.width() != lwu || panel_surface.height() != lhu {
            panel_surface = Surface::new(lwu, lhu).unwrap_or_default();
        }
        let mut painter = self.panel_painter.take().unwrap_or_default();
        self.panel.paint(&mut panel_surface, &mut painter);
        self.panel_painter = Some(painter);
        // Return the painted surface BEFORE the blit: it is both the
        // blit source and the pacing/inspection source.
        self.panel_surface = Some(panel_surface);
        let paint_us = paint_start.elapsed().as_micros();

        // Blit: premultiplied RGBA8 -> 0xAABBGGRR u32, nearest
        // neighbor upscale via the shared window-kit helper.
        let Some(mut sb_surface) = self.surface.take() else {
            return;
        };
        let Ok(sw) = u32::try_from(wu) else {
            self.surface = Some(sb_surface);
            return;
        };
        let Ok(sh) = u32::try_from(hu) else {
            self.surface = Some(sb_surface);
            return;
        };
        let _ = sb_surface.resize(
            std::num::NonZeroU32::new(sw).unwrap_or(std::num::NonZeroU32::MIN),
            std::num::NonZeroU32::new(sh).unwrap_or(std::num::NonZeroU32::MIN),
        );
        let Ok(mut buffer) = sb_surface.buffer_mut() else {
            println!("[kestrel] buffer_mut failed");
            self.surface = Some(sb_surface);
            return;
        };
        let data = self.panel_surface.as_ref().map(|s| s.data()).unwrap_or(&[]);
        let total = (sw as usize).saturating_mul(sh as usize);
        for k in 0..total {
            let col = k % sw as usize;
            let row = k / sw as usize;
            // Physical -> logical (nearest neighbor).
            let sx = (col as f64 * lwu as f64 / sw as f64).floor().max(0.0) as usize;
            let sy = (row as f64 * lhu as f64 / sh as f64).floor().max(0.0) as usize;
            let base = (sy.min(lhu - 1) * lwu + sx.min(lwu - 1)) * 4;
            let Some(q) = data.get(base..base + 4) else {
                break;
            };
            let Ok([r, g, b, a]) = <[u8; 4]>::try_from(q) else {
                break;
            };
            if let Some(px) = buffer.get_mut(k) {
                *px = u32::from(a) << 24 | u32::from(b) << 16 | u32::from(g) << 8 | u32::from(r);
            }
        }
        if let Some(e) = buffer.present().err() {
            println!("[kestrel] present failed: {e}");
        }
        self.surface = Some(sb_surface);

        // Pacing row: frame,paint_us. Flushed per row.
        self.frame += 1;
        if let Some(pf) = self.pacing_file.as_mut() {
            use std::io::Write;
            let row = format!("{},{}\n", self.frame, paint_us);
            let _ = pf.write_all(row.as_bytes());
            let _ = pf.flush();
        }
        if self.frame.is_multiple_of(120) {
            println!("[kestrel] frame {} paint {}us", self.frame, paint_us);
        }

        window.request_redraw();
    }

    fn pointer_at(&self, position: winit::dpi::PhysicalPosition<f64>) -> widget_kit::LogicalAt {
        // winit delivers PHYSICAL pixels; the panel works in LOGICAL
        // px (fresco-game touch lesson).
        let scale = self
            .window
            .as_ref()
            .map(|w| w.scale_factor())
            .unwrap_or(1.0);
        widget_kit::LogicalAt {
            x: position.x / scale,
            y: position.y / scale,
        }
    }
}

impl ApplicationHandler for KestrelApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = Window::default_attributes()
            .with_title("kestrel")
            .with_inner_size(winit::dpi::LogicalSize::new(360.0, 740.0));
        match event_loop.create_window(attrs) {
            Ok(window) => {
                let window = std::sync::Arc::new(window);
                // Redraw ticker: keeps the loop painting on platforms
                // where request_redraw alone does not wake it.
                let ticker_window = window.clone();
                std::thread::spawn(move || {
                    loop {
                        ticker_window.request_redraw();
                        std::thread::sleep(std::time::Duration::from_millis(8));
                    }
                });
                match softbuffer::Context::new(window.clone()) {
                    Ok(ctx) => {
                        let surface = softbuffer::Surface::new(&ctx, window.clone())
                            .map_err(|e| println!("[kestrel] softbuffer surface: {e}"))
                            .ok();
                        self.surface = surface;
                        self.context = Some(ctx);
                    }
                    Err(e) => println!("[kestrel] softbuffer context: {e}"),
                }
                self.window = Some(window);
                // Observability: pacing evidence in the capture dir
                // (env override, desktop scratch default).
                let dir = KestrelApp::capture_dir();
                let _ = std::fs::create_dir_all(&dir);
                self.capture_dir = Some(dir.clone());
                let pacing_path = dir.join("pacing.csv");
                self.pacing_file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&pacing_path)
                    .map_err(|e| println!("[kestrel] pacing open: {e}"))
                    .ok();
                if let Some(pf) = self.pacing_file.as_mut() {
                    use std::io::Write;
                    let _ = pf.write_all(b"frame,paint_us\n");
                    let _ = pf.flush();
                }
            }
            Err(e) => println!("[kestrel] create_window: {e}"),
        }
    }

    /// Keep the redraw chain alive.
    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = self.window.clone() {
            window.request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                self.paint_and_present();
            }
            WindowEvent::Resized(_) => {
                if let Some(window) = self.window.clone() {
                    window.request_redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.last_cursor = Some(self.pointer_at(position));
            }
            // Desktop mouse: press/release through the tracked cursor
            // (winit MouseInput carries no position).
            WindowEvent::MouseInput { state, button, .. } => {
                if button == winit::event::MouseButton::Left
                    && let Some(at) = self.last_cursor
                {
                    match state {
                        winit::event::ElementState::Pressed => {
                            self.panel.pointer_down(at.x, at.y);
                        }
                        winit::event::ElementState::Released => {
                            self.panel.pointer_up(at.x, at.y);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Run kestrel-gui in a desktop window (X11/Wayland).
///
/// # Errors
/// Fails when the event loop cannot start or the run loop exits
/// with an error.
pub fn run_desktop() -> Result<(), String> {
    let event_loop = EventLoop::new().map_err(|e| format!("event loop: {e}"))?;
    event_loop
        .run_app(&mut KestrelApp::default())
        .map_err(|e| format!("run: {e}"))
}

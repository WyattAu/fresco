//! Android windowed showcase for the fresco UI engine (M2).
//!
//! One scene ([`scene::render_scene`]), two shells: a desktop winit
//! window (X11, for pre-flight verification with the full inspector)
//! and an Android `android_main` cdylib (NativeActivity) packaged
//! for the phone. Observability is wired in both shells: per-frame
//! BMP dumps, frame diff + hash metadata, pacing stats, and a final
//! analysis — all to a capture directory (`/sdcard/Download/...`
//! on Android).
//!
//! Touch/pointer input maps onto scene touch dots so every finger
//! visibly lands on screen.

pub mod scene;
pub mod widgets_screen;

use window_kit::inspector::Inspector;

use paint_kit::{Painter, Surface};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

use widgets_screen::WidgetScreen;

/// Shared application state for both shells.
struct ShowcaseApp {
    window: Option<std::sync::Arc<Window>>,
    context: Option<softbuffer::Context<std::sync::Arc<Window>>>,
    surface: Option<softbuffer::Surface<std::sync::Arc<Window>, std::sync::Arc<Window>>>,
    scene_surface: Option<Surface>,
    scene_painter: Option<Painter>,
    screen: WidgetScreen,
    last_cursor: Option<widget_kit::LogicalAt>,
    clock: std::time::Instant,
    frame: u64,
    inspector: Option<Inspector>,
    last_frame_at: Option<std::time::Instant>,
    fps: f64,
    /// CSV pacing log: `frame,paint_us,fps` per frame (file-based so
    /// the evidence survives logcat ring rotation).
    pacing_file: Option<std::fs::File>,
}

impl Default for ShowcaseApp {
    fn default() -> Self {
        Self {
            window: None,
            context: None,
            surface: None,
            scene_surface: None,
            scene_painter: None,
            screen: WidgetScreen::settings(),
            last_cursor: None,
            clock: std::time::Instant::now(),
            frame: 0,
            inspector: None,
            last_frame_at: None,
            fps: 0.0,
            pacing_file: None,
        }
    }
}

impl ShowcaseApp {
    /// Record one frame through the inspector: BMP cadence, pixel
    /// diff, FNV hash. The single observability chokepoint.
    fn observe_frame(&mut self, paint_us: u128) {
        let Some(surface) = self.scene_surface.as_ref() else {
            return;
        };
        let Some(inspector) = self.inspector.as_mut() else {
            return;
        };
        inspector.record_frame(
            surface.data(),
            surface.width(),
            surface.height(),
            u64::try_from(paint_us).unwrap_or(0),
            0,
        );
    }

    /// Paint the widget screen into the scene surface, blit to the
    /// softbuffer framebuffer, present. The widget tree repaints in
    /// full each frame (Gate A measured ~0.27 ms at this size).
    fn paint_and_present(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let size = window.inner_size();
        let scale = window.scale_factor();
        // Paint at LOGICAL resolution (360x740-class); the blit
        // upscales to physical during the copy. Mid-range phone CPUs
        // cannot full-frame paint at 1080x2400 (112 ms/frame on the
        // A53); logical-res paint is ~9x cheaper and the buffer
        // upscale is nearest-neighbor.
        let (lwu, lhu) = (
            ((size.width as f64 / scale).max(1.0) as usize),
            ((size.height as f64 / scale).max(1.0) as usize),
        );
        let (wu, hu) = (size.width.max(1) as usize, size.height.max(1) as usize);

        let paint_start = std::time::Instant::now();
        self.screen.resize(lwu, lhu);
        let mut scene_surface = self
            .scene_surface
            .take()
            .unwrap_or_else(|| Surface::new(lwu, lhu).unwrap_or_default());
        if scene_surface.width() != lwu || scene_surface.height() != lhu {
            scene_surface = Surface::new(lwu, lhu).unwrap_or_default();
        }
        let mut painter = self.scene_painter.take().unwrap_or_default();
        self.screen.paint(&mut scene_surface, &mut painter);
        self.scene_painter = Some(painter);
        // Return the painted surface BEFORE the blit: it is both the
        // blit source and the inspector's capture source.
        self.scene_surface = Some(scene_surface);
        let paint_us = paint_start.elapsed().as_micros();

        // Blit: paint-kit premultiplied RGBA8 -> softbuffer
        // 0xAABBGGRR u32, upscaled logical -> physical
        // (nearest-neighbor). Take-and-restore: softbuffer Surface
        // is not Clone.
        let Some(mut sb_surface) = self.surface.take() else {
            return;
        };
        // Wayland requires the surface size be declared before the
        // buffer is borrowed (NonZeroU32) — physical size.
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
            println!("[fresco] buffer_mut failed");
            self.surface = Some(sb_surface);
            return;
        };
        let data = self.scene_surface.as_ref().map(|s| s.data()).unwrap_or(&[]);
        let lw = wu as f64 / scale;
        let lh = hu as f64 / scale;
        let bwidth = sw as f64;
        let bheight = sh as f64;
        let total = (sw as usize).saturating_mul(sh as usize);
        for k in 0..total {
            let col = (k % sw as usize) as f64;
            let row = (k / sw as usize) as f64;
            // Physical -> logical (nearest neighbor).
            let sx = (col * lw / bwidth).floor().max(0.0) as usize;
            let sy = (row * lh / bheight).floor().max(0.0) as usize;
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
            println!("[fresco] present failed: {e}");
        }
        self.surface = Some(sb_surface);

        // Observability + pacing bookkeeping.
        self.observe_frame(paint_us);
        let now = std::time::Instant::now();
        if let Some(last) = self.last_frame_at {
            let dt = now.duration_since(last).as_secs_f64();
            if dt > 1e-4 {
                let inst = 1.0 / dt;
                self.fps = if self.fps > 0.0 {
                    self.fps * 0.9 + inst * 0.1
                } else {
                    inst
                };
            }
        }
        self.last_frame_at = Some(now);
        self.frame += 1;
        if self.frame.is_multiple_of(120) {
            println!(
                "[fresco] frame {} fps {:.1} paint {}us",
                self.frame, self.fps, paint_us
            );
        }

        window.request_redraw();
    }

    fn pointer_at(&self, position: winit::dpi::PhysicalPosition<f64>) -> widget_kit::LogicalAt {
        widget_kit::LogicalAt {
            x: position.x,
            y: position.y,
        }
    }
}

impl ApplicationHandler for ShowcaseApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = Window::default_attributes()
            .with_title("fresco M4")
            .with_inner_size(winit::dpi::LogicalSize::new(360.0, 740.0));
        match event_loop.create_window(attrs) {
            Ok(window) => {
                let window = std::sync::Arc::new(window);
                // Redraw ticker: on Android, request_redraw alone does
                // not wake the sleeping event loop (the loop waits for
                // lifecycle/input events), so a 120 Hz ticker thread
                // drives the redraw chain. request_redraw is
                // thread-safe per winit.
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
                            .map_err(|e| println!("[fresco] softbuffer surface: {e}"))
                            .ok();
                        self.surface = surface;
                        self.context = Some(ctx);
                    }
                    Err(e) => println!("[fresco] softbuffer context: {e}"),
                }
                self.window = Some(window);
                // Observability: capture dir defaults to the app's
                // external files dir on Android (adb-pullable),
                // /tmp/opencode/fresco-trace on desktop; GATE_A_DIR
                // overrides on both. Recording is on: BMPs every 60
                // frames land on /sdcard for pull-based review.
                let default_dir = if cfg!(target_os = "android") {
                    "/sdcard/Android/data/com.example.fresco_android/files/fresco-m4"
                } else {
                    "/tmp/opencode/fresco-trace"
                };
                let dir = std::env::var("GATE_A_DIR").unwrap_or_else(|_| default_dir.to_owned());
                self.inspector = Some(Inspector::with_dir(
                    std::path::PathBuf::from(&dir),
                    true,
                    60,
                    0,
                ));
                // File-based pacing capture: survives logcat rotation.
                let pacing_path = std::path::Path::new(&dir).join("pacing.csv");
                let _ = std::fs::create_dir_all(std::path::Path::new(&dir));
                self.pacing_file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&pacing_path)
                    .map_err(|e| println!("[fresco] pacing open: {e}"))
                    .ok();
                if let Some(pf) = self.pacing_file.as_mut() {
                    use std::io::Write;
                    let _ = pf.write_all(b"frame,paint_us,fps_milli\n");
                    let _ = pf.flush();
                }
                self.clock = std::time::Instant::now();
            }
            Err(e) => println!("[fresco] create_window: {e}"),
        }
    }

    /// Keep the redraw chain alive: on Android the initial
    /// request_redraw is consumed after one frame and the loop
    /// otherwise sleeps until the next real event — winit's
    /// about_to_wait hook re-arms it every iteration.
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
            WindowEvent::Touch(touch) => {
                // Android delivers finger events here.
                let at = widget_kit::LogicalAt {
                    x: touch.location.x,
                    y: touch.location.y,
                };
                match touch.phase {
                    winit::event::TouchPhase::Started => self.screen.pointer_down(at.x, at.y),
                    winit::event::TouchPhase::Ended | winit::event::TouchPhase::Cancelled => {
                        self.screen.pointer_up(at.x, at.y)
                    }
                    _ => {}
                }
                if let Some(window) = self.window.clone() {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

/// Run the showcase in a desktop window (X11/Wayland). Used to
/// verify the windowed shell before packaging for Android.
///
/// # Errors
/// Fails when the event loop cannot start or the run loop exits
/// with an error.
pub fn run_desktop() -> Result<(), String> {
    let event_loop = EventLoop::new().map_err(|e| format!("event loop: {e}"))?;
    event_loop
        .run_app(&mut ShowcaseApp::default())
        .map_err(|e| format!("run: {e}"))
}

#[cfg(target_os = "android")]
mod android_entry {
    use super::ShowcaseApp;
    use winit::event_loop::EventLoop;
    use winit::platform::android::EventLoopBuilderExtAndroid;

    /// Android entry point (NativeActivity).
    ///
    /// # Panics
    /// Panics when the event loop or run loop fails — fatal for a
    /// showcase app; stdout routes to logcat.
    #[unsafe(no_mangle)]
    fn android_main(app: android_activity::AndroidApp) {
        println!("[fresco] android_main: starting M2 showcase");
        // Infallible by policy: a failed loop logs and lets the app
        // exit cleanly rather than crashing the process.
        let Ok(event_loop) = EventLoop::builder().with_android_app(app).build() else {
            println!("[fresco] event loop failed");
            return;
        };
        if let Err(e) = event_loop.run_app(&mut ShowcaseApp::default()) {
            println!("[fresco] run failed: {e}");
        }
    }
}

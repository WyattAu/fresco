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

use paint_kit::Surface;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

use window_kit::inspector::Inspector;

use scene::TouchDot;

/// Shared application state for both shells.
struct ShowcaseApp {
    window: Option<std::sync::Arc<Window>>,
    context: Option<softbuffer::Context<std::sync::Arc<Window>>>,
    surface: Option<softbuffer::Surface<std::sync::Arc<Window>, std::sync::Arc<Window>>>,
    scene_surface: Option<Surface>,
    scene_painter: Option<paint_kit::Painter>,
    touches: Vec<TouchDot>,
    clock: std::time::Instant,
    frame: u64,
    inspector: Option<Inspector>,
    last_frame_at: Option<std::time::Instant>,
    fps: f64,
}

impl Default for ShowcaseApp {
    fn default() -> Self {
        Self {
            window: None,
            context: None,
            surface: None,
            scene_surface: None,
            scene_painter: None,
            touches: Vec::new(),
            clock: std::time::Instant::now(),
            frame: 0,
            inspector: None,
            last_frame_at: None,
            fps: 0.0,
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

    /// Paint the scene into the scene surface and blit to the
    /// softbuffer framebuffer, then present.
    fn paint_and_present(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let size = window.inner_size();
        let (wu, hu) = (size.width.max(1) as usize, size.height.max(1) as usize);

        // Paint the scene.
        let paint_start = std::time::Instant::now();
        let scene_surface = self
            .scene_surface
            .get_or_insert_with(|| Surface::new(wu, hu).unwrap_or_default());
        if scene_surface.width() != wu || scene_surface.height() != hu {
            *scene_surface = Surface::new(wu, hu).unwrap_or_default();
        }
        let mut painter = self
            .scene_painter
            .take()
            .unwrap_or_else(paint_kit::Painter::default);
        let fps = self.fps;
        let touches = self.touches.clone();
        let _ops = scene::render_scene(
            scene_surface,
            &mut painter,
            wu,
            hu,
            self.clock.elapsed().as_secs_f64(),
            &touches,
            fps,
        );
        self.scene_painter = Some(painter);
        let paint_us = paint_start.elapsed().as_micros();

        // Blit to the present buffer: paint-kit premultiplied RGBA8
        // -> softbuffer 0xAABBGGRR u32. Take-and-restore: Surface
        // is not Clone.
        let Some(mut sb_surface) = self.surface.take() else {
            return;
        };
        // Wayland requires the surface size be declared before the
        // buffer is borrowed (NonZeroU32).
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
        let data = scene_surface.data();
        for (k, px) in buffer.iter_mut().enumerate() {
            let base = k * 4;
            let Some(quadruple) = data.get(base..base + 4) else {
                break;
            };
            *px = u32::from(quadruple[3]) << 24
                | u32::from(quadruple[2]) << 16
                | u32::from(quadruple[1]) << 8
                | u32::from(quadruple[0]);
        }
        let present_err = buffer.present().err();
        if let Some(e) = present_err {
            println!("[fresco] present failed: {e}");
        }
        // Restore the surface for the next frame (also on error).
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
        if self.frame % 120 == 0 {
            println!(
                "[fresco] frame {} fps {:.1} paint {}us touches {}",
                self.frame,
                self.fps,
                paint_us,
                self.touches.len()
            );
        }

        window.request_redraw();
    }

    /// Age out touch dots (2 s trail).
    fn age_touches(&mut self) {
        let now = self.clock.elapsed().as_secs_f64();
        self.touches.retain_mut(|dot| {
            dot.age = now - dot.age;
            dot.age < 2.0
        });
    }

    /// Normalize a pointer position against the window size.
    fn push_pointer(&mut self, position: winit::dpi::PhysicalPosition<f64>) {
        let size = self
            .window
            .as_ref()
            .map(|w| {
                let s = w.inner_size();
                (f64::from(s.width).max(1.0), f64::from(s.height).max(1.0))
            })
            .unwrap_or((1.0, 1.0));
        let t = self.clock.elapsed().as_secs_f64();
        self.touches.push(TouchDot {
            pos: (position.x / size.0, position.y / size.1),
            age: t,
        });
        if self.touches.len() > 48 {
            self.touches.remove(0);
        }
    }
}

impl ApplicationHandler for ShowcaseApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = Window::default_attributes()
            .with_title("fresco M2")
            .with_inner_size(winit::dpi::LogicalSize::new(360.0, 740.0));
        match event_loop.create_window(attrs) {
            Ok(window) => {
                let window = std::sync::Arc::new(window);
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
                // external files dir on Android (adb-pullable without
                // permissions), /tmp/opencode/fresco-trace on desktop;
                // GATE_A_DIR overrides on both.
                let default_dir = if cfg!(target_os = "android") {
                    "/sdcard/Android/data/com.example.fresco_android/files/fresco-m2"
                } else {
                    "/tmp/opencode/fresco-trace"
                };
                let dir = std::env::var("GATE_A_DIR").unwrap_or_else(|_| default_dir.to_owned());
                self.inspector = Some(Inspector::with_dir(
                    std::path::PathBuf::from(dir),
                    true,
                    60,
                    0,
                ));
                self.clock = std::time::Instant::now();
            }
            Err(e) => println!("[fresco] create_window: {e}"),
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
                self.age_touches();
                self.paint_and_present();
            }
            WindowEvent::Resized(_) => {
                if let Some(window) = self.window.clone() {
                    window.request_redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.push_pointer(position);
                if let Some(window) = self.window.clone() {
                    window.request_redraw();
                }
            }
            WindowEvent::Touch(touch) => {
                // Android delivers finger events here.
                if matches!(
                    touch.phase,
                    winit::event::TouchPhase::Started | winit::event::TouchPhase::Moved
                ) {
                    self.push_pointer(touch.location);
                    if let Some(window) = self.window.clone() {
                        window.request_redraw();
                    }
                }
            }
            WindowEvent::MouseInput { state, .. } => {
                if state == ElementState::Pressed {
                    if let Some(window) = self.window.clone() {
                        window.request_redraw();
                    }
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
        let event_loop = EventLoop::builder()
            .with_android_app(app)
            .build()
            .expect("android event loop");
        event_loop
            .run_app(&mut ShowcaseApp::default())
            .expect("android run");
    }
}

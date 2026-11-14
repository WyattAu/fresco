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

use paint_kit::{Paint, Rect, Solid, Surface, Vec2};
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
    /// Static layer (background + grid), rendered once per resize.
    base: Option<Surface>,
    /// The sweep band's rect last frame (damage region).
    band_prev: Option<(f64, f64, f64, f64)>,
    touches: Vec<TouchDot>,
    clock: std::time::Instant,
    frame: u64,
    inspector: Option<Inspector>,
    last_frame_at: Option<std::time::Instant>,
    fps: f64,
    /// Per-frame paint cost of the last frame (us), for pacing.
    paint_us: u64,
}

impl Default for ShowcaseApp {
    fn default() -> Self {
        Self {
            window: None,
            context: None,
            surface: None,
            scene_surface: None,
            scene_painter: None,
            base: None,
            band_prev: None,
            touches: Vec::new(),
            clock: std::time::Instant::now(),
            frame: 0,
            inspector: None,
            last_frame_at: None,
            fps: 0.0,
            paint_us: 0,
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

    /// Render the static base layer (background + grid) once.
    fn render_base(&mut self, w: usize, h: usize) {
        let mut painter = paint_kit::Painter::default();
        let mut base = Surface::new(w, h).unwrap_or_default();
        let bw = w as f64;
        let bh = h as f64;
        painter.fill_rect(
            &mut base,
            Rect::from_size(Vec2::ZERO, Vec2::new(bw, bh)),
            &Paint::from(Solid::premul([22, 24, 32, 255])),
        );
        let cell = 48.0;
        let mut x = 0.0;
        while x <= bw {
            painter.fill_rect(
                &mut base,
                Rect::from_size(Vec2::new(x, 0.0), Vec2::new(1.0, bh)),
                &Paint::from(Solid::premul([70, 74, 88, 110])),
            );
            x += cell;
        }
        let mut y = 0.0;
        while y <= bh {
            painter.fill_rect(
                &mut base,
                Rect::from_size(Vec2::new(0.0, y), Vec2::new(bw, 1.0)),
                &Paint::from(Solid::premul([70, 74, 88, 110])),
            );
            y += cell;
        }
        self.base = Some(base);
    }

    /// Copy a rect of pixels from the base layer onto the scene
    /// surface (damage restore).
    fn restore_region(
        scene: &mut Surface,
        base: &Surface,
        x0: usize,
        y0: usize,
        w: usize,
        h: usize,
    ) {
        let sw = scene.width();
        for row in y0..(y0 + h).min(scene.height()) {
            let dst_base = (row * sw + x0) * 4;
            let src_base = (row * base.width() + x0) * 4;
            let span = w * 4;
            let dst = scene.data_mut().get_mut(dst_base..dst_base + span);
            let src = base.data().get(src_base..src_base + span);
            if let (Some(d), Some(s)) = (dst, src) {
                d.copy_from_slice(s);
            }
        }
    }

    /// Copy a rect of pixels from base onto scene (shared impl used
    /// by the loop and tests).
    /// The sweep band's rect at time `t`.
    fn band_rect(t: f64, w: f64, h: f64) -> (f64, f64, f64, f64) {
        let band_y = (t * 120.0) % (h + 160.0) - 80.0;
        (0.0, band_y, w, 120.0)
    }

    /// Paint the sweep band (after damage restore).
    fn paint_band(
        surface: &mut Surface,
        painter: &mut paint_kit::Painter,
        rect: (f64, f64, f64, f64),
    ) {
        let (x0, y0, bw, bh) = rect;
        painter.fill_rect(
            surface,
            Rect::from_size(Vec2::new(x0, y0), Vec2::new(bw, bh)),
            &Paint::from(Solid::premul([60, 90, 160, 46])),
        );
    }

    /// Paint the scene into the scene surface and blit to the
    /// softbuffer framebuffer, then present. Damage-tracked: the
    /// static base layer provides background + grid (rendered once,
    /// restored under damaged rects); per frame only the band and
    /// touch dots are painted.
    fn paint_and_present(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let size = window.inner_size();
        let (wu, hu) = (size.width.max(1) as usize, size.height.max(1) as usize);

        let paint_start = std::time::Instant::now();
        let mut scene_surface = self
            .scene_surface
            .take()
            .unwrap_or_else(|| Surface::new(wu, hu).unwrap_or_default());
        if scene_surface.width() != wu || scene_surface.height() != hu {
            scene_surface = Surface::new(wu, hu).unwrap_or_default();
            self.base = None;
            self.band_prev = None;
        }
        if self.base.is_none() {
            self.render_base(wu, hu);
        }
        let Some(base) = self.base.clone() else {
            self.scene_surface = Some(scene_surface);
            return;
        };

        let mut painter = self
            .scene_painter
            .take()
            .unwrap_or_else(paint_kit::Painter::default);
        let t = self.clock.elapsed().as_secs_f64();
        let band = Self::band_rect(t, wu as f64, hu as f64);

        // DAMAGE RESTORE FIRST: base pixels under the previous band,
        // the new band, and the touch dots. Painting the band before
        // the restore erased it (caught on-device: the sweep band
        // vanished from the screen).
        let mut damage_rects = self.band_prev.iter().copied().collect::<Vec<_>>();
        damage_rects.push(band);
        for dot in &self.touches {
            let age = t - dot.age;
            let fade = (1.0 - age / 2.0).clamp(0.15, 1.0);
            let cx = dot.pos.0 * wu as f64;
            let cy = dot.pos.1 * hu as f64;
            let r = 10.0 + 14.0 * (1.0 - fade) + 16.0;
            damage_rects.push((cx - r, cy - r, r * 2.0, r * 2.0));
        }
        for (x0, y0, bw, bh) in &damage_rects {
            let rx = (*x0).max(0.0).floor() as usize;
            let ry = (*y0).max(0.0).floor() as usize;
            let rw = (*bw).ceil() as usize;
            let rh = (*bh).ceil() as usize;
            let rw = rw.min(wu.saturating_sub(rx));
            let rh = rh.min(hu.saturating_sub(ry));
            if rw == 0 || rh == 0 {
                continue;
            }
            Self::restore_region(&mut scene_surface, &base, rx, ry, rw, rh);
        }

        // THEN paint the band and the touch dots on the restored base.
        Self::paint_band(&mut scene_surface, &mut painter, band);

        let mut touches = self.touches.clone();
        for dot in &mut touches {
            dot.age = t - dot.age;
        }
        for dot in &touches {
            let fade = (1.0 - dot.age / 2.0).clamp(0.15, 1.0);
            let a = f64::from(220_u8) * fade;
            let cx = dot.pos.0 * wu as f64;
            let cy = dot.pos.1 * hu as f64;
            let r = 10.0 + 14.0 * (1.0 - fade);
            for (rr, aa) in [(r, a), (r * 0.7, a / 2.0), (r * 0.45, a)] {
                painter.fill_rect(
                    &mut scene_surface,
                    Rect::from_size(Vec2::new(cx - rr, cy - rr), Vec2::new(rr * 2.0, rr * 2.0)),
                    &Paint::from(Solid::premul([90, 200, 160, aa.round().min(255.0) as u8])),
                );
            }
        }
        let now_t = t;
        let mut touches = self.touches.clone();
        for dot in &mut touches {
            dot.age = now_t - dot.age;
        }
        for dot in &touches {
            let fade = (1.0 - dot.age / 2.0).clamp(0.15, 1.0);
            let a = f64::from(220_u8) * fade;
            let cx = dot.pos.0 * wu as f64;
            let cy = dot.pos.1 * hu as f64;
            let r = 10.0 + 14.0 * (1.0 - fade);
            for (rr, aa) in [(r, a), (r * 0.7, a / 2.0), (r * 0.45, a)] {
                painter.fill_rect(
                    &mut scene_surface,
                    Rect::from_size(Vec2::new(cx - rr, cy - rr), Vec2::new(rr * 2.0, rr * 2.0)),
                    &Paint::from(Solid::premul([90, 200, 160, aa.round().min(255.0) as u8])),
                );
            }
        }
        self.scene_painter = Some(painter);
        self.band_prev = Some(band);
        self.scene_surface = Some(scene_surface);
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
        let data = self.scene_surface.as_ref().map(|s| s.data()).unwrap_or(&[]);
        for (k, px) in buffer.iter_mut().enumerate() {
            let base_i = k * 4;
            let Some(quadruple) = data.get(base_i..base_i + 4) else {
                break;
            };
            let Ok([r, g, b, a]) = <[u8; 4]>::try_from(quadruple) else {
                break;
            };
            *px = u32::from(a) << 24 | u32::from(b) << 16 | u32::from(g) << 8 | u32::from(r);
        }
        let present_err = buffer.present().err();
        if let Some(e) = present_err {
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
                "[fresco] frame {} fps {:.1} paint {}us touches {}",
                self.frame,
                self.fps,
                paint_us,
                self.touches.len()
            );
        }

        window.request_redraw();
    }

    /// Age out touch dots (2 s trail).    /// Age out touch dots (2 s trail).
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
                if state == ElementState::Pressed
                    && let Some(window) = self.window.clone()
                {
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

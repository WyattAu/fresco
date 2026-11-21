//! Phase B game demo for the fresco UI engine: one game
//! ([`game::GameScreen`]), two shells: a desktop winit window (X11,
//! for pre-flight verification with the full inspector) and an
//! Android `android_main` cdylib (NativeActivity) packaged for the
//! phone.
//!
//! Observability is wired in both shells: per-frame BMP dumps, frame
//! diff + hash metadata, pacing rows (`frame,paint_us,fps_milli`,
//! flushed per row), and a final analysis - all to a capture
//! directory. The sim itself is deterministic under a fixed timestep;
//! see the `game` module oracles.

pub mod atlas;
pub mod game;
pub mod menu;

use game::{GameScreen, hud_button_at};
use menu::MenuScreen;
use window_kit::inspector::Inspector;

use paint_kit::{Painter, Surface};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

/// Which screen the shell shows.
enum AppState {
    /// Main menu (widget-kit screen).
    Menu,
    /// The running game.
    Game,
}

/// Shared application state for both shells.
struct GameApp {
    state: AppState,
    menu: MenuScreen,
    window: Option<std::sync::Arc<Window>>,
    context: Option<softbuffer::Context<std::sync::Arc<Window>>>,
    surface: Option<softbuffer::Surface<std::sync::Arc<Window>, std::sync::Arc<Window>>>,
    scene_surface: Option<Surface>,
    scene_painter: Option<Painter>,
    screen: GameScreen,
    last_cursor: Option<widget_kit::LogicalAt>,
    /// Desktop drag state: left button held (touch devices deliver
    /// Moved touch events instead).
    mouse_down: bool,
    clock: std::time::Instant,
    frame: u64,
    inspector: Option<Inspector>,
    last_frame_at: Option<std::time::Instant>,
    fps: f64,
    /// Best score this device has seen (persisted in the capture
    /// dir as `highscore.txt`).
    best: u64,
    /// Capture dir (observability artifacts + highscore file).
    capture_dir: Option<std::path::PathBuf>,
    /// Last present() cost (microseconds) - the queueBuffer/vsync
    /// wait is the frame-period component paint_us cannot explain.
    present_us: u128,
    /// CSV pacing log: `frame,paint_us,fps_milli` per frame (file-based
    /// so the evidence survives logcat ring rotation).
    pacing_file: Option<std::fs::File>,
}

impl Default for GameApp {
    fn default() -> Self {
        Self {
            state: AppState::Menu,
            menu: MenuScreen::new(),
            window: None,
            context: None,
            surface: None,
            scene_surface: None,
            scene_painter: None,
            screen: GameScreen::new(),
            last_cursor: None,
            mouse_down: false,
            clock: std::time::Instant::now(),
            frame: 0,
            inspector: None,
            last_frame_at: None,
            fps: 0.0,
            best: 0,
            capture_dir: None,
            present_us: 0,
            pacing_file: None,
        }
    }
}

impl GameApp {
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

    /// Step the sim on wall time, paint the game into the scene
    /// surface, blit to the softbuffer framebuffer, present.
    fn paint_and_present(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let size = window.inner_size();
        let scale = window.scale_factor();
        // Paint at LOGICAL resolution (360x740-class); the blit
        // upscales to physical during the copy (nearest-neighbor).
        // Mid-range phone CPUs cannot full-frame paint at 1080x2400.
        let (lwu, lhu) = (
            ((size.width as f64 / scale).max(1.0) as usize),
            ((size.height as f64 / scale).max(1.0) as usize),
        );
        let (wu, hu) = (size.width.max(1) as usize, size.height.max(1) as usize);

        let paint_start = std::time::Instant::now();
        // Sim only while playing; the menu is a static widget screen.
        if matches!(self.state, AppState::Game) {
            self.screen.step_wall_time(std::time::Instant::now());
        }
        self.screen.resize(lwu, lhu);
        self.menu.resize(lwu, lhu);
        let mut scene_surface = self
            .scene_surface
            .take()
            .unwrap_or_else(|| Surface::new(lwu, lhu).unwrap_or_default());
        if scene_surface.width() != lwu || scene_surface.height() != lhu {
            scene_surface = Surface::new(lwu, lhu).unwrap_or_default();
        }
        let mut painter = self.scene_painter.take().unwrap_or_default();
        match self.state {
            AppState::Menu => self.menu.paint(&mut scene_surface, &mut painter),
            AppState::Game => self.screen.paint(&mut scene_surface, &mut painter),
        }
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
        // buffer is borrowed (NonZeroU32) - physical size.
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
            println!("[fresco-game] buffer_mut failed");
            self.surface = Some(sb_surface);
            return;
        };
        // Blit via the shared window-kit helper: integer scales take
        // the expand + row-repeat fast path (memcpy per row), other
        // scales fall back to per-pixel nearest-neighbor.
        if let Some(scene) = self.scene_surface.as_ref() {
            window_kit::blit::blit_upscaled(scene, &mut buffer, sw as usize, sh as usize);
        }
        let present_start = std::time::Instant::now();
        if let Some(e) = buffer.present().err() {
            println!("[fresco-game] present failed: {e}");
        }
        self.present_us = present_start.elapsed().as_micros();
        self.surface = Some(sb_surface);

        // Best-score bookkeeping: grow + persist only on change.
        if matches!(self.state, AppState::Game) {
            let score = self.screen.score();
            if score > self.best {
                self.best = score;
                self.screen.set_best(self.best);
                write_best(&persist_dir(), self.best);
            }
        }

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
        // File-based pacing row: `frame,paint_us,fps_milli`. Flushed
        // per row: a Freecess freeze or process kill would eat
        // buffered rows.
        if let Some(pf) = self.pacing_file.as_mut() {
            use std::io::Write;
            let row = format!(
                "{},{},{}\n",
                self.frame,
                paint_us,
                u64::try_from((self.fps * 1000.0) as i64).unwrap_or(0)
            );
            let _ = pf.write_all(row.as_bytes());
            let _ = pf.flush();
        }
        if self.frame.is_multiple_of(120) {
            println!(
                "[fresco-game] frame {} fps {:.1} paint {}us present {}us balls {}",
                self.frame,
                self.fps,
                paint_us,
                self.present_us,
                self.screen.entity_count()
            );
        }

        window.request_redraw();
    }

    /// Menu hook: when the Play button activated, transition to the
    /// game with the selected difficulty.
    fn maybe_start_game(&mut self) {
        if self.menu.play_pressed() {
            let rate = self.menu.difficulty().fountain_every();
            self.screen = GameScreen::new();
            self.screen.set_fountain_every(rate);
            self.state = AppState::Game;
        }
    }

    fn pointer_at(&self, position: winit::dpi::PhysicalPosition<f64>) -> widget_kit::LogicalAt {
        // winit delivers PHYSICAL pixels; the game + menus work in
        // LOGICAL px. Divide by the scale factor (fresco-android M5
        // lesson - without this, touches land 3x too low on 3x
        // screens and never hit their targets).
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

impl ApplicationHandler for GameApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = Window::default_attributes()
            .with_title("fresco game")
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
                        // 2 ms: the redraw cadence must sit well under
                        // the frame budget, or the ticker - not the
                        // frame work - caps the fps (8 ms gave 33 fps
                        // with 11 ms of real work per frame).
                        std::thread::sleep(std::time::Duration::from_millis(2));
                    }
                });
                match softbuffer::Context::new(window.clone()) {
                    Ok(ctx) => {
                        let surface = softbuffer::Surface::new(&ctx, window.clone())
                            .map_err(|e| println!("[fresco-game] softbuffer surface: {e}"))
                            .ok();
                        self.surface = surface;
                        self.context = Some(ctx);
                    }
                    Err(e) => println!("[fresco-game] softbuffer context: {e}"),
                }
                self.window = Some(window);
                // Observability: capture dir defaults to the app's
                // external files dir on Android (adb-pullable),
                // /tmp/opencode/fresco-game-trace on desktop;
                // GATE_A_DIR overrides on both. BMPs land every 60
                // frames.
                let default_dir = if cfg!(target_os = "android") {
                    "/sdcard/Android/data/com.example.fresco_game/files/fresco-game"
                } else {
                    "/tmp/opencode/fresco-game-trace"
                };
                let dir = std::env::var("GATE_A_DIR").unwrap_or_else(|_| default_dir.to_owned());
                self.capture_dir = Some(std::path::PathBuf::from(&dir));
                self.best = read_best(&persist_dir());
                self.screen.set_best(self.best);
                self.menu.set_best(self.best);
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
                    .map_err(|e| println!("[fresco-game] pacing open: {e}"))
                    .ok();
                if let Some(pf) = self.pacing_file.as_mut() {
                    use std::io::Write;
                    let _ = pf.write_all(b"frame,paint_us,fps_milli\n");
                    let _ = pf.flush();
                }
                self.clock = std::time::Instant::now();
            }
            Err(e) => println!("[fresco-game] create_window: {e}"),
        }
    }

    /// Keep the redraw chain alive: on Android the initial
    /// request_redraw is consumed after one frame and the loop
    /// otherwise sleeps until the next real event - winit's
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
                let at = self.pointer_at(position);
                self.last_cursor = Some(at);
                // Drag: carry the grabbed ball while the button is
                // down (winit MouseInput carries no position - the
                // fresco-android lesson).
                if self.mouse_down {
                    self.screen.pointer_move(at.x, at.y);
                }
            }
            // Desktop mouse: route press/release through the tracked
            // cursor (winit MouseInput carries no position).
            // Menu keyboard a11y: Tab/arrows navigate, Enter/Space
            // activate (widget-kit key_nav + key_char machinery).
            WindowEvent::KeyboardInput { event, .. } => {
                if matches!(self.state, AppState::Menu)
                    && event.state == winit::event::ElementState::Pressed
                {
                    use window_kit::event::NavKey;
                    use winit::keyboard::NamedKey;
                    match &event.logical_key {
                        winit::keyboard::Key::Named(NamedKey::Tab) => {
                            self.menu.key_nav(NavKey::Tab);
                        }
                        winit::keyboard::Key::Named(NamedKey::ArrowDown) => {
                            self.menu.key_nav(NavKey::Down);
                        }
                        winit::keyboard::Key::Named(NamedKey::ArrowUp) => {
                            self.menu.key_nav(NavKey::Up);
                        }
                        winit::keyboard::Key::Named(NamedKey::ArrowLeft) => {
                            self.menu.key_nav(NavKey::Left);
                        }
                        winit::keyboard::Key::Named(NamedKey::ArrowRight) => {
                            self.menu.key_nav(NavKey::Right);
                        }
                        winit::keyboard::Key::Named(NamedKey::Enter)
                        | winit::keyboard::Key::Named(NamedKey::Space) => {
                            self.menu.key_char('\n');
                        }
                        _ => {}
                    }
                    if self.menu.play_pressed() {
                        let rate = self.menu.difficulty().fountain_every();
                        self.screen = GameScreen::new();
                        self.screen.set_fountain_every(rate);
                        self.state = AppState::Game;
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if button == winit::event::MouseButton::Left {
                    match state {
                        winit::event::ElementState::Pressed => {
                            self.mouse_down = true;
                            if let Some(at) = self.last_cursor {
                                match self.state {
                                    AppState::Menu => self.menu.pointer_down(at.x, at.y),
                                    AppState::Game => {
                                        // HUD overlay buttons take
                                        // priority over game input.
                                        match hud_button_at(
                                            self.screen.view_width(),
                                            self.screen.view_height(),
                                            at.x,
                                            at.y,
                                        ) {
                                            Some(game::HudButton::Pause) => {
                                                self.screen.toggle_pause();
                                            }
                                            Some(game::HudButton::Menu) => {
                                                self.menu.set_best(self.best);
                                                self.state = AppState::Menu;
                                            }
                                            None => self.screen.pointer_down(at.x, at.y),
                                        }
                                    }
                                }
                            }
                        }
                        winit::event::ElementState::Released => {
                            self.mouse_down = false;
                            if let Some(at) = self.last_cursor {
                                match self.state {
                                    AppState::Menu => {
                                        self.menu.set_best(self.best);
                                        self.menu.pointer_up(at.x, at.y);
                                        self.maybe_start_game();
                                    }
                                    AppState::Game => self.screen.pointer_up(at.x, at.y),
                                }
                            }
                        }
                    }
                }
            }
            WindowEvent::Touch(touch) => {
                // Android delivers finger events here in PHYSICAL px;
                // divide by the scale factor (fresco-android M5
                // lesson) - raw coords land 3x off on 3x screens.
                let scale = self
                    .window
                    .as_ref()
                    .map(|w| w.scale_factor())
                    .unwrap_or(1.0);
                let at = widget_kit::LogicalAt {
                    x: touch.location.x / scale,
                    y: touch.location.y / scale,
                };
                if touch.phase == winit::event::TouchPhase::Started {
                    println!(
                        "[fresco-game][dbg] touch started at ({:.1},{:.1}) state={:?}",
                        touch.location.x,
                        touch.location.y,
                        matches!(self.state, AppState::Game)
                    );
                }
                match touch.phase {
                    winit::event::TouchPhase::Started => match self.state {
                        AppState::Menu => self.menu.pointer_down(at.x, at.y),
                        AppState::Game => {
                            match hud_button_at(
                                self.screen.view_width(),
                                self.screen.view_height(),
                                at.x,
                                at.y,
                            ) {
                                Some(game::HudButton::Pause) => self.screen.toggle_pause(),
                                Some(game::HudButton::Menu) => self.state = AppState::Menu,
                                None => self.screen.pointer_down(at.x, at.y),
                            }
                        }
                    },
                    winit::event::TouchPhase::Moved => {
                        if matches!(self.state, AppState::Game) {
                            self.screen.pointer_move(at.x, at.y);
                        }
                    }
                    winit::event::TouchPhase::Ended | winit::event::TouchPhase::Cancelled => {
                        match self.state {
                            AppState::Menu => {
                                self.menu.set_best(self.best);
                                self.menu.pointer_up(at.x, at.y);
                                self.maybe_start_game();
                            }
                            AppState::Game => self.screen.pointer_up(at.x, at.y),
                        }
                    }
                }
                if let Some(window) = self.window.clone() {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

/// Run the game in a desktop window (X11/Wayland). Used to verify the
/// windowed shell before packaging for Android.
///
/// # Errors
/// Fails when the event loop cannot start or the run loop exits
/// with an error.
pub fn run_desktop() -> Result<(), String> {
    let event_loop = EventLoop::new().map_err(|e| format!("event loop: {e}"))?;
    event_loop
        .run_app(&mut GameApp::default())
        .map_err(|e| format!("run: {e}"))
}

/// The persistence directory: Android internal app data (set into
/// [`PERSIST_DIR`] from `android_main`), desktop XDG state, env
/// override wins for tests. Deliberately SEPARATE from the trace
/// capture dir - persistence must not die with trace rotation.
#[must_use]
fn persist_dir() -> std::path::PathBuf {
    if let Some(p) = std::env::var_os("PERSIST_DIR") {
        return std::path::PathBuf::from(p);
    }
    #[cfg(target_os = "android")]
    if let Some(p) = PERSIST_DIR.get() {
        return p.clone();
    }
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_default();
    home.join(".local/state/fresco-game")
}

/// Android internal app data path, set once from `android_main`.
#[cfg(target_os = "android")]
static PERSIST_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// Parse the persisted best score; anything unreadable is 0.
#[must_use]
fn parse_best(bytes: &[u8]) -> u64 {
    let text = String::from_utf8_lossy(bytes);
    let token = text.split_whitespace().next().unwrap_or("0");
    let digits: String = token.chars().filter(|c| c.is_ascii_digit()).collect();
    digits.parse::<u64>().unwrap_or(0)
}

/// Read the persisted best from `<dir>/highscore.txt` (0 when absent).
fn read_best(dir: &std::path::Path) -> u64 {
    let path = dir.join("highscore.txt");
    match std::fs::read(&path) {
        Ok(bytes) => parse_best(&bytes),
        Err(_) => 0,
    }
}

/// Persist the best when it grew.
fn write_best(dir: &std::path::Path, best: u64) {
    let path = dir.join("highscore.txt");
    let _ = std::fs::write(path, format!("{best}\n"));
}

#[cfg(all(target_os = "android", feature = "android-entry"))]
mod android_entry {
    use super::GameApp;
    use winit::event_loop::EventLoop;
    use winit::platform::android::EventLoopBuilderExtAndroid;

    /// Android entry point (NativeActivity).
    ///
    /// # Panics
    /// Panics when the event loop or run loop fails - fatal for a
    /// game shell; stdout routes to logcat.
    #[unsafe(no_mangle)]
    fn android_main(app: android_activity::AndroidApp) {
        println!("[fresco-game] android_main: starting game demo");
        // Persistence home: the app's internal data dir (private,
        // survives trace rotation).
        if let Some(p) = app.internal_data_path() {
            let _ = super::PERSIST_DIR.set(p);
        }
        // Infallible by policy: a failed loop logs and lets the app
        // exit cleanly rather than crashing the process.
        let Ok(event_loop) = EventLoop::builder().with_android_app(app).build() else {
            println!("[fresco-game] event loop failed");
            return;
        };
        if let Err(e) = event_loop.run_app(&mut GameApp::default()) {
            println!("[fresco-game] run failed: {e}");
        }
    }
}

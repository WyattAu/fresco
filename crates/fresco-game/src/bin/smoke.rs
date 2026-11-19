//! Headless smoke runner: step + render N frames of the game through
//! the inspector into the capture dir (no window). Mirrors the
//! on-device observability pipeline, including a tap burst and the
//! fountain.
fn main() {
    let frames: usize = std::env::var("GAME_FRAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(120);
    let mut surface = paint_kit::Surface::new(360, 740).unwrap_or_default();
    let mut painter = paint_kit::Painter::default();
    let mut inspector = window_kit::inspector::Inspector::from_env();
    let mut game = fresco_game::game::GameScreen::new();
    game.resize(360, 740);
    game.pointer_down(180.0, 300.0);
    let mut sim_us = 0u128;
    let mut paint_us_total = 0u128;
    for _k in 0..frames {
        let t0 = std::time::Instant::now();
        game.step();
        sim_us += t0.elapsed().as_micros();
        let t1 = std::time::Instant::now();
        game.paint(&mut surface, &mut painter);
        paint_us_total += t1.elapsed().as_micros();
        inspector.record_frame(surface.data(), surface.width(), surface.height(), 0, 0);
    }
    println!(
        "[profile] sim {sim_us}us paint {paint_us_total}us over {frames}f = {}us/frame paint",
        paint_us_total / u128::try_from(frames.max(1)).unwrap_or(1)
    );
    let analysis = inspector.analyze();
    println!("{analysis}");
    println!(
        "[fresco-game] smoke done: {} frames, {} balls alive",
        frames,
        game.entity_count()
    );

    // Menu evidence: one deterministic menu frame next to the game
    // frames (same observability dir).
    let mut menu = fresco_game::menu::MenuScreen::new();
    menu.resize(360, 740);
    let mut menu_surface = paint_kit::Surface::new(360, 740).unwrap_or_default();
    let mut menu_painter = paint_kit::Painter::default();
    menu.paint(&mut menu_surface, &mut menu_painter);
    let dir = inspector.dir();
    let path = dir.join("menu.bmp");
    let _ = window_kit::inspector::write_bmp24(
        &path,
        menu_surface.data(),
        menu_surface.width(),
        menu_surface.height(),
    );
    println!("[fresco-game] menu evidence: {}", path.display());
}

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
    for _k in 0..frames {
        let paint_start = std::time::Instant::now();
        game.step();
        if _k % 15 == 0 {
            let c = game.ball_centers();
            if let (Some(first), Some(last)) = (c.first(), c.last()) {
                println!(
                    "[probe] f{}=({:.1},{:.1}) bot=({:.1},{:.1})",
                    _k, first.0, first.1, last.0, last.1
                );
            }
        }
        game.paint(&mut surface, &mut painter);
        let paint_us = paint_start.elapsed().as_micros() as u64;
        inspector.record_frame(
            surface.data(),
            surface.width(),
            surface.height(),
            paint_us,
            0,
        );
    }
    for (k, (x, y)) in game.ball_centers().iter().enumerate().take(6) {
        println!("[probe] ball {k}: x={x:.1} y={y:.1}");
    }
    let analysis = inspector.analyze();
    println!("{analysis}");
    println!(
        "[fresco-game] smoke done: {} frames, {} balls alive",
        frames,
        game.entity_count()
    );
}

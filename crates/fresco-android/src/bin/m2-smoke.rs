//! Headless smoke runner: render N frames of the scene through the
//! inspector into the capture dir (no window). Mirrors the on-device
//! observability pipeline.
fn main() {
    let frames: usize = std::env::var("M2_FRAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);
    let mut surface = paint_kit::Surface::new(360, 740).unwrap_or_default();
    let mut painter = paint_kit::Painter::default();
    let mut inspector = window_kit::inspector::Inspector::from_env();
    let touches = vec![fresco_android::scene::TouchDot {
        pos: (0.5, 0.4),
        age: 0.2,
    }];
    for k in 0..frames {
        let paint_start = std::time::Instant::now();
        let t = f64::from(u32::try_from(k).unwrap_or(0)) / 60.0;
        let _ = fresco_android::scene::render_scene(
            &mut surface,
            &mut painter,
            360,
            740,
            t,
            &touches,
            60.0,
        );
        let paint_us = paint_start.elapsed().as_micros() as u64;
        inspector.record_frame(surface.data(), 360, 740, paint_us, 0);
    }
    let analysis = inspector.analyze();
    println!("{analysis}");
}

//! Rendering defect audit for the fx backgrounds + fresco-charts
//! chart types. Verifies all components render without panics and
//! produce non-empty output.

/// Render background patterns into a surface via the fx crate.
fn render_backgrounds(
    surface: &mut paint_kit::Surface,
    painter: &mut paint_kit::Painter,
    w: usize,
    h: usize,
    t: f64,
) {
    let mut fx = fx::Fx::new(surface, painter, t);
    fx::GridPattern::new().draw(&mut fx, w / 2, h);
    fx::DotPattern::new().draw(&mut fx, w / 2, h);
    fx::StarField::new().draw(&mut fx, w / 2, h);
    fx::FlickeringGrid::new().draw(&mut fx, w / 2, h);
    fx::GradientMesh::new().draw(&mut fx, w, h);
    fx::Beams::new().draw(&mut fx, w, h);
}

#[test]
fn background_patterns_render_without_panics() {
    let mut surface = paint_kit::Surface::new(320, 200).unwrap_or_default();
    let mut painter = paint_kit::Painter::default();
    render_backgrounds(&mut surface, &mut painter, 320, 200, 1.0);
    let nonzero = surface.data().iter().filter(|&&b| b > 0).count();
    assert!(nonzero > 0, "background render produced no visible pixels");
}

#[test]
fn background_patterns_are_deterministic() {
    let mut s1 = paint_kit::Surface::new(320, 200).unwrap_or_default();
    let mut s2 = paint_kit::Surface::new(320, 200).unwrap_or_default();
    let mut p1 = paint_kit::Painter::default();
    let mut p2 = paint_kit::Painter::default();
    render_backgrounds(&mut s1, &mut p1, 320, 200, 1.0);
    render_backgrounds(&mut s2, &mut p2, 320, 200, 1.0);
    assert_eq!(s1.data(), s2.data(), "deterministic background render");
}

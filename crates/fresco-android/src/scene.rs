//! The M2 showcase scene: deterministic paint callable from both
//! the desktop headless twin and the Android winit loop.
//!
//! A dark slate background with an animated highlight sweeping the
//! screen, plus touch trail markers — every input visibly lands.
//! Pure paint-kit, no windowing: the same function renders to a
//! desktop Surface and to the Android present surface.

use paint_kit::{Paint, Painter, Rect, Solid, Surface, Vec2};

/// A live touch point (finger position + age in seconds).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchDot {
    /// Normalized position in `[0, 1]` on both axes.
    pub pos: (f64, f64),
    /// Seconds since the touch landed.
    pub age: f64,
}

/// Render one frame of the showcase scene.
///
/// * `width`/`height`: pixel dimensions of the surface.
/// * `t`: animation clock in seconds.
/// * `touches`: live touch trail (normalized coordinates).
/// * `fps`: measured frames-per-second (drawn on-screen when > 0).
///
/// Returns the number of paint ops issued (observability counter).
#[must_use]
pub fn render_scene(
    surface: &mut Surface,
    painter: &mut Painter,
    width: usize,
    height: usize,
    t: f64,
    touches: &[TouchDot],
    fps: f64,
) -> usize {
    let w = width as f64;
    let h = height as f64;
    let mut ops = 0_usize;

    // Background: dark slate.
    painter.fill_rect(
        surface,
        Rect::from_size(Vec2::ZERO, Vec2::new(w, h)),
        &Paint::from(Solid::premul([22, 24, 32, 255])),
    );
    ops += 1;

    // Animated highlight: a soft band sweeping vertically.
    let band_y = (t * 120.0) % (h + 160.0) - 80.0;
    let band_alpha = 46_u8;
    painter.fill_rect(
        surface,
        Rect::from_size(Vec2::new(0.0, band_y), Vec2::new(w, 120.0)),
        &Paint::from(Solid::premul([60, 90, 160, band_alpha])),
    );
    ops += 1;

    // Grid ticks: a faint 48 px lattice (the "fresco UI" identity).
    let cell = 48.0;
    let mut x = 0.0;
    while x <= w {
        painter.fill_rect(
            surface,
            Rect::from_size(Vec2::new(x, 0.0), Vec2::new(1.0, h)),
            &Paint::from(Solid::premul([70, 74, 88, 110])),
        );
        x += cell;
        ops += 1;
    }
    let mut y = 0.0;
    while y <= h {
        painter.fill_rect(
            surface,
            Rect::from_size(Vec2::new(0.0, y), Vec2::new(w, 1.0)),
            &Paint::from(Solid::premul([70, 74, 88, 110])),
        );
        y += cell;
        ops += 1;
    }

    // Touch trail: rings fading by age, newest brightest.
    for touch in touches {
        let fade = (1.0 - touch.age / 2.0).clamp(0.15, 1.0);
        let a = (f64::from(220) * fade) as u8;
        let cx = touch.pos.0 * w;
        let cy = touch.pos.1 * h;
        let r = 10.0 + 14.0 * (1.0 - fade);
        // Four concentric squares approximating a ring.
        for (rr, aa) in [(r, a), (r * 0.7, a / 2), (r * 0.45, a)] {
            painter.fill_rect(
                surface,
                Rect::from_size(Vec2::new(cx - rr, cy - rr), Vec2::new(rr * 2.0, rr * 2.0)),
                &Paint::from(Solid::premul([90, 200, 160, aa])),
            );
            ops += 1;
        }
    }

    // FPS badge (top-left): painted as bars + text-free block so the
    // headless twin stays glyph-free; height reflects measured fps.
    if fps > 0.0 {
        let fps_h = (fps / 120.0).clamp(0.05, 1.0) * 60.0;
        painter.fill_rect(
            surface,
            Rect::from_size(Vec2::new(8.0, 8.0), Vec2::new(8.0, fps_h)),
            &Paint::from(Solid::premul([120, 220, 140, 230])),
        );
        ops += 1;
    }

    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The desktop headless twin: the same render_scene the Android
    /// app calls, driven through the Inspector for capture.
    #[test]
    fn scene_renders_deterministically() {
        let render = |t: f64| -> Vec<u8> {
            let mut surface = Surface::new(360, 740).unwrap_or_default();
            let mut painter = Painter::default();
            let touches = vec![TouchDot {
                pos: (0.5, 0.4),
                age: 0.3,
            }];
            let _ = render_scene(&mut surface, &mut painter, 360, 740, t, &touches, 60.0);
            surface.data().to_vec()
        };
        let a = render(0.5);
        let b = render(0.5);
        assert_eq!(a, b, "scene deterministic at fixed t");
        let c = render(0.9);
        assert_ne!(a, c, "animation advances with t");
    }

    #[test]
    fn scene_marks_touched_region() {
        let mut untouched = Surface::new(360, 740).unwrap_or_default();
        let mut painter = Painter::default();
        let _ = render_scene(&mut untouched, &mut painter, 360, 740, 0.0, &[], 0.0);

        let mut touched = Surface::new(360, 740).unwrap_or_default();
        let mut painter2 = Painter::default();
        let _ = render_scene(
            &mut touched,
            &mut painter2,
            360,
            740,
            0.0,
            &[TouchDot {
                pos: (0.5, 0.5),
                age: 0.0,
            }],
            0.0,
        );
        assert_ne!(untouched.data(), touched.data(), "touch visibly lands");
    }
}

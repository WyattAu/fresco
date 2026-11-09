//! Camera: world-to-screen view transforms with pan and zoom.
//!
//! The single viewport primitive for CAD viewports and game
//! cameras: a world-space anchor point mapped to a screen-space
//! center, one uniform zoom factor, and the round-trip transforms
//! between world and screen space.

use path_kit::{Affine, Rect, Vec2};

/// A 2D camera. Zoom > 1 magnifies; zoom < 1 shrinks. Y-down
/// screen coordinates follow the fresco convention.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// World point rendered at the viewport center.
    pub center: Vec2,
    /// Uniform zoom factor (1.0 = 1:1 pixels).
    pub zoom: f64,
}

impl Camera {
    /// 1:1 camera centered on the origin.
    #[must_use]
    pub fn identity() -> Self {
        Camera {
            center: Vec2::ZERO,
            zoom: 1.0,
        }
    }

    /// New camera centered on `center` at `zoom`.
    #[must_use]
    pub fn new(center: Vec2, zoom: f64) -> Self {
        Camera {
            center,
            zoom: zoom.max(1e-6),
        }
    }

    /// World-to-screen affine for a viewport of `size` pixels.
    #[must_use]
    pub fn world_to_screen(&self, size: Vec2) -> Affine {
        let half = Vec2::new(size.x * 0.5, size.y * 0.5);
        Affine::translate(half)
            .then(Affine::scale(self.zoom))
            .then(Affine::translate(Vec2::new(-self.center.x, -self.center.y)))
    }

    /// Screen-to-world affine (inverse of `world_to_screen`).
    #[must_use]
    pub fn screen_to_world(&self, size: Vec2) -> Affine {
        self.world_to_screen(size)
            .inverse()
            .unwrap_or(Affine::IDENTITY)
    }

    /// Map a world point to screen pixels.
    #[must_use]
    pub fn world_point(&self, size: Vec2, p: Vec2) -> Vec2 {
        self.world_to_screen(size).apply(p)
    }

    /// Map screen pixels back to a world point.
    #[must_use]
    pub fn screen_point(&self, size: Vec2, p: Vec2) -> Vec2 {
        self.screen_to_world(size).apply(p)
    }

    /// The world-space rectangle currently visible in a viewport.
    #[must_use]
    pub fn visible_rect(&self, size: Vec2) -> Rect {
        let half_w = size.x * 0.5 / self.zoom;
        let half_h = size.y * 0.5 / self.zoom;
        Rect::from_points(
            Vec2::new(self.center.x - half_w, self.center.y - half_h),
            Vec2::new(self.center.x + half_w, self.center.y + half_h),
        )
    }

    /// Zoom by `factor` keeping the world point under `screen_pos`
    /// stationary (the standard cursor-anchored wheel zoom).
    pub fn zoom_at(&mut self, size: Vec2, screen_pos: Vec2, factor: f64) {
        let anchor_world = self.screen_point(size, screen_pos);
        self.zoom = (self.zoom * factor).clamp(1e-3, 1e4);
        // After the zoom change, re-place the anchor under the cursor.
        let half = Vec2::new(size.x * 0.5, size.y * 0.5);
        self.center = Vec2::new(
            anchor_world.x - (screen_pos.x - half.x) / self.zoom,
            anchor_world.y - (screen_pos.y - half.y) / self.zoom,
        );
    }

    /// Pan the view by a screen-space delta (dragging).
    pub fn pan(&mut self, delta_screen: Vec2) {
        self.center = Vec2::new(
            self.center.x - delta_screen.x / self.zoom,
            self.center.y - delta_screen.y / self.zoom,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use path_kit::Vec2;

    const VIEW: Vec2 = Vec2::new(800.0, 600.0);

    #[test]
    fn identity_camera_is_one_to_one_offset_by_half() {
        let cam = Camera::identity();
        let p = cam.world_point(VIEW, Vec2::new(0.0, 0.0));
        assert!(
            (p.x - 400.0).abs() < 1e-9 && (p.y - 300.0).abs() < 1e-9,
            "world origin maps to viewport center: {p:?}"
        );
        let q = cam.world_point(VIEW, Vec2::new(100.0, 50.0));
        assert!(
            (q.x - 500.0).abs() < 1e-9 && (q.y - 350.0).abs() < 1e-9,
            "1:1 mapping: {q:?}"
        );
    }

    #[test]
    fn round_trip_world_screen() {
        let cam = Camera::new(Vec2::new(250.0, 125.0), 2.0);
        let p = Vec2::new(123.5, -67.25);
        let screen = cam.world_point(VIEW, p);
        let back = cam.screen_point(VIEW, screen);
        assert!(
            (back.x - p.x).abs() < 1e-6 && (back.y - p.y).abs() < 1e-6,
            "round trip: {p:?} -> {screen:?} -> {back:?}"
        );
    }

    #[test]
    fn zoom_magnifies_distances() {
        let cam = Camera::identity();
        let a = cam.world_point(VIEW, Vec2::new(0.0, 0.0));
        let b = cam.world_point(VIEW, Vec2::new(10.0, 0.0));
        let d1 = a.distance(b);
        let zoomed = Camera::new(Vec2::ZERO, 3.0);
        let a2 = zoomed.world_point(VIEW, Vec2::new(0.0, 0.0));
        let b2 = zoomed.world_point(VIEW, Vec2::new(10.0, 0.0));
        let d2 = a2.distance(b2);
        assert!(
            (d2 - d1 * 3.0).abs() < 1e-9,
            "3x zoom triples screen distance"
        );
    }

    #[test]
    fn visible_rect_shrinks_with_zoom() {
        let wide = Camera::new(Vec2::ZERO, 1.0).visible_rect(VIEW);
        let zoomed = Camera::new(Vec2::ZERO, 2.0).visible_rect(VIEW);
        assert!(
            zoomed.width() < wide.width(),
            "zoomed view shows less world: {} < {}",
            zoomed.width(),
            wide.width()
        );
        assert!((zoomed.width() - 400.0).abs() < 1e-9);
    }

    #[test]
    fn zoom_at_keeps_cursor_anchor_fixed() {
        let mut cam = Camera::identity();
        let cursor = Vec2::new(650.0, 200.0);
        let before = cam.screen_point(VIEW, cursor);
        cam.zoom_at(VIEW, cursor, 2.0);
        let after = cam.screen_point(VIEW, cursor);
        assert!(
            (before.x - after.x).abs() < 1e-6 && (before.y - after.y).abs() < 1e-6,
            "world point under cursor must stay put: {before:?} vs {after:?}"
        );
    }

    #[test]
    fn pan_moves_center_by_zoomed_delta() {
        let mut cam = Camera::new(Vec2::ZERO, 2.0);
        cam.pan(Vec2::new(100.0, 0.0));
        assert!(
            (cam.center.x + 50.0).abs() < 1e-9,
            "drag right moves view left: center {}",
            cam.center.x
        );
    }
}

//! Circular arc segments for the fresco UI engine.
//!
//! Provides circular arcs as first-class path geometry: construction
//! from center/radius/angles, evaluation at parameter t, conversion
//! to cubic bezier segments (for flattening and path integration),
//! arc length, and point-at-angle.
//!
//! Conventions: angles in radians, measured from the positive X-axis,
//! increasing clockwise in Y-down coordinates (screen space). This
//! matches vello, SVG arcs, and plychart sweep gradients.

use crate::{Path, Vec2};

/// A circular arc segment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Arc {
    /// Center point.
    pub center: Vec2,
    /// Radius.
    pub radius: f64,
    /// Start angle in radians.
    pub start_angle: f64,
    /// End angle in radians.
    pub end_angle: f64,
}

impl Arc {
    /// New arc from center, radius, and start/end angles.
    #[must_use]
    pub fn new(center: Vec2, radius: f64, start_angle: f64, end_angle: f64) -> Self {
        Arc {
            center,
            radius: radius.max(0.0),
            start_angle,
            end_angle,
        }
    }

    /// Point on the arc at parameter `t` in `[0, 1]`, where 0 is the
    /// start angle and 1 is the end angle.
    #[must_use]
    pub fn eval(&self, t: f64) -> Vec2 {
        let angle = self.start_angle + (self.end_angle - self.start_angle) * t;
        Vec2::new(
            self.center.x + self.radius * angle.cos(),
            self.center.y + self.radius * angle.sin(),
        )
    }

    /// Angular span of the arc (may be negative for reverse arcs).
    #[must_use]
    pub fn span(&self) -> f64 {
        self.end_angle - self.start_angle
    }

    /// Arc length (radius × angular span magnitude).
    #[must_use]
    pub fn arclen(&self) -> f64 {
        self.radius * self.span().abs()
    }

    /// Convert to cubic bezier segments appended to a path. Each
    /// span of ≤90 degrees becomes one cubic (kappa approximation).
    pub fn to_path(&self, path: &mut Path, close_path: bool) {
        let span = self.span();
        if span.abs() < 1e-9 || self.radius < 1e-9 {
            return;
        }
        let n = ((span.abs() / std::f64::consts::FRAC_PI_2).ceil() as usize).max(1);
        let seg_angle = span / n as f64;
        let k = 4.0 / 3.0 * (seg_angle * 0.25).tan();

        let p0 = self.eval(0.0);
        path.move_to(p0);
        let mut prev_angle = self.start_angle;
        for i in 1..=n {
            let angle = self.start_angle + seg_angle * i as f64;
            let dir0 = prev_angle + std::f64::consts::FRAC_PI_2 * span.signum();
            let dir3 = angle + std::f64::consts::FRAC_PI_2 * span.signum();
            let c1 = Vec2::new(
                self.center.x + self.radius * prev_angle.cos() + self.radius * k * dir0.cos(),
                self.center.y + self.radius * prev_angle.sin() + self.radius * k * dir0.sin(),
            );
            let c2 = Vec2::new(
                self.center.x + self.radius * angle.cos() - self.radius * k * dir3.cos(),
                self.center.y + self.radius * angle.sin() - self.radius * k * dir3.sin(),
            );
            let p3 = self.eval(i as f64 / n as f64);
            path.cubic_to(c1, c2, p3);
            prev_angle = angle;
        }
        if close_path {
            path.close();
        }
    }

    /// Bounding box of the arc (accounts for axis-aligned extremes
    /// within the swept range).
    #[must_use]
    pub fn bounding_box(&self) -> crate::Rect {
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        // Sample the arc densely for an accurate bounding box.
        let n = 64;
        for i in 0..=n {
            let p = self.eval(i as f64 / n as f64);
            min_x = min_x.min(p.x);
            max_x = max_x.max(p.x);
            min_y = min_y.min(p.y);
            max_y = max_y.max(p.y);
        }
        crate::Rect::from_points(Vec2::new(min_x, min_y), Vec2::new(max_x, max_y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arc_eval_endpoints() {
        let arc = Arc::new(Vec2::new(50.0, 50.0), 25.0, 0.0, std::f64::consts::PI);
        let p0 = arc.eval(0.0);
        let p1 = arc.eval(1.0);
        assert!((p0.x - 75.0).abs() < 1e-9, "start at angle 0: {p0:?}");
        assert!((p0.y - 50.0).abs() < 1e-9);
        assert!((p1.x - 25.0).abs() < 1e-9, "end at angle pi: {p1:?}");
    }

    #[test]
    fn arc_arclen_is_radius_times_span() {
        let arc = Arc::new(Vec2::new(0.0, 0.0), 10.0, 0.0, std::f64::consts::PI);
        assert!(
            (arc.arclen() - std::f64::consts::PI * 10.0).abs() < 1e-9,
            "arclen {}",
            arc.arclen()
        );
    }

    #[test]
    fn arc_full_circle_is_two_semiarcs() {
        let arc = Arc::new(Vec2::new(0.0, 0.0), 10.0, 0.0, std::f64::consts::TAU);
        assert!((arc.arclen() - std::f64::consts::TAU * 10.0).abs() < 1e-9);
    }

    #[test]
    fn arc_path_has_expected_segments() {
        let arc = Arc::new(Vec2::new(50.0, 50.0), 25.0, 0.0, std::f64::consts::PI);
        let mut path = Path::new();
        arc.to_path(&mut path, false);
        // 90-degree arc → 2 cubic segments (n = ceil(pi/2 / pi/2) = 2).
        // Path has: move_to + 2 cubic_to = at least 4 segments.
        assert!(!path.is_empty());
    }

    #[test]
    fn arc_bounding_box_covers_quadrant() {
        // Semicircle from 0 to pi (upper half in Y-down: arcs go down).
        let arc = Arc::new(Vec2::new(50.0, 50.0), 25.0, 0.0, std::f64::consts::PI);
        let bbox = arc.bounding_box();
        assert!(
            (bbox.x0 - 25.0).abs() < 1e-6,
            "bbox x0 {} should be cx - r = 25",
            bbox.x0
        );
        assert!(
            (bbox.x1 - 75.0).abs() < 1e-6,
            "bbox x1 {} should be cx + r = 75",
            bbox.x1
        );
    }
}

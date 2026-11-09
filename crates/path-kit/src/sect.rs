//! Intersection primitives: segment-segment, segment-ray, ray-rect.
//!
//! Used for CAD snapping, selection lassos, and game raycasts.

use crate::{Rect, Vec2};

/// Closest point on segment `a-b` to `p`, with the clamped
/// parameter `t` in `[0, 1]` along the segment.
#[must_use]
pub fn closest_point_on_segment(a: Vec2, b: Vec2, p: Vec2) -> (Vec2, f64) {
    let ab = Vec2::new(b.x - a.x, b.y - a.y);
    let len2 = ab.len2();
    if len2 < 1e-12 {
        return (a, 0.0);
    }
    let t = (((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / len2).clamp(0.0, 1.0);
    let q = Vec2::new(a.x + ab.x * t, a.y + ab.y * t);
    (q, t)
}

/// Segment-segment intersection: `Some(point)` when the closed
/// segments `a0-a1` and `b0-b1` cross (endpoints count as crossing).
#[must_use]
pub fn segment_segment(a0: Vec2, a1: Vec2, b0: Vec2, b1: Vec2) -> Option<Vec2> {
    let d1 = Vec2::new(a1.x - a0.x, a1.y - a0.y);
    let d2 = Vec2::new(b1.x - b0.x, b1.y - b0.y);
    let denom = d1.cross(d2);
    if denom.abs() < 1e-12 {
        return None; // parallel or collinear: treat as no single point
    }
    let t = (b0.x - a0.x) * d2.y - (b0.y - a0.y) * d2.x;
    let u = (b0.x - a0.x) * d1.y - (b0.y - a0.y) * d1.x;
    let t = t / denom;
    let u = u / denom;
    if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
        Some(Vec2::new(a0.x + d1.x * t, a0.y + d1.y * t))
    } else {
        None
    }
}

/// Ray-AABB intersection (slab method): entry distance along the
/// ray and hit point, when the ray `origin + t * dir` (t >= 0)
/// hits the axis-aligned box.
#[must_use]
pub fn ray_rect(origin: Vec2, dir: Vec2, rect: Rect) -> Option<(f64, Vec2)> {
    let inv_x = if dir.x.abs() > 1e-12 {
        1.0 / dir.x
    } else {
        f64::INFINITY * dir.x.signum()
    };
    let inv_y = if dir.y.abs() > 1e-12 {
        1.0 / dir.y
    } else {
        f64::INFINITY * dir.y.signum()
    };
    let t1 = (rect.x0 - origin.x) * inv_x;
    let t2 = (rect.x1 - origin.x) * inv_x;
    let t3 = (rect.y0 - origin.y) * inv_y;
    let t4 = (rect.y1 - origin.y) * inv_y;
    let tmin = t1.min(t2).max(t3.min(t4));
    let tmax = t1.max(t2).min(t3.max(t4));
    if tmax < 0.0 || tmin > tmax {
        return None;
    }
    let t = if tmin < 0.0 { 0.0 } else { tmin };
    Some((t, Vec2::new(origin.x + dir.x * t, origin.y + dir.y * t)))
}

/// Segment-rect intersection: whether the closed segment crosses
/// the box (either endpoint inside or the segment crosses an edge).
#[must_use]
pub fn segment_rect(a: Vec2, b: Vec2, rect: Rect) -> bool {
    if rect.contains(a) || rect.contains(b) {
        return true;
    }
    let corners = [
        Vec2::new(rect.x0, rect.y0),
        Vec2::new(rect.x1, rect.y0),
        Vec2::new(rect.x1, rect.y1),
        Vec2::new(rect.x0, rect.y1),
    ];
    for i in 0..4 {
        let c0 = corners.get(i).copied().unwrap_or_default();
        let c1 = corners.get((i + 1) % 4).copied().unwrap_or_default();
        if segment_segment(a, b, c0, c1).is_some() {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vec2;

    #[test]
    fn closest_point_midpoint_projection() {
        let (q, t) = closest_point_on_segment(
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(5.0, 3.0),
        );
        assert!((q.x - 5.0).abs() < 1e-9 && q.y.abs() < 1e-9);
        assert!((t - 0.5).abs() < 1e-9);
    }

    #[test]
    fn closest_point_clamps_to_endpoints() {
        let (q, t) = closest_point_on_segment(
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(15.0, 4.0),
        );
        assert!((q.x - 10.0).abs() < 1e-9, "clamped to far end: {q:?}");
        assert!((t - 1.0).abs() < 1e-9);
    }

    #[test]
    fn segments_cross_at_center() {
        let hit = segment_segment(
            Vec2::new(-5.0, 0.0),
            Vec2::new(5.0, 0.0),
            Vec2::new(0.0, -5.0),
            Vec2::new(0.0, 5.0),
        );
        assert!(hit.is_some(), "crossing segments intersect");
        let hit = hit.unwrap_or_default();
        assert!(hit.x.abs() < 1e-9 && hit.y.abs() < 1e-9);
    }

    #[test]
    fn parallel_segments_do_not_cross() {
        let hit = segment_segment(
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(10.0, 1.0),
        );
        assert!(
            hit.is_none(),
            "parallel segments have no single cross point"
        );
    }

    #[test]
    fn disjoint_segments_do_not_cross() {
        let hit = segment_segment(
            Vec2::new(0.0, 0.0),
            Vec2::new(4.0, 0.0),
            Vec2::new(6.0, 0.0),
            Vec2::new(10.0, 0.0),
        );
        assert!(hit.is_none(), "collinear disjoint segments");
    }

    #[test]
    fn ray_hits_rect_front() {
        let rect = Rect::from_points(Vec2::new(4.0, -2.0), Vec2::new(8.0, 2.0));
        let hit = ray_rect(Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), rect);
        assert!(hit.is_some(), "ray must hit the box");
        let (t, p) = hit.unwrap_or((0.0, Vec2::ZERO));
        assert!((t - 4.0).abs() < 1e-9, "entry at t=4, got {t}");
        assert!((p.x - 4.0).abs() < 1e-9);
    }

    #[test]
    fn ray_misses_rect() {
        let rect = Rect::from_points(Vec2::new(4.0, -2.0), Vec2::new(8.0, 2.0));
        let hit = ray_rect(Vec2::new(0.0, 0.0), Vec2::new(0.0, 1.0), rect);
        assert!(hit.is_none(), "parallel ray misses");
    }

    #[test]
    fn ray_from_inside_starts_at_zero() {
        let rect = Rect::from_points(Vec2::new(-5.0, -5.0), Vec2::new(5.0, 5.0));
        let hit = ray_rect(Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), rect);
        assert!(hit.is_some());
        let (t, _) = hit.unwrap_or((0.0, Vec2::ZERO));
        assert!(t.abs() < 1e-9, "origin inside: t=0, got {t}");
    }

    #[test]
    fn segment_crossing_rect_edges() {
        let rect = Rect::from_points(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0));
        assert!(segment_rect(
            Vec2::new(-5.0, 5.0),
            Vec2::new(15.0, 5.0),
            rect
        ));
        assert!(!segment_rect(
            Vec2::new(-5.0, 15.0),
            Vec2::new(15.0, 15.0),
            rect
        ));
        assert!(segment_rect(Vec2::new(5.0, 5.0), Vec2::new(6.0, 6.0), rect));
    }
}

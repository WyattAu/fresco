//! Adaptive flattening under a caller tolerance.
//!
//! Guarantee (REQ-P1): each curve stays inside the convex hull of its
//! control points, so subdividing while the farthest control point is more
//! than `tol` from the chord keeps every curve point within `tol` of the
//! emitted chord. Recursion is capped at [`crate::MAX_DEPTH`]; hitting the
//! cap means the tolerance is unachievable at this depth and the caller
//! should raise it.

use crate::bez::{CubicBez, QuadBez};
use crate::geom::Vec2;

/// Flatten a cubic into chord end points, excluding its start point.
pub fn flatten_cubic(c: &CubicBez, tol: f64, depth: u32, out: &mut std::vec::Vec<Vec2>) {
    if depth >= crate::MAX_DEPTH || c.flatness() <= tol {
        out.push(c.p3);
    } else {
        let (a, b) = c.split(0.5);
        flatten_cubic(&a, tol, depth + 1, out);
        flatten_cubic(&b, tol, depth + 1, out);
    }
}

/// Flatten a quadratic into chord end points, excluding its start point.
pub fn flatten_quad(q: &QuadBez, tol: f64, depth: u32, out: &mut std::vec::Vec<Vec2>) {
    if depth >= crate::MAX_DEPTH || q.flatness() <= tol {
        out.push(q.p2);
    } else {
        let (a, b) = q.split(0.5);
        flatten_quad(&a, tol, depth + 1, out);
        flatten_quad(&b, tol, depth + 1, out);
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn dist_point_segment(p: Vec2, a: Vec2, b: Vec2) -> f64 {
        let ab = b - a;
        let l2 = ab.len2();
        if l2 == 0.0 {
            return p.distance(a);
        }
        let t = ((p - a).dot(ab) / l2).clamp(0.0, 1.0);
        p.distance(a + ab * t)
    }

    fn dist_to_polyline(p: Vec2, pts: &[Vec2]) -> f64 {
        let mut best = f64::INFINITY;
        for w in pts.windows(2) {
            if let [a, b] = w {
                best = best.min(dist_point_segment(p, *a, *b));
            }
        }
        best
    }

    #[test]
    fn flatten_tolerance_cubic() {
        let c = CubicBez {
            p0: Vec2::ZERO,
            p1: Vec2::new(0.0, 40.0),
            p2: Vec2::new(40.0, 40.0),
            p3: Vec2::new(40.0, 0.0),
        };
        let tol = 0.25;
        let mut pts = vec![c.p0];
        flatten_cubic(&c, tol, 0, &mut pts);
        assert_eq!(*pts.first().unwrap_or(&Vec2::ZERO), c.p0);
        assert_eq!(*pts.last().unwrap_or(&Vec2::ZERO), c.p3);
        // Sample the true curve densely; every point within tol of polyline.
        for i in 0..=400 {
            let t = f64::from(i) / 400.0;
            let p = c.eval(t);
            assert!(
                dist_to_polyline(p, &pts) <= tol * (1.0 + 1e-9),
                "curve point {t} beyond tolerance"
            );
        }
    }

    #[test]
    fn flatten_tolerance_quad() {
        let q = QuadBez {
            p0: Vec2::ZERO,
            p1: Vec2::new(50.0, 100.0),
            p2: Vec2::new(100.0, 0.0),
        };
        let tol = 0.5;
        let mut pts = vec![q.p0];
        flatten_quad(&q, tol, 0, &mut pts);
        for i in 0..=400 {
            let t = f64::from(i) / 400.0;
            let p = q.eval(t);
            assert!(dist_to_polyline(p, &pts) <= tol * (1.0 + 1e-9));
        }
    }

    #[test]
    fn flatten_coarser_tol_fewer_points() {
        let c = CubicBez {
            p0: Vec2::ZERO,
            p1: Vec2::new(0.0, 40.0),
            p2: Vec2::new(40.0, 40.0),
            p3: Vec2::new(40.0, 0.0),
        };
        let mut fine = vec![c.p0];
        flatten_cubic(&c, 0.05, 0, &mut fine);
        let mut coarse = vec![c.p0];
        flatten_cubic(&c, 5.0, 0, &mut coarse);
        assert!(coarse.len() < fine.len());
    }
}

//! Winding-number computation and point-in-path hit testing.

use crate::geom::Vec2;
use crate::path::Poly;

/// Fill rule for containment decisions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillRule {
    /// Interior is where winding is nonzero.
    NonZero,
    /// Interior is where crossing parity is odd.
    EvenOdd,
}

/// Signed winding number of a closed polygon around `point`.
///
/// Uses the PNPOLY half-open crossing rule so vertices are counted
/// deterministically. Positive for clockwise windings in y-down screen
/// coordinates.
#[must_use]
pub fn winding_number(point: Vec2, poly: &Poly) -> i32 {
    let mut winding = 0;
    let pts = &poly.points;
    let n = pts.len();
    if n < 3 {
        return 0;
    }
    // Cyclic pairs via windows + wrap pair, no indexing.
    let mut prev = match pts.last() {
        Some(&p) => p,
        None => return 0,
    };
    for p in pts.iter().take(n.saturating_sub(1)) {
        winding += crossing(prev, *p, point);
        prev = *p;
    }
    winding += crossing(prev, pts.first().copied().unwrap_or(prev), point);
    winding
}

fn crossing(p: Vec2, q: Vec2, point: Vec2) -> i32 {
    if (p.y > point.y) != (q.y > point.y) {
        let x_int = (q.x - p.x).mul_add(point.y - p.y, 0.0) / (q.y - p.y) + p.x;
        if point.x < x_int {
            return if q.y > p.y { 1 } else { -1 };
        }
    }
    0
}

/// Point containment over a set of subpath polygons.
#[must_use]
pub fn point_in_path(point: Vec2, polys: &[Poly], rule: FillRule) -> bool {
    let mut winding = 0;
    for poly in polys {
        winding += winding_number(point, poly);
    }
    match rule {
        FillRule::NonZero => winding != 0,
        FillRule::EvenOdd => winding % 2 != 0,
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn square(x0: f64, y0: f64, size: f64) -> Poly {
        Poly {
            points: vec![
                Vec2::new(x0, y0),
                Vec2::new(x0 + size, y0),
                Vec2::new(x0 + size, y0 + size),
                Vec2::new(x0, y0 + size),
            ],
            closed: true,
        }
    }

    #[test]
    fn inside_outside_winding() {
        let s = square(0.0, 0.0, 10.0);
        assert_eq!(winding_number(Vec2::new(5.0, 5.0), &s), 1);
        assert_eq!(winding_number(Vec2::new(15.0, 5.0), &s), 0);
        assert_eq!(winding_number(Vec2::new(-1.0, -1.0), &s), 0);
    }

    #[test]
    fn hole_under_nonzero_needs_opposite_orientation() {
        let outer = square(0.0, 0.0, 10.0);
        let mut inner = square(3.0, 3.0, 4.0);
        inner.points.reverse();
        let polys = vec![outer.clone(), inner.clone()];
        assert!(point_in_path(
            Vec2::new(1.0, 1.0),
            &polys,
            FillRule::NonZero
        ));
        assert!(!point_in_path(
            Vec2::new(5.0, 5.0),
            &polys,
            FillRule::NonZero
        ));
        // Same shapes, even-odd: hole counts crossings -> odd -> inside?
        // Opposite orientation ring: point in hole has |winding| 0 ->
        // even-odd parity 0 -> outside too.
        assert!(!point_in_path(
            Vec2::new(5.0, 5.0),
            &polys,
            FillRule::EvenOdd
        ));
    }

    #[test]
    fn even_odd_counts_same_orientation() {
        let outer = square(0.0, 0.0, 10.0);
        let inner = square(3.0, 3.0, 4.0);
        let polys = vec![outer, inner];
        // Same orientation: winding 2 in ring, 1 in hole.
        // Even-odd: ring odd -> inside; hole even -> outside.
        assert!(point_in_path(
            Vec2::new(1.0, 1.0),
            &polys,
            FillRule::EvenOdd
        ));
        assert!(!point_in_path(
            Vec2::new(5.0, 5.0),
            &polys,
            FillRule::EvenOdd
        ));
        // NonZero on same-orientation: winding 2 in the hole region ->
        // solid (this is the case the parity flip repairs).
        assert!(point_in_path(
            Vec2::new(5.0, 5.0),
            &polys,
            FillRule::NonZero
        ));
    }

    #[test]
    fn degenerate_polys_ignore() {
        let p = Poly {
            points: vec![Vec2::ZERO, Vec2::new(1.0, 1.0)],
            closed: false,
        };
        assert_eq!(winding_number(Vec2::new(0.5, 0.5), &p), 0);
    }
}

//! Path offsetting, arc-length measurement, and reversal.
//!
//! CAD primitives: offset a polygon inward/outward by a constant
//! distance, measure the total arc length of a path, and reverse a
//! path's direction.

use crate::{Poly, Vec2};

/// Offset a closed polygon inward (positive distance) or outward
/// (negative distance) by `distance` pixels. Uses per-edge normal
/// offsetting with miter joins at convex corners.
///
/// Returns the offset polygon, or `None` if the input has fewer than
/// 3 points or the offset would invert the polygon.
#[must_use]
pub fn offset_poly(poly: &Poly, distance: f64) -> Option<Poly> {
    let n = poly.points.len();
    if n < 3 {
        return None;
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let prev = poly.points.get((i + n - 1) % n)?;
        let curr = poly.points.get(i)?;
        let next = poly.points.get((i + 1) % n)?;
        let e0 = Vec2::new(curr.x - prev.x, curr.y - prev.y);
        let e1 = Vec2::new(next.x - curr.x, next.y - curr.y);
        let n0 = Vec2::new(e0.y, -e0.x).normalize()?;
        let n1 = Vec2::new(e1.y, -e1.x).normalize()?;
        let miter = Vec2::new(n0.x + n1.x, n0.y + n1.y).normalize()?;
        let cos_half = miter.dot(n0);
        let miter_len = if cos_half.abs() > 1e-9 {
            1.0 / cos_half.abs()
        } else {
            3.0
        };
        out.push(Vec2::new(
            curr.x + miter.x * distance * miter_len,
            curr.y + miter.y * distance * miter_len,
        ));
    }
    Some(Poly {
        points: out,
        closed: true,
    })
}

/// Total arc length of a closed or open polygon (sum of segment
/// lengths, including the closing segment for closed polygons).
#[must_use]
pub fn poly_length(points: &[Vec2]) -> f64 {
    if points.len() < 2 {
        return 0.0;
    }
    let mut total = 0.0;
    for pair in points.windows(2) {
        if let (Some(&a), Some(&b)) = (pair.first(), pair.get(1)) {
            total += a.distance(b);
        }
    }
    // Closing segment for closed polygons.
    if let (Some(&first), Some(&last)) = (points.first(), points.last()) {
        total += last.distance(first);
    }
    total
}

/// Reverse the winding direction of a polygon.
#[must_use]
pub fn reverse_poly(poly: &Poly) -> Poly {
    let mut points = poly.points.clone();
    points.reverse();
    Poly {
        points,
        closed: poly.closed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Poly;

    fn square_poly(size: f64) -> Poly {
        Poly {
            points: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(size, 0.0),
                Vec2::new(size, size),
                Vec2::new(0.0, size),
            ],
            closed: true,
        }
    }

    #[test]
    fn offset_square_grows_outward() {
        let poly = square_poly(20.0);
        let offset = offset_poly(&poly, 2.0);
        assert!(offset.is_some(), "offset must succeed for a square");
        for p in offset.iter().flat_map(|o| o.points.iter()) {
            assert!(
                p.x < -1.5 || p.x > 21.5 || p.y < -1.5 || p.y > 21.5,
                "point {p:?} should be outside [0,20]x[0,20]"
            );
        }
    }

    #[test]
    fn offset_inward_shrinks() {
        let poly = square_poly(20.0);
        let offset = offset_poly(&poly, -2.0);
        assert!(offset.is_some(), "inset must succeed for a square");
        for p in offset.iter().flat_map(|o| o.points.iter()) {
            assert!(
                p.x > 1.5 && p.x < 18.5 && p.y > 1.5 && p.y < 18.5,
                "point {p:?} should be inside the inset square"
            );
        }
    }

    #[test]
    fn poly_length_measures_perimeter() {
        let pts = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(10.0, 10.0),
            Vec2::new(0.0, 10.0),
        ];
        let len = poly_length(&pts);
        assert!(
            (len - 40.0).abs() < 1e-9,
            "square perimeter is 40, got {len}"
        );
    }

    #[test]
    fn reverse_poly_flips_winding() {
        let poly = square_poly(10.0);
        let rev = reverse_poly(&poly);
        assert_eq!(rev.points.len(), poly.points.len());
        assert_eq!(rev.points.first(), poly.points.get(3));
    }

    #[test]
    fn offset_square_perimeter_changes() {
        let poly = square_poly(20.0);
        let offset = offset_poly(&poly, 2.0);
        assert!(offset.is_some(), "offset must succeed for a square");
        let orig_perimeter = poly_length(&poly.points);
        let offset_perimeter = offset
            .iter()
            .map(|o| poly_length(&o.points))
            .fold(0.0_f64, f64::max);
        assert!(
            offset_perimeter > orig_perimeter,
            "outward offset must increase perimeter"
        );
    }
}

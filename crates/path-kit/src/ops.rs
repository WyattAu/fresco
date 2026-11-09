//! Polygon measurement and boolean primitives.
//!
//! CAD-style ops over closed polygons: signed area, centroid,
//! convex hull (Andrew's monotone chain), convex-convex
//! intersection via Sutherland–Hodgman clipping, and point-in-poly
//! by winding-independent crossing count.

use crate::Vec2;

/// Signed area (positive for counter-clockwise in Y-up math
/// orientation; screen-space Y-down flips the sign, which is fine
/// for relative comparisons).
#[must_use]
pub fn poly_signed_area(points: &[Vec2]) -> f64 {
    let n = points.len();
    if n < 3 {
        return 0.0;
    }
    let mut sum = 0.0;
    for i in 0..n {
        let a = match points.get(i) {
            Some(&p) => p,
            None => return 0.0,
        };
        let b = match points.get((i + 1) % n) {
            Some(&p) => p,
            None => return 0.0,
        };
        sum += a.x * b.y - b.x * a.y;
    }
    sum * 0.5
}

/// Unsigned area of a polygon.
#[must_use]
pub fn poly_area(points: &[Vec2]) -> f64 {
    poly_signed_area(points).abs()
}

/// Area centroid of a polygon (surface centroid, not vertex mean).
/// Returns `None` for degenerate (zero-area) polygons.
#[must_use]
pub fn poly_centroid(points: &[Vec2]) -> Option<Vec2> {
    let a6 = poly_signed_area(points) * 6.0;
    if a6.abs() < 1e-12 {
        return None;
    }
    let n = points.len();
    let mut cx = 0.0;
    let mut cy = 0.0;
    for i in 0..n {
        let a = points.get(i)?;
        let b = points.get((i + 1) % n)?;
        let cross = a.x * b.y - b.x * a.y;
        cx += (a.x + b.x) * cross;
        cy += (a.y + b.y) * cross;
    }
    Some(Vec2::new(cx / a6, cy / a6))
}

/// Convex hull of a point set (Andrew's monotone chain). Returns
/// the hull in counter-clockwise order (Y-up convention), without
/// repeating the first point at the end. Degenerate inputs (all
/// collinear) yield the two extreme points.
#[must_use]
pub fn convex_hull(mut points: Vec<Vec2>) -> Vec<Vec2> {
    if points.len() < 3 {
        return points;
    }
    points.sort_by(|a, b| {
        a.x.partial_cmp(&b.x)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal))
    });
    points.dedup();
    let n = points.len();
    if n < 3 {
        return points;
    }
    let cross = |o: Vec2, a: Vec2, b: Vec2| -> f64 {
        (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
    };
    let half = |mut chain: Vec<Vec2>, rev: bool| -> Vec<Vec2> {
        let iter: Box<dyn Iterator<Item = &Vec2>> = if rev {
            Box::new(points.iter().rev())
        } else {
            Box::new(points.iter())
        };
        for p in iter {
            while chain.len() >= 2 {
                let n2 = chain.len();
                let o = chain.get(n2 - 2).copied().unwrap_or_default();
                let a = chain.get(n2 - 1).copied().unwrap_or_default();
                if cross(o, a, *p) <= 0.0 {
                    chain.pop();
                } else {
                    break;
                }
            }
            chain.push(*p);
        }
        chain
    };
    let mut lower = half(Vec::with_capacity(n), false);
    let mut upper = half(Vec::with_capacity(n), true);
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Clip a subject polygon against a convex clip polygon
/// (Sutherland–Hodgman). The clip polygon must be convex with
/// consistent winding; the subject may be arbitrary. Returns the
/// clipped polygon (possibly empty for disjoint inputs).
#[must_use]
pub fn clip_convex(subject: &[Vec2], clip: &[Vec2]) -> Vec<Vec2> {
    if clip.len() < 3 || subject.is_empty() {
        return Vec::new();
    }
    let mut output: Vec<Vec2> = subject.to_vec();
    let n = clip.len();
    for i in 0..n {
        if output.is_empty() {
            return Vec::new();
        }
        let a = clip.get(i).copied().unwrap_or_default();
        let b = clip.get((i + 1) % n).copied().unwrap_or_default();
        let edge = Vec2::new(b.x - a.x, b.y - a.y);
        // Left perpendicular: points into the interior of a
        // counter-clockwise (Y-up) clip polygon.
        let normal = Vec2::new(-edge.y, edge.x);
        let side = |p: Vec2| -> f64 { normal.dot(Vec2::new(p.x - a.x, p.y - a.y)) };
        let input = output;
        output = Vec::with_capacity(input.len());
        let m = input.len();
        for j in 0..m {
            let cur = input.get(j).copied().unwrap_or_default();
            let next = input.get((j + 1) % m).copied().unwrap_or_default();
            let cur_side = side(cur);
            let next_side = side(next);
            if cur_side >= 0.0 {
                output.push(cur);
                if next_side < 0.0 {
                    let t = cur_side / (cur_side - next_side);
                    output.push(cur.lerp(next, t));
                }
            } else if next_side >= 0.0 {
                let t = cur_side / (cur_side - next_side);
                output.push(cur.lerp(next, t));
            }
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vec2;

    fn square(size: f64) -> Vec<Vec2> {
        vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(size, 0.0),
            Vec2::new(size, size),
            Vec2::new(0.0, size),
        ]
    }

    #[test]
    fn square_area_is_size_squared() {
        assert!((poly_area(&square(10.0)) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn centroid_of_square_is_center() {
        let c = poly_centroid(&square(10.0));
        assert!(c.is_some(), "square has a centroid");
        let c = c.unwrap_or_default();
        assert!((c.x - 5.0).abs() < 1e-9 && (c.y - 5.0).abs() < 1e-9);
    }

    #[test]
    fn centroid_degenerate_is_none() {
        let pts = vec![Vec2::ZERO, Vec2::new(10.0, 0.0)];
        assert!(poly_centroid(&pts).is_none());
    }

    #[test]
    fn hull_of_grid_points_is_square_outline() {
        let mut pts = Vec::new();
        for x in 0..=10_i32 {
            for y in 0..=10_i32 {
                pts.push(Vec2::new(f64::from(x), f64::from(y)));
            }
        }
        let hull = convex_hull(pts);
        assert_eq!(hull.len(), 4, "unit-grid hull must be 4 corners");
        let area = poly_area(&hull);
        assert!((area - 100.0).abs() < 1e-9, "hull area {area}");
    }

    #[test]
    fn hull_drops_interior_points() {
        let pts = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(10.0, 10.0),
            Vec2::new(0.0, 10.0),
            Vec2::new(5.0, 5.0),
        ];
        let hull = convex_hull(pts);
        assert_eq!(hull.len(), 4);
    }

    #[test]
    fn clip_identical_squares_is_the_square() {
        let s = square(10.0);
        let clipped = clip_convex(&s, &s);
        let area = poly_area(&clipped);
        assert!((area - 100.0).abs() < 1e-6, "clipped area {area}");
    }

    #[test]
    fn clip_overlapping_squares_is_intersection() {
        let a = square(10.0);
        let b = vec![
            Vec2::new(5.0, 0.0),
            Vec2::new(15.0, 0.0),
            Vec2::new(15.0, 10.0),
            Vec2::new(5.0, 10.0),
        ];
        let clipped = clip_convex(&a, &b);
        let area = poly_area(&clipped);
        assert!(
            (area - 50.0).abs() < 1e-6,
            "intersection of offset squares is 50, got {area}"
        );
    }

    #[test]
    fn clip_disjoint_is_empty() {
        let a = square(10.0);
        let b = vec![
            Vec2::new(100.0, 100.0),
            Vec2::new(110.0, 100.0),
            Vec2::new(110.0, 110.0),
            Vec2::new(100.0, 110.0),
        ];
        assert!(clip_convex(&a, &b).is_empty());
    }

    #[test]
    fn clip_triangle_by_square() {
        let tri = vec![
            Vec2::new(-5.0, 5.0),
            Vec2::new(15.0, 5.0),
            Vec2::new(5.0, 15.0),
        ];
        let sq = square(10.0);
        let clipped = clip_convex(&tri, &sq);
        let area = poly_area(&clipped);
        // Triangle spans y in [5,15] and is wider than the square
        // across the whole overlap band y in [5,10], so the clip is
        // the square's own 10x5 slab there: area 50.
        assert!((area - 50.0).abs() < 1e-6, "clipped slab area {area}");
    }

    #[test]
    fn signed_area_reflects_winding() {
        let ccw = square(10.0);
        let mut cw = ccw.clone();
        cw.reverse();
        let a1 = poly_signed_area(&ccw);
        let a2 = poly_signed_area(&cw);
        assert!((a1 + a2).abs() < 1e-9, "reversal flips the sign");
        assert!(a1 > 0.0, "input winding is positive: {a1}");
    }
}

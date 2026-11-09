//! Vectors, rectangles, and affine transforms.

use core::ops::{Add, Div, Mul, Neg, Sub};

/// 2D vector in `f64`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    /// X coordinate.
    pub x: f64,
    /// Y coordinate.
    pub y: f64,
}

impl Vec2 {
    /// Zero vector.
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };

    /// New vector.
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Vec2 { x, y }
    }

    /// Dot product.
    #[must_use]
    pub fn dot(self, other: Vec2) -> f64 {
        self.x * other.x + self.y * other.y
    }

    /// Perp dot product (`self.x * other.y - self.y * other.x`), positive
    /// for a counter-clockwise turn in a y-down coordinate system when the
    /// operands are consecutive edge directions.
    #[must_use]
    pub fn cross(self, other: Vec2) -> f64 {
        self.x * other.y - self.y * other.x
    }

    /// Squared length.
    #[must_use]
    pub fn len2(self) -> f64 {
        self.dot(self)
    }

    /// Length.
    #[must_use]
    pub fn len(self) -> f64 {
        self.len2().sqrt()
    }

    /// Euclidean distance to another vector.
    #[must_use]
    pub fn distance(self, other: Vec2) -> f64 {
        (other - self).len()
    }

    /// Unit vector, or `None` for the zero vector.
    #[must_use]
    pub fn normalize(self) -> Option<Vec2> {
        let l = self.len();
        if l > 0.0 { Some(self / l) } else { None }
    }

    /// Linear interpolation; `t = 0` gives `self`, `t = 1` gives `other`.
    #[must_use]
    pub fn lerp(self, other: Vec2, t: f64) -> Vec2 {
        self + (other - self) * t
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, rhs: Vec2) -> Vec2 {
        Vec2::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, rhs: Vec2) -> Vec2 {
        Vec2::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Neg for Vec2 {
    type Output = Vec2;
    fn neg(self) -> Vec2 {
        Vec2::new(-self.x, -self.y)
    }
}

impl Mul<f64> for Vec2 {
    type Output = Vec2;
    fn mul(self, s: f64) -> Vec2 {
        Vec2::new(self.x * s, self.y * s)
    }
}

impl Div<f64> for Vec2 {
    type Output = Vec2;
    fn div(self, s: f64) -> Vec2 {
        Vec2::new(self.x / s, self.y / s)
    }
}

/// Axis-aligned rectangle. Edges may be given in any order; accessors
/// normalize.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    /// Left edge.
    pub x0: f64,
    /// Top edge (y-down coordinates).
    pub y0: f64,
    /// Right edge.
    pub x1: f64,
    /// Bottom edge (y-down coordinates).
    pub y1: f64,
}

impl Rect {
    /// Rectangle from two corners, normalized.
    #[must_use]
    pub fn from_points(a: Vec2, b: Vec2) -> Rect {
        Rect {
            x0: a.x.min(b.x),
            y0: a.y.min(b.y),
            x1: a.x.max(b.x),
            y1: a.y.max(b.y),
        }
    }

    /// Rectangle from a position and size; non-positive sizes give an
    /// empty rectangle.
    #[must_use]
    pub fn from_size(origin: Vec2, size: Vec2) -> Rect {
        Rect::from_points(origin, origin + size)
    }

    /// Width (never negative).
    #[must_use]
    pub fn width(&self) -> f64 {
        (self.x1 - self.x0).max(0.0)
    }

    /// Height (never negative).
    #[must_use]
    pub fn height(&self) -> f64 {
        (self.y1 - self.y0).max(0.0)
    }

    /// True when width or height is zero.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.width() <= 0.0 || self.height() <= 0.0
    }

    /// Smallest rectangle covering both inputs.
    #[must_use]
    pub fn union(&self, other: Rect) -> Rect {
        Rect {
            x0: self.x0.min(other.x0),
            y0: self.y0.min(other.y0),
            x1: self.x1.max(other.x1),
            y1: self.y1.max(other.y1),
        }
    }

    /// Overlap, or `None` when they do not intersect or either is empty.
    #[must_use]
    pub fn intersect(&self, other: Rect) -> Option<Rect> {
        let r = Rect {
            x0: self.x0.max(other.x0),
            y0: self.y0.max(other.y0),
            x1: self.x1.min(other.x1),
            y1: self.y1.min(other.y1),
        };
        if r.is_empty() { None } else { Some(r) }
    }

    /// Whether the two rectangles overlap at all (inclusive of
    /// edge-touching, consistent with [`Rect::intersect`]).
    #[must_use]
    pub fn intersects(&self, other: Rect) -> bool {
        self.x0 <= other.x1 && other.x0 <= self.x1 && self.y0 <= other.y1 && other.y0 <= self.y1
    }

    /// Whether `other` is fully contained in this rectangle
    /// (inclusive of edges).
    #[must_use]
    pub fn contains_rect(&self, other: Rect) -> bool {
        other.x0 >= self.x0 && other.x1 <= self.x1 && other.y0 >= self.y0 && other.y1 <= self.y1
    }

    /// Point containment.
    #[must_use]
    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.x0 && p.x <= self.x1 && p.y >= self.y0 && p.y <= self.y1
    }

    /// Expand by `d` on all sides.
    #[must_use]
    pub fn inflate(&self, d: f64) -> Rect {
        Rect {
            x0: self.x0 - d,
            y0: self.y0 - d,
            x1: self.x1 + d,
            y1: self.y1 + d,
        }
    }

    /// Area (zero for empty rects).
    #[must_use]
    pub fn area(&self) -> f64 {
        self.width() * self.height()
    }
}

/// Affine transform, column-major `[a, b, c, d, e, f]`:
/// `x' = a*x + c*y + e`, `y' = b*x + d*y + f`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    /// Coefficients `[a, b, c, d, e, f]`.
    pub coeff: [f64; 6],
}

impl Affine {
    /// Identity.
    pub const IDENTITY: Affine = Affine {
        coeff: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    };

    /// Translation.
    #[must_use]
    pub const fn translate(v: Vec2) -> Affine {
        Affine {
            coeff: [1.0, 0.0, 0.0, 1.0, v.x, v.y],
        }
    }

    /// Uniform scale.
    #[must_use]
    pub const fn scale(s: f64) -> Affine {
        Affine {
            coeff: [s, 0.0, 0.0, s, 0.0, 0.0],
        }
    }

    /// Rotation by `theta` radians.
    #[must_use]
    pub fn rotate(theta: f64) -> Affine {
        let (s, c) = theta.sin_cos();
        Affine {
            coeff: [c, s, -s, c, 0.0, 0.0],
        }
    }

    /// Compose transforms: `self.then(other)` applies `other` first, then
    /// `self` (function composition order; kurbo convention).
    #[must_use]
    pub fn then(&self, other: Affine) -> Affine {
        let a = &self.coeff;
        let b = &other.coeff;
        Affine {
            coeff: [
                a[0] * b[0] + a[2] * b[1],
                a[1] * b[0] + a[3] * b[1],
                a[0] * b[2] + a[2] * b[3],
                a[1] * b[2] + a[3] * b[3],
                a[0] * b[4] + a[2] * b[5] + a[4],
                a[1] * b[4] + a[3] * b[5] + a[5],
            ],
        }
    }

    /// Apply to a point.
    #[must_use]
    pub fn apply(&self, p: Vec2) -> Vec2 {
        let a = &self.coeff;
        Vec2::new(
            a[0] * p.x + a[2] * p.y + a[4],
            a[1] * p.x + a[3] * p.y + a[5],
        )
    }

    /// Apply to a rectangle: the axis-aligned bounds of the transformed
    /// corners (exact for axis-aligned and 90-degree transforms).
    #[must_use]
    pub fn apply_rect(&self, r: Rect) -> Rect {
        let p0 = self.apply(Vec2::new(r.x0, r.y0));
        let p1 = self.apply(Vec2::new(r.x1, r.y0));
        let p2 = self.apply(Vec2::new(r.x0, r.y1));
        let p3 = self.apply(Vec2::new(r.x1, r.y1));
        Rect::from_points(p0, p1).union(Rect::from_points(p2, p3))
    }

    /// Inverse, or `None` for a singular transform.
    #[must_use]
    pub fn inverse(&self) -> Option<Affine> {
        let a = &self.coeff;
        let det = a[0] * a[3] - a[1] * a[2];
        if det == 0.0 || !det.is_finite() {
            return None;
        }
        let inv = 1.0 / det;
        Some(Affine {
            coeff: [
                a[3] * inv,
                -a[1] * inv,
                -a[2] * inv,
                a[0] * inv,
                (a[2] * a[5] - a[3] * a[4]) * inv,
                (a[1] * a[4] - a[0] * a[5]) * inv,
            ],
        })
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-12;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < EPS
    }

    #[test]
    fn vec_ops() {
        let v = Vec2::new(3.0, 4.0);
        assert!(close(v.len(), 5.0));
        assert_eq!(v.normalize(), Some(Vec2::new(0.6, 0.8)));
        assert_eq!(Vec2::ZERO.normalize(), None);
        assert!(close(Vec2::new(1.0, 0.0).cross(Vec2::new(0.0, 1.0)), 1.0));
        assert_eq!(
            Vec2::new(0.0, 0.0).lerp(Vec2::new(2.0, 4.0), 0.5),
            Vec2::new(1.0, 2.0)
        );
    }

    #[test]
    fn rect_algebra() {
        let a = Rect::from_size(Vec2::ZERO, Vec2::new(10.0, 10.0));
        let b = Rect::from_size(Vec2::new(5.0, 5.0), Vec2::new(10.0, 10.0));
        let i = a.intersect(b).unwrap_or(a);
        assert!(close(i.area(), 25.0));
        let far = Rect::from_size(Vec2::new(100.0, 100.0), Vec2::new(10.0, 10.0));
        assert_eq!(a.intersect(far), None);
        assert!(a.contains(Vec2::new(0.0, 0.0)));
        assert!(!a.contains(Vec2::new(10.1, 0.0)));
        assert!(close(a.union(b).area(), 225.0));
    }

    #[test]
    fn affine_compose_and_invert() {
        let t = Affine::translate(Vec2::new(10.0, 0.0));
        let r = Affine::rotate(core::f64::consts::FRAC_PI_2);
        let m = t.then(r);
        let p = m.apply(Vec2::new(1.0, 0.0));
        assert!(close(p.x, 10.0) && close(p.y, 1.0));
        let inv = m.inverse().unwrap_or(Affine::IDENTITY);
        let q = inv.apply(p);
        assert!(close(q.x, 1.0) && close(q.y, 0.0));
        assert_eq!(Affine::scale(0.0).inverse(), None);
    }

    #[test]
    fn affine_rect_bounds() {
        let r = Rect::from_size(Vec2::new(0.0, 0.0), Vec2::new(2.0, 1.0));
        let got = Affine::rotate(core::f64::consts::FRAC_PI_2).apply_rect(r);
        assert!(close(got.x0, -1.0) && close(got.x1, 0.0));
        assert!(close(got.y0, 0.0) && close(got.y1, 2.0));
    }
}

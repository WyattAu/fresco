//! Robust orientation predicates (paradigm 831).
//!
//! [`orient2d`] computes the signed cross product `(a-c) x (b-c)`. A
//! floating-point filter handles the common case; when the result is
//! inside the rounding error bound, an exact-product fallback (FMA
//! `two_product` + Knuth `two_sum`, magnitude-sorted fold) resolves the
//! sign where naive evaluation would return zero or flip. This is the
//! "practically exact" two-level scheme; Shewchuk's full adaptive
//! expansion suite is deliberately deferred.

use crate::geom::Vec2;

/// Relative error bound: `(3 + 16 * eps) * eps` for the 2x2 determinant.
const ORIENT_ERR: f64 = 6.661_338_147_750_943e-15;

/// Exact product: returns `(p, e)` with `a * b == p + e`, using the FMA.
fn two_product(a: f64, b: f64) -> (f64, f64) {
    let p = a * b;
    let e = a.mul_add(b, -p);
    (p, e)
}

/// Knuth two-sum: returns `(hi, lo)` with `a + b == hi + lo`, `|lo| <= ulp`.
fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let x = a + b;
    let bvirt = x - a;
    let avirt = x - bvirt;
    let bround = b - bvirt;
    let around = a - avirt;
    (x, around + bround)
}

/// Signed orientation of the triangle `a`, `b`, `c`.
///
/// Positive when `a`, `b`, `c` make a counter-clockwise turn in y-down
/// screen coordinates (clockwise in math coordinates), negative the other
/// way, zero when exactly collinear. Sign-robust: the fallback path
/// resolves cases where naive `f64` arithmetic rounds the determinant to
/// zero (REQ-P2).
#[must_use]
pub fn orient2d(a: Vec2, b: Vec2, c: Vec2) -> f64 {
    let acx = a.x - c.x;
    let acy = a.y - c.y;
    let bcx = b.x - c.x;
    let bcy = b.y - c.y;

    let detleft = acx * bcy;
    let detright = acy * bcx;
    let det = detleft - detright;

    let errbound = ORIENT_ERR * (detleft.abs() + detright.abs());
    if det.abs() >= errbound {
        return det;
    }

    // Exact-product fallback. The 2x2 determinant's exact expansion has
    // at most four components; fold them by descending magnitude.
    let (p1, e1) = two_product(acx, bcy);
    let (p2, e2) = two_product(acy, bcx);
    let mut terms = [p1, e1, -p2, -e2];
    // Selection sort, largest magnitude first (4 items).
    for i in 0..terms.len() {
        let mut best = i;
        for j in (i + 1)..terms.len() {
            if terms.get(j).map(|t: &f64| t.abs()).unwrap_or(0.0)
                > terms.get(best).map(|t: &f64| t.abs()).unwrap_or(0.0)
            {
                best = j;
            }
        }
        terms.swap(i, best);
    }
    let Some(&first) = terms.first() else {
        return 0.0;
    };
    let mut hi = first;
    let mut lo = 0.0;
    for &t in terms.iter().skip(1) {
        let (h, l) = two_sum(hi, t);
        hi = h;
        lo += l;
    }
    hi + lo
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    /// Naive determinant, for demonstrating the fast-path failure.
    fn naive(a: Vec2, b: Vec2, c: Vec2) -> f64 {
        (a.x - c.x) * (b.y - c.y) - (a.y - c.y) * (b.x - c.x)
    }

    #[test]
    fn basic_signs() {
        let c = Vec2::ZERO;
        let a = Vec2::new(1.0, 0.0);
        let b = Vec2::new(0.0, 1.0);
        assert!(orient2d(a, b, c) > 0.0);
        assert!(orient2d(b, a, c) < 0.0);
        assert_eq!(orient2d(a, a, c), 0.0);
        assert_eq!(
            orient2d(
                Vec2::new(0.0, 0.0),
                Vec2::new(1.0, 1.0),
                Vec2::new(2.0, 2.0)
            ),
            0.0
        );
    }

    #[test]
    fn robust_orientation() {
        // Products of magnitude ~1.9e32 each round to the same f64 (ulp
        // 3.6e16 exceeds the determinant), so naive evaluation returns
        // exactly 0.0 while the true integer determinant is -2A
        // (REQ-P2). Values verified against exact integer arithmetic.
        let a_pos: f64 = 13840623208129544.0;
        let a = Vec2::new(a_pos, a_pos);
        let b = Vec2::new(a_pos, a_pos - 2.0);
        let c = Vec2::ZERO;

        let naive_det = naive(a, b, c);
        assert_eq!(naive_det, 0.0, "naive must lose this case");

        let got = orient2d(a, b, c);
        assert!(got < 0.0, "robust predicate lost the sign: got {got}");
        assert!(orient2d(b, a, c) > 0.0);
    }

    #[test]
    fn robust_orientation_negative_side() {
        let a_pos: f64 = 13840623208129544.0;
        let a = Vec2::new(a_pos, a_pos - 2.0);
        let b = Vec2::new(a_pos, a_pos);
        let c = Vec2::ZERO;
        assert_eq!(naive_unused(a, b, c), 0.0);
        assert!(orient2d(a, b, c) > 0.0);
    }

    fn naive_unused(a: Vec2, b: Vec2, c: Vec2) -> f64 {
        naive(a, b, c)
    }

    #[test]
    fn collinear_huge_span_still_zero() {
        let a = Vec2::new(1.0e10, 1.0e10);
        let b = Vec2::new(2.0e10, 2.0e10);
        let c = Vec2::new(3.0e10, 3.0e10);
        assert_eq!(orient2d(a, b, c), 0.0);
    }
}

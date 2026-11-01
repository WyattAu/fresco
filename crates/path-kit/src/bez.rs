//! Quadratic and cubic bezier curves.
//!
//! Arc length uses 4-point Legendre-Gauss quadrature on the derivative
//! magnitude (paradigm 883): exact on lines, within tight relative error
//! on smooth cubics.

use crate::geom::Vec2;

/// Quadratic bezier: `p0`, control `p1`, end `p2`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuadBez {
    /// Start point.
    pub p0: Vec2,
    /// Control point.
    pub p1: Vec2,
    /// End point.
    pub p2: Vec2,
}

impl QuadBez {
    /// Point at parameter `t` in `[0, 1]`.
    #[must_use]
    pub fn eval(&self, t: f64) -> Vec2 {
        let mt = 1.0 - t;
        self.p0 * (mt * mt) + self.p1 * (2.0 * mt * t) + self.p2 * (t * t)
    }

    /// Derivative at `t`.
    #[must_use]
    pub fn deriv(&self, t: f64) -> Vec2 {
        let mt = 1.0 - t;
        (self.p1 - self.p0) * (2.0 * mt) + (self.p2 - self.p1) * (2.0 * t)
    }

    /// Perpendicular distance of the control point from the chord.
    #[must_use]
    pub fn flatness(&self) -> f64 {
        let chord = self.p2 - self.p0;
        let l = chord.len();
        if l == 0.0 {
            return (self.p1 - self.p0).len();
        }
        (self.p1 - self.p0).cross(chord).abs() / l
    }

    /// Split at `t` via de Casteljau.
    #[must_use]
    pub fn split(&self, t: f64) -> (QuadBez, QuadBez) {
        let p01 = self.p0.lerp(self.p1, t);
        let p12 = self.p1.lerp(self.p2, t);
        let m = p01.lerp(p12, t);
        (
            QuadBez {
                p0: self.p0,
                p1: p01,
                p2: m,
            },
            QuadBez {
                p0: m,
                p1: p12,
                p2: self.p2,
            },
        )
    }
}

/// Cubic bezier: `p0`, controls `p1`, `p2`, end `p3`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicBez {
    /// Start point.
    pub p0: Vec2,
    /// First control point.
    pub p1: Vec2,
    /// Second control point.
    pub p2: Vec2,
    /// End point.
    pub p3: Vec2,
}

impl CubicBez {
    /// Point at parameter `t` in `[0, 1]`.
    #[must_use]
    pub fn eval(&self, t: f64) -> Vec2 {
        let mt = 1.0 - t;
        let a = mt * mt * mt;
        let b = 3.0 * mt * mt * t;
        let c = 3.0 * mt * t * t;
        let d = t * t * t;
        self.p0 * a + self.p1 * b + self.p2 * c + self.p3 * d
    }

    /// Derivative at `t`.
    #[must_use]
    pub fn deriv(&self, t: f64) -> Vec2 {
        let mt = 1.0 - t;
        let a = 3.0 * mt * mt;
        let b = 6.0 * mt * t;
        let c = 3.0 * t * t;
        (self.p1 - self.p0) * a + (self.p2 - self.p1) * b + (self.p3 - self.p2) * c
    }

    /// Sum of perpendicular distances of both control points from the
    /// chord. A conservative flatness measure: the curve stays inside the
    /// control hull, so `flatness() <= tol` bounds curve-to-chord error.
    #[must_use]
    pub fn flatness(&self) -> f64 {
        let chord = self.p3 - self.p0;
        let l = chord.len();
        if l == 0.0 {
            return (self.p1 - self.p0).len().max((self.p2 - self.p0).len());
        }
        let d1 = (self.p1 - self.p0).cross(chord).abs() / l;
        let d2 = (self.p2 - self.p0).cross(chord).abs() / l;
        d1.max(d2)
    }

    /// Split at `t` via de Casteljau.
    #[must_use]
    pub fn split(&self, t: f64) -> (CubicBez, CubicBez) {
        let p01 = self.p0.lerp(self.p1, t);
        let p12 = self.p1.lerp(self.p2, t);
        let p23 = self.p2.lerp(self.p3, t);
        let p012 = p01.lerp(p12, t);
        let p123 = p12.lerp(p23, t);
        let m = p012.lerp(p123, t);
        (
            CubicBez {
                p0: self.p0,
                p1: p01,
                p2: p012,
                p3: m,
            },
            CubicBez {
                p0: m,
                p1: p123,
                p2: p23,
                p3: self.p3,
            },
        )
    }

    /// Arc length via 4-point Legendre-Gauss quadrature of `|deriv|`.
    /// Exact on lines (REQ-P3).
    #[must_use]
    pub fn arclen(&self) -> f64 {
        // Legendre-Gauss n=4 nodes mapped to [0, 1], weights halved.
        const N1: f64 = 0.069_431_844_202_973_71;
        const N2: f64 = 0.330_009_478_207_571_87;
        const N3: f64 = 0.669_990_521_792_428_1;
        const N4: f64 = 0.930_568_155_797_026_2;
        const W1: f64 = 0.173_927_422_568_726_92;
        const W2: f64 = 0.326_072_577_431_272_6;
        self.deriv(N1).len() * W1
            + self.deriv(N2).len() * W2
            + self.deriv(N3).len() * W2
            + self.deriv(N4).len() * W1
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-9;

    #[test]
    fn eval_endpoints() {
        let c = CubicBez {
            p0: Vec2::new(0.0, 0.0),
            p1: Vec2::new(1.0, 2.0),
            p2: Vec2::new(3.0, 2.0),
            p3: Vec2::new(4.0, 0.0),
        };
        assert_eq!(c.eval(0.0), c.p0);
        assert_eq!(c.eval(1.0), c.p3);
        let mid = c.eval(0.5);
        assert!((mid.y - 1.5).abs() < EPS);
    }

    #[test]
    fn split_roundtrip() {
        let c = CubicBez {
            p0: Vec2::ZERO,
            p1: Vec2::new(0.0, 10.0),
            p2: Vec2::new(10.0, 10.0),
            p3: Vec2::new(10.0, 0.0),
        };
        let (a, b) = c.split(0.5);
        assert_eq!(a.p0, c.p0);
        assert_eq!(b.p3, c.p3);
        assert_eq!(a.p3, b.p0);
        assert!((a.eval(1.0).x - 5.0).abs() < EPS);
        assert!((a.eval(1.0).y - 7.5).abs() < EPS);
    }

    #[test]
    fn arclen_line_exact() {
        let line = CubicBez {
            p0: Vec2::new(0.0, 0.0),
            p1: Vec2::new(1.0, 1.0),
            p2: Vec2::new(2.0, 2.0),
            p3: Vec2::new(3.0, 3.0),
        };
        let got = line.arclen();
        let want = 3.0 * 2.0_f64.sqrt();
        assert!((got - want).abs() / want < 1e-12);
    }

    #[test]
    fn arclen_kappa_circle() {
        // Cubic approximation of a quarter unit circle, kappa = 0.5522847498.
        let k = 0.552_284_749_8;
        let quarter = CubicBez {
            p0: Vec2::new(0.0, 0.0),
            p1: Vec2::new(k, 0.0),
            p2: Vec2::new(1.0, 1.0 - k),
            p3: Vec2::new(1.0, 1.0),
        };
        // Unit circle has radius 1 centered (0, 1)? No: this arc runs from
        // (0,0) to (1,1) with center (0,1) radius 1: quarter of radius 1.
        let got = quarter.arclen();
        let want = core::f64::consts::FRAC_PI_2;
        assert!((got - want).abs() / want < 1e-3, "got {got}, want {want}");
    }
}

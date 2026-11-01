//! Closed-form animation for the fresco UI engine.
//!
//! L0 layer: std, zero dependencies, no allocation (REQ-A1, REQ-A2).
//!
//! The centerpiece is [`Spring`]: the damped harmonic oscillator
//! `m*x'' + c*x' + k*x = 0` solved in closed form (paradigm 880). Sampling
//! is O(1) math, exact at any time `t`, and frame-rate independent by
//! construction: no integrator drift, no per-frame cost beyond a few
//! flops, no state machine.
//!
//! ```
//! use anim_kit::Spring;
//!
//! let spring = Spring::new(1.0, 12.0, 170.0);
//! // Animate 0.0 -> 1.0 with initial velocity 0, sampled at t = 0.5s.
//! let x = spring.sample(0.5, 0.0, 1.0, 0.0);
//! let settle = spring.settle_time(1e-3, 0.0, 1.0, 0.0);
//! assert!(settle > 0.0);
//! let _ = x;
//! ```

#![forbid(unsafe_code)]

use core::f64::consts::E;

/// A damped harmonic oscillator: `m*x'' + c*x' + k*x = 0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    /// Mass.
    pub mass: f64,
    /// Damping coefficient.
    pub damping: f64,
    /// Spring stiffness.
    pub stiffness: f64,
}

impl Spring {
    /// New spring. Non-positive parameters are clamped to minimal valid
    /// values so sampling never divides by zero.
    #[must_use]
    pub fn new(mass: f64, damping: f64, stiffness: f64) -> Self {
        Spring {
            mass: mass.max(1e-9),
            damping: damping.max(0.0),
            stiffness: stiffness.max(1e-9),
        }
    }

    /// Damping ratio `c / (2 * sqrt(m*k))`: 0 undamped, 1 critical, >1
    /// overdamped.
    #[must_use]
    pub fn damping_ratio(&self) -> f64 {
        self.damping / (2.0 * (self.mass * self.stiffness).sqrt())
    }

    /// Displacement and velocity at time `t` for a motion from `from` to
    /// `to` with initial velocity `v0`. Returns `(x, v)`.
    ///
    /// Analytic solution; no integration (REQ-A1). Branch selection is by
    /// damping ratio so near-critical springs use the stable closed form.
    #[must_use]
    pub fn sample(&self, t: f64, from: f64, to: f64, v0: f64) -> (f64, f64) {
        let m = self.mass;
        let c = self.damping;
        let k = self.stiffness;
        let u0 = from - to;
        let alpha = c / (2.0 * m);
        let ratio = self.damping_ratio();

        if ratio < 1.0 - 1e-9 {
            // Underdamped: u = e^-a (u0 cos(w t) + B sin(w t)), B = (v0 + a u0)/w
            let w = (k / m - alpha * alpha).max(0.0).sqrt();
            let b = (v0 + alpha * u0) / w;
            let wt = w * t;
            let decay = (-alpha * t).exp();
            let cos = wt.cos();
            let sin = wt.sin();
            let u = decay * (u0 * cos + b * sin);
            let du = decay * (-alpha * (u0 * cos + b * sin) - u0 * w * sin + b * w * cos);
            (to + u, du)
        } else if ratio <= 1.0 + 1e-9 {
            // Critically damped: u = e^-a (u0 + (v0 + a u0) t)
            let lin = v0 + alpha * u0;
            let decay = (-alpha * t).exp();
            let u = decay * (u0 + lin * t);
            let du = decay * (lin - alpha * (u0 + lin * t));
            (to + u, du)
        } else {
            // Overdamped: u = A e^(r1 t) + B e^(r2 t)
            let beta = (4.0 * m * k).sqrt() * (ratio * ratio - 1.0).sqrt() / (2.0 * m);
            let r1 = -alpha + beta;
            let r2 = -alpha - beta;
            let a_coef = (v0 - r2 * u0) / (r1 - r2);
            let b_coef = (v0 - r1 * u0) / (r2 - r1);
            let u = a_coef * (r1 * t).exp() + b_coef * (r2 * t).exp();
            let du = a_coef * r1 * (r1 * t).exp() + b_coef * r2 * (r2 * t).exp();
            (to + u, du)
        }
    }

    /// Position only, see [`Spring::sample`].
    #[must_use]
    pub fn position(&self, t: f64, from: f64, to: f64, v0: f64) -> f64 {
        self.sample(t, from, to, v0).0
    }

    /// Smallest returned `t` guaranteed to satisfy `|u(t)| <= epsilon *
    /// scale`, where `scale` is the initial displacement magnitude
    /// (guarded to 1.0 at zero). The bound is solved exactly from each
    /// regime's decay envelope, then verified against [`Spring::sample`];
    /// verification failure escalates `t`. Undamped springs return
    /// infinity when the epsilon is below the oscillation amplitude
    /// (REQ-A2).
    #[must_use]
    pub fn settle_time(&self, epsilon: f64, from: f64, to: f64, v0: f64) -> f64 {
        let scale = (from - to).abs().max(1.0);
        let bound = epsilon * scale;
        let u0 = from - to;
        let alpha = self.damping / (2.0 * self.mass);
        let ratio = self.damping_ratio();

        if alpha <= 0.0 {
            // Undamped: oscillates forever with constant amplitude.
            let w = (self.stiffness / self.mass).sqrt();
            let amp = (u0 * u0 + (v0 / w).powi(2)).sqrt();
            return if amp <= bound { 0.0 } else { f64::INFINITY };
        }

        let t_bound = if ratio < 1.0 - 1e-9 {
            // |u(t)| <= C e^{-a t}, C = hypot(u0, (v0 + a u0)/w).
            let w = (self.stiffness / self.mass - alpha * alpha).max(0.0).sqrt();
            let amp = (u0 * u0 + ((v0 + alpha * u0) / w).powi(2)).sqrt();
            if amp <= bound {
                0.0
            } else {
                (amp / bound).ln() / alpha
            }
        } else if ratio <= 1.0 + 1e-9 {
            // |u(t)| <= e^{-a t} (|u0| + |v0 + a u0| t); fixed-point solve.
            let lin = (v0 + alpha * u0).abs();
            if u0.abs() <= bound {
                0.0
            } else {
                let mut t = (u0.abs() / bound).ln() / alpha;
                for _ in 0..8 {
                    let next = ((u0.abs() + lin * t) / bound).ln() / alpha;
                    if (next - t).abs() < 1e-12 {
                        t = next;
                        break;
                    }
                    t = next.max(0.0);
                }
                t
            }
        } else {
            // |u(t)| <= (|A| + |B|) e^{r_max t}; solved exactly.
            let beta = (4.0 * self.mass * self.stiffness).sqrt() * (ratio * ratio - 1.0).sqrt()
                / (2.0 * self.mass);
            let r1 = -alpha + beta;
            let r2 = -alpha - beta;
            let a_coef = (v0 - r2 * u0) / (r1 - r2);
            let b_coef = (v0 - r1 * u0) / (r2 - r1);
            let amp = a_coef.abs() + b_coef.abs();
            if amp <= bound {
                0.0
            } else {
                // r1 is the slower (larger, still negative) root; solve
                // amp * e^(r1 t) = bound for t > 0.
                (amp / bound).ln() / (-r1)
            }
        };

        // Verify against the sampled solution; escalate if numerics drift.
        let mut t = if t_bound.is_finite() && t_bound > 0.0 {
            t_bound
        } else if t_bound.is_finite() {
            1.0 / (alpha * 1000.0)
        } else {
            return f64::INFINITY;
        };
        for _ in 0..64 {
            let (u, _) = self.sample(t, from, to, v0);
            if u.abs() <= bound {
                return t;
            }
            t *= 2.0;
        }
        t
    }
}

/// Easing functions over `t` in `[0, 1]`, clamped. Time-warp curves for
/// linear tweening where spring physics is not wanted.
#[must_use]
pub fn ease(t: f64, curve: Easing) -> f64 {
    let t = t.clamp(0.0, 1.0);
    match curve {
        Easing::Linear => t,
        Easing::InCubic => t * t * t,
        Easing::OutCubic => {
            let u = 1.0 - t;
            1.0 - u * u * u
        }
        Easing::InOutCubic => {
            if t < 0.5 {
                4.0 * t * t * t
            } else {
                let u = -2.0 * t + 2.0;
                1.0 - u * u * u / 2.0
            }
        }
        Easing::OutExpo => {
            if t >= 1.0 {
                1.0
            } else {
                1.0 - E.powf(-10.0 * t)
            }
        }
    }
}

/// Named easing curves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Easing {
    /// Linear.
    Linear,
    /// Cubic ease-in.
    InCubic,
    /// Cubic ease-out.
    OutCubic,
    /// Cubic ease-in-out.
    InOutCubic,
    /// Exponential ease-out.
    OutExpo,
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-6;

    fn spring_under() -> Spring {
        Spring::new(1.0, 12.0, 170.0)
    }

    fn spring_crit() -> Spring {
        Spring::new(1.0, 2.0 * 170.0_f64.sqrt(), 170.0)
    }

    fn spring_over() -> Spring {
        Spring::new(1.0, 60.0, 170.0)
    }

    /// RK4 integration of u' = v, v' = -(k u + c v)/m for cross-checking.
    fn rk4(s: &Spring, t_end: f64, u0: f64, v0: f64, steps: usize) -> f64 {
        let h = t_end / f64::from(steps as u32);
        let (mut u, mut v) = (u0, v0);
        let accel = |u: f64, v: f64| -> f64 { -(s.stiffness * u + s.damping * v) / s.mass };
        for _ in 0..steps {
            let k1u = v;
            let k1v = accel(u, v);
            let k2u = v + h * k1v * 0.5;
            let k2v = accel(u + h * k1u * 0.5, v + h * k1v * 0.5);
            let k3u = v + h * k2v * 0.5;
            let k3v = accel(u + h * k2u * 0.5, v + h * k2v * 0.5);
            let k4u = v + h * k3v;
            let k4v = accel(u + h * k3u, v + h * k3v);
            u += h * (k1u + 2.0 * k2u + 2.0 * k3u + k4u) / 6.0;
            v += h * (k1v + 2.0 * k2v + 2.0 * k3v + k4v) / 6.0;
        }
        u
    }

    #[test]
    fn initial_conditions_satisfied() {
        for s in [spring_under(), spring_crit(), spring_over()] {
            let (x, v) = s.sample(0.0, 1.0, 5.0, -3.0);
            assert!((x - 1.0).abs() < EPS);
            assert!((v + 3.0).abs() < EPS);
        }
    }

    #[test]
    fn matches_numeric_integration() {
        for s in [spring_under(), spring_crit(), spring_over()] {
            let got = s.sample(1.0, 1.0, 5.0, -3.0).0;
            let want = rk4(&s, 1.0, 1.0 - 5.0, -3.0, 10_000) + 5.0;
            assert!(
                (got - want).abs() < 1e-6,
                "spring {:?} analytic {got} vs rk4 {want}",
                s.damping_ratio()
            );
        }
    }

    #[test]
    fn overdamped_never_overshoots() {
        let s = spring_over();
        let mut prev = s.position(0.0, 0.0, 1.0, 0.0);
        for i in 1..=600 {
            let t = f64::from(i) / 100.0;
            let x = s.position(t, 0.0, 1.0, 0.0);
            assert!(
                x >= prev - EPS,
                "overdamped went backwards at t={t}: {x} < {prev}"
            );
            assert!(x <= 1.0 + EPS, "overdamped overshot at t={t}: {x}");
            prev = x;
        }
    }

    #[test]
    fn underdamped_decays_envelope() {
        let s = spring_under();
        let alpha = s.damping / (2.0 * s.mass);
        let u0: f64 = 1.0;
        let w = (s.stiffness / s.mass - alpha * alpha).sqrt();
        let v0 = 0.0;
        let b_amp = (v0 + alpha * u0) / w;
        let c_amp = (u0 * u0 + b_amp * b_amp).sqrt();
        for i in 1..=200 {
            let t = f64::from(i) / 10.0;
            let u = (s.sample(t, 0.0, 1.0, v0).0 - 1.0).abs();
            let amp = c_amp * (-alpha * t).exp();
            assert!(u <= amp + 1e-9, "|u({t})|={u} exceeds envelope {amp}");
        }
    }

    #[test]
    fn settle_bound_holds() {
        for s in [spring_under(), spring_crit(), spring_over()] {
            let eps = 1e-3;
            let t = s.settle_time(eps, 0.0, 1.0, 0.0);
            let u = (s.sample(t, 0.0, 1.0, 0.0).0 - 1.0).abs();
            assert!(u <= eps, "settle at t={t} left |u|={u} > {eps}");
            assert!(t > 0.0);
        }
    }

    #[test]
    fn damping_ratio_classes() {
        assert!(spring_under().damping_ratio() < 1.0);
        assert!((spring_crit().damping_ratio() - 1.0).abs() < 1e-12);
        assert!(spring_over().damping_ratio() > 1.0);
    }

    #[test]
    fn zero_displacement_settle_guard() {
        let s = spring_under();
        let t = s.settle_time(1e-3, 1.0, 1.0, 0.0);
        assert!(t >= 0.0 && t.is_finite());
    }

    #[test]
    fn easing_clamped_and_monotone() {
        assert_eq!(ease(-1.0, Easing::OutCubic), 0.0);
        assert_eq!(ease(2.0, Easing::OutCubic), 1.0);
        assert_eq!(ease(1.0, Easing::InCubic), 1.0);
        let mut prev = -1.0;
        for i in 0..=100 {
            let t = f64::from(i) / 100.0;
            let v = ease(t, Easing::InOutCubic);
            assert!(v >= prev - EPS);
            prev = v;
        }
        assert!((ease(0.5, Easing::InOutCubic) - 0.5).abs() < EPS);
    }

    #[test]
    fn clamps_invalid_params() {
        let s = Spring::new(0.0, -5.0, 0.0);
        let (x, _) = s.sample(0.1, 0.0, 1.0, 0.0);
        assert!(x.is_finite());
    }
}

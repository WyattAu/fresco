//! Verlet particle physics for the fresco UI engine.
//!
//! Position-based dynamics: particles carry current + previous
//! position (velocity is implicit), distance constraints are relaxed
//! iteratively, gravity is a per-step acceleration. Deterministic:
//! same inputs produce byte-identical worlds, which makes it
//! headless-testable and replay-safe.
//!
//! Typical uses: ropes and chains, cloth sheets, soft-body props,
//! particle debris — anything a 2D game engine or CAD sketch
//! solver needs at the low end.

pub mod clock;
pub mod collision;

use path_kit::Vec2;

/// A point mass. Velocity is implicit: `pos - prev`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Particle {
    /// Current position.
    pub pos: Vec2,
    /// Position at the end of the previous step.
    pub prev: Vec2,
    /// Inverse mass (0 = pinned / static).
    pub inv_mass: f64,
    /// Global damping applied per step (0 = none, 1 = full stop).
    pub damping: f64,
}

impl Particle {
    /// Free particle at `pos` with mass `mass`.
    #[must_use]
    pub fn free(pos: Vec2, mass: f64) -> Self {
        Particle {
            pos,
            prev: pos,
            inv_mass: if mass > 0.0 { 1.0 / mass } else { 0.0 },
            damping: 0.0,
        }
    }

    /// Pinned particle (does not move under forces or constraints).
    #[must_use]
    pub fn pinned(pos: Vec2) -> Self {
        Particle {
            pos,
            prev: pos,
            inv_mass: 0.0,
            damping: 0.0,
        }
    }
}

/// Distance constraint between two particle indices, relaxed toward
/// `rest_len` with `stiffness` in `[0, 1]`.
#[derive(Clone, Copy, Debug)]
pub struct DistanceConstraint {
    /// Index of the first particle.
    pub a: usize,
    /// Index of the second particle.
    pub b: usize,
    /// Target distance.
    pub rest_len: f64,
    /// Per-iteration correction strength in `[0, 1]`.
    pub stiffness: f64,
}

impl DistanceConstraint {
    /// New constraint with rest length equal to the current
    /// distance between `a` and `b`.
    #[must_use]
    pub fn at_current_length(a: usize, b: usize, particles: &[Particle]) -> Self {
        let rest = match (particles.get(a), particles.get(b)) {
            (Some(&p), Some(&q)) => p.pos.distance(q.pos),
            _ => 0.0,
        };
        DistanceConstraint {
            a,
            b,
            rest_len: rest,
            stiffness: 1.0,
        }
    }
}

/// A verlet particle world: particles + constraints + gravity.
#[derive(Clone, Debug, Default)]
pub struct World {
    /// All particles.
    pub particles: Vec<Particle>,
    /// Distance constraints (indices into `particles`).
    pub constraints: Vec<DistanceConstraint>,
    /// Constant acceleration applied each step (default gravity is
    /// caller-set; no built-in default).
    pub gravity: Vec2,
    /// Constraint-relaxation iterations per step.
    pub iterations: u32,
}

impl World {
    /// New empty world with downward gravity of 500 px/s².
    #[must_use]
    pub fn new() -> Self {
        World {
            particles: Vec::new(),
            constraints: Vec::new(),
            gravity: Vec2::new(0.0, 500.0),
            iterations: 8,
        }
    }

    /// Add a particle, returning its index.
    pub fn add(&mut self, p: Particle) -> usize {
        self.particles.push(p);
        self.particles.len().saturating_sub(1)
    }

    /// Add a distance constraint between two indices with the given
    /// rest length and stiffness.
    pub fn link(&mut self, a: usize, b: usize, rest_len: f64, stiffness: f64) {
        self.constraints.push(DistanceConstraint {
            a,
            b,
            rest_len,
            stiffness,
        });
    }

    /// Advance the world by `dt` seconds: gravity, verlet
    /// integration, constraint relaxation.
    pub fn step(&mut self, dt: f64) {
        // Integrate.
        for p in &mut self.particles {
            if p.inv_mass <= 0.0 {
                p.prev = p.pos;
                continue;
            }
            let vel = Vec2::new(p.pos.x - p.prev.x, p.pos.y - p.prev.y);
            let keep = 1.0 - p.damping;
            let next = Vec2::new(
                p.pos.x + vel.x * keep + self.gravity.x * dt * dt,
                p.pos.y + vel.y * keep + self.gravity.y * dt * dt,
            );
            p.prev = p.pos;
            p.pos = next;
        }
        // Relax.
        for _ in 0..self.iterations {
            self.relax_once();
        }
    }

    fn relax_once(&mut self) {
        for c in &self.constraints {
            let (ia, ib) = (c.a, c.b);
            let (Some(&pa), Some(&pb)) = (self.particles.get(ia), self.particles.get(ib)) else {
                continue;
            };
            let delta = Vec2::new(pb.pos.x - pa.pos.x, pb.pos.y - pa.pos.y);
            let dist = delta.len();
            if dist < 1e-12 {
                continue;
            }
            let w_sum = pa.inv_mass + pb.inv_mass;
            if w_sum <= 0.0 {
                continue;
            }
            let diff = (dist - c.rest_len) / dist * c.stiffness;
            let corr = Vec2::new(delta.x * diff, delta.y * diff);
            let wa = pa.inv_mass / w_sum;
            let wb = pb.inv_mass / w_sum;
            if let Some(p) = self.particles.get_mut(ia).filter(|p| p.inv_mass > 0.0) {
                p.pos = Vec2::new(p.pos.x + corr.x * wa, p.pos.y + corr.y * wa);
            }
            if let Some(p) = self.particles.get_mut(ib).filter(|p| p.inv_mass > 0.0) {
                p.pos = Vec2::new(p.pos.x - corr.x * wb, p.pos.y - corr.y * wb);
            }
        }
    }

    /// Total constraint error: sum of |current - rest| over all
    /// constraints. Useful as a convergence metric in tests.
    #[must_use]
    pub fn constraint_error(&self) -> f64 {
        let mut total = 0.0;
        for c in &self.constraints {
            let (Some(&pa), Some(&pb)) = (self.particles.get(c.a), self.particles.get(c.b)) else {
                continue;
            };
            total += (pa.pos.distance(pb.pos) - c.rest_len).abs();
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use path_kit::Vec2;

    #[test]
    fn free_particle_falls_under_gravity() {
        let mut w = World::new();
        let i = w.add(Particle::free(Vec2::new(100.0, 100.0), 1.0));
        w.step(1.0 / 60.0);
        let p = w
            .particles
            .get(i)
            .copied()
            .unwrap_or(Particle::pinned(Vec2::ZERO));
        assert!(p.pos.y > 100.0, "gravity pulls down: y = {}", p.pos.y);
    }

    #[test]
    fn pinned_particle_never_moves() {
        let mut w = World::new();
        let i = w.add(Particle::pinned(Vec2::new(50.0, 50.0)));
        for _ in 0..30 {
            w.step(1.0 / 60.0);
        }
        let p = w
            .particles
            .get(i)
            .copied()
            .unwrap_or(Particle::pinned(Vec2::ZERO));
        assert_eq!(p.pos, Vec2::new(50.0, 50.0), "pinned stays put");
    }

    #[test]
    fn rope_constrains_lengths_under_gravity() {
        // A 5-link rope pinned at one end must stay within a small
        // factor of its total rest length after settling.
        let mut w = World::new();
        let anchor = w.add(Particle::pinned(Vec2::new(100.0, 20.0)));
        let mut prev = anchor;
        for k in 1..=5_u32 {
            let idx = w.add(Particle::free(
                Vec2::new(100.0, 20.0 + f64::from(k) * 10.0),
                1.0,
            ));
            w.link(prev, idx, 10.0, 1.0);
            prev = idx;
        }
        for _ in 0..240 {
            w.step(1.0 / 60.0);
        }
        let err = w.constraint_error();
        assert!(err < 1.0, "rope links must hold: total error {err}");
    }

    #[test]
    fn verlet_is_deterministic() {
        let build = || {
            let mut w = World::new();
            let a = w.add(Particle::free(Vec2::new(0.0, 0.0), 1.0));
            let b = w.add(Particle::free(Vec2::new(30.0, 0.0), 2.0));
            w.link(a, b, 25.0, 0.8);
            for _ in 0..60 {
                w.step(1.0 / 60.0);
            }
            w.particles
                .iter()
                .map(|p| (p.pos.x, p.pos.y))
                .collect::<Vec<_>>()
        };
        let r1 = build();
        let r2 = build();
        assert_eq!(r1, r2, "identical runs must be byte-identical");
    }

    #[test]
    fn constraint_pulls_to_rest_length() {
        let mut w = World::new();
        let a = w.add(Particle::free(Vec2::new(0.0, 0.0), 1.0));
        let b = w.add(Particle::free(Vec2::new(30.0, 0.0), 1.0));
        w.link(a, b, 25.0, 1.0);
        // Several steps: the spring contracts toward rest length.
        for _ in 0..30 {
            w.step(1.0 / 60.0);
        }
        let err = w.constraint_error();
        assert!(err < 5.0, "constraint must converge: error {err}");
    }

    #[test]
    fn damping_bleeds_energy() {
        let damped = {
            let mut w = World::new();
            w.gravity = Vec2::ZERO;
            let i = w.add(Particle::free(Vec2::new(0.0, 0.0), 1.0));
            if let Some(p) = w.particles.get_mut(i) {
                p.prev = Vec2::new(-10.0, 0.0); // initial velocity 10 px/s
                p.damping = 0.5;
            }
            for _ in 0..10 {
                w.step(1.0 / 60.0);
            }
            w.particles
                .get(i)
                .map(|p| p.pos.distance(p.prev))
                .unwrap_or(0.0)
        };
        let undamped = {
            let mut w = World::new();
            w.gravity = Vec2::ZERO;
            let i = w.add(Particle::free(Vec2::new(0.0, 0.0), 1.0));
            if let Some(p) = w.particles.get_mut(i) {
                p.prev = Vec2::new(-10.0, 0.0);
            }
            for _ in 0..10 {
                w.step(1.0 / 60.0);
            }
            w.particles
                .get(i)
                .map(|p| p.pos.distance(p.prev))
                .unwrap_or(0.0)
        };
        assert!(
            damped < undamped,
            "damping must reduce speed: {damped} < {undamped}"
        );
    }
}

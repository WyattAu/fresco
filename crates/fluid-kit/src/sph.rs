//! SPH particle fluid: Lagrangian solver alongside the grid one
//! (M7). Smoothed-particle hydrodynamics with the Müller et al.
//! 2003 formulation - poly6 density kernel, spiky pressure gradient,
//! viscosity Laplacian - in pixel units, fully deterministic:
//! fixed iteration order, no random state, f64 throughout.
//!
//! Neighbor search is brute force O(n²) with a squared-distance
//! early-out, sized for demo pools (hundreds of particles). A
//! spatial hash is the M7.x upgrade path when pools grow.

use path_kit::Vec2;

/// Solver parameters.
#[derive(Clone, Copy, Debug)]
pub struct SphParams {
    /// Kernel radius (interaction distance), px.
    pub h: f64,
    /// Rest density (mass units - particle mass is 1).
    pub rest_density: f64,
    /// Pressure stiffness: p = k * (rho - rho0), clamped >= 0.
    pub stiffness: f64,
    /// Viscosity coefficient.
    pub viscosity: f64,
    /// Gravity (px/s²), screen y-down.
    pub gravity: Vec2,
    /// Wall restitution in [0, 1].
    pub wall_restitution: f64,
}

impl Default for SphParams {
    fn default() -> Self {
        SphParams {
            h: 16.0,
            rest_density: 6.0,
            stiffness: 40.0,
            viscosity: 0.6,
            gravity: Vec2::new(0.0, 900.0),
            wall_restitution: 0.2,
        }
    }
}

/// One SPH particle: implicit mass 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SphParticle {
    /// Position, px.
    pub pos: Vec2,
    /// Velocity, px/s.
    pub vel: Vec2,
}

/// A bounded SPH system inside an axis-aligned box.
pub struct SphSystem {
    particles: Vec<SphParticle>,
    params: SphParams,
    bounds_min: Vec2,
    bounds_max: Vec2,
}

/// poly6 kernel (2D): 315/(64 pi h^9) * (h^2 - r^2)^3.
fn poly6(r2: f64, h: f64) -> f64 {
    let h9 = h.powi(9);
    let t = h * h - r2;
    if t <= 0.0 {
        0.0
    } else {
        315.0 / (64.0 * std::f64::consts::PI * h9) * t * t * t
    }
}

/// Spiky gradient magnitude factor (2D): -45/(pi h^6) * (h - r)^2.
/// Direction is the unit vector between the pair.
fn spiky_grad(r: f64, h: f64) -> f64 {
    let h6 = h.powi(6);
    -45.0 / (std::f64::consts::PI * h6) * (h - r) * (h - r)
}

impl SphSystem {
    /// New empty system inside the box.
    #[must_use]
    pub fn new(bounds_min: Vec2, bounds_max: Vec2, params: SphParams) -> Self {
        SphSystem {
            particles: Vec::new(),
            params,
            bounds_min,
            bounds_max,
        }
    }

    /// Fill a disc of particles on a grid of `spacing` px (an initial
    /// fluid block). Returns the number added.
    pub fn fill_disc(&mut self, center: Vec2, radius: f64, spacing: f64) -> usize {
        let mut added = 0;
        let mut y = center.y - radius;
        while y <= center.y + radius {
            let mut x = center.x - radius;
            while x <= center.x + radius {
                let dx = x - center.x;
                let dy = y - center.y;
                if dx * dx + dy * dy <= radius * radius {
                    self.particles.push(SphParticle {
                        pos: Vec2::new(x, y),
                        vel: Vec2::ZERO,
                    });
                    added += 1;
                }
                x += spacing;
            }
            y += spacing;
        }
        added
    }

    /// Live particle count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.particles.len()
    }

    /// Whether no particles exist.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.particles.is_empty()
    }

    /// Particle slice (render + oracle use).
    #[must_use]
    pub fn particles(&self) -> &[SphParticle] {
        &self.particles
    }

    /// Mutable particle slice (external coupling: game bodies push
    /// the fluid).
    pub fn particles_mut(&mut self) -> &mut [SphParticle] {
        &mut self.particles
    }

    /// One fixed step: gravity, density, pressure + viscosity forces,
    /// integration, wall clamp. Deterministic under a fixed dt.
    pub fn step(&mut self, dt: f64) {
        let h = self.params.h;
        if self.particles.is_empty() {
            return;
        }
        let r2_max = h * h;

        // Density pass (includes the self-contribution).
        let densities: Vec<f64> = self
            .particles
            .iter()
            .enumerate()
            .map(|(i, pi)| {
                let mut rho = poly6(0.0, h);
                for (j, pj) in self.particles.iter().enumerate() {
                    if i == j {
                        continue;
                    }
                    let dx = pi.pos.x - pj.pos.x;
                    let dy = pi.pos.y - pj.pos.y;
                    let r2 = dx * dx + dy * dy;
                    if r2 < r2_max {
                        rho += poly6(r2, h);
                    }
                }
                rho
            })
            .collect();

        // Force pass: pressure gradient + viscosity.
        let accel: Vec<Vec2> = self
            .particles
            .iter()
            .zip(densities.iter())
            .enumerate()
            .map(|(i, (pi, &rho_i))| {
                let rho_i = rho_i.max(1e-6);
                let p_i = (self.params.stiffness * (rho_i - self.params.rest_density)).max(0.0);
                let mut f = Vec2::ZERO;
                for (j, (pj, &rho_j)) in self.particles.iter().zip(densities.iter()).enumerate() {
                    if i == j {
                        continue;
                    }
                    let dx = pi.pos.x - pj.pos.x;
                    let dy = pi.pos.y - pj.pos.y;
                    let r2 = dx * dx + dy * dy;
                    if r2 >= r2_max {
                        continue;
                    }
                    let r = r2.sqrt().max(1e-6);
                    let rho_j = rho_j.max(1e-6);
                    let p_j = (self.params.stiffness * (rho_j - self.params.rest_density)).max(0.0);
                    let dir = Vec2::new(dx / r, dy / r);
                    // Pressure: symmetric gradient, density-weighted.
                    let pm = (p_i + p_j) * 0.5 / rho_j;
                    let gp = spiky_grad(r, h);
                    f.x += dir.x * gp * pm;
                    f.y += dir.y * gp * pm;
                    // Viscosity: Laplacian kernel, velocity difference.
                    let vl = 45.0 / (std::f64::consts::PI * h6(h)) * (h - r);
                    let dvx = pj.vel.x - pi.vel.x;
                    let dvy = pj.vel.y - pi.vel.y;
                    f.x += self.params.viscosity * dvx / rho_j * vl;
                    f.y += self.params.viscosity * dvy / rho_j * vl;
                }
                // Acceleration clamp: a runaway force must not explode
                // the pool (the oracle bounds particle speed).
                let speed2 = f.x * f.x + f.y * f.y;
                const MAX_ACC: f64 = 400_000.0;
                if speed2 > MAX_ACC * MAX_ACC {
                    let s = MAX_ACC / speed2.sqrt();
                    f = Vec2::new(f.x * s, f.y * s);
                }
                f
            })
            .collect();

        // Integration (semi-implicit Euler) + wall clamp.
        let rest = self.params.wall_restitution;
        let r = 2.0_f64; // wall skin
        for (p, a) in self.particles.iter_mut().zip(accel.iter()) {
            p.vel.x += (self.params.gravity.x + a.x) * dt;
            p.vel.y += (self.params.gravity.y + a.y) * dt;
            p.pos.x += p.vel.x * dt;
            p.pos.y += p.vel.y * dt;
            if p.pos.x < self.bounds_min.x + r {
                p.pos.x = self.bounds_min.x + r;
                p.vel.x = -p.vel.x * rest;
            }
            if p.pos.x > self.bounds_max.x - r {
                p.pos.x = self.bounds_max.x - r;
                p.vel.x = -p.vel.x * rest;
            }
            if p.pos.y < self.bounds_min.y + r {
                p.pos.y = self.bounds_min.y + r;
                p.vel.y = -p.vel.y * rest;
            }
            if p.pos.y > self.bounds_max.y - r {
                p.pos.y = self.bounds_max.y - r;
                p.vel.y = -p.vel.y * rest;
            }
        }
    }
}

fn h6(h: f64) -> f64 {
    h.powi(6)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settled_system() -> SphSystem {
        let mut sys = SphSystem::new(Vec2::ZERO, Vec2::new(360.0, 740.0), SphParams::default());
        sys.fill_disc(Vec2::new(180.0, 300.0), 60.0, 9.0);
        sys
    }

    /// Oracle: mass conservation - the pool never gains or loses
    /// particles.
    #[test]
    fn particle_count_conserved() {
        let mut sys = settled_system();
        let n0 = sys.len();
        assert!(n0 > 100, "disc fill must place a pool: {n0}");
        for _ in 0..300 {
            sys.step(1.0 / 120.0);
        }
        assert_eq!(sys.len(), n0, "SPH conserves particles");
    }

    /// Oracle: walls contain the pool.
    #[test]
    fn walls_contain_pool() {
        let mut sys = settled_system();
        for _ in 0..300 {
            sys.step(1.0 / 120.0);
        }
        for p in sys.particles() {
            assert!(p.pos.x >= 0.0 && p.pos.x <= 360.0, "x out: {}", p.pos.x);
            assert!(p.pos.y >= 0.0 && p.pos.y <= 740.0, "y out: {}", p.pos.y);
        }
    }

    /// Oracle: the pool settles - bounded speed, no explosion.
    #[test]
    fn pool_settles_without_explosion() {
        let mut sys = settled_system();
        for _ in 0..600 {
            sys.step(1.0 / 120.0);
        }
        let mut max_speed = 0.0_f64;
        for p in sys.particles() {
            let s = p.vel.x * p.vel.x + p.vel.y * p.vel.y;
            max_speed = max_speed.max(s.sqrt());
        }
        assert!(
            max_speed < 900.0,
            "settled pool must not explode: max speed {max_speed:.0}"
        );
    }

    /// Oracle: same inputs, same bytes - the solver is deterministic.
    #[test]
    fn sph_is_deterministic() {
        let run = || {
            let mut sys = settled_system();
            for _ in 0..120 {
                sys.step(1.0 / 120.0);
            }
            sys.particles()
                .iter()
                .map(|p| (p.pos.x.to_bits(), p.pos.y.to_bits()))
                .collect::<Vec<_>>()
        };
        assert_eq!(run(), run(), "same inputs must produce identical state");
    }
}

//! The stable-fluids solver.
//!
//! Eulerian 3D solver per Stam's "Stable Fluids": semi-Lagrangian
//! advection (unconditionally stable), buoyancy from density, and
//! Jacobi-iterated pressure projection to keep the velocity field
//! divergence-free. Solid-box boundaries: the normal velocity
//! component is zeroed on boundary faces.
//!
//! Deterministic: fixed cell iteration order, pure arithmetic.

use crate::grid::{Field, VelocityField};

/// Solver configuration and state.
#[derive(Clone, Debug)]
pub struct FluidSolver {
    /// Grid dimensions.
    pub dims: (usize, usize, usize),
    /// Velocity field.
    pub velocity: VelocityField,
    /// Smoke density field (advected, drives buoyancy).
    pub density: Field,
    /// Buoyancy: upward (Y) acceleration per unit density.
    pub buoyancy: f64,
    /// Jacobi iterations for the pressure solve per step.
    pub pressure_iters: u32,
}

impl FluidSolver {
    /// New solver over `dims` with all fields zeroed.
    #[must_use]
    pub fn new(dims: (usize, usize, usize)) -> Self {
        FluidSolver {
            velocity: VelocityField::zero(dims),
            density: Field::zero(dims),
            dims,
            buoyancy: 2.0,
            pressure_iters: 20,
        }
    }

    /// Inject density at a cell (smoke source).
    pub fn add_density(&mut self, x: usize, y: usize, z: usize, amount: f64) {
        self.density.add(x, y, z, amount);
    }

    /// Add velocity at a cell (forces, emitters).
    pub fn add_velocity(&mut self, x: usize, y: usize, z: usize, v: [f64; 3]) {
        self.velocity.add(x, y, z, v);
    }

    /// Advance the simulation by `dt`.
    pub fn step(&mut self, dt: f64) {
        self.apply_buoyancy(dt);
        self.project();
        self.advect_velocity(dt);
        self.advect_density(dt);
        self.enforce_boundaries();
        self.project();
    }

    /// Buoyancy: density pushes velocity up (+Y).
    fn apply_buoyancy(&mut self, dt: f64) {
        let (nx, ny, nz) = self.dims;
        for z in 0..nz {
            for y in 0..ny {
                for x in 0..nx {
                    let d = self.density.get(x, y, z);
                    if d > 0.0 {
                        self.velocity
                            .add(x, y, z, [0.0, self.buoyancy * d * dt, 0.0]);
                    }
                }
            }
        }
    }

    /// Semi-Lagrangian advection: for each cell, backtrace along
    /// the current velocity and sample the source field there.
    fn advect(&self, field: &Field, dt: f64) -> Field {
        let (nx, ny, nz) = self.dims;
        let mut out = Field::zero(self.dims);
        for z in 0..nz {
            for y in 0..ny {
                for x in 0..nx {
                    let [vx, vy, vz] = self.velocity.get(x, y, z);
                    let gx = x as f64 - vx * dt;
                    let gy = y as f64 - vy * dt;
                    let gz = z as f64 - vz * dt;
                    let v = field.sample(gx, gy, gz);
                    out.set(x, y, z, v);
                }
            }
        }
        out
    }

    fn advect_density(&mut self, dt: f64) {
        self.density = self.advect(&self.density, dt);
    }

    fn advect_velocity(&mut self, dt: f64) {
        let (nx, ny, nz) = self.dims;
        let mut out = VelocityField::zero(self.dims);
        for z in 0..nz {
            for y in 0..ny {
                for x in 0..nx {
                    let [vx, vy, vz] = self.velocity.get(x, y, z);
                    let gx = x as f64 - vx * dt;
                    let gy = y as f64 - vy * dt;
                    let gz = z as f64 - vz * dt;
                    let v = self.velocity.sample(gx, gy, gz);
                    out.set(x, y, z, v);
                }
            }
        }
        let _ = (nx, ny, nz);
        self.velocity = out;
    }

    /// Jacobi pressure projection: iterate toward the pressure that
    /// removes the divergence, then subtract its gradient.
    fn project(&mut self) {
        let (nx, ny, nz) = self.dims;
        // Divergence per interior cell (central differences).
        let mut div = Field::zero(self.dims);
        for z in 1..nz.saturating_sub(1) {
            for y in 1..ny.saturating_sub(1) {
                for x in 1..nx.saturating_sub(1) {
                    let [vx_m, _, _] = self.velocity.get(x.saturating_sub(1), y, z);
                    let [vx_p, _, _] = self.velocity.get(x + 1, y, z);
                    let [_, vy_m, _] = self.velocity.get(x, y.saturating_sub(1), z);
                    let [_, vy_p, _] = self.velocity.get(x, y + 1, z);
                    let [_, _, vz_m] = self.velocity.get(x, y, z.saturating_sub(1));
                    let [_, _, vz_p] = self.velocity.get(x, y, z + 1);
                    div.set(
                        x,
                        y,
                        z,
                        -0.5 * ((vx_p - vx_m) + (vy_p - vy_m) + (vz_p - vz_m)),
                    );
                }
            }
        }
        // Jacobi iterations on pressure: p = (sum of 6 neighbors +
        // div) / 6.
        let mut pressure = Field::zero(self.dims);
        let mut next = Field::zero(self.dims);
        for _ in 0..self.pressure_iters {
            for z in 1..nz.saturating_sub(1) {
                for y in 1..ny.saturating_sub(1) {
                    for x in 1..nx.saturating_sub(1) {
                        let sum = pressure.get(x - 1, y, z)
                            + pressure.get(x + 1, y, z)
                            + pressure.get(x, y - 1, z)
                            + pressure.get(x, y + 1, z)
                            + pressure.get(x, y, z - 1)
                            + pressure.get(x, y, z + 1);
                        next.set(x, y, z, (sum + div.get(x, y, z)) / 6.0);
                    }
                }
            }
            std::mem::swap(&mut pressure, &mut next);
        }
        // Subtract the pressure gradient from velocity.
        for z in 1..nz.saturating_sub(1) {
            for y in 1..ny.saturating_sub(1) {
                for x in 1..nx.saturating_sub(1) {
                    let gx = (pressure.get(x + 1, y, z) - pressure.get(x - 1, y, z)) * 0.5;
                    let gy = (pressure.get(x, y + 1, z) - pressure.get(x, y - 1, z)) * 0.5;
                    let gz = (pressure.get(x, y, z + 1) - pressure.get(x, y, z - 1)) * 0.5;
                    self.velocity.add(x, y, z, [-gx, -gy, -gz]);
                }
            }
        }
    }

    /// Solid-box boundaries: zero the velocity of boundary cells.
    fn enforce_boundaries(&mut self) {
        let (nx, ny, nz) = self.dims;
        for z in 0..nz {
            for y in 0..ny {
                for x in 0..nx {
                    let on_boundary =
                        x == 0 || y == 0 || z == 0 || x == nx - 1 || y == ny - 1 || z == nz - 1;
                    if on_boundary {
                        self.velocity.set(x, y, z, [0.0, 0.0, 0.0]);
                    }
                }
            }
        }
    }

    /// Mean absolute divergence over interior cells (the
    /// incompressibility metric: shrinks as projection converges).
    #[must_use]
    pub fn mean_abs_divergence(&self) -> f64 {
        let (nx, ny, nz) = self.dims;
        let mut total = 0.0;
        let mut count = 0_usize;
        for z in 1..nz.saturating_sub(1) {
            for y in 1..ny.saturating_sub(1) {
                for x in 1..nx.saturating_sub(1) {
                    let [vx0, _, _] = self.velocity.get(x, y, z);
                    let [vx1, _, _] = self.velocity.get(x + 1, y, z);
                    let [_, vy0, _] = self.velocity.get(x, y - 1, z);
                    let [_, vy1, _] = self.velocity.get(x, y + 1, z);
                    let [_, _, vz0] = self.velocity.get(x, y, z.saturating_sub(1));
                    let [_, _, vz1] = self.velocity.get(x, y, z + 1);
                    total += ((vx1 - vx0) + (vy1 - vy0) + (vz1 - vz0)).abs();
                    count += 1;
                }
            }
        }
        if count == 0 {
            return 0.0;
        }
        total / count as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIMS: (usize, usize, usize) = (16, 16, 16);

    #[test]
    fn project_reduces_divergence() {
        let mut s = FluidSolver::new(DIMS);
        // A random-ish velocity field: add alternating impulses.
        for z in 1..15 {
            for y in 1..15 {
                for x in 1..15 {
                    let a = ((x * 7 + y * 13 + z * 3) % 5) as f64 - 2.0;
                    s.add_velocity(x, y, z, [a, a * 0.5, -a]);
                }
            }
        }
        let before = s.mean_abs_divergence();
        s.step(1.0 / 60.0);
        let after = s.mean_abs_divergence();
        assert!(
            after < before,
            "projection must shrink divergence: {before} -> {after}"
        );
    }

    #[test]
    fn density_advects_with_velocity() {
        let mut s = FluidSolver::new(DIMS);
        // Inject a small density blob.
        for dx in 7..=9 {
            for dy in 7..=9 {
                for dz in 7..=9 {
                    s.add_density(dx, dy, dz, 2.0);
                }
            }
        }
        // A uniform +X wind through the blob's slab: a constant
        // field is divergence-free, so projection preserves it.
        for x in 2..13 {
            for y in 5..12 {
                for z in 5..12 {
                    s.add_velocity(x, y, z, [5.0, 0.0, 0.0]);
                }
            }
        }
        s.step(1.0 / 60.0);
        s.step(1.0 / 60.0);
        // Density must have moved toward +X: cells at x > 8 hold
        // more than cells at x < 8.
        let mut right = 0.0;
        let mut left = 0.0;
        for z in 6..11 {
            for y in 6..11 {
                for x in 9..12 {
                    right += s.density.get(x, y, z);
                }
                for x in 5..8 {
                    left += s.density.get(x, y, z);
                }
            }
        }
        assert!(
            right > left,
            "density advects along +X: right {right} > left {left}"
        );
    }

    #[test]
    fn buoyancy_rises_density() {
        let mut s = FluidSolver::new(DIMS);
        // Density slab.
        for x in 6..=10 {
            for z in 6..=10 {
                for y in 3..=5 {
                    s.add_density(x, y, z, 2.0);
                }
            }
        }
        // A uniform updraft through the slab's column: a constant
        // field survives projection.
        for x in 5..=11 {
            for z in 5..=11 {
                for y in 2..=8 {
                    s.add_velocity(x, y, z, [0.0, 2.0, 0.0]);
                }
            }
        }
        s.step(1.0 / 60.0);
        s.step(1.0 / 60.0);
        s.step(1.0 / 60.0);
        // Density must have climbed: cells above y=4 now hold mass.
        let mut above = 0.0;
        let mut at_start = 0.0;
        for z in 6..11 {
            for x in 6..11 {
                for y in 5..8 {
                    above += s.density.get(x, y, z);
                }
                at_start += s.density.get(x, 4, z);
            }
        }
        assert!(
            above > at_start,
            "buoyancy lifts density: above {above} > at {at_start}"
        );
    }

    #[test]
    fn boundaries_zero_out() {
        let mut s = FluidSolver::new(DIMS);
        s.add_velocity(0, 5, 5, [9.0, 9.0, 9.0]);
        s.add_velocity(15, 5, 5, [9.0, 9.0, 9.0]);
        s.step(1.0 / 60.0);
        assert_eq!(s.velocity.get(0, 5, 5), [0.0, 0.0, 0.0]);
        assert_eq!(s.velocity.get(15, 5, 5), [0.0, 0.0, 0.0]);
    }

    #[test]
    fn solver_is_deterministic() {
        let run = || {
            let mut s = FluidSolver::new(DIMS);
            for k in 0..40_usize {
                s.add_density(4 + (k % 4), 4, 4, 5.0);
                s.add_velocity(4 + (k % 4), 4, 4, [1.0, 2.0, 0.5]);
            }
            for _ in 0..20 {
                s.step(1.0 / 60.0);
            }
            // Hash both fields.
            let mut h: u64 = 0xcbf29ce484222325;
            for v in s.velocity.data.iter().chain(s.density.data.iter()) {
                let bits = v.to_bits();
                for b in bits.to_le_bytes() {
                    h = (h ^ u64::from(b)).wrapping_mul(0x100000001b3);
                }
            }
            h
        };
        assert_eq!(run(), run(), "solver byte-identical across runs");
    }

    #[test]
    fn total_density_stays_bounded() {
        let mut s = FluidSolver::new(DIMS);
        s.add_density(8, 4, 8, 50.0);
        let m0 = s.density.total();
        for _ in 0..30 {
            s.step(1.0 / 60.0);
        }
        let m1 = s.density.total();
        assert!(
            m1 <= m0 + 1e-6 && m1 >= -1e-6,
            "density mass bounded (no injection, no explosion): {m0} -> {m1}"
        );
    }
}

//! Scalar and vector fields on a 3D grid.
//!
//! Linear-indexed dense grids (`x + nx*(y + ny*z)`), trilinear
//! sampling at fractional positions, and divergence-free-friendly
//! neighbor access for the pressure solve.

/// A scalar field on a uniform grid.
#[derive(Clone, Debug)]
pub struct Field {
    /// Grid dimensions.
    pub dims: (usize, usize, usize),
    /// Cell values, linear-indexed.
    pub data: Vec<f64>,
}

impl Field {
    /// Zero-filled field of the given dimensions.
    #[must_use]
    pub fn zero(dims: (usize, usize, usize)) -> Self {
        let len = dims.0.saturating_mul(dims.1).saturating_mul(dims.2);
        Field {
            dims,
            data: vec![0.0; len],
        }
    }

    /// Linear index of a cell.
    #[must_use]
    pub fn index(&self, x: usize, y: usize, z: usize) -> usize {
        let (nx, ny, _) = self.dims;
        x.saturating_add(nx.saturating_mul(y.saturating_add(ny.saturating_mul(z))))
    }

    /// Read a cell (bounds-clamped).
    #[must_use]
    pub fn get(&self, x: usize, y: usize, z: usize) -> f64 {
        let (nx, ny, nz) = self.dims;
        let x = x.min(nx.saturating_sub(1));
        let y = y.min(ny.saturating_sub(1));
        let z = z.min(nz.saturating_sub(1));
        self.data.get(self.index(x, y, z)).copied().unwrap_or(0.0)
    }

    /// Write a cell (bounds-clamped).
    pub fn set(&mut self, x: usize, y: usize, z: usize, v: f64) {
        let (nx, ny, nz) = self.dims;
        let x = x.min(nx.saturating_sub(1));
        let y = y.min(ny.saturating_sub(1));
        let z = z.min(nz.saturating_sub(1));
        let i = self.index(x, y, z);
        if let Some(slot) = self.data.get_mut(i) {
            *slot = v;
        }
    }

    /// Add to a cell (bounds-clamped).
    pub fn add(&mut self, x: usize, y: usize, z: usize, v: f64) {
        let (nx, ny, nz) = self.dims;
        let x = x.min(nx.saturating_sub(1));
        let y = y.min(ny.saturating_sub(1));
        let z = z.min(nz.saturating_sub(1));
        let i = self.index(x, y, z);
        if let Some(slot) = self.data.get_mut(i) {
            *slot += v;
        }
    }

    /// Trilinear sample at fractional grid coordinates (cell
    /// units). Out-of-range coordinates clamp to the border.
    #[must_use]
    pub fn sample(&self, gx: f64, gy: f64, gz: f64) -> f64 {
        let (nx, ny, nz) = self.dims;
        let max_x = (nx as f64 - 1.0).max(0.0);
        let max_y = (ny as f64 - 1.0).max(0.0);
        let max_z = (nz as f64 - 1.0).max(0.0);
        let gx = gx.clamp(0.0, max_x);
        let gy = gy.clamp(0.0, max_y);
        let gz = gz.clamp(0.0, max_z);
        let x0 = gx.floor() as usize;
        let y0 = gy.floor() as usize;
        let z0 = gz.floor() as usize;
        let x1 = (x0 + 1).min(nx.saturating_sub(1));
        let y1 = (y0 + 1).min(ny.saturating_sub(1));
        let z1 = (z0 + 1).min(nz.saturating_sub(1));
        let fx = gx - x0 as f64;
        let fy = gy - y0 as f64;
        let fz = gz - z0 as f64;
        let c000 = self.get(x0, y0, z0);
        let c100 = self.get(x1, y0, z0);
        let c010 = self.get(x0, y1, z0);
        let c110 = self.get(x1, y1, z0);
        let c001 = self.get(x0, y0, z1);
        let c101 = self.get(x1, y0, z1);
        let c011 = self.get(x0, y1, z1);
        let c111 = self.get(x1, y1, z1);
        let x00 = c000 + (c100 - c000) * fx;
        let x10 = c010 + (c110 - c010) * fx;
        let x01 = c001 + (c101 - c001) * fx;
        let x11 = c011 + (c111 - c011) * fx;
        let y0v = x00 + (x10 - x00) * fy;
        let y1v = x01 + (x11 - x01) * fy;
        y0v + (y1v - y0v) * fz
    }

    /// Sum of all cell values (mass check).
    #[must_use]
    pub fn total(&self) -> f64 {
        self.data.iter().sum()
    }
}

/// A vector field: three scalar components packed per cell.
#[derive(Clone, Debug)]
pub struct VelocityField {
    /// Grid dimensions.
    pub dims: (usize, usize, usize),
    /// Velocities as flat `[x, y, z]` triples, linear-indexed.
    pub data: Vec<f64>,
}

impl VelocityField {
    /// Zero-filled velocity field of the given dimensions.
    #[must_use]
    pub fn zero(dims: (usize, usize, usize)) -> Self {
        let len = dims.0.saturating_mul(dims.1).saturating_mul(dims.2) * 3;
        VelocityField {
            dims,
            data: vec![0.0; len],
        }
    }

    /// Linear base index of a cell's triple.
    #[must_use]
    pub fn index(&self, x: usize, y: usize, z: usize) -> usize {
        let (nx, ny, _) = self.dims;
        let cell = x.saturating_add(nx.saturating_mul(y.saturating_add(ny.saturating_mul(z))));
        cell.saturating_mul(3)
    }

    /// Read a cell's velocity.
    #[must_use]
    pub fn get(&self, x: usize, y: usize, z: usize) -> [f64; 3] {
        let i = self.index(x, y, z);
        [
            self.data.get(i).copied().unwrap_or(0.0),
            self.data.get(i + 1).copied().unwrap_or(0.0),
            self.data.get(i + 2).copied().unwrap_or(0.0),
        ]
    }

    /// Write a cell's velocity.
    pub fn set(&mut self, x: usize, y: usize, z: usize, v: [f64; 3]) {
        let i = self.index(x, y, z);
        for (k, slot) in (i..i + 3).enumerate() {
            if let (Some(s), Some(val)) = (self.data.get_mut(slot), v.get(k)) {
                *s = *val;
            }
        }
    }

    /// Add to a cell's velocity.
    pub fn add(&mut self, x: usize, y: usize, z: usize, v: [f64; 3]) {
        let i = self.index(x, y, z);
        for (k, slot) in (i..i + 3).enumerate() {
            if let (Some(s), Some(val)) = (self.data.get_mut(slot), v.get(k)) {
                *s += *val;
            }
        }
    }

    /// Trilinear sample of a single component at fractional
    /// grid coordinates.
    #[must_use]
    pub fn sample_component(&self, comp: usize, gx: f64, gy: f64, gz: f64) -> f64 {
        let (nx, ny, nz) = self.dims;
        let max_x = (nx as f64 - 1.0).max(0.0);
        let max_y = (ny as f64 - 1.0).max(0.0);
        let max_z = (nz as f64 - 1.0).max(0.0);
        let gx = gx.clamp(0.0, max_x);
        let gy = gy.clamp(0.0, max_y);
        let gz = gz.clamp(0.0, max_z);
        let x0 = gx.floor() as usize;
        let y0 = gy.floor() as usize;
        let z0 = gz.floor() as usize;
        let x1 = (x0 + 1).min(nx.saturating_sub(1));
        let y1 = (y0 + 1).min(ny.saturating_sub(1));
        let z1 = (z0 + 1).min(nz.saturating_sub(1));
        let fx = gx - x0 as f64;
        let fy = gy - y0 as f64;
        let fz = gz - z0 as f64;
        let s = |x: usize, y: usize, z: usize| -> f64 {
            let i = self.index(x.min(nx - 1), y.min(ny - 1), z.min(nz - 1)) + comp;
            self.data.get(i).copied().unwrap_or(0.0)
        };
        let c000 = s(x0, y0, z0);
        let c100 = s(x1, y0, z0);
        let c010 = s(x0, y1, z0);
        let c110 = s(x1, y1, z0);
        let c001 = s(x0, y0, z1);
        let c101 = s(x1, y0, z1);
        let c011 = s(x0, y1, z1);
        let c111 = s(x1, y1, z1);
        let x00 = c000 + (c100 - c000) * fx;
        let x10 = c010 + (c110 - c010) * fx;
        let x01 = c001 + (c101 - c001) * fx;
        let x11 = c011 + (c111 - c011) * fx;
        let y0v = x00 + (x10 - x00) * fy;
        let y1v = x01 + (x11 - x01) * fy;
        y0v + (y1v - y0v) * fz
    }

    /// Trilinear sample of the full velocity vector.
    #[must_use]
    pub fn sample(&self, gx: f64, gy: f64, gz: f64) -> [f64; 3] {
        [
            self.sample_component(0, gx, gy, gz),
            self.sample_component(1, gx, gy, gz),
            self.sample_component(2, gx, gy, gz),
        ]
    }

    /// Sum of all component magnitudes (kinetic-energy proxy).
    #[must_use]
    pub fn magnitude_total(&self) -> f64 {
        self.data.iter().map(|v| v.abs()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_index_is_linear() {
        let f = Field::zero((4, 4, 4));
        assert_eq!(f.index(0, 0, 0), 0);
        assert_eq!(f.index(1, 0, 0), 1);
        assert_eq!(f.index(0, 1, 0), 4);
        assert_eq!(f.index(0, 0, 1), 16);
    }

    #[test]
    fn get_set_round_trip_and_clamp() {
        let mut f = Field::zero((4, 4, 4));
        f.set(1, 2, 3, 7.5);
        assert_eq!(f.get(1, 2, 3), 7.5);
        // Out-of-range reads clamp.
        assert_eq!(f.get(9, 9, 9), 0.0);
        f.set(99, 0, 0, 1.0); // clamps to (3,0,0)
        assert_eq!(f.get(3, 0, 0), 1.0);
    }

    #[test]
    fn trilinear_sample_at_cell_centers_is_exact() {
        let mut f = Field::zero((4, 4, 4));
        f.set(2, 1, 1, 8.0);
        // Exactly on cell (2,1,1).
        assert!((f.sample(2.0, 1.0, 1.0) - 8.0).abs() < 1e-9);
        // Halfway between (1,1,1)=0 and (2,1,1)=8.
        assert!((f.sample(1.5, 1.0, 1.0) - 4.0).abs() < 1e-9);
    }

    #[test]
    fn velocity_sample_interpolates() {
        let mut v = VelocityField::zero((4, 4, 4));
        v.set(1, 1, 1, [3.0, 6.0, 9.0]);
        assert!((v.sample_component(0, 1.0, 1.0, 1.0) - 3.0).abs() < 1e-9);
        let [vx, vy, vz] = v.sample(1.0, 1.0, 1.0);
        assert!((vx - 3.0).abs() < 1e-9 && (vy - 6.0).abs() < 1e-9 && (vz - 9.0).abs() < 1e-9);
    }

    #[test]
    fn total_sums_mass() {
        let mut f = Field::zero((4, 4, 4));
        f.add(1, 1, 1, 2.0);
        f.add(2, 1, 1, 3.0);
        assert!((f.total() - 5.0).abs() < 1e-9);
    }
}

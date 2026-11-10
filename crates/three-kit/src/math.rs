//! 3D math: vectors, matrices, projection.
//!
//! Column-major 4×4 matrices, right-handed view space looking
//! down -Z, standard perspective projection. Deterministic by
//! construction (pure arithmetic, no platform state).

/// A 3-component vector.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec3 {
    /// X component.
    pub x: f64,
    /// Y component.
    pub y: f64,
    /// Z component.
    pub z: f64,
}

impl Vec3 {
    /// Origin.
    pub const ZERO: Vec3 = Vec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    /// New vector.
    #[must_use]
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Vec3 { x, y, z }
    }

    /// Component-wise addition.
    #[must_use]
    pub const fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }

    /// Component-wise subtraction.
    #[must_use]
    pub const fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }

    /// Scalar multiply.
    #[must_use]
    pub const fn scale(self, s: f64) -> Vec3 {
        Vec3::new(self.x * s, self.y * s, self.z * s)
    }

    /// Dot product.
    #[must_use]
    pub const fn dot(self, o: Vec3) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    /// Cross product.
    #[must_use]
    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    /// Squared length.
    #[must_use]
    pub const fn len2(self) -> f64 {
        self.dot(self)
    }

    /// Length.
    #[must_use]
    pub fn len(self) -> f64 {
        self.len2().sqrt()
    }

    /// Unit vector, or `None` for zero-length input.
    #[must_use]
    pub fn normalize(self) -> Option<Vec3> {
        let l = self.len();
        if l < 1e-12 {
            return None;
        }
        Some(self.scale(1.0 / l))
    }
}

/// A column-major 4×4 matrix (`m[col * 4 + row]`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4 {
    /// 16 components, column-major.
    pub m: [f64; 16],
}

impl Mat4 {
    /// Identity.
    #[must_use]
    pub const fn identity() -> Self {
        Mat4 {
            m: [
                1.0, 0.0, 0.0, 0.0, //
                0.0, 1.0, 0.0, 0.0, //
                0.0, 0.0, 1.0, 0.0, //
                0.0, 0.0, 0.0, 1.0,
            ],
        }
    }

    /// Matrix product `self * other` (apply `other` first).
    #[must_use]
    pub fn mul(&self, o: &Mat4) -> Mat4 {
        let mut r = [0.0_f64; 16];
        for (i, slot) in r.iter_mut().enumerate() {
            let col = i / 4;
            let row = i % 4;
            let mut sum = 0.0;
            for k in 0..4 {
                let a = self.m.get(k * 4 + row).copied().unwrap_or(0.0);
                let b = o.m.get(col * 4 + k).copied().unwrap_or(0.0);
                sum += a * b;
            }
            *slot = sum;
        }
        Mat4 { m: r }
    }

    /// Translation matrix.
    #[must_use]
    pub fn translate(t: Vec3) -> Self {
        let mut r = Self::identity();
        r.m[12] = t.x;
        r.m[13] = t.y;
        r.m[14] = t.z;
        r
    }

    /// Uniform scale matrix.
    #[must_use]
    pub fn scale(s: f64) -> Self {
        let mut r = Self::identity();
        r.m[0] = s;
        r.m[5] = s;
        r.m[10] = s;
        r
    }

    /// Rotation about the X axis.
    #[must_use]
    pub fn rotate_x(theta: f64) -> Self {
        let (s, c) = theta.sin_cos();
        let mut r = Self::identity();
        r.m[5] = c;
        r.m[6] = s;
        r.m[9] = -s;
        r.m[10] = c;
        r
    }

    /// Rotation about the Y axis.
    #[must_use]
    pub fn rotate_y(theta: f64) -> Self {
        let (s, c) = theta.sin_cos();
        let mut r = Self::identity();
        r.m[0] = c;
        r.m[2] = -s;
        r.m[8] = s;
        r.m[10] = c;
        r
    }

    /// Rotation about the Z axis.
    #[must_use]
    pub fn rotate_z(theta: f64) -> Self {
        let (s, c) = theta.sin_cos();
        let mut r = Self::identity();
        r.m[0] = c;
        r.m[1] = s;
        r.m[4] = -s;
        r.m[5] = c;
        r
    }

    /// Standard perspective projection (right-handed, -Z forward):
    /// `fov_y` radians, `aspect` = width/height, z in `[near, far]`.
    #[must_use]
    pub fn perspective(fov_y: f64, aspect: f64, near: f64, far: f64) -> Self {
        let f = 1.0 / (fov_y * 0.5).tan();
        let mut r = Mat4 { m: [0.0; 16] };
        r.m[0] = f / aspect;
        r.m[5] = f;
        r.m[10] = (far + near) / (near - far);
        r.m[11] = -1.0;
        r.m[14] = 2.0 * far * near / (near - far);
        r
    }

    /// Right-handed look-at view matrix.
    #[must_use]
    pub fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Self {
        let Some(fwd) = target.sub(eye).normalize() else {
            return Self::identity();
        };
        let Some(right) = fwd.cross(up).normalize() else {
            return Self::identity();
        };
        let up2 = right.cross(fwd);
        let mut r = Self::identity();
        r.m[0] = right.x;
        r.m[4] = right.y;
        r.m[8] = right.z;
        r.m[1] = up2.x;
        r.m[5] = up2.y;
        r.m[9] = up2.z;
        r.m[2] = -fwd.x;
        r.m[6] = -fwd.y;
        r.m[10] = -fwd.z;
        r.m[12] = -right.dot(eye);
        r.m[13] = -up2.dot(eye);
        r.m[14] = fwd.dot(eye);
        r
    }

    /// Transform a point (w-divide applied). Returns `None` when
    /// the point lands on the w=0 plane.
    #[must_use]
    pub fn transform_point(&self, p: Vec3) -> Option<(f64, f64, f64)> {
        let x = self.m[0] * p.x + self.m[4] * p.y + self.m[8] * p.z + self.m[12];
        let y = self.m[1] * p.x + self.m[5] * p.y + self.m[9] * p.z + self.m[13];
        let z = self.m[2] * p.x + self.m[6] * p.y + self.m[10] * p.z + self.m[14];
        let w = self.m[3] * p.x + self.m[7] * p.y + self.m[11] * p.z + self.m[15];
        if w.abs() < 1e-12 {
            return None;
        }
        Some((x / w, y / w, z / w))
    }

    /// Transform a point, returning `(x, y, z, w)` without the
    /// divide — `w` is the view depth (visible when `w >= near`).
    #[must_use]
    pub fn transform_point_w(&self, p: Vec3) -> Option<(f64, f64, f64, f64)> {
        let x = self.m[0] * p.x + self.m[4] * p.y + self.m[8] * p.z + self.m[12];
        let y = self.m[1] * p.x + self.m[5] * p.y + self.m[9] * p.z + self.m[13];
        let z = self.m[2] * p.x + self.m[6] * p.y + self.m[10] * p.z + self.m[14];
        let w = self.m[3] * p.x + self.m[7] * p.y + self.m[11] * p.z + self.m[15];
        if w.abs() < 1e-12 {
            return None;
        }
        Some((x, y, z, w))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cross_is_perpendicular_to_both() {
        let a = Vec3::new(1.0, 0.0, 0.0);
        let b = Vec3::new(0.0, 1.0, 0.0);
        let c = a.cross(b);
        assert!(a.dot(c).abs() < 1e-12 && b.dot(c).abs() < 1e-12);
        assert_eq!(c, Vec3::new(0.0, 0.0, 1.0), "right-handed");
    }

    #[test]
    fn translate_then_transform() {
        let t = Mat4::translate(Vec3::new(5.0, -2.0, 1.0));
        let (x, y, z) = t
            .transform_point(Vec3::new(1.0, 2.0, 3.0))
            .unwrap_or((0.0, 0.0, 0.0));
        assert!((x - 6.0).abs() < 1e-9 && (y - 0.0).abs() < 1e-9 && (z - 4.0).abs() < 1e-9);
    }

    #[test]
    fn mul_is_associative() {
        let a = Mat4::rotate_y(0.3);
        let b = Mat4::translate(Vec3::new(1.0, 2.0, 3.0));
        let c = Mat4::scale(2.0);
        let ab_c = a.mul(&b).mul(&c);
        let a_bc = a.mul(&b.mul(&c));
        assert_eq!(
            ab_c.m.map(|v| (v * 1e9).round() / 1e9),
            a_bc.m.map(|v| (v * 1e9).round() / 1e9)
        );
    }

    #[test]
    fn rotate_z_moves_x_toward_y() {
        let r = Mat4::rotate_z(std::f64::consts::FRAC_PI_2);
        let (x, y, _) = r
            .transform_point(Vec3::new(1.0, 0.0, 0.0))
            .unwrap_or((0.0, 0.0, 0.0));
        assert!(x.abs() < 1e-9 && (y - 1.0).abs() < 1e-9);
    }

    #[test]
    fn perspective_maps_forward_point_into_ndc() {
        let proj = Mat4::perspective(std::f64::consts::FRAC_PI_2, 1.0, 0.1, 100.0);
        let view = Mat4::look_at(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::ZERO,
            Vec3::new(0.0, 1.0, 0.0),
        );
        let vp = proj.mul(&view);
        // A point in front of the camera at the world origin.
        assert!(
            vp.transform_point(Vec3::ZERO).is_some(),
            "origin is in front: w != 0"
        );
        let (x, y, _z) = vp.transform_point(Vec3::ZERO).unwrap_or((1.0, 1.0, 1.0));
        assert!(x.abs() < 1e-9 && y.abs() < 1e-9, "centered: ({x},{y})");
    }

    #[test]
    fn look_at_views_eye_to_origin() {
        let v = Mat4::look_at(
            Vec3::new(0.0, 0.0, 10.0),
            Vec3::ZERO,
            Vec3::new(0.0, 1.0, 0.0),
        );
        // The eye itself lands at the view-space origin.
        let eye_view = v
            .transform_point(Vec3::new(0.0, 0.0, 10.0))
            .unwrap_or((1.0, 1.0, 1.0));
        assert!(
            eye_view.0.abs() < 1e-9 && eye_view.1.abs() < 1e-9 && eye_view.2.abs() < 1e-9,
            "eye maps to origin: {eye_view:?}"
        );
        // The look target (world origin) sits 10 units down -Z.
        let target_view = v.transform_point(Vec3::ZERO).unwrap_or((1.0, 1.0, 1.0));
        assert!(
            target_view.0.abs() < 1e-9
                && target_view.1.abs() < 1e-9
                && (target_view.2 + 10.0).abs() < 1e-6,
            "target maps to (0,0,-10): {target_view:?}"
        );
    }
}

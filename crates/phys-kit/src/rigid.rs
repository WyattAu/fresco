//! Rigid bodies: convex polygons with linear + angular dynamics.
//!
//! Semi-implicit Euler integration, SAT collision with deepest-
//! vertex contacts, impulse resolution with restitution and
//! Coulomb friction, Baumgarte-free positional correction scaled
//! by inverse mass. Static bodies are `inv_mass == 0`.
//!
//! Deterministic: fixed iteration order, no random state.

use path_kit::Vec2;

/// A rigid body: convex polygon in local space (centroid at
/// origin, CCW in Y-up math convention), positioned by `pos` and
/// rotated by `angle`.
#[derive(Clone, Debug)]
pub struct Body {
    /// Center of mass position.
    pub pos: Vec2,
    /// Orientation in radians.
    pub angle: f64,
    /// Linear velocity.
    pub vel: Vec2,
    /// Angular velocity (rad/s).
    pub ang_vel: f64,
    /// Inverse mass (0 = static).
    pub inv_mass: f64,
    /// Inverse rotational inertia about the center of mass.
    pub inv_inertia: f64,
    /// Local-space vertices, convex and CCW, centroid at origin.
    pub verts: Vec<Vec2>,
    /// Bounce factor in `[0, 1]`.
    pub restitution: f64,
    /// Friction coefficient.
    pub friction: f64,
}

impl Body {
    /// Static body from local-space vertices (never moves).
    #[must_use]
    pub fn static_poly(verts: Vec<Vec2>) -> Self {
        let mut b = Body::dynamic_poly(verts, 0.0);
        b.inv_mass = 0.0;
        b.inv_inertia = 0.0;
        b
    }

    /// Dynamic body from local-space vertices at `density`
    /// (mass per unit area). Vertices are re-centered on the
    /// polygon centroid; mass and inertia derived from the area.
    #[must_use]
    pub fn dynamic_poly(verts: Vec<Vec2>, density: f64) -> Self {
        let n = verts.len();
        let mut area = 0.0_f64;
        let mut cx = 0.0_f64;
        let mut cy = 0.0_f64;
        for i in 0..n {
            let a = verts.get(i).copied().unwrap_or_default();
            let b = verts.get((i + 1) % n.max(1)).copied().unwrap_or_default();
            let cross = a.x * b.y - b.x * a.y;
            area += cross * 0.5;
            cx += (a.x + b.x) * cross;
            cy += (a.y + b.y) * cross;
        }
        let area = area.abs();
        let mass = area * density;
        let centroid = if area > 1e-9 {
            Vec2::new(cx / (6.0 * area), cy / (6.0 * area))
        } else {
            Vec2::ZERO
        };
        // Re-center vertices on the centroid; recompute inertia about it.
        let verts: Vec<Vec2> = verts
            .iter()
            .map(|v| Vec2::new(v.x - centroid.x, v.y - centroid.y))
            .collect();
        let mut i = 0.0_f64;
        let m = n;
        for k in 0..m {
            let a = verts.get(k).copied().unwrap_or_default();
            let b = verts.get((k + 1) % m.max(1)).copied().unwrap_or_default();
            let cross = a.x * b.y - b.x * a.y;
            let a2 = a.x * a.x + a.y * a.y;
            let b2 = b.x * b.x + b.y * b.y;
            let ab = a.x * b.x + a.y * b.y;
            i += cross * (a2 + ab + b2);
        }
        let inertia = (i.abs() / 12.0) * density;
        Body {
            pos: centroid,
            angle: 0.0,
            vel: Vec2::ZERO,
            ang_vel: 0.0,
            inv_mass: if mass > 1e-9 { 1.0 / mass } else { 0.0 },
            inv_inertia: if inertia > 1e-9 { 1.0 / inertia } else { 0.0 },
            verts,
            restitution: 0.0,
            friction: 0.4,
        }
    }

    /// World-space vertices (rotate + translate).
    #[must_use]
    pub fn world_verts(&self) -> Vec<Vec2> {
        let (s, c) = self.angle.sin_cos();
        self.verts
            .iter()
            .map(|v| {
                Vec2::new(
                    c * v.x - s * v.y + self.pos.x,
                    s * v.x + c * v.y + self.pos.y,
                )
            })
            .collect()
    }

    /// Apply a world-space impulse at world point `p`.
    pub fn apply_impulse(&mut self, imp: Vec2, p: Vec2) {
        if self.inv_mass <= 0.0 {
            return;
        }
        self.vel = Vec2::new(
            self.vel.x + imp.x * self.inv_mass,
            self.vel.y + imp.y * self.inv_mass,
        );
        let r = Vec2::new(p.x - self.pos.x, p.y - self.pos.y);
        let r_cross = r.x * imp.y - r.y * imp.x;
        self.ang_vel += r_cross * self.inv_inertia;
    }

    /// Advance one timestep: semi-implicit Euler with mild angular
    /// drag (keeps resting bodies from ringing on vertex contacts).
    pub fn step(&mut self, gravity: Vec2, dt: f64) {
        if self.inv_mass <= 0.0 {
            self.ang_vel = 0.0;
            return;
        }
        self.vel = Vec2::new(self.vel.x + gravity.x * dt, self.vel.y + gravity.y * dt);
        self.pos = Vec2::new(self.pos.x + self.vel.x * dt, self.pos.y + self.vel.y * dt);
        self.ang_vel *= 0.99;
        self.angle += self.ang_vel * dt;
    }
}

/// Contact between two convex bodies.
#[derive(Clone, Copy, Debug)]
struct Contact {
    /// Unit normal pointing from A to B.
    normal: Vec2,
    /// Penetration depth along the normal.
    depth: f64,
    /// Contact point in world space.
    point: Vec2,
}

/// SAT: axis of least penetration between convex polygons.
/// Returns `None` when separated. Edge normals are oriented outward
/// against the polygon's own vertex-average centroid, so any
/// consistent winding works in either Y-up or Y-down convention.
fn sat_contact(a_world: &[Vec2], b_world: &[Vec2]) -> Option<Contact> {
    let mut best_sep = f64::NEG_INFINITY;
    let mut best_normal = Vec2::ZERO;
    for (poly, other, flip) in [(a_world, b_world, false), (b_world, a_world, true)] {
        let m = poly.len();
        // Vertex-average centroid for outward orientation.
        let mut c = Vec2::ZERO;
        for v in poly {
            c = Vec2::new(c.x + v.x, c.y + v.y);
        }
        let c = Vec2::new(c.x / m.max(1) as f64, c.y / m.max(1) as f64);
        for i in 0..m {
            let p0 = poly.get(i).copied()?;
            let p1 = poly.get((i + 1) % m.max(1)).copied()?;
            let edge = Vec2::new(p1.x - p0.x, p1.y - p0.y);
            let raw = Vec2::new(-edge.y, edge.x);
            let raw = raw.normalize()?;
            // Flip to point away from the polygon interior.
            let mid = Vec2::new((p0.x + p1.x) * 0.5, (p0.y + p1.y) * 0.5);
            let outward = (mid.x - c.x) * raw.x + (mid.y - c.y) * raw.y;
            let n = if outward >= 0.0 {
                raw
            } else {
                Vec2::new(-raw.x, -raw.y)
            };
            // Separation: the CLOSEST opposite vertex to the face
            // plane (positive = that vertex is still in front, so
            // the plane separates the shapes; negative = intrusion
            // depth of B's deepest vertex).
            let mut sep = f64::INFINITY;
            for v in other {
                let d = (v.x - p0.x) * n.x + (v.y - p0.y) * n.y;
                if d < sep {
                    sep = d;
                }
            }
            if sep > 0.0 {
                return None; // separating axis found
            }
            // Least-penetration face = largest (closest-to-zero)
            // separation.
            if sep > best_sep {
                best_sep = sep;
                best_normal = if flip { Vec2::new(-n.x, -n.y) } else { n };
            }
        }
    }
    // Contact point: average of B's vertices within a tolerance of
    // the deepest intrusion (a face-contact stabilizer: a lone
    // corner vertex rocks the box corner-to-corner).
    const CONTACT_TOL: f64 = 0.5;
    let mut min_proj = f64::INFINITY;
    for v in b_world {
        let proj = v.x * best_normal.x + v.y * best_normal.y;
        if proj < min_proj {
            min_proj = proj;
        }
    }
    let mut acc = Vec2::ZERO;
    let mut count = 0_usize;
    for v in b_world {
        let proj = v.x * best_normal.x + v.y * best_normal.y;
        if proj - min_proj <= CONTACT_TOL {
            acc = Vec2::new(acc.x + v.x, acc.y + v.y);
            count += 1;
        }
    }
    let contact = if count > 0 {
        Vec2::new(acc.x / count as f64, acc.y / count as f64)
    } else {
        b_world.first().copied().unwrap_or_default()
    };
    Some(Contact {
        normal: best_normal,
        depth: -best_sep,
        point: contact,
    })
}

/// Resolve a contact with positional correction and impulses.
fn resolve_contact(c: &Contact, a: &mut Body, b: &mut Body) {
    // Positional correction, mass-weighted.
    let w_sum = a.inv_mass + b.inv_mass;
    if w_sum <= 0.0 {
        return;
    }
    let wa = a.inv_mass / w_sum;
    let wb = b.inv_mass / w_sum;
    let corr_a = Vec2::new(-c.normal.x * c.depth * wa, -c.normal.y * c.depth * wa);
    let corr_b = Vec2::new(c.normal.x * c.depth * wb, c.normal.y * c.depth * wb);
    a.pos = Vec2::new(a.pos.x + corr_a.x, a.pos.y + corr_a.y);
    b.pos = Vec2::new(b.pos.x + corr_b.x, b.pos.y + corr_b.y);

    // Impulse.
    let ra = Vec2::new(c.point.x - a.pos.x, c.point.y - a.pos.y);
    let rb = Vec2::new(c.point.x - b.pos.x, c.point.y - b.pos.y);
    // Velocity at contact: v + omega x r (2D cross: omega * (-r.y, r.x)).
    let va = Vec2::new(a.vel.x - a.ang_vel * ra.y, a.vel.y + a.ang_vel * ra.x);
    let vb = Vec2::new(b.vel.x - b.ang_vel * rb.y, b.vel.y + b.ang_vel * rb.x);
    let rel = Vec2::new(vb.x - va.x, vb.y - va.y);
    let vel_n = rel.x * c.normal.x + rel.y * c.normal.y;
    if vel_n > 0.0 {
        return; // separating
    }
    let ra_n = ra.x * c.normal.y - ra.y * c.normal.x;
    let rb_n = rb.x * c.normal.y - rb.y * c.normal.x;
    let inv_eff =
        a.inv_mass + b.inv_mass + ra_n * ra_n * a.inv_inertia + rb_n * rb_n * b.inv_inertia;
    if inv_eff <= 0.0 {
        return;
    }
    let e = a.restitution.min(b.restitution);
    let j = -(1.0 + e) * vel_n / inv_eff;
    let imp = Vec2::new(c.normal.x * j, c.normal.y * j);
    a.apply_impulse(Vec2::new(-imp.x, -imp.y), c.point);
    b.apply_impulse(imp, c.point);

    // Friction along the tangent.
    let t = Vec2::new(-c.normal.y, c.normal.x);
    let va2 = Vec2::new(a.vel.x - a.ang_vel * ra.y, a.vel.y + a.ang_vel * ra.x);
    let vb2 = Vec2::new(b.vel.x - b.ang_vel * rb.y, b.vel.y + b.ang_vel * rb.x);
    let rel_t = Vec2::new(vb2.x - va2.x, vb2.y - va2.y);
    let vel_t = rel_t.x * t.x + rel_t.y * t.y;
    let ra_t = ra.x * t.y - ra.y * t.x;
    let rb_t = rb.x * t.y - rb.y * t.x;
    let inv_eff_t =
        a.inv_mass + b.inv_mass + ra_t * ra_t * a.inv_inertia + rb_t * rb_t * b.inv_inertia;
    if inv_eff_t <= 0.0 {
        return;
    }
    let jt = -vel_t / inv_eff_t;
    let max_friction = j.abs() * a.friction.min(b.friction);
    let jt = jt.clamp(-max_friction, max_friction);
    let timp = Vec2::new(t.x * jt, t.y * jt);
    a.apply_impulse(Vec2::new(-timp.x, -timp.y), c.point);
    b.apply_impulse(timp, c.point);
}

/// Advance a set of rigid bodies: integrate, then resolve all
/// overlapping pairs for `iterations` rounds.
pub fn step_rigid(bodies: &mut [Body], gravity: Vec2, dt: f64, iterations: u32) {
    for b in bodies.iter_mut() {
        b.step(gravity, dt);
    }
    for _ in 0..iterations {
        for i in 0..bodies.len() {
            for j in (i + 1)..bodies.len() {
                let (left, right) = bodies.split_at_mut(j);
                let (Some(a), Some(b)) = (left.get_mut(i), right.first_mut()) else {
                    continue;
                };
                if a.inv_mass <= 0.0 && b.inv_mass <= 0.0 {
                    continue;
                }
                let aw = a.world_verts();
                let bw = b.world_verts();
                if let Some(c) = sat_contact(&aw, &bw) {
                    resolve_contact(&c, a, b);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use path_kit::Vec2;

    fn square_body(cx: f64, cy: f64, half: f64, density: f64) -> Body {
        let verts = vec![
            Vec2::new(-half, -half),
            Vec2::new(half, -half),
            Vec2::new(half, half),
            Vec2::new(-half, half),
        ];
        let mut b = Body::dynamic_poly(verts, density);
        b.pos = Vec2::new(cx, cy);
        b
    }

    #[test]
    fn mass_and_inertia_derive_from_area() {
        let b = square_body(0.0, 0.0, 10.0, 1.0);
        let mass = 1.0 / b.inv_mass;
        assert!(
            (mass - 400.0).abs() < 1.0,
            "10x10 square at density 1 has mass 400: {mass}"
        );
        let inertia = 1.0 / b.inv_inertia;
        let expected = 400.0 * (20.0_f64 * 20.0 + 20.0 * 20.0) / 12.0;
        assert!(
            (inertia - expected).abs() < 1.0,
            "box inertia m(w²+h²)/12: {inertia} vs {expected}"
        );
    }

    #[test]
    fn head_on_collision_bounces_bodies_apart() {
        let mut bodies = vec![
            square_body(0.0, 0.0, 10.0, 1.0),
            square_body(30.0, 0.0, 10.0, 1.0),
        ];
        if let Some(b) = bodies.first_mut() {
            b.vel = Vec2::new(100.0, 0.0);
            b.restitution = 1.0;
        }
        if let Some(b) = bodies.get_mut(1) {
            b.vel = Vec2::new(-100.0, 0.0);
            b.restitution = 1.0;
        }
        for _ in 0..30 {
            step_rigid(&mut bodies, Vec2::ZERO, 1.0 / 60.0, 8);
        }
        let v0 = bodies.first().map(|b| b.vel.x).unwrap_or(0.0);
        let v1 = bodies.get(1).map(|b| b.vel.x).unwrap_or(0.0);
        assert!(
            v0 < 50.0 && v1 > -50.0,
            "equal masses exchange velocities: v0 {v0}, v1 {v1}"
        );
    }

    #[test]
    fn box_settles_on_static_floor() {
        let mut bodies = vec![
            Body::static_poly(vec![
                Vec2::new(-200.0, 60.0),
                Vec2::new(200.0, 60.0),
                Vec2::new(200.0, 80.0),
                Vec2::new(-200.0, 80.0),
            ]),
            square_body(0.0, -100.0, 15.0, 1.0),
        ];
        if let Some(b) = bodies.get_mut(1) {
            b.friction = 0.6;
        }
        for _ in 0..300 {
            step_rigid(&mut bodies, Vec2::new(0.0, 400.0), 1.0 / 60.0, 8);
        }
        let b = bodies
            .get(1)
            .cloned()
            .unwrap_or_else(|| Body::static_poly(Vec::new()));
        let floor_top = 60.0;
        let penetration = b.pos.y + 15.0 - floor_top;
        assert!(
            penetration.abs() < 2.0,
            "box rests on the floor without sinking: {penetration}"
        );
        assert!(b.vel.len() < 5.0, "settled velocity small: {}", b.vel.len());
        assert!(b.ang_vel.abs() < 0.1, "settled angular velocity small");
    }

    #[test]
    fn off_center_impact_spins_the_body() {
        // A box landing corner-first on a floor edge: angular response.
        let mut bodies = vec![
            Body::static_poly(vec![
                Vec2::new(-200.0, 40.0),
                Vec2::new(200.0, 40.0),
                Vec2::new(200.0, 80.0),
                Vec2::new(-200.0, 80.0),
            ]),
            square_body(-150.0, -80.0, 15.0, 1.0),
        ];
        if let Some(b) = bodies.get_mut(1) {
            b.restitution = 0.0;
            b.angle = 0.35; // corner-first impact
        }
        let mut saw_spin = 0.0_f64;
        for _ in 0..200 {
            step_rigid(&mut bodies, Vec2::new(0.0, 400.0), 1.0 / 60.0, 8);
            saw_spin = saw_spin.max(bodies.get(1).map(|b| b.ang_vel.abs()).unwrap_or(0.0));
        }
        assert!(
            saw_spin > 1e-3,
            "off-center contact must produce rotation: saw {saw_spin}"
        );
    }

    #[test]
    fn static_bodies_never_move() {
        let mut bodies = vec![
            Body::static_poly(vec![
                Vec2::new(-50.0, -50.0),
                Vec2::new(50.0, -50.0),
                Vec2::new(50.0, 50.0),
                Vec2::new(-50.0, 50.0),
            ]),
            square_body(90.0, 0.0, 20.0, 5.0),
        ];
        if let Some(b) = bodies.get_mut(1) {
            b.vel = Vec2::new(-200.0, 0.0);
        }
        for _ in 0..60 {
            step_rigid(&mut bodies, Vec2::new(0.0, 300.0), 1.0 / 60.0, 8);
        }
        let first = bodies
            .first()
            .cloned()
            .unwrap_or_else(|| Body::static_poly(Vec::new()));
        assert_eq!(first.pos, Vec2::ZERO, "static body unmoved");
        assert_eq!(first.angle, 0.0, "static body unrotated");
    }

    #[test]
    fn rigid_step_is_deterministic() {
        let run = || {
            let mut bodies = vec![
                Body::static_poly(vec![
                    Vec2::new(-200.0, 60.0),
                    Vec2::new(200.0, 60.0),
                    Vec2::new(200.0, 80.0),
                    Vec2::new(-200.0, 80.0),
                ]),
                square_body(-40.0, -120.0, 15.0, 1.0),
                square_body(40.0, -160.0, 15.0, 1.0),
            ];
            for _ in 0..180 {
                step_rigid(&mut bodies, Vec2::new(0.0, 400.0), 1.0 / 60.0, 6);
            }
            bodies
                .iter()
                .map(|b| (b.pos.x, b.pos.y, b.angle))
                .collect::<Vec<_>>()
        };
        assert_eq!(run(), run(), "rigid sim byte-identical across runs");
    }
    #[test]
    fn sat_direct_overlapping_squares() {
        use super::*;
        let a = vec![
            Vec2::new(-5.0, -10.0),
            Vec2::new(15.0, -10.0),
            Vec2::new(15.0, 10.0),
            Vec2::new(-5.0, 10.0),
        ];
        let b = vec![
            Vec2::new(13.0, -10.0),
            Vec2::new(33.0, -10.0),
            Vec2::new(33.0, 10.0),
            Vec2::new(13.0, 10.0),
        ];
        let c = sat_contact(&a, &b);
        assert!(c.is_some(), "overlap must be detected");
        let c = c.unwrap_or(Contact {
            normal: Vec2::ZERO,
            depth: 0.0,
            point: Vec2::ZERO,
        });
        assert!(c.depth > 0.0 && c.depth < 10.0, "depth {}", c.depth);
        assert!(c.normal.x.abs() > 0.5, "x normal wins, got {:?}", c.normal);
    }
}

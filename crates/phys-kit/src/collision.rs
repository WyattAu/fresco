//! Narrow-phase collision primitives for the phys-kit world.
//!
//! Circle-circle overlap and resolution, and particle-vs-axis-
//! aligned-box containment with push-out. All resolutions are
//! positional (no impulse solver): fitting for verlet integration
//! where velocity is implicit in position history.

use crate::Particle;
use path_kit::Vec2;

/// A circle collider.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Circle {
    /// Center.
    pub center: Vec2,
    /// Radius.
    pub radius: f64,
}

impl Circle {
    /// New circle.
    #[must_use]
    pub fn new(center: Vec2, radius: f64) -> Self {
        Circle {
            center,
            radius: radius.max(0.0),
        }
    }

    /// Overlap depth of two circles (positive when intersecting).
    #[must_use]
    pub fn overlap_depth(a: Circle, b: Circle) -> f64 {
        let dist = a.center.distance(b.center);
        a.radius + b.radius - dist
    }

    /// Resolve overlap by pushing both circles apart along the
    /// center line, weighted by inverse mass (0 = immovable).
    /// Returns `true` when a correction was applied.
    pub fn resolve_circle_circle(a: &mut Particle, ra: f64, b: &mut Particle, rb: f64) -> bool {
        let depth = Circle::overlap_depth(Circle::new(a.pos, ra), Circle::new(b.pos, rb));
        if depth <= 0.0 {
            return false;
        }
        let delta = Vec2::new(b.pos.x - a.pos.x, b.pos.y - a.pos.y);
        let dist = delta.len();
        let dir = if dist < 1e-12 {
            Vec2::new(0.0, -1.0) // coincident centers: push up
        } else {
            Vec2::new(delta.x / dist, delta.y / dist)
        };
        let w_sum = a.inv_mass + b.inv_mass;
        if w_sum <= 0.0 {
            return false;
        }
        let wa = a.inv_mass / w_sum;
        let wb = b.inv_mass / w_sum;
        a.pos = Vec2::new(a.pos.x - dir.x * depth * wa, a.pos.y - dir.y * depth * wa);
        b.pos = Vec2::new(b.pos.x + dir.x * depth * wb, b.pos.y + dir.y * depth * wb);
        true
    }
}

/// An axis-aligned static box collider.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StaticBox {
    /// Minimum corner.
    pub min: Vec2,
    /// Maximum corner.
    pub max: Vec2,
}

impl StaticBox {
    /// New box from min/max corners (ordered automatically).
    #[must_use]
    pub fn new(a: Vec2, b: Vec2) -> Self {
        StaticBox {
            min: Vec2::new(a.x.min(b.x), a.y.min(b.y)),
            max: Vec2::new(a.x.max(b.x), a.y.max(b.y)),
        }
    }

    /// Push a particle out of the box along the axis of least
    /// penetration. Returns `true` when the particle was inside and
    /// has been corrected.
    pub fn resolve_particle_box(p: &mut Particle, box_: &StaticBox) -> bool {
        if p.inv_mass <= 0.0 {
            return false;
        }
        let inside = p.pos.x >= box_.min.x
            && p.pos.x <= box_.max.x
            && p.pos.y >= box_.min.y
            && p.pos.y <= box_.max.y;
        if !inside {
            return false;
        }
        // Distances to each face; exit through the nearest.
        let dx_left = p.pos.x - box_.min.x;
        let dx_right = box_.max.x - p.pos.x;
        let dy_top = p.pos.y - box_.min.y;
        let dy_bottom = box_.max.y - p.pos.y;
        const EPS: f64 = 1e-6;
        let min_x = dx_left.min(dx_right);
        let min_y = dy_top.min(dy_bottom);
        let (nx, ny) = if min_x <= min_y {
            if dx_left <= dx_right {
                (box_.min.x - EPS, p.pos.y)
            } else {
                (box_.max.x + EPS, p.pos.y)
            }
        } else if dy_top <= dy_bottom {
            (p.pos.x, box_.min.y - EPS)
        } else {
            (p.pos.x, box_.max.y + EPS)
        };
        p.pos = Vec2::new(nx, ny);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use path_kit::Vec2;

    #[test]
    fn circles_overlap_by_depth() {
        let a = Circle::new(Vec2::new(0.0, 0.0), 5.0);
        let b = Circle::new(Vec2::new(8.0, 0.0), 5.0);
        let d = Circle::overlap_depth(a, b);
        assert!((d - 2.0).abs() < 1e-9, "depth {d}");
        let c = Circle::new(Vec2::new(100.0, 0.0), 5.0);
        assert!(Circle::overlap_depth(a, c) <= 0.0, "disjoint");
    }

    #[test]
    fn resolution_separates_circles() {
        let mut a = Particle::free(Vec2::new(0.0, 0.0), 1.0);
        let mut b = Particle::free(Vec2::new(8.0, 0.0), 1.0);
        Circle::resolve_circle_circle(&mut a, 5.0, &mut b, 5.0);
        let dist = a.pos.distance(b.pos);
        assert!(
            (dist - 10.0).abs() < 1e-6,
            "equal masses split the correction: dist {dist}"
        );
    }

    #[test]
    fn immovable_circle_pushes_only_the_free_one() {
        let mut pinned = Particle::pinned(Vec2::new(0.0, 0.0));
        let mut free = Particle::free(Vec2::new(8.0, 0.0), 1.0);
        Circle::resolve_circle_circle(&mut pinned, 5.0, &mut free, 5.0);
        assert_eq!(pinned.pos, Vec2::new(0.0, 0.0), "pinned unmoved");
        let dist = pinned.pos.distance(free.pos);
        assert!(
            (dist - 10.0).abs() < 1e-6,
            "free circle fully pushed: {dist}"
        );
    }

    #[test]
    fn box_resolution_pushes_out_nearest_face() {
        let bx = StaticBox::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0));
        // Particle deep inside near the top face.
        let mut p = Particle::free(Vec2::new(5.0, 2.0), 1.0);
        StaticBox::resolve_particle_box(&mut p, &bx);
        assert!(
            p.pos.y < 0.0 && p.pos.y > -1e-3,
            "exited through the nearest (top) face with margin: {p:?}"
        );
        assert!(
            !StaticBox::resolve_particle_box(&mut p, &bx),
            "no longer inside"
        );
    }

    #[test]
    fn box_resolution_ignores_outside_particles() {
        let bx = StaticBox::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0));
        let mut p = Particle::free(Vec2::new(50.0, 50.0), 1.0);
        let hit = StaticBox::resolve_particle_box(&mut p, &bx);
        assert!(!hit, "no correction outside");
        assert_eq!(p.pos, Vec2::new(50.0, 50.0));
    }

    #[test]
    fn box_resolution_ignores_pinned_particles() {
        let bx = StaticBox::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0));
        let mut p = Particle::pinned(Vec2::new(5.0, 5.0));
        let hit = StaticBox::resolve_particle_box(&mut p, &bx);
        assert!(!hit, "pinned particles are immovable");
    }

    #[test]
    fn box_orders_min_max() {
        let bx = StaticBox::new(Vec2::new(10.0, 10.0), Vec2::new(0.0, 0.0));
        assert_eq!(bx.min, Vec2::new(0.0, 0.0));
        assert_eq!(bx.max, Vec2::new(10.0, 10.0));
    }
}

//! Cloth: a 2D particle grid with structural and shear constraints.
//!
//! The canonical many-constraint verlet showcase. Built as a plain
//! function over the existing `World` — no special casing — proving
//! the constraint solver holds dozens of simultaneous links.

use crate::{DistanceConstraint, Particle, World};
use path_kit::Vec2;

/// Build a cloth sheet: `cols` x `rows` particles spaced `spacing`
/// apart from a top-left origin, pinned at the two top corners,
/// linked structurally (right/down) and diagonally (shear).
#[must_use]
pub fn build_cloth(cols: u32, rows: u32, spacing: f64, origin: Vec2) -> World {
    let mut w = World::new();
    let idx_of = |x: u32, y: u32| -> usize { (y * cols + x) as usize };
    // Particles.
    for y in 0..rows {
        for x in 0..cols {
            let p = Vec2::new(
                origin.x + f64::from(x) * spacing,
                origin.y + f64::from(y) * spacing,
            );
            let pinned = y == 0 && (x == 0 || x == cols - 1);
            let mut particle = if pinned {
                Particle::pinned(p)
            } else {
                Particle::free(p, 1.0)
            };
            // Slight air drag keeps the sheet from ringing forever.
            particle.damping = 0.02;
            w.add(particle);
        }
    }
    // Constraints.
    let link = |a: usize, b: usize, w_: &mut World| {
        let rest = match (w_.particles.get(a), w_.particles.get(b)) {
            (Some(&p), Some(&q)) => p.pos.distance(q.pos),
            _ => spacing,
        };
        w_.constraints.push(DistanceConstraint {
            a,
            b,
            rest_len: rest,
            stiffness: 0.9,
        });
    };
    for y in 0..rows {
        for x in 0..cols {
            let i = idx_of(x, y);
            if x + 1 < cols {
                link(i, idx_of(x + 1, y), &mut w); // structural right
            }
            if y + 1 < rows {
                link(i, idx_of(x, y + 1), &mut w); // structural down
                if x + 1 < cols {
                    link(i, idx_of(x + 1, y + 1), &mut w); // shear
                }
                if x > 0 {
                    link(i, idx_of(x - 1, y + 1), &mut w); // shear
                }
            }
        }
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;
    use path_kit::Vec2;

    fn cloth_10x8() -> World {
        build_cloth(10, 8, 14.0, Vec2::new(90.0, 20.0))
    }

    #[test]
    fn cloth_holds_under_gravity() {
        let mut w = cloth_10x8();
        for _ in 0..240 {
            w.step(1.0 / 60.0);
        }
        let links = w.constraints.len().max(1) as f64;
        let per_link = w.constraint_error() / links;
        assert!(
            per_link < 0.5,
            "dozens of simultaneous links hold: {per_link} px/link"
        );
    }

    #[test]
    fn pinned_corners_stay_put() {
        let mut w = cloth_10x8();
        let origin = Vec2::new(90.0, 20.0);
        for _ in 0..120 {
            w.step(1.0 / 60.0);
        }
        let tl = w
            .particles
            .first()
            .copied()
            .unwrap_or(Particle::pinned(Vec2::ZERO));
        assert_eq!(tl.pos, origin, "top-left pin unmoved");
        let tr_idx = 9; // cols - 1
        let tr = w
            .particles
            .get(tr_idx)
            .copied()
            .unwrap_or(Particle::pinned(Vec2::ZERO));
        let expected = Vec2::new(origin.x + 9.0 * 14.0, origin.y);
        assert_eq!(tr.pos, expected, "top-right pin unmoved");
    }

    #[test]
    fn cloth_sags_but_does_not_explode() {
        let mut w = cloth_10x8();
        for _ in 0..240 {
            w.step(1.0 / 60.0);
        }
        // Free particles drift downward but stay in a sane envelope:
        // no particle may fall more than 3x the rest height.
        let rest_height = 7.0 * 14.0;
        let max_y = w.particles.iter().map(|p| p.pos.y).fold(0.0_f64, f64::max);
        assert!(
            max_y < 20.0 + rest_height * 3.0,
            "cloth stays in its envelope: max_y {max_y}"
        );
    }

    #[test]
    fn cloth_is_deterministic() {
        let run = || {
            let mut w = cloth_10x8();
            for _ in 0..120 {
                w.step(1.0 / 60.0);
            }
            w.particles
                .iter()
                .map(|p| (p.pos.x, p.pos.y))
                .collect::<Vec<_>>()
        };
        assert_eq!(run(), run(), "cloth sim byte-identical across runs");
    }
}

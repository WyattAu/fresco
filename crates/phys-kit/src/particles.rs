//! Particle systems: deterministic emitters for game-style effects.
//!
//! Sparks, embers, debris — spawn, integrate, fade, die. No RNG:
//! spawn jitter derives from a hash of the spawn index, so every
//! run is byte-identical (the fresco determinism discipline).
//! Rendering is a thin helper painting each live particle as a
//! fading round point through paint-kit.

use path_kit::Vec2;

/// Deterministic pseudo-random in `[0, 1)` from a u64 seed
/// (FNV-1a mix).
#[must_use]
pub fn hash_unit(seed: u64) -> f64 {
    let mut h = seed.wrapping_mul(0x100000001b3);
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51afd7ed558ccd);
    h ^= h >> 33;
    // 32 fractional bits.
    f64::from((h >> 32) as u32) / f64::from(u32::MAX)
}

/// One live effect particle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxParticle {
    /// Current position.
    pub pos: Vec2,
    /// Current velocity.
    pub vel: Vec2,
    /// Seconds lived.
    pub age: f64,
    /// Total lifetime in seconds.
    pub lifetime: f64,
    /// Spawn index (seed for deterministic jitter).
    seed: u64,
}

/// Continuous emitter configuration.
#[derive(Clone, Copy, Debug)]
pub struct Emitter {
    /// Spawn position.
    pub pos: Vec2,
    /// Base initial velocity.
    pub velocity: Vec2,
    /// Velocity jitter magnitude (deterministic per spawn).
    pub spread: f64,
    /// Constant acceleration (gravity).
    pub gravity: Vec2,
    /// Particle lifetime in seconds.
    pub lifetime: f64,
    /// Particles spawned per second.
    pub rate: f64,
}

/// A bounded particle pool with one emitter.
pub struct ParticleSystem {
    particles: Vec<FxParticle>,
    emitter: Emitter,
    capacity: usize,
    spawn_carry: f64,
    spawn_counter: u64,
}

impl ParticleSystem {
    /// New system over `capacity` particles (hard cap).
    #[must_use]
    pub fn new(emitter: Emitter, capacity: usize) -> Self {
        ParticleSystem {
            particles: Vec::new(),
            emitter,
            capacity: capacity.max(1),
            spawn_carry: 0.0,
            spawn_counter: 0,
        }
    }

    /// Number of live particles.
    #[must_use]
    pub fn len(&self) -> usize {
        self.particles.len()
    }

    /// Whether no particles are alive.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.particles.is_empty()
    }

    /// Advance `dt` seconds: spawn per rate, integrate, expire.
    /// Expired slots are compacted (order-preserving).
    pub fn step(&mut self, dt: f64) {
        // Spawn (accumulate fractional spawns; cap at capacity).
        if self.emitter.rate > 0.0 {
            self.spawn_carry += self.emitter.rate * dt;
            while self.spawn_carry >= 1.0 {
                self.spawn_carry -= 1.0;
                if self.particles.len() < self.capacity {
                    self.spawn_one();
                }
            }
        }
        // Integrate.
        for p in &mut self.particles {
            p.vel = Vec2::new(
                p.vel.x + self.emitter.gravity.x * dt,
                p.vel.y + self.emitter.gravity.y * dt,
            );
            p.pos = Vec2::new(p.pos.x + p.vel.x * dt, p.pos.y + p.vel.y * dt);
            p.age += dt;
        }
        // Expire (order-preserving compaction).
        self.particles.retain(|p| p.age < p.lifetime);
    }

    fn spawn_one(&mut self) {
        let seed = self.spawn_counter;
        self.spawn_counter = self.spawn_counter.wrapping_add(1);
        // Deterministic jitter in [-0.5, 0.5) per axis from the seed.
        let jx = (hash_unit(seed.wrapping_mul(0x9E3779B97F4A7C15)) - 0.5) * self.emitter.spread;
        let jy = (hash_unit(seed.wrapping_mul(0xBF58476D1CE4E5B9)) - 0.5) * self.emitter.spread;
        self.particles.push(FxParticle {
            pos: self.emitter.pos,
            vel: Vec2::new(self.emitter.velocity.x + jx, self.emitter.velocity.y + jy),
            age: 0.0,
            lifetime: self.emitter.lifetime,
            seed,
        });
    }

    /// Iterate live particles with their fade factor in `[0, 1]`
    /// (1 fresh, 0 at the end of life).
    pub fn iter_faded(&self) -> impl Iterator<Item = (Vec2, f64)> + '_ {
        self.particles.iter().map(|p| {
            let fade = 1.0 - (p.age / p.lifetime.max(1e-9));
            (p.pos, fade.clamp(0.0, 1.0))
        })
    }

    /// Interpolate the particle color ramp by fade factor.
    /// Public so render layers (dev-dependency paint-kit) can build
    /// paints from the same ramp the simulation defines.
    #[must_use]
    pub fn lerp_color(fade: f64, color_start: [u8; 4], color_end: [u8; 4]) -> [u8; 4] {
        let ch = |a: u8, b: u8| -> u8 {
            (f64::from(a) * fade + f64::from(b) * (1.0 - fade)).round() as u8
        };
        [
            ch(color_start[0], color_end[0]),
            ch(color_start[1], color_end[1]),
            ch(color_start[2], color_end[2]),
            ch(color_start[3], color_end[3]),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paint_kit::{Paint, Painter, Rect, Solid, Surface};
    use path_kit::Vec2;

    fn spark_emitter() -> Emitter {
        Emitter {
            pos: Vec2::new(100.0, 100.0),
            velocity: Vec2::new(0.0, -80.0),
            spread: 40.0,
            gravity: Vec2::new(0.0, 300.0),
            lifetime: 1.0,
            rate: 60.0,
        }
    }

    #[test]
    fn spawns_up_to_rate_and_cap() {
        let mut ps = ParticleSystem::new(spark_emitter(), 10);
        ps.step(1.0 / 60.0); // rate 60/s -> 1 spawn per step
        assert_eq!(ps.len(), 1);
        // A full second: capped at 10.
        for _ in 0..90 {
            ps.step(1.0 / 60.0);
        }
        assert!(ps.len() <= 10, "capacity is the hard cap: {}", ps.len());
        assert!(ps.len() >= 8, "spawns accumulate near the rate");
    }

    #[test]
    fn particles_expire_after_lifetime() {
        let mut ps = ParticleSystem::new(spark_emitter(), 100);
        for _ in 0..60 {
            ps.step(1.0 / 60.0);
        }
        assert!(!ps.is_empty());
        // Stop spawning (zero rate) and let everything die.
        ps.emitter.rate = 0.0;
        for _ in 0..70 {
            ps.step(1.0 / 60.0);
        }
        assert!(ps.is_empty(), "all particles expired");
    }

    #[test]
    fn gravity_pulls_particles_down() {
        let mut ps = ParticleSystem::new(spark_emitter(), 10);
        ps.step(1.0 / 60.0);
        let start_y = ps.iter_faded().next().map(|(p, _)| p.y).unwrap_or(0.0);
        ps.emitter.rate = 0.0;
        // The spark's initial velocity is upward (-Y); gravity wins
        // after roughly half a second. 50 steps lets the trajectory
        // pass its apex and land below the start.
        for _ in 0..50 {
            ps.step(1.0 / 60.0);
        }
        let end_y = ps.iter_faded().next().map(|(p, _)| p.y).unwrap_or(0.0);
        assert!(end_y > start_y, "gravity pulls +Y: {start_y} -> {end_y}");
    }

    #[test]
    fn deterministic_across_runs() {
        let run = || {
            let mut ps = ParticleSystem::new(spark_emitter(), 50);
            for _ in 0..90 {
                ps.step(1.0 / 60.0);
            }
            ps.iter_faded()
                .map(|(p, f)| {
                    (
                        (p.x * 1000.0) as i64,
                        (p.y * 1000.0) as i64,
                        (f * 1000.0) as i64,
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(run(), run(), "hash-jittered spawns byte-identical");
    }

    #[test]
    fn render_marks_fading_pixels() {
        let mut ps = ParticleSystem::new(spark_emitter(), 20);
        for _ in 0..30 {
            ps.step(1.0 / 60.0);
        }
        let Some(mut surface) = Surface::new(200, 200) else {
            return;
        };
        let mut painter = Painter::default();
        for (pos, fade) in ps.iter_faded() {
            let paint = Paint::from(Solid::premul(ParticleSystem::lerp_color(
                fade,
                [255, 200, 80, 255],
                [200, 40, 20, 255],
            )));
            let cell = Rect::from_size(Vec2::new(pos.x - 2.0, pos.y - 2.0), Vec2::new(4.0, 4.0));
            painter.fill_round_rect(&mut surface, cell, 2.0, &paint);
        }
        let painted = surface
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|px| px[3] > 0)
            .count();
        assert!(painted > 20, "particles paint visible pixels: {painted}");
    }

    #[test]
    fn fade_ends_at_zero() {
        let mut ps = ParticleSystem::new(spark_emitter(), 5);
        ps.step(1.0 / 60.0);
        let fresh = ps.iter_faded().next().map(|(_, f)| f).unwrap_or(1.0);
        assert!(fresh > 0.9, "fresh particle: fade {fresh}");
        ps.emitter.rate = 0.0;
        // Age to the very end of life.
        for _ in 0..59 {
            ps.step(1.0 / 60.0);
        }
        let dying = ps.iter_faded().next().map(|(_, f)| f).unwrap_or(0.0);
        assert!(dying < 0.1, "dying particle: fade {dying}");
    }
}

//! Physics-to-render integration: simulate a rope with the verlet
//! world, rasterize it through paint-kit's CPU reference painter,
//! and gate the composite on end-to-end determinism.
//!
//! This is the phys -> path -> paint pipeline proof: the same
//! golden-hash discipline the fx gallery uses, applied to a
//! simulated object.

use paint_kit::{Painter, Solid, Surface};
use path_kit::{Poly, Rect, Vec2};
use phys_kit::{Particle, World};

/// Simulate a pinned rope and return its settled particle chain.
fn simulate_rope() -> World {
    let mut w = World::new();
    let anchor = w.add(Particle::pinned(Vec2::new(160.0, 24.0)));
    let mut prev = anchor;
    for k in 1..=12_u32 {
        let idx = w.add(Particle::free(
            Vec2::new(160.0, 24.0 + f64::from(k) * 12.0),
            1.0,
        ));
        w.link(prev, idx, 12.0, 1.0);
        prev = idx;
    }
    for _ in 0..240 {
        w.step(1.0 / 60.0);
    }
    w
}

/// Render a world's distance constraints as thick quads.
fn render_rope(w: &World, surface: &mut Surface, painter: &mut Painter) {
    let bg = Paint::from(Solid::premul([16, 18, 24, 255]));
    painter.fill_rect(
        surface,
        Rect::from_points(Vec2::new(0.0, 0.0), Vec2::new(320.0, 240.0)),
        &bg,
    );
    let rope = Paint::from(Solid::premul([230, 220, 120, 255]));
    let r = 3.0;
    let mut quads: Vec<Poly> = Vec::new();
    for c in &w.constraints {
        let (Some(&a), Some(&b)) = (w.particles.get(c.a), w.particles.get(c.b)) else {
            continue;
        };
        let dx = b.pos.x - a.pos.x;
        let dy = b.pos.y - a.pos.y;
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-9 {
            continue;
        }
        // Unit perpendicular.
        let px = -dy / len;
        let py = dx / len;
        quads.push(Poly {
            points: vec![
                Vec2::new(a.pos.x + px * r, a.pos.y + py * r),
                Vec2::new(b.pos.x + px * r, b.pos.y + py * r),
                Vec2::new(b.pos.x - px * r, b.pos.y - py * r),
                Vec2::new(a.pos.x - px * r, a.pos.y - py * r),
            ],
            closed: true,
        });
    }
    let slices: Vec<&[Vec2]> = quads.iter().map(|q| q.points.as_slice()).collect();
    painter.fill_slices(surface, &slices, &rope);
}

fn fnv1a(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in data {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

use paint_kit::Paint;

#[test]
fn rope_sim_render_is_deterministic() {
    let run = || {
        let w = simulate_rope();
        let mut surface = Surface::new(320, 240).unwrap_or_default();
        let mut painter = Painter::default();
        render_rope(&w, &mut surface, &mut painter);
        fnv1a(surface.data())
    };
    let h1 = run();
    let h2 = run();
    assert_eq!(h1, h2, "sim + render must be byte-identical across runs");
}

#[test]
fn rope_render_marks_pixels() {
    let w = simulate_rope();
    let mut surface = Surface::new(320, 240).unwrap_or_default();
    let mut painter = Painter::default();
    render_rope(&w, &mut surface, &mut painter);
    let non_bg = surface
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|px| px[0] != 16 || px[1] != 18 || px[2] != 24)
        .count();
    assert!(
        non_bg > 500,
        "rope must paint a visible share of pixels: {non_bg}"
    );
}

#[test]
fn settled_rope_holds_link_lengths() {
    let w = simulate_rope();
    let links = w.constraints.len().max(1);
    let per_link = w.constraint_error() / links as f64;
    assert!(
        per_link < 0.1,
        "settled rope links hold: {per_link} px/link average error"
    );
}

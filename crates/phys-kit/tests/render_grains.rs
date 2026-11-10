//! Many-body physics-to-render integration: grains settle in a
//! container, render through paint-kit, gate on end-to-end
//! determinism — the particle counterpart to `render_rope`.

use paint_kit::{Painter, Solid, Surface};
use path_kit::{Rect, Vec2};
use phys_kit::collision::StaticBox;
use phys_kit::{Particle, World};

/// A 10-wide container: walls + floor.
fn build_world() -> World {
    let mut w = World::new();
    w.particle_radius = 5.0;
    w.static_boxes = vec![
        StaticBox::new(Vec2::new(-110.0, 40.0), Vec2::new(110.0, 60.0)), // floor
        StaticBox::new(Vec2::new(-120.0, -200.0), Vec2::new(-110.0, 60.0)), // left wall
        StaticBox::new(Vec2::new(110.0, -200.0), Vec2::new(120.0, 60.0)), // right wall
    ];
    // 4 rows x 6 cols of grains dropped into the container.
    for gy in 0..4_u32 {
        for gx in 0..6_u32 {
            let x = f64::from(gx) * 11.0 - 30.0;
            let y = -f64::from(gy) * 11.0 - 60.0;
            w.add(Particle::free(Vec2::new(x, y), 1.0));
        }
    }
    w
}

/// Render each grain as a filled circle (round-rect with half
/// radius), walls as rects.
fn render_world(w: &World, surface: &mut Surface, painter: &mut Painter) {
    let bg = Paint::from(Solid::premul([14, 16, 20, 255]));
    painter.fill_rect(
        surface,
        Rect::from_points(Vec2::new(0.0, 0.0), Vec2::new(260.0, 200.0)),
        &bg,
    );
    let wall = Paint::from(Solid::premul([90, 96, 110, 255]));
    for bx in &w.static_boxes {
        painter.fill_rect(
            surface,
            Rect::from_points(
                Vec2::new(bx.min.x + 110.0, bx.min.y + 150.0),
                Vec2::new(bx.max.x + 110.0, bx.max.y + 150.0),
            ),
            &wall,
        );
    }
    let grain = Paint::from(Solid::premul([240, 200, 90, 255]));
    let r = w.particle_radius;
    for p in &w.particles {
        let cx = p.pos.x + 110.0;
        let cy = p.pos.y + 150.0;
        let cell = Rect::from_points(Vec2::new(cx - r, cy - r), Vec2::new(cx + r, cy + r));
        painter.fill_round_rect(surface, cell, r, &grain);
    }
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
fn grains_settle_and_render_deterministically() {
    let run = || {
        let mut w = build_world();
        for _ in 0..240 {
            w.step(1.0 / 60.0);
        }
        let mut surface = Surface::new(260, 200).unwrap_or_default();
        let mut painter = Painter::default();
        render_world(&w, &mut surface, &mut painter);
        fnv1a(surface.data())
    };
    let h1 = run();
    let h2 = run();
    assert_eq!(h1, h2, "grain sim + render byte-identical across runs");
}

#[test]
fn settled_grains_rest_inside_container() {
    let mut w = build_world();
    for step in 0..240 {
        w.step(1.0 / 60.0);
        let max_y = w.particles.iter().map(|p| p.pos.y).fold(0.0_f64, f64::max);
        assert!(max_y <= 41.0, "escape at step {step}: max_y {max_y}");
    }
    // No grain below the floor top (y > 40) or outside the walls.
    let inside = w
        .particles
        .iter()
        .all(|p| p.pos.y < 40.0 && p.pos.x > -110.0 && p.pos.x < 110.0);
    let max_y = w.particles.iter().map(|p| p.pos.y).fold(0.0_f64, f64::max);
    let max_x = w.particles.iter().map(|p| p.pos.x).fold(0.0_f64, f64::max);
    let min_x = w.particles.iter().map(|p| p.pos.x).fold(0.0_f64, f64::min);
    assert!(
        inside,
        "all grains contained: max_y {max_y}, x [{min_x}, {max_x}]"
    );
    // Deep interpenetration would indicate the solver broke down.
    let r = w.particle_radius;
    let mut worst = 0.0_f64;
    for i in 0..w.particles.len() {
        for j in (i + 1)..w.particles.len() {
            let (Some(&a), Some(&b)) = (w.particles.get(i), w.particles.get(j)) else {
                continue;
            };
            let d = a.pos.distance(b.pos);
            worst = worst.max(2.0 * r - d);
        }
    }
    assert!(worst < 2.0, "no deep overlap: {worst}");
}

#[test]
fn grain_render_marks_pixels() {
    let mut w = build_world();
    for _ in 0..240 {
        w.step(1.0 / 60.0);
    }
    let mut surface = Surface::new(260, 200).unwrap_or_default();
    let mut painter = Painter::default();
    render_world(&w, &mut surface, &mut painter);
    let non_bg = surface
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|px| px[0] != 14 || px[1] != 16 || px[2] != 20)
        .count();
    assert!(non_bg > 400, "grains and walls must paint: {non_bg}");
}

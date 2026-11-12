//! Smoke showcase: a buoyant smoke plume simulated in a 24-cube
//! grid and rendered by raymarching the density field into BMP
//! frames. Visual verification artifact for the fluid stack.
//!
//! `SMOKE_SHOWCASE_OUT` selects the output directory (default
//! `/tmp/opencode/smoke-showcase`).

use fluid_kit::FluidSolver;
use paint_kit::{Painter, Solid, Surface};
use three_kit::Vec3;

/// World-space box half-extent (the grid maps onto [-1, 1] x
/// [0, 2] x [-1, 1] with Y up).
const BOX_MIN: Vec3 = Vec3::new(-1.0, 0.0, -1.0);
const BOX_MAX: Vec3 = Vec3::new(1.0, 2.0, 1.0);

/// Ray-box entry/exit distances (slab method). Returns
/// (t_enter, t_exit) or None when the ray misses.
fn ray_box(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> Option<(f64, f64)> {
    let inv = |d: f64| -> f64 {
        if d.abs() > 1e-12 {
            1.0 / d
        } else {
            f64::INFINITY * d.signum()
        }
    };
    let tx0 = (min.x - origin.x) * inv(dir.x);
    let tx1 = (max.x - origin.x) * inv(dir.x);
    let ty0 = (min.y - origin.y) * inv(dir.y);
    let ty1 = (max.y - origin.y) * inv(dir.y);
    let tz0 = (min.z - origin.z) * inv(dir.z);
    let tz1 = (max.z - origin.z) * inv(dir.z);
    let tmin = tx0.min(tx1).max(ty0.min(ty1)).max(tz0.min(tz1));
    let tmax = tx0.max(tx1).min(ty0.max(ty1)).min(tz0.max(tz1));
    if tmax < 0.0 || tmin > tmax {
        return None;
    }
    Some((tmin.max(0.0), tmax))
}

/// March the ray through the grid, accumulating smoke opacity and
/// color. Returns the composited pixel color over the background.
#[allow(clippy::too_many_lines)]
fn raymarch(
    solver: &FluidSolver,
    origin: Vec3,
    dir: Vec3,
    steps: usize,
    bg: [u8; 3],
    obs_min: Vec3,
    obs_max: Vec3,
) -> [u8; 3] {
    let Some((t0, t1)) = ray_box(origin, dir, BOX_MIN, BOX_MAX) else {
        return bg;
    };
    // Obstacle entry distance (occludes smoke behind it).
    let t_obs = ray_box(origin, dir, obs_min, obs_max)
        .map(|(e, _)| e)
        .unwrap_or(f64::INFINITY);
    let span = t1 - t0;
    let step_len = span / steps.max(1) as f64;
    let mut alpha = 0.0_f64; // accumulated opacity
    let mut smoke = 0.0_f64; // accumulated density for shading
    let (sx, sy, sz) = solver.density.dims;
    for k in 0..steps {
        let t = t0 + step_len * (k as f64 + 0.5);
        // Stop at the obstacle surface: smoke behind it is hidden.
        if t >= t_obs {
            break;
        }
        let p = Vec3::new(
            origin.x + dir.x * t,
            origin.y + dir.y * t,
            origin.z + dir.z * t,
        );
        // World -> grid coordinates (cell centers at half-steps).
        let gx = (p.x - BOX_MIN.x) / (BOX_MAX.x - BOX_MIN.x) * f64::from(sx as u32 - 1);
        let gy = (p.y - BOX_MIN.y) / (BOX_MAX.y - BOX_MIN.y) * f64::from(sy as u32 - 1);
        let gz = (p.z - BOX_MIN.z) / (BOX_MAX.z - BOX_MIN.z) * f64::from(sz as u32 - 1);
        let d = solver.density.sample(gx, gy, gz);
        if d > 0.0 {
            smoke += d * step_len;
            // Opacity saturates as we accumulate.
            alpha = 1.0 - (1.0 - alpha) * (-d * step_len * 6.0).exp();
        }
        if alpha > 0.99 {
            break;
        }
    }
    // Smoke color: brighter where denser (fake self-shadowing).
    let bright = (smoke * 1.5).clamp(0.0, 1.0);
    let smoke_color = [
        90.0 + 120.0 * bright,
        90.0 + 110.0 * bright,
        95.0 + 105.0 * bright,
    ];
    let mut out = [f64::from(bg[0]), f64::from(bg[1]), f64::from(bg[2])];
    for (slot, c) in out.iter_mut().zip(smoke_color) {
        *slot = *slot * (1.0 - alpha) + c * alpha;
    }
    // Obstacle surface (opaque, flat-shaded by its entry face).
    if let Some((e, _)) = ray_box(origin, dir, obs_min, obs_max)
        && e >= t0
    {
        let hp = Vec3::new(
            origin.x + dir.x * e,
            origin.y + dir.y * e,
            origin.z + dir.z * e,
        );
        // Entry face normal: nearest slab boundary.
        let eps = 1e-6;
        let n = if (hp.x - obs_min.x).abs() < eps {
            Vec3::new(-1.0, 0.0, 0.0)
        } else if (hp.x - obs_max.x).abs() < eps {
            Vec3::new(1.0, 0.0, 0.0)
        } else if (hp.y - obs_min.y).abs() < eps {
            Vec3::new(0.0, -1.0, 0.0)
        } else if (hp.y - obs_max.y).abs() < eps {
            Vec3::new(0.0, 1.0, 0.0)
        } else if (hp.z - obs_min.z).abs() < eps {
            Vec3::new(0.0, 0.0, -1.0)
        } else {
            Vec3::new(0.0, 0.0, 1.0)
        };
        // Lambert against the showcase light.
        let l = Vec3::new(0.45, 0.8, 0.35).normalize().unwrap_or(n);
        let k = 0.35 + 0.65 * n.dot(l).max(0.0);
        let obs = [110.0 * k, 100.0 * k, 90.0 * k];
        for (slot, c) in out.iter_mut().zip(obs) {
            *slot = c * (1.0 - alpha) + *slot * alpha;
        }
    }
    [out[0] as u8, out[1] as u8, out[2] as u8]
}

fn main() {
    let out_dir = std::env::var("SMOKE_SHOWCASE_OUT")
        .unwrap_or_else(|_| "/tmp/opencode/smoke-showcase".to_owned());
    let _ = std::fs::create_dir_all(&out_dir);

    let width = 256_usize;
    let height = 192_usize;
    let mut solver = FluidSolver::new((24, 24, 24));
    solver.pressure_iters = 24;
    solver.buoyancy = 9.0;
    // Floating obstacle slab offset from the emitter: smoke rises,
    // hits it, diverts around the sides.
    for x in 10..=14 {
        for y in 12..=16 {
            for z in 10..=14 {
                solver.set_solid(x, y, z, true);
            }
        }
    }
    // Obstacle AABB in world space (for ray-box compositing).
    let cell = || {
        let span = BOX_MAX.sub(BOX_MIN);
        span.scale(1.0 / 24.0)
    };
    let obs_min = Vec3::new(
        BOX_MIN.x + cell().x * 10.0,
        BOX_MIN.y + cell().y * 12.0,
        BOX_MIN.z + cell().z * 10.0,
    );
    let obs_max = Vec3::new(
        BOX_MIN.x + cell().x * 15.0,
        BOX_MIN.y + cell().y * 17.0,
        BOX_MIN.z + cell().z * 15.0,
    );

    // Camera orbits slightly per frame. Twelve simulated steps per
    // rendered frame let the plume develop (0.4 s per frame).
    let frames = 30_u32;
    for f in 0..frames {
        // Emit smoke at the bottom center each simulated step.
        for _ in 0..12 {
            for dx in 10..14 {
                for dz in 10..14 {
                    solver.add_density(dx, 2, dz, 1.5);
                    solver.add_velocity(dx, 2, dz, [0.0, 3.0, 0.0]);
                }
            }
            solver.step(1.0 / 30.0);
        }

        let angle = f64::from(f) * 0.05;
        let eye = Vec3::new(angle.sin() * 4.0, 1.6, angle.cos() * 4.0);
        let target = Vec3::new(0.0, 0.9, 0.0);
        let fwd = target
            .sub(eye)
            .normalize()
            .unwrap_or(Vec3::new(0.0, 0.0, -1.0));
        let right = fwd
            .cross(Vec3::new(0.0, 1.0, 0.0))
            .normalize()
            .unwrap_or(Vec3::new(1.0, 0.0, 0.0));
        let up = right.cross(fwd);

        let mut surface = Surface::new(width, height).unwrap_or_default();
        {
            let mut painter = Painter::default();
            painter.fill_rect(
                &mut surface,
                path_kit::Rect::from_size(Vec2::ZERO, Vec2::new(256.0, 192.0)),
                &Paint::from(Solid::premul([28, 30, 40, 255])),
            );
        }
        // Raymarch: per-pixel camera rays.
        for row in 0..height {
            for col in 0..width {
                let ndc_x = (f64::from(col as u32) + 0.5) / f64::from(width as u32) * 2.0 - 1.0;
                let ndc_y = 1.0 - (f64::from(row as u32) + 0.5) / f64::from(height as u32) * 2.0;
                let dir = fwd
                    .add(right.scale(ndc_x * 0.8))
                    .add(up.scale(ndc_y * 0.6))
                    .normalize()
                    .unwrap_or(fwd);
                let px = raymarch(&solver, eye, dir, 48, [40, 42, 52], obs_min, obs_max);
                let base = (row * width + col) * 4;
                let data = surface.data_mut();
                if let Some(slot) = data.get_mut(base..base + 4) {
                    // Straight-alpha overwrite via premultiplied bytes.
                    let bytes = [px[0], px[1], px[2], 255];
                    slot.copy_from_slice(&bytes);
                }
            }
        }

        let path = format!("{out_dir}/smoke-{f:02}.bmp");
        let aref = render_conformance::SurfaceRef {
            data: surface.data(),
            width: surface.width(),
            height: surface.height(),
            stride: surface.width() * 4,
        };
        let _ = render_conformance::write_bmp(std::path::Path::new(&path), &aref);
    }
    println!("frames written to {out_dir}");
}

use paint_kit::{Paint, Vec2};

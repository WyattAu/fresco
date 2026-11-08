//! Procedural background patterns (solid-fx Backgrounds family).
//!
//! Grids, dot matrices, and flickering fields: pure paint, no state,
//! fully deterministic in space and time.

use crate::Fx;
use crate::rand::{FxRand, cell_hash};
use paint_kit::{Paint, Rect, Solid, Vec2};

/// Draw one axis-aligned rectangle as a solid fill.
fn fill_rect(fx: &mut Fx, x: f64, y: f64, w: f64, h: f64, color: [u8; 4]) {
    // Straight-alpha input; Solid::new premultiplies.
    fx.painter.fill_rect(
        fx.surface,
        Rect::from_size(Vec2::new(x, y), Vec2::new(w, h)),
        &Paint::from(Solid::new(color)),
    );
}

/// Square grid lines over the whole surface.
///
/// `cell` is the spacing between lines in pixels, `thickness` the line
/// width, `color` the line color (straight alpha, premultiplied
/// internally).
#[derive(Clone, Copy, Debug)]
pub struct GridPattern {
    /// Grid cell spacing in px.
    pub cell: f64,
    /// Line thickness in px.
    pub thickness: f64,
    /// Line color, straight RGBA.
    pub color: [u8; 4],
}

impl GridPattern {
    /// Defaults: 24 px cells, 1 px lines, light gray.
    #[must_use]
    pub fn new() -> Self {
        GridPattern {
            cell: 24.0,
            thickness: 1.0,
            color: [180, 184, 194, 90],
        }
    }

    /// Paint the grid over the full surface.
    pub fn draw(&self, fx: &mut Fx, width: usize, height: usize) {
        let (w, h) = (width as f64, height as f64);
        let cell = self.cell.max(2.0);
        let t = self.thickness.max(0.5);
        let mut x = 0.0;
        while x <= w {
            fill_rect(fx, x, 0.0, t, h, self.color);
            x += cell;
        }
        let mut y = 0.0;
        while y <= h {
            fill_rect(fx, 0.0, y, w, t, self.color);
            y += cell;
        }
    }
}

impl Default for GridPattern {
    fn default() -> Self {
        Self::new()
    }
}

/// Square dot matrix: a small filled square at each grid vertex.
#[derive(Clone, Copy, Debug)]
pub struct DotPattern {
    /// Spacing between dot centers in px.
    pub cell: f64,
    /// Dot size in px.
    pub dot: f64,
    /// Dot color, straight RGBA.
    pub color: [u8; 4],
}

impl DotPattern {
    /// Defaults: 22 px spacing, 2 px dots, mid gray.
    #[must_use]
    pub fn new() -> Self {
        DotPattern {
            cell: 22.0,
            dot: 2.0,
            color: [150, 155, 168, 110],
        }
    }

    /// Paint the dot matrix over the full surface.
    pub fn draw(&self, fx: &mut Fx, width: usize, height: usize) {
        let (w, h) = (width as f64, height as f64);
        let cell = self.cell.max(2.0);
        let dot = self.dot.max(1.0);
        let mut x = cell * 0.5;
        while x < w {
            let mut y = cell * 0.5;
            while y < h {
                fill_rect(fx, x - dot * 0.5, y - dot * 0.5, dot, dot, self.color);
                y += cell;
            }
            x += cell;
        }
    }
}

impl Default for DotPattern {
    fn default() -> Self {
        Self::new()
    }
}

/// Flickering grid: every cell's brightness wobbles over time on a
/// per-cell deterministic schedule (solid-fx FlickeringGrid).
#[derive(Clone, Copy, Debug)]
pub struct FlickeringGrid {
    /// Grid cell spacing in px.
    pub cell: f64,
    /// Base cell color, straight RGBA (the flicker modulates alpha).
    pub color: [u8; 4],
    /// Per-cell seed (vary per instance to decorrelate).
    pub seed: u64,
}

impl FlickeringGrid {
    /// Defaults: 18 px cells, dim violet-gray.
    #[must_use]
    pub fn new() -> Self {
        FlickeringGrid {
            cell: 18.0,
            color: [122, 110, 170, 120],
            seed: 0x5EED,
        }
    }

    /// Paint the flickering field over the full surface at time `fx.t`.
    pub fn draw(&self, fx: &mut Fx, width: usize, height: usize) {
        let (w, h) = (width as f64, height as f64);
        let cell = self.cell.max(2.0);
        let mut gx = 0_u64;
        let mut x = 0.0;
        while x < w {
            let mut gy = 0_u64;
            let mut y = 0.0;
            while y < h {
                // Per-cell phase: stable hash, one wobble per ~2 s.
                let h = cell_hash(self.seed, gx, gy);
                let phase = (h % 1000) as f64 / 1000.0 * std::f64::consts::TAU;
                let speed = 0.5 + (h >> 40) as f64 / (u64::MAX >> 40) as f64 * 1.5;
                let wave = (fx.t * speed + phase).sin();
                // 0.25..1.0 brightness multiplier.
                let m = 0.625 + wave * 0.375;
                let a = ((f64::from(self.color[3]) * m).round() as u8).max(8);
                fill_rect(
                    fx,
                    x + 1.0,
                    y + 1.0,
                    cell - 2.0,
                    cell - 2.0,
                    [self.color[0], self.color[1], self.color[2], a],
                );
                y += cell;
                gy += 1;
            }
            x += cell;
            gx += 1;
        }
    }
}

impl Default for FlickeringGrid {
    fn default() -> Self {
        Self::new()
    }
}

/// Deterministic star field: `count` dots at hashed positions with
/// hashed sizes and phases (the deterministic core of solid-fx
/// Sparkles / Particles backgrounds).
#[derive(Clone, Copy, Debug)]
pub struct StarField {
    /// Dot count.
    pub count: usize,
    /// Per-instance seed.
    pub seed: u64,
    /// Dot color, straight RGBA.
    pub color: [u8; 4],
}

impl StarField {
    /// Defaults: 80 white stars.
    #[must_use]
    pub fn new() -> Self {
        StarField {
            count: 80,
            seed: 0xABCD,
            color: [235, 238, 245, 220],
        }
    }

    /// Paint the field over the full surface at time `fx.t`; each
    /// star twinkles on its own phase.
    pub fn draw(&self, fx: &mut Fx, width: usize, height: usize) {
        let (w, h) = (width as f64, height as f64);
        let mut rand = FxRand::new(self.seed);
        for _ in 0..self.count {
            let x = rand.next_range(0.0, w);
            let y = rand.next_range(0.0, h);
            let size = rand.next_range(1.0, 3.0);
            let phase = rand.next_range(0.0, std::f64::consts::TAU);
            let tw = (fx.t * 2.0 + phase).sin();
            let m = 0.5 + tw * 0.5;
            let a = ((f64::from(self.color[3]) * m).round() as u8).max(4);
            fill_rect(
                fx,
                x - size * 0.5,
                y - size * 0.5,
                size,
                size,
                [self.color[0], self.color[1], self.color[2], a],
            );
        }
    }
}

impl Default for StarField {
    fn default() -> Self {
        Self::new()
    }
}

/// Layered sine waves along the bottom edge (solid-fx Waves family):
/// three translucent wave bands with phase-shifted sine curves.
pub struct Waves {
    /// Wave band colors, straight RGBA (alpha sets band opacity).
    pub colors: [[u8; 4]; 3],
    /// Vertical amplitude in px.
    pub amplitude: f64,
    /// Horizontal wavelength in px.
    pub wavelength: f64,
    /// Animation speed (cycles per second).
    pub speed: f64,
}

impl Waves {
    /// Defaults: three blue-teal bands.
    #[must_use]
    pub fn new() -> Self {
        Waves {
            colors: [[40, 70, 130, 140], [50, 110, 160, 120], [70, 160, 190, 100]],
            amplitude: 24.0,
            wavelength: 180.0,
            speed: 0.8,
        }
    }

    /// Paint the wave bands anchored to the bottom of the surface.
    pub fn draw(&self, fx: &mut Fx, width: usize, height: usize) {
        let (w, h) = (width as f64, height as f64);
        let step = 1.0;
        // Bands stack downward from their base heights.
        let bases = [h * 0.62, h * 0.72, h * 0.82];
        for (band, &base) in bases.iter().enumerate() {
            let phase = fx.t * self.speed + band as f64 * 1.7;
            let amp = self.amplitude * (1.0 - band as f64 * 0.18);
            let color = self
                .colors
                .get(band)
                .copied()
                .unwrap_or_else(|| self.colors.first().copied().unwrap_or([40, 70, 130, 140]));
            // Column strips: from wave crest down to the surface bottom.
            let mut x = 0.0;
            while x < w {
                let wave_y = base + (x / self.wavelength.max(1.0) + phase).sin() * amp;
                fill_rect(fx, x, wave_y, step + 0.5, h - wave_y, color);
                x += step;
            }
        }
    }
}

impl Default for Waves {
    fn default() -> Self {
        Self::new()
    }
}

/// Gradient mesh: soft radial color blobs drifting over the surface
/// (solid-fx Aurora/Gradient family), computed per pixel.
pub struct GradientMesh {
    /// Blob centers as (x, y) fractions of the surface, moved by
    /// `drift * t` along a hashed orbit.
    pub blobs: Vec<(f64, f64)>,
    /// Per-blob drift speed.
    pub drift: f64,
    /// Blob radius as a fraction of the surface diagonal.
    pub radius_frac: f64,
}

impl GradientMesh {
    /// Defaults: three blobs.
    #[must_use]
    pub fn new() -> Self {
        GradientMesh {
            blobs: vec![(0.25, 0.3), (0.7, 0.6), (0.5, 0.85)],
            drift: 0.15,
            radius_frac: 0.45,
        }
    }

    /// Paint per-pixel soft color blend: each pixel takes the color of
    /// the nearest blob weighted by radial falloff.
    pub fn draw(&self, fx: &mut Fx, width: usize, height: usize) {
        let diag = ((width * width + height * height) as f64).sqrt();
        let r = diag * self.radius_frac.max(0.05);
        let t = fx.t * self.drift;
        // Per-blob orbit centers and tint colors.
        let tints: [[u8; 3]; 3] = [[40, 90, 160], [130, 60, 150], [40, 150, 120]];
        let centers: Vec<(f64, f64)> = self
            .blobs
            .iter()
            .enumerate()
            .map(|(i, &(bx, by))| {
                let a = t * (1.0 + i as f64 * 0.35) + i as f64 * 2.1;
                (
                    bx * width as f64 + (a.cos()) * width as f64 * 0.08,
                    by * height as f64 + (a.sin()) * height as f64 * 0.08,
                )
            })
            .collect();
        // Per-pixel blend of falloff-weighted tints over black.
        for y in 0..height {
            for x in 0..width {
                let px = x as f64 + 0.5;
                let py = y as f64 + 0.5;
                let mut cr = 0.0;
                let mut cg = 0.0;
                let mut cb = 0.0;
                for (i, &(cx, cy)) in centers.iter().enumerate() {
                    let d2 = (px - cx) * (px - cx) + (py - cy) * (py - cy);
                    let w = 1.0 / (1.0 + d2 / (r * r * 0.25));
                    let tint = tints.get(i % tints.len()).copied().unwrap_or([40, 90, 160]);
                    cr += f64::from(tint[0]) * w;
                    cg += f64::from(tint[1]) * w;
                    cb += f64::from(tint[2]) * w;
                }
                // Premultiplied black-blend: color is its own alpha.
                let rgba = [
                    cr.min(255.0).round() as u8,
                    cg.min(255.0).round() as u8,
                    cb.min(255.0).round() as u8,
                    255,
                ];
                fx.surface.blend_pixel(x, y, rgba);
            }
        }
    }
}

impl Default for GradientMesh {
    fn default() -> Self {
        Self::new()
    }
}

/// Drifting particle field: hashed base positions + per-particle
/// velocity, positions wrapped at the surface bounds (solid-fx
/// Particles family core).
pub struct Particles {
    /// Particle count.
    pub count: usize,
    /// Per-instance seed.
    pub seed: u64,
    /// Particle color, straight RGBA.
    pub color: [u8; 4],
    /// Particle size in px.
    pub size: f64,
    /// Movement speed multiplier.
    pub speed: f64,
}

impl Particles {
    /// Defaults: 120 white particles at 1.5 px.
    #[must_use]
    pub fn new() -> Self {
        Particles {
            count: 120,
            seed: 0xCAFE,
            color: [235, 238, 245, 200],
            size: 1.5,
            speed: 30.0,
        }
    }

    /// Paint particles at time `fx.t`.
    pub fn draw(&self, fx: &mut Fx, width: usize, height: usize) {
        let (w, h) = (width as f64, height as f64);
        let mut rand = FxRand::new(self.seed);
        for _ in 0..self.count {
            let bx = rand.next_range(0.0, w);
            let by = rand.next_range(0.0, h);
            let dir_a = rand.next_range(0.0, std::f64::consts::TAU);
            let spd = self.speed * rand.next_range(0.5, 1.5);
            // Drift with wrap.
            let x = (bx + dir_a.cos() * fx.t * spd).rem_euclid(w);
            let y = (by + dir_a.sin() * fx.t * spd).rem_euclid(h);
            fill_rect(
                fx,
                x - self.size * 0.5,
                y - self.size * 0.5,
                self.size,
                self.size,
                self.color,
            );
        }
    }
}

impl Default for Particles {
    fn default() -> Self {
        Self::new()
    }
}

/// Static per-pixel noise field (solid-fx noise/static background):
/// deterministic per-pixel gray noise from a cell hash, alpha
/// thresholded.
pub struct NoiseField {
    /// Per-instance seed.
    pub seed: u64,
    /// Noise density in `[0, 1]` (fraction of pixels lit).
    pub density: f64,
    /// Noise pixel color, straight RGBA.
    pub color: [u8; 4],
}

impl NoiseField {
    /// Defaults: 25% density, dim gray.
    #[must_use]
    pub fn new() -> Self {
        NoiseField {
            seed: 0xD00D,
            density: 0.25,
            color: [110, 114, 126, 255],
        }
    }

    /// Paint the noise field over the full surface (static — does not
    /// animate; combine with [`FlickeringGrid`] for animated noise).
    pub fn draw(&self, fx: &mut Fx, width: usize, height: usize) {
        let density = self.density.clamp(0.0, 1.0);
        for y in 0..height {
            for x in 0..width {
                let h = cell_hash(self.seed, x as u64, y as u64);
                let frac = (h >> 11) as f64 / (1_u64 << 53) as f64;
                if frac >= density {
                    continue;
                }
                let lum = 40_u8 + ((h >> 40) % 180) as u8;
                fx.surface.blend_pixel(x, y, [lum, lum, lum, self.color[3]]);
            }
        }
    }
}

impl Default for NoiseField {
    fn default() -> Self {
        Self::new()
    }
}

/// Rotated light beams: translucent strips sweeping at a fixed angle
/// across the surface (solid-fx Beams family).
pub struct Beams {
    /// Beam angle in radians.
    pub angle: f64,
    /// Beam width in px.
    pub width: f64,
    /// Spacing between beam centers in px.
    pub spacing: f64,
    /// Beam color, straight RGBA.
    pub color: [u8; 4],
}

impl Beams {
    /// Defaults: 45-degree beams, 40 px wide, 120 px spacing.
    #[must_use]
    pub fn new() -> Self {
        Beams {
            angle: std::f64::consts::FRAC_PI_4,
            width: 40.0,
            spacing: 120.0,
            color: [120, 140, 220, 90],
        }
    }

    /// Paint rotated beams over the full surface. Beams are computed
    /// by rotating the sample point into beam space (inverse
    /// transform) and testing `((x + y) mod spacing) < width` there.
    pub fn draw(&self, fx: &mut Fx, width: usize, height: usize) {
        let cos_a = self.angle.cos();
        let sin_a = self.angle.sin();
        let diag = (width * width + height * height) as f64;
        let beams = (diag / self.spacing.max(1.0)).ceil() as usize + 2;
        let half = self.width * 0.5;
        // Center the beam field on the surface.
        let cx = width as f64 * 0.5;
        let cy = height as f64 * 0.5;
        for b in 0..beams {
            let offset = (b as f64 - beams as f64 * 0.5) * self.spacing;
            // Beam quad in beam-local coordinates, rotated to device
            // space around the surface center.
            let local = [
                (-diag * 0.6, offset - half),
                (diag * 0.6, offset - half),
                (diag * 0.6, offset + half),
                (-diag * 0.6, offset + half),
            ];
            let quad: Vec<Vec2> = local
                .iter()
                .map(|(lx, ly)| {
                    Vec2::new(cx + lx * cos_a - ly * sin_a, cy + lx * sin_a + ly * cos_a)
                })
                .collect();
            fx.painter.fill_path(
                fx.surface,
                &poly_path(&quad),
                paint_kit::Affine::IDENTITY,
                paint_kit::FillRule::NonZero,
                &Paint::from(Solid::new(self.color)),
            );
        }
    }
}

impl Default for Beams {
    fn default() -> Self {
        Self::new()
    }
}

/// Build a closed polygon path from points.
#[must_use]
pub fn poly_path(points: &[Vec2]) -> path_kit::Path {
    let mut path = path_kit::Path::new();
    for (i, p) in points.iter().enumerate() {
        if i == 0 {
            path.move_to(*p);
        } else {
            path.line_to(*p);
        }
    }
    path.close();
    path
}

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

/// Hexagon lattice outlines: pointy-top hexes tiling the surface,
/// stroked as 2 px segments. CAD-style honeycomb background.
#[derive(Clone, Copy, Debug)]
pub struct HexGrid {
    /// Hex circumradius in px.
    pub size: f64,
    /// Stroke color, straight RGBA.
    pub color: [u8; 4],
}

impl HexGrid {
    /// Defaults: 18 px hexes, soft gray strokes.
    #[must_use]
    pub fn new() -> Self {
        HexGrid {
            size: 18.0,
            color: [150, 158, 170, 70],
        }
    }

    /// Paint the hex lattice over the full surface.
    pub fn draw(&self, fx: &mut Fx, width: usize, height: usize) {
        let size = self.size.max(6.0);
        let w = size * f64::sqrt(3.0); // column width
        let vstep = size * 1.5; // row vertical step
        let stroke = |fx: &mut Fx, x0: f64, y0: f64, x1: f64, y1: f64| {
            // Axis-aligned or diagonal segments as small filled
            // squares along the line (2 px strokes).
            let steps = ((x1 - x0).abs().max((y1 - y0).abs()) / 1.5).ceil().max(1.0) as i32;
            for k in 0..=steps {
                let f = f64::from(k) / f64::from(steps);
                fill_rect(
                    fx,
                    x0 + (x1 - x0) * f - 1.0,
                    y0 + (y1 - y0) * f - 1.0,
                    2.0,
                    2.0,
                    self.color,
                );
            }
        };
        let mut row = 0;
        let mut cy = -size;
        while cy < height as f64 + size {
            let offset = if row % 2 == 0 { 0.0 } else { w * 0.5 };
            let mut cx = -w * 0.5 - offset;
            while cx < width as f64 + w {
                // Six hex corners (pointy-top).
                let mut corners = [(0.0, 0.0); 6];
                for (k, c) in corners.iter_mut().enumerate() {
                    let a = std::f64::consts::FRAC_PI_6 + std::f64::consts::TAU * k as f64 / 6.0;
                    *c = (cx + size * a.cos(), cy + size * a.sin());
                }
                for k in 0..6 {
                    let (Some((x0, y0)), Some((x1, y1))) =
                        (corners.get(k).copied(), corners.get((k + 1) % 6).copied())
                    else {
                        continue;
                    };
                    stroke(fx, x0, y0, x1, y1);
                }
                cx += w;
            }
            cy += vstep;
            row += 1;
        }
    }
}

impl Default for HexGrid {
    fn default() -> Self {
        Self::new()
    }
}

/// Circuit-board traces: seeded orthogonal paths with right-angle
/// turns and solder-dot endpoints. Static per seed.
#[derive(Clone, Copy, Debug)]
pub struct CircuitBoard {
    /// Trace count.
    pub traces: usize,
    /// Per-instance seed.
    pub seed: u64,
    /// Trace color, straight RGBA.
    pub color: [u8; 4],
}

impl CircuitBoard {
    /// Defaults: 18 traces, dim green.
    #[must_use]
    pub fn new() -> Self {
        CircuitBoard {
            traces: 18,
            seed: 0xC1BC,
            color: [70, 180, 110, 120],
        }
    }

    /// Paint the traces over the full surface.
    pub fn draw(&self, fx: &mut Fx, width: usize, height: usize) {
        let mut rand = FxRand::new(self.seed);
        for _ in 0..self.traces {
            let mut x = rand.next_range(0.0, width as f64);
            let mut y = rand.next_range(0.0, height as f64);
            let mut horizontal = rand.next_f64() > 0.5;
            let segments = 3 + rand.next_index(3);
            for _ in 0..segments {
                let len = rand.next_range(20.0, 90.0);
                let dir = if rand.next_f64() > 0.5 { 1.0 } else { -1.0 };
                let (nx, ny) = if horizontal {
                    (x + len * dir, y)
                } else {
                    (x, y + len * dir)
                };
                // Trace as a 2 px stroke.
                let steps = ((nx - x).abs().max((ny - y).abs()) / 2.0).ceil().max(1.0) as i32;
                for k in 0..=steps {
                    let f = f64::from(k) / f64::from(steps);
                    fill_rect(
                        fx,
                        x + (nx - x) * f - 1.0,
                        y + (ny - y) * f - 1.0,
                        2.0,
                        2.0,
                        self.color,
                    );
                }
                x = nx.clamp(0.0, width as f64);
                y = ny.clamp(0.0, height as f64);
                horizontal = !horizontal;
            }
            // Solder dot at the path end.
            fill_rect(fx, x - 3.0, y - 3.0, 6.0, 6.0, self.color);
        }
    }
}

impl Default for CircuitBoard {
    fn default() -> Self {
        Self::new()
    }
}

/// Falling code-rain columns: bright heads with fading trails
/// cascading down, advancing over time. Animated.
#[derive(Clone, Copy, Debug)]
pub struct CodeRain {
    /// Column spacing in px.
    pub cell: f64,
    /// Fall speed in px/s.
    pub speed: f64,
    /// Head color, straight RGBA.
    pub color: [u8; 4],
}

impl CodeRain {
    /// Defaults: 28 px columns, 140 px/s, bright green.
    #[must_use]
    pub fn new() -> Self {
        CodeRain {
            cell: 28.0,
            speed: 140.0,
            color: [120, 240, 150, 230],
        }
    }

    /// Paint the rain over the full surface.
    pub fn draw(&self, fx: &mut Fx, width: usize, height: usize) {
        let cell = self.cell.max(8.0);
        let h = height as f64;
        let mut col = 0;
        let mut x = cell * 0.5;
        while x < width as f64 {
            // Per-column phase from a stable hash: desynchronizes columns.
            let phase = (cell_hash(7, x as u64, col as u64) % 1000) as f64 / 1000.0 * h;
            let head = (fx.t * self.speed + phase) % (h + 120.0);
            // Trail: 4 segments fading behind the head.
            for k in 0..4 {
                let seg_y = head - f64::from(k) * 24.0;
                if seg_y < 0.0 || seg_y > h {
                    continue;
                }
                let fade = 1.0 - f64::from(k) * 0.22;
                let c = [
                    (f64::from(self.color[0]) * fade) as u8,
                    (f64::from(self.color[1]) * fade) as u8,
                    (f64::from(self.color[2]) * fade) as u8,
                    self.color[3],
                ];
                fill_rect(fx, x - 2.0, seg_y - 12.0, 4.0, 24.0, c);
            }
            // Bright head.
            fill_rect(
                fx,
                x - 2.0,
                head - 2.0,
                4.0,
                4.0,
                [self.color[0], 255, self.color[2], self.color[3]],
            );
            x += cell;
            col += 1;
        }
    }
}

impl Default for CodeRain {
    fn default() -> Self {
        Self::new()
    }
}

/// Plasma field: layered sine interference colored through a fire
/// ramp, animating over time. Per-4px-block fill (soft pixels).
#[derive(Clone, Copy, Debug)]
pub struct Plasma {
    /// Spatial frequency.
    pub frequency: f64,
    /// Animation speed.
    pub speed: f64,
}

impl Plasma {
    /// Defaults: gentle frequency, slow drift.
    #[must_use]
    pub fn new() -> Self {
        Plasma {
            frequency: 0.02,
            speed: 0.6,
        }
    }

    /// Paint the plasma over the full surface.
    pub fn draw(&self, fx: &mut Fx, width: usize, height: usize) {
        let block = 4.0;
        let t = fx.t * self.speed;
        let bx = (width as f64 / block).ceil() as i32;
        let by = (height as f64 / block).ceil() as i32;
        for j in 0..by {
            for i in 0..bx {
                let x = f64::from(i) * block;
                let y = f64::from(j) * block;
                let v = (x * self.frequency + t).sin()
                    + (y * self.frequency * 1.3 - t * 0.7).sin()
                    + ((x + y) * self.frequency * 0.7 + t * 0.4).sin();
                // v in ~[-3, 3] -> [0, 1].
                let n = (v + 3.0) / 6.0;
                let c = [
                    (80.0 + 120.0 * n) as u8,
                    (40.0 + 90.0 * (1.0 - n)) as u8,
                    (120.0 + 80.0 * n.sin().abs()) as u8,
                    160,
                ];
                fill_rect(fx, x, y, block, block, c);
            }
        }
    }
}

impl Default for Plasma {
    fn default() -> Self {
        Self::new()
    }
}

/// CRT scan lines: horizontal darker stripes that drift slowly
/// downward over time. Animated overlay.
#[derive(Clone, Copy, Debug)]
pub struct ScanLines {
    /// Spacing between stripe centers in px.
    pub spacing: f64,
    /// Stripe darkness (alpha of the overlay stripes).
    pub strength: f64,
    /// Downward drift in px/s.
    pub drift: f64,
}

impl ScanLines {
    /// Defaults: 6 px spacing, subtle, slow drift.
    #[must_use]
    pub fn new() -> Self {
        ScanLines {
            spacing: 6.0,
            strength: 0.18,
            drift: 12.0,
        }
    }

    /// Paint the scan lines over the full surface.
    pub fn draw(&self, fx: &mut Fx, width: usize, height: usize) {
        let spacing = self.spacing.max(3.0);
        let h = height as f64;
        let offset = (fx.t * self.drift) % spacing;
        let mut y = -spacing + offset;
        while y < h {
            let a = (255.0 * self.strength.clamp(0.0, 1.0)).round() as u8;
            fill_rect(fx, 0.0, y, width as f64, spacing * 0.45, [10, 12, 18, a]);
            y += spacing;
        }
    }
}

impl Default for ScanLines {
    fn default() -> Self {
        Self::new()
    }
}

/// Aurora curtain: layered vertical gradient bands swaying with
/// sine curves, alpha-composited. Animated, calm.
#[derive(Clone, Copy, Debug)]
pub struct Aurora {
    /// Band count.
    pub bands: usize,
    /// Sway amplitude in px.
    pub sway: f64,
    /// Animation speed.
    pub speed: f64,
}

impl Aurora {
    /// Defaults: 3 bands, 40 px sway.
    #[must_use]
    pub fn new() -> Self {
        Aurora {
            bands: 3,
            sway: 40.0,
            speed: 0.4,
        }
    }

    /// Paint the aurora over the full surface.
    pub fn draw(&self, fx: &mut Fx, width: usize, height: usize) {
        let t = fx.t * self.speed;
        let palette: [[u8; 4]; 3] = [
            [80, 240, 180, 60],  // mint
            [90, 150, 240, 55],  // blue
            [180, 110, 240, 50], // violet
        ];
        for band in 0..self.bands.max(1) {
            let fband = f64::from(u32::try_from(band).unwrap_or(0));
            let color = match palette.get(band % palette.len()) {
                Some(&c) => c,
                None => continue,
            };
            let base_y = f64::from(u32::try_from(height).unwrap_or(0)) * (0.25 + 0.18 * fband);
            let amp = self.sway * (1.0 + 0.3 * fband);
            let phase = t + fband * 1.7;
            // Column strips with sine-swayed band tops, fading down.
            let step = 8.0;
            let mut x = 0.0;
            while x < width as f64 {
                let wobble =
                    (x * 0.02 + phase).sin() * amp + (x * 0.007 - phase * 0.6).sin() * amp * 0.5;
                let top = base_y + wobble;
                let band_h = f64::from(u32::try_from(height).unwrap_or(0))
                    * 0.16
                    * (1.0 + 0.3 * (x * 0.01 + phase).sin());
                let c = [
                    (f64::from(color[0])
                        * (1.0
                            - 0.3 * fband
                                / f64::from(u32::try_from(self.bands.max(1)).unwrap_or(1))))
                        as u8,
                    color[1],
                    color[2],
                    color[3],
                ];
                fill_rect(fx, x, top, step, band_h, c);
                x += step;
            }
        }
    }
}

impl Default for Aurora {
    fn default() -> Self {
        Self::new()
    }
}

/// Constellation lines: seeded star points connected by thin lines
/// to their nearest neighbors. Static per seed.
#[derive(Clone, Copy, Debug)]
pub struct Constellation {
    /// Star count.
    pub stars: usize,
    /// Per-instance seed.
    pub seed: u64,
    /// Line + star color, straight RGBA.
    pub color: [u8; 4],
}

impl Constellation {
    /// Defaults: 14 stars, soft white.
    #[must_use]
    pub fn new() -> Self {
        Constellation {
            stars: 14,
            seed: 0xC051,
            color: [210, 216, 235, 130],
        }
    }

    /// Paint stars and their connection lines.
    pub fn draw(&self, fx: &mut Fx, width: usize, height: usize) {
        let mut rand = FxRand::new(self.seed);
        let mut pts: Vec<(f64, f64)> = Vec::with_capacity(self.stars);
        for _ in 0..self.stars {
            pts.push((
                rand.next_range(16.0, width as f64 - 16.0),
                rand.next_range(16.0, height as f64 - 16.0),
            ));
        }
        // Lines: each star connects to its nearest 1-2 successors.
        let seg = |fx: &mut Fx, x0: f64, y0: f64, x1: f64, y1: f64| {
            let steps = ((x1 - x0).abs().max((y1 - y0).abs()) / 2.0).ceil().max(1.0) as i32;
            for k in 0..=steps {
                let f = f64::from(k) / f64::from(steps);
                fill_rect(
                    fx,
                    x0 + (x1 - x0) * f - 1.0,
                    y0 + (y1 - y0) * f - 1.0,
                    2.0,
                    2.0,
                    self.color,
                );
            }
        };
        for (i, (x, y)) in pts.iter().enumerate() {
            // Distances to later stars.
            let mut cands: Vec<(usize, f64)> = pts
                .iter()
                .enumerate()
                .skip(i + 1)
                .map(|(j, (px, py))| (j, (px - x).hypot(py - y)))
                .collect();
            cands.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            for (j, _) in cands.iter().take(2) {
                let Some((px, py)) = pts.get(*j).copied() else {
                    continue;
                };
                seg(fx, *x, *y, px, py);
            }
        }
        // Stars on top: 3 px dots.
        for (x, y) in &pts {
            fill_rect(
                fx,
                x - 1.5,
                y - 1.5,
                3.0,
                3.0,
                [self.color[0], self.color[1], self.color[2], 235],
            );
        }
    }
}

impl Default for Constellation {
    fn default() -> Self {
        Self::new()
    }
}

/// Topographic contour lines: seeded nested wobbly rings suggesting
/// elevation maps. Static per seed.
#[derive(Clone, Copy, Debug)]
pub struct TopoMap {
    /// Contour system count (independent centers).
    pub systems: usize,
    /// Rings per system.
    pub rings: usize,
    /// Per-instance seed.
    pub seed: u64,
    /// Line color, straight RGBA.
    pub color: [u8; 4],
}

impl TopoMap {
    /// Defaults: 3 systems, 5 rings, dim lines.
    #[must_use]
    pub fn new() -> Self {
        TopoMap {
            systems: 3,
            rings: 5,
            seed: 0x7090,
            color: [140, 150, 165, 80],
        }
    }

    /// Paint contour rings over the full surface.
    pub fn draw(&self, fx: &mut Fx, width: usize, height: usize) {
        let mut rand = FxRand::new(self.seed);
        for _ in 0..self.systems {
            let cx = rand.next_range(40.0, width as f64 - 40.0);
            let cy = rand.next_range(40.0, height as f64 - 40.0);
            let base_r = rand.next_range(18.0, 42.0);
            let wob = rand.next_range(0.1, 0.35);
            let lobes = 3 + rand.next_index(3);
            let phase = rand.next_range(0.0, std::f64::consts::TAU);
            for ring in 0..self.rings {
                let r = base_r + f64::from(u32::try_from(ring).unwrap_or(0)) * 14.0;
                let steps = 48;
                let mut prev: Option<(f64, f64)> = None;
                for k in 0..=steps {
                    let a = std::f64::consts::TAU * f64::from(k) / f64::from(steps);
                    let rr = r * (1.0 + wob * (lobes as f64 * a + phase).sin());
                    let px = cx + rr * a.cos();
                    let py = cy + rr * a.sin();
                    if let Some((x0, y0)) = prev {
                        let steps2 =
                            ((px - x0).abs().max((py - y0).abs()) / 2.0).ceil().max(1.0) as i32;
                        for k2 in 0..=steps2 {
                            let f = f64::from(k2) / f64::from(steps2);
                            fill_rect(
                                fx,
                                x0 + (px - x0) * f - 1.0,
                                y0 + (py - y0) * f - 1.0,
                                2.0,
                                2.0,
                                self.color,
                            );
                        }
                    }
                    prev = Some((px, py));
                }
            }
        }
    }
}

impl Default for TopoMap {
    fn default() -> Self {
        Self::new()
    }
}

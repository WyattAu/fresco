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

//! Charts (plychart family): data-driven plots rendered onto the
//! exact-coverage rasterizer. Geometry conventions ported from
//! plychart: linear value normalization to `[min, max]`, padding, and
//! area fills.

use crate::Fx;
use paint_kit::{Paint, Rect, Solid, Vec2};
use path_kit::Path;

/// Map a value into normalized `[0, 1]` against `[min, max]` with a
/// small pad, ported from plychart's sparkline.
fn normalize(v: f64, min: f64, max: f64) -> f64 {
    let range = (max - min).abs().max(f64::EPSILON);
    ((v - min) / range).clamp(0.0, 1.0)
}

fn min_max(values: &[f64]) -> (f64, f64) {
    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    (min, max)
}

/// Draw a filled polygon (one subpath, auto-closed) as premultiplied
/// solid color.
fn fill_poly(fx: &mut Fx, points: &[Vec2], color: [u8; 4]) {
    if points.len() < 3 {
        return;
    }
    let mut path = Path::new();
    for (i, p) in points.iter().enumerate() {
        if i == 0 {
            path.move_to(*p);
        } else {
            path.line_to(*p);
        }
    }
    path.close();
    fx.painter.fill_path(
        fx.surface,
        &path,
        paint_kit::Affine::IDENTITY,
        paint_kit::FillRule::NonZero,
        &Paint::from(Solid::new(color)),
    );
}

/// Fill one axis-aligned rectangle.
fn fill_rect(fx: &mut Fx, x: f64, y: f64, w: f64, h: f64, color: [u8; 4]) {
    fx.painter.fill_rect(
        fx.surface,
        Rect::from_size(Vec2::new(x, y), Vec2::new(w, h)),
        &Paint::from(Solid::new(color)),
    );
}

/// Sparkline: minimal area chart without axes (plychart sparkline).
/// The polygon under the curve is filled; the line itself is
/// expressed as a thin filled strip along the data points.
pub struct Sparkline {
    /// Data values.
    pub values: Vec<f64>,
    /// Line + fill color, straight RGBA.
    pub color: [u8; 4],
    /// Fill opacity multiplier in `[0, 1]` for the area under the
    /// curve.
    pub fill_alpha: f64,
}

impl Sparkline {
    /// Defaults: steel-blue.
    #[must_use]
    pub fn new(values: Vec<f64>) -> Self {
        Sparkline {
            values,
            color: [90, 150, 220, 255],
            fill_alpha: 0.35,
        }
    }

    /// Draw into `rect` at time `fx.t` (unused; kept for the uniform
    /// draw signature).
    pub fn draw(&self, fx: &mut Fx, rect: Rect, _t: f64) {
        if self.values.len() < 2 {
            return;
        }
        let (min, max) = min_max(&self.values);
        let pad = 2.0;
        let w = (rect.x1 - rect.x0) - pad * 2.0;
        let h = (rect.y1 - rect.y0) - pad * 2.0;
        let n = self.values.len();
        let point = |i: usize| -> Vec2 {
            let x = rect.x0 + pad + (i as f64 / (n - 1) as f64) * w;
            let v = self.values.get(i).copied().unwrap_or(min);
            let y = rect.y0 + pad + h - normalize(v, min, max) * h;
            Vec2::new(x, y)
        };
        let base_y = rect.y0 + pad + h;

        // Area fill under the curve.
        let mut area: Vec<Vec2> = self
            .values
            .iter()
            .enumerate()
            .map(|(i, _)| point(i))
            .collect();
        area.push(Vec2::new(rect.x0 + pad + w, base_y));
        area.push(Vec2::new(rect.x0 + pad, base_y));
        let [r, g, b, a] = self.color;
        let fill = [
            r,
            g,
            b,
            ((f64::from(a)) * self.fill_alpha.clamp(0.0, 1.0)).round() as u8,
        ];
        fill_poly(fx, &area, fill);

        // Line strip: quads along each segment, 2 px thick.
        for i in 0..n.saturating_sub(1) {
            let p0 = point(i);
            let p1 = point(i + 1);
            let dx = p1.x - p0.x;
            let dy = p1.y - p0.y;
            let len = (dx * dx + dy * dy).sqrt().max(1e-9);
            let nx = -dy / len;
            let ny = dx / len;
            let half = 1.0;
            let quad = [
                Vec2::new(p0.x + nx * half, p0.y + ny * half),
                Vec2::new(p1.x + nx * half, p1.y + ny * half),
                Vec2::new(p1.x - nx * half, p1.y - ny * half),
                Vec2::new(p0.x - nx * half, p0.y - ny * half),
            ];
            fill_poly(fx, &quad, self.color);
        }
    }
}

/// Bar chart: one vertical bar per value, normalized against the
/// max, with a baseline.
pub struct BarChart {
    /// Data values.
    pub values: Vec<f64>,
    /// Bar color, straight RGBA.
    pub color: [u8; 4],
    /// Gap between bars as a fraction of bar width, in `[0, 1)`.
    pub gap: f64,
    /// Baseline color, straight RGBA.
    pub baseline: [u8; 4],
}

impl BarChart {
    /// Defaults: teal bars, 15% gaps.
    #[must_use]
    pub fn new(values: Vec<f64>) -> Self {
        BarChart {
            values,
            color: [60, 190, 160, 255],
            gap: 0.15,
            baseline: [90, 95, 105, 255],
        }
    }

    /// Draw into `rect`.
    pub fn draw(&self, fx: &mut Fx, rect: Rect, _t: f64) {
        if self.values.is_empty() {
            return;
        }
        let max = self
            .values
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max)
            .max(0.0);
        let w = rect.x1 - rect.x0;
        let h = rect.y1 - rect.y0;
        let n = self.values.len();
        let slot = w / n as f64;
        let gap = slot * self.gap.clamp(0.0, 0.9);
        let bar_w = (slot - gap).max(1.0);
        for (i, v) in self.values.iter().enumerate() {
            let bh = ((*v / max.max(f64::EPSILON)) * (h - 2.0)).clamp(0.0, h - 2.0);
            let x = rect.x0 + i as f64 * slot + gap * 0.5;
            fill_rect(fx, x, rect.y1 - bh - 1.0, bar_w, bh, self.color);
        }
        // Baseline strip.
        fill_rect(fx, rect.x0, rect.y1 - 1.0, w, 1.0, self.baseline);
    }
}

/// Line chart: polyline of the data series with a frame + axis ticks
/// (minimal plychart line).
pub struct LineChart {
    /// Data values.
    pub values: Vec<f64>,
    /// Line color, straight RGBA.
    pub color: [u8; 4],
    /// Axis color, straight RGBA.
    pub axis: [u8; 4],
}

impl LineChart {
    /// Defaults: amber line on gray axes.
    #[must_use]
    pub fn new(values: Vec<f64>) -> Self {
        LineChart {
            values,
            color: [235, 180, 80, 255],
            axis: [120, 124, 136, 255],
        }
    }

    /// Draw into `rect` with a 1 px frame and axis lines.
    pub fn draw(&self, fx: &mut Fx, rect: Rect, _t: f64) {
        if self.values.len() < 2 {
            return;
        }
        let (min, max) = min_max(&self.values);
        let w = rect.x1 - rect.x0;
        let h = rect.y1 - rect.y0;
        // Frame: 4 thin strips.
        fill_rect(fx, rect.x0, rect.y0, w, 1.0, self.axis);
        fill_rect(fx, rect.x0, rect.y1 - 1.0, w, 1.0, self.axis);
        fill_rect(fx, rect.x0, rect.y0, 1.0, h, self.axis);
        fill_rect(fx, rect.x1 - 1.0, rect.y0, 1.0, h, self.axis);
        // Axis lines at 25% and 75% height.
        fill_rect(fx, rect.x0, rect.y0 + h * 0.25, w, 1.0, self.axis);
        fill_rect(fx, rect.x0, rect.y0 + h * 0.75, w, 1.0, self.axis);

        let n = self.values.len();
        let point = |i: usize| -> Vec2 {
            let x = rect.x0 + 1.0 + (i as f64 / (n - 1) as f64) * (w - 2.0);
            let v = self.values.get(i).copied().unwrap_or(min);
            let t = normalize(v, min, max);
            Vec2::new(x, rect.y0 + 1.0 + (1.0 - t) * (h - 2.0))
        };
        for i in 0..n.saturating_sub(1) {
            let p0 = point(i);
            let p1 = point(i + 1);
            let dx = p1.x - p0.x;
            let dy = p1.y - p0.y;
            let len = (dx * dx + dy * dy).sqrt().max(1e-9);
            let nx = -dy / len;
            let ny = dx / len;
            let half = 1.0;
            let quad = [
                Vec2::new(p0.x + nx * half, p0.y + ny * half),
                Vec2::new(p1.x + nx * half, p1.y + ny * half),
                Vec2::new(p1.x - nx * half, p1.y - ny * half),
                Vec2::new(p0.x - nx * half, p0.y - ny * half),
            ];
            // Poly fill via path-based painter.
            let mut path = Path::new();
            for (j, p) in quad.iter().enumerate() {
                if j == 0 {
                    path.move_to(*p);
                } else {
                    path.line_to(*p);
                }
            }
            path.close();
            fx.painter.fill_path(
                fx.surface,
                &path,
                paint_kit::Affine::IDENTITY,
                paint_kit::FillRule::NonZero,
                &Paint::from(Solid::new(self.color)),
            );
        }
    }
}

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

/// Area chart: filled series with a frame (plychart area).
pub struct AreaChart {
    /// Data series.
    pub series: Vec<f64>,
    /// Fill color, straight RGBA.
    pub color: [u8; 4],
}

impl AreaChart {
    /// Defaults: translucent blue fill.
    #[must_use]
    pub fn new(series: Vec<f64>) -> Self {
        AreaChart {
            series,
            color: [80, 140, 230, 160],
        }
    }

    /// Draw into `rect`.
    pub fn draw(&self, fx: &mut Fx, rect: Rect, _t: f64) {
        if self.series.len() < 2 {
            return;
        }
        let (min, max) = min_max(&self.series);
        let w = rect.x1 - rect.x0;
        let h = rect.y1 - rect.y0;
        let n = self.series.len();
        let pts: Vec<Vec2> = (0..n)
            .map(|i| {
                let v = self.series.get(i).copied().unwrap_or(min);
                Vec2::new(
                    rect.x0 + (i as f64 / (n - 1) as f64) * w,
                    rect.y0 + (1.0 - normalize(v, min, max)) * h,
                )
            })
            .collect();
        let mut poly = pts.clone();
        poly.push(Vec2::new(rect.x1, rect.y1));
        poly.push(Vec2::new(rect.x0, rect.y1));
        fill_poly(fx, &poly, self.color);
    }
}

/// Multiline: several series, one strip per series.
pub struct Multiline {
    /// Series list (all normalized independently).
    pub series: Vec<Vec<f64>>,
    /// Line colors, cycled per series.
    pub colors: Vec<[u8; 4]>,
}

impl Multiline {
    /// Defaults: three distinguishable colors.
    #[must_use]
    pub fn new(series: Vec<Vec<f64>>) -> Self {
        Multiline {
            series,
            colors: vec![
                [90, 150, 220, 255],
                [230, 120, 90, 255],
                [90, 200, 130, 255],
            ],
        }
    }

    /// Draw into `rect`.
    pub fn draw(&self, fx: &mut Fx, rect: Rect, _t: f64) {
        for (si, series) in self.series.iter().enumerate() {
            if series.len() < 2 {
                continue;
            }
            let (min, max) = min_max(series);
            let n = series.len();
            let color = self
                .colors
                .get(si % self.colors.len())
                .copied()
                .unwrap_or([200, 200, 200, 255]);
            for i in 0..n - 1 {
                let v0 = series.get(i).copied().unwrap_or(min);
                let v1 = series.get(i + 1).copied().unwrap_or(min);
                let p0 = Vec2::new(
                    rect.x0 + (i as f64 / (n - 1) as f64) * rect.width(),
                    rect.y0 + (1.0 - normalize(v0, min, max)) * rect.height(),
                );
                let p1 = Vec2::new(
                    rect.x0 + ((i + 1) as f64 / (n - 1) as f64) * rect.width(),
                    rect.y0 + (1.0 - normalize(v1, min, max)) * rect.height(),
                );
                fill_poly(fx, &strip(p0, p1, 1.5), color);
            }
        }
    }
}

/// Histogram: binned value distribution.
pub struct Histogram {
    /// Raw values to bin.
    pub values: Vec<f64>,
    /// Bin count.
    pub bins: usize,
    /// Bar color, straight RGBA.
    pub color: [u8; 4],
}

impl Histogram {
    /// Defaults: 12 bins, teal.
    #[must_use]
    pub fn new(values: Vec<f64>) -> Self {
        Histogram {
            values,
            bins: 12,
            color: [70, 130, 200, 255],
        }
    }

    /// Draw into `rect`.
    pub fn draw(&self, fx: &mut Fx, rect: Rect, _t: f64) {
        if self.values.is_empty() || self.bins == 0 {
            return;
        }
        let (min, max) = min_max(&self.values);
        let span = (max - min).max(f64::EPSILON);
        let mut counts = vec![0_usize; self.bins];
        for v in &self.values {
            let b = (((v - min) / span) * self.bins as f64) as usize;
            if let Some(slot) = counts.get_mut(b.min(self.bins - 1)) {
                *slot += 1;
            }
        }
        let maxc = *counts.iter().max().unwrap_or(&1);
        let w = rect.width();
        let h = rect.height();
        for (i, c) in counts.iter().enumerate() {
            let bh = (*c as f64 / maxc as f64) * (h - 2.0);
            let x = rect.x0 + (i as f64 / self.bins as f64) * w;
            let bw = w / self.bins as f64 - 1.0;
            fill_rect(fx, x, rect.y1 - bh, bw.max(1.0), bh, self.color);
        }
    }
}

/// Stacked bars: each bar is a stack of segment values.
pub struct StackedBar {
    /// Rows of segment values.
    pub rows: Vec<Vec<f64>>,
    /// Segment colors, cycled.
    pub colors: Vec<[u8; 4]>,
}

impl StackedBar {
    /// Defaults: three segment colors.
    #[must_use]
    pub fn new(rows: Vec<Vec<f64>>) -> Self {
        StackedBar {
            rows,
            colors: vec![
                [90, 150, 220, 255],
                [230, 170, 80, 255],
                [90, 200, 130, 255],
            ],
        }
    }

    /// Draw into `rect`.
    pub fn draw(&self, fx: &mut Fx, rect: Rect, _t: f64) {
        if self.rows.is_empty() {
            return;
        }
        let max_total = self
            .rows
            .iter()
            .map(|r| r.iter().sum::<f64>())
            .fold(f64::NEG_INFINITY, f64::max);
        let n = self.rows.len();
        let slot = rect.width() / n as f64;
        let bar_w = slot * 0.7;
        for (i, row) in self.rows.iter().enumerate() {
            let x = rect.x0 + i as f64 * slot + (slot - bar_w) * 0.5;
            let mut y_top = rect.y1;
            for (si, v) in row.iter().enumerate() {
                let color = self
                    .colors
                    .get(si % self.colors.len())
                    .copied()
                    .unwrap_or([150, 150, 150, 255]);
                let seg_h = (v / max_total) * rect.height();
                fill_rect(fx, x, y_top - seg_h, bar_w, seg_h, color);
                y_top -= seg_h;
            }
        }
    }
}

/// Candlestick: OHLC rows (open, high, low, close) - body + wick.
pub struct Candlestick {
    /// (open, high, low, close) per candle.
    pub candles: Vec<(f64, f64, f64, f64)>,
    /// Up-candle color (close >= open).
    pub up: [u8; 4],
    /// Down-candle color.
    pub down: [u8; 4],
}

impl Candlestick {
    /// Defaults: green up, red down.
    #[must_use]
    pub fn new(candles: Vec<(f64, f64, f64, f64)>) -> Self {
        Candlestick {
            candles,
            up: [60, 200, 130, 255],
            down: [220, 80, 80, 255],
        }
    }

    /// Draw into `rect`.
    pub fn draw(&self, fx: &mut Fx, rect: Rect, _t: f64) {
        if self.candles.is_empty() {
            return;
        }
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for &(_, h, l, _) in &self.candles {
            lo = lo.min(l);
            hi = hi.max(h);
        }
        let span = (hi - lo).max(f64::EPSILON);
        let n = self.candles.len();
        let slot = rect.width() / n as f64;
        let body_w = slot * 0.6;
        for (i, &(o, h, l, c)) in self.candles.iter().enumerate() {
            let x_mid = rect.x0 + (i as f64 + 0.5) * slot;
            // Wick.
            let yh = rect.y0 + (1.0 - (h - lo) / span) * rect.height();
            let yl = rect.y0 + (1.0 - (l - lo) / span) * rect.height();
            fill_rect(
                fx,
                x_mid - 0.5,
                yh,
                1.0,
                (yl - yh).max(1.0),
                [180, 184, 194, 255],
            );
            // Body.
            let color = if c >= o { self.up } else { self.down };
            let yo = rect.y0 + (1.0 - (o.max(c) - lo) / span) * rect.height();
            let yc = rect.y0 + (1.0 - (o.min(c) - lo) / span) * rect.height();
            fill_rect(
                fx,
                x_mid - body_w * 0.5,
                yo,
                body_w,
                (yc - yo).max(1.0),
                color,
            );
        }
    }
}

/// Heatmap: value matrix as colored cells (two-stop blue ramp).
pub struct Heatmap {
    /// Value matrix [row][col].
    pub cells: Vec<Vec<f64>>,
    /// Cold color.
    pub cold: [u8; 4],
    /// Hot color.
    pub hot: [u8; 4],
}

impl Heatmap {
    /// Defaults: dark blue to bright cyan.
    #[must_use]
    pub fn new(cells: Vec<Vec<f64>>) -> Self {
        Heatmap {
            cells,
            cold: [18, 24, 44, 255],
            hot: [80, 220, 255, 255],
        }
    }

    /// Draw into `rect`.
    pub fn draw(&self, fx: &mut Fx, rect: Rect, _t: f64) {
        let rows = self.cells.len();
        if rows == 0 {
            return;
        }
        let Some(cols) = self.cells.first().map(Vec::len) else {
            return;
        };
        if cols == 0 {
            return;
        }
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for row in &self.cells {
            for &v in row {
                lo = lo.min(v);
                hi = hi.max(v);
            }
        }
        let span = (hi - lo).max(f64::EPSILON);
        let cw = rect.width() / cols as f64;
        let ch = rect.height() / rows as f64;
        for (r, row) in self.cells.iter().enumerate() {
            for (c, &v) in row.iter().enumerate() {
                let t = ((v - lo) / span).clamp(0.0, 1.0);
                let color = [
                    (f64::from(self.cold[0])
                        + (f64::from(self.hot[0]) - f64::from(self.cold[0])) * t)
                        .round() as u8,
                    (f64::from(self.cold[1])
                        + (f64::from(self.hot[1]) - f64::from(self.cold[1])) * t)
                        .round() as u8,
                    (f64::from(self.cold[2])
                        + (f64::from(self.hot[2]) - f64::from(self.cold[2])) * t)
                        .round() as u8,
                    255,
                ];
                fill_rect(
                    fx,
                    rect.x0 + c as f64 * cw,
                    rect.y0 + r as f64 * ch,
                    cw.max(1.0),
                    ch.max(1.0),
                    color,
                );
            }
        }
    }
}

/// Correlation matrix: symmetric heatmap with values in [-1, 1] mapped
/// through a diverging ramp (red = -1, gray = 0, green = +1).
pub struct Correlation {
    /// Symmetric matrix.
    pub cells: Vec<Vec<f64>>,
}

impl Correlation {
    /// New.
    #[must_use]
    pub fn new(cells: Vec<Vec<f64>>) -> Self {
        Correlation { cells }
    }

    /// Draw into `rect`.
    pub fn draw(&self, fx: &mut Fx, rect: Rect, _t: f64) {
        let rows = self.cells.len();
        if rows == 0 {
            return;
        }
        let Some(cols) = self.cells.first().map(Vec::len) else {
            return;
        };
        if cols == 0 {
            return;
        }
        let cw = rect.width() / cols as f64;
        let ch = rect.height() / rows as f64;
        for (r, row) in self.cells.iter().enumerate() {
            for (c, &v) in row.iter().enumerate() {
                let t = v.clamp(-1.0, 1.0);
                let (color, m) = if t < 0.0 {
                    ([220, 80, 80, 255], -t)
                } else {
                    ([80, 200, 120, 255], t)
                };
                let a = (40.0 + m * 215.0).round() as u8;
                fill_rect(
                    fx,
                    rect.x0 + c as f64 * cw,
                    rect.y0 + r as f64 * ch,
                    cw.max(1.0),
                    ch.max(1.0),
                    [color[0], color[1], color[2], a],
                );
            }
        }
    }
}

/// Treemap: slice-and-dice layout of values into nested rects.
pub struct Treemap {
    /// Values (leaf areas proportional to value).
    pub values: Vec<f64>,
    /// Leaf colors, cycled.
    pub colors: Vec<[u8; 4]>,
}

impl Treemap {
    /// Defaults: four-color cycle.
    #[must_use]
    pub fn new(values: Vec<f64>) -> Self {
        Treemap {
            values,
            colors: vec![
                [70, 110, 190, 255],
                [200, 120, 70, 255],
                [80, 180, 120, 255],
                [180, 80, 150, 255],
            ],
        }
    }

    /// Draw into `rect` (horizontal slice-and-dice).
    pub fn draw(&self, fx: &mut Fx, rect: Rect, _t: f64) {
        if self.values.is_empty() {
            return;
        }
        let total: f64 = self.values.iter().sum();
        if total <= 0.0 {
            return;
        }
        let mut x = rect.x0;
        for (i, v) in self.values.iter().enumerate() {
            let w = (v / total) * rect.width();
            let color = self
                .colors
                .get(i % self.colors.len())
                .copied()
                .unwrap_or([120, 120, 140, 255]);
            fill_rect(fx, x, rect.y0, w.max(1.0), rect.height(), color);
            // 1px gutter to separate leaves.
            fill_rect(fx, x, rect.y0, 1.0, rect.height(), [16, 17, 22, 255]);
            x += w;
        }
    }
}

/// Waterfall: bars floating between cumulative values, colored by
/// delta sign, with connector lines.
pub struct Waterfall {
    /// Step values (positive = gain, negative = loss).
    pub steps: Vec<f64>,
    /// Gain color.
    pub gain: [u8; 4],
    /// Loss color.
    pub loss: [u8; 4],
}

impl Waterfall {
    /// Defaults: green gain, red loss.
    #[must_use]
    pub fn new(steps: Vec<f64>) -> Self {
        Waterfall {
            steps,
            gain: [70, 190, 120, 255],
            loss: [210, 80, 80, 255],
        }
    }

    /// Draw into `rect`.
    pub fn draw(&self, fx: &mut Fx, rect: Rect, _t: f64) {
        if self.steps.is_empty() {
            return;
        }
        // Cumulative levels.
        let mut levels = vec![0.0_f64];
        let mut acc = 0.0_f64;
        for &s in &self.steps {
            acc += s;
            levels.push(acc);
        }
        let mut lo = levels.iter().cloned().fold(f64::INFINITY, f64::min);
        let mut hi = levels.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        for &s in &self.steps {
            lo = lo.min(s);
            hi = hi.max(s);
        }
        lo = lo.min(0.0);
        let span = (hi - lo).max(f64::EPSILON);
        let n = self.steps.len();
        let slot = rect.width() / n as f64;
        let bar_w = slot * 0.6;
        let y_of = |v: f64| rect.y0 + (1.0 - (v - lo) / span) * rect.height();
        for (i, &s) in self.steps.iter().enumerate() {
            let x = rect.x0 + i as f64 * slot + (slot - bar_w) * 0.5;
            let (Some(&y0v), Some(&y1v)) = (levels.get(i), levels.get(i + 1)) else {
                continue;
            };
            let y0 = y_of(y0v);
            let y1 = y_of(y1v);
            let top = y0.min(y1);
            let h = (y1 - y0).abs().max(1.0);
            let color = if s >= 0.0 { self.gain } else { self.loss };
            fill_rect(fx, x, top, bar_w, h, color);
            // Connector to next level.
            if i + 1 < n
                && let Some(&cy) = levels.get(i + 1)
            {
                let cy = y_of(cy);
                fill_rect(fx, x + bar_w, cy, slot - bar_w, 1.0, [120, 124, 136, 160]);
            }
        }
    }
}

/// Uniform strip between two points (line segment with thickness).
#[must_use]
pub fn strip(p0: Vec2, p1: Vec2, half: f64) -> Vec<Vec2> {
    let dx = p1.x - p0.x;
    let dy = p1.y - p0.y;
    let len = (dx * dx + dy * dy).sqrt().max(1e-9);
    let nx = -dy / len;
    let ny = dx / len;
    vec![
        Vec2::new(p0.x + nx * half, p0.y + ny * half),
        Vec2::new(p1.x + nx * half, p1.y + ny * half),
        Vec2::new(p1.x - nx * half, p1.y - ny * half),
        Vec2::new(p0.x - nx * half, p0.y - ny * half),
    ]
}

/// Polar direction unit vector at `angle` (radians, clockwise in
/// Y-down coordinates - the vello convention).
fn polar_dir(angle: f64) -> Vec2 {
    Vec2::new(angle.cos(), angle.sin())
}

/// Filled quad between two radii across a small angular slice.
fn arc_segment(fx: &mut Fx, center: Vec2, r_in: f64, r_out: f64, a0: f64, a1: f64, color: [u8; 4]) {
    let p = |r: f64, a: f64| Vec2::new(center.x + r * a.cos(), center.y + r * a.sin());
    let quad = [p(r_in, a0), p(r_out, a0), p(r_out, a1), p(r_in, a1)];
    fill_poly(fx, &quad, color);
}

/// Gauge: background arc, value arc, and needle, built from polar
/// segments (no rotation transforms - quad points are computed
/// directly in device space).
pub struct Gauge {
    /// Value in `[0, 1]`.
    pub value: f64,
    /// Sweep from `start_angle` to `end_angle` (radians).
    /// Sweep start angle in radians.
    pub start_angle: f64,
    /// Sweep end angle in radians.
    pub end_angle: f64,
    /// Track color (background arc).
    pub track: [u8; 4],
    /// Value arc color.
    pub color: [u8; 4],
    /// Needle color.
    pub needle: [u8; 4],
    /// Segment count for the arc approximation.
    pub segments: usize,
}

impl Gauge {
    /// Defaults: 270-degree sweep, 24 segments.
    #[must_use]
    pub fn new(value: f64) -> Self {
        Gauge {
            value: value.clamp(0.0, 1.0),
            start_angle: std::f64::consts::FRAC_PI_4,
            end_angle: std::f64::consts::TAU - std::f64::consts::FRAC_PI_4,
            track: [60, 64, 76, 255],
            color: [80, 200, 140, 255],
            needle: [240, 244, 250, 255],
            segments: 24,
        }
    }

    /// Draw into `rect` with the arc centered in the cell.
    pub fn draw(&self, fx: &mut Fx, rect: Rect, _t: f64) {
        let cx = (rect.x0 + rect.x1) * 0.5;
        let cy = (rect.y0 + rect.y1) * 0.5;
        let r = (rect.width().min(rect.height()) * 0.5) - 6.0;
        let thickness = 10.0;
        let sweep = self.end_angle - self.start_angle;
        let segs = self.segments.max(4);
        let seg_width = sweep / segs as f64;
        // Background track.
        for i in 0..segs {
            let a = self.start_angle + (i as f64 + 0.5) * seg_width;
            arc_segment(
                fx,
                Vec2::new(cx, cy),
                r - thickness * 0.5,
                r + thickness * 0.5,
                a - seg_width * 0.5,
                a + seg_width * 0.5,
                self.track,
            );
        }
        // Value arc up to fraction.
        let vsegs = ((segs as f64 * self.value).round() as usize).min(segs);
        for i in 0..vsegs {
            let a = self.start_angle + (i as f64 + 0.5) * seg_width;
            arc_segment(
                fx,
                Vec2::new(cx, cy),
                r - thickness * 0.5,
                r + thickness * 0.5,
                a - seg_width * 0.5,
                a + seg_width * 0.5,
                self.color,
            );
        }
        // Needle from center to the value angle.
        let vangle = self.start_angle + sweep * self.value;
        let dir = polar_dir(vangle);
        let tip = Vec2::new(
            cx + dir.x * (r + thickness * 0.5),
            cy + dir.y * (r + thickness * 0.5),
        );
        fill_poly(fx, &strip(Vec2::new(cx, cy), tip, 1.5), self.needle);
        // Hub.
        fill_rect(fx, cx - 3.0, cy - 3.0, 6.0, 6.0, self.needle);
    }
}

/// Pie: filled wedges from the center, one per value.
pub struct Pie {
    /// Values (wedge angular fraction = value / total).
    pub values: Vec<f64>,
    /// Wedge colors, cycled.
    pub colors: Vec<[u8; 4]>,
    /// Start angle in radians.
    pub start_angle: f64,
}

impl Pie {
    /// Defaults: four colors, sweep starts at +X.
    #[must_use]
    pub fn new(values: Vec<f64>) -> Self {
        Pie {
            values,
            colors: vec![
                [80, 130, 210, 255],
                [230, 170, 80, 255],
                [90, 200, 130, 255],
                [200, 90, 150, 255],
            ],
            start_angle: 0.0,
        }
    }

    /// Draw into `rect`.
    pub fn draw(&self, fx: &mut Fx, rect: Rect, _t: f64) {
        if self.values.is_empty() {
            return;
        }
        let total: f64 = self.values.iter().sum();
        if total <= 0.0 {
            return;
        }
        let cx = (rect.x0 + rect.x1) * 0.5;
        let cy = (rect.y0 + rect.y1) * 0.5;
        let r = rect.width().min(rect.height()) * 0.45;
        let segs_per_wedge = 8;
        let mut a = self.start_angle;
        for (i, &v) in self.values.iter().enumerate() {
            let frac = v / total;
            let sweep = frac * std::f64::consts::TAU;
            let color = self
                .colors
                .get(i % self.colors.len())
                .copied()
                .unwrap_or([150, 150, 160, 255]);
            // Wedge polygon: center + arc points across the sweep.
            let mut wedge = vec![Vec2::new(cx, cy)];
            for s in 0..=segs_per_wedge {
                let ang = a + (s as f64 / segs_per_wedge as f64) * sweep;
                wedge.push(Vec2::new(cx + r * ang.cos(), cy + r * ang.sin()));
            }
            fill_poly(fx, &wedge, color);
            a += sweep;
        }
    }
}

/// Radar: polygon web, spokes, and a data polygon.
pub struct Radar {
    /// Values in `[0, 1]` per axis.
    pub values: Vec<f64>,
    /// Web/spoke color.
    pub web: [u8; 4],
    /// Data polygon color (filled).
    pub color: [u8; 4],
}

impl Radar {
    /// Defaults: gray web, translucent teal polygon.
    #[must_use]
    pub fn new(values: Vec<f64>) -> Self {
        Radar {
            values,
            web: [110, 116, 130, 200],
            color: [80, 200, 170, 110],
        }
    }

    /// Draw into `rect`.
    pub fn draw(&self, fx: &mut Fx, rect: Rect, _t: f64) {
        let k = self.values.len();
        if k < 3 {
            return;
        }
        let cx = (rect.x0 + rect.x1) * 0.5;
        let cy = (rect.y0 + rect.y1) * 0.5;
        let r = rect.width().min(rect.height()) * 0.4;
        let angle = |i: usize| -> f64 {
            -std::f64::consts::FRAC_PI_2 + (i as f64 / k as f64) * std::f64::consts::TAU
        };
        let vertex = |i: usize, frac: f64| -> Vec2 {
            let a = angle(i);
            Vec2::new(cx + r * frac * a.cos(), cy + r * frac * a.sin())
        };
        // Web rings.
        for frac in [0.25, 0.5, 0.75, 1.0] {
            let mut ring: Vec<Vec2> = (0..k).map(|i| vertex(i, frac)).collect();
            if let Some(&first) = ring.first() {
                ring.push(first);
            }
            fill_poly(fx, &ring, [self.web[0], self.web[1], self.web[2], 40]);
            // Ring outline via strip edges.
            for i in 0..k {
                let a = vertex(i, frac);
                let b = vertex((i + 1) % k, frac);
                fill_poly(fx, &strip(a, b, 0.5), self.web);
            }
        }
        // Spokes.
        for i in 0..k {
            fill_poly(fx, &strip(Vec2::new(cx, cy), vertex(i, 1.0), 0.5), self.web);
        }
        // Data polygon.
        let mut data: Vec<Vec2> = (0..k)
            .map(|i| {
                let v = self.values.get(i).copied().unwrap_or(0.0).clamp(0.0, 1.0);
                vertex(i, v)
            })
            .collect();
        if let Some(&first) = data.first() {
            data.push(first);
        }
        fill_poly(fx, &data, self.color);
    }
}

/// Scatter: point cloud with per-point markers.
pub struct Scatter {
    /// (x, y) pairs normalized to `[0, 1]`.
    pub points: Vec<(f64, f64)>,
    /// Point color, straight RGBA.
    pub color: [u8; 4],
    /// Marker size in px.
    pub size: f64,
}

impl Scatter {
    /// Defaults: 60 points, 3 px markers.
    #[must_use]
    pub fn new(points: Vec<(f64, f64)>) -> Self {
        Scatter {
            points,
            color: [230, 130, 80, 255],
            size: 3.0,
        }
    }

    /// Draw into `rect`.
    pub fn draw(&self, fx: &mut Fx, rect: Rect, _t: f64) {
        for &(nx, ny) in &self.points {
            let x = rect.x0 + nx.clamp(0.0, 1.0) * rect.width();
            let y = rect.y0 + (1.0 - ny.clamp(0.0, 1.0)) * rect.height();
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

/// Order book: horizontal depth bars around a mid price - bids below
/// (green), asks above (red).
pub struct OrderBook {
    /// Bid sizes, best first.
    pub bids: Vec<f64>,
    /// Ask sizes, best first.
    pub asks: Vec<f64>,
    /// Bid color.
    pub bid: [u8; 4],
    /// Ask color.
    pub ask: [u8; 4],
}

impl OrderBook {
    /// Defaults.
    #[must_use]
    pub fn new(bids: Vec<f64>, asks: Vec<f64>) -> Self {
        OrderBook {
            bids,
            asks,
            bid: [60, 190, 120, 255],
            ask: [210, 80, 90, 255],
        }
    }

    /// Draw into `rect`.
    pub fn draw(&self, fx: &mut Fx, rect: Rect, _t: f64) {
        let max = self
            .bids
            .iter()
            .chain(self.asks.iter())
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max)
            .max(f64::EPSILON);
        let rows = self.bids.len() + self.asks.len();
        if rows == 0 {
            return;
        }
        let row_h = rect.height() / rows as f64;
        let mut y = rect.y0;
        // Asks on top (reverse so best ask is nearest mid).
        for size in self.asks.iter().rev() {
            let w = (size / max) * rect.width();
            fill_rect(fx, rect.x0, y, w, row_h - 1.0, self.ask);
            y += row_h;
        }
        // Mid line.
        fill_rect(fx, rect.x0, y, rect.width(), 1.0, [120, 124, 136, 255]);
        y += 1.0;
        // Bids below.
        for size in self.bids.iter() {
            let w = (size / max) * rect.width();
            fill_rect(fx, rect.x0, y, w, row_h - 1.0, self.bid);
            y += row_h;
        }
    }
}

/// Volume: bars along the bottom portion of the chart.
pub struct Volume {
    /// Per-candle volume.
    pub volumes: Vec<f64>,
    /// Bar color, straight RGBA.
    pub color: [u8; 4],
}

impl Volume {
    /// Defaults: dim steel bars.
    #[must_use]
    pub fn new(volumes: Vec<f64>) -> Self {
        Volume {
            volumes,
            color: [90, 130, 180, 180],
        }
    }

    /// Draw into the bottom `fraction` of `rect`.
    pub fn draw(&self, fx: &mut Fx, rect: Rect, fraction: f64) {
        if self.volumes.is_empty() {
            return;
        }
        let max = self
            .volumes
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max)
            .max(f64::EPSILON);
        let zone_h = rect.height() * fraction.clamp(0.05, 1.0);
        let n = self.volumes.len();
        let slot = rect.width() / n as f64;
        let bar_w = slot * 0.7;
        for (i, &v) in self.volumes.iter().enumerate() {
            let bh = (v / max) * zone_h;
            let x = rect.x0 + i as f64 * slot + (slot - bar_w) * 0.5;
            fill_rect(fx, x, rect.y1 - bh, bar_w, bh, self.color);
        }
    }
}

/// Backtest: price line + trade markers (triangles) at indices.
pub struct Backtest {
    /// Price series.
    pub prices: Vec<f64>,
    /// Trade (index, buy?) pairs.
    pub trades: Vec<(usize, bool)>,
}

impl Backtest {
    /// New.
    #[must_use]
    pub fn new(prices: Vec<f64>, trades: Vec<(usize, bool)>) -> Self {
        Backtest { prices, trades }
    }

    /// Draw into `rect`: line plus up/down triangles at trade points.
    pub fn draw(&self, fx: &mut Fx, rect: Rect, _t: f64) {
        if self.prices.len() < 2 {
            return;
        }
        let (min, max) = min_max(&self.prices);
        let n = self.prices.len();
        let point = |i: usize| -> Vec2 {
            let v = self.prices.get(i).copied().unwrap_or(min);
            Vec2::new(
                rect.x0 + (i as f64 / (n - 1) as f64) * rect.width(),
                rect.y0 + (1.0 - normalize(v, min, max)) * rect.height(),
            )
        };
        for i in 0..n - 1 {
            fill_poly(
                fx,
                &strip(point(i), point(i + 1), 1.0),
                [160, 165, 180, 255],
            );
        }
        for &(idx, buy) in &self.trades {
            if idx >= n {
                continue;
            }
            let p = point(idx);
            let size = 6.0;
            let dir = if buy { -1.0 } else { 1.0 };
            let base_y = p.y + dir * size;
            let tri = vec![
                Vec2::new(p.x, p.y + dir * 2.0),
                Vec2::new(p.x - size * 0.6, base_y),
                Vec2::new(p.x + size * 0.6, base_y),
            ];
            let color = if buy {
                [70, 200, 130, 255]
            } else {
                [220, 90, 90, 255]
            };
            fill_poly(fx, &tri, color);
        }
    }
}

/// Chart grid: regular grid lines for a chart area.
pub struct ChartGrid {
    /// Vertical line count.
    pub vertical: usize,
    /// Horizontal line count.
    pub horizontal: usize,
    /// Grid color, straight RGBA.
    pub color: [u8; 4],
}

impl Default for ChartGrid {
    fn default() -> Self {
        Self::new()
    }
}

impl ChartGrid {
    /// Defaults: 4 vertical, 4 horizontal.
    #[must_use]
    pub fn new() -> Self {
        ChartGrid {
            vertical: 4,
            horizontal: 4,
            color: [60, 64, 76, 200],
        }
    }

    /// Draw into `rect`.
    pub fn draw(&self, fx: &mut Fx, rect: Rect, _t: f64) {
        let w = rect.width();
        let h = rect.height();
        for i in 1..self.vertical.max(1) {
            let x = rect.x0 + (i as f64 / self.vertical as f64) * w;
            fill_rect(fx, x, rect.y0, 1.0, h, self.color);
        }
        for i in 1..self.horizontal.max(1) {
            let y = rect.y0 + (i as f64 / self.horizontal as f64) * h;
            fill_rect(fx, rect.x0, y, w, 1.0, self.color);
        }
    }
}

/// Crosshair: vertical + horizontal hairlines through a data point.
pub struct Crosshair {
    /// Normalized cross point in `[0, 1]`.
    pub at: (f64, f64),
    /// Line color, straight RGBA.
    pub color: [u8; 4],
}

impl Crosshair {
    /// Defaults: dim white crosshair.
    #[must_use]
    pub fn new(at: (f64, f64)) -> Self {
        Crosshair {
            at,
            color: [200, 204, 214, 180],
        }
    }

    /// Draw over `rect`.
    pub fn draw(&self, fx: &mut Fx, rect: Rect, _t: f64) {
        let x = rect.x0 + self.at.0.clamp(0.0, 1.0) * rect.width();
        let y = rect.y0 + (1.0 - self.at.1.clamp(0.0, 1.0)) * rect.height();
        fill_rect(fx, x - 0.5, rect.y0, 1.0, rect.height(), self.color);
        fill_rect(fx, rect.x0, y - 0.5, rect.width(), 1.0, self.color);
    }
}

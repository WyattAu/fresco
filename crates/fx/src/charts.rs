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
                Vec2::new(
                    rect.x0 + (i as f64 / (n - 1) as f64) * w,
                    rect.y0 + (1.0 - normalize(self.series[i], min, max)) * h,
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
                let p0 = Vec2::new(
                    rect.x0 + (i as f64 / (n - 1) as f64) * rect.width(),
                    rect.y0 + (1.0 - normalize(series[i], min, max)) * rect.height(),
                );
                let p1 = Vec2::new(
                    rect.x0 + ((i + 1) as f64 / (n - 1) as f64) * rect.width(),
                    rect.y0 + (1.0 - normalize(series[i + 1], min, max)) * rect.height(),
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
            counts[b.min(self.bins - 1)] += 1;
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
        let cols = self.cells[0].len();
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
        let cols = self.cells[0].len();
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
            let y0 = y_of(levels[i]);
            let y1 = y_of(levels[i + 1]);
            let top = y0.min(y1);
            let h = (y1 - y0).abs().max(1.0);
            let color = if s >= 0.0 { self.gain } else { self.loss };
            fill_rect(fx, x, top, bar_w, h, color);
            // Connector to next level.
            if i + 1 < n {
                let cy = y_of(levels[i + 1]);
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

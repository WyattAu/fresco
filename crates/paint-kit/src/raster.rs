//! Winding-integral scanline rasterization.
//!
//! Given closed polygon outlines in device pixels, produce exact per-pixel
//! coverage rows between y-events. Coverage of a pixel is the integral of
//! the winding function over the pixel area, computed analytically: within
//! a y-band each edge's x is linear, so the per-column contribution is a
//! piecewise-linear trapezoid integral (exact, not sampled).

use path_kit::Vec2;
use std::vec::Vec;

const EPS: f64 = 1e-9;

/// One polygon edge prepared for scanline rasterization: vertical span
/// `[ya, yb]`, x positions at both ends, and winding sign (+1 downward in
/// y-down coordinates, -1 upward).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Edge {
    /// Y at the edge start.
    pub ya: f64,
    /// Y at the edge end.
    pub yb: f64,
    /// X at the edge start (at `ya`).
    pub xa: f64,
    /// X at the edge end (at `yb`).
    pub xb: f64,
    /// Winding sign: +1 downward, -1 upward.
    pub sign: f64,
}

impl Edge {
    /// X position at `y`, clamped into the edge's span.
    #[must_use]
    pub fn x_at(&self, y: f64) -> f64 {
        let (lo, hi) = if self.ya <= self.yb {
            (self.ya, self.yb)
        } else {
            (self.yb, self.ya)
        };
        let yc = y.clamp(lo, hi);
        let dt = self.yb - self.ya;
        if dt.abs() <= EPS {
            return self.xa;
        }
        let t = (yc - self.ya) / dt;
        self.xa + (self.xb - self.xa) * t
    }

    /// Minimum x over the edge span.
    #[must_use]
    pub const fn min_x(&self) -> f64 {
        if self.xa <= self.xb { self.xa } else { self.xb }
    }

    /// Maximum x over the edge span.
    #[must_use]
    pub const fn max_x(&self) -> f64 {
        if self.xa >= self.xb { self.xa } else { self.xb }
    }
}

/// Build rasterizer edges from closed polygons (implicit wrap from last
/// point to first). Horizontal and non-finite segments are skipped.
#[must_use]
pub fn build_edges(polys: &[&[Vec2]]) -> Vec<Edge> {
    let mut edges = Vec::new();
    for pts in polys {
        if pts.len() < 3 {
            continue;
        }
        let mut push = |p: Vec2, q: Vec2| {
            let dy = q.y - p.y;
            if dy.abs() < EPS || !dy.is_finite() || !p.x.is_finite() || !q.x.is_finite() {
                return;
            }
            edges.push(Edge {
                ya: p.y,
                yb: q.y,
                xa: p.x,
                xb: q.x,
                sign: if dy > 0.0 { 1.0 } else { -1.0 },
            });
        };
        for pair in pts.windows(2) {
            if let [p, q] = pair {
                push(*p, *q);
            }
        }
        if let (Some(&first), Some(&last)) = (pts.first(), pts.last()) {
            push(last, first);
        }
    }
    edges
}

/// Exact winding-area contribution of one edge to one pixel column over a
/// y-band: the integral of `f(t(y))` where `f(t) = cx1 - clamp(t, cx0, cx1)`
/// and `t(y)` is the edge x at height `y`.
#[must_use]
pub fn edge_column_integral(
    x_at_ya: f64,
    x_at_yb: f64,
    cx0: f64,
    cx1: f64,
    ya: f64,
    yb: f64,
) -> f64 {
    let f = |t: f64| -> f64 { cx1 - t.clamp(cx0, cx1) };
    let dt = x_at_yb - x_at_ya;
    // Breakpoints where the edge x crosses the column edges.
    let mut samples: [(f64, f64); 4] = [(ya, x_at_ya), (yb, x_at_yb), (0.0, 0.0), (0.0, 0.0)];
    let mut n = 2_usize;
    if dt.abs() > EPS {
        for boundary in [cx0, cx1] {
            let lo = x_at_ya.min(x_at_yb);
            let hi = x_at_ya.max(x_at_yb);
            if boundary > lo && boundary < hi && n < 4 {
                let frac = (boundary - x_at_ya) / dt;
                let ybk = ya + (yb - ya) * frac;
                if ybk > ya && ybk < yb {
                    if let Some(slot) = samples.get_mut(n) {
                        *slot = (ybk, boundary);
                    }
                    n += 1;
                }
            }
        }
        // Sort samples by y (selection sort on at most 4 items).
        for i in 0..n {
            let mut best = i;
            for j in (i + 1)..n {
                let (Some(a), Some(b)) = (samples.get(j), samples.get(best)) else {
                    continue;
                };
                if a.0 < b.0 {
                    best = j;
                }
            }
            samples.swap(i, best);
        }
    }
    let mut area = 0.0;
    for i in 1..n {
        let Some(&(y1, t1)) = samples.get(i - 1) else {
            continue;
        };
        let Some(&(y2, t2)) = samples.get(i) else {
            continue;
        };
        if y2 > y1 {
            area += 0.5 * (f(t1) + f(t2)) * (y2 - y1);
        }
    }
    area
}

/// Rasterize `edges` over a `width * height` target, emitting one
/// `(row, coverage)` pair per touched row. `cov` receives each coverage
/// row (sized to `width`); `active` is per-band working storage. Both are
/// reused across calls: the steady-state frame path allocates nothing
/// here.
pub fn rasterize_polys(
    edges: &[Edge],
    width: usize,
    height: usize,
    cov: &mut Vec<f32>,
    active: &mut Vec<(f64, f64, f64)>,
    emit: &mut dyn FnMut(usize, &[f32]),
) {
    if edges.is_empty() || width == 0 || height == 0 {
        return;
    }
    let height_f = f64::from(u32::try_from(height).unwrap_or(u32::MAX));
    let mut events: Vec<f64> = Vec::with_capacity(edges.len() * 2 + height + 2);
    // Every integer row boundary is an event so a band never spans two
    // pixel rows: each band's integral lands in exactly one row.
    for r in 0..=height {
        events.push(f64::from(u32::try_from(r).unwrap_or(u32::MAX)));
    }
    for e in edges {
        if e.ya > 0.0 && e.ya < height_f {
            events.push(e.ya);
        }
        if e.yb > 0.0 && e.yb < height_f {
            events.push(e.yb);
        }
    }
    events.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
    events.dedup_by(|a, b| (*a - *b).abs() < EPS);

    cov.clear();
    cov.resize(width, 0.0);

    let mut current_row: Option<usize> = None;
    for pair in events.windows(2) {
        if let [ya, yb] = pair {
            let (ya, yb) = (*ya, *yb);
            if yb - ya < EPS {
                continue;
            }
            let row_f = ya.floor();
            if row_f < 0.0 || row_f >= height_f {
                continue;
            }
            let row = row_f as usize;

            if current_row != Some(row) {
                if let Some(prev) = current_row {
                    emit(prev, cov);
                    cov.iter_mut().for_each(|s| *s = 0.0);
                }
                current_row = Some(row);
            }

            // Band column range over active edges. Activity uses the
            // edge's true y-span: upward edges store ya > yb.
            let mut min_x = f64::INFINITY;
            let mut max_x = f64::NEG_INFINITY;
            active.clear();
            for e in edges {
                let (ey0, ey1) = if e.ya <= e.yb {
                    (e.ya, e.yb)
                } else {
                    (e.yb, e.ya)
                };
                if ey0 < yb && ey1 > ya {
                    let x0 = e.x_at(ya);
                    let x1 = e.x_at(yb);
                    active.push((x0, x1, e.sign));
                    min_x = min_x.min(x0.min(x1));
                    max_x = max_x.max(x0.max(x1));
                }
            }
            if active.is_empty() {
                continue;
            }
            let c0 = ((min_x.floor() as isize).max(0)) as usize;
            let c1 = ((max_x.ceil() as isize).max(0) as usize).min(width);
            for col in c0..c1 {
                let px0 = f64::from(u32::try_from(col).unwrap_or(u32::MAX));
                let px1 = px0 + 1.0;
                let mut sum = 0.0;
                for &(x0, x1, sign) in active.iter() {
                    // Edges strictly right of the column contribute zero.
                    // Edges at or left of it still wind over the column.
                    if x0.min(x1) >= px1 {
                        continue;
                    }
                    sum += sign * edge_column_integral(x0, x1, px0, px1, ya, yb);
                }
                if sum != 0.0
                    && let Some(slot) = cov.get_mut(col)
                {
                    // Coverage magnitude: winding sign cancels here.
                    *slot += sum.abs() as f32;
                }
            }
        }
    }
    if let Some(prev) = current_row {
        emit(prev, cov);
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn rect_edges(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Edge> {
        let pts = [
            Vec2::new(x0, y0),
            Vec2::new(x1, y0),
            Vec2::new(x1, y1),
            Vec2::new(x0, y1),
        ];
        build_edges(&[&pts])
    }

    fn total_coverage(width: usize, height: usize, edges: &[Edge]) -> f64 {
        let mut cov: Vec<f32> = Vec::new();
        let mut active: Vec<(f64, f64, f64)> = Vec::new();
        let mut sum = 0.0_f64;
        rasterize_polys(
            edges,
            width,
            height,
            &mut cov,
            &mut active,
            &mut |row, row_cov| {
                let _ = row;
                for c in row_cov {
                    sum += f64::from(c.clamp(0.0, 1.0));
                }
            },
        );
        sum
    }

    #[test]
    fn integral_exact_trapezoids() {
        // Edge x sweeps 0.5 -> 1.5 across band [0, 1]; column [0, 1].
        // f(t) = 1 - clamp(t, 0, 1): f(0.5) = 0.5, breakpoint at t = 1
        // (y = 0.5) gives f = 0, then 0 to the band end: trapezoids
        // 0.125 + 0 = 0.125.
        let got = edge_column_integral(0.5, 1.5, 0.0, 1.0, 0.0, 1.0);
        assert!((got - 0.125).abs() < 1e-12, "got {got}");
    }

    #[test]
    fn rect_area_exact() {
        // 10x10 rect at (3.2, 4.7): exact area 100.
        let edges = rect_edges(3.2, 4.7, 13.2, 14.7);
        let sum = total_coverage(32, 32, &edges);
        assert!((sum - 100.0).abs() < 1e-6, "area {sum}");
    }

    #[test]
    fn winding_cancels_outside() {
        let edges = rect_edges(5.0, 5.0, 15.0, 15.0);
        let sum = total_coverage(32, 32, &edges);
        assert!((sum - 100.0).abs() < 1e-6);
    }
}

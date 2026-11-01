//! Painter: fill API over surfaces, tying flattening to rasterization.

use crate::raster::{build_edges, rasterize_polys};
use crate::{Affine, FillRule, Rect, Surface, Vec2, premultiply};

use path_kit::Poly;

/// Solid fill paint: premultiplied sRGBA8.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Solid {
    /// Premultiplied `[r, g, b, a]` bytes.
    pub color: [u8; 4],
}

impl Solid {
    /// From premultiplied bytes.
    #[must_use]
    pub const fn premul(color: [u8; 4]) -> Self {
        Solid { color }
    }

    /// From straight-alpha bytes (multiplied out).
    #[must_use]
    pub fn new(rgba: [u8; 4]) -> Self {
        Solid {
            color: premultiply(rgba),
        }
    }
}

/// Fill painter over [`Surface`] targets.
///
/// Owns its rasterization scratch so repeated fills in a frame loop do
/// not allocate (setup-time flattening still allocates per shape in v0;
/// see the honest-limitations note in README).
#[derive(Debug)]
pub struct Painter {
    tolerance: f64,
    cov: Vec<f32>,
    active: Vec<(f64, f64, f64)>,
    edges: Vec<crate::raster::Edge>,
}

impl Painter {
    /// Painter with a given flattening tolerance (device pixels).
    #[must_use]
    pub fn new(tolerance: f64) -> Self {
        Painter {
            tolerance: tolerance.max(1e-6),
            cov: Vec::new(),
            active: Vec::new(),
            edges: Vec::new(),
        }
    }

    /// Current tolerance.
    #[must_use]
    pub const fn tolerance(&self) -> f64 {
        self.tolerance
    }

    /// Fill a path under a transform.
    pub fn fill_path(
        &mut self,
        surface: &mut Surface,
        path: &crate::Path,
        transform: Affine,
        rule: FillRule,
        paint: &Solid,
    ) {
        let polys = path.flatten(self.tolerance);
        self.fill_polys(surface, &polys, transform, rule, paint);
    }

    /// Fill flattened polygons under a transform.
    ///
    /// Open subpaths are closed implicitly (last point to first).
    pub fn fill_polys(
        &mut self,
        surface: &mut Surface,
        polys: &[Poly],
        transform: Affine,
        rule: FillRule,
        paint: &Solid,
    ) {
        let device: Vec<Poly> = transform_polys(polys, transform);
        let prepared = match rule {
            FillRule::EvenOdd => parity_reduce(&device),
            FillRule::NonZero => device,
        };
        let slices: Vec<&[Vec2]> = prepared.iter().map(|p| p.points.as_slice()).collect();
        self.fill_slices(surface, &slices, paint);
    }

    /// Fill already-device-space polygon slices.
    pub fn fill_slices(&mut self, surface: &mut Surface, polys: &[&[Vec2]], paint: &Solid) {
        self.edges.clear();
        self.edges.extend(build_edges(polys).iter().copied());
        if self.edges.is_empty() {
            return;
        }
        let target = surface.rect();
        let (w, h) = (surface.width(), surface.height());
        let cov = &mut self.cov;
        let active = &mut self.active;
        let color = paint.color;
        rasterize_polys(&self.edges, w, h, cov, active, &mut |row, row_cov| {
            surface.blend_row(row, row_cov, color);
            let _ = target;
        });
    }

    /// Fill an axis-aligned device-space rect exactly (fast path).
    pub fn fill_rect(&mut self, surface: &mut Surface, rect: Rect, paint: &Solid) {
        let Some(clip) = surface.rect().intersect(rect) else {
            return;
        };
        let (w, h) = (surface.width(), surface.height());
        let y0 = clip.y0.floor().max(0.0) as usize;
        let y1 = (clip
            .y1
            .ceil()
            .min(f64::from(u32::try_from(h).unwrap_or(u32::MAX)))) as usize;
        let x0 = clip.x0.floor().max(0.0) as usize;
        let x1 = (clip
            .x1
            .ceil()
            .min(f64::from(u32::try_from(w).unwrap_or(u32::MAX)))) as usize;
        for row in y0..y1.min(h) {
            let ry0 = f64::from(u32::try_from(row).unwrap_or(u32::MAX));
            let ry1 = ry0 + 1.0;
            let fy = (clip.y1.min(ry1) - clip.y0.max(ry0)).max(0.0);
            if fy <= 0.0 {
                continue;
            }
            self.cov.clear();
            self.cov.resize(w, 0.0);
            for col in x0..x1.min(w) {
                let rx0 = f64::from(u32::try_from(col).unwrap_or(u32::MAX));
                let rx1 = rx0 + 1.0;
                let fx = (clip.x1.min(rx1) - clip.x0.max(rx0)).max(0.0);
                if fx > 0.0
                    && let Some(slot) = self.cov.get_mut(col)
                {
                    *slot = (fx * fy) as f32;
                }
            }
            surface.blend_row(row, &self.cov, paint.color);
        }
    }
}

impl Default for Painter {
    fn default() -> Self {
        Painter::new(0.25)
    }
}

/// Transform polygons into device space.
#[must_use]
pub fn transform_polys(polys: &[Poly], t: Affine) -> Vec<Poly> {
    polys
        .iter()
        .map(|p| Poly {
            points: p.points.iter().map(|v| t.apply(*v)).collect(),
            closed: p.closed,
        })
        .collect()
}

/// Reduce an even-odd polygon set to an equivalent nonzero set: contours
/// nested an odd number of levels deep reverse orientation (REQ-R2).
///
/// Representative points are first vertices; contours are assumed
/// properly nested without shared edges (the UI case). Self-overlapping
/// same-depth contours remain an approximation (clamped winding integral).
#[must_use]
pub fn parity_reduce(polys: &[Poly]) -> Vec<Poly> {
    polys
        .iter()
        .enumerate()
        .map(|(i, poly)| {
            let rep = match poly.points.first() {
                Some(&p) => p,
                None => return poly.clone(),
            };
            let mut depth = 0_i32;
            for (j, other) in polys.iter().enumerate() {
                if i != j && path_kit::winding_number(rep, other) != 0 {
                    depth += 1;
                }
            }
            if depth % 2 == 0 {
                poly.clone()
            } else {
                let mut points = poly.points.clone();
                points.reverse();
                Poly {
                    points,
                    closed: poly.closed,
                }
            }
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::Surface;
    use crate::surface::unreachable_surface;
    use path_kit::Path;

    fn surface(w: usize, h: usize) -> Surface {
        Surface::new(w, h).unwrap_or_else(unreachable_surface)
    }

    fn square_path(x: f64, y: f64, size: f64) -> Path {
        let mut p = Path::new();
        p.move_to(Vec2::new(x, y));
        p.line_to(Vec2::new(x + size, y));
        p.line_to(Vec2::new(x + size, y + size));
        p.line_to(Vec2::new(x, y + size));
        p.close();
        p
    }

    fn sum_alpha(s: &Surface) -> u64 {
        let mut sum = 0_u64;
        for y in 0..s.height() {
            for x in 0..s.width() {
                if let Some(px) = s.pixel(x, y) {
                    sum += u64::from(px[3]);
                }
            }
        }
        sum
    }

    #[test]
    fn coverage_area_rect_exact() {
        // Integer-aligned 8x8 at (4, 4) in 16x16: every covered pixel is
        // fully covered (REQ-R1).
        let mut s = surface(16, 16);
        let mut painter = Painter::default();
        painter.fill_path(
            &mut s,
            &square_path(4.0, 4.0, 8.0),
            Affine::IDENTITY,
            FillRule::NonZero,
            &Solid::new([255, 0, 0, 255]),
        );
        for y in 0..16_usize {
            for x in 0..16_usize {
                let a = s.pixel(x, y).unwrap_or([0; 4])[3];
                let inside = (4..12).contains(&x) && (4..12).contains(&y);
                let want = u8::from(inside) * 255;
                assert_eq!(a, want, "pixel ({x},{y})");
            }
        }
        assert_eq!(sum_alpha(&s), 64 * 255);
    }

    #[test]
    fn coverage_area_triangle_analytic() {
        // Right triangle (0,0) (20,0) (0,20): area 200 (REQ-R1).
        let mut p = Path::new();
        p.move_to(Vec2::new(0.0, 0.0));
        p.line_to(Vec2::new(20.0, 0.0));
        p.line_to(Vec2::new(0.0, 20.0));
        p.close();
        let mut s = surface(24, 24);
        let mut painter = Painter::default();
        painter.fill_path(
            &mut s,
            &p,
            Affine::IDENTITY,
            FillRule::NonZero,
            &Solid::new([255, 255, 255, 255]),
        );
        let got = sum_alpha(&s) as f64 / 255.0;
        assert!(
            (got - 200.0).abs() < 2.0,
            "triangle coverage {got} vs analytic 200"
        );
    }

    #[test]
    fn fill_rules_nonzero_hole() {
        // Outer square + opposite-oriented inner square: hole empty
        // (REQ-R2). Build polys directly to control orientation.
        let outer = Poly {
            points: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(20.0, 0.0),
                Vec2::new(20.0, 20.0),
                Vec2::new(0.0, 20.0),
            ],
            closed: true,
        };
        let mut inner = Poly {
            points: vec![
                Vec2::new(5.0, 5.0),
                Vec2::new(15.0, 5.0),
                Vec2::new(15.0, 15.0),
                Vec2::new(5.0, 15.0),
            ],
            closed: true,
        };
        inner.points.reverse();
        let mut s = surface(24, 24);
        let mut painter = Painter::default();
        painter.fill_polys(
            &mut s,
            &[outer, inner],
            Affine::IDENTITY,
            FillRule::NonZero,
            &Solid::new([255, 255, 255, 255]),
        );
        assert_eq!(s.pixel(10, 10).unwrap_or([0; 4])[3], 0, "hole empty");
        assert_eq!(s.pixel(2, 2).unwrap_or([0; 4])[3], 255, "ring solid");
    }

    #[test]
    fn fill_rules_evenodd_nested() {
        // Same-orientation nested squares under even-odd: parity flip must
        // make ring solid and hole empty (REQ-R2).
        let outer = Poly {
            points: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(20.0, 0.0),
                Vec2::new(20.0, 20.0),
                Vec2::new(0.0, 20.0),
            ],
            closed: true,
        };
        let inner = Poly {
            points: vec![
                Vec2::new(5.0, 5.0),
                Vec2::new(15.0, 5.0),
                Vec2::new(15.0, 15.0),
                Vec2::new(5.0, 15.0),
            ],
            closed: true,
        };
        let mut s = surface(24, 24);
        let mut painter = Painter::default();
        painter.fill_polys(
            &mut s,
            &[outer, inner],
            Affine::IDENTITY,
            FillRule::EvenOdd,
            &Solid::new([255, 255, 255, 255]),
        );
        assert_eq!(s.pixel(10, 10).unwrap_or([0; 4])[3], 0, "hole empty");
        assert_eq!(s.pixel(2, 2).unwrap_or([0; 4])[3], 255, "ring solid");
    }

    #[test]
    fn compositing_matches_reference() {
        // Two half-alpha red fills over black: first 128, second
        // (128*128 + 128*127 + 127)/255 = 191 (REQ-R3).
        let mut s = surface(4, 4);
        let mut painter = Painter::default();
        painter.fill_rect(
            &mut s,
            Rect::from_size(Vec2::ZERO, Vec2::new(4.0, 4.0)),
            &Solid::new([255, 0, 0, 128]),
        );
        let mid = s.pixel(1, 1).unwrap_or([0; 4]);
        assert_eq!(mid[3], 128);
        painter.fill_rect(
            &mut s,
            Rect::from_size(Vec2::ZERO, Vec2::new(4.0, 4.0)),
            &Solid::new([255, 0, 0, 128]),
        );
        let after = s.pixel(1, 1).unwrap_or([0; 4]);
        // 0.5 over 0.5: exact 191.75; integer pipeline rounds to 192.
        assert_eq!(after[3], 192, "second blend got {}", after[3]);
    }

    #[test]
    fn rect_fast_path_matches_path_fill() {
        let mut a = surface(16, 16);
        let mut b = surface(16, 16);
        let mut painter = Painter::default();
        painter.fill_rect(
            &mut a,
            Rect::from_size(Vec2::new(4.0, 4.0), Vec2::new(8.0, 8.0)),
            &Solid::new([10, 20, 30, 255]),
        );
        painter.fill_path(
            &mut b,
            &square_path(4.0, 4.0, 8.0),
            Affine::IDENTITY,
            FillRule::NonZero,
            &Solid::new([10, 20, 30, 255]),
        );
        assert_eq!(a.data(), b.data(), "rect and path fills identical");
    }

    #[test]
    fn transformed_fill_shifts() {
        let mut s = surface(24, 24);
        let mut painter = Painter::default();
        painter.fill_path(
            &mut s,
            &square_path(0.0, 0.0, 8.0),
            Affine::translate(Vec2::new(10.0, 6.0)),
            FillRule::NonZero,
            &Solid::new([255, 255, 255, 255]),
        );
        assert_eq!(s.pixel(9, 6).unwrap_or([0; 4])[3], 0);
        assert_eq!(s.pixel(10, 6).unwrap_or([0; 4])[3], 255);
        assert_eq!(s.pixel(17, 13).unwrap_or([0; 4])[3], 255);
        assert_eq!(s.pixel(18, 13).unwrap_or([0; 4])[3], 0);
    }

    #[test]
    fn degenerate_inputs_no_op() {
        let mut s = surface(8, 8);
        let mut painter = Painter::default();
        painter.fill_path(
            &mut s,
            &Path::new(),
            Affine::IDENTITY,
            FillRule::NonZero,
            &Solid::new([255, 0, 0, 255]),
        );
        painter.fill_rect(
            &mut s,
            Rect::from_size(Vec2::ZERO, Vec2::ZERO),
            &Solid::new([255, 0, 0, 255]),
        );
        assert_eq!(sum_alpha(&s), 0);
    }
}

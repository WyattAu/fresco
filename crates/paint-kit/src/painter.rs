//! Painter: fill API over surfaces, tying flattening to rasterization.

use crate::paint::Paint;
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
    clips: Vec<Rect>,
}

/// A decoded image: premultiplied RGBA8, row-major, tight stride.
#[derive(Clone, Debug)]
pub struct Image {
    /// Pixel width.
    pub width: usize,
    /// Pixel height.
    pub height: usize,
    /// `width * height * 4` premultiplied bytes.
    pub data: Vec<u8>,
}

impl Image {
    /// New image from premultiplied bytes; validates the length.
    #[must_use]
    pub fn new(width: usize, height: usize, data: Vec<u8>) -> Option<Image> {
        let len = width.checked_mul(height)?.checked_mul(4)?;
        if data.len() != len {
            return None;
        }
        Some(Image {
            width,
            height,
            data,
        })
    }

    /// Sample with nearest-neighbor scaling into a unit rect position.
    pub fn sample(&self, u: f64, v: f64) -> [u8; 4] {
        let x = (u.clamp(0.0, 1.0) * f64::from(u32::try_from(self.width).unwrap_or(1))).floor()
            as usize;
        let y = (v.clamp(0.0, 1.0) * f64::from(u32::try_from(self.height).unwrap_or(1))).floor()
            as usize;
        let base = y
            .saturating_mul(self.width)
            .saturating_add(x)
            .saturating_mul(4);
        let mut px = [0_u8; 4];
        for (slot, b) in px
            .iter_mut()
            .zip(self.data.get(base..base + 4).unwrap_or(&[]))
        {
            *slot = *b;
        }
        px
    }
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
            clips: Vec::new(),
        }
    }

    /// Push an axis-aligned clip (scissor). Intersects the current clip.
    pub fn push_clip(&mut self, rect: Rect) {
        let next = match self.clips.last() {
            Some(current) => match current.intersect(rect) {
                Some(r) => r,
                None => Rect::from_size(Vec2::ZERO, Vec2::ZERO),
            },
            None => rect,
        };
        self.clips.push(next);
    }

    /// Pop the last clip.
    pub fn pop_clip(&mut self) {
        self.clips.pop();
    }

    /// Active clip (empty rect means nothing draws).
    #[must_use]
    pub fn clip(&self) -> Option<Rect> {
        self.clips.last().copied()
    }

    /// Current tolerance.
    #[must_use]
    pub const fn tolerance(&self) -> f64 {
        self.tolerance
    }

    /// Fill an axis-aligned rounded rect (uniform corner radius,
    /// clamped to half the shorter side) through the winding raster.
    pub fn fill_round_rect(
        &mut self,
        surface: &mut Surface,
        rect: Rect,
        radius: f64,
        paint: &Paint,
    ) {
        let path = rounded_rect_path(rect, radius);
        self.fill_path(surface, &path, Affine::IDENTITY, FillRule::NonZero, paint);
    }

    /// Fill a soft drop shadow under an axis-aligned rounded rect
    /// (roadmap 4.3 "SDF shadows"): per-pixel signed distance to the
    /// rounded-rect silhouette, alpha = exp(-d²/(2σ²)) outside the
    /// shape and 1 inside, composited as premultiplied `color`. The
    /// shadow extends `3σ` beyond the rect in every direction; callers
    /// that clip or damage-track must account for that extent.
    ///
    /// This is the CPU reference implementation; the sparse-strip
    /// backend wires vello's native `fill_blurred_rounded_rect` for
    /// the same visual (later slice).
    pub fn fill_shadow_round_rect(
        &mut self,
        surface: &mut Surface,
        rect: Rect,
        radius: f64,
        sigma: f64,
        color: [u8; 4],
    ) {
        let sigma = sigma.max(0.01);
        let premul = premultiply(color);
        // Shadow region: the rect inflated by 3σ, clamped to the
        // surface bounds.
        let pad = 3.0 * sigma;
        let x_min = (rect.x0 - pad).max(0.0).floor() as usize;
        let y_min = (rect.y0 - pad).max(0.0).floor() as usize;
        let x_max = (rect.x1 + pad).ceil().min(surface.rect().width()) as usize;
        let y_max = (rect.y1 + pad).ceil().min(surface.rect().height()) as usize;
        if x_max <= x_min || y_max <= y_min {
            return;
        }
        // Respect the active painter clip: blend_pixel writes straight
        // to the surface and would otherwise double-blend over pixels
        // a damage region pass must not touch.
        let clip = self.clip();
        let r = radius.clamp(0.0, rect.width().min(rect.height()) * 0.5);
        let cx = (rect.x0 + rect.x1) * 0.5;
        let cy = (rect.y0 + rect.y1) * 0.5;
        let hx = rect.width() * 0.5 - r;
        let hy = rect.height() * 0.5 - r;
        let inv = 1.0 / (2.0 * sigma * sigma);
        for py in y_min..y_max {
            for px in x_min..x_max {
                let pxf = f64::from(u32::try_from(px).unwrap_or(u32::MAX)) + 0.5;
                let pyf = f64::from(u32::try_from(py).unwrap_or(u32::MAX)) + 0.5;
                // Signed distance to the rounded rect (negative
                // inside): abs form of the SDF with corner rounds.
                let qx = (pxf - cx).abs() - hx;
                let qy = (pyf - cy).abs() - hy;
                let outer = qx.max(0.0).hypot(qy.max(0.0));
                let d = (outer + qx.max(qy).min(0.0)).max(0.0);
                if let Some(clip) = clip {
                    // Pixel center must lie inside the clip rect.
                    if pxf < clip.x0 || pxf > clip.x1 || pyf < clip.y0 || pyf > clip.y1 {
                        continue;
                    }
                }
                let alpha = (-d * d * inv).exp();
                if alpha < 0.004 {
                    continue;
                }
                let blended = [
                    ((f64::from(premul[0]) / 255.0) * alpha * 255.0).round() as u8,
                    ((f64::from(premul[1]) / 255.0) * alpha * 255.0).round() as u8,
                    ((f64::from(premul[2]) / 255.0) * alpha * 255.0).round() as u8,
                    ((f64::from(premul[3]) / 255.0) * alpha * 255.0).round() as u8,
                ];
                surface.blend_pixel(px, py, blended);
            }
        }
    }

    /// Fill a path under a transform.
    pub fn fill_path(
        &mut self,
        surface: &mut Surface,
        path: &crate::Path,
        transform: Affine,
        rule: FillRule,
        paint: &Paint,
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
        paint: &Paint,
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
    pub fn fill_slices(&mut self, surface: &mut Surface, polys: &[&[Vec2]], paint: &Paint) {
        self.edges.clear();
        self.edges.extend(build_edges(polys).iter().copied());
        if self.edges.is_empty() {
            return;
        }
        let full = surface.rect();
        let clip = self.clip().and_then(|c| full.intersect(c)).unwrap_or(full);
        if clip.is_empty() {
            return;
        }
        let (w, h) = (surface.width(), surface.height());
        let cov = &mut self.cov;
        let active = &mut self.active;
        let paint = paint.clone();
        rasterize_polys(&self.edges, w, h, cov, active, &mut |row, row_cov| {
            let rf = f64::from(u32::try_from(row).unwrap_or(u32::MAX));
            if rf < clip.y0 || rf + 1.0 > clip.y1 {
                return;
            }
            let mut masked: Vec<f32> = row_cov.to_vec();
            for (x, slot) in masked.iter_mut().enumerate() {
                let xf = f64::from(u32::try_from(x).unwrap_or(u32::MAX));
                if xf < clip.x0 || xf + 1.0 > clip.x1 {
                    *slot = 0.0;
                }
            }
            surface.blend_row_with(row, &masked, &mut |x| {
                paint.eval(f64::from(u32::try_from(x).unwrap_or(0)) + 0.5, rf + 0.5)
            });
        });
    }

    /// Draw an image with nearest-neighbor sampling into `dst` (device
    /// pixels), respecting the clip. Premultiplied over-compositing.
    pub fn draw_image(&mut self, surface: &mut Surface, image: &Image, dst: Rect) {
        let src = Rect::from_size(
            Vec2::ZERO,
            Vec2::new(
                f64::from(u32::try_from(image.width).unwrap_or(0)),
                f64::from(u32::try_from(image.height).unwrap_or(0)),
            ),
        );
        self.draw_image_region(surface, image, src, dst);
    }

    /// Draw a sub-region of `image` (sprite-atlas source rect in
    /// pixels) scaled into `dst`.
    pub fn draw_image_region(
        &mut self,
        surface: &mut Surface,
        image: &Image,
        src: Rect,
        dst: Rect,
    ) {
        let full = surface.rect();
        let Some(clip) = self
            .clip()
            .map(|c| full.intersect(c))
            .unwrap_or(Some(full))
            .and_then(|c| c.intersect(dst))
        else {
            return;
        };
        if clip.is_empty() || image.width == 0 || image.height == 0 {
            return;
        }
        // Normalized source window.
        let iw = f64::from(u32::try_from(image.width).unwrap_or(1));
        let ih = f64::from(u32::try_from(image.height).unwrap_or(1));
        let u0 = src.x0 / iw;
        let v0 = src.y0 / ih;
        let u1 = src.x1 / iw;
        let v1 = src.y1 / ih;
        let y0 = clip.y0.floor().max(0.0) as usize;
        let y1 = (clip.y1.ceil() as usize).min(surface.height());
        let x0 = clip.x0.floor().max(0.0) as usize;
        let x1 = (clip.x1.ceil() as usize).min(surface.width());
        for row in y0..y1 {
            let cy = f64::from(u32::try_from(row).unwrap_or(0)) + 0.5;
            let v = v0 + (cy - dst.y0) / dst.height().max(1e-9) * (v1 - v0);
            for col in x0..x1 {
                let cx = f64::from(u32::try_from(col).unwrap_or(0)) + 0.5;
                let u = u0 + (cx - dst.x0) / dst.width().max(1e-9) * (u1 - u0);
                let px = image.sample(u, v);
                surface.blend_pixel(col, row, px);
            }
        }
    }

    /// Fill an axis-aligned device-space rect exactly (fast path).
    pub fn fill_rect(&mut self, surface: &mut Surface, rect: Rect, paint: &Paint) {
        let surface_full = surface.rect();
        let bounded = match self.clip() {
            Some(c) => match surface_full.intersect(c).and_then(|x| x.intersect(rect)) {
                Some(r) => r,
                None => return,
            },
            None => match surface_full.intersect(rect) {
                Some(r) => r,
                None => return,
            },
        };
        let clip = bounded;
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
            surface.blend_row_with(row, &self.cov, &mut |x| {
                paint.eval(f64::from(u32::try_from(x).unwrap_or(0)) + 0.5, ry0 + 0.5)
            });
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

/// Build the outline of an axis-aligned rounded rect: four lines and
/// four quarter circles (cubic approximation, kappa). Same geometry
/// the sparse-strip backend uses via kurbo's `RoundedRect`, so the
/// parity harness compares like with like.
#[must_use]
pub fn rounded_rect_path(rect: Rect, radius: f64) -> crate::Path {
    const KAPPA: f64 = 0.552_284_749_830_793_3;
    let r = radius.clamp(0.0, rect.width().min(rect.height()) * 0.5);
    let (x0, y0, x1, y1) = (rect.x0, rect.y0, rect.x1, rect.y1);
    let k = r * KAPPA;
    let mut path = crate::Path::new();
    path.move_to(crate::Vec2::new(x0 + r, y0));
    path.line_to(crate::Vec2::new(x1 - r, y0));
    path.cubic_to(
        crate::Vec2::new(x1 - r + k, y0),
        crate::Vec2::new(x1, y0 + r - k),
        crate::Vec2::new(x1, y0 + r),
    );
    path.line_to(crate::Vec2::new(x1, y1 - r));
    path.cubic_to(
        crate::Vec2::new(x1, y1 - r + k),
        crate::Vec2::new(x1 - r + k, y1),
        crate::Vec2::new(x1 - r, y1),
    );
    path.line_to(crate::Vec2::new(x0 + r, y1));
    path.cubic_to(
        crate::Vec2::new(x0 + r - k, y1),
        crate::Vec2::new(x0, y1 - r + k),
        crate::Vec2::new(x0, y1 - r),
    );
    path.line_to(crate::Vec2::new(x0, y0 + r));
    path.cubic_to(
        crate::Vec2::new(x0, y0 + r - k),
        crate::Vec2::new(x0 + r - k, y0),
        crate::Vec2::new(x0 + r, y0),
    );
    path.close();
    path
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::Surface;
    use path_kit::Path;

    fn surface(w: usize, h: usize) -> Surface {
        Surface::new(w, h).unwrap_or_default()
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
            &Paint::from(Solid::new([255, 0, 0, 255])),
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
            &Paint::from(Solid::new([255, 255, 255, 255])),
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
            &Paint::from(Solid::new([255, 255, 255, 255])),
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
            &Paint::from(Solid::new([255, 255, 255, 255])),
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
            &Paint::from(Solid::new([255, 0, 0, 128])),
        );
        let mid = s.pixel(1, 1).unwrap_or([0; 4]);
        assert_eq!(mid[3], 128);
        painter.fill_rect(
            &mut s,
            Rect::from_size(Vec2::ZERO, Vec2::new(4.0, 4.0)),
            &Paint::from(Solid::new([255, 0, 0, 128])),
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
            &Paint::from(Solid::new([10, 20, 30, 255])),
        );
        painter.fill_path(
            &mut b,
            &square_path(4.0, 4.0, 8.0),
            Affine::IDENTITY,
            FillRule::NonZero,
            &Paint::from(Solid::new([10, 20, 30, 255])),
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
            &Paint::from(Solid::new([255, 255, 255, 255])),
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
            &Paint::from(Solid::new([255, 0, 0, 255])),
        );
        painter.fill_rect(
            &mut s,
            Rect::from_size(Vec2::ZERO, Vec2::ZERO),
            &Paint::from(Solid::new([255, 0, 0, 255])),
        );
        assert_eq!(sum_alpha(&s), 0);
    }

    #[test]
    fn gradient_fill_horizontal_interpolates() {
        use crate::paint::{ColorStop, LinearGradient};
        let mut s = surface(24, 8);
        let mut painter = Painter::default();
        let paint = Paint::Linear(LinearGradient::new(
            Vec2::ZERO,
            Vec2::new(24.0, 0.0),
            vec![
                ColorStop {
                    offset: 0.0,
                    color: [0, 0, 0, 255],
                },
                ColorStop {
                    offset: 1.0,
                    color: [240, 0, 0, 255],
                },
            ],
        ));
        painter.fill_rect(
            &mut s,
            Rect::from_size(Vec2::ZERO, Vec2::new(24.0, 8.0)),
            &paint,
        );
        let left = s.pixel(2, 4).unwrap_or([0; 4])[0];
        let right = s.pixel(21, 4).unwrap_or([0; 4])[0];
        assert!(left < 40, "left dark: {left}");
        assert!(right > 200, "right bright: {right}");
        // Monotone across the row.
        let mut prev = 0_u8;
        for x in 0..24 {
            let v = s.pixel(x, 4).unwrap_or([0; 4])[0];
            assert!(v >= prev, "non-monotone at {x}: {v} < {prev}");
            prev = v;
        }
    }

    #[test]
    fn shadow_showcase_artifact_written() {
        // Device-screen artifact: two shadowed gradient-ish cards on a
        // dark surface (raw premultiplied RGBA8 for offline
        // conversion; paint-kit has no image encoder).
        let mut surface = Surface::new(200, 120).unwrap_or_default();
        surface.fill_solid([16, 17, 22, 255]);
        let mut painter = Painter::default();
        painter.fill_shadow_round_rect(
            &mut surface,
            Rect::from_points(Vec2::new(20.0, 20.0), Vec2::new(90.0, 80.0)),
            10.0,
            4.0,
            [0, 0, 0, 230],
        );
        painter.fill_round_rect(
            &mut surface,
            Rect::from_points(Vec2::new(20.0, 20.0), Vec2::new(90.0, 80.0)),
            10.0,
            &Paint::from(Solid::premul([36, 62, 140, 255])),
        );
        painter.fill_shadow_round_rect(
            &mut surface,
            Rect::from_points(Vec2::new(104.0, 36.0), Vec2::new(184.0, 96.0)),
            10.0,
            6.0,
            [0, 0, 0, 230],
        );
        painter.fill_round_rect(
            &mut surface,
            Rect::from_points(Vec2::new(104.0, 36.0), Vec2::new(184.0, 96.0)),
            10.0,
            &Paint::from(Solid::premul([40, 140, 100, 255])),
        );
        let dir = std::path::Path::new("/tmp/opencode");
        std::fs::create_dir_all(dir).ok();
        std::fs::write(dir.join("shadow-showcase.rgba"), surface.data()).ok();
    }

    #[test]
    fn shadow_alpha_falls_off_monotonically() {
        let mut surface = Surface::new(80, 40).unwrap_or_default();
        let mut painter = Painter::default();
        painter.fill_shadow_round_rect(
            &mut surface,
            Rect::from_points(Vec2::new(20.0, 10.0), Vec2::new(60.0, 30.0)),
            6.0,
            3.0,
            [0, 0, 0, 255],
        );
        // Sample along the outward normal past the right edge
        // (x = 60 + d), y = 20 (the vertical middle).
        let mut prev = 255_u8;
        for (d, x) in [(0.0_f64, 59_usize), (1.0, 61), (2.0, 62), (4.0, 64)] {
            let alpha = surface.pixel(x, 20).map(|p| p[3]).unwrap_or(0);
            assert!(
                alpha <= prev,
                "alpha must fall off monotonically: d={d} alpha={alpha} prev={prev}"
            );
            prev = alpha;
        }
        // Far outside 3 sigma: fully transparent.
        let far = surface.pixel(75, 20).map(|p| p[3]).unwrap_or(255);
        assert_eq!(far, 0, "beyond 3 sigma the shadow is empty: {far}");
    }

    #[test]
    fn shadow_inside_shape_is_opaque() {
        let mut surface = Surface::new(60, 40).unwrap_or_default();
        let mut painter = Painter::default();
        painter.fill_shadow_round_rect(
            &mut surface,
            Rect::from_points(Vec2::new(10.0, 10.0), Vec2::new(50.0, 30.0)),
            6.0,
            3.0,
            [10, 20, 30, 255],
        );
        // Deep inside the silhouette the distance is negative, so the
        // alpha saturates at 1 and the premultiplied color is exact.
        let c = surface.pixel(30, 20).unwrap_or([9, 9, 9, 9]);
        assert_eq!(c, [10, 20, 30, 255], "interior must be exact: {c:?}");
    }

    #[test]
    fn shadow_respects_surface_bounds() {
        // A rect hanging off the canvas edge must not panic and must
        // not write outside the surface (blend_pixel bounds-checks).
        let mut surface = Surface::new(40, 20).unwrap_or_default();
        let mut painter = Painter::default();
        painter.fill_shadow_round_rect(
            &mut surface,
            Rect::from_points(Vec2::new(30.0, 5.0), Vec2::new(70.0, 15.0)),
            4.0,
            3.0,
            [0, 0, 0, 200],
        );
        // Just sanity: the visible part carries some shadow.
        let alpha = surface.pixel(35, 10).map(|p| p[3]).unwrap_or(0);
        assert!(alpha > 0, "visible part must carry shadow: {alpha}");
    }

    #[test]
    fn clip_scissor_is_exact() {
        let mut s = surface(16, 16);
        let mut painter = Painter::default();
        painter.push_clip(Rect::from_size(Vec2::new(8.0, 0.0), Vec2::new(8.0, 16.0)));
        painter.fill_rect(
            &mut s,
            Rect::from_size(Vec2::ZERO, Vec2::new(16.0, 16.0)),
            &Paint::from(Solid::new([255, 255, 255, 255])),
        );
        for y in 0..16_usize {
            for x in 0..16_usize {
                let a = s.pixel(x, y).unwrap_or([0; 4])[3];
                let want = if x >= 8 { 255 } else { 0 };
                assert_eq!(a, want, "pixel ({x},{y})");
            }
        }
        painter.pop_clip();
        painter.fill_rect(
            &mut s,
            Rect::from_size(Vec2::new(0.0, 0.0), Vec2::new(2.0, 2.0)),
            &Paint::from(Solid::new([255, 255, 255, 255])),
        );
        assert_eq!(s.pixel(1, 1).unwrap_or([0; 4])[3], 255, "clip popped");
    }

    #[test]
    fn image_blit_identity_scale_and_alpha() {
        let mut s = surface(8, 8);
        let mut painter = Painter::default();
        // 2x2 premultiplied image: opaque red, transparent, half-alpha
        // blue (premultiplied: b = 255*128/255 = 128), opaque green.
        let img = Image::new(
            2,
            2,
            vec![
                255, 0, 0, 255, //
                0, 0, 0, 0, //
                0, 0, 128, 128, //
                0, 255, 0, 255, //
            ],
        )
        .unwrap_or_else(|| Image {
            width: 1,
            height: 1,
            data: vec![0; 4],
        });
        // Identity: dst rect exactly 2x2 at (3, 3).
        painter.draw_image(
            &mut s,
            &img,
            Rect::from_size(Vec2::new(3.0, 3.0), Vec2::new(2.0, 2.0)),
        );
        assert_eq!(s.pixel(3, 3).unwrap_or([0; 4]), [255, 0, 0, 255], "red");
        assert_eq!(s.pixel(4, 3).unwrap_or([0; 4])[3], 0, "transparent");
        let half = s.pixel(3, 4).unwrap_or([0; 4]);
        assert_eq!(half[2], 128, "half blue: {}", half[2]);
        assert_eq!(half[3], 128);
        assert_eq!(s.pixel(4, 4).unwrap_or([0; 4]), [0, 255, 0, 255], "green");
    }

    #[test]
    fn image_blit_nearest_scale() {
        let mut s = surface(8, 8);
        let mut painter = Painter::default();
        // 1x1 opaque white scaled to 2x2.
        let img = Image::new(1, 1, vec![255, 255, 255, 255]).unwrap_or_else(|| Image {
            width: 1,
            height: 1,
            data: vec![0; 4],
        });
        painter.draw_image(
            &mut s,
            &img,
            Rect::from_size(Vec2::new(2.0, 2.0), Vec2::new(2.0, 2.0)),
        );
        assert_eq!(s.pixel(2, 2).unwrap_or([0; 4])[3], 255);
        assert_eq!(s.pixel(3, 3).unwrap_or([0; 4])[3], 255);
        assert_eq!(s.pixel(4, 4).unwrap_or([0; 4])[3], 0, "outside dst");
    }

    #[test]
    fn radial_gradient_fill_center_is_dark() {
        use crate::paint::{ColorStop, RadialGradient};
        let mut s = surface(20, 20);
        let mut painter = Painter::default();
        let paint = Paint::Radial(RadialGradient::new(
            Vec2::new(10.0, 10.0),
            10.0,
            vec![
                ColorStop {
                    offset: 0.0,
                    color: [0, 0, 0, 255],
                },
                ColorStop {
                    offset: 1.0,
                    color: [200, 200, 200, 255],
                },
            ],
        ));
        painter.fill_rect(
            &mut s,
            Rect::from_size(Vec2::ZERO, Vec2::new(20.0, 20.0)),
            &paint,
        );
        let center = s.pixel(10, 10).unwrap_or([0; 4])[0];
        let corner = s.pixel(19, 19).unwrap_or([0; 4])[0];
        assert!(center < 20, "center dark: {center}");
        assert!(corner > 140, "corner bright: {corner}");
    }
}

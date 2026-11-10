//! Software triangle rasterizer with a depth buffer.
//!
//! Flat-shaded triangles over a paint-kit surface: perspective-
//! projected vertices, screen-space barycentric fill, per-pixel
//! depth test against a z-buffer. Triangles with any vertex behind
//! the near plane are rejected (no near clipping — documented
//! simplification for the foundation increment).

use crate::math::Mat4;

/// A vertex position in world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    /// World position.
    pub pos: crate::math::Vec3,
}

/// A flat-shaded triangle.
#[derive(Clone, Copy, Debug)]
pub struct Triangle {
    /// Three vertices.
    pub verts: [Vertex; 3],
    /// Premultiplied RGBA color.
    pub color: [u8; 4],
}

/// A texture-mapped triangle: per-vertex UVs in normalized image
/// space. UVs interpolate affinely in screen space (documented
/// approximation: no perspective-correct divide).
#[derive(Clone, Copy, Debug)]
pub struct TexturedTriangle {
    /// Three vertices.
    pub verts: [Vertex; 3],
    /// Normalized UV per vertex, aligned to `verts`.
    pub uvs: [(f64, f64); 3],
}

/// A frame's render target state.
pub struct Frame {
    depth: Vec<f64>,
    width: usize,
    height: usize,
}

impl Frame {
    /// New depth buffer sized to the surface.
    #[must_use]
    pub fn new(width: usize, height: usize) -> Self {
        Frame {
            depth: vec![f64::INFINITY; width.saturating_mul(height)],
            width,
            height,
        }
    }

    /// Clear the depth buffer for a new frame.
    pub fn clear(&mut self) {
        self.depth.fill(f64::INFINITY);
    }

    /// Render triangles through `view_projection` onto `surface`.
    /// Near-plane rejection: any vertex with `z < near` drops the
    /// whole triangle.
    pub fn draw_triangles(
        &mut self,
        surface: &mut paint_kit::Surface,
        painter: &mut paint_kit::Painter,
        view_projection: &Mat4,
        triangles: &[Triangle],
        near: f64,
    ) {
        for t in triangles {
            let mut pts = [(0.0_f64, 0.0_f64, 0.0_f64); 3];
            let mut ok = true;
            for (k, pt_slot) in pts.iter_mut().enumerate() {
                let Some(v) = t.verts.get(k) else {
                    ok = false;
                    break;
                };
                match view_projection.transform_point_w(v.pos) {
                    // w is the view depth: behind-camera (w <= near)
                    // vertices reject the whole triangle. NDC z alone
                    // cannot detect this — the divide flips sign.
                    Some((x, y, z, w)) if w >= near => {
                        *pt_slot = (x / w, y / w, z / w);
                    }
                    _ => {
                        ok = false;
                        break;
                    }
                }
            }
            if !ok {
                continue;
            }
            self.raster(surface, painter, pts, t.color);
        }
    }

    /// Render texture-mapped triangles: UVs interpolated affinely,
    /// sampled nearest-neighbor from `image`.
    pub fn draw_textured(
        &mut self,
        surface: &mut paint_kit::Surface,
        painter: &mut paint_kit::Painter,
        view_projection: &Mat4,
        triangles: &[TexturedTriangle],
        image: &paint_kit::Image,
        near: f64,
    ) {
        for t in triangles {
            let mut pts = [(0.0_f64, 0.0_f64, 0.0_f64); 3];
            let mut uvs = [(0.0_f64, 0.0_f64); 3];
            let mut ok = true;
            for (k, pt_slot) in pts.iter_mut().enumerate() {
                let (Some(v), Some(uv)) = (t.verts.get(k), t.uvs.get(k)) else {
                    ok = false;
                    break;
                };
                match view_projection.transform_point_w(v.pos) {
                    Some((x, y, z, w)) if w >= near => {
                        *pt_slot = (x / w, y / w, z / w);
                        if let Some(uv_slot) = uvs.get_mut(k) {
                            *uv_slot = *uv;
                        }
                    }
                    _ => {
                        ok = false;
                        break;
                    }
                }
            }
            if !ok {
                continue;
            }
            self.raster_textured(surface, painter, pts, uvs, image);
        }
    }

    /// Fill one projected textured triangle with a depth test.
    fn raster_textured(
        &mut self,
        surface: &mut paint_kit::Surface,
        painter: &mut paint_kit::Painter,
        pts: [(f64, f64, f64); 3],
        uvs: [(f64, f64); 3],
        image: &paint_kit::Image,
    ) {
        let _ = painter;
        let w = f64::from(u32::try_from(self.width).unwrap_or(1));
        let h = f64::from(u32::try_from(self.height).unwrap_or(1));
        let sx = |ndc_x: f64| -> f64 { (ndc_x + 1.0) * 0.5 * w };
        let sy = |ndc_y: f64| -> f64 { (1.0 - ndc_y) * 0.5 * h };
        let ax = sx(pts[0].0);
        let ay = sy(pts[0].1);
        let bx = sx(pts[1].0);
        let by = sy(pts[1].1);
        let cx = sx(pts[2].0);
        let cy = sy(pts[2].1);
        let area = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
        if area.abs() < 1e-12 {
            return;
        }
        let min_x = ax.min(bx).min(cx).floor().max(0.0) as usize;
        let min_y = ay.min(by).min(cy).floor().max(0.0) as usize;
        let max_x = (ax.max(bx).max(cx).ceil() as usize).min(self.width.saturating_sub(1));
        let max_y = (ay.max(by).max(cy).ceil() as usize).min(self.height.saturating_sub(1));
        for row in min_y..=max_y {
            for col in min_x..=max_x {
                let px = f64::from(u32::try_from(col).unwrap_or(0)) + 0.5;
                let py = f64::from(u32::try_from(row).unwrap_or(0)) + 0.5;
                let w0 = ((bx - ax) * (py - ay) - (by - ay) * (px - ax)) / area;
                let w1 = ((cx - bx) * (py - by) - (cy - by) * (px - bx)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < -1e-9 || w1 < -1e-9 || w2 < -1e-9 {
                    continue;
                }
                let z = pts[0].2 * w1 + pts[1].2 * w2 + pts[2].2 * w0;
                let idx = row.checked_mul(self.width).and_then(|b| b.checked_add(col));
                let Some(idx) = idx else {
                    continue;
                };
                let old = match self.depth.get(idx) {
                    Some(&d) => d,
                    None => continue,
                };
                if z < old {
                    if let Some(slot) = self.depth.get_mut(idx) {
                        *slot = z;
                    }
                    // UVs use the same weight alignment as z.
                    let u = uvs[0].0 * w1 + uvs[1].0 * w2 + uvs[2].0 * w0;
                    let v = uvs[0].1 * w1 + uvs[1].1 * w2 + uvs[2].1 * w0;
                    let texel = image.sample(u, v);
                    surface.blend_pixel(col, row, texel);
                }
            }
        }
    }

    /// Fill one projected triangle with a depth test.
    fn raster(
        &mut self,
        surface: &mut paint_kit::Surface,
        painter: &mut paint_kit::Painter,
        pts: [(f64, f64, f64); 3],
        color: [u8; 4],
    ) {
        let _ = painter; // painter reserved for future shading hooks
        // NDC (-1..1) to pixel coordinates.
        let w = f64::from(u32::try_from(self.width).unwrap_or(1));
        let h = f64::from(u32::try_from(self.height).unwrap_or(1));
        let sx = |ndc_x: f64| -> f64 { (ndc_x + 1.0) * 0.5 * w };
        let sy = |ndc_y: f64| -> f64 { (1.0 - ndc_y) * 0.5 * h };
        let ax = sx(pts[0].0);
        let ay = sy(pts[0].1);
        let bx = sx(pts[1].0);
        let by = sy(pts[1].1);
        let cx = sx(pts[2].0);
        let cy = sy(pts[2].1);
        // Signed area: positive when counter-clockwise on screen.
        let area = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
        if area.abs() < 1e-12 {
            return; // degenerate
        }
        let min_x = ax.min(bx).min(cx).floor().max(0.0) as usize;
        let min_y = ay.min(by).min(cy).floor().max(0.0) as usize;
        let max_x = (ax.max(bx).max(cx).ceil() as usize).min(self.width.saturating_sub(1));
        let max_y = (ay.max(by).max(cy).ceil() as usize).min(self.height.saturating_sub(1));
        for row in min_y..=max_y {
            for col in min_x..=max_x {
                let px = f64::from(u32::try_from(col).unwrap_or(0)) + 0.5;
                let py = f64::from(u32::try_from(row).unwrap_or(0)) + 0.5;
                let w0 = ((bx - ax) * (py - ay) - (by - ay) * (px - ax)) / area;
                let w1 = ((cx - bx) * (py - by) - (cy - by) * (px - bx)) / area;
                let w2 = 1.0 - w0 - w1;
                // Inside when all barycentrics are >= -epsilon.
                if w0 < -1e-9 || w1 < -1e-9 || w2 < -1e-9 {
                    continue;
                }
                // w0 is c's weight, w1 a's, w2 b's (edge-function
                // attribution) — keep attributes aligned to vertices.
                let z = pts[0].2 * w1 + pts[1].2 * w2 + pts[2].2 * w0;
                let idx = row.checked_mul(self.width).and_then(|b| b.checked_add(col));
                let Some(idx) = idx else {
                    continue;
                };
                let old = match self.depth.get(idx) {
                    Some(&d) => d,
                    None => continue,
                };
                if z < old {
                    if let Some(slot) = self.depth.get_mut(idx) {
                        *slot = z;
                    }
                    let px = [color[0], color[1], color[2], color[3]];
                    surface.blend_pixel(col, row, px);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{Mat4, Vec3};

    fn tri(a: Vec3, b: Vec3, c: Vec3, color: [u8; 4]) -> Triangle {
        Triangle {
            verts: [Vertex { pos: a }, Vertex { pos: b }, Vertex { pos: c }],
            color,
        }
    }

    fn vp() -> Mat4 {
        let proj = Mat4::perspective(std::f64::consts::FRAC_PI_2, 1.0, 0.1, 100.0);
        let view = Mat4::look_at(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::ZERO,
            Vec3::new(0.0, 1.0, 0.0),
        );
        proj.mul(&view)
    }

    #[test]
    fn front_facing_triangle_paints_its_center() {
        let Some(mut surface) = paint_kit::Surface::new(64, 64) else {
            return;
        };
        let mut painter = paint_kit::Painter::default();
        let mut frame = Frame::new(64, 64);
        let t = tri(
            Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            [255, 0, 0, 255],
        );
        frame.draw_triangles(&mut surface, &mut painter, &vp(), &[t], 0.1);
        // The projected triangle covers the screen center.
        let px = surface.data().get((32 * 64 + 32) * 4);
        assert_eq!(px, Some(&255), "red at screen center");
    }

    #[test]
    fn behind_near_plane_rejected() {
        let Some(mut surface) = paint_kit::Surface::new(64, 64) else {
            return;
        };
        let mut painter = paint_kit::Painter::default();
        let mut frame = Frame::new(64, 64);
        // Triangle at z = +5 is BEHIND the camera (view looks -Z from z=5).
        let t = tri(
            Vec3::new(-1.0, -1.0, 10.0),
            Vec3::new(1.0, -1.0, 10.0),
            Vec3::new(0.0, 1.0, 10.0),
            [255, 0, 0, 255],
        );
        frame.draw_triangles(&mut surface, &mut painter, &vp(), &[t], 0.1);
        let painted = surface.data().iter().any(|&b| b != 0);
        assert!(!painted, "behind-camera triangle paints nothing");
    }

    #[test]
    fn depth_test_prefers_nearer_triangle() {
        let Some(mut surface) = paint_kit::Surface::new(64, 64) else {
            return;
        };
        let mut painter = paint_kit::Painter::default();
        let mut frame = Frame::new(64, 64);
        // Two overlapping triangles at different depths; the nearer
        // one (drawn LAST) must win the shared pixels. The eye sits
        // at world z=+5 looking -Z: world z=0 is NEARER than z=-1.
        let farther = tri(
            Vec3::new(-1.0, -1.0, -1.0),
            Vec3::new(1.0, -1.0, -1.0),
            Vec3::new(0.0, 1.0, -1.0),
            [255, 0, 0, 255],
        );
        let nearer = tri(
            Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            [0, 0, 255, 255],
        );
        // Same footprint: both cover the screen center.
        frame.draw_triangles(&mut surface, &mut painter, &vp(), &[farther, nearer], 0.1);
        let px = surface.data().get((32 * 64 + 32) * 4 + 2);
        assert_eq!(px, Some(&255), "blue (near) wins the center pixel");
    }

    #[test]
    fn raster_is_deterministic() {
        let run = || {
            let Some(mut surface) = paint_kit::Surface::new(64, 64) else {
                return 0_u64;
            };
            let mut painter = paint_kit::Painter::default();
            let mut frame = Frame::new(64, 64);
            let ts = [
                tri(
                    Vec3::new(-1.0, -1.0, 0.0),
                    Vec3::new(1.0, -1.0, 0.0),
                    Vec3::new(0.0, 1.0, 0.0),
                    [255, 0, 0, 255],
                ),
                tri(
                    Vec3::new(0.0, -1.0, -0.5),
                    Vec3::new(1.5, -1.0, -0.5),
                    Vec3::new(0.75, 1.0, -0.5),
                    [0, 255, 0, 255],
                ),
            ];
            frame.draw_triangles(&mut surface, &mut painter, &vp(), &ts, 0.1);
            surface.data().iter().fold(0xcbf29ce484222325_u64, |h, &b| {
                (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
            })
        };
        assert_eq!(run(), run(), "raster byte-identical across runs");
    }

    #[test]
    fn clear_resets_depth() {
        let mut frame = Frame::new(8, 8);
        if let Some(slot) = frame.depth.first_mut() {
            *slot = 1.0;
        }
        frame.clear();
        assert_eq!(frame.depth.first(), Some(&f64::INFINITY));
    }
}

#[cfg(test)]
mod textured_tests {
    use super::*;
    use crate::math::Vec3;

    /// Left half red, right half blue.
    fn split_image() -> paint_kit::Image {
        let w = 8_usize;
        let h = 4_usize;
        let mut data = vec![0_u8; w * h * 4];
        for row in 0..h {
            for col in 0..w {
                let base = (row * w + col) * 4;
                let bytes: [u8; 4] = if col < w / 2 {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 255, 255]
                };
                if let Some(slot) = data.get_mut(base..base + 4) {
                    slot.copy_from_slice(&bytes);
                }
            }
        }
        paint_kit::Image::new(w, h, data).unwrap_or_else(|| paint_kit::Image {
            width: 1,
            height: 1,
            data: vec![0, 0, 0, 0],
        })
    }

    #[test]
    fn textured_triangle_samples_image() {
        let Some(mut surface) = paint_kit::Surface::new(64, 64) else {
            return;
        };
        let mut painter = paint_kit::Painter::default();
        let mut frame = Frame::new(64, 64);
        let img = split_image();
        // Full-screen quad as two triangles, UVs spanning the image.
        let quad = [
            TexturedTriangle {
                verts: [
                    Vertex {
                        pos: Vec3::new(-1.0, -1.0, 0.0),
                    },
                    Vertex {
                        pos: Vec3::new(1.0, -1.0, 0.0),
                    },
                    Vertex {
                        pos: Vec3::new(-1.0, 1.0, 0.0),
                    },
                ],
                uvs: [(0.0, 1.0), (1.0, 1.0), (0.0, 0.0)],
            },
            TexturedTriangle {
                verts: [
                    Vertex {
                        pos: Vec3::new(1.0, -1.0, 0.0),
                    },
                    Vertex {
                        pos: Vec3::new(1.0, 1.0, 0.0),
                    },
                    Vertex {
                        pos: Vec3::new(-1.0, 1.0, 0.0),
                    },
                ],
                uvs: [(1.0, 1.0), (1.0, 0.0), (0.0, 0.0)],
            },
        ];
        frame.draw_textured(
            &mut surface,
            &mut painter,
            &Mat4::identity(),
            &quad,
            &img,
            0.1,
        );
        // u=0.5 column: pixel at x=15 (left of center) red, x=48 blue.
        let left = surface.data().get((32 * 64 + 15) * 4);
        let right = surface.data().get((32 * 64 + 48) * 4 + 2);
        assert_eq!(left, Some(&255), "left half red");
        assert_eq!(right, Some(&255), "right half blue");
        let left_blue = surface.data().get((32 * 64 + 15) * 4 + 2);
        assert_eq!(left_blue, Some(&0), "left has no blue");
    }

    #[test]
    fn textured_render_is_deterministic() {
        let run = || {
            let Some(mut surface) = paint_kit::Surface::new(64, 64) else {
                return 0_u64;
            };
            let mut painter = paint_kit::Painter::default();
            let mut frame = Frame::new(64, 64);
            let img = split_image();
            let quad = [TexturedTriangle {
                verts: [
                    Vertex {
                        pos: Vec3::new(-1.0, -1.0, 0.0),
                    },
                    Vertex {
                        pos: Vec3::new(1.0, -1.0, 0.0),
                    },
                    Vertex {
                        pos: Vec3::new(-1.0, 1.0, 0.0),
                    },
                ],
                uvs: [(0.0, 1.0), (1.0, 1.0), (0.0, 0.0)],
            }];
            frame.draw_textured(
                &mut surface,
                &mut painter,
                &Mat4::identity(),
                &quad,
                &img,
                0.1,
            );
            surface.data().iter().fold(0xcbf29ce484222325_u64, |h, &b| {
                (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
            })
        };
        assert_eq!(run(), run(), "textured raster byte-identical");
    }
}

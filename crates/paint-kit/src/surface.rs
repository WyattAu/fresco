//! Premultiplied RGBA8 pixel surfaces.

/// A raster target: premultiplied RGBA8, row-major, explicit stride.
#[derive(Clone, Debug)]
pub struct Surface {
    width: usize,
    height: usize,
    stride: usize,
    data: Vec<u8>,
}

impl Surface {
    /// New surface with tight packing (`stride == width * 4`).
    ///
    /// Dimension overflow returns `None` instead of panicking.
    #[must_use]
    pub fn new(width: usize, height: usize) -> Option<Surface> {
        let stride = width.checked_mul(4)?;
        let len = stride.checked_mul(height)?;
        Some(Surface {
            width,
            height,
            stride,
            data: vec![0_u8; len],
        })
    }

    /// New surface with a caller stride (>= `width * 4`).
    #[must_use]
    pub fn with_stride(width: usize, height: usize, stride: usize) -> Option<Surface> {
        let min = width.checked_mul(4)?;
        if stride < min {
            return None;
        }
        let len = stride.checked_mul(height)?;
        Some(Surface {
            width,
            height,
            stride,
            data: vec![0_u8; len],
        })
    }

    /// Pixel width.
    #[must_use]
    pub const fn width(&self) -> usize {
        self.width
    }

    /// Pixel height.
    #[must_use]
    pub const fn height(&self) -> usize {
        self.height
    }

    /// Byte stride.
    #[must_use]
    pub const fn stride(&self) -> usize {
        self.stride
    }

    /// Raw premultiplied bytes.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Mutable raw bytes.
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// Bounds as a float rect.
    #[must_use]
    pub fn rect(&self) -> crate::Rect {
        crate::Rect::from_size(
            crate::Vec2::ZERO,
            crate::Vec2::new(self.width as f64, self.height as f64),
        )
    }

    /// Read one pixel.
    #[must_use]
    pub fn pixel(&self, x: usize, y: usize) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let base = y.checked_mul(self.stride)?.checked_add(x.checked_mul(4)?)?;
        let d = &self.data;
        Some([
            *d.get(base)?,
            *d.get(base + 1)?,
            *d.get(base + 2)?,
            *d.get(base + 3)?,
        ])
    }

    /// Write one pixel.
    pub fn set_pixel(&mut self, x: usize, y: usize, rgba: [u8; 4]) {
        if x >= self.width || y >= self.height {
            return;
        }
        let base = match y
            .checked_mul(self.stride)
            .and_then(|v| v.checked_add(x.checked_mul(4)?))
        {
            Some(b) => b,
            None => return,
        };
        for (k, byte) in rgba.into_iter().enumerate() {
            if let Some(slot) = self.data.get_mut(base + k) {
                *slot = byte;
            }
        }
    }

    /// Fill everything with a premultiplied color.
    pub fn fill_solid(&mut self, color: [u8; 4]) {
        for y in 0..self.height {
            let start = y * self.stride;
            let end = start + self.width * 4;
            if let Some(row) = self.data.get_mut(start..end) {
                for chunk in row.chunks_exact_mut(4) {
                    for (slot, src) in chunk.iter_mut().zip(color) {
                        *slot = src;
                    }
                }
            }
        }
    }

    /// Clear to transparent.
    pub fn clear(&mut self) {
        self.data.fill(0);
    }

    /// Blend a premultiplied color into one row under per-pixel coverage
    /// in `[0, 1]`. Internal to the rasterizer; public for tests.
    pub fn blend_row(&mut self, y: usize, cov: &[f32], color: [u8; 4]) {
        if y >= self.height || cov.is_empty() {
            return;
        }
        let cols = cov.len().min(self.width);
        let row_start = y * self.stride;
        let end = (row_start + self.width * 4).min(self.data.len());
        let Some(row) = self.data.get_mut(row_start..end) else {
            return;
        };
        for (x, c) in cov.iter().take(cols).enumerate() {
            let cv = f64::from(c.clamp(0.0, 1.0));
            // Effective source = paint premultiplied color scaled by
            // coverage; its alpha is paint_alpha * coverage.
            let a_eff = (cv * f64::from(color[3])).round() as u32;
            if a_eff == 0 {
                continue;
            }
            let inv = 255 - a_eff;
            let base = x * 4;
            let Some(chunk) = row.get_mut(base..base + 4) else {
                continue;
            };
            let mut dst = [0_u8; 4];
            for (d, b) in dst.iter_mut().zip(chunk.iter()) {
                *d = *b;
            }
            let src_eff = [
                (cv * f64::from(color[0])).round() as u32,
                (cv * f64::from(color[1])).round() as u32,
                (cv * f64::from(color[2])).round() as u32,
                a_eff,
            ];
            let blended = src_eff.iter().zip(dst).map(|(src, dstv)| {
                let v = (src * 255 + u32::from(dstv) * inv + 127) / 255;
                v.min(255) as u8
            });
            for (slot, v) in chunk.iter_mut().zip(blended) {
                *slot = v;
            }
        }
    }
}

#[cfg(test)]
pub(crate) fn unreachable_surface() -> Surface {
    match Surface::new(1, 1) {
        Some(s) => s,
        None => Surface {
            width: 1,
            height: 1,
            stride: 4,
            data: Vec::new(),
        },
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn construct_and_access() {
        let mut s = Surface::new(4, 3)
            .unwrap_or_else(|| Surface::with_stride(1, 1, 4).unwrap_or_else(unreachable_surface));
        assert_eq!(s.width(), 4);
        assert_eq!(s.height(), 3);
        assert_eq!(s.stride(), 16);
        s.set_pixel(2, 1, [10, 20, 30, 40]);
        assert_eq!(s.pixel(2, 1), Some([10, 20, 30, 40]));
        assert_eq!(s.pixel(4, 1), None);
        assert_eq!(s.pixel(2, 3), None);
    }

    fn unreachable_surface() -> Surface {
        Surface::new(1, 1).unwrap_or_else(|| Surface {
            width: 1,
            height: 1,
            stride: 4,
            data: Vec::new(),
        })
    }

    #[test]
    fn overflow_returns_none() {
        let big = usize::MAX;
        assert!(Surface::new(big, big).is_none());
        assert!(
            Surface::with_stride(10, 10, 8).is_none(),
            "stride below min"
        );
    }

    #[test]
    fn stride_padding_isolated() {
        let mut s = Surface::with_stride(3, 2, 16).unwrap_or_else(unreachable_surface);
        assert_eq!(s.stride(), 16);
        s.set_pixel(2, 0, [255, 0, 0, 255]);
        s.set_pixel(0, 1, [0, 255, 0, 255]);
        assert_eq!(s.pixel(0, 1), Some([0, 255, 0, 255]));
        assert_eq!(s.pixel(2, 0), Some([255, 0, 0, 255]));
    }

    #[test]
    fn fill_and_clear() {
        let mut s = Surface::new(3, 2).unwrap_or_else(unreachable_surface);
        s.fill_solid([10, 20, 30, 255]);
        assert_eq!(s.pixel(1, 1), Some([10, 20, 30, 255]));
        s.clear();
        assert_eq!(s.pixel(1, 1), Some([0, 0, 0, 0]));
    }

    #[test]
    fn blend_row_premul_math() {
        let mut s = Surface::new(2, 1).unwrap_or_else(unreachable_surface);
        let cov = [0.0_f32, 0.5];
        let red = [255_u8, 0, 0, 255]; // premul opaque red
        s.blend_row(0, &cov, red);
        assert_eq!(s.pixel(0, 0), Some([0, 0, 0, 0]));
        // 50 percent coverage over opaque red on black:
        // r = (255*128 + 0*127 + 127)/255 = 128
        assert_eq!(s.pixel(1, 0), Some([128, 0, 0, 128]));
    }

    #[test]
    fn blend_row_over_existing() {
        let mut s = Surface::new(1, 1).unwrap_or_else(unreachable_surface);
        s.set_pixel(0, 0, [100, 100, 100, 255]);
        let cov = [0.5_f32];
        let blue = [0_u8, 0, 255, 255];
        s.blend_row(0, &cov, blue);
        let p = s.pixel(0, 0).unwrap_or([0, 0, 0, 0]);
        // b = (255*128 + 100*127 + 127)/255 = 178
        assert_eq!(p[2], 178);
        assert_eq!(p[0], 50); // (0*128 + 100*127 + 127)/255 = 50
    }
}

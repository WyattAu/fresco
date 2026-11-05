//! Sparse-strip raster backend (vello_cpu 0.2), the second option of
//! roadmap Gate C.
//!
//! The CPU reference in [`crate::painter`] is an exact-coverage
//! winding-integral scanline rasterizer. This backend delegates the
//! same draw calls to vello_cpu's sparse-strip pipeline, so the
//! render-conformance parity harness can rasterize one fixture set
//! through both and compare. Swapping the primary rasterizer is then a
//! measured decision, not a rewrite (ADR-0004).
//!
//! Determinism posture: single-threaded dispatch, quality-first f32
//! rasterization, auto-detected SIMD level. The parity harness compares
//! both backends on the same host at the same time, so host-to-host
//! SIMD differences do not affect the verdict. vello_cpu's internal
//! unsafe is out of this crate; every entry point here stays under the
//! workspace `#![forbid(unsafe_code)]`.

use crate::{Rect, Surface};
use vello_cpu::{
    Level, Pixmap, RasterizerSettings, RenderContext, RenderMode, RenderSettings, Resources,
    kurbo::{Rect as KRect, Shape},
};

/// Quality-first f32 rasterization with single-threaded dispatch.
fn rasterizer_settings() -> RasterizerSettings {
    RasterizerSettings {
        render_mode: RenderMode::OptimizeQuality,
        ..RasterizerSettings::default()
    }
}

/// One render target on the sparse-strip pipeline.
///
/// Draw with [`StripTarget::fill_rect`], then read the composited
/// premultiplied RGBA8 bytes through [`StripTarget::composite`]. The
/// target starts fully transparent.
pub struct StripTarget {
    ctx: RenderContext,
    resources: Resources,
    pixmap: Pixmap,
    width: u16,
    height: u16,
}

impl StripTarget {
    /// A transparent target of `width` x `height` pixels. Returns
    /// `None` when either dimension exceeds `u16::MAX` (the vello_cpu
    /// coordinate limit) or is zero.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Option<Self> {
        let w = u16::try_from(width).ok()?;
        let h = u16::try_from(height).ok()?;
        if w == 0 || h == 0 {
            return None;
        }
        Some(StripTarget {
            ctx: RenderContext::new_with(
                w,
                h,
                RenderSettings {
                    level: Level::new(),
                    num_threads: 0,
                },
            ),
            resources: Resources::new(),
            pixmap: Pixmap::new(w, h),
            width: w,
            height: h,
        })
    }

    /// Pixel width.
    #[must_use]
    pub const fn width(&self) -> u16 {
        self.width
    }

    /// Pixel height.
    #[must_use]
    pub const fn height(&self) -> u16 {
        self.height
    }

    /// Fill an axis-aligned rect with a straight (non-premultiplied)
    /// RGBA8 color. Same draw-call shape as
    /// [`crate::Painter::fill_rect`].
    pub fn fill_rect(&mut self, rect: Rect, color: [u8; 4]) {
        self.ctx.set_paint(vello_cpu::color::AlphaColor::from_rgba8(
            color[0], color[1], color[2], color[3],
        ));
        self.ctx
            .fill_rect(&KRect::new(rect.x0, rect.y0, rect.x1, rect.y1));
    }

    /// Build vello color stops from straight RGBA8 pairs.
    fn vello_stops(stops: &[(f32, [u8; 4])]) -> vello_common::peniko::ColorStops {
        let mut out = vello_common::peniko::ColorStops::new();
        for (offset, color) in stops {
            out.push(vello_common::peniko::ColorStop::from((
                *offset,
                vello_cpu::color::AlphaColor::from_rgba8(color[0], color[1], color[2], color[3]),
            )));
        }
        out
    }

    /// Fill with a linear gradient along the `start` -> `end` axis in
    /// device pixels; stops are straight RGBA8 at offsets in `[0, 1]`
    /// and must be opaque (alpha 255) so both backends interpolate the
    /// same premultiplied values.
    pub fn fill_linear(
        &mut self,
        rect: Rect,
        start: (f64, f64),
        end: (f64, f64),
        stops: &[(f32, [u8; 4])],
    ) {
        let gradient = vello_common::peniko::Gradient {
            kind: vello_common::peniko::GradientKind::Linear(
                vello_common::peniko::LinearGradientPosition {
                    start: vello_cpu::kurbo::Point::new(start.0, start.1),
                    end: vello_cpu::kurbo::Point::new(end.0, end.1),
                },
            ),
            stops: Self::vello_stops(stops),
            extend: vello_common::peniko::Extend::Pad,
            ..Default::default()
        };
        self.ctx.set_paint(gradient);
        self.ctx
            .fill_rect(&KRect::new(rect.x0, rect.y0, rect.x1, rect.y1));
    }

    /// Fill with a radial gradient from `center` with `radius` in
    /// device pixels; same stop conventions as [`Self::fill_linear`].
    pub fn fill_radial(
        &mut self,
        rect: Rect,
        center: (f64, f64),
        radius: f64,
        stops: &[(f32, [u8; 4])],
    ) {
        let c = vello_cpu::kurbo::Point::new(center.0, center.1);
        let gradient = vello_common::peniko::Gradient {
            kind: vello_common::peniko::GradientKind::Radial(
                vello_common::peniko::RadialGradientPosition {
                    start_center: c,
                    start_radius: 0.0,
                    end_center: c,
                    end_radius: radius as f32,
                },
            ),
            stops: Self::vello_stops(stops),
            extend: vello_common::peniko::Extend::Pad,
            ..Default::default()
        };
        self.ctx.set_paint(gradient);
        self.ctx
            .fill_rect(&KRect::new(rect.x0, rect.y0, rect.x1, rect.y1));
    }

    /// Fill an axis-aligned rounded rect (uniform corner radius
    /// clamped to half the shorter side) with a straight RGBA8 color.
    /// Same geometry as [`crate::Painter::fill_round_rect`].
    pub fn fill_round_rect(&mut self, rect: Rect, radius: f64, color: [u8; 4]) {
        let r = radius.clamp(0.0, rect.width().min(rect.height()) * 0.5);
        let krect = KRect::new(rect.x0, rect.y0, rect.x1, rect.y1);
        let shape = vello_cpu::kurbo::RoundedRect::from_rect(krect, r);
        self.ctx.set_paint(vello_cpu::color::AlphaColor::from_rgba8(
            color[0], color[1], color[2], color[3],
        ));
        self.ctx.fill_path(&shape.to_path(0.1));
    }

    /// Flush the command stream and composite into the internal
    /// premultiplied RGBA8 buffer (replace mode: the pixmap is cleared
    /// and re-rendered from the full command stream, so repeated calls
    /// after more draws stay correct). Returns the composited bytes,
    /// row-major, stride = width * 4.
    pub fn composite(&mut self) -> &[u8] {
        self.ctx.flush();
        self.ctx
            .render_with(&mut self.pixmap, &mut self.resources, rasterizer_settings());
        self.pixmap.data_as_u8_slice()
    }
}

/// Copy a composited strip result into a stock [`Surface`], for
/// consumers that hand surfaces to the rest of the engine.
///
/// The surface must be tight-stride (`stride == width * 4`) and match
/// the target dimensions exactly; otherwise `None`.
#[must_use]
pub fn strip_into_surface(target: &mut StripTarget, surface: &mut Surface) -> Option<()> {
    let srect = surface.rect();
    if srect.width() != f64::from(target.width()) || srect.height() != f64::from(target.height()) {
        return None;
    }
    let bytes = target.composite().to_vec();
    let dst = surface.data_mut();
    if dst.len() != bytes.len() {
        return None;
    }
    for (d, s) in dst.iter_mut().zip(bytes.iter()) {
        *d = *s;
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_oversize_zero_and_tiny_ok() {
        assert!(StripTarget::new(0, 10).is_none());
        assert!(StripTarget::new(10, 0).is_none());
        assert!(StripTarget::new(u32::from(u16::MAX) + 1, 10).is_none());
        assert!(StripTarget::new(10, u32::from(u16::MAX) + 1).is_none());
        let t = StripTarget::new(16, 16);
        assert_eq!(t.as_ref().map(|t| t.width()), Some(16));
        assert_eq!(t.as_ref().map(|t| t.height()), Some(16));
    }

    #[test]
    fn transparent_start_then_opaque_fill_exact() {
        let Some(mut t) = StripTarget::new(8, 4) else {
            return;
        };
        let bytes = t.composite();
        assert!(
            bytes.iter().all(|b| *b == 0),
            "a fresh target composites transparent"
        );
        let full = Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 8.0,
            y1: 4.0,
        };
        t.fill_rect(full, [40, 80, 200, 255]);
        let bytes = t.composite();
        assert_eq!(bytes.len(), 8 * 4 * 4);
        for px in bytes.as_chunks::<4>().0 {
            assert_eq!(px, &[40, 80, 200, 255], "opaque fill must be exact");
        }
    }

    #[test]
    fn rounded_rect_covers_center_not_square_corner() {
        let Some(mut t) = StripTarget::new(20, 20) else {
            return;
        };
        t.fill_round_rect(
            Rect {
                x0: 0.0,
                y0: 0.0,
                x1: 20.0,
                y1: 20.0,
            },
            6.0,
            [10, 200, 90, 255],
        );
        let bytes = t.composite();
        let px = |x: usize, y: usize| -> Option<[u8; 4]> {
            let b = (y * 20 + x) * 4;
            bytes.get(b..b + 4).map(|quad| {
                let mut out = [0_u8; 4];
                for (slot, byte) in out.iter_mut().zip(quad) {
                    *slot = *byte;
                }
                out
            })
        };
        // Center is solidly covered.
        let c = px(10, 10).unwrap_or_default();
        assert_eq!(c, [10, 200, 90, 255], "center must be exact: {c:?}");
        // The square corner sits outside the rounded outline.
        let corner = px(0, 0).unwrap_or_default();
        assert_eq!(corner[3], 0, "square corner must be empty: {corner:?}");
    }

    #[test]
    fn semi_transparent_blend_is_premultiplied() {
        let Some(mut t) = StripTarget::new(4, 1) else {
            return;
        };
        t.fill_rect(
            Rect {
                x0: 0.0,
                y0: 0.0,
                x1: 4.0,
                y1: 1.0,
            },
            [255, 0, 0, 128],
        );
        // Straight (255,0,0,128) premultiplied is (128,0,0,128); the
        // composite of a partially covered row carries reduced coverage
        // on the outer columns.
        let bytes = t.composite();
        let center: Vec<u8> = bytes.iter().skip(4).take(4).copied().collect();
        assert_eq!(center.get(1), Some(&0), "green channel stays zero");
        assert!(
            center.first().is_some_and(|r| *r <= 128),
            "red premultiplies down, got {:?}",
            center.first()
        );
        assert_eq!(
            center.get(3),
            Some(&128),
            "alpha is preserved at full coverage"
        );
    }
}

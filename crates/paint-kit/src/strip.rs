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

/// Effective clip state for the next [`StripTarget`] draw.
enum ClipState {
    /// No clip: draws are unconstrained.
    None,
    /// Clip empties the region: draws are suppressed.
    Empty,
    /// Intersected active clip rect.
    Rect(Rect),
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
    clips: Vec<Option<Rect>>,
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
            clips: Vec::new(),
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

    /// Push an axis-aligned clip that intersects the active clip.
    /// Every draw is confined to the active clip until [`Self::pop_clip`]
    /// (implemented with vello clip layers per draw, which matches the
    /// CPU painter's intersecting scissor semantics).
    pub fn push_clip(&mut self, rect: Rect) {
        // A `None` entry on the stack means an EMPTY clip (draws
        // suppressed), not the absence of clipping.
        let next = match self.clips.last() {
            Some(None) => None,
            Some(Some(active)) => active.intersect(rect),
            None => Some(rect),
        };
        self.clips.push(next);
    }

    /// Pop the most recent clip.
    pub fn pop_clip(&mut self) {
        self.clips.pop();
    }

    /// The effective clip state for the next draw.
    fn active_clip(&self) -> ClipState {
        match self.clips.last() {
            // No clip pushed: draw unconstrained.
            None => ClipState::None,
            // Empty clip: suppress the draw entirely.
            Some(None) => ClipState::Empty,
            Some(Some(clip)) => ClipState::Rect(*clip),
        }
    }

    /// Intersect a draw rect with the active clip. Axis-aligned rect
    /// draws scissor exactly by shrinking the rect: the gradient or
    /// solid stays in device space, and coverage is identical to a
    /// clip layer without one. `None` = fully clipped away.
    fn scissored(&self, rect: Rect) -> Option<Rect> {
        match self.active_clip() {
            ClipState::None => Some(rect),
            ClipState::Empty => None,
            ClipState::Rect(clip) => clip.intersect(rect),
        }
    }

    fn rect_contains(outer: Rect, inner: Rect) -> bool {
        outer.x0 <= inner.x0 && outer.y0 <= inner.y0 && outer.x1 >= inner.x1 && outer.y1 >= inner.y1
    }

    /// Fill an axis-aligned rect with a straight (non-premultiplied)
    /// RGBA8 color. Same draw-call shape as
    /// [`crate::Painter::fill_rect`].
    pub fn fill_rect(&mut self, rect: Rect, color: [u8; 4]) {
        let Some(rect) = self.scissored(rect) else {
            return;
        };
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
        let Some(rect) = self.scissored(rect) else {
            return;
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
        let Some(rect) = self.scissored(rect) else {
            return;
        };
        self.ctx.set_paint(gradient);
        self.ctx
            .fill_rect(&KRect::new(rect.x0, rect.y0, rect.x1, rect.y1));
    }

    /// Fill with an angular sweep around `center` from `start_angle`
    /// to `end_angle` (radians from +X, clockwise in Y-down); same
    /// stop conventions as [`Self::fill_linear`].
    pub fn fill_sweep(
        &mut self,
        rect: Rect,
        center: (f64, f64),
        start_angle: f32,
        end_angle: f32,
        stops: &[(f32, [u8; 4])],
    ) {
        let c = vello_cpu::kurbo::Point::new(center.0, center.1);
        let gradient = vello_common::peniko::Gradient {
            kind: vello_common::peniko::GradientKind::Sweep(
                vello_common::peniko::SweepGradientPosition::new(c, start_angle, end_angle),
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
        // A rounded rect's corners can be cut by a clip, so rect
        // intersection is not enough when the clip is partial: draw
        // under a clip layer in that case, directly when the clip
        // contains the shape, and not at all when it is disjoint.
        let r = radius.clamp(0.0, rect.width().min(rect.height()) * 0.5);
        let krect = KRect::new(rect.x0, rect.y0, rect.x1, rect.y1);
        let shape = vello_cpu::kurbo::RoundedRect::from_rect(krect, r);
        let path = shape.to_path(0.1);
        match self.active_clip() {
            ClipState::Empty => {}
            ClipState::None => {
                self.ctx.set_paint(vello_cpu::color::AlphaColor::from_rgba8(
                    color[0], color[1], color[2], color[3],
                ));
                self.ctx.fill_path(&path);
            }
            ClipState::Rect(clip) => {
                if Self::rect_contains(clip, rect) {
                    self.ctx.set_paint(vello_cpu::color::AlphaColor::from_rgba8(
                        color[0], color[1], color[2], color[3],
                    ));
                    self.ctx.fill_path(&path);
                } else if clip.intersect(rect).is_some() {
                    let clip_path = KRect::new(clip.x0, clip.y0, clip.x1, clip.y1).to_path(0.1);
                    self.ctx.push_clip_path(&clip_path);
                    self.ctx.set_paint(vello_cpu::color::AlphaColor::from_rgba8(
                        color[0], color[1], color[2], color[3],
                    ));
                    self.ctx.fill_path(&path);
                    self.ctx.pop_clip_path();
                }
            }
        }
    }

    /// Fill a soft drop shadow under an axis-aligned rounded rect
    /// using vello's native analytic gaussian
    /// (`fill_blurred_rounded_rect`). Same extent convention as
    /// [`crate::Painter::fill_shadow_round_rect`]: the shadow extends
    /// ~3σ beyond the rect. Note the edge behavior differs from the
    /// CPU reference (vello's convolution gives 50% coverage at the
    /// silhouette edge; the CPU SDF starts at 100%), so shadows are
    /// deliberately excluded from the per-pixel parity gate and
    /// verified structurally instead.
    pub fn fill_shadow_round_rect(&mut self, rect: Rect, radius: f64, sigma: f64, color: [u8; 4]) {
        let r = radius.clamp(0.0, rect.width().min(rect.height()) * 0.5) as f32;
        let sigma = sigma.max(0.01) as f32;
        let krect = KRect::new(rect.x0, rect.y0, rect.x1, rect.y1);
        let paint =
            vello_cpu::color::AlphaColor::from_rgba8(color[0], color[1], color[2], color[3]);
        let path = krect.to_path(0.1);
        match self.active_clip() {
            ClipState::Empty => {}
            ClipState::None => {
                self.ctx.set_paint(paint);
                self.ctx.fill_blurred_rounded_rect(&krect, r, sigma, false);
            }
            ClipState::Rect(clip) => {
                if Self::rect_contains(clip, rect) {
                    self.ctx.set_paint(paint);
                    self.ctx.fill_blurred_rounded_rect(&krect, r, sigma, false);
                } else if clip.intersect(rect).is_some() {
                    self.ctx.push_clip_path(&path);
                    self.ctx.set_paint(paint);
                    self.ctx.fill_blurred_rounded_rect(&krect, r, sigma, false);
                    self.ctx.pop_clip_path();
                }
            }
        }
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
    fn clip_confines_draws_to_the_region() {
        let Some(mut t) = StripTarget::new(20, 4) else {
            return;
        };
        t.push_clip(Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 10.0,
            y1: 4.0,
        });
        t.fill_rect(
            Rect {
                x0: 0.0,
                y0: 0.0,
                x1: 20.0,
                y1: 4.0,
            },
            [10, 200, 90, 255],
        );
        t.pop_clip();
        let bytes = t.composite();
        let inside: Vec<u8> = bytes.iter().take(4).copied().collect();
        assert_eq!(
            inside,
            [10, 200, 90, 255],
            "inside the clip must draw: {inside:?}"
        );
        let outside: Vec<u8> = bytes.iter().skip(12 * 4).take(4).copied().collect();
        assert!(
            outside.iter().all(|b| *b == 0),
            "outside the clip must stay transparent: {outside:?}"
        );
    }

    #[test]
    fn disjoint_clip_makes_draws_no_op() {
        let Some(mut t) = StripTarget::new(4, 4) else {
            return;
        };
        t.push_clip(Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 2.0,
            y1: 2.0,
        });
        // Intersecting with a disjoint rect yields an empty clip.
        t.push_clip(Rect {
            x0: 3.0,
            y0: 3.0,
            x1: 4.0,
            y1: 4.0,
        });
        t.fill_rect(
            Rect {
                x0: 0.0,
                y0: 0.0,
                x1: 4.0,
                y1: 4.0,
            },
            [255, 0, 0, 255],
        );
        t.pop_clip();
        t.pop_clip();
        let bytes = t.composite();
        assert!(
            bytes.iter().all(|b| *b == 0),
            "an empty clip must suppress the draw"
        );
    }

    #[test]
    fn shadow_falls_off_outside_the_rect() {
        let Some(mut t) = StripTarget::new(80, 40) else {
            return;
        };
        // Shadow of a centered card, sigma = 4.
        t.fill_shadow_round_rect(
            Rect {
                x0: 20.0,
                y0: 10.0,
                x1: 60.0,
                y1: 30.0,
            },
            8.0,
            4.0,
            [0, 0, 0, 255],
        );
        let bytes = t.composite();
        let alpha = |x: usize, y: usize| -> u8 {
            let base = (y * 80 + x) * 4 + 3;
            bytes.get(base).copied().unwrap_or(0)
        };
        // Just outside the right edge: the falloff is present.
        let near = alpha(66, 20);
        assert!(near > 0, "falloff must be visible near the edge");
        // Far outside (4 sigma past the edge at 2.5-sigma vello cutoff):
        // fully transparent.
        let far = alpha(78, 20);
        assert_eq!(far, 0, "beyond the cutoff the shadow is empty: {far}");
        // Monotonic decrease moving away from the edge.
        let a1 = alpha(62, 20);
        let a2 = alpha(64, 20);
        assert!(a1 >= a2, "falloff must decrease outward: {a1} -> {a2}");
    }

    #[test]
    fn shadow_center_is_opaque() {
        let Some(mut t) = StripTarget::new(40, 40) else {
            return;
        };
        t.fill_shadow_round_rect(
            Rect {
                x0: 5.0,
                y0: 5.0,
                x1: 35.0,
                y1: 35.0,
            },
            6.0,
            3.0,
            [50, 60, 70, 255],
        );
        let bytes = t.composite();
        let base = (20 * 40 + 20) * 4 + 3;
        let a = bytes.get(base).copied().unwrap_or(0);
        assert_eq!(a, 255, "shadow center must be fully opaque: {a}");
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

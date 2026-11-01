//! CPU rasterization for the fresco UI engine.
//!
//! L2 layer: std allowed; depends on `path-kit` only (SPEC section 3).
//!
//! Coverage model: the winding integral over each pixel column, evaluated
//! exactly between y-events (paradigm 928, exact-coverage anti-aliasing,
//! the `font-shape` lineage). No supersampling. Exact for every region
//! with `|winding| <= 1`, which covers all ordinary UI shapes; heavier
//! overlaps clamp (documented limitation, README).
//!
//! Even-odd fill reduces to nonzero via parity-depth orientation flip, so
//! one exact code path serves both rules (REQ-R2).
//!
//! Surfaces are premultiplied RGBA8. The CPU backend is the reference
//! implementation: headless by construction, it backs `render-conformance`
//! golden images and any future GPU backend must match it (ADR-0004).
#![forbid(unsafe_code)]

pub mod painter;
pub mod raster;
pub mod surface;

pub use painter::{Painter, Solid};
pub use raster::{Edge, rasterize_polys};
pub use surface::Surface;

pub use path_kit::{Affine, FillRule, Path, Rect, Vec2};

/// Convert straight-alpha sRGBA bytes to premultiplied.
#[must_use]
pub const fn premultiply(rgba: [u8; 4]) -> [u8; 4] {
    let a = rgba[3] as u16;
    let r = ((rgba[0] as u16 * a + 127) / 255) as u8;
    let g = ((rgba[1] as u16 * a + 127) / 255) as u8;
    let b = ((rgba[2] as u16 * a + 127) / 255) as u8;
    [r, g, b, rgba[3]]
}

//! fx: the solid-fx and plychart capability catalog for the fresco UI
//! engine.
//!
//! A widget library porting the visual capability of the estate's
//! `solid-fx` component registry (218 animated SolidJS components:
//! TextAnimations, Animations, Backgrounds, Components, Micro) and the
//! `plychart` WASM charting library (22 chart types) into fresco's
//! retained paint model.
//!
//! Design:
//!
//! - every component is a plain struct of parameters with a
//!   `draw(&mut Surface, &mut Painter, t)` method; `t` is the
//!   animation clock in seconds;
//! - everything is deterministic: randomness comes from a seeded
//!   [`rand::FxRand`], never from the OS or the wall clock;
//! - text renders through text-kit's 3x5 micro-font (the same
//!   hermetic glyphs widget-kit uses);
//! - charts port plychart's geometry conventions (linear value
//!   normalization, pad, area fills) onto paint-kit's exact-coverage
//!   rasterizer.
//!
//! Catalog status (families shipped vs target):
//!
//! | Family | Target | Shipped |
//! |---|---|---|
//! | backgrounds (solid-fx Backgrounds) | 64 | grid, dots, flicker |
//! | text effects (TextAnimations) | 33 | count-up, scramble, shimmer |
//! | charts (plychart) | 22 | sparkline, line, bars |
//!
//! Remaining families land in subsequent releases following the same
//! pattern: parameter struct, deterministic draw, unit tests, gallery
//! entry.
#![forbid(unsafe_code)]

pub mod backgrounds;
pub mod rand;
pub mod text;

pub use backgrounds::{
    Beams, DotPattern, FlickeringGrid, GradientMesh, GridPattern, NoiseField, Particles, StarField,
    Waves,
};
pub use text::{CountUp, ScrambledText, ShimmerText};

pub use rand::FxRand;

use paint_kit::{Painter, Surface};

/// Draw context handed to every fx component: the surface, a painter
/// with reusable scratch, and the animation clock.
pub struct Fx<'a> {
    /// Target surface.
    pub surface: &'a mut Surface,
    /// Painter scratch (reused across frames).
    pub painter: &'a mut Painter,
    /// Animation clock in seconds.
    pub t: f64,
}

impl<'a> Fx<'a> {
    /// New draw context.
    #[must_use]
    pub fn new(surface: &'a mut Surface, painter: &'a mut Painter, t: f64) -> Self {
        Fx {
            surface,
            painter,
            t,
        }
    }
}

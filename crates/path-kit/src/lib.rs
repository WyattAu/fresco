//! 2D geometry for the fresco UI engine.
//!
//! L0 layer: std, zero dependencies, `f64` precision,
//! `#![forbid(unsafe_code)]`. Owns the math the rest of fresco builds on:
//!
//! - Vectors, rects, affine transforms ([`geom`])
//! - Quadratic and cubic beziers with Legendre-Gauss arc length ([`bez`])
//! - Adaptive flattening under a caller tolerance ([`flatten`])
//! - Path construction and flattening to polylines ([`path`])
//! - Polyline stroking with joins and caps ([`stroke`])
//! - Dashing by arc length ([`dash`])
//! - Robust orientation predicates ([`pred`])
//! - Winding-number hit testing ([`hit`])
//!
//! Flattening guarantee: because the true curve lies inside the convex hull
//! of its control points, subdividing while the control points are farther
//! than `tol` from their chord keeps every curve point within `tol` of the
//! emitted polyline (REQ-P1).

#![forbid(unsafe_code)]

pub mod arc;
pub mod bez;
pub mod dash;
pub mod flatten;
pub mod geom;
pub mod hit;
pub mod offset;
pub mod path;
pub mod pred;
pub mod stroke;

pub use bez::{CubicBez, QuadBez};
pub use geom::{Affine, Rect, Vec2};
pub use hit::{FillRule, point_in_path, winding_number};
pub use path::{Path, PathSeg, Poly};
pub use pred::orient2d;
pub use stroke::{Cap, Join, stroke_poly};

/// Maximum recursion depth for adaptive subdivision. Bounds worst-case
/// work; tolerances small enough to hit the cap are rejected by tests.
pub(crate) const MAX_DEPTH: u32 = 32;

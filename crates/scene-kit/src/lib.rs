//! Retained scene graph for the fresco UI engine.
//!
//! L1 layer: depends on `path-kit` only (SPEC section 3).
//!
//! - [`arena`]: generational arena, stable handles, ABA-safe reuse
//!   (REQ-S1).
//! - [`Scene`]: node tree with transforms, layers, draw payloads.
//! - [`damage`]: rectangle damage accumulation and frame plans (REQ-S2).
//! - [`flatten`]: draw-list flattening sorted by `layer:depth:seq` keys
//!   (REQ-S3, paradigm 930).
//!
//! The scene model rationale is ADR-0001: retained graph plus damage as
//! data; reactive and immediate-mode layers sit above this, not instead
//! of it.

#![forbid(unsafe_code)]

pub mod arena;
pub mod camera;
pub mod damage;
pub mod export;
pub mod flatten;
pub mod history;
pub mod pick;
pub mod scene;

pub use arena::{Arena, Id};
pub use camera::Camera;
pub use damage::{DamageTracker, FramePlan};
pub use flatten::{Command, DrawList, SortKey, flatten_scene};
pub use scene::{Brush, Draw, Node, Scene};

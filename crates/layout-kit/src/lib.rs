//! Incremental layout for the fresco UI engine.
//!
//! L1 layer: `no_std + alloc`, zero dependencies (REQ-L1, REQ-L2).
//!
//! Two pieces:
//!
//! - [`MeasureCache`]: memoized layout measurements keyed by node and an
//!   input hash, invalidated per-node or globally (paradigm 871: dirty-bit
//!   DAG semantics without a full framework).
//! - [`flex`]: the single-axis flex distribution solver (grow, shrink,
//!   gap, min/max clamps) that consumes those measurements.
//!
//! The cache is deliberately generic over the caller's node key type so
//! both the scene graph and future widget trees can share it.
#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod cache;
pub mod engine;
pub mod flex;
pub mod flow;

pub use cache::MeasureCache;
pub use engine::{LayoutEngine, LayoutError, NodeStyle, Rect2D, Size2D};
pub use flex::{Axis, FlexItem, solve_flex};
pub use flow::FlowEngine;

#[cfg(feature = "taffy")]
pub mod taffy_backend;
#[cfg(feature = "taffy")]
pub use taffy_backend::TaffyEngine;

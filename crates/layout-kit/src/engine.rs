//! The engine-agnostic layout contract.
//!
//! Widget code targets [`LayoutEngine`]; backends are the estate's own
//! flow solver (no_std, zero-dep) and Taffy (feature `taffy`, Gate B).
//! This is the insurance pattern from ADR-0002 applied to layout: the
//! contract is ours, the mass is theirs.

use crate::Axis;

/// Two-dimensional size in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Size2D {
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
}

/// Computed rectangle in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Rect2D {
    /// Left.
    pub x: f64,
    /// Top.
    pub y: f64,
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
}

/// Node layout style (the subset the facade supports; engines map it to
/// their native style models).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodeStyle {
    /// Fixed width; `None` lets the engine derive it.
    pub width: Option<f64>,
    /// Fixed height; `None` lets the engine derive it.
    pub height: Option<f64>,
    /// Fraction of free main-axis space to absorb (flex grow).
    pub grow: f64,
    /// Main-axis direction for containers.
    pub direction: Axis,
    /// Gap between children, main axis.
    pub gap: f64,
}

impl Default for NodeStyle {
    fn default() -> Self {
        NodeStyle {
            width: None,
            height: None,
            grow: 0.0,
            direction: Axis::Row,
            gap: 0.0,
        }
    }
}

impl NodeStyle {
    /// Container in the given direction with a gap.
    #[must_use]
    pub fn container(direction: Axis, gap: f64) -> Self {
        NodeStyle {
            direction,
            gap,
            ..NodeStyle::default()
        }
    }

    /// Leaf with a fixed size.
    #[must_use]
    pub fn fixed(width: f64, height: f64) -> Self {
        NodeStyle {
            width: Some(width),
            height: Some(height),
            ..NodeStyle::default()
        }
    }
}

/// Layout computation failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutError {
    /// The root handle is not known to the engine.
    UnknownRoot,
    /// The backend rejected the input.
    Backend,
}

/// Engine-agnostic layout contract.
pub trait LayoutEngine {
    /// The engine's node handle.
    type Key: Copy;

    /// Add a leaf node.
    fn add_leaf(&mut self, style: NodeStyle) -> Self::Key;

    /// Add a container node with children.
    fn add_container(&mut self, style: NodeStyle, children: &[Self::Key]) -> Self::Key;

    /// Compute the layout of `root` inside `available`.
    fn compute(&mut self, root: Self::Key, available: Size2D) -> Result<(), LayoutError>;

    /// Computed rectangle of a node (relative to the root's origin).
    fn rect(&self, node: Self::Key) -> Option<Rect2D>;
}

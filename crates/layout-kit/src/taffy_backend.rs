//! Taffy backend for the `LayoutEngine` contract (feature `taffy`).
//!
//! Taffy provides CSS flexbox and grid (Gate B adoption: measured on
//! kestrel screens before it becomes the default). This adapter maps the
//! estate `NodeStyle` subset to Taffy styles and reads rects back.

use crate::Axis;
use crate::engine::{LayoutEngine, LayoutError, NodeStyle, Rect2D, Size2D};
use alloc::vec::Vec;

use taffy::TaffyTree;
use taffy::prelude::*;

/// Taffy-backed engine.
pub struct TaffyEngine {
    tree: TaffyTree,
    nodes: Vec<taffy::NodeId>,
}

impl Default for TaffyEngine {
    fn default() -> Self {
        TaffyEngine {
            tree: TaffyTree::new(),
            nodes: Vec::new(),
        }
    }
}

impl TaffyEngine {
    /// New empty engine.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

fn to_taffy_style(style: &NodeStyle) -> Style {
    Style {
        size: Size {
            width: match style.width {
                Some(w) => length(w as f32),
                None => auto(),
            },
            height: match style.height {
                Some(h) => length(h as f32),
                None => auto(),
            },
        },
        flex_grow: style.grow as f32,
        flex_direction: match style.direction {
            Axis::Row => FlexDirection::Row,
            Axis::Column => FlexDirection::Column,
        },
        gap: Size {
            width: length(style.gap as f32),
            height: length(style.gap as f32),
        },
        ..Style::default()
    }
}

impl LayoutEngine for TaffyEngine {
    type Key = usize;

    fn add_leaf(&mut self, style: NodeStyle) -> usize {
        let node = match self.tree.new_leaf(to_taffy_style(&style)) {
            Ok(n) => n,
            Err(_) => return self.nodes.len(),
        };
        self.nodes.push(node);
        self.nodes.len() - 1
    }

    fn add_container(&mut self, style: NodeStyle, children: &[usize]) -> usize {
        let child_ids: Vec<taffy::NodeId> = children
            .iter()
            .filter_map(|c| self.nodes.get(*c).copied())
            .collect();
        let node = match self
            .tree
            .new_with_children(to_taffy_style(&style), &child_ids)
        {
            Ok(n) => n,
            Err(_) => return self.nodes.len(),
        };
        self.nodes.push(node);
        self.nodes.len() - 1
    }

    fn compute(&mut self, root: usize, available: Size2D) -> Result<(), LayoutError> {
        let Some(&root_node) = self.nodes.get(root) else {
            return Err(LayoutError::UnknownRoot);
        };
        let size = Size {
            width: length(available.width as f32),
            height: length(available.height as f32),
        };
        self.tree
            .compute_layout(root_node, size)
            .map_err(|_| LayoutError::Backend)
    }

    fn rect(&self, node: usize) -> Option<Rect2D> {
        let &id = self.nodes.get(node)?;
        let layout = self.tree.layout(id).ok()?;
        Some(Rect2D {
            x: f64::from(layout.location.x),
            y: f64::from(layout.location.y),
            width: f64::from(layout.size.width),
            height: f64::from(layout.size.height),
        })
    }
}

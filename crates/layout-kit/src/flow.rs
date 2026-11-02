//! The estate's own flow engine: rows and columns with gaps and grow,
//! single container level per node, recursive.
//!
//! `no_std`-compatible and dependency-free; this is the correctness
//! reference the Taffy backend is gated against (Gate B).

use crate::engine::{LayoutEngine, LayoutError, NodeStyle, Rect2D, Size2D};
use crate::{Axis, FlexItem, solve_flex};
use alloc::vec::Vec;

#[derive(Clone, Debug)]
struct FlowNode {
    style: NodeStyle,
    children: Vec<usize>,
    rect: Rect2D,
    parent: Option<usize>,
}

impl FlowNode {
    fn direction(&self) -> Axis {
        self.style.direction
    }
}

/// The estate flow engine.
#[derive(Clone, Debug, Default)]
pub struct FlowEngine {
    nodes: Vec<FlowNode>,
}

impl FlowEngine {
    /// New empty engine.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl LayoutEngine for FlowEngine {
    type Key = usize;

    fn add_leaf(&mut self, style: NodeStyle) -> usize {
        self.nodes.push(FlowNode {
            style,
            children: Vec::new(),
            rect: Rect2D::default(),
            parent: None,
        });
        self.nodes.len() - 1
    }

    fn add_container(&mut self, style: NodeStyle, children: &[usize]) -> usize {
        let idx = self.add_leaf(style);
        if let Some(node) = self.nodes.get_mut(idx) {
            node.children = children.to_vec();
        }
        for c in children {
            if let Some(child) = self.nodes.get_mut(*c) {
                child.parent = Some(idx);
            }
        }
        idx
    }

    fn compute(&mut self, root: usize, available: Size2D) -> Result<(), LayoutError> {
        let Some(root_node) = self.nodes.get(root) else {
            return Err(LayoutError::UnknownRoot);
        };
        let width = root_node.style.width.unwrap_or(available.width);
        let height = root_node.style.height.unwrap_or(available.height);
        if let Some(node) = self.nodes.get_mut(root) {
            node.rect = Rect2D {
                x: 0.0,
                y: 0.0,
                width,
                height,
            };
        }
        self.layout_children(root);
        Ok(())
    }

    fn rect(&self, node: usize) -> Option<Rect2D> {
        self.nodes.get(node).map(|n| n.rect)
    }
}

impl FlowEngine {
    /// Extent of `node` measured along `direction` (its main size if it
    /// flows in `direction`, else its cross size), content-derived.
    fn extent(&self, node: usize, direction: Axis) -> f64 {
        let Some(n) = self.nodes.get(node) else {
            return 0.0;
        };
        let fixed = match direction {
            Axis::Row => n.style.width,
            Axis::Column => n.style.height,
        };
        if let Some(f) = fixed {
            return f;
        }
        if n.direction() == direction {
            self.content_main(node, direction)
        } else {
            self.content_cross(node, direction).unwrap_or(0.0)
        }
    }

    /// Main-axis content size of a container flowing in `direction`:
    /// the sum of child extents plus gaps. Leaves return 0 (they stretch
    /// or grow instead).
    fn content_main(&self, node: usize, direction: Axis) -> f64 {
        let Some(n) = self.nodes.get(node) else {
            return 0.0;
        };
        if n.children.is_empty() {
            return 0.0;
        }
        let count = n.children.len();
        let sum: f64 = n.children.iter().map(|c| self.extent(*c, direction)).sum();
        sum + n.style.gap * (count.saturating_sub(1)) as f64
    }

    /// Cross-axis content size of a container measured along
    /// `direction`: the max child extent (recursive). Unused after the
    /// extent() unification; kept for reference tests. Returns `None`
    /// for leaves.
    #[allow(dead_code)]
    fn content_cross(&self, node: usize, direction: Axis) -> Option<f64> {
        let n = self.nodes.get(node)?;
        if n.children.is_empty() {
            return None;
        }
        let mut best: Option<f64> = None;
        for c in &n.children {
            let extent = self.extent(*c, direction);
            best = Some(best.map_or(extent, |b: f64| b.max(extent)));
        }
        best
    }

    /// Lay out the children of `parent` inside its rect, recursively.    /// Lay out the children of `parent` inside its rect, recursively.
    fn layout_children(&mut self, parent: usize) {
        let (direction, gap, children) = {
            let Some(node) = self.nodes.get(parent) else {
                return;
            };
            (node.style.direction, node.style.gap, node.children.clone())
        };
        if children.is_empty() {
            return;
        }
        let parent_rect = match self.nodes.get(parent) {
            Some(n) => n.rect,
            None => return,
        };

        // Measure children on the main axis (fixed sizes only; grows
        // absorb the remainder through the flex solver).
        let items: Vec<FlexItem> = children
            .iter()
            .map(|c| {
                let s = match self.nodes.get(*c) {
                    Some(n) => &n.style,
                    None => return FlexItem::new(0.0),
                };
                let basis = match direction {
                    Axis::Row => s.width.unwrap_or_else(|| self.extent(*c, Axis::Row)),
                    Axis::Column => s.height.unwrap_or_else(|| self.extent(*c, Axis::Column)),
                };
                FlexItem {
                    basis,
                    grow: s.grow,
                    shrink: 0.0,
                    min: None,
                    max: None,
                }
            })
            .collect();

        let main_size = match direction {
            Axis::Row => parent_rect.width,
            Axis::Column => parent_rect.height,
        };
        let sizes = solve_flex(main_size, gap, &items);
        let _ = gap;

        // Cross size: fixed wins, then container content cross, then
        // leaves stretch to the parent cross.
        let mut offset = 0.0_f64;
        for (i, child) in children.iter().enumerate() {
            let main = match sizes.get(i) {
                Some(s) => *s,
                None => 0.0,
            };
            let perp = match direction {
                Axis::Row => Axis::Column,
                Axis::Column => Axis::Row,
            };
            let is_leaf = self
                .nodes
                .get(*child)
                .map(|n| n.children.is_empty())
                .unwrap_or(true);
            let fixed_cross = self.nodes.get(*child).and_then(|n| match direction {
                Axis::Row => n.style.height,
                Axis::Column => n.style.width,
            });
            // Leaves stretch to the parent cross; containers take their
            // content extent along the perpendicular.
            let cross = match fixed_cross {
                Some(f) => f,
                None if is_leaf => match direction {
                    Axis::Row => parent_rect.height,
                    Axis::Column => parent_rect.width,
                },
                None => self.extent(*child, perp),
            };
            let Some(node) = self.nodes.get_mut(*child) else {
                continue;
            };
            let (x, y, w, h) = match direction {
                Axis::Row => (parent_rect.x + offset, parent_rect.y, main, cross),
                Axis::Column => (parent_rect.x, parent_rect.y + offset, cross, main),
            };
            node.rect = Rect2D {
                x,
                y,
                width: w,
                height: h,
            };
            offset += main + gap;
        }
        // Recurse (collected before mutation to satisfy the borrow).
        let with_children: Vec<usize> = children
            .iter()
            .filter(|c| {
                self.nodes
                    .get(**c)
                    .map(|n| !n.children.is_empty())
                    .unwrap_or(false)
            })
            .copied()
            .collect();
        for c in with_children {
            self.layout_children(c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-9;

    #[test]
    fn column_children_stack_with_gap() {
        let mut engine = FlowEngine::new();
        let a = engine.add_leaf(NodeStyle::fixed(200.0, 40.0));
        let b = engine.add_leaf(NodeStyle::fixed(200.0, 40.0));
        let root = engine.add_container(NodeStyle::container(Axis::Column, 10.0), &[a, b]);
        engine
            .compute(
                root,
                Size2D {
                    width: 200.0,
                    height: 100.0,
                },
            )
            .unwrap_or(());
        let ra = engine.rect(a).unwrap_or_default();
        let rb = engine.rect(b).unwrap_or_default();
        assert!((ra.y - 0.0).abs() < EPS);
        assert!((rb.y - 50.0).abs() < EPS, "gap 10 between 40s: {}", rb.y);
        assert!((rb.height - 40.0).abs() < EPS);
    }

    #[test]
    fn row_grow_distributes_free_space() {
        let mut engine = FlowEngine::new();
        let a = engine.add_leaf(NodeStyle {
            grow: 1.0,
            height: Some(30.0),
            ..NodeStyle::default()
        });
        let b = engine.add_leaf(NodeStyle::fixed(80.0, 30.0));
        let root = engine.add_container(NodeStyle::container(Axis::Row, 0.0), &[a, b]);
        engine
            .compute(
                root,
                Size2D {
                    width: 200.0,
                    height: 30.0,
                },
            )
            .unwrap_or(());
        let ra = engine.rect(a).unwrap_or_default();
        assert!(
            (ra.width - 120.0).abs() < EPS,
            "grow absorbs 200-80: {}",
            ra.width
        );
        let rb = engine.rect(b).unwrap_or_default();
        assert!((rb.x - 120.0).abs() < EPS);
    }

    #[test]
    fn nested_containers_compose() {
        let mut engine = FlowEngine::new();
        let label = engine.add_leaf(NodeStyle::fixed(100.0, 20.0));
        let extra = engine.add_leaf(NodeStyle::fixed(50.0, 20.0));
        let row = engine.add_container(NodeStyle::container(Axis::Row, 8.0), &[label, extra]);
        let root = engine.add_container(NodeStyle::container(Axis::Column, 5.0), &[row]);
        assert!(
            engine
                .compute(
                    root,
                    Size2D {
                        width: 300.0,
                        height: 100.0
                    }
                )
                .is_ok()
        );
        let rrow = engine.rect(row).unwrap_or_default();
        assert!((rrow.height - 20.0).abs() < EPS, "row cross = child height");
        let rlabel = engine.rect(label).unwrap_or_default();
        assert!((rlabel.x - 0.0).abs() < EPS);
        assert!((rlabel.y - 0.0).abs() < EPS);
    }

    #[test]
    fn unknown_root_is_error() {
        let mut engine = FlowEngine::new();
        assert_eq!(
            engine.compute(
                42,
                Size2D {
                    width: 10.0,
                    height: 10.0
                }
            ),
            Err(LayoutError::UnknownRoot)
        );
    }
}

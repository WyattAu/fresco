//! Scene flattening into a sorted draw list.

use crate::arena::Id;
use crate::damage::FramePlan;
use crate::scene::{Brush, Draw, Scene};
use path_kit::{Affine, FillRule};
use std::vec::Vec;

/// Draw ordering key packing `layer:16 | depth:16 | seq:32` into a u64 so
/// ordering the frame is a single sort over contiguous keys (paradigm 930).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SortKey(u64);

impl SortKey {
    /// Pack a key. `seq` wraps at u32 range; stable within a frame.
    #[must_use]
    pub const fn new(layer: u16, depth: u16, seq: u32) -> Self {
        SortKey(((layer as u64) << 48) | ((depth as u64) << 32) | seq as u64)
    }

    /// Unpacked layer.
    #[must_use]
    pub const fn layer(self) -> u16 {
        (self.0 >> 48) as u16
    }

    /// Unpacked depth.
    #[must_use]
    pub const fn depth(self) -> u16 {
        (self.0 >> 32) as u16
    }

    /// Unpacked sequence.
    #[must_use]
    pub const fn seq(self) -> u32 {
        self.0 as u32
    }
}

/// One flattened draw command in global space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    /// Fill a stored path under a transform.
    FillPath {
        /// Path store index.
        path: u32,
        /// Fill paint.
        brush: Brush,
        /// Fill rule.
        fill: FillRule,
        /// Global transform (root-to-node composition).
        transform: Affine,
    },
}

/// Reusable draw list. Callers keep one instance per frame loop and call
/// [`DrawList::clear`] between frames; capacity is retained (zero-alloc
/// steady state, invariant 3 in `README.md`).
#[derive(Debug, Default)]
pub struct DrawList {
    items: Vec<(SortKey, Command)>,
}

impl DrawList {
    /// New empty list.
    #[must_use]
    pub const fn new() -> Self {
        DrawList { items: Vec::new() }
    }

    /// Sorted draw items.
    #[must_use]
    pub fn items(&self) -> &[(SortKey, Command)] {
        &self.items
    }

    /// Clear for reuse, retaining capacity.
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Item count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// True when empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// Flatten `scene` into `out`, limited to `plan` regions when given.
///
/// Nodes are skipped when invisible or when their draw bounds miss the
/// region rects. Depth is capped at `u16::MAX` (deeper nodes saturate and
/// still draw, ordered by sequence number).
pub fn flatten_scene(scene: &Scene, plan: &FramePlan, out: &mut DrawList) {
    out.clear();
    let Some(root) = scene.root() else {
        return;
    };
    let mut seq: u32 = 0;
    let mut stack: Vec<(Id, Affine, u16)> = Vec::new();
    stack.push((root, Affine::IDENTITY, 0));

    while let Some((id, parent_t, depth)) = stack.pop() {
        let Some(node) = scene.node(id) else {
            continue;
        };
        if !node.visible {
            continue;
        }
        let global_t = parent_t.then(node.transform);
        let depth = depth.saturating_add(1);

        if let Some(draw) = node.draw {
            let in_region = match plan {
                FramePlan::None => false,
                FramePlan::Full => true,
                FramePlan::Regions(regions) => match scene.draw_bounds(id) {
                    Some(bounds) => regions.iter().any(|r| r.intersect(bounds).is_some()),
                    None => true,
                },
            };
            if in_region {
                match draw {
                    Draw::Path { path, brush, fill } => {
                        let key = SortKey::new(node.layer, depth, seq);
                        seq = seq.wrapping_add(1);
                        out.items.push((
                            key,
                            Command::FillPath {
                                path,
                                brush,
                                fill,
                                transform: global_t,
                            },
                        ));
                    }
                }
            }
        }

        // Push children in reverse so traversal order (and therefore the
        // seq allocation) matches document order before the sort.
        for child in node.children.iter().rev() {
            stack.push((*child, global_t, depth));
        }
    }

    out.items.sort_by_key(|(k, _)| *k);
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::scene::Node;

    use path_kit::Vec2;

    fn square_path() -> path_kit::Path {
        let mut p = path_kit::Path::new();
        p.move_to(Vec2::new(0.0, 0.0));
        p.line_to(Vec2::new(10.0, 0.0));
        p.line_to(Vec2::new(10.0, 10.0));
        p.line_to(Vec2::new(0.0, 10.0));
        p.close();
        p
    }

    fn red() -> Brush {
        Brush::opaque(200, 30, 30)
    }

    #[test]
    fn flatten_order_by_layer_then_doc_order() {
        let mut scene = Scene::new();
        let path_idx = scene.add_path(square_path());
        let root = scene.root().unwrap_or_else(root_fallback);
        let low = scene
            .spawn(
                root,
                Node::filled(Some(root), path_idx, red(), FillRule::NonZero),
            )
            .unwrap_or_else(root_fallback);
        if let Some(n) = scene.node_mut(low) {
            n.layer = 1;
        }
        let high = scene
            .spawn(
                root,
                Node::filled(Some(root), path_idx, red(), FillRule::NonZero),
            )
            .unwrap_or_else(root_fallback);
        if let Some(n) = scene.node_mut(high) {
            n.layer = 0;
        }

        let mut out = DrawList::new();
        flatten_scene(&scene, &FramePlan::Full, &mut out);
        let items = out.items();
        assert_eq!(items.len(), 2, "both nodes drawn");
        let first_layer = items.first().map(|(k, _)| k.layer());
        let second_layer = items.get(1).map(|(k, _)| k.layer());
        assert_eq!(first_layer, Some(0));
        assert_eq!(second_layer, Some(1));
    }

    fn root_fallback() -> crate::arena::Id {
        crate::arena::poison_id()
    }

    #[test]
    fn flatten_composes_transforms() {
        let mut scene = Scene::new();
        let path_idx = scene.add_path(square_path());
        let root = scene.root().unwrap_or_else(root_fallback);
        let mid = scene
            .spawn(root, Node::group(Some(root)))
            .unwrap_or_else(root_fallback);
        if let Some(n) = scene.node_mut(mid) {
            n.transform = path_kit::Affine::translate(Vec2::new(100.0, 0.0));
        }
        let leaf = scene
            .spawn(
                mid,
                Node::filled(Some(mid), path_idx, red(), FillRule::NonZero),
            )
            .unwrap_or_else(root_fallback);
        if let Some(n) = scene.node_mut(leaf) {
            n.transform = path_kit::Affine::translate(Vec2::new(0.0, 5.0));
        }
        let mut out = DrawList::new();
        flatten_scene(&scene, &FramePlan::Full, &mut out);
        let cmd = out.items().first().map(|(_, c)| *c);
        let Some(Command::FillPath { transform, .. }) = cmd else {
            return;
        };
        let p = transform.apply(Vec2::ZERO);
        assert!((p.x - 100.0).abs() < 1e-9 && (p.y - 5.0).abs() < 1e-9);
    }

    #[test]
    fn flatten_respects_visibility_and_regions() {
        let mut scene = Scene::new();
        let path_idx = scene.add_path(square_path());
        let root = scene.root().unwrap_or_else(root_fallback);
        let a = scene
            .spawn(
                root,
                Node::filled(Some(root), path_idx, red(), FillRule::NonZero),
            )
            .unwrap_or_else(root_fallback);
        let _b = scene
            .spawn(
                root,
                Node::filled(Some(root), path_idx, red(), FillRule::NonZero),
            )
            .unwrap_or_else(root_fallback);
        if let Some(n) = scene.node_mut(a) {
            n.visible = false;
        }

        let mut out = DrawList::new();
        flatten_scene(&scene, &FramePlan::Full, &mut out);
        assert_eq!(out.len(), 1, "invisible node skipped");

        // Region culling: draw bounds are (0,0)-(10,10).
        let regions = vec![path_kit::Rect::from_points(
            Vec2::new(500.0, 500.0),
            Vec2::new(600.0, 600.0),
        )];
        flatten_scene(&scene, &FramePlan::Regions(regions), &mut out);
        assert_eq!(out.len(), 0, "region miss culls the node");

        let hit = vec![path_kit::Rect::from_points(
            Vec2::new(5.0, 5.0),
            Vec2::new(600.0, 600.0),
        )];
        flatten_scene(&scene, &FramePlan::Regions(hit), &mut out);
        assert_eq!(out.len(), 1, "region hit keeps the node");

        flatten_scene(&scene, &FramePlan::None, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn sort_key_packing_roundtrip() {
        let k = SortKey::new(65535, 65534, 4294967295);
        assert_eq!(k.layer(), 65535);
        assert_eq!(k.depth(), 65534);
        assert_eq!(k.seq(), 4294967295);
        assert!(SortKey::new(1, 0, 0) < SortKey::new(2, 0, 0));
        assert!(SortKey::new(1, 0, 0) < SortKey::new(1, 1, 0));
    }
}

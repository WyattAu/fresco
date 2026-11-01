//! The retained scene: nodes, transforms, layers, draw payloads.

use crate::arena::{Arena, Id};
use path_kit::{Affine, FillRule, Path, Rect};

/// Paint for a draw command: a premultiplied sRGBA8 solid color.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Brush {
    /// Premultiplied `[r, g, b, a]` bytes.
    pub color: [u8; 4],
}

impl Brush {
    /// Brush from premultiplied bytes.
    #[must_use]
    pub const fn new(color: [u8; 4]) -> Self {
        Brush { color }
    }

    /// Opaque brush from straight sRGB bytes.
    #[must_use]
    pub const fn opaque(r: u8, g: u8, b: u8) -> Self {
        Brush {
            color: [r, g, b, 255],
        }
    }
}

/// Per-node draw payload.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Draw {
    /// Fill the referenced path (index into [`Scene::paths`]).
    Path {
        /// Path store index.
        path: u32,
        /// Fill paint.
        brush: Brush,
        /// Fill rule.
        fill: FillRule,
    },
}

/// A scene node.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// Parent handle, `None` for the root.
    pub parent: Option<Id>,
    /// Children in draw order.
    pub children: Vec<Id>,
    /// Local transform relative to the parent.
    pub transform: Affine,
    /// Draw payload; `None` for pure grouping nodes.
    pub draw: Option<Draw>,
    /// Compositing layer: major sort key for flattening.
    pub layer: u16,
    /// Visibility; invisible nodes do not draw.
    pub visible: bool,
}

impl Node {
    /// Grouping node with identity transform, visible, layer 0.
    #[must_use]
    pub fn group(parent: Option<Id>) -> Self {
        Node {
            parent,
            children: Vec::new(),
            transform: Affine::IDENTITY,
            draw: None,
            layer: 0,
            visible: true,
        }
    }

    /// Draw node for a path index.
    #[must_use]
    pub fn filled(parent: Option<Id>, path: u32, brush: Brush, fill: FillRule) -> Self {
        Node {
            draw: Some(Draw::Path { path, brush, fill }),
            ..Node::group(parent)
        }
    }
}

/// A retained scene: node arena plus a path store.
#[derive(Debug, Default)]
pub struct Scene {
    nodes: Arena<Node>,
    paths: Vec<Path>,
    root: Option<Id>,
}

impl Scene {
    /// New scene with a single root group.
    #[must_use]
    pub fn new() -> Self {
        let mut nodes = Arena::new();
        let root = nodes.insert(Node::group(None));
        Scene {
            nodes,
            paths: Vec::new(),
            root: Some(root),
        }
    }

    /// Root handle.
    #[must_use]
    pub fn root(&self) -> Option<Id> {
        self.root
    }

    /// Store a path, returning its index.
    pub fn add_path(&mut self, path: Path) -> u32 {
        let idx = u32::try_from(self.paths.len()).unwrap_or(u32::MAX);
        if (idx as usize) == self.paths.len() {
            self.paths.push(path);
        }
        idx
    }

    /// Borrow the path store.
    #[must_use]
    pub fn path(&self, idx: u32) -> Option<&Path> {
        self.paths.get(idx as usize)
    }

    /// Borrow a node.
    #[must_use]
    pub fn node(&self, id: Id) -> Option<&Node> {
        self.nodes.get(id)
    }

    /// Mutate a node.
    #[must_use]
    pub fn node_mut(&mut self, id: Id) -> Option<&mut Node> {
        self.nodes.get_mut(id)
    }

    /// Spawn `node` under `parent`. Fails when the parent is stale.
    /// Cycles are impossible: a fresh node has no children, so it cannot
    /// be an ancestor of the parent.
    pub fn spawn(&mut self, parent: Id, node: Node) -> Option<Id> {
        self.nodes.get(parent)?;
        let mut node = node;
        node.parent = Some(parent);
        let id = self.nodes.insert(node);
        if let Some(p) = self.nodes.get_mut(parent) {
            p.children.push(id);
        }
        Some(id)
    }

    /// Remove `id` and its whole subtree. Returns the removed count.
    /// The root itself can be removed only by explicit call; the scene
    /// then reports no root.
    pub fn remove(&mut self, id: Id) -> usize {
        // Detach from parent first.
        let parent = self.nodes.get(id).and_then(|n| n.parent);
        if let Some(pid) = parent
            && let Some(p) = self.nodes.get_mut(pid)
        {
            p.children.retain(|&c| c != id);
        }
        if self.root == Some(id) {
            self.root = None;
        }
        // Iterative subtree removal.
        let mut count = 0;
        let mut stack: Vec<Id> = vec![id];
        while let Some(top) = stack.pop() {
            let children = match self.nodes.get(top) {
                Some(n) => n.children.clone(),
                None => continue,
            };
            for c in children {
                stack.push(c);
            }
            if self.nodes.remove(top).is_some() {
                count += 1;
            }
        }
        count
    }

    /// Global transform for a node: composition from the root down.
    /// Stale or detached nodes return `None`.
    #[must_use]
    pub fn global_transform(&self, id: Id) -> Option<Affine> {
        let mut chain: Vec<Affine> = Vec::new();
        let mut cur = Some(id);
        while let Some(cid) = cur {
            let node = self.nodes.get(cid)?;
            chain.push(node.transform);
            cur = node.parent;
        }
        let mut t = Affine::IDENTITY;
        for a in chain.iter().rev() {
            t = t.then(*a);
        }
        Some(t)
    }

    /// Draw bounds of a node's payload in global space (control-point
    /// hull, conservative). `None` for grouping nodes.
    #[must_use]
    pub fn draw_bounds(&self, id: Id) -> Option<Rect> {
        let node = self.nodes.get(id)?;
        let draw = node.draw?;
        let path = self.path(match draw {
            Draw::Path { path, .. } => path,
        })?;
        let local = path.bounds()?;
        let t = self.global_transform(id)?;
        Some(t.apply_rect(local))
    }

    /// Conservative bounds of a node and all descendants in global space.
    #[must_use]
    pub fn subtree_bounds(&self, id: Id) -> Option<Rect> {
        let mut r: Option<Rect> = None;
        let mut stack: Vec<Id> = vec![id];
        while let Some(top) = stack.pop() {
            if let Some(b) = self.draw_bounds(top) {
                r = Some(match r {
                    Some(old) => old.union(b),
                    None => b,
                });
            }
            if let Some(n) = self.nodes.get(top) {
                for c in &n.children {
                    stack.push(*c);
                }
            }
        }
        r
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;
    use path_kit::Vec2;

    fn unit_square_path() -> Path {
        let mut p = Path::new();
        p.move_to(Vec2::new(0.0, 0.0));
        p.line_to(Vec2::new(10.0, 0.0));
        p.line_to(Vec2::new(10.0, 10.0));
        p.line_to(Vec2::new(0.0, 10.0));
        p.close();
        p
    }

    #[test]
    fn spawn_and_traverse() {
        let mut scene = Scene::new();
        let root = scene.root().unwrap_or_else(unreachable_root);
        let child = scene.spawn(root, Node::group(Some(root)));
        assert!(child.is_some());
        let child = child.unwrap_or_else(unreachable_root);
        assert_eq!(scene.node(child).map(|n| n.parent), Some(Some(root)));
        let kids = scene.node(root).map(|n| n.children.len());
        assert_eq!(kids, Some(1));
    }

    fn unreachable_root() -> Id {
        // Tests assert root existence first; this fallback never becomes
        // a valid handle (index and generation are both poisoned).
        crate::arena::poison_id()
    }

    #[test]
    fn spawn_under_stale_parent_fails() {
        let mut scene = Scene::new();
        let root = scene.root().unwrap_or_else(unreachable_root);
        let temp = scene
            .spawn(root, Node::group(Some(root)))
            .unwrap_or_else(unreachable_root);
        scene.remove(temp);
        assert_eq!(scene.spawn(temp, Node::group(None)), None);
    }

    #[test]
    fn remove_takes_subtree() {
        let mut scene = Scene::new();
        let root = scene.root().unwrap_or_else(unreachable_root);
        let a = scene
            .spawn(root, Node::group(Some(root)))
            .unwrap_or_else(unreachable_root);
        let b = scene
            .spawn(a, Node::group(Some(a)))
            .unwrap_or_else(unreachable_root);
        let c = scene
            .spawn(b, Node::group(Some(b)))
            .unwrap_or_else(unreachable_root);
        assert_eq!(scene.remove(a), 3, "a, b, c removed");
        assert!(scene.node(b).is_none());
        assert!(scene.node(c).is_none());
        assert_eq!(scene.node(root).map(|n| n.children.is_empty()), Some(true));
    }

    #[test]
    fn global_transform_composes() {
        let mut scene = Scene::new();
        let root = scene.root().unwrap_or_else(unreachable_root);
        if let Some(n) = scene.node_mut(root) {
            n.transform = Affine::translate(Vec2::new(100.0, 0.0));
        }
        let path_idx = scene.add_path(unit_square_path());
        let child = scene
            .spawn(
                root,
                Node::filled(
                    Some(root),
                    path_idx,
                    Brush::opaque(255, 0, 0),
                    FillRule::NonZero,
                ),
            )
            .unwrap_or_else(unreachable_root);
        if let Some(n) = scene.node_mut(child) {
            n.transform = Affine::translate(Vec2::new(0.0, 5.0));
        }
        let t = scene.global_transform(child).unwrap_or(Affine::IDENTITY);
        let p = t.apply(Vec2::ZERO);
        assert!((p.x - 100.0).abs() < 1e-9 && (p.y - 5.0).abs() < 1e-9);
    }

    #[test]
    fn draw_bounds_use_global_space() {
        let mut scene = Scene::new();
        let root = scene.root().unwrap_or_else(unreachable_root);
        if let Some(n) = scene.node_mut(root) {
            n.transform = Affine::translate(Vec2::new(100.0, 0.0));
        }
        let path_idx = scene.add_path(unit_square_path());
        let child = scene
            .spawn(
                root,
                Node::filled(
                    Some(root),
                    path_idx,
                    Brush::opaque(255, 0, 0),
                    FillRule::NonZero,
                ),
            )
            .unwrap_or_else(unreachable_root);
        let b = scene.draw_bounds(child).unwrap_or(Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 0.0,
            y1: 0.0,
        });
        assert!((b.x0 - 100.0).abs() < 1e-9 && (b.x1 - 110.0).abs() < 1e-9);
        let sub = scene.subtree_bounds(root).unwrap_or(b);
        assert!((sub.x1 - 110.0).abs() < 1e-9);
    }
}

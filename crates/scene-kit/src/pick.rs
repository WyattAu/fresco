//! Picking: CAD-style selection queries against the scene graph.
//!
//! `pick` returns the visible filled nodes containing a point,
//! topmost first; `pick_rect` returns nodes intersecting a
//! rectangle (lasso/marquee selection). Hit geometry is the node's
//! flattened path transformed by its global transform, tested with
//! the same fill rule the renderer uses.

use crate::{Draw, Scene};
use path_kit::{FillRule, Poly, Rect, Vec2, point_in_path};

impl Scene {
    /// Draw-order list of `(node id, path index, fill rule)` for
    /// visible filled nodes, deepest first.
    fn draw_order(&self) -> Vec<(crate::Id, u32, FillRule)> {
        let mut order: Vec<(crate::Id, u32, FillRule)> = Vec::new();
        let mut stack: Vec<crate::Id> = Vec::new();
        if let Some(root) = self.root() {
            stack.push(root);
        }
        while let Some(id) = stack.pop() {
            let Some(node) = self.node(id) else {
                continue;
            };
            if !node.visible {
                continue;
            }
            if let Some(Draw::Path { path, fill, .. }) = &node.draw {
                order.push((id, *path, *fill));
            }
            // Push children reversed so the first child pops first
            // (preserving draw order for a topmost-last list).
            for child in node.children.iter().rev() {
                stack.push(*child);
            }
        }
        order
    }

    /// Flattened hit geometry of a node in world space.
    fn hit_polys(&self, id: crate::Id, path_idx: u32, tol: f64) -> Option<Vec<Poly>> {
        let t = self.global_transform(id)?;
        let path = self.path(path_idx)?;
        let polys = path.flatten(tol.max(0.01));
        if polys.is_empty() {
            return None;
        }
        Some(
            polys
                .iter()
                .map(|p| Poly {
                    points: p.points.iter().map(|v| t.apply(*v)).collect(),
                    closed: p.closed,
                })
                .collect(),
        )
    }

    /// Ids of visible filled nodes containing `point`, topmost
    /// (last-drawn) first. `tol` inflates the hit test by a few
    /// pixels of flatten tolerance (pass 0.0 for exact geometry).
    #[must_use]
    pub fn pick(&self, point: Vec2, tol: f64) -> Vec<crate::Id> {
        let mut hits = Vec::new();
        for (id, path_idx, fill) in self.draw_order().iter().rev() {
            let Some(polys) = self.hit_polys(*id, *path_idx, tol) else {
                continue;
            };
            if point_in_path(point, &polys, *fill) {
                hits.push(*id);
            }
        }
        hits
    }

    /// Ids of visible filled nodes intersecting `rect`, topmost
    /// first (marquee selection). A node hits when any flattened
    /// segment crosses the rect or the rect's top-left corner lies
    /// inside the shape.
    #[must_use]
    pub fn pick_rect(&self, rect: Rect) -> Vec<crate::Id> {
        let corner = Vec2::new(rect.x0, rect.y0);
        let mut hits = Vec::new();
        for (id, path_idx, _fill) in self.draw_order().iter().rev() {
            let Some(polys) = self.hit_polys(*id, *path_idx, 0.0) else {
                continue;
            };
            let mut hit = rect.contains(corner)
                && polys
                    .iter()
                    .any(|p| point_in_path(corner, std::slice::from_ref(p), FillRule::NonZero));
            if !hit {
                'outer: for poly in &polys {
                    for seg in poly.points.windows(2) {
                        let a = seg.first().copied().unwrap_or_default();
                        let b = seg.get(1).copied().unwrap_or_default();
                        if path_kit::sect::segment_rect(a, b, rect) {
                            hit = true;
                            break 'outer;
                        }
                    }
                }
            }
            if hit {
                hits.push(*id);
            }
        }
        hits
    }
}

#[cfg(test)]
mod tests {
    use crate::{Brush, Node, Scene};
    use path_kit::{FillRule, Path, Rect, Vec2};

    fn square_path(cx: f64, cy: f64, half: f64) -> Path {
        let mut p = Path::new();
        let pts = [
            Vec2::new(cx - half, cy - half),
            Vec2::new(cx + half, cy - half),
            Vec2::new(cx + half, cy + half),
            Vec2::new(cx - half, cy + half),
        ];
        p.move_to(pts[0]);
        p.line_to(pts[1]);
        p.line_to(pts[2]);
        p.line_to(pts[3]);
        p.close();
        p
    }

    /// Back square (red, drawn first), front square (blue, drawn
    /// last), both centered at (100, 100) with half 40 and 25.
    fn scene_two_squares() -> Scene {
        let mut s = Scene::new();
        let Some(root) = s.root() else {
            return s;
        };
        let back_path = s.add_path(square_path(100.0, 100.0, 40.0));
        let Some(back) = s.spawn(
            root,
            Node::filled(
                None,
                back_path,
                Brush::opaque(200, 60, 60),
                FillRule::NonZero,
            ),
        ) else {
            return s;
        };
        let front_path = s.add_path(square_path(100.0, 100.0, 25.0));
        let Some(_front) = s.spawn(
            back,
            Node::filled(
                None,
                front_path,
                Brush::opaque(200, 60, 60),
                FillRule::NonZero,
            ),
        ) else {
            return s;
        };
        s
    }

    #[test]
    fn pick_returns_topmost_first() {
        let s = scene_two_squares();
        // Center point: inside both squares.
        let hits = s.pick(Vec2::new(100.0, 100.0), 0.0);
        assert_eq!(hits.len(), 2, "point inside both: {hits:?}");
        let back = s
            .root()
            .and_then(|r| s.node(r).and_then(|n| n.children.first().copied()));
        let front = back.and_then(|b| s.node(b).and_then(|n| n.children.first().copied()));
        assert_eq!(
            hits.first(),
            front.as_ref(),
            "front (last-drawn) listed first"
        );
        assert_eq!(hits.get(1), back.as_ref());
    }

    #[test]
    fn pick_misses_outside() {
        let s = scene_two_squares();
        let hits = s.pick(Vec2::new(250.0, 100.0), 0.0);
        assert!(hits.is_empty(), "outside both squares");
    }

    #[test]
    fn pick_respects_visibility() {
        let mut s = scene_two_squares();
        // Hide the FRONT square; invisible nodes hide their whole
        // subtree, so hiding the parent would hide both.
        let front = s
            .root()
            .and_then(|r| s.node(r).and_then(|n| n.children.first().copied()))
            .and_then(|back| s.node(back).and_then(|n| n.children.first().copied()));
        if let Some(n) = front.and_then(|f| s.node_mut(f)) {
            n.visible = false;
        }
        let hits = s.pick(Vec2::new(100.0, 100.0), 0.0);
        assert_eq!(hits.len(), 1, "only the back square remains");
    }

    #[test]
    fn pick_rect_lasso() {
        let s = scene_two_squares();
        // Lasso covering only the top-left corner of both squares.
        let hits = s.pick_rect(Rect::from_points(
            Vec2::new(50.0, 50.0),
            Vec2::new(90.0, 90.0),
        ));
        assert_eq!(hits.len(), 2, "lasso crosses both outlines: {hits:?}");
        // Tiny rect well outside.
        let misses = s.pick_rect(Rect::from_points(
            Vec2::new(250.0, 50.0),
            Vec2::new(260.0, 60.0),
        ));
        assert!(misses.is_empty());
    }

    #[test]
    fn pick_honors_transforms() {
        let mut s = Scene::new();
        let Some(root) = s.root() else {
            return;
        };
        // A square drawn at the origin, under a parent translated to
        // (150, 0).
        let mut group = Node::group(None);
        group.transform = path_kit::Affine::translate(Vec2::new(150.0, 0.0));
        let Some(group) = s.spawn(root, group) else {
            return;
        };
        let sq_path = s.add_path(square_path(0.0, 0.0, 30.0));
        let Some(_sq) = s.spawn(
            group,
            Node::filled(None, sq_path, Brush::opaque(200, 60, 60), FillRule::NonZero),
        ) else {
            return;
        };
        // World point inside the transformed square.
        let hits = s.pick(Vec2::new(150.0, 0.0), 0.0);
        assert_eq!(hits.len(), 1, "transform-aware pick");
        // Same local point without the transform: miss.
        let misses = s.pick(Vec2::new(0.0, 0.0), 0.0);
        assert!(misses.is_empty(), "untransformed point misses");
    }
}

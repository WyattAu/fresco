//! Quadtree spatial index for rect-region queries.
//!
//! Broad-phase structure for CAD selection hit-testing and game
//! culling: insert (rect, payload) pairs, query by rect overlap.
//! Fixed max depth and per-node capacity. Items that straddle the
//! split lines are stored at the branching level itself (never
//! forced into a child), so every item is always discoverable by a
//! query overlapping its bounds.

use crate::Rect;

/// Payload handle stored in the tree.
pub type ItemId = u32;

const CAPACITY: usize = 8;
const MAX_DEPTH: u32 = 8;

#[derive(Clone, Copy, Debug)]
struct Item {
    bounds: Rect,
    id: ItemId,
}

#[derive(Clone, Debug)]
enum Node {
    /// Leaf holding up to `CAPACITY` items.
    Leaf { items: Vec<Item> },
    /// Internal node: children own their quadrants, `straddlers`
    /// holds items crossing the split lines.
    Branch {
        children: Box<[Quadtree; 4]>,
        straddlers: Vec<Item>,
    },
}

/// A quadtree over a fixed rectangular region.
#[derive(Clone, Debug)]
pub struct Quadtree {
    region: Rect,
    depth: u32,
    node: Node,
}

impl Quadtree {
    /// New quadtree covering `region`.
    #[must_use]
    pub fn new(region: Rect) -> Self {
        Quadtree {
            region,
            depth: 0,
            node: Node::Leaf { items: Vec::new() },
        }
    }

    fn child(region: Rect, depth: u32) -> Self {
        Quadtree {
            region,
            depth,
            node: Node::Leaf { items: Vec::new() },
        }
    }

    /// Insert `id` covering `bounds`.
    pub fn insert(&mut self, bounds: Rect, id: ItemId) {
        self.insert_item(Item { bounds, id });
    }

    fn insert_item(&mut self, item: Item) {
        match &mut self.node {
            Node::Leaf { items } => {
                items.push(item);
                if items.len() > CAPACITY && self.depth < MAX_DEPTH {
                    self.split();
                }
            }
            Node::Branch {
                children,
                straddlers,
            } => {
                for child in children.iter_mut() {
                    if child.region.contains_rect(item.bounds) {
                        child.insert_item(item);
                        return;
                    }
                }
                // Straddles a split line (or pokes outside the root
                // region): stays at this level.
                straddlers.push(item);
            }
        }
    }

    fn split(&mut self) {
        let Node::Leaf { items: _ } = &mut self.node else {
            return;
        };
        let cx = (self.region.x0 + self.region.x1) * 0.5;
        let cy = (self.region.y0 + self.region.y1) * 0.5;
        let child_depth = self.depth + 1;
        let children = Box::new([
            Self::child(
                Rect::from_points(
                    crate::Vec2::new(self.region.x0, self.region.y0),
                    crate::Vec2::new(cx, cy),
                ),
                child_depth,
            ),
            Self::child(
                Rect::from_points(
                    crate::Vec2::new(cx, self.region.y0),
                    crate::Vec2::new(self.region.x1, cy),
                ),
                child_depth,
            ),
            Self::child(
                Rect::from_points(
                    crate::Vec2::new(self.region.x0, cy),
                    crate::Vec2::new(cx, self.region.y1),
                ),
                child_depth,
            ),
            Self::child(
                Rect::from_points(
                    crate::Vec2::new(cx, cy),
                    crate::Vec2::new(self.region.x1, self.region.y1),
                ),
                child_depth,
            ),
        ]);
        let old = std::mem::replace(
            &mut self.node,
            Node::Branch {
                children,
                straddlers: Vec::new(),
            },
        );
        let Node::Leaf { items: old_items } = old else {
            return;
        };
        for item in old_items {
            self.insert_item(item);
        }
    }

    /// Collect ids of all items whose bounds intersect `query`.
    /// Duplicates are impossible: each item lives at exactly one
    /// level (one child or one straddler list).
    pub fn query(&self, query: Rect) -> Vec<ItemId> {
        let mut out = Vec::new();
        self.query_into(query, &mut out);
        out
    }

    /// Query with stable, sorted, deduplicated output.
    #[must_use]
    pub fn query_unique(&self, query: Rect) -> Vec<ItemId> {
        let mut out = self.query(query);
        out.sort_unstable();
        out.dedup();
        out
    }

    fn query_into(&self, query: Rect, out: &mut Vec<ItemId>) {
        match &self.node {
            Node::Leaf { items } => {
                // Reached via gated descent (region already known to
                // intersect) or as the root leaf, which may hold
                // items poking outside the region; per-item checks
                // keep both cases correct.
                for item in items {
                    if item.bounds.intersects(query) {
                        out.push(item.id);
                    }
                }
            }
            Node::Branch {
                children,
                straddlers,
            } => {
                // Straddlers are not guaranteed inside `region`
                // (root-level ones may poke out): scan ungated.
                for item in straddlers {
                    if item.bounds.intersects(query) {
                        out.push(item.id);
                    }
                }
                // Children only ever hold items fully inside their
                // region, so the region gate is sound.
                for child in children.iter() {
                    if child.region.intersects(query) {
                        child.query_into(query, out);
                    }
                }
            }
        }
    }

    /// Total number of items stored.
    #[must_use]
    pub fn count(&self) -> usize {
        match &self.node {
            Node::Leaf { items } => items.len(),
            Node::Branch {
                children,
                straddlers,
            } => children.iter().map(Self::count).sum::<usize>() + straddlers.len(),
        }
    }

    /// Remove every item.
    pub fn clear(&mut self) {
        self.node = Node::Leaf { items: Vec::new() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vec2;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Rect {
        Rect::from_points(Vec2::new(x0, y0), Vec2::new(x1, y1))
    }

    #[test]
    fn insert_and_query_single() {
        let mut qt = Quadtree::new(rect(0.0, 0.0, 100.0, 100.0));
        qt.insert(rect(10.0, 10.0, 20.0, 20.0), 1);
        let hits = qt.query(rect(15.0, 15.0, 25.0, 25.0));
        assert_eq!(hits, vec![1]);
        let misses = qt.query(rect(50.0, 50.0, 60.0, 60.0));
        assert!(misses.is_empty(), "no hits in empty region");
    }

    #[test]
    fn split_preserves_all_items() {
        let mut qt = Quadtree::new(rect(0.0, 0.0, 100.0, 100.0));
        for i in 0..40_u32 {
            let x = f64::from(i % 10) * 10.0;
            let y = f64::from(i / 10) * 10.0;
            qt.insert(rect(x, y, x + 5.0, y + 5.0), i);
        }
        assert_eq!(qt.count(), 40, "all items survive the split");
        let hits = qt.query_unique(rect(0.0, 0.0, 12.0, 12.0));
        assert!(hits.contains(&0), "item 0 at (0,0) must be found");
        assert!(hits.contains(&1), "item 1 at (10,0) must be found");
    }

    #[test]
    fn query_deep_quadrant() {
        let mut qt = Quadtree::new(rect(0.0, 0.0, 256.0, 256.0));
        for i in 0..64_u32 {
            let x = f64::from(i % 8) * 32.0;
            let y = f64::from(i / 8) * 32.0;
            qt.insert(rect(x, y, x + 16.0, y + 16.0), i);
        }
        // Item 63 is at (224, 224).
        let hits = qt.query_unique(rect(220.0, 220.0, 240.0, 240.0));
        assert_eq!(hits, vec![63]);
    }

    #[test]
    fn straddling_item_found_from_both_sides() {
        let mut qt = Quadtree::new(rect(0.0, 0.0, 100.0, 100.0));
        // Force a split.
        for i in 0..20_u32 {
            let x = f64::from(i) * 4.0;
            qt.insert(rect(x, 0.0, x + 2.0, 2.0), i);
        }
        // Insert an item straddling the center lines.
        qt.insert(rect(48.0, 48.0, 52.0, 52.0), 99);
        let left = qt.query(rect(40.0, 40.0, 50.0, 50.0));
        assert!(left.contains(&99), "straddler found from upper-left");
        let right = qt.query(rect(50.0, 50.0, 60.0, 60.0));
        assert!(right.contains(&99), "straddler found from lower-right");
    }

    /// The regression case: an item straddling a *nested* split line
    /// must still be found by a query that overlaps its bounds but
    /// not the region of the child it would have been forced into.
    #[test]
    fn nested_straddler_query_misses_impossible() {
        let mut qt = Quadtree::new(rect(0.0, 0.0, 100.0, 100.0));
        // Grid of small cells forces nested splits at 12.5 etc.
        for i in 0..40_u32 {
            let x = f64::from(i % 10) * 10.0;
            let y = f64::from(i / 10) * 10.0;
            qt.insert(rect(x, y, x + 5.0, y + 5.0), i);
        }
        // Item 1 spans x [10, 15]: straddles the nested split at
        // x = 12.5. A query hugging its left half must find it.
        let left_half = qt.query(rect(10.0, 0.0, 12.0, 5.0));
        assert!(left_half.contains(&1), "nested straddler found from left");
        let right_half = qt.query(rect(13.0, 0.0, 15.0, 5.0));
        assert!(right_half.contains(&1), "nested straddler found from right");
    }

    #[test]
    fn every_item_findable_by_point_query_on_its_interior() {
        let mut qt = Quadtree::new(rect(0.0, 0.0, 100.0, 100.0));
        for i in 0..60_u32 {
            let x = f64::from(i % 12) * 8.0;
            let y = f64::from(i / 12) * 25.0;
            qt.insert(rect(x, y, x + 6.0, y + 20.0), i);
        }
        // A point strictly inside each item's bounds must hit it.
        for i in 0..60_u32 {
            let x = f64::from(i % 12) * 8.0;
            let y = f64::from(i / 12) * 25.0;
            let hits = qt.query_unique(rect(x + 3.0, y + 10.0, x + 3.5, y + 10.5));
            assert!(
                hits.contains(&i),
                "point inside item {i} at ({x},{y}) must find it"
            );
        }
    }

    #[test]
    fn clear_empties_tree() {
        let mut qt = Quadtree::new(rect(0.0, 0.0, 100.0, 100.0));
        qt.insert(rect(10.0, 10.0, 20.0, 20.0), 1);
        qt.clear();
        assert_eq!(qt.count(), 0);
        assert!(qt.query(rect(0.0, 0.0, 100.0, 100.0)).is_empty());
    }

    #[test]
    fn count_matches_unique_queries() {
        let mut qt = Quadtree::new(rect(0.0, 0.0, 100.0, 100.0));
        for i in 0..30_u32 {
            let x = f64::from(i) * 3.0;
            let y = f64::from(i % 5) * 20.0;
            qt.insert(rect(x, y, x + 2.0, y + 2.0), i);
        }
        let everything = qt.query_unique(rect(0.0, 0.0, 100.0, 100.0));
        assert_eq!(everything.len(), qt.count());
    }
}

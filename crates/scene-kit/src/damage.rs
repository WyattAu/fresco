//! Damage accumulation and frame plans.
//!
//! Damage is plain data: rects in, one [`FramePlan`] out. Pure and
//! headless-testable (REQ-S2), and the direct source for platform damage
//! protocols (Wayland damage, DirectComposition virtual-surface tiles:
//! paradigm 852 in `docs/paradigms.md`).

use path_kit::Rect;
use std::vec::Vec;

/// What a frame must repaint.
#[derive(Clone, Debug, PartialEq)]
pub enum FramePlan {
    /// Nothing invalid: skip painting and presentation entirely.
    None,
    /// Everything invalid: repaint the whole viewport.
    Full,
    /// Repaint exactly these clipped regions.
    Regions(Vec<Rect>),
}

/// Accumulates damage rectangles and turns them into frame plans.
#[derive(Debug)]
pub struct DamageTracker {
    regions: Vec<Rect>,
    /// Region-count cap before falling back to a full repaint.
    max_regions: usize,
    /// Covered-viewport-area fraction beyond which a full repaint wins
    /// (region churn costs more than the blit it saves).
    area_threshold: f64,
}

impl DamageTracker {
    /// New tracker. `max_regions` of 0 disables region mode entirely.
    /// `area_threshold` in `(0, 1]`; values outside clamp.
    #[must_use]
    pub fn new(max_regions: usize, area_threshold: f64) -> Self {
        DamageTracker {
            regions: Vec::new(),
            max_regions,
            area_threshold: area_threshold.clamp(0.01, 1.0),
        }
    }

    /// Add a damaged rect. Empty rects are ignored.
    pub fn invalidate(&mut self, rect: Rect) {
        if rect.is_empty() || !rect.x0.is_finite() || !rect.y0.is_finite() {
            return;
        }
        self.regions.push(rect);
    }

    /// Accumulated raw damage count (for tests and tuning).
    #[must_use]
    pub fn pending(&self) -> usize {
        self.regions.len()
    }

    /// Drop all pending damage without producing a plan.
    pub fn reset(&mut self) {
        self.regions.clear();
    }

    /// Coalesce pending damage into a plan for a `viewport`.
    ///
    /// - no damage: [`FramePlan::None`];
    /// - too many regions, or regions covering more than the area
    ///   threshold of the viewport: [`FramePlan::Full`];
    /// - otherwise: overlapping regions merged, then clipped to the
    ///   viewport ([`FramePlan::Regions`]).
    #[must_use]
    pub fn take_plan(&mut self, viewport: Rect) -> FramePlan {
        if self.regions.is_empty() {
            return FramePlan::None;
        }
        let mut merged = merge_overlapping(self.regions.clone());
        self.regions.clear();

        if merged.len() > self.max_regions {
            return FramePlan::Full;
        }
        let covered: f64 = merged.iter().map(|r| r.area()).sum();
        let viewport_area = viewport.area().max(1.0);
        if covered >= self.area_threshold * viewport_area {
            return FramePlan::Full;
        }
        merged.retain_mut(|r| match viewport.intersect(*r) {
            Some(clipped) => {
                *r = clipped;
                true
            }
            None => false,
        });
        if merged.is_empty() {
            FramePlan::None
        } else {
            FramePlan::Regions(merged)
        }
    }
}

/// Merge pairwise-intersecting rects until fixpoint (quadratic, fine at
/// damage scale).
#[must_use]
pub fn merge_overlapping(rects: Vec<Rect>) -> Vec<Rect> {
    let mut out = rects;
    let mut changed = true;
    while changed {
        changed = false;
        let mut i = 0;
        while i < out.len() {
            let mut j = i + 1;
            let mut merged_here = false;
            while j < out.len() {
                let (a, b) = match (out.get(i), out.get(j)) {
                    (Some(a), Some(b)) => (*a, *b),
                    _ => break,
                };
                if a.intersect(b).is_some() {
                    let u = a.union(b);
                    if let Some(slot) = out.get_mut(i) {
                        *slot = u;
                    }
                    out.remove(j);
                    merged_here = true;
                    changed = true;
                } else {
                    j += 1;
                }
            }
            if !merged_here {
                i += 1;
            }
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    use path_kit::Vec2;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Rect {
        Rect::from_points(Vec2::new(x0, y0), Vec2::new(x1, y1))
    }

    fn viewport() -> Rect {
        rect(0.0, 0.0, 1000.0, 1000.0)
    }

    #[test]
    fn damage_plans_none_when_clean() {
        let mut tracker = DamageTracker::new(8, 0.3);
        assert_eq!(tracker.take_plan(viewport()), FramePlan::None);
        tracker.invalidate(rect(0.0, 0.0, 0.0, 0.0)); // empty: ignored
        assert_eq!(tracker.take_plan(viewport()), FramePlan::None);
    }

    #[test]
    fn damage_plans_regions_cover_invalidation() {
        let mut tracker = DamageTracker::new(8, 0.9);
        tracker.invalidate(rect(100.0, 100.0, 200.0, 200.0));
        let plan = tracker.take_plan(viewport());
        assert!(
            matches!(plan, FramePlan::Regions(_)),
            "expected regions, got {plan:?}"
        );
        let FramePlan::Regions(regions) = plan else {
            return;
        };
        assert_eq!(regions.len(), 1);
        let r = regions.first().copied().unwrap_or(default_rect());
        assert!((r.x0 - 100.0).abs() < 1e-9 && (r.x1 - 200.0).abs() < 1e-9);
        assert!(tracker.pending() == 0, "plan consumes damage");
    }

    #[test]
    fn damage_plans_full_when_threshold_exceeded() {
        let mut tracker = DamageTracker::new(8, 0.3);
        tracker.invalidate(rect(0.0, 0.0, 900.0, 900.0)); // 81 percent of viewport
        assert_eq!(tracker.take_plan(viewport()), FramePlan::Full);
    }

    #[test]
    fn damage_plans_full_when_too_many_regions() {
        let mut tracker = DamageTracker::new(2, 0.9);
        for k in 0..3 {
            let x = f64::from(k) * 10.0;
            tracker.invalidate(rect(x, 0.0, x + 5.0, 5.0));
        }
        assert_eq!(tracker.take_plan(viewport()), FramePlan::Full);
    }

    #[test]
    fn damage_plans_merge_overlaps() {
        let mut tracker = DamageTracker::new(8, 0.9);
        tracker.invalidate(rect(0.0, 0.0, 10.0, 10.0));
        tracker.invalidate(rect(5.0, 0.0, 15.0, 10.0));
        let plan = tracker.take_plan(viewport());
        assert!(
            matches!(plan, FramePlan::Regions(_)),
            "expected regions, got {plan:?}"
        );
        let FramePlan::Regions(regions) = plan else {
            return;
        };
        assert_eq!(regions.len(), 1, "overlapping rects merged");
        let r = regions.first().copied().unwrap_or(default_rect());
        assert!((r.x1 - 15.0).abs() < 1e-9);
    }

    #[test]
    fn damage_plans_clip_to_viewport() {
        let mut tracker = DamageTracker::new(8, 0.9);
        tracker.invalidate(rect(-100.0, -100.0, 50.0, 50.0));
        let plan = tracker.take_plan(viewport());
        assert!(
            matches!(plan, FramePlan::Regions(_)),
            "expected regions, got {plan:?}"
        );
        let FramePlan::Regions(regions) = plan else {
            return;
        };
        let r = regions.first().copied().unwrap_or(default_rect());
        assert!((r.x0 - 0.0).abs() < 1e-9 && (r.y0 - 0.0).abs() < 1e-9);
    }

    #[test]
    fn damage_plans_none_when_all_damage_offscreen() {
        let mut tracker = DamageTracker::new(8, 0.9);
        tracker.invalidate(rect(2000.0, 2000.0, 2100.0, 2100.0));
        assert_eq!(tracker.take_plan(viewport()), FramePlan::None);
    }

    #[test]
    fn merge_overlapping_fixpoint() {
        let rects = vec![
            rect(0.0, 0.0, 10.0, 10.0),
            rect(8.0, 0.0, 12.0, 10.0),
            rect(11.0, 0.0, 20.0, 10.0),
        ];
        let merged = merge_overlapping(rects);
        assert_eq!(merged.len(), 1);
        let r = merged.first().copied().unwrap_or(default_rect());
        assert!((r.x0 - 0.0).abs() < 1e-9 && (r.x1 - 20.0).abs() < 1e-9);
    }

    fn default_rect() -> Rect {
        Rect::from_points(Vec2::ZERO, Vec2::ZERO)
    }
}

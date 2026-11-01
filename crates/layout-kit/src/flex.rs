//! Single-axis flex distribution solver.

use alloc::vec::Vec;

/// Layout axis (naming only; the solver is axis-agnostic).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// Main axis is horizontal.
    Row,
    /// Main axis is vertical.
    Column,
}

/// One flex child's main-axis inputs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlexItem {
    /// Preferred main-axis size.
    pub basis: f64,
    /// Proportion of free space to absorb.
    pub grow: f64,
    /// Weight for shrinking on overflow.
    pub shrink: f64,
    /// Lower clamp on the resolved size.
    pub min: Option<f64>,
    /// Upper clamp on the resolved size.
    pub max: Option<f64>,
}

impl FlexItem {
    /// Item with a basis and default grow/shrink (0.0, 1.0).
    #[must_use]
    pub fn new(basis: f64) -> Self {
        FlexItem {
            basis,
            grow: 0.0,
            shrink: 1.0,
            min: None,
            max: None,
        }
    }
}

/// Distribute `main_size` across `items` on one axis with `gap` between
/// children (REQ-L2).
///
/// Rules, CSS-flexbox subset:
/// - overflow (`free < 0`): shrink proportional to `shrink * basis`
///   (fallback: even reduction when all shrink weights are zero);
/// - underflow (`free > 0`): distribute by `grow` (none: pack at start);
/// - min/max clamps apply last; leftover space is not redistributed (v0
///   limitation, documented in SPEC).
///
/// Negative or non-finite `main_size` yields an empty vector; basis is
/// clamped to >= 0 before use.
#[must_use]
pub fn solve_flex(main_size: f64, gap: f64, items: &[FlexItem]) -> Vec<f64> {
    if items.is_empty() || !main_size.is_finite() || main_size <= 0.0 {
        return Vec::new();
    }
    let n = items.len();
    let bases: Vec<f64> = items.iter().map(|it| it.basis.max(0.0)).collect();
    let total_gap = gap.max(0.0) * f64::from(n.saturating_sub(1) as u32);
    let total_basis: f64 = bases.iter().sum();
    let free = main_size - total_basis - total_gap;

    let mut sizes: Vec<f64> = bases.clone();
    if free > 0.0 {
        let total_grow: f64 = items.iter().map(|it| it.grow.max(0.0)).sum();
        if total_grow > 0.0 {
            for (size, it) in sizes.iter_mut().zip(items.iter()) {
                *size += free * (it.grow.max(0.0) / total_grow);
            }
        }
    } else if free < 0.0 {
        let over = -free;
        let weighted: f64 = bases
            .iter()
            .zip(items.iter())
            .map(|(b, it)| b * it.shrink.max(0.0))
            .sum();
        if weighted > 0.0 {
            for (size, (b, it)) in sizes.iter_mut().zip(bases.iter().zip(items.iter())) {
                *size -= over * (b * it.shrink.max(0.0)) / weighted;
            }
        } else {
            let even = over / f64::from(n as u32);
            for size in sizes.iter_mut() {
                *size -= even;
            }
        }
    }

    // Clamps last; no redistribution (documented v0 limitation).
    for (size, it) in sizes.iter_mut().zip(items.iter()) {
        if let Some(min) = it.min {
            *size = size.max(min);
        }
        if let Some(max) = it.max {
            *size = size.min(max);
        }
        if !size.is_finite() || *size < 0.0 {
            *size = 0.0;
        }
    }
    sizes
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;
    use alloc::vec;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn grow_distributes_free_space() {
        let items = vec![
            FlexItem {
                grow: 1.0,
                ..FlexItem::new(10.0)
            },
            FlexItem {
                grow: 3.0,
                ..FlexItem::new(10.0)
            },
        ];
        let got = solve_flex(50.0, 0.0, &items);
        assert_eq!(got.len(), 2);
        assert!(close(got[0], 17.5), "{:?}", got);
        assert!(close(got[1], 32.5), "{:?}", got);
    }

    #[test]
    fn shrink_proportional_to_shrink_times_basis() {
        let items = vec![
            FlexItem {
                shrink: 1.0,
                ..FlexItem::new(100.0)
            },
            FlexItem {
                shrink: 1.0,
                ..FlexItem::new(300.0)
            },
        ];
        let got = solve_flex(200.0, 0.0, &items);
        // Overflow 200: losses 50 and 150.
        assert!(close(got[0], 50.0), "{:?}", got);
        assert!(close(got[1], 150.0), "{:?}", got);
    }

    #[test]
    fn gap_is_accounted() {
        let items = vec![FlexItem::new(10.0), FlexItem::new(10.0)];
        let got = solve_flex(30.0, 2.0, &items);
        // Free = 30 - 20 - 2 = 8; no grow: sizes stay 10 (packed).
        assert!(close(got[0], 10.0) && close(got[1], 10.0), "{:?}", got);
        let grow = vec![
            FlexItem {
                grow: 1.0,
                ..FlexItem::new(10.0)
            },
            FlexItem {
                grow: 1.0,
                ..FlexItem::new(10.0)
            },
        ];
        let got = solve_flex(30.0, 2.0, &grow);
        assert!(close(got[0], 14.0) && close(got[1], 14.0), "{:?}", got);
    }

    #[test]
    fn clamps_apply_last() {
        let items = vec![
            FlexItem {
                grow: 1.0,
                min: Some(20.0),
                max: None,
                ..FlexItem::new(0.0)
            },
            FlexItem {
                grow: 1.0,
                min: None,
                max: Some(15.0),
                ..FlexItem::new(0.0)
            },
        ];
        let got = solve_flex(100.0, 0.0, &items);
        assert!(
            close(got[0], 50.0),
            "no redistribution of clamped sibling: {:?}",
            got
        );
        assert!(close(got[1], 15.0), "max clamps: {:?}", got);
        let clamp = vec![FlexItem {
            grow: 1.0,
            min: Some(20.0),
            max: None,
            ..FlexItem::new(0.0)
        }];
        let one = solve_flex(10.0, 0.0, &clamp);
        assert!(close(one[0], 20.0), "min clamps: {:?}", one);
    }

    #[test]
    fn zero_grow_packs_at_start() {
        let items = vec![FlexItem::new(10.0), FlexItem::new(5.0)];
        let got = solve_flex(100.0, 0.0, &items);
        assert!(close(got[0], 10.0) && close(got[1], 5.0));
    }

    #[test]
    fn degenerate_inputs() {
        assert!(solve_flex(0.0, 0.0, &[FlexItem::new(1.0)]).is_empty());
        assert!(solve_flex(-5.0, 0.0, &[FlexItem::new(1.0)]).is_empty());
        assert!(solve_flex(100.0, 0.0, &[]).is_empty());
        assert!(solve_flex(f64::NAN, 0.0, &[FlexItem::new(1.0)]).is_empty());
    }

    #[test]
    fn all_zero_shrink_reduces_evenly() {
        let items = vec![
            FlexItem {
                shrink: 0.0,
                ..FlexItem::new(10.0)
            },
            FlexItem {
                shrink: 0.0,
                ..FlexItem::new(10.0)
            },
        ];
        let got = solve_flex(10.0, 0.0, &items);
        assert!(close(got[0], 5.0) && close(got[1], 5.0), "{:?}", got);
    }
}

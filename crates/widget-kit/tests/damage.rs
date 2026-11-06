//! Damage-plan rendering: a [`FramePlan`]-driven repaint must be
//! byte-identical to a full re-render, and `FramePlan::None` must
//! leave the previous frame untouched.

use scene_kit::{DamageTracker, FramePlan};
use widget_kit::{
    Size2D, TreeItem, WidgetColors, WidgetKind, WidgetTree, paint_tree, paint_tree_plan,
};

const W: f64 = 220.0;
const H: f64 = 130.0;

/// A small settings-shaped tree: label, row (button + checkbox),
/// select, tree list.
fn make_tree() -> (
    WidgetTree,
    widget_kit::WidgetRef,
    widget_kit::WidgetRef,
    widget_kit::WidgetRef,
) {
    let mut tree = WidgetTree::new();
    let root = tree.push(WidgetKind::Column { gap: 4.0 });
    let label = tree.push(WidgetKind::Label {
        text: "Settings".to_string(),
        px_size: 14.0,
    });
    tree.add_child(root, label);
    let row = tree.push(WidgetKind::Row { gap: 8.0 });
    let button = tree.push(WidgetKind::Button {
        label: "Save".to_string(),
        action: 1,
    });
    let checkbox = tree.push(WidgetKind::Checkbox {
        label: "Compact".to_string(),
        checked: false,
    });
    tree.add_child(root, row);
    tree.add_child(row, button);
    tree.add_child(row, checkbox);
    let list = tree.push(WidgetKind::TreeList {
        items: vec![
            TreeItem {
                label: "Projects".to_string(),
                depth: 0,
                has_children: true,
                expanded: true,
            },
            TreeItem {
                label: "fresco".to_string(),
                depth: 1,
                has_children: false,
                expanded: false,
            },
        ],
        selected: 0,
    });
    tree.add_child(root, list);
    tree.compute(Size2D {
        width: W,
        height: H,
    });
    (tree, button, checkbox, list)
}

fn render_full(tree: &WidgetTree) -> paint_kit::Surface {
    let mut surface = paint_kit::Surface::new(220, 130).unwrap_or_default();
    let mut painter = paint_kit::Painter::default();
    paint_tree(tree, &mut surface, &mut painter, &WidgetColors::default());
    surface
}

fn bytes(surface: &paint_kit::Surface) -> Vec<u8> {
    surface.data().to_vec()
}

/// Damage = old and new rects of the changed widget, coalesced by the
/// same tracker the compositor would use.
fn damage_plan(old: Option<paint_kit::Rect>, new: Option<paint_kit::Rect>) -> FramePlan {
    let mut tracker = DamageTracker::new(8, 0.9);
    for rect in [old, new].into_iter().flatten() {
        tracker.invalidate(rect);
    }
    tracker.take_plan(paint_kit::Rect::from_points(
        paint_kit::Vec2::new(0.0, 0.0),
        paint_kit::Vec2::new(W, H),
    ))
}

fn to_device(rect: widget_kit::Rect2D) -> paint_kit::Rect {
    paint_kit::Rect::from_points(
        paint_kit::Vec2::new(rect.x, rect.y),
        paint_kit::Vec2::new(rect.x + rect.width, rect.y + rect.height),
    )
}

#[test]
fn damaged_frame_matches_full_render() {
    let (mut tree, _button, checkbox, _list) = make_tree();

    // Frame A: full render.
    let mut frame = render_full(&tree);
    let Some(old_rect) = tree.widget(checkbox).map(|w| to_device(w.rect)) else {
        return;
    };

    // State change: toggle the checkbox in place.
    tree.set_checked(checkbox, true);
    tree.compute(Size2D {
        width: W,
        height: H,
    });
    let Some(new_rect) = tree.widget(checkbox).map(|w| to_device(w.rect)) else {
        return;
    };

    // Frame B: repaint only the damaged regions over frame A.
    let plan = damage_plan(Some(old_rect), Some(new_rect));
    let mut painter = paint_kit::Painter::default();
    paint_tree_plan(
        &tree,
        &mut frame,
        &mut painter,
        &WidgetColors::default(),
        &plan,
    );

    // Must be byte-identical to a full re-render of state B.
    let full = render_full(&tree);
    assert_eq!(
        bytes(&frame),
        bytes(&full),
        "damaged repaint must equal full render"
    );
}

#[test]
fn none_plan_leaves_frame_untouched() {
    let (mut tree, _button, _checkbox, _list) = make_tree();
    let mut frame = render_full(&tree);
    let before = bytes(&frame);

    // State changes, but the plan says nothing needs repainting.
    tree.set_checked(_checkbox, true);
    tree.compute(Size2D {
        width: W,
        height: H,
    });
    let mut painter = paint_kit::Painter::default();
    paint_tree_plan(
        &tree,
        &mut frame,
        &mut painter,
        &WidgetColors::default(),
        &FramePlan::None,
    );
    assert_eq!(bytes(&frame), before, "None must not touch the surface");
}

#[test]
fn tree_selection_move_damages_rows_not_panel() {
    let (mut tree, _button, _checkbox, list) = make_tree();

    let mut frame = render_full(&tree);
    let Some(old_rect) = tree.widget(list).map(|w| to_device(w.rect)) else {
        return;
    };

    // Selection moves: the highlight jumps rows inside the list.
    tree.tree_toggle(list, 1);
    tree.compute(Size2D {
        width: W,
        height: H,
    });
    let Some(new_rect) = tree.widget(list).map(|w| to_device(w.rect)) else {
        return;
    };

    let plan = damage_plan(Some(old_rect), Some(new_rect));
    let mut painter = paint_kit::Painter::default();
    paint_tree_plan(
        &tree,
        &mut frame,
        &mut painter,
        &WidgetColors::default(),
        &plan,
    );

    let full = render_full(&tree);
    assert_eq!(bytes(&frame), bytes(&full));

    // The plan must be regions, not a full repaint: the tree list is
    // far smaller than the viewport.
    assert!(
        matches!(plan, FramePlan::Regions(ref r) if r.len() <= 2),
        "a one-widget change must produce at most two regions: {plan:?}"
    );
}

#[test]
fn full_plan_equals_paint_tree() {
    let (tree, _button, _checkbox, _list) = make_tree();
    let mut surface = paint_kit::Surface::new(220, 130).unwrap_or_default();
    let mut painter = paint_kit::Painter::default();
    paint_tree_plan(
        &tree,
        &mut surface,
        &mut painter,
        &WidgetColors::default(),
        &FramePlan::Full,
    );
    assert_eq!(bytes(&surface), bytes(&render_full(&tree)));
}

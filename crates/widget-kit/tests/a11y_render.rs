//! A11y emission and rendering smoke tests.

use paint_kit::{Painter, Surface};
use widget_kit::{
    Size2D as WSize, WidgetColors, WidgetKind, WidgetTree, emit_a11y_tree, paint_tree,
};

fn demo() -> WidgetTree {
    let mut tree = WidgetTree::new();
    let root = tree.push(WidgetKind::Column { gap: 5.0 });
    let label = tree.push(WidgetKind::Label {
        text: "Inbox".into(),
        px_size: 14.0,
    });
    tree.add_child(root, label);
    let row = tree.push(WidgetKind::Row { gap: 8.0 });
    let button = tree.push(WidgetKind::Button {
        label: "Compose".into(),
        action: 1,
    });
    let checkbox = tree.push(WidgetKind::Checkbox {
        label: "Compact".into(),
        checked: true,
    });
    tree.add_child(root, row);
    tree.add_child(row, button);
    tree.add_child(row, checkbox);
    tree.set_focus(Some(button));
    tree
}

#[test]
fn a11y_tree_has_roles_and_audit_passes() {
    let mut tree = demo();
    tree.compute(WSize {
        width: 300.0,
        height: 120.0,
    });
    let a11y = emit_a11y_tree(&tree);
    // Paragraph + Group + Button + Checkbox emitted.
    let roles: Vec<String> = a11y
        .nodes()
        .iter()
        .map(|n| format!("{:?}", n.role))
        .collect();
    assert!(
        roles.iter().any(|r| r.contains("Button")),
        "roles: {roles:?}"
    );
    assert!(
        roles.iter().any(|r| r.contains("Checkbox")),
        "roles: {roles:?}"
    );
    assert!(
        roles.iter().any(|r| r.contains("Paragraph")),
        "roles: {roles:?}"
    );
    // Checkbox carried its checked state.
    let checked = a11y
        .nodes()
        .iter()
        .any(|n| n.states.checked == Some(a11y_kit::TriState::Checked));
    assert!(checked, "checkbox tri-state present");
    // The estate flagship gate: the emitted tree audits clean.
    assert!(a11y.audit().is_ok(), "a11y audit must pass");
}

#[test]
fn a11y_focus_state_tracks_widget_focus() {
    let tree = demo();
    let a11y = emit_a11y_tree(&tree);
    assert_eq!(a11y.focused().len(), 1, "exactly one focused node");
}

#[test]
fn paint_tree_produces_opaque_pixels() {
    let mut tree = demo();
    tree.compute(WSize {
        width: 300.0,
        height: 120.0,
    });
    let mut surface = Surface::new(300, 120)
        .unwrap_or_else(|| Surface::with_stride(1, 1, 4).unwrap_or_else(unreachable_surface));
    let mut painter = Painter::default();
    paint_tree(&tree, &mut surface, &mut painter, &WidgetColors::default());
    let mut lit = 0_u64;
    for (i, b) in surface.data().iter().enumerate() {
        if i % 4 == 3 && *b > 0 {
            lit += 1;
        }
    }
    assert!(lit > 1000, "paint covered {lit} pixels");
}

fn unreachable_surface() -> Surface {
    // 1x1 cannot overflow; unreachable in practice.
    match Surface::new(1, 1) {
        Some(s) => s,
        None => Surface::with_stride(1, 1, 4).unwrap_or_else(unreachable_surface),
    }
}

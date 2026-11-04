//! TreeList behavior: derived visibility, click select + toggle,
//! keyboard navigation over visible rows, collapse/expand semantics,
//! and a11y emission.

use widget_kit::{
    LogicalAt, NavKey, Size2D, TreeItem, WidgetColors, WidgetKind, WidgetRef, WidgetTree,
    emit_a11y_tree, tree_visible_rows,
};

fn demo_items() -> Vec<TreeItem> {
    vec![
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
        TreeItem {
            label: "kestrel".to_string(),
            depth: 1,
            has_children: false,
            expanded: false,
        },
        TreeItem {
            label: "Archive".to_string(),
            depth: 0,
            has_children: true,
            expanded: false,
        },
        TreeItem {
            label: "old".to_string(),
            depth: 1,
            has_children: false,
            expanded: false,
        },
    ]
}

fn make_tree() -> (WidgetTree, WidgetRef) {
    let mut tree = WidgetTree::new();
    let root = tree.push(WidgetKind::Column { gap: 0.0 });
    let list = tree.push(WidgetKind::TreeList {
        items: demo_items(),
        selected: 0,
    });
    tree.add_child(root, list);
    tree.compute(Size2D {
        width: 200.0,
        height: 96.0,
    });
    (tree, list)
}

fn selected_of(tree: &WidgetTree, list: WidgetRef) -> usize {
    tree.tree_selected(list).unwrap_or(usize::MAX)
}

fn expanded_of(tree: &WidgetTree, list: WidgetRef, row: usize) -> Option<bool> {
    match tree.widget(list).map(|w| &w.kind) {
        Some(WidgetKind::TreeList { items, .. }) => items.get(row).map(|i| i.expanded),
        _ => None,
    }
}

#[test]
fn tree_visible_rows_skip_collapsed_subtrees() {
    let items = demo_items();
    // "Archive" starts collapsed, so its child "old" is hidden up front.
    assert_eq!(tree_visible_rows(&items), [0, 1, 2, 3]);
    let mut collapsed_root = demo_items();
    if let Some(first) = collapsed_root.first_mut() {
        first.expanded = false;
    }
    // Projects' children vanish; Archive stays visible, its child not.
    assert_eq!(tree_visible_rows(&collapsed_root), [0, 3]);
    let mut expanded_leaf = collapsed_root.clone();
    if let Some(archive) = expanded_leaf.get_mut(3) {
        archive.expanded = true;
    }
    // Projects stays collapsed; Archive's child returns.
    assert_eq!(tree_visible_rows(&expanded_leaf), [0, 3, 4]);
}

#[test]
fn tree_click_selects_and_toggles() {
    let (mut tree, list) = make_tree();
    let rect = tree.widget(list).map(|w| w.rect).unwrap_or_default();
    // Row 0 = "Projects" (expanded): click toggles it collapsed.
    let at = LogicalAt {
        x: rect.x + 10.0,
        y: rect.y + widget_kit::TREE_ROW_H * 0.5,
    };
    tree.pointer_down(at);
    tree.pointer_up(at);
    assert_eq!(selected_of(&tree, list), 0, "click selects the row");
    assert_eq!(expanded_of(&tree, list, 0), Some(false), "click toggles");
    // "Archive" (row 3) now sits at visual slot 1: click selects and
    // expands it. Empty space below the rows (slot 3) must not.
    let at3 = LogicalAt {
        x: rect.x + 10.0,
        y: rect.y + widget_kit::TREE_ROW_H * 1.5,
    };
    tree.pointer_down(at3);
    tree.pointer_up(at3);
    assert_eq!(selected_of(&tree, list), 3);
    assert_eq!(expanded_of(&tree, list, 3), Some(true));
    assert_eq!(tree.tree_selected_label(list).as_deref(), Some("Archive"));
    // Empty space below the last row changes nothing.
    let at_empty = LogicalAt {
        x: rect.x + 10.0,
        y: rect.y + widget_kit::TREE_ROW_H * 3.5,
    };
    tree.pointer_down(at_empty);
    tree.pointer_up(at_empty);
    assert_eq!(selected_of(&tree, list), 3);
    assert_eq!(expanded_of(&tree, list, 3), Some(true));
}

#[test]
fn tree_keyboard_moves_selection_over_visible_rows() {
    let (mut tree, list) = make_tree();
    tree.set_focus(Some(list));
    tree.key_nav(NavKey::Down);
    tree.key_nav(NavKey::Down);
    assert_eq!(selected_of(&tree, list), 2, "Down steps visible rows");
    // Left on a leaf moves selection to the nearest ancestor.
    tree.key_nav(NavKey::Left);
    assert_eq!(selected_of(&tree, list), 0, "Left climbs to the ancestor");
    assert_eq!(expanded_of(&tree, list, 0), Some(true));
    // Right from an expanded parent steps into its first child.
    tree.key_nav(NavKey::Right);
    assert_eq!(selected_of(&tree, list), 1, "Right enters the first child");
    // Up returns to the parent row.
    tree.key_nav(NavKey::Up);
    assert_eq!(selected_of(&tree, list), 0);
}

#[test]
fn tree_keyboard_skips_collapsed_descendants() {
    let (mut tree, list) = make_tree();
    tree.set_focus(Some(list));
    // Collapse "Projects" from row 0.
    tree.key_nav(NavKey::Left);
    assert_eq!(expanded_of(&tree, list, 0), Some(false));
    // Down from row 0 lands on "Archive" (row 3), skipping hidden rows.
    tree.key_nav(NavKey::Down);
    assert_eq!(selected_of(&tree, list), 3);
    // Right expands "Archive"; Down steps into "old" (row 4).
    tree.key_nav(NavKey::Right);
    assert_eq!(expanded_of(&tree, list, 3), Some(true));
    tree.key_nav(NavKey::Down);
    assert_eq!(selected_of(&tree, list), 4);
    assert_eq!(tree.tree_selected_label(list).as_deref(), Some("old"));
}

#[test]
fn tree_activate_toggles_selected_row() {
    let (mut tree, list) = make_tree();
    tree.set_focus(Some(list));
    tree.activate(list);
    assert_eq!(
        expanded_of(&tree, list, 0),
        Some(false),
        "activate toggles the selected row"
    );
    // Leaves never toggle.
    tree.tree_toggle(list, 1);
    assert_eq!(selected_of(&tree, list), 1);
    assert_eq!(expanded_of(&tree, list, 1), Some(false));
}

#[test]
fn tree_a11y_surfaces_selection_as_list() {
    let (mut tree, list) = make_tree();
    tree.set_focus(Some(list));
    tree.tree_toggle(list, 1);
    let a11y = emit_a11y_tree(&tree);
    let roles: Vec<String> = a11y
        .nodes()
        .iter()
        .map(|n| format!("{:?}", n.role))
        .collect();
    assert!(roles.iter().any(|r| r.contains("List")), "roles: {roles:?}");
    let list_node = a11y
        .nodes()
        .iter()
        .find(|n| format!("{:?}", n.role).contains("List"));
    assert_eq!(
        list_node.map(|n| n.name.as_str()),
        Some("fresco"),
        "a11y name follows the selected row"
    );
    assert!(a11y.audit().is_ok(), "the emitted tree must audit clean");
}

#[test]
fn tree_render_differs_by_selection() {
    use widget_kit::paint_tree;
    let paint = |selected: usize| {
        let tree = {
            let mut tree = WidgetTree::new();
            let root = tree.push(WidgetKind::Column { gap: 0.0 });
            let list = tree.push(WidgetKind::TreeList {
                items: demo_items(),
                selected,
            });
            tree.add_child(root, list);
            tree.compute(Size2D {
                width: 200.0,
                height: 96.0,
            });
            tree
        };
        let mut surface = paint_kit::Surface::new(200, 96).unwrap_or_default();
        let mut painter = paint_kit::Painter::default();
        paint_tree(&tree, &mut surface, &mut painter, &WidgetColors::default());
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in surface.data() {
            h ^= u64::from(*byte);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        h
    };
    let a = paint(0);
    let b = paint(1);
    assert_ne!(a, b, "selection highlight must change rendered pixels");
}

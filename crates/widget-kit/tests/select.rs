//! Select behavior: open/close, click commit, keyboard stepping, a11y
//! emission, and render distinctness.

use widget_kit::{
    LogicalAt, NavKey, SELECT_HEADER_H, Size2D, TREE_ROW_H, WidgetColors, WidgetKind, WidgetRef,
    WidgetTree, emit_a11y_tree,
};

fn make_select() -> (WidgetTree, WidgetRef) {
    let mut tree = WidgetTree::new();
    let root = tree.push(WidgetKind::Column { gap: 0.0 });
    let sel = tree.push(WidgetKind::Select {
        options: vec!["Read".to_string(), "Unread".to_string(), "Sent".to_string()],
        selected: 0,
        open: false,
    });
    tree.add_child(root, sel);
    tree.compute(Size2D {
        width: 160.0,
        height: 120.0,
    });
    (tree, sel)
}

fn selected_of(tree: &WidgetTree, sel: WidgetRef) -> usize {
    tree.select_selected(sel).unwrap_or(usize::MAX)
}

/// Click the header (visual row 0).
fn click_header(tree: &mut WidgetTree, sel: WidgetRef) {
    let rect = tree.widget(sel).map(|w| w.rect).unwrap_or_default();
    let at = LogicalAt {
        x: rect.x + 20.0,
        y: rect.y + SELECT_HEADER_H * 0.5,
    };
    tree.pointer_down(at);
    tree.pointer_up(at);
}

/// Click option `idx` (visual row idx + 1, below the header).
fn click_option(tree: &mut WidgetTree, sel: WidgetRef, idx: usize) {
    let rect = tree.widget(sel).map(|w| w.rect).unwrap_or_default();
    let at = LogicalAt {
        x: rect.x + 20.0,
        y: rect.y + SELECT_HEADER_H + (idx as f64 + 0.5) * TREE_ROW_H,
    };
    tree.pointer_down(at);
    tree.pointer_up(at);
}

/// Click far below the option rows (empty space).
fn click_below(tree: &mut WidgetTree, sel: WidgetRef) {
    let rect = tree.widget(sel).map(|w| w.rect).unwrap_or_default();
    let at = LogicalAt {
        x: rect.x + 20.0,
        y: rect.y + 500.0,
    };
    tree.pointer_down(at);
    tree.pointer_up(at);
}

#[test]
fn select_click_opens_and_commits() {
    let (mut tree, sel) = make_select();
    assert_eq!(tree.select_is_open(sel), Some(false));
    // First click (header) opens.
    click_header(&mut tree, sel);
    assert_eq!(tree.select_is_open(sel), Some(true));
    // State changed -> the integration re-computes layout; without it
    // the grown rect is stale and the option click would miss.
    tree.compute(Size2D {
        width: 160.0,
        height: 120.0,
    });
    // Click on option 2 commits and closes.
    click_option(&mut tree, sel, 2);
    assert_eq!(selected_of(&tree, sel), 2);
    assert_eq!(tree.select_is_open(sel), Some(false));
    assert_eq!(tree.select_selected_label(sel).as_deref(), Some("Sent"));
}

#[test]
fn select_click_header_when_open_closes() {
    let (mut tree, sel) = make_select();
    click_header(&mut tree, sel);
    assert_eq!(tree.select_is_open(sel), Some(true));
    tree.compute(Size2D {
        width: 160.0,
        height: 120.0,
    });
    // Header click while open closes without changing the selection.
    click_header(&mut tree, sel);
    assert_eq!(tree.select_is_open(sel), Some(false));
    assert_eq!(selected_of(&tree, sel), 0);
}

#[test]
fn select_click_below_rows_is_ignored() {
    let (mut tree, sel) = make_select();
    click_header(&mut tree, sel);
    tree.compute(Size2D {
        width: 160.0,
        height: 120.0,
    });
    // A click far below the option rows: nothing changes.
    click_below(&mut tree, sel);
    assert_eq!(tree.select_is_open(sel), Some(true));
    assert_eq!(selected_of(&tree, sel), 0);
}

#[test]
fn select_keyboard_steps_and_closes() {
    let (mut tree, sel) = make_select();
    tree.set_focus(Some(sel));
    tree.key_nav(NavKey::Down);
    assert_eq!(tree.select_is_open(sel), Some(true));
    assert_eq!(selected_of(&tree, sel), 1);
    tree.key_nav(NavKey::Down);
    assert_eq!(selected_of(&tree, sel), 2);
    // Enter closes; selection is already committed (immediate-commit
    // style, like an HTML select driven by arrows).
    tree.activate(sel);
    assert_eq!(tree.select_is_open(sel), Some(false));
    assert_eq!(tree.select_selected_label(sel).as_deref(), Some("Sent"));
    // Up from option 2 steps to option 1 and re-opens the dropdown.
    tree.key_nav(NavKey::Up);
    assert_eq!(selected_of(&tree, sel), 1);
    assert_eq!(tree.select_is_open(sel), Some(true));
}

#[test]
fn select_open_changes_layout_height() {
    let height = |open: bool| {
        let mut tree = WidgetTree::new();
        let root = tree.push(WidgetKind::Column { gap: 0.0 });
        let sel = tree.push(WidgetKind::Select {
            options: vec!["Read".to_string(), "Unread".to_string()],
            selected: 0,
            open,
        });
        tree.add_child(root, sel);
        tree.compute(Size2D {
            width: 160.0,
            height: 120.0,
        });
        tree.widget(sel).map(|w| w.rect.height).unwrap_or_default()
    };
    let closed_h = height(false);
    let open_h = height(true);
    assert!(
        open_h > closed_h,
        "open dropdown must reserve option rows: {open_h} > {closed_h}"
    );
    // Closed = header only; open = header + one row per option.
    assert_eq!(closed_h, SELECT_HEADER_H);
    assert_eq!(open_h, SELECT_HEADER_H + 2.0 * TREE_ROW_H);
}

#[test]
fn select_a11y_surfaces_selection_as_combobox() {
    let (mut tree, sel) = make_select();
    tree.set_focus(Some(sel));
    tree.key_nav(NavKey::Down);
    let a11y = emit_a11y_tree(&tree);
    let roles: Vec<String> = a11y
        .nodes()
        .iter()
        .map(|n| format!("{:?}", n.role))
        .collect();
    assert!(
        roles.iter().any(|r| r.contains("ComboBox")),
        "roles: {roles:?}"
    );
    let node = a11y
        .nodes()
        .iter()
        .find(|n| format!("{:?}", n.role).contains("ComboBox"));
    assert_eq!(
        node.map(|n| n.name.as_str()),
        Some("Unread"),
        "a11y name follows the selected option"
    );
    assert!(a11y.audit().is_ok(), "the emitted tree must audit clean");
}

#[test]
fn select_render_differs_by_selection_and_state() {
    use widget_kit::paint_tree;
    let paint = |selected: usize, open: bool| {
        let mut tree = WidgetTree::new();
        let root = tree.push(WidgetKind::Column { gap: 0.0 });
        let sel = tree.push(WidgetKind::Select {
            options: vec!["Read".to_string(), "Unread".to_string()],
            selected,
            open,
        });
        tree.add_child(root, sel);
        tree.compute(Size2D {
            width: 160.0,
            height: 120.0,
        });
        let mut surface = paint_kit::Surface::new(160, 120).unwrap_or_default();
        let mut painter = paint_kit::Painter::default();
        paint_tree(&tree, &mut surface, &mut painter, &WidgetColors::default());
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in surface.data() {
            h ^= u64::from(*byte);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        h
    };
    assert_ne!(
        paint(0, false),
        paint(1, false),
        "selection must change rendered pixels"
    );
    assert_ne!(
        paint(0, false),
        paint(0, true),
        "open state must change rendered pixels"
    );
}

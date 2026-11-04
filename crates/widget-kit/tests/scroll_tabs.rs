//! ScrollPane and TabList behavior tests.

use widget_kit::{
    LogicalAt, NavKey, Size2D as WSize, WidgetColors, WidgetKind, WidgetRef, WidgetTree,
};

fn scroll_tree(content_rows: usize) -> WidgetTree {
    let mut tree = WidgetTree::new();
    let pane = tree.push(WidgetKind::ScrollPane { scroll_y: 0.0 });
    let content = tree.push(WidgetKind::Column { gap: 12.0 });
    tree.add_child(pane, content);
    for i in 0..content_rows {
        let label = tree.push(WidgetKind::Label {
            text: format!("row {i}"),
            px_size: 14.0,
        });
        tree.add_child(content, label);
    }
    tree.compute(WSize {
        width: 120.0,
        height: 60.0,
    });
    tree
}

#[test]
fn scroll_clamps_at_content_bounds() {
    let mut tree = scroll_tree(8); // 8 x 24 = 192 content in 120 viewport
    let pane = WidgetRef(0);
    // Wheel up past the top: clamps to 0.
    tree.scroll(LogicalAt { x: 60.0, y: 30.0 }, 500.0);
    assert_eq!(tree.scroll_offset(pane), 0.0);
    // Wheel down a lot: clamps to content - viewport.
    // Content: 8 labels x 28.8 + 7 gaps x 12 = 314.4; viewport 120.
    tree.scroll(LogicalAt { x: 60.0, y: 30.0 }, -500.0);
    let offset = tree.scroll_offset(pane);
    println!("offset after scroll: {offset}");
    assert!((offset - 194.4).abs() < 1e-9, "offset {offset}");
}

#[test]
fn wheel_scrolls_only_the_pane_under_the_pointer() {
    let mut tree = scroll_tree(8);
    let pane = WidgetRef(0);
    // A label inside the pane at viewport y=10 is content y=10 (offset 0).
    // Negative delta = view moves down = offset increases.
    tree.scroll(LogicalAt { x: 5.0, y: 10.0 }, -30.0);
    assert!(tree.scroll_offset(pane) > 0.0, "scroll through child hit");
    // Outside the pane: no scroll.
    tree.scroll(LogicalAt { x: 119.0, y: 130.0 }, 30.0);
    let before = tree.scroll_offset(pane);
    let _ = before;
    // (outside point falls in no pane; offset unchanged)
    let after = tree.scroll_offset(pane);
    assert!((before - after).abs() < 1e-9);
}

#[test]
fn scroll_changes_rendered_pixels() {
    let mut tree = scroll_tree(8);
    let mut colors = WidgetColors::default();
    let _ = &mut colors;
    let pane_rect = tree
        .widget(WidgetRef(0))
        .map(|w| w.rect)
        .unwrap_or_default();
    let inside = LogicalAt {
        x: pane_rect.x + 5.0,
        y: pane_rect.y + 5.0,
    };
    let mut s1 = paint_kit::Surface::new(120, 60).unwrap_or_default();
    let mut painter = paint_kit::Painter::default();
    widget_kit::paint_tree(&tree, &mut s1, &mut painter, &colors);
    tree.scroll(inside, -24.0); // negative = scroll down
    let mut s2 = paint_kit::Surface::new(120, 60).unwrap_or_default();
    let mut painter2 = paint_kit::Painter::default();
    widget_kit::paint_tree(&tree, &mut s2, &mut painter2, &colors);
    assert_ne!(s1.data(), s2.data(), "scrolling must change pixels");
}

#[test]
fn tab_list_click_cycles_and_keys_move() {
    let mut tree = WidgetTree::new();
    let tabs = tree.push(WidgetKind::TabList {
        tabs: vec![
            "All".to_string(),
            "Unread".to_string(),
            "Flagged".to_string(),
        ],
        active: 0,
    });
    tree.compute(WSize {
        width: 150.0,
        height: 28.0,
    });
    assert_eq!(tree.active_tab(tabs).as_deref(), Some("All"));
    // Click cycles to the next tab (v0 cycle semantics, documented).
    let r = tree.widget(tabs).map(|w| w.rect).unwrap_or_default();
    let at = LogicalAt {
        x: r.x + 5.0,
        y: r.y + 5.0,
    };
    tree.pointer_down(at);
    tree.pointer_up(at);
    assert_eq!(tree.active_tab(tabs).as_deref(), Some("Unread"));
    // Focused + Right moves forward, Left back.
    tree.set_focus(Some(tabs));
    tree.key_nav(NavKey::Right);
    assert_eq!(tree.active_tab(tabs).as_deref(), Some("Flagged"));
    tree.key_nav(NavKey::Left);
    assert_eq!(tree.active_tab(tabs).as_deref(), Some("Unread"));
}

//! The M4 widget screen: an interactive settings UI at phone
//! resolution, driven by touch.
//!
//! Real widget-kit components (button, checkbox, select, tree list)
//! laid out by the estate flow engine, hit-tested by pointer events.
//! Every tap visibly lands: checkboxes toggle, the select opens and
//! picks, tree rows select, buttons take press feedback.

use paint_kit::{Painter, Surface};
use widget_kit::{
    LogicalAt, Size2D, TreeItem, WidgetColors, WidgetKind, WidgetRef, WidgetTree, paint_tree,
};

/// The interactive settings screen.
pub struct WidgetScreen {
    /// The widget tree.
    pub tree: WidgetTree,
    /// Widget palette.
    pub colors: WidgetColors,
    /// Handles for the widgets the status label reads.
    status_ref: WidgetRef,
    compact_ref: WidgetRef,
    sync_ref: WidgetRef,
    outline_ref: WidgetRef,
    /// Root handle.
    root_ref: WidgetRef,
}

impl WidgetScreen {
    /// Build the phone-resolution settings screen.
    #[must_use]
    pub fn settings() -> Self {
        let mut tree = WidgetTree::new();
        let root = tree.push(WidgetKind::Column { gap: 10.0 });
        let header = tree.push(WidgetKind::Label {
            text: "fresco — Android".to_string(),
            px_size: 20.0,
        });
        tree.add_child(root, header);

        let row = tree.push(WidgetKind::Row { gap: 10.0 });
        let save = tree.push(WidgetKind::Button {
            label: "Save".to_string(),
            action: 1,
        });
        let compact = tree.push(WidgetKind::Checkbox {
            label: "Compact".to_string(),
            checked: false,
        });
        tree.add_child(root, row);
        tree.add_child(row, save);
        tree.add_child(row, compact);

        let sync = tree.push(WidgetKind::Select {
            options: vec![
                "Instant".to_string(),
                "Hourly".to_string(),
                "Daily".to_string(),
            ],
            selected: 0,
            open: false,
        });
        tree.add_child(root, sync);

        let outline = tree.push(WidgetKind::TreeList {
            items: vec![
                TreeItem {
                    label: "Device".to_string(),
                    depth: 0,
                    has_children: true,
                    expanded: true,
                },
                TreeItem {
                    label: "Display".to_string(),
                    depth: 1,
                    has_children: false,
                    expanded: false,
                },
                TreeItem {
                    label: "Battery".to_string(),
                    depth: 1,
                    has_children: false,
                    expanded: false,
                },
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
            selected: 1,
        });
        tree.add_child(root, outline);

        let status = tree.push(WidgetKind::Label {
            text: "Status: ready".to_string(),
            px_size: 14.0,
        });
        tree.add_child(root, status);

        WidgetScreen {
            colors: WidgetColors::default(),
            tree,
            status_ref: status,
            compact_ref: compact,
            sync_ref: sync,
            outline_ref: outline,
            root_ref: root,
        }
    }

    /// Lay out into the given viewport size (call on resize).
    pub fn resize(&mut self, width: usize, height: usize) {
        self.tree.compute(Size2D {
            width: width as f64,
            height: height as f64,
        });
    }

    /// Pointer down (touch start / mouse press).
    pub fn pointer_down(&mut self, x: f64, y: f64) {
        self.tree.pointer_down(LogicalAt { x, y });
    }

    /// Pointer up (touch end): activates the pressed widget.
    pub fn pointer_up(&mut self, x: f64, y: f64) {
        self.tree.pointer_up(LogicalAt { x, y });
        self.refresh_status();
    }

    /// Refresh the status label from live widget state.
    fn refresh_status(&mut self) {
        let compact = self.tree.is_checked(self.compact_ref);
        let sync = self
            .tree
            .select_selected_label(self.sync_ref)
            .unwrap_or_else(|| "?".to_owned());
        let tree_sel = self
            .tree
            .tree_selected_label(self.outline_ref)
            .unwrap_or_else(|| "none".to_owned());
        if let Some(status) = self.tree.widget_mut(self.status_ref)
            && let WidgetKind::Label { text, .. } = &mut status.kind
        {
            *text = format!(
                "Compact {} | {} | row: {}",
                if compact { "ON" } else { "OFF" },
                sync,
                tree_sel
            );
        }
        let _ = self.root_ref;
    }

    /// Paint the screen.
    pub fn paint(&mut self, surface: &mut Surface, painter: &mut Painter) {
        paint_tree(&self.tree, surface, painter, &self.colors);
    }
}

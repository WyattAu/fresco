//! AccessKit bridge (feature `accesskit`).
//!
//! Lowers the widget tree into an AccessKit [`TreeUpdate`]: the root
//! widget becomes the a11y Window, labels become Label nodes, buttons
//! Button, checkboxes CheckBox with their toggled state, text fields
//! TextEntry with their value. Focus rides on the update.

use crate::{WidgetKind, WidgetTree};
use accesskit::{Node, NodeId, Role, Toggled, TreeId, TreeInfo, TreeUpdate};

/// Node ids mirror widget arena indexes, offset by one: some adapters
/// treat 0 as a sentinel.
fn node_id(r: crate::WidgetRef) -> NodeId {
    NodeId((r.0 as u64).saturating_add(1))
}

/// A stable synthetic id for the i-th non-widget child of a widget
/// (tab entries, tree items): a 1000-id block per widget, far above
/// the real widget id space, so blocks and widgets never collide.
fn synthetic_id(r: crate::WidgetRef, index: usize) -> NodeId {
    NodeId((r.0 as u64) * 1000 + 1_000_001 + index as u64)
}

/// Build the full tree update (initial tree).
#[must_use]
pub fn emit_accesskit_update(tree: &WidgetTree) -> TreeUpdate {
    let mut update = TreeUpdate {
        nodes: Vec::new(),
        tree: None,
        tree_id: TreeId::ROOT,
        focus: NodeId(0),
    };
    let Some(root) = tree.root() else {
        return update;
    };
    update.focus = node_id(root);
    let mut built = Vec::new();
    walk(tree, Some(root), &mut built);
    update.nodes = built;
    let root_id = node_id(root);
    update.tree = Some(TreeInfo::new(root_id));
    if let Some(focused) = tree.focus() {
        update.focus = node_id(focused);
    }
    update
}

fn walk(tree: &WidgetTree, current: Option<crate::WidgetRef>, built: &mut Vec<(NodeId, Node)>) {
    let Some(r) = current else {
        return;
    };
    let Some(w) = tree.widget(r) else {
        return;
    };
    let mut node = Node::new(match &w.kind {
        WidgetKind::Label { .. } => Role::Label,
        WidgetKind::Button { .. } => Role::Button,
        WidgetKind::Checkbox { .. } => Role::CheckBox,
        WidgetKind::TextField { .. } => Role::TextInput,
        WidgetKind::Row { .. } | WidgetKind::Column { .. } | WidgetKind::Spacer => Role::Group,
        WidgetKind::ScrollPane { .. } => Role::List,
        WidgetKind::TabList { .. } => Role::TabList,
        WidgetKind::TreeList { .. } => Role::Tree,
        WidgetKind::Select { .. } => Role::ComboBox,
    });
    let mut extra_ids: Vec<NodeId> = Vec::new();
    match &w.kind {
        WidgetKind::Label { text, .. } => node.set_label(text.clone()),
        WidgetKind::Button { label, .. } => node.set_label(label.clone()),
        WidgetKind::Checkbox { label, checked } => {
            node.set_label(label.clone());
            node.set_toggled(Toggled::from(*checked));
        }
        WidgetKind::TextField { placeholder, field } => {
            let text = field.text();
            if text.is_empty() {
                node.set_label(placeholder.clone());
            } else {
                node.set_value(text);
            }
        }
        WidgetKind::Row { .. } | WidgetKind::Column { .. } | WidgetKind::Spacer => {}
        WidgetKind::ScrollPane { .. } => {}
        WidgetKind::TabList { tabs, active } => {
            // Each tab becomes an a11y Tab child; the active tab
            // carries the selected state.
            for (index, tab) in tabs.iter().enumerate() {
                let mut tab_node = Node::new(Role::Tab);
                tab_node.set_label(tab.clone());
                tab_node.set_selected(index == *active);
                let id = synthetic_id(r, index);
                extra_ids.push(id);
                built.push((id, tab_node));
            }
        }
        WidgetKind::TreeList { items, selected } => {
            if let Some(item) = items.get(*selected) {
                node.set_label(item.label.clone());
            }
            // Each row becomes an a11y TreeItem child; the selected
            // row carries the selected state.
            for (index, item) in items.iter().enumerate() {
                let mut item_node = Node::new(Role::TreeItem);
                item_node.set_label(item.label.clone());
                item_node.set_selected(index == *selected);
                let id = synthetic_id(r, index);
                extra_ids.push(id);
                built.push((id, item_node));
            }
        }
        WidgetKind::Select {
            options, selected, ..
        } => {
            if let Some(opt) = options.get(*selected) {
                node.set_label(opt.clone());
            }
        }
    }
    let mut child_ids: Vec<NodeId> = tree.children_of(r).iter().map(|c| node_id(*c)).collect();
    child_ids.extend(extra_ids.iter().copied());
    if !child_ids.is_empty() {
        node.set_children(child_ids);
    }
    built.push((node_id(r), node));
    for child in tree.children_of(r) {
        walk(tree, Some(child), built);
    }
}

use std::vec::Vec;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Size2D, WidgetKind, WidgetTree};

    /// Oracle (AK-1): a TabList emits a Tab child per tab with the
    /// active tab selected; a TreeList emits TreeItem children with
    /// the selected row marked; the focused widget carries the
    /// update's focus id.
    #[test]
    fn synthetic_children_and_focus() -> Result<(), Box<dyn std::error::Error>> {
        use crate::TreeItem;
        let mut tree = WidgetTree::new();
        let root = tree.push(WidgetKind::Column { gap: 6.0 });
        let button = tree.push(WidgetKind::Button {
            label: "go".to_string(),
            action: 1,
        });
        tree.add_child(root, button);
        let tabs = tree.push(WidgetKind::TabList {
            tabs: vec!["one".to_string(), "two".to_string()],
            active: 1,
        });
        tree.add_child(root, tabs);
        let list = tree.push(WidgetKind::TreeList {
            items: vec![
                TreeItem {
                    label: "alpha".to_string(),
                    depth: 0,
                    has_children: false,
                    expanded: false,
                },
                TreeItem {
                    label: "beta".to_string(),
                    depth: 0,
                    has_children: false,
                    expanded: false,
                },
            ],
            selected: 1,
        });
        tree.add_child(root, list);
        tree.set_focus(Some(button));
        tree.compute(Size2D {
            width: 300.0,
            height: 300.0,
        });

        let update = emit_accesskit_update(&tree);
        assert_eq!(update.focus, node_id(button), "focus follows the tree");

        let find = |id: NodeId| update.nodes.iter().find(|(nid, _)| *nid == id);
        let tabs_node = find(node_id(tabs)).ok_or("tab list node missing")?;
        assert!(tabs_node.1.children().len() >= 2, "tab children listed");
        for (index, name) in ["one", "two"].iter().enumerate() {
            let id = synthetic_id(tabs, index);
            let (nid, node) = find(id).ok_or(format!("tab {name} missing"))?;
            assert_eq!(node.role(), Role::Tab, "role for {name} at {nid:?}");
            assert_eq!(
                node.is_selected(),
                Some(index == 1),
                "active tab {name} selected state"
            );
        }
        for (index, name) in ["alpha", "beta"].iter().enumerate() {
            let id = synthetic_id(list, index);
            let (_, node) = find(id).ok_or(format!("item {name} missing"))?;
            assert_eq!(node.role(), Role::TreeItem, "role for {name}");
            assert_eq!(
                node.is_selected(),
                Some(index == 1),
                "selected row {name} state"
            );
        }
        Ok(())
    }
}

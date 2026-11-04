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
        WidgetKind::ScrollPane { .. } | WidgetKind::TabList { .. } => {}
        WidgetKind::TreeList { items, selected } => {
            if let Some(item) = items.get(*selected) {
                node.set_label(item.label.clone());
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
    let child_ids: Vec<NodeId> = tree.children_of(r).iter().map(|c| node_id(*c)).collect();
    if !child_ids.is_empty() {
        node.set_children(child_ids);
    }
    built.push((node_id(r), node));
    for child in tree.children_of(r) {
        walk(tree, Some(child), built);
    }
}

use std::vec::Vec;

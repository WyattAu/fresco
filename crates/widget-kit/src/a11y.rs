//! Accessibility emission into a11y-kit trees.
//!
//! Every widget becomes an audited semantic node: labels are paragraphs,
//! buttons are buttons, checkboxes carry their tri-state. The estate
//! flagship claim lives here: the UI's a11y tree is a build-tested
//! artifact, audited through a11y-kit's `TreeAudit` on demand.

use crate::{WidgetKind, WidgetRef, WidgetTree};
use a11y_kit::{A11yNode, A11yRole, A11yTree, NodeId, States, TriState};

/// Build an a11y tree from the widget tree.
///
/// Node ids mirror widget arena indexes (unique by construction). Focused
/// state is carried on the focused widget; checkboxes carry their
/// tri-state.
#[must_use]
pub fn emit_a11y_tree(tree: &WidgetTree) -> A11yTree {
    let mut out = A11yTree::new();
    let root = tree.root();
    walk(tree, root, None, &mut out);
    out
}

fn walk(
    tree: &WidgetTree,
    current: Option<WidgetRef>,
    parent_node: Option<NodeId>,
    out: &mut A11yTree,
) {
    let Some(r) = current else {
        return;
    };
    let Some(w) = tree.widget(r) else {
        return;
    };
    let is_root = tree.root() == Some(r);
    let (role, name, checked, focused) = match &w.kind {
        WidgetKind::Label { text, .. } => (A11yRole::Paragraph, text.clone(), None, false),
        WidgetKind::Button { label, .. } => (A11yRole::Button, label.clone(), None, w.focused),
        WidgetKind::Checkbox { label, checked } => (
            A11yRole::Checkbox,
            label.clone(),
            Some(if *checked {
                TriState::Checked
            } else {
                TriState::Unchecked
            }),
            w.focused,
        ),
        WidgetKind::Row { .. } | WidgetKind::Column { .. } => {
            // The root container is the a11y Window; nested flow
            // containers are Groups.
            (
                if is_root {
                    A11yRole::Window
                } else {
                    A11yRole::Group
                },
                String::new(),
                None,
                false,
            )
        }
        WidgetKind::Spacer => (A11yRole::Group, String::new(), None, false),
    };
    let states = States {
        focused,
        checked,
        ..States::default()
    };
    let node_id = NodeId(u32::try_from(r.0).unwrap_or(u32::MAX));
    let mut node = A11yNode::new(node_id, role, name).with_states(states);
    if let Some(p) = parent_node {
        node = node.with_parent(p);
    }
    out.insert(node);
    for child in tree.children_of(r) {
        walk(tree, Some(child), Some(node_id), out);
    }
}

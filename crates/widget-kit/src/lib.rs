//! Retained widget layer for the fresco UI engine.
//!
//! Phase 3, Gate D spike: the API shape is **retained property structs**
//! (widgets are data; the engine recomputes layout and redraws on
//! mutation), not a reactive signal graph (item 874). The spike evidence
//! lives in the tests: building, measuring, focusing, and activating a
//! kestrel-shaped screen with zero callbacks and zero closures. If the
//! consumer phase finds mutation ergonomics wanting, the signal layer
//! (874) arrives on top of this model, never instead of it.
//!
//! Responsibilities:
//!
//! - Widget model over an arena, with containers flowing via
//!   layout-kit's `FlowEngine` (Taffy is a consumer swap behind
//!   `LayoutEngine`, Gate B).
//! - Pointer hit testing and click pairing (down inside, up inside).
//! - Focus chain with wrapping Tab traversal (876) and keyboard
//!   activation: Enter/Space on buttons and checkboxes (875).
//! - Accessibility emission into a11y-kit trees, audited (the estate's
//!   flagship claim: every frame's a11y tree is auditable).
//! - Rendering through the [`WidgetRenderer`] contract; the shipped
//!   implementation paints via paint-kit. Label *text* pixels are a
//!   placeholder box in the stock renderer until the text-kit glyph
//!   path is wired at the consumer (documented limitation).
//!
//! Not in this crate: styling resolution (theme-kit wiring happens at
//! the consumer: colors are caller-supplied via [`WidgetColors`]),
//! animation, and platform input transport (window-kit).
#![forbid(unsafe_code)]

pub mod a11y;
#[cfg(feature = "accesskit")]
pub mod accesskit_bridge;
pub mod render;
pub mod theme;

pub use a11y::emit_a11y_tree;
#[cfg(feature = "accesskit")]
pub use accesskit_bridge::emit_accesskit_update;
pub use render::{
    DrawListPainter, WidgetColors, WidgetRenderer, paint_tree, paint_tree_plan, paint_tree_with,
};
#[cfg(feature = "strip-renderer")]
pub mod strip_renderer;
#[cfg(feature = "strip-renderer")]
pub use strip_renderer::StripListPainter;

use layout_kit::engine::NodeStyle;

// Re-exported: widget rects and sizes are layout geometry.
pub use layout_kit::engine::{Rect2D, Size2D};
use layout_kit::{Axis, FlowEngine, LayoutEngine};
use std::vec::Vec;
use text_kit::fake::FakeEngine;
use text_kit::{FontId, ShapeQuery, TextEngine};
use ui_text_edit::{StringBuffer, TextField as TextEditField};
pub use window_kit::event::NavKey;

/// Stable widget handle (index into the tree's arena).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WidgetRef(pub usize);

/// Widget kinds.
#[derive(Clone, Debug)]
pub enum WidgetKind {
    /// Static text.
    Label {
        /// The text.
        text: String,
        /// Pixel size.
        px_size: f64,
    },
    /// Pressable button that emits an action id.
    Button {
        /// Label text.
        label: String,
        /// Action id surfaced through [`WidgetTree::take_actions`].
        action: u32,
    },
    /// Toggleable checkbox.
    Checkbox {
        /// Label text.
        label: String,
        /// Checked state.
        checked: bool,
    },
    /// Horizontal flow container.
    Row {
        /// Gap between children.
        gap: f64,
    },
    /// Vertical flow container.
    Column {
        /// Gap between children.
        gap: f64,
    },
    /// Flexible empty space.
    Spacer,
    /// Single-line text entry over a ui-text-edit field.
    TextField {
        /// Placeholder shown when empty.
        placeholder: String,
        /// The field (buffer + caret + selection).
        field: TextEditField<StringBuffer>,
    },
    /// Clipped viewport over one content container. Wheel input moves
    /// [`WidgetTree::scroll_offset`]; rendering clips to the viewport.
    ScrollPane {
        /// Vertical scroll offset in logical pixels (0 = top).
        scroll_y: f64,
    },
    /// Tab strip: the active tab is the shown name; Left/Right (focused)
    /// or click move it.
    TabList {
        /// Tab names in order.
        tabs: Vec<String>,
        /// Active tab index.
        active: usize,
    },
    /// Dropdown selector: closed shows the selected option; opening
    /// reveals one row per option. Click opens, click on an option
    /// commits + closes, click on the header closes. Up/Down (focused)
    /// open and step the selection with immediate commit; Enter closes.
    Select {
        /// Options in order.
        options: Vec<String>,
        /// Selected option index.
        selected: usize,
        /// Dropdown open state.
        open: bool,
    },
    /// Collapsible outline: flat rows with depth and expand state.
    /// Click selects + toggles; Up/Down move selection among visible
    /// rows; Left/Right collapse/expand.
    TreeList {
        /// Rows in display order (ancestors before descendants).
        items: Vec<TreeItem>,
        /// Selected row index (an index into `items`).
        selected: usize,
    },
}

/// One row of a [`WidgetKind::TreeList`]: label, nesting depth, and
/// expand state. Rows are stored flat; visibility (which rows render,
/// hit-test, and take keyboard selection) is derived: a row is visible
/// when every ancestor is expanded.
#[derive(Clone, Debug)]
pub struct TreeItem {
    /// Row text.
    pub label: String,
    /// Nesting depth (0 = top level).
    pub depth: usize,
    /// Children can exist under this row (expand marker shown).
    pub has_children: bool,
    /// Expanded state (meaningless without `has_children`).
    pub expanded: bool,
}

/// Row height of a [`WidgetKind::TreeList`] and of a [`WidgetKind::Select`]
/// option row.
pub const TREE_ROW_H: f64 = 24.0;

/// Closed header height of a [`WidgetKind::Select`].
pub const SELECT_HEADER_H: f64 = 26.0;

/// Per-depth indent of a [`WidgetKind::TreeList`] row.
pub const TREE_INDENT: f64 = 16.0;

/// Indexes of the visible rows of `items`: a row is visible when every
/// ancestor is expanded. Collapsed subtrees vanish from rendering, hit
/// testing, and keyboard selection alike.
#[must_use]
pub fn tree_visible_rows(items: &[TreeItem]) -> std::vec::Vec<usize> {
    let mut out = std::vec::Vec::new();
    let mut hide_from: Option<usize> = None;
    for (i, item) in items.iter().enumerate() {
        if let Some(d) = hide_from {
            if item.depth > d {
                continue;
            }
            hide_from = None;
        }
        out.push(i);
        if item.has_children && !item.expanded {
            hide_from = Some(item.depth);
        }
    }
    out
}

/// Widget state: kind plus geometry and interaction flags.
#[derive(Debug)]
pub struct Widget {
    /// What it is.
    pub kind: WidgetKind,
    /// Fixed width override.
    pub fixed_width: Option<f64>,
    /// Fixed height override.
    pub fixed_height: Option<f64>,
    /// Fraction of free space to absorb.
    pub grow: f64,
    /// Computed rect (after `compute`).
    pub rect: Rect2D,
    /// Pointer is currently held down on this widget.
    pub pressed: bool,
    /// Keyboard focus.
    pub focused: bool,
}

impl Widget {
    fn new(kind: WidgetKind) -> Self {
        Widget {
            kind,
            fixed_width: None,
            fixed_height: None,
            grow: 0.0,
            rect: Rect2D::default(),
            pressed: false,
            focused: false,
        }
    }

    /// Focusable: buttons and checkboxes.
    #[must_use]
    pub fn is_focusable(&self) -> bool {
        matches!(
            self.kind,
            WidgetKind::Button { .. }
                | WidgetKind::Checkbox { .. }
                | WidgetKind::TreeList { .. }
                | WidgetKind::Select { .. }
        )
    }

    /// Rect inflated by the hit-test slop (touch targets).
    fn hit_rect(&self) -> Rect2D {
        self.rect
    }
}

/// The retained widget tree.
#[derive(Default)]
pub struct WidgetTree {
    widgets: Vec<Widget>,
    children: Vec<Vec<WidgetRef>>,
    root: Option<WidgetRef>,
    focus: Option<WidgetRef>,
    pressed: Option<WidgetRef>,
    actions: Vec<u32>,
    engine: FakeEngine,
    flow_keys: Vec<usize>,
}

impl WidgetTree {
    /// New empty tree.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a widget; returns its handle.
    pub fn push(&mut self, kind: WidgetKind) -> WidgetRef {
        self.widgets.push(Widget::new(kind));
        self.children.push(Vec::new());
        let r = WidgetRef(self.widgets.len() - 1);
        // The first pushed widget is the root container.
        if self.root.is_none() {
            self.root = Some(r);
        }
        r
    }

    /// Add `child` under `parent`. The first added widget at depth zero
    /// becomes the root.
    pub fn add_child(&mut self, parent: WidgetRef, child: WidgetRef) {
        if let Some(list) = self.children.get_mut(parent.0) {
            list.push(child);
        }
        if self.root.is_none() && parent.0 == 0 {
            self.root = Some(parent);
        }
    }

    /// Root handle once a widget exists.
    #[must_use]
    pub fn root(&self) -> Option<WidgetRef> {
        self.root
    }

    /// Widget access.
    #[must_use]
    pub fn widget(&self, r: WidgetRef) -> Option<&Widget> {
        self.widgets.get(r.0)
    }

    /// Children of a widget, in discovery order.
    #[must_use]
    pub fn children_of(&self, r: WidgetRef) -> Vec<WidgetRef> {
        self.children.get(r.0).cloned().unwrap_or_default()
    }

    /// Mutable widget access.
    pub fn widget_mut(&mut self, r: WidgetRef) -> Option<&mut Widget> {
        self.widgets.get_mut(r.0)
    }

    /// Currently focused widget.
    #[must_use]
    pub fn focus(&self) -> Option<WidgetRef> {
        self.focus
    }

    /// Drained action queue (button activations in order).
    pub fn take_actions(&mut self) -> std::vec::Vec<u32> {
        std::mem::take(&mut self.actions)
    }

    /// Set checkbox state directly.
    pub fn set_checked(&mut self, r: WidgetRef, checked: bool) {
        if let Some(Widget {
            kind: WidgetKind::Checkbox { checked: c, .. },
            ..
        }) = self.widgets.get_mut(r.0)
        {
            *c = checked;
        }
    }

    /// Checkbox state.
    #[must_use]
    pub fn is_checked(&self, r: WidgetRef) -> bool {
        match self.widgets.get(r.0).map(|w| &w.kind) {
            Some(WidgetKind::Checkbox { checked, .. }) => *checked,
            _ => false,
        }
    }

    /// Compute layout into `size` using the estate flow engine.
    pub fn compute(&mut self, size: Size2D) {
        let mut engine = FlowEngine::new();
        let Some(root) = self.root else {
            return;
        };
        self.flow_keys.clear();
        self.flow_keys.resize(self.widgets.len(), usize::MAX);
        let root_key = self.build_layout_node(&mut engine, root);
        if engine.compute(root_key, size).is_err() {
            return;
        }
        self.apply_rects(&mut engine, root);
    }

    fn node_style(&self, r: WidgetRef) -> NodeStyle {
        let Some(w) = self.widgets.get(r.0) else {
            return NodeStyle::default();
        };
        let mut style = match &w.kind {
            WidgetKind::Row { gap } => NodeStyle::container(Axis::Row, *gap),
            WidgetKind::Column { gap } => NodeStyle::container(Axis::Column, *gap),
            WidgetKind::Spacer => {
                return NodeStyle {
                    grow: 1.0,
                    ..NodeStyle::default()
                };
            }
            WidgetKind::TextField { .. } => {
                return NodeStyle {
                    width: w.fixed_width.or(Some(120.0)),
                    height: w.fixed_height.or(Some(28.0)),
                    grow: w.grow,
                    ..NodeStyle::default()
                };
            }
            WidgetKind::ScrollPane { .. } => {
                return NodeStyle {
                    direction: Axis::Column,
                    height: w.fixed_height.or(Some(120.0)),
                    grow: w.grow,
                    ..NodeStyle::default()
                };
            }
            WidgetKind::TabList { .. } => {
                return NodeStyle {
                    height: w.fixed_height.or(Some(28.0)),
                    grow: w.grow,
                    ..NodeStyle::default()
                };
            }
            WidgetKind::TreeList { items, .. } => {
                let visible = tree_visible_rows(items).len().max(1);
                let h = f64::from(u16::try_from(visible).unwrap_or(u16::MAX)) * TREE_ROW_H;
                return NodeStyle {
                    height: w.fixed_height.or(Some(h)),
                    grow: w.grow,
                    ..NodeStyle::default()
                };
            }
            WidgetKind::Select { options, open, .. } => {
                let rows = if *open { options.len() + 1 } else { 1 };
                let h = SELECT_HEADER_H
                    + f64::from(u16::try_from(rows.saturating_sub(1)).unwrap_or(u16::MAX))
                        * TREE_ROW_H;
                return NodeStyle {
                    height: w.fixed_height.or(Some(h)),
                    grow: w.grow,
                    ..NodeStyle::default()
                };
            }
            _ => NodeStyle::default(),
        };
        style.width = w.fixed_width;
        style.height = w.fixed_height;
        style.grow = w.grow;
        style
    }

    fn build_layout_node(&mut self, engine: &mut FlowEngine, r: WidgetRef) -> usize {
        let style = self.node_style(r);
        let children = self.children.get(r.0).cloned().unwrap_or_default();
        let key = if children.is_empty() {
            // Leaves get measured content sizes from the text engine.
            let mut style = style;
            if style.width.is_none() {
                style.width = Some(self.measure_width(r));
            }
            if style.height.is_none() {
                style.height = Some(self.measure_height(r));
            }
            engine.add_leaf(style)
        } else {
            let keys: Vec<usize> = children
                .iter()
                .map(|c| self.build_layout_node(engine, *c))
                .collect();
            engine.add_container(style, &keys)
        };
        if let Some(slot) = self.flow_keys.get_mut(r.0) {
            *slot = key;
        }
        key
    }

    fn text_of(&self, r: WidgetRef) -> Option<String> {
        match self.widgets.get(r.0).map(|w| &w.kind) {
            Some(WidgetKind::Label { text, .. }) => Some(text.clone()),
            Some(WidgetKind::Button { label, .. }) => Some(label.clone()),
            Some(WidgetKind::Checkbox { label, .. }) => Some(label.clone()),
            Some(WidgetKind::TextField { placeholder, field }) => {
                let text = field.text();
                if text.is_empty() {
                    Some(placeholder.clone())
                } else {
                    Some(text.to_string())
                }
            }
            Some(WidgetKind::TabList { tabs, active }) => {
                Some(tabs.get(*active).cloned().unwrap_or_default())
            }
            Some(WidgetKind::TreeList { items, selected }) => Some(
                items
                    .get(*selected)
                    .map(|i| i.label.clone())
                    .unwrap_or_default(),
            ),
            Some(WidgetKind::Select {
                options, selected, ..
            }) => Some(options.get(*selected).cloned().unwrap_or_default()),
            _ => None,
        }
    }

    fn measure_width(&mut self, r: WidgetRef) -> f64 {
        let Some(text) = self.text_of(r) else {
            return 0.0;
        };
        let shaped = self.engine.shape(&ShapeQuery {
            text: text.as_str(),
            font: FontId(0),
            px_size: 14.0,
            letter_spacing: 0.0,
        });
        shaped.width + 16.0
    }

    fn measure_height(&mut self, r: WidgetRef) -> f64 {
        let has_text = self.text_of(r).is_some();
        if has_text { 14.0 * 1.2 + 12.0 } else { 24.0 }
    }

    fn apply_rects(&mut self, engine: &mut FlowEngine, r: WidgetRef) {
        let flow = self.flow_keys.get(r.0).copied().unwrap_or(usize::MAX);
        if let Some(rect) = engine.rect(flow)
            && let Some(w) = self.widgets.get_mut(r.0)
        {
            w.rect = rect;
        }
        let kids = self.children.get(r.0).cloned().unwrap_or_default();
        for k in kids {
            self.apply_rects(engine, k);
        }
    }

    /// Push a single-line text field with a placeholder.
    pub fn add_text_field(&mut self, parent: WidgetRef, placeholder: &str) -> WidgetRef {
        let r = self.push(WidgetKind::TextField {
            placeholder: placeholder.to_string(),
            field: TextEditField::new(StringBuffer::new()),
        });
        self.add_child(parent, r);
        r
    }

    /// Focus the next focusable widget in tree order, wrapping.
    pub fn focus_next(&mut self) {
        self.step_focus(1);
    }

    /// Focus the previous focusable widget, wrapping.
    pub fn focus_prev(&mut self) {
        self.step_focus(-1);
    }

    fn focusables(&self) -> Vec<WidgetRef> {
        self.widgets
            .iter()
            .enumerate()
            .filter(|(_, w)| w.is_focusable())
            .map(|(i, _)| WidgetRef(i))
            .collect()
    }

    fn step_focus(&mut self, dir: i32) {
        let focusables = self.focusables();
        if focusables.is_empty() {
            self.focus = None;
            return;
        }
        let current = self
            .focus
            .and_then(|f| focusables.iter().position(|&r| r == f));
        let next = match current {
            Some(i) => {
                let n = focusables.len() as i32;
                let j = (i as i32 + dir).rem_euclid(n);
                focusables.get(j as usize).copied()
            }
            None => focusables.first().copied(),
        };
        self.set_focus(next);
    }

    /// Set focus explicitly (clears the previous holder).
    pub fn set_focus(&mut self, target: Option<WidgetRef>) {
        let previous = self.focus;
        self.focus = target;
        if let Some(r) = previous
            && let Some(w) = self.widgets.get_mut(r.0)
        {
            w.focused = false;
        }
        if let Some(r) = target
            && let Some(w) = self.widgets.get_mut(r.0)
        {
            w.focused = true;
        }
    }

    /// Pointer down: records the press target (hit test front-most).
    pub fn pointer_down(&mut self, at: LogicalAt) {
        let hit = self.hit_test(at);
        if let Some(r) = hit
            && let Some(w) = self.widgets.get_mut(r.0)
        {
            w.pressed = true;
        }
        self.pressed = hit;
    }

    /// Pointer up: pairs with the press to activate.
    pub fn pointer_up(&mut self, at: LogicalAt) {
        let release_target = self.hit_test(at);
        let press_target = self.pressed.take();
        if let (Some(p), Some(r)) = (press_target, release_target)
            && p == r
        {
            self.activate_at(r, at);
        }
        for w in &mut self.widgets {
            w.pressed = false;
        }
    }

    /// Keyboard character: Tab traversal and activation keys; printable
    /// characters route to the focused text field.
    pub fn key_char(&mut self, ch: char) {
        match ch {
            '\t' => self.focus_next(),
            '\n' | ' ' => {
                if let Some(f) = self.focus {
                    if self.kind_is_text_field(f) {
                        self.field_insert(f, ch);
                    } else {
                        self.activate(f);
                    }
                }
            }
            c => {
                if let Some(f) = self.focus
                    && self.kind_is_text_field(f)
                {
                    self.field_insert(f, c);
                }
            }
        }
    }

    /// Navigation keys: caret motion for the focused text field, focus
    /// traversal for BackTab, activation for Enter.
    pub fn key_nav(&mut self, key: NavKey) {
        match key {
            NavKey::Tab => self.focus_next(),
            NavKey::BackTab => self.focus_prev(),
            NavKey::Backspace => {
                if let Some(f) = self.focus {
                    self.field_backspace(f);
                }
            }
            NavKey::Delete => {
                if let Some(f) = self.focus {
                    self.field_delete_forward(f);
                }
            }
            NavKey::Left => {
                if let Some(f) = self.focus {
                    if self.kind_is_text_field(f) {
                        self.field_move(f, -1);
                    } else if self.kind_is_tab_list(f) {
                        self.tab_step(f, -1);
                    } else if self.kind_is_tree_list(f) {
                        self.tree_collapse(f);
                    }
                }
            }
            NavKey::Right => {
                if let Some(f) = self.focus {
                    if self.kind_is_text_field(f) {
                        self.field_move(f, 1);
                    } else if self.kind_is_tab_list(f) {
                        self.tab_step(f, 1);
                    } else if self.kind_is_tree_list(f) {
                        self.tree_expand(f);
                    }
                }
            }
            NavKey::Up => {
                if let Some(f) = self.focus {
                    if self.kind_is_tree_list(f) {
                        self.tree_step(f, -1);
                    } else if self.kind_is_select(f) {
                        self.select_step(f, -1);
                    }
                }
            }
            NavKey::Down => {
                if let Some(f) = self.focus {
                    if self.kind_is_tree_list(f) {
                        self.tree_step(f, 1);
                    } else if self.kind_is_select(f) {
                        self.select_step(f, 1);
                    }
                }
            }
            NavKey::Home => {
                if let Some(f) = self.focus
                    && self.kind_is_text_field(f)
                {
                    self.field_home(f);
                }
            }
            NavKey::End => {
                if let Some(f) = self.focus
                    && self.kind_is_text_field(f)
                {
                    self.field_end(f);
                }
            }
        }
    }

    /// Enter for the facade's KeyChar '\n' on text fields inserts a
    /// newline only in multiline fields; single-line fields activate
    /// like buttons (no-op v0).
    fn kind_is_text_field(&self, r: WidgetRef) -> bool {
        matches!(
            self.widgets.get(r.0).map(|w| &w.kind),
            Some(WidgetKind::TextField { .. })
        )
    }

    fn kind_is_tab_list(&self, r: WidgetRef) -> bool {
        matches!(
            self.widgets.get(r.0).map(|w| &w.kind),
            Some(WidgetKind::TabList { .. })
        )
    }

    fn kind_is_tree_list(&self, r: WidgetRef) -> bool {
        matches!(
            self.widgets.get(r.0).map(|w| &w.kind),
            Some(WidgetKind::TreeList { .. })
        )
    }

    fn kind_is_select(&self, r: WidgetRef) -> bool {
        matches!(
            self.widgets.get(r.0).map(|w| &w.kind),
            Some(WidgetKind::Select { .. })
        )
    }

    /// Open a focused Select (if closed) and step the selection by
    /// `delta` with immediate commit (wrapping, skipping nothing).
    pub fn select_step(&mut self, r: WidgetRef, delta: i32) {
        let Some(Widget {
            kind:
                WidgetKind::Select {
                    options,
                    selected,
                    open,
                },
            ..
        }) = self.widgets.get_mut(r.0)
        else {
            return;
        };
        if options.is_empty() {
            return;
        }
        *open = true;
        let n = options.len() as i32;
        *selected = ((*selected as i32) + delta).rem_euclid(n) as usize;
    }

    /// Selected option index of a Select.
    #[must_use]
    pub fn select_selected(&self, r: WidgetRef) -> Option<usize> {
        match self.widgets.get(r.0).map(|w| &w.kind) {
            Some(WidgetKind::Select { selected, .. }) => Some(*selected),
            _ => None,
        }
    }

    /// Selected option text of a Select.
    #[must_use]
    pub fn select_selected_label(&self, r: WidgetRef) -> Option<String> {
        match self.widgets.get(r.0).map(|w| &w.kind) {
            Some(WidgetKind::Select {
                options, selected, ..
            }) => options.get(*selected).cloned(),
            _ => None,
        }
    }

    /// Whether a Select is open.
    #[must_use]
    pub fn select_is_open(&self, r: WidgetRef) -> Option<bool> {
        match self.widgets.get(r.0).map(|w| &w.kind) {
            Some(WidgetKind::Select { open, .. }) => Some(*open),
            _ => None,
        }
    }

    /// Select an option by index and close the dropdown (no-op for an
    /// out-of-range row).
    pub fn select_pick(&mut self, r: WidgetRef, row: usize) {
        let Some(Widget {
            kind:
                WidgetKind::Select {
                    options,
                    selected,
                    open,
                },
            ..
        }) = self.widgets.get_mut(r.0)
        else {
            return;
        };
        if options.get(row).is_some() {
            *selected = row;
            *open = false;
        }
    }

    /// Pointer activation: TreeList selects the clicked visible row and
    /// toggles it when it has children (clicks on empty space below the
    /// rows do nothing); Select opens on a header click and picks +
    /// closes on an option-row click; everything else defers to
    /// `activate`.
    pub fn activate_at(&mut self, r: WidgetRef, at: LogicalAt) {
        let rel = at.y - self.widgets.get(r.0).map_or(0.0, |w| w.rect.y);
        let list = match self.widgets.get(r.0).map(|w| &w.kind) {
            Some(WidgetKind::TreeList { items, .. }) => {
                let visible = tree_visible_rows(items);
                if rel < 0.0 {
                    return;
                }
                visible.get((rel / TREE_ROW_H) as usize).copied()
            }
            Some(WidgetKind::Select { options, open, .. }) => {
                if !*open {
                    // Header click on a closed dropdown opens it.
                    return self.select_open(r);
                }
                // Open: row 0 is the header (closes); rows below are
                // options (pick + close).
                if rel < SELECT_HEADER_H {
                    return self.activate(r);
                }
                let row = ((rel - SELECT_HEADER_H) / TREE_ROW_H) as usize;
                if row < options.len() {
                    return self.select_pick(r, row);
                }
                return;
            }
            _ => None,
        };
        if self.kind_is_tree_list(r) {
            let Some(row) = list else {
                return;
            };
            if let Some(Widget {
                kind: WidgetKind::TreeList { items, selected },
                ..
            }) = self.widgets.get_mut(r.0)
                && items.get(row).is_some()
            {
                *selected = row;
                if let Some(item) = items.get_mut(row)
                    && item.has_children
                {
                    item.expanded = !item.expanded;
                }
            }
            return;
        }
        self.activate(r);
    }

    /// Open a closed Select (leaves an open one open).
    fn select_open(&mut self, r: WidgetRef) {
        if let Some(Widget {
            kind: WidgetKind::Select { open, .. },
            ..
        }) = self.widgets.get_mut(r.0)
        {
            *open = true;
        }
    }

    /// Move the selection of a focused TreeList among visible rows
    /// (wrapping). Collapsed descendants are skipped.
    pub fn tree_step(&mut self, r: WidgetRef, delta: i32) {
        let Some(Widget {
            kind: WidgetKind::TreeList { items, selected },
            ..
        }) = self.widgets.get_mut(r.0)
        else {
            return;
        };
        let visible = tree_visible_rows(items);
        if visible.is_empty() {
            return;
        }
        let pos = visible.iter().position(|v| *v == *selected).unwrap_or(0);
        let n = visible.len() as i32;
        let next = ((pos as i32) + delta).rem_euclid(n) as usize;
        *selected = visible.get(next).copied().unwrap_or(*selected);
    }

    /// Collapse the selected row; already-collapsed (or leaf) rows move
    /// selection to the nearest ancestor instead.
    pub fn tree_collapse(&mut self, r: WidgetRef) {
        let Some(Widget {
            kind: WidgetKind::TreeList { items, selected },
            ..
        }) = self.widgets.get_mut(r.0)
        else {
            return;
        };
        let Some(item) = items.get_mut(*selected) else {
            return;
        };
        if item.has_children && item.expanded {
            item.expanded = false;
            return;
        }
        let depth = item.depth;
        let mut i = *selected;
        while i > 0 {
            i -= 1;
            if items.get(i).is_some_and(|p| p.depth < depth) {
                *selected = i;
                return;
            }
        }
    }

    /// Expand the selected row; already-expanded (or leaf) rows move
    /// selection to the first child (the next row) instead.
    pub fn tree_expand(&mut self, r: WidgetRef) {
        let Some(Widget {
            kind: WidgetKind::TreeList { items, selected },
            ..
        }) = self.widgets.get_mut(r.0)
        else {
            return;
        };
        if let Some(item) = items.get_mut(*selected)
            && item.has_children
            && !item.expanded
        {
            item.expanded = true;
            return;
        }
        let depth = items.get(*selected).map_or(0, |i| i.depth);
        let Some(next) = items.get(*selected + 1) else {
            return;
        };
        if next.depth > depth {
            *selected += 1;
        }
    }

    /// Selected row index of a TreeList.
    #[must_use]
    pub fn tree_selected(&self, r: WidgetRef) -> Option<usize> {
        match self.widgets.get(r.0).map(|w| &w.kind) {
            Some(WidgetKind::TreeList { selected, .. }) => Some(*selected),
            _ => None,
        }
    }

    /// Selected row label of a TreeList.
    #[must_use]
    pub fn tree_selected_label(&self, r: WidgetRef) -> Option<String> {
        match self.widgets.get(r.0).map(|w| &w.kind) {
            Some(WidgetKind::TreeList { items, selected }) => {
                items.get(*selected).map(|i| i.label.clone())
            }
            _ => None,
        }
    }

    /// Toggle the expanded state of row `row` and select it (no-op for
    /// leaves).
    pub fn tree_toggle(&mut self, r: WidgetRef, row: usize) {
        let Some(Widget {
            kind: WidgetKind::TreeList { items, selected },
            ..
        }) = self.widgets.get_mut(r.0)
        else {
            return;
        };
        if let Some(item) = items.get_mut(row)
            && item.has_children
        {
            item.expanded = !item.expanded;
        }
        if items.get(row).is_some() {
            *selected = row;
        }
    }

    /// Move the active tab of a TabList by `delta` (wrapping).
    pub fn tab_step(&mut self, r: WidgetRef, delta: i32) {
        let Some(Widget {
            kind: WidgetKind::TabList { tabs, active },
            ..
        }) = self.widgets.get_mut(r.0)
        else {
            return;
        };
        if tabs.is_empty() {
            return;
        }
        let n = tabs.len() as i32;
        *active = ((*active as i32) + delta).rem_euclid(n) as usize;
    }

    /// Active tab name of a TabList.
    #[must_use]
    pub fn active_tab(&self, r: WidgetRef) -> Option<String> {
        match self.widgets.get(r.0).map(|w| &w.kind) {
            Some(WidgetKind::TabList { tabs, active }) => tabs.get(*active).cloned(),
            _ => None,
        }
    }

    /// Scroll a ScrollPane (or the ScrollPane containing the hit widget)
    /// by `delta_y` (positive = view moves up). The offset clamps to
    /// `[0, content height - viewport height]`.
    pub fn scroll(&mut self, at: LogicalAt, delta_y: f64) {
        let Some(hit) = self.hit_test(at) else {
            return;
        };
        let mut cursor = hit;
        let pane = loop {
            if matches!(
                self.widgets.get(cursor.0).map(|w| &w.kind),
                Some(WidgetKind::ScrollPane { .. })
            ) {
                break Some(cursor);
            }
            match self.parent_of(cursor) {
                Some(p) => cursor = p,
                None => break None,
            }
        };
        let Some(pane) = pane else {
            return;
        };
        let current = match self.widgets.get(pane.0).map(|w| &w.kind) {
            Some(WidgetKind::ScrollPane { scroll_y }) => *scroll_y,
            _ => return,
        };
        let Some((content_h, viewport_h)) = self.scroll_extents(pane) else {
            return;
        };
        let max = (content_h - viewport_h).max(0.0);
        let next = (current - delta_y).clamp(0.0, max);
        if let Some(Widget {
            kind: WidgetKind::ScrollPane { scroll_y },
            ..
        }) = self.widgets.get_mut(pane.0)
        {
            *scroll_y = next;
        }
    }

    /// Scroll offset of a ScrollPane.
    #[must_use]
    pub fn scroll_offset(&self, r: WidgetRef) -> f64 {
        match self.widgets.get(r.0).map(|w| &w.kind) {
            Some(WidgetKind::ScrollPane { scroll_y }) => *scroll_y,
            _ => 0.0,
        }
    }

    fn scroll_extents(&self, pane: WidgetRef) -> Option<(f64, f64)> {
        let w = self.widgets.get(pane.0)?;
        let viewport_h = if w.rect.height > 0.0 {
            w.rect.height
        } else {
            w.fixed_height.unwrap_or(120.0)
        };
        let child = self.children_of(pane).first().copied()?;
        let content_h = self.widget(child).map(|c| c.rect.height).unwrap_or(0.0);
        Some((content_h, viewport_h))
    }

    /// Parent handle of a widget (None for the root or orphans).
    #[must_use]
    pub fn parent_of(&self, r: WidgetRef) -> Option<WidgetRef> {
        for (i, kids) in self.children.iter().enumerate() {
            if kids.contains(&r) {
                return Some(WidgetRef(i));
            }
        }
        None
    }

    fn with_field(&mut self, r: WidgetRef, f: impl FnOnce(&mut TextEditField<StringBuffer>)) {
        if let Some(Widget {
            kind: WidgetKind::TextField { field, .. },
            ..
        }) = self.widgets.get_mut(r.0)
        {
            f(field);
        }
    }

    fn field_insert(&mut self, r: WidgetRef, ch: char) {
        self.with_field(r, |field| {
            field.replace_selection(&ch.to_string());
        });
    }

    fn field_backspace(&mut self, r: WidgetRef) {
        self.with_field(r, |field| {
            field.backspace();
        });
    }

    fn field_delete_forward(&mut self, r: WidgetRef) {
        self.with_field(r, |field| {
            field.delete_forward();
        });
    }

    fn field_move(&mut self, r: WidgetRef, delta: i32) {
        self.with_field(r, |field| {
            if delta < 0 {
                field.move_left();
            } else {
                field.move_right();
            }
        });
    }

    fn field_home(&mut self, r: WidgetRef) {
        self.with_field(r, |field| field.move_to_start());
    }

    fn field_end(&mut self, r: WidgetRef) {
        self.with_field(r, |field| field.move_to_end());
    }

    /// Text content of a text field.
    #[must_use]
    pub fn field_text(&self, r: WidgetRef) -> String {
        match self.widgets.get(r.0).map(|w| &w.kind) {
            Some(WidgetKind::TextField { field, .. }) => field.text().to_string(),
            _ => String::new(),
        }
    }

    /// Caret cluster index of a text field.
    #[must_use]
    pub fn field_caret(&self, r: WidgetRef) -> usize {
        match self.widgets.get(r.0).map(|w| &w.kind) {
            Some(WidgetKind::TextField { field, .. }) => field.caret(),
            _ => 0,
        }
    }

    /// Mutable field access (tests and consumers).
    pub fn field_mut(&mut self, r: WidgetRef) -> Option<&mut TextEditField<StringBuffer>> {
        match self.widgets.get_mut(r.0).map(|w| &mut w.kind) {
            Some(WidgetKind::TextField { field, .. }) => Some(field),
            _ => None,
        }
    }

    /// Activate a widget (button action or checkbox toggle).
    pub fn activate(&mut self, r: WidgetRef) {
        let Some(w) = self.widgets.get_mut(r.0) else {
            return;
        };
        match &mut w.kind {
            WidgetKind::Button { action, .. } => {
                self.actions.push(*action);
            }
            WidgetKind::Checkbox { checked, .. } => {
                *checked = !*checked;
            }
            WidgetKind::TabList { tabs, active } if !tabs.is_empty() => {
                *active = ((*active as i32) + 1).rem_euclid(tabs.len() as i32) as usize;
            }
            WidgetKind::TreeList { items, selected } => {
                if let Some(item) = items.get_mut(*selected)
                    && item.has_children
                {
                    item.expanded = !item.expanded;
                }
            }
            WidgetKind::Select { open, .. } => {
                *open = !*open;
            }
            _ => {}
        }
    }

    /// Front-most widget whose rect contains `at` (depth-first, last
    /// child first matches paint order).
    #[must_use]
    pub fn hit_test(&self, at: LogicalAt) -> Option<WidgetRef> {
        let mut best: Option<WidgetRef> = None;
        for (i, w) in self.widgets.iter().enumerate() {
            let r = w.hit_rect();
            let inside =
                at.x >= r.x && at.y >= r.y && at.x <= r.x + r.width && at.y <= r.y + r.height;
            if inside {
                best = Some(WidgetRef(i));
            }
        }
        best
    }
}

/// Logical point (owned variant for input APIs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogicalAt {
    /// X.
    pub x: f64,
    /// Y.
    pub y: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_tree() -> WidgetTree {
        let mut tree = WidgetTree::new();
        let root = tree.push(WidgetKind::Column { gap: 5.0 });
        let _ = tree.push(WidgetKind::Label {
            text: "Inbox".to_string(),
            px_size: 14.0,
        });
        let row = tree.push(WidgetKind::Row { gap: 8.0 });
        let button = tree.push(WidgetKind::Button {
            label: "Compose".to_string(),
            action: 1,
        });
        let checkbox = tree.push(WidgetKind::Checkbox {
            label: "Compact".to_string(),
            checked: false,
        });
        tree.add_child(root, row);
        tree.add_child(row, button);
        tree.add_child(row, checkbox);
        tree
    }

    #[test]
    fn compute_assigns_nested_rects() {
        let mut tree = demo_tree();
        tree.compute(Size2D {
            width: 300.0,
            height: 120.0,
        });
        let row = WidgetRef(2);
        let button = WidgetRef(3);
        let (rr, br) = (
            tree.widget(row).map(|w| w.rect).unwrap_or_default(),
            tree.widget(button).map(|w| w.rect).unwrap_or_default(),
        );
        assert!(rr.width > 0.0 && rr.height > 0.0);
        assert!(br.width >= rr.width || br.width > 0.0);
        assert!((br.y - rr.y).abs() < 1e-9, "button inside row row-band");
    }

    #[test]
    fn click_button_emits_action_once() {
        let mut tree = demo_tree();
        tree.compute(Size2D {
            width: 300.0,
            height: 120.0,
        });
        let button = WidgetRef(3);
        let r = tree.widget(button).map(|w| w.rect).unwrap_or_default();
        let at = LogicalAt {
            x: r.x + 2.0,
            y: r.y + 2.0,
        };
        tree.pointer_down(at);
        tree.pointer_up(at);
        tree.pointer_down(at);
        // Released well past the button's right edge.
        tree.pointer_up(LogicalAt {
            x: r.x + r.width + 40.0,
            y: r.y + 2.0,
        });
        assert_eq!(
            tree.take_actions(),
            vec![1],
            "one full click fires once; release outside cancels"
        );
    }

    #[test]
    fn checkbox_toggles_by_click_and_by_key() {
        let mut tree = demo_tree();
        tree.compute(Size2D {
            width: 300.0,
            height: 120.0,
        });
        let checkbox = WidgetRef(4);
        let r = tree.widget(checkbox).map(|w| w.rect).unwrap_or_default();
        let at = LogicalAt {
            x: r.x + 1.0,
            y: r.y + 1.0,
        };
        tree.pointer_down(at);
        tree.pointer_up(at);
        assert!(tree.is_checked(checkbox), "click toggles on");
        tree.pointer_down(at);
        tree.pointer_up(at);
        assert!(!tree.is_checked(checkbox), "click toggles off");
        // Keyboard: focus it and press space.
        tree.set_focus(Some(checkbox));
        tree.key_char(' ');
        assert!(tree.is_checked(checkbox), "space toggles");
    }

    #[test]
    fn focus_chain_wraps_tab_traversal() {
        let mut tree = demo_tree();
        tree.compute(Size2D {
            width: 300.0,
            height: 120.0,
        });
        assert_eq!(tree.focus(), None);
        tree.key_char('\t');
        let first = tree.focus();
        assert!(first.is_some(), "tab focuses the first focusable");
        tree.key_char('\t');
        let second = tree.focus();
        assert_ne!(first, second);
        tree.key_char('\t');
        assert_eq!(tree.focus(), first, "wraps to the first focusable");
        // Enter on a focused button fires its action.
        tree.set_focus(Some(WidgetRef(3)));
        tree.key_char('\n');
        assert_eq!(tree.take_actions(), vec![1]);
    }
}

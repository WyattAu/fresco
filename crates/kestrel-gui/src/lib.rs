//! Kestrel GUI: the estate's consumer application shell, bootstrapped
//! on the fresco UI stack.
//!
//! The integration milestone for the fresco roadmap: a product-facing
//! application hosting real widget-kit components (labels, buttons)
//! laid out by the flow engine, painted through paint-kit's raster,
//! with pacing evidence wired from frame one.
//!
//! M9 bootstrap scope: a static info panel (title, lines, a refresh
//! button) rendered into any paint-kit surface. Windowed shells
//! follow the fresco-game pattern.

use paint_kit::{Painter, Surface};
use widget_kit::{LogicalAt, Size2D, WidgetColors, WidgetKind, WidgetRef, WidgetTree, paint_tree};

/// The kestrel panel: title, status lines, refresh button.
pub struct KestrelPanel {
    tree: WidgetTree,
    colors: WidgetColors,
    status_ref: WidgetRef,
    refresh_ref: WidgetRef,
    refreshes: u64,
}

impl Default for KestrelPanel {
    fn default() -> Self {
        KestrelPanel::new()
    }
}

impl KestrelPanel {
    /// Build the panel.
    #[must_use]
    pub fn new() -> Self {
        let mut tree = WidgetTree::new();
        let root = tree.push(WidgetKind::Column { gap: 12.0 });
        let title = tree.push(WidgetKind::Label {
            text: "kestrel".to_string(),
            px_size: 22.0,
        });
        tree.add_child(root, title);
        let status = tree.push(WidgetKind::Label {
            text: "panel ready".to_string(),
            px_size: 14.0,
        });
        tree.add_child(root, status);
        let refresh = tree.push(WidgetKind::Button {
            label: "Refresh".to_string(),
            action: 1,
        });
        tree.add_child(root, refresh);
        KestrelPanel {
            tree,
            colors: WidgetColors::default(),
            status_ref: status,
            refresh_ref: refresh,
            refreshes: 0,
        }
    }

    /// Lay out into the viewport (call on resize).
    pub fn resize(&mut self, width: usize, height: usize) {
        self.tree.compute(Size2D {
            width: f64::from(u32::try_from(width.max(1)).unwrap_or(1)),
            height: f64::from(u32::try_from(height.max(1)).unwrap_or(1)),
        });
    }

    /// Pointer down (touch start / mouse press).
    pub fn pointer_down(&mut self, x: f64, y: f64) {
        self.tree.pointer_down(LogicalAt { x, y });
    }

    /// Pointer up: activates the pressed widget.
    pub fn pointer_up(&mut self, x: f64, y: f64) {
        self.tree.pointer_up(LogicalAt { x, y });
        let actions = self.tree.take_actions();
        if actions.contains(&1) {
            self.refreshes += 1;
            self.refresh_status();
        }
    }

    /// Refresh the status label from live state.
    fn refresh_status(&mut self) {
        if let Some(w) = self.tree.widget_mut(self.status_ref)
            && let WidgetKind::Label { text, .. } = &mut w.kind
        {
            *text = format!("panel ready  refreshes {}", self.refreshes);
        }
    }

    /// The status label's text (oracle use).
    #[must_use]
    pub fn status_text(&self) -> String {
        match self.tree.widget(self.status_ref) {
            Some(w) => match &w.kind {
                WidgetKind::Label { text, .. } => text.clone(),
                _ => String::new(),
            },
            None => String::new(),
        }
    }

    /// Refresh count (oracle use).
    #[must_use]
    pub const fn refreshes(&self) -> u64 {
        self.refreshes
    }

    /// The refresh button's laid-out rect (oracle use).
    #[must_use]
    pub fn refresh_rect(&self) -> Option<(f64, f64, f64, f64)> {
        let w = self.tree.widget(self.refresh_ref)?;
        Some((w.rect.x, w.rect.y, w.rect.width, w.rect.height))
    }

    /// Paint the panel.
    pub fn paint(&mut self, surface: &mut Surface, painter: &mut Painter) {
        paint_tree(&self.tree, surface, painter, &self.colors);
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    /// Oracle: the panel paints deterministically.
    #[test]
    fn panel_paints_deterministically() {
        let paint = || {
            let mut panel = KestrelPanel::new();
            panel.resize(360, 740);
            let mut surface = Surface::new(360, 740).unwrap_or_default();
            let mut painter = Painter::default();
            panel.paint(&mut surface, &mut painter);
            surface.data().to_vec()
        };
        assert_eq!(paint(), paint(), "panel bytes must be deterministic");
    }

    /// Oracle: the refresh button increments the counter and updates
    /// the status label.
    #[test]
    fn refresh_button_updates_status() {
        let mut panel = KestrelPanel::new();
        panel.resize(360, 740);
        // Press + release on the laid-out button center.
        let Some((bx, by, bw, bh)) = panel.refresh_rect() else {
            return;
        };
        panel.pointer_down(bx + bw * 0.5, by + bh * 0.5);
        panel.pointer_up(bx + bw * 0.5, by + bh * 0.5);
        assert_eq!(panel.refreshes(), 1, "first refresh");
        assert!(
            panel.status_text().contains("refreshes 1"),
            "status reflects refreshes: {:?}",
            panel.status_text()
        );
    }

    /// Oracle: status starts at zero refreshes.
    #[test]
    fn status_starts_ready() {
        let panel = KestrelPanel::new();
        assert_eq!(panel.status_text(), "panel ready");
        assert_eq!(panel.refreshes(), 0);
    }
}

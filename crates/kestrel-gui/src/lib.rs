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

pub mod app;

use audio_kit::{Envelope, Oscillator, SilentSink, WaveKind, render_clip};
use paint_kit::{Painter, Surface};
use widget_kit::{LogicalAt, Size2D, WidgetColors, WidgetKind, WidgetRef, WidgetTree, paint_tree};

/// The shell's click: a deterministic audio-kit clip played into the
/// SilentSink on kestrel's own button activations (the consumer
/// shell's audio hook - the same pattern fresco-game's Sfx uses).
struct Click {
    sink: SilentSink,
    clip: Vec<f32>,
}

impl Click {
    fn new() -> Self {
        let mut osc = Oscillator::new(WaveKind::Sine, 660.0);
        let mut env = Envelope::adsr(0.002, 0.04, 0.0, 0.02);
        let clip = render_clip(&mut osc, &mut env, 0.06, 8000);
        Click {
            sink: SilentSink::new(),
            clip,
        }
    }

    fn play(&mut self) {
        for frame in self.clip.chunks(1) {
            self.sink.write(frame);
        }
    }

    fn frames(&self) -> u64 {
        self.sink.frames_written()
    }

    fn len(&self) -> usize {
        self.clip.len()
    }
}

/// The kestrel panel: title, status lines, refresh button.
pub struct KestrelPanel {
    tree: WidgetTree,
    colors: WidgetColors,
    status_ref: WidgetRef,
    best_ref: WidgetRef,
    refresh_ref: WidgetRef,
    play_ref: WidgetRef,
    refreshes: u64,
    /// The displayed best score (read from fresco-game's persist dir).
    best: u64,
    click: Click,
    /// Play activation staged by `pointer_up`: the tree's action
    /// queue is drained there (the refresh handler), so `play_pressed`
    /// must consume a staging flag, not the spent queue.
    play_pending: bool,
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
        let best = tree.push(WidgetKind::Label {
            text: "best 0".to_string(),
            px_size: 14.0,
        });
        tree.add_child(root, best);
        let refresh = tree.push(WidgetKind::Button {
            label: "Refresh".to_string(),
            action: 1,
        });
        tree.add_child(root, refresh);
        let play = tree.push(WidgetKind::Button {
            label: "Play fresco game".to_string(),
            action: 2,
        });
        tree.add_child(root, play);
        KestrelPanel {
            tree,
            colors: WidgetColors::default(),
            status_ref: status,
            best_ref: best,
            refresh_ref: refresh,
            play_ref: play,
            refreshes: 0,
            best: 0,
            click: Click::new(),
            play_pending: false,
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
            self.best = fresco_game::best_score();
            self.refresh_status();
            self.set_best(self.best);
            self.click.play();
        }
        if actions.contains(&2) {
            self.play_pending = true;
            self.click.play();
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

    /// Whether the Play button activated since the last call.
    ///
    /// Consumes the staging flag `pointer_up` set: the tree's action
    /// queue is drained during `pointer_up` (the refresh handler
    /// lives there), so reading the queue here saw it already spent
    /// and the Play button could never activate.
    pub fn play_pressed(&mut self) -> bool {
        std::mem::take(&mut self.play_pending)
    }

    /// Set the displayed best score (the shell reads it from the
    /// game's persist dir and pushes it here).
    pub fn set_best(&mut self, best: u64) {
        self.best = best;
        if let Some(w) = self.tree.widget_mut(self.best_ref)
            && let WidgetKind::Label { text, .. } = &mut w.kind
        {
            *text = format!("best {best}");
        }
    }

    /// The best label's text (oracle use).
    #[must_use]
    pub fn best_text(&self) -> String {
        match self.tree.widget(self.best_ref) {
            Some(w) => match &w.kind {
                WidgetKind::Label { text, .. } => text.clone(),
                _ => String::new(),
            },
            None => String::new(),
        }
    }

    /// Click frames written to the sink (oracle use).
    #[must_use]
    pub fn click_frames(&self) -> u64 {
        self.click.frames()
    }

    /// The click clip's length in samples (oracle use).
    #[must_use]
    pub fn click_clip_len(&self) -> usize {
        self.click.len()
    }

    /// The Play button's laid-out rect (oracle + routing use).
    #[must_use]
    pub fn play_rect(&self) -> Option<(f64, f64, f64, f64)> {
        let w = self.tree.widget(self.play_ref)?;
        Some((w.rect.x, w.rect.y, w.rect.width, w.rect.height))
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

    /// Oracle: set_best updates the best label.
    #[test]
    fn set_best_updates_label() {
        let mut panel = KestrelPanel::new();
        assert_eq!(panel.best_text(), "best 0");
        panel.set_best(42);
        assert_eq!(panel.best_text(), "best 42");
    }

    /// Oracle: kestrel's own button activations feed the click into
    /// the sink - one activation writes exactly one clip.
    #[test]
    fn button_click_feeds_sink() {
        let mut panel = KestrelPanel::new();
        panel.resize(360, 740);
        let before = panel.click_frames();
        let Some((bx, by, bw, bh)) = panel.refresh_rect() else {
            return;
        };
        panel.pointer_down(bx + bw * 0.5, by + bh * 0.5);
        panel.pointer_up(bx + bw * 0.5, by + bh * 0.5);
        assert_eq!(
            panel.click_frames(),
            before + panel.click_clip_len() as u64,
            "refresh writes one click"
        );
        let Some((px, py, pw, ph)) = panel.play_rect() else {
            return;
        };
        panel.pointer_down(px + pw * 0.5, py + ph * 0.5);
        panel.pointer_up(px + pw * 0.5, py + ph * 0.5);
        assert!(panel.play_pressed(), "play activated");
        assert_eq!(
            panel.click_frames(),
            before + 2 * panel.click_clip_len() as u64,
            "play writes the second click"
        );
    }
}

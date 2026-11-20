//! The game's main menu: real widget-kit components (label, select,
//! button) laid out by the estate flow engine - the same retained-mode
//! UI layer a full app would host.
//!
//! Play transitions to the game; the difficulty select maps to the
//! fountain rate (Relaxed 30 / Normal 15 / Frenzy 6 sim frames per
//! spawned ball).

use paint_kit::{Painter, Surface};
use widget_kit::{LogicalAt, Size2D, WidgetColors, WidgetKind, WidgetRef, WidgetTree, paint_tree};

/// Difficulty choice: sim frames between fountain spawns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Difficulty {
    /// A ball every 30 frames.
    Relaxed,
    /// A ball every 15 frames (default).
    Normal,
    /// A ball every 6 frames.
    Frenzy,
}

impl Difficulty {
    /// Fountain cadence in sim frames.
    #[must_use]
    pub const fn fountain_every(self) -> u64 {
        match self {
            Difficulty::Relaxed => 30,
            Difficulty::Normal => 15,
            Difficulty::Frenzy => 6,
        }
    }

    /// From the select's option index.
    #[must_use]
    pub const fn from_index(i: usize) -> Self {
        match i {
            0 => Difficulty::Relaxed,
            2 => Difficulty::Frenzy,
            _ => Difficulty::Normal,
        }
    }
}

/// The main menu screen.
pub struct MenuScreen {
    tree: WidgetTree,
    colors: WidgetColors,
    play_ref: WidgetRef,
    difficulty_ref: WidgetRef,
    best_ref: WidgetRef,
}

impl Default for MenuScreen {
    fn default() -> Self {
        MenuScreen::new()
    }
}

impl MenuScreen {
    /// Build the menu.
    #[must_use]
    pub fn new() -> Self {
        let mut tree = WidgetTree::new();
        let root = tree.push(WidgetKind::Column { gap: 14.0 });
        // ASCII-only strings: realfont covers them and the micro-font
        // fallback stays ASCII-safe.
        let title = tree.push(WidgetKind::Label {
            text: "fresco game".to_string(),
            px_size: 24.0,
        });
        tree.add_child(root, title);
        let subtitle = tree.push(WidgetKind::Label {
            text: "pop targets - grab + fling balls".to_string(),
            px_size: 13.0,
        });
        tree.add_child(root, subtitle);
        let difficulty = tree.push(WidgetKind::Select {
            options: vec![
                "Relaxed".to_string(),
                "Normal".to_string(),
                "Frenzy".to_string(),
            ],
            selected: 1,
            open: false,
        });
        tree.add_child(root, difficulty);
        let best = tree.push(WidgetKind::Label {
            text: "best 0".to_string(),
            px_size: 13.0,
        });
        tree.add_child(root, best);
        let play = tree.push(WidgetKind::Button {
            label: "Play".to_string(),
            action: 1,
        });
        tree.add_child(root, play);
        MenuScreen {
            tree,
            colors: WidgetColors::default(),
            play_ref: play,
            difficulty_ref: difficulty,
            best_ref: best,
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

    /// Pointer move: no menu widgets track hover yet.
    pub fn pointer_move(&mut self, _x: f64, _y: f64) {}

    /// Keyboard navigation (a11y): Tab/arrows move focus and step
    /// the select; Enter/Space activate the focused widget. Forwards
    /// to the tree's focus machinery.
    pub fn key_nav(&mut self, key: window_kit::event::NavKey) {
        self.tree.key_nav(key);
    }

    /// Keyboard activation: Enter/Space on the focused button.
    pub fn key_char(&mut self, ch: char) {
        self.tree.key_char(ch);
    }

    /// Pointer up: activates the pressed widget; drain the action
    /// queue afterward via [`Self::play_pressed`].
    pub fn pointer_up(&mut self, x: f64, y: f64) {
        self.tree.pointer_up(LogicalAt { x, y });
    }

    /// Whether the Play button activated since the last call.
    pub fn play_pressed(&mut self) -> bool {
        self.tree.take_actions().contains(&1)
    }

    /// Show the persisted best score on the best label.
    pub fn set_best(&mut self, best: u64) {
        if let Some(w) = self.tree.widget_mut(self.best_ref)
            && let WidgetKind::Label { text, .. } = &mut w.kind
        {
            *text = format!("best {best}");
        }
    }

    /// The best label's current text (oracle use).
    #[must_use]
    pub fn best_label(&self) -> String {
        match self.tree.widget(self.best_ref) {
            Some(w) => match &w.kind {
                WidgetKind::Label { text, .. } => text.clone(),
                _ => String::new(),
            },
            None => String::new(),
        }
    }

    /// The selected difficulty.
    #[must_use]
    pub fn difficulty(&self) -> Difficulty {
        Difficulty::from_index(self.tree.select_selected(self.difficulty_ref).unwrap_or(1))
    }

    /// Paint the menu.
    pub fn paint(&mut self, surface: &mut Surface, painter: &mut Painter) {
        paint_tree(&self.tree, surface, painter, &self.colors);
    }

    /// The Play button's laid-out rect (oracle use).
    #[must_use]
    pub fn play_rect(&self) -> Option<(f64, f64, f64, f64)> {
        let w = self.tree.widget(self.play_ref)?;
        Some((w.rect.x, w.rect.y, w.rect.width, w.rect.height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: a press + release inside the Play button rect queues
    /// action 1 (the play transition).
    #[test]
    fn play_button_activates() {
        let mut menu = MenuScreen::new();
        menu.resize(360, 740);
        let Some((bx, by, bw, bh)) = menu.play_rect() else {
            return;
        };
        let cx = bx + bw * 0.5;
        let cy = by + bh * 0.5;
        assert!(!menu.play_pressed(), "no press yet");
        menu.pointer_down(cx, cy);
        menu.pointer_up(cx, cy);
        assert!(menu.play_pressed(), "press on Play must queue action 1");
        assert!(
            !menu.play_pressed(),
            "actions drain: queue must be empty after take"
        );
    }

    /// Oracle: the difficulty select maps to fountain rates.
    #[test]
    fn difficulty_maps_to_rate() {
        let mut menu = MenuScreen::new();
        menu.resize(360, 740);
        assert_eq!(menu.difficulty(), Difficulty::Normal);
        assert_eq!(Difficulty::Relaxed.fountain_every(), 30);
        assert_eq!(Difficulty::Frenzy.fountain_every(), 6);
        assert_eq!(Difficulty::Normal.fountain_every(), 15);
    }

    /// Oracle: set_best updates the best label.
    #[test]
    fn best_label_reflects_score() {
        let mut menu = MenuScreen::new();
        menu.resize(360, 740);
        menu.set_best(7);
        assert_eq!(menu.best_label(), "best 7", "label must show best");
    }

    /// Oracle: keyboard a11y - Tab focuses the Play button and
    /// Enter activates it (no pointer involved).
    #[test]
    fn keyboard_activates_play() {
        let mut menu = MenuScreen::new();
        menu.resize(360, 740);
        use window_kit::event::NavKey;
        menu.key_nav(NavKey::Tab); // focus the difficulty select
        menu.key_nav(NavKey::Tab); // focus the Play button
        assert!(!menu.play_pressed(), "focus alone does not activate");
        menu.key_char('\n'); // Enter activates the focused button
        assert!(menu.play_pressed(), "Enter must activate Play");
    }

    /// Oracle: keyboard arrows step the focused difficulty select.
    #[test]
    fn keyboard_steps_difficulty() {
        use window_kit::event::NavKey;
        let mut menu = MenuScreen::new();
        menu.resize(360, 740);
        menu.key_nav(NavKey::Tab); // focus the select (first focusable)
        assert_eq!(menu.difficulty(), Difficulty::Normal);
        menu.key_nav(NavKey::Down);
        assert_eq!(
            menu.difficulty(),
            Difficulty::Frenzy,
            "Down steps to Frenzy"
        );
        menu.key_nav(NavKey::Up);
        assert_eq!(menu.difficulty(), Difficulty::Normal, "Up steps back");
    }

    /// Oracle: the menu paints deterministically.
    #[test]
    fn menu_paints_deterministically() {
        let paint = || {
            let mut menu = MenuScreen::new();
            menu.resize(360, 740);
            let mut surface = Surface::new(360, 740).unwrap_or_default();
            let mut painter = Painter::default();
            menu.paint(&mut surface, &mut painter);
            surface.data().to_vec()
        };
        assert_eq!(paint(), paint(), "menu bytes must be deterministic");
    }
}

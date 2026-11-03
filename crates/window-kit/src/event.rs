//! Events delivered to an [`crate::EventSink`].
//!
//! The set is deliberately minimal and non-exhaustive: platform matrices
//! (raw input 859, DirectManipulation 860, touch prediction 1029) extend
//! through new variants, never through backend-specific downcasts.

/// Stable per-session window identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WindowId(pub u64);

/// State of a character key press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyCharState {
    /// Key went down.
    Pressed,
    /// Key came up.
    Released,
}

/// A facade event.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Event {
    /// The window should repaint now.
    RedrawRequested,
    /// Physical size or scale changed; logical size is derived.
    Resized(PhysicalSize),
    /// The platform asks to close the window.
    CloseRequested,
    /// Keyboard focus changed.
    Focused(bool),
    /// Cursor moved to a logical point.
    CursorMoved(LogicalPoint),
    /// Mouse button at a logical point; button 0 is primary.
    MouseInput {
        /// Down or up.
        pressed: bool,
        /// Button index, 0 primary.
        button: u8,
        /// Logical position.
        at: LogicalPoint,
    },
    /// Wheel or trackpad scroll in logical units (positive y = up).
    Scrolled {
        /// Logical delta.
        delta: LogicalPoint,
    },
    /// Text input character with press state (IME-committed text arrives
    /// here as individual characters in v0; input-kit carries preedit
    /// semantics).
    KeyChar {
        /// The character.
        ch: char,
        /// Press state.
        state: KeyCharState,
    },
    /// Editing/navigation key that carries no text (Backspace, Delete,
    /// arrows, Home, End). Tab stays in [`Event::KeyChar`] as `\t`.
    KeyNav {
        /// Which navigation key.
        key: NavKey,
        /// Press state.
        state: KeyCharState,
    },
}

/// Text-free editing and navigation keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavKey {
    /// Delete the cluster before the caret.
    Backspace,
    /// Delete the cluster after the caret.
    Delete,
    /// Caret one cluster left.
    Left,
    /// Caret one cluster right.
    Right,
    /// Caret to the start.
    Home,
    /// Caret to the end.
    End,
    /// Focus the next focusable widget.
    Tab,
    /// Focus the previous focusable widget.
    BackTab,
}

/// Physical pixel size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysicalSize {
    /// Width in physical pixels.
    pub width: u32,
    /// Height in physical pixels.
    pub height: u32,
}

/// Logical (scene) size: physical divided by scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogicalSize {
    /// Logical width.
    pub width: f64,
    /// Logical height.
    pub height: f64,
}

/// Logical (scene) point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogicalPoint {
    /// Logical x.
    pub x: f64,
    /// Logical y.
    pub y: f64,
}

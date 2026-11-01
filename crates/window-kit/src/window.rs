//! Window trait, present trait, and attributes.

use crate::event::{Event, LogicalPoint, LogicalSize, PhysicalSize, WindowId};

/// Window creation attributes.
#[derive(Clone, Debug)]
pub struct WindowAttrs {
    /// Initial title.
    pub title: String,
    /// Initial logical size; platform default when `None`.
    pub logical_size: Option<LogicalSize>,
    /// Resizable by the user.
    pub resizable: bool,
}

impl Default for WindowAttrs {
    fn default() -> Self {
        WindowAttrs {
            title: String::new(),
            logical_size: None,
            resizable: true,
        }
    }
}

/// A platform window, viewed through the facade.
///
/// Object-safe by design: backends own the real window object; the
/// engine sees this trait plus [`Event`] delivery.
pub trait Window {
    /// Stable id for this session.
    fn id(&self) -> WindowId;

    /// Current logical (scene) size.
    fn logical_size(&self) -> LogicalSize;

    /// Current physical pixel size.
    fn physical_size(&self) -> PhysicalSize;

    /// Current DPI scale (physical / logical).
    fn scale(&self) -> f64;

    /// Convert a physical pixel position to logical coordinates.
    fn to_logical(&self, x: f64, y: f64) -> LogicalPoint {
        let s = self.scale().max(1e-9);
        LogicalPoint { x: x / s, y: y / s }
    }

    /// Ask the platform for a redraw event.
    fn request_redraw(&mut self);

    /// Update the title.
    fn set_title(&mut self, title: &str);

    /// True once the platform has confirmed closure.
    fn is_closed(&self) -> bool;
}

/// Software present: hand the backend a premultiplied RGBA8 frame.
///
/// Alpha handling is backend policy (opaque windows ignore it); the
/// flip-model waitable swapchain (item 855) and Wayland zero-copy (item
/// 952) are backend upgrades behind this same trait.
pub trait Present {
    /// Present `width * height * 4` premultiplied RGBA8 bytes for `id`.
    fn present(
        &mut self,
        id: WindowId,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> Result<(), PresentError>;
}

/// Present failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentError {
    /// No such window this session.
    UnknownWindow(WindowId),
    /// Pixel buffer length does not match `width * height * 4`.
    SizeMismatch {
        /// Expected byte length.
        expected: usize,
        /// Provided byte length.
        got: usize,
    },
    /// The backend rejected the frame.
    Backend,
}

/// Events the sink receives, with the affected window and present path
/// in scope.
pub trait EventSink {
    /// Handle one event. `present` is valid for the session.
    fn event(&mut self, window: &mut dyn Window, present: &mut dyn Present, event: &Event);
}

/// Convenience for sinks that react to the full event set: request a
/// redraw from a resize.
pub fn redraw_on_resize(window: &mut dyn Window, event: &Event) {
    if matches!(event, Event::Resized(_)) {
        window.request_redraw();
    }
}

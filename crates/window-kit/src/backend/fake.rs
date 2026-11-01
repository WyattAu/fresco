//! Headless fake backend: scripted events, captured presents.
//!
//! This is what makes the facade CI-testable: the entire engine runs
//! against `FakeRunner` and `FakePresent` with zero display dependency.

use crate::event::PhysicalSize;
use crate::event::{Event, LogicalSize, WindowId};
use crate::window::{Present, PresentError, Window, WindowAttrs};
use crate::{Runner, RunnerError};
use std::vec::Vec;

/// A fake window: everything the engine can observe, nothing it can't.
#[derive(Debug)]
pub struct FakeWindow {
    id: WindowId,
    logical: LogicalSize,
    scale: f64,
    closed: bool,
    redraws: u32,
    title: String,
}

impl FakeWindow {
    /// New fake window from attributes.
    #[must_use]
    pub fn new(id: WindowId, attrs: &WindowAttrs) -> Self {
        let logical = attrs.logical_size.unwrap_or(LogicalSize {
            width: 1024.0,
            height: 768.0,
        });
        FakeWindow {
            id,
            logical,
            scale: 1.0,
            closed: false,
            redraws: 0,
            title: attrs.title.clone(),
        }
    }

    /// Set the DPI scale (logical size is preserved, physical follows).
    pub fn set_scale(&mut self, scale: f64) {
        self.scale = scale.max(1e-9);
    }

    /// Set logical size directly (simulates a user resize).
    pub fn set_logical_size(&mut self, width: f64, height: f64) {
        self.logical = LogicalSize { width, height };
    }
}

impl Window for FakeWindow {
    fn id(&self) -> WindowId {
        self.id
    }

    fn logical_size(&self) -> LogicalSize {
        self.logical
    }

    fn physical_size(&self) -> PhysicalSize {
        PhysicalSize {
            width: (self.logical.width * self.scale).round() as u32,
            height: (self.logical.height * self.scale).round() as u32,
        }
    }

    fn scale(&self) -> f64 {
        self.scale
    }

    fn request_redraw(&mut self) {
        self.redraws = self.redraws.saturating_add(1);
    }

    fn set_title(&mut self, title: &str) {
        self.title = title.to_string();
    }

    fn is_closed(&self) -> bool {
        self.closed
    }
}

/// Captures presented frames for assertions.
#[derive(Debug, Default)]
pub struct FakePresent {
    frames: Vec<(WindowId, u32, u32, Vec<u8>)>,
}

impl FakePresent {
    /// Frames presented so far: `(id, w, h, pixels)`.
    #[must_use]
    pub fn frames(&self) -> &[(WindowId, u32, u32, Vec<u8>)] {
        &self.frames
    }

    /// Drop captured frames, retaining capacity.
    pub fn clear(&mut self) {
        self.frames.clear();
    }
}

impl Present for FakePresent {
    fn present(
        &mut self,
        id: WindowId,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> Result<(), PresentError> {
        let expected = usize::try_from(width)
            .ok()
            .and_then(|w| w.checked_mul(height as usize))
            .and_then(|px| px.checked_mul(4))
            .unwrap_or(usize::MAX);
        if pixels.len() != expected {
            return Err(PresentError::SizeMismatch {
                expected,
                got: pixels.len(),
            });
        }
        self.frames.push((id, width, height, pixels.to_vec()));
        Ok(())
    }
}

/// Scripted headless runner: replays `(window, event)` pairs through the
/// sink, then closes.
#[derive(Debug, Default)]
pub struct FakeRunner {
    windows: Vec<FakeWindow>,
    script: Vec<(usize, Event)>,
    next_id: u64,
}

impl FakeRunner {
    /// New runner with no windows.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a window; returns its script index.
    pub fn add_window(&mut self, attrs: &WindowAttrs) -> usize {
        let id = WindowId(self.next_id);
        self.next_id += 1;
        self.windows.push(FakeWindow::new(id, attrs));
        self.windows.len() - 1
    }

    /// Window by script index.
    #[must_use]
    pub fn window(&mut self, index: usize) -> Option<&mut FakeWindow> {
        self.windows.get_mut(index)
    }

    /// Queue an event for window `index`.
    pub fn push(&mut self, index: usize, event: Event) {
        self.script.push((index, event));
    }

    /// Windows created so far.
    #[must_use]
    pub fn window_count(&self) -> usize {
        self.windows.len()
    }
}

impl Runner for FakeRunner {
    fn run(&mut self, sink: &mut dyn crate::window::EventSink) -> Result<(), RunnerError> {
        let mut present = FakePresent::default();
        for (index, event) in self.script.drain(..) {
            let Some(window) = self.windows.get_mut(index) else {
                continue;
            };
            if matches!(event, Event::CloseRequested) {
                window.closed = true;
            }
            sink.event(window, &mut present, &event);
        }
        Ok(())
    }
}

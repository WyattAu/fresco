//! Windowing facade for the fresco UI engine (ADR-0003).
//!
//! Estate-owned traits (`Window`, `Present`, `EventSink`, `FrameClock`)
//! with the first backend being winit plus a softbuffer software present
//! path. The traits are shaped by the platform matrices in
//! `docs/paradigms.md` so native backends can replace individual concerns
//! without facade changes.
//!
//! Core rules:
//!
//! - The engine never links a display server: this crate compiles and its
//!   logic is fully tested headless via `backend::fake`. The `winit`
//!   feature (default) adds the real backend.
//! - DPI is a continuous scale factor; logical coordinates are scene
//!   coordinates (858, 956, 990).
//! - Present receives a premultiplied RGBA8 buffer; backends own the
//!   platform swap semantics (flip-model waitable, item 855, is a
//!   backend detail, never a facade property).
#![forbid(unsafe_code)]

pub mod backend;
pub mod clock;
pub mod event;
pub mod window;

#[cfg(feature = "winit")]
pub mod winit_backend;

pub use clock::{FrameClock, Ticker};
pub use event::{Event, KeyCharState, LogicalPoint, LogicalSize, PhysicalSize, WindowId};
pub use window::{EventSink, Present, PresentError, Window, WindowAttrs};

/// Connection to the platform: creates windows and drives `EventSink`
/// callbacks until exit. Backends: `backend::fake::FakeRunner` (headless,
/// scripted, test-observable) and `winit_backend::WinitRunner` (default
/// feature).
pub trait Runner {
    /// Run until the sink (or the platform) ends the session.
    fn run(&mut self, sink: &mut dyn window::EventSink) -> Result<(), RunnerError>;
}

/// Runner construction and runtime failures.
#[derive(Clone, Debug, PartialEq)]
pub enum RunnerError {
    /// The backend could not initialize (no display, no event loop).
    Init,
    /// The platform loop returned an error condition.
    Loop,
}

impl core::fmt::Display for RunnerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RunnerError::Init => write!(f, "backend initialization failed"),
            RunnerError::Loop => write!(f, "event loop failed"),
        }
    }
}

impl std::error::Error for RunnerError {}

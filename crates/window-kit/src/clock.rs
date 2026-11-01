//! Frame pacing measurement.
//!
//! The real pacing sources are platform frame clocks (Wayland frame
//! callbacks 959, CADisplayLink 1032, Choreographer 1017) and arrive as
//! backend upgrades. This facade type measures what the engine controls
//! so the partial-redraw claim stays measured (item 947).

use std::time::{Duration, Instant};

/// Frame pacing trait.
pub trait Ticker {
    /// Mark the start of a frame.
    fn begin_frame(&mut self);

    /// Mark the end of a frame; returns the frame duration.
    fn end_frame(&mut self) -> Duration;

    /// Number of completed frames.
    fn frames(&self) -> u64;

    /// Mean frame duration over completed frames.
    fn mean(&self) -> Duration;
}

/// Moving-window frame clock: durations of the last N frames.
#[derive(Debug)]
pub struct FrameClock {
    last_begin: Option<Instant>,
    window: [Duration; 64],
    cursor: usize,
    filled: usize,
    frames: u64,
    last: Duration,
}

impl Default for FrameClock {
    fn default() -> Self {
        FrameClock {
            last_begin: None,
            window: [Duration::ZERO; 64],
            cursor: 0,
            filled: 0,
            frames: 0,
            last: Duration::ZERO,
        }
    }
}

impl Ticker for FrameClock {
    fn begin_frame(&mut self) {
        self.last_begin = Some(Instant::now());
    }

    fn end_frame(&mut self) -> Duration {
        let dur = match self.last_begin.take() {
            Some(t0) => t0.elapsed(),
            None => Duration::ZERO,
        };
        if let Some(slot) = self.window.get_mut(self.cursor) {
            *slot = dur;
        }
        self.cursor = (self.cursor + 1) % self.window.len();
        self.filled = (self.filled + 1).min(self.window.len());
        self.frames = self.frames.saturating_add(1);
        self.last = dur;
        dur
    }

    fn frames(&self) -> u64 {
        self.frames
    }

    fn mean(&self) -> Duration {
        if self.filled == 0 {
            return Duration::ZERO;
        }
        let total: Duration = self.window.iter().take(self.filled).sum();
        total / self.filled as u32
    }

    // The remaining trait methods have default-free bodies above; clippy
    // treats the trait as fully implemented.
}

impl FrameClock {
    /// Duration of the most recent frame.
    #[must_use]
    pub fn last(&self) -> Duration {
        self.last
    }
}

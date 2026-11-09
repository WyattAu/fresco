//! Fixed-timestep simulation clock for game loops.
//!
//! The classic accumulator pattern: wall-clock time feeds in,
//! fixed-size simulation steps come out, and rendering interpolation
//! alpha tells you how far between the last two steps the frame is.
//! Keeps physics deterministic regardless of frame rate.

/// Fixed-timestep accumulator.
#[derive(Clone, Copy, Debug)]
pub struct Clock {
    /// Fixed simulation step in seconds.
    pub step: f64,
    /// Seconds carried over between frames.
    acc: f64,
    /// Total simulated time.
    simulated: f64,
    /// Guard against spiral-of-death: max steps per frame.
    max_steps_per_frame: u32,
}

impl Clock {
    /// New clock with a 1/60 s step and a 5-step per-frame cap.
    #[must_use]
    pub fn new() -> Self {
        Clock {
            step: 1.0 / 60.0,
            acc: 0.0,
            simulated: 0.0,
            max_steps_per_frame: 5,
        }
    }

    /// New clock with an explicit step size.
    #[must_use]
    pub fn with_step(step: f64) -> Self {
        Clock {
            step: if step > 0.0 { step } else { 1.0 / 60.0 },
            acc: 0.0,
            simulated: 0.0,
            max_steps_per_frame: 5,
        }
    }

    /// Feed a frame's wall-clock duration; returns how many fixed
    /// steps to run this frame (capped to avoid the spiral of death).
    pub fn tick(&mut self, dt: f64) -> u32 {
        let dt = if dt > 0.0 { dt } else { 0.0 };
        self.acc += dt;
        let mut steps = 0_u32;
        while self.acc >= self.step && steps < self.max_steps_per_frame {
            self.acc -= self.step;
            self.simulated += self.step;
            steps += 1;
        }
        if steps == self.max_steps_per_frame {
            // Dropped time: discard the backlog rather than burst.
            self.acc = 0.0;
        }
        steps
    }

    /// Interpolation alpha in `[0, 1)`: how far between the last two
    /// simulation steps the current frame sits. Feed to renderers
    /// for smooth motion at fixed simulation rates.
    #[must_use]
    pub fn alpha(&self) -> f64 {
        (self.acc / self.step).clamp(0.0, 1.0)
    }

    /// Total simulated time in seconds (excludes carried remainder).
    #[must_use]
    pub fn simulated_time(&self) -> f64 {
        self.simulated
    }

    /// Remaining backlog in seconds (should stay below one step).
    #[must_use]
    pub fn backlog(&self) -> f64 {
        self.acc
    }

    /// Reset the accumulator (e.g. after a pause) without losing
    /// the simulated-time total.
    pub fn reset_accumulator(&mut self) {
        self.acc = 0.0;
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulates_into_fixed_steps() {
        let mut c = Clock::new();
        // 16.6 ms frames at a 16.67 ms step: one step roughly every frame.
        let mut total = 0;
        for _ in 0..60 {
            total += c.tick(1.0 / 55.0); // slightly slow frames
        }
        // 60 frames at 1/55 s = 1.0909 s -> 65 steps at 1/60 s.
        assert!(
            (60..=66).contains(&total),
            "steps should track wall time: {total}"
        );
    }

    #[test]
    fn zero_and_negative_ticks_are_ignored() {
        let mut c = Clock::new();
        assert_eq!(c.tick(0.0), 0);
        assert_eq!(c.tick(-1.0), 0);
        assert!((c.backlog() - 0.0).abs() < 1e-12);
    }

    #[test]
    fn alpha_sits_between_zero_and_one() {
        let mut c = Clock::new();
        c.tick(1.0 / 120.0); // half a step
        let a = c.alpha();
        assert!(a > 0.45 && a < 0.55, "half-step alpha ~0.5: {a}");
    }

    #[test]
    fn spiral_of_death_is_capped() {
        let mut c = Clock::new();
        // One massive frame (1 s) must not burst 60 steps.
        let steps = c.tick(1.0);
        assert_eq!(steps, c.max_steps_per_frame, "capped");
        assert!((c.backlog() - 0.0).abs() < 1e-12, "backlog discarded");
    }

    #[test]
    fn simulated_time_counts_steps_only() {
        let mut c = Clock::with_step(0.1);
        c.tick(0.35); // 3 full steps + 0.05 remainder
        assert!((c.simulated_time() - 0.3).abs() < 1e-9);
        assert!((c.backlog() - 0.05).abs() < 1e-9);
        c.reset_accumulator();
        assert!((c.backlog() - 0.0).abs() < 1e-12);
        assert!((c.simulated_time() - 0.3).abs() < 1e-9, "total preserved");
    }

    #[test]
    fn deterministic_step_counting() {
        let frames = [0.016, 0.02, 0.011, 0.017, 0.03, 0.008];
        let run = || {
            let mut c = Clock::new();
            let counts: Vec<u32> = frames.iter().map(|&f| c.tick(f)).collect();
            (counts, c.simulated_time())
        };
        let (a, ta) = run();
        let (b, tb) = run();
        assert_eq!(a, b);
        assert!((ta - tb).abs() < 1e-12);
    }
}

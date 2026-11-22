//! Game sound effects: clips generated deterministically at startup
//! through audio-kit's synth core (oscillator + envelope), played
//! into the SilentSink backend (M9.1). A real output stream swaps in
//! behind the same play() contract later.

use audio_kit::{Envelope, Oscillator, SilentSink, WaveKind, mix, render_clip};

/// Sound effect kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SfxKind {
    /// A target popped (bright short blip).
    Pop,
    /// A ball was grabbed (low tick).
    Grab,
    /// A ball hit the water pool (soft low thud).
    Splash,
}

/// The game's sound effects: clips generated once at construction,
/// played on demand into the sink.
pub struct Sfx {
    sink: SilentSink,
    clips: Vec<Vec<f32>>,
}

impl Default for Sfx {
    fn default() -> Self {
        Self::new()
    }
}

impl Sfx {
    /// Generate the effect set at 8000 Hz (mono). Deterministic:
    /// byte-identical across constructions.
    #[must_use]
    pub fn new() -> Self {
        const RATE: u32 = 8000;
        let pop = {
            let mut osc = Oscillator::new(WaveKind::Sine, 880.0);
            let mut env = Envelope::adsr(0.005, 0.1, 0.0, 0.05);
            render_clip(&mut osc, &mut env, 0.12, RATE)
        };
        let grab = {
            let mut osc = Oscillator::new(WaveKind::Triangle, 220.0);
            let mut env = Envelope::adsr(0.002, 0.03, 0.0, 0.02);
            render_clip(&mut osc, &mut env, 0.05, RATE)
        };
        let splash = {
            let mut osc = Oscillator::new(WaveKind::Sine, 180.0);
            let mut env = Envelope::adsr(0.004, 0.18, 0.0, 0.1);
            render_clip(&mut osc, &mut env, 0.25, RATE)
        };
        Sfx {
            sink: SilentSink::new(),
            clips: vec![pop, grab, splash],
        }
    }

    /// Play one effect into the sink.
    pub fn play(&mut self, kind: SfxKind) {
        let idx = match kind {
            SfxKind::Pop => 0,
            SfxKind::Grab => 1,
            SfxKind::Splash => 2,
        };
        if let Some(clip) = self.clips.get(idx) {
            // Mono: one sample per frame.
            for frame in clip.chunks(1) {
                self.sink.write(frame);
            }
        }
    }

    /// Frames written to the sink (oracle + stats use).
    #[must_use]
    pub const fn frames_written(&self) -> u64 {
        self.sink.frames_written()
    }

    /// A clip's length in samples (oracle use).
    #[must_use]
    pub fn clip_len(&self, kind: SfxKind) -> usize {
        match kind {
            SfxKind::Pop => self.clips.first().map_or(0, Vec::len),
            SfxKind::Grab => self.clips.get(1).map_or(0, Vec::len),
            SfxKind::Splash => self.clips.get(2).map_or(0, Vec::len),
        }
    }

    /// Mix down the whole effect set (evidence use).
    #[must_use]
    pub fn mixdown(&self) -> Vec<f32> {
        let mut acc: Option<Vec<f32>> = None;
        for clip in &self.clips {
            acc = Some(match acc {
                Some(acc) => mix(&acc, clip, 1.0, 1.0),
                None => clip.clone(),
            });
        }
        acc.unwrap_or_default()
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    /// Oracle: the effect set is deterministic - two constructions
    /// are byte-identical.
    #[test]
    fn clips_are_deterministic() {
        let a = Sfx::new();
        let b = Sfx::new();
        assert_eq!(a.mixdown(), b.mixdown(), "mixdown byte-identical");
    }

    /// Oracle: the pop clip is a short bright burst (sub-200ms).
    #[test]
    fn pop_clip_is_short() {
        let sfx = Sfx::new();
        assert!(
            sfx.clip_len(SfxKind::Pop) < 1600,
            "pop at 8kHz must be < 0.2s: {}",
            sfx.clip_len(SfxKind::Pop)
        );
    }
}

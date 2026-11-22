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
    /// Last accepted play tick per kind (the rate-limit clock).
    last_played: [Option<u64>; 3],
    /// Accepted play count per kind (oracle + HUD evidence).
    plays: [u64; 3],
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
            last_played: [None; 3],
            plays: [0; 3],
        }
    }

    /// Play one effect into the sink. `now` is the caller's tick
    /// (game sim frames): a kind replayed inside its rate-limit
    /// window is suppressed, so combo bursts and pool churn cannot
    /// machine-gun the backend.
    pub fn play(&mut self, kind: SfxKind, now: u64) {
        let idx = match kind {
            SfxKind::Pop => 0,
            SfxKind::Grab => 1,
            SfxKind::Splash => 2,
        };
        let Some(slot) = self.last_played.get_mut(idx) else {
            return;
        };
        if let Some(last) = *slot
            && now.saturating_sub(last) < Self::min_interval_frames(kind)
        {
            return; // inside the rate-limit window
        }
        *slot = Some(now);
        if let Some(count) = self.plays.get_mut(idx) {
            *count = count.saturating_add(1);
        }
        if let Some(clip) = self.clips.get(idx) {
            // Mono: one sample per frame.
            for frame in clip.chunks(1) {
                self.sink.write(frame);
            }
        }
    }

    /// Minimum sim-frame gap between accepted plays of a kind (the
    /// rate limit; sim ticks at ~60 Hz, so 3 frames is ~50 ms and
    /// splash churn is throttled to half a second).
    #[must_use]
    pub const fn min_interval_frames(kind: SfxKind) -> u64 {
        match kind {
            SfxKind::Pop => 3,
            SfxKind::Grab => 2,
            SfxKind::Splash => 30,
        }
    }

    /// Accepted plays of a kind so far (oracle use).
    #[must_use]
    pub const fn plays(&self, kind: SfxKind) -> u64 {
        let [pop, grab, splash] = self.plays;
        match kind {
            SfxKind::Pop => pop,
            SfxKind::Grab => grab,
            SfxKind::Splash => splash,
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

    /// Oracle: a kind replayed inside its rate-limit window is
    /// suppressed - one clip's worth of frames, one accepted play;
    /// after the window a replay is accepted.
    #[test]
    fn rate_limit_suppresses_rapid_same_kind() {
        let mut sfx = Sfx::new();
        sfx.play(SfxKind::Pop, 0);
        let one_clip = sfx.frames_written();
        sfx.play(SfxKind::Pop, 1);
        sfx.play(SfxKind::Pop, 2);
        assert_eq!(sfx.frames_written(), one_clip, "burst collapses");
        assert_eq!(sfx.plays(SfxKind::Pop), 1, "one accepted play");
        let window = Sfx::min_interval_frames(SfxKind::Pop);
        sfx.play(SfxKind::Pop, window);
        let expected = one_clip + u64::try_from(sfx.clip_len(SfxKind::Pop)).unwrap_or(0);
        assert_eq!(
            sfx.frames_written(),
            expected,
            "replay after the window is accepted"
        );
        assert_eq!(sfx.plays(SfxKind::Pop), 2);
    }

    /// Oracle: the rate limit is per kind - a pop does not silence
    /// a grab in the same frame.
    #[test]
    fn rate_limit_is_per_kind() {
        let mut sfx = Sfx::new();
        sfx.play(SfxKind::Pop, 0);
        sfx.play(SfxKind::Grab, 0);
        let total = u64::try_from(sfx.clip_len(SfxKind::Pop)).unwrap_or(0)
            + u64::try_from(sfx.clip_len(SfxKind::Grab)).unwrap_or(0);
        assert_eq!(sfx.frames_written(), total, "both kinds played");
        assert_eq!(sfx.plays(SfxKind::Pop), 1);
        assert_eq!(sfx.plays(SfxKind::Grab), 1);
    }

    /// Oracle: splash is throttled hard - pool churn inside the
    /// half-second window collapses to one clip.
    #[test]
    fn splash_throttled_hard() {
        let mut sfx = Sfx::new();
        sfx.play(SfxKind::Splash, 0);
        let one_clip = sfx.frames_written();
        sfx.play(SfxKind::Splash, 10);
        sfx.play(SfxKind::Splash, 29);
        assert_eq!(sfx.frames_written(), one_clip, "churn suppressed");
        assert_eq!(sfx.plays(SfxKind::Splash), 1);
    }
}

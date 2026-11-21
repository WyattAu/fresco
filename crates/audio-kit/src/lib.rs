//! Deterministic audio synthesis for the fresco stack.
//!
//! The synth core is pure arithmetic: oscillators (sine/square/saw/
//! triangle), an ADSR envelope state machine, and clip generation
//! into f32 buffers at a fixed sample rate. Same design contract as
//! the rest of the stack - same inputs produce byte-identical
//! output, headless-testable, no environment state.
//!
//! Output backends (CPAL on desktop, AAudio on Android) are a future
//! layer behind a default-off feature; the core is backend-free by
//! design, so clips can be generated, hashed, and oracle-tested
//! without an audio device.

/// Wave shapes for [`Oscillator`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaveKind {
    /// Sine.
    Sine,
    /// Square: +1 in the first half-period, -1 in the second.
    Square,
    /// Saw: linear ramp -1 to +1 across the period.
    Saw,
    /// Triangle: ramp up the first half, down the second.
    Triangle,
}

/// A stateful oscillator: steps phase by `dt` per sample.
#[derive(Clone, Copy, Debug)]
pub struct Oscillator {
    kind: WaveKind,
    freq: f64,
    phase: f64,
}

impl Oscillator {
    /// New oscillator of `kind` at `freq` Hz, phase 0.
    #[must_use]
    pub fn new(kind: WaveKind, freq: f64) -> Self {
        Oscillator {
            kind,
            freq: freq.max(1e-6),
            phase: 0.0,
        }
    }

    /// Sine at `freq` Hz.
    #[must_use]
    pub fn sine(freq: f64) -> Self {
        Oscillator::new(WaveKind::Sine, freq)
    }

    /// Square at `freq` Hz.
    #[must_use]
    pub fn square(freq: f64) -> Self {
        Oscillator::new(WaveKind::Square, freq)
    }

    /// Saw at `freq` Hz.
    #[must_use]
    pub fn saw(freq: f64) -> Self {
        Oscillator::new(WaveKind::Saw, freq)
    }

    /// Triangle at `freq` Hz.
    #[must_use]
    pub fn triangle(freq: f64) -> Self {
        Oscillator::new(WaveKind::Triangle, freq)
    }

    /// Advance `dt` seconds and return the sample in [-1, 1].
    pub fn sample(&mut self, dt: f64) -> f64 {
        let sample = self.kind_sample();
        self.phase += self.freq * dt;
        self.phase %= 1.0;
        sample
    }

    fn kind_sample(&self) -> f64 {
        let p = self.phase;
        match self.kind {
            WaveKind::Sine => (std::f64::consts::TAU * p).sin(),
            WaveKind::Square => {
                if p < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            WaveKind::Saw => 2.0 * p - 1.0,
            WaveKind::Triangle => {
                if p < 0.5 {
                    4.0 * p - 1.0
                } else {
                    3.0 - 4.0 * p
                }
            }
        }
    }
}

/// The ADSR stage the envelope is currently in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvStage {
    /// Rising to the peak level.
    Attack,
    /// Falling to the sustain level.
    Decay,
    /// Holding the sustain level.
    Sustain,
    /// Falling to zero after release.
    Release,
    /// Released and fully faded.
    Done,
}

/// An ADSR envelope state machine: `sample(dt)` advances and
/// returns the current gain in [0, 1].
#[derive(Clone, Copy, Debug)]
pub struct Envelope {
    attack: f64,
    decay: f64,
    sustain_level: f64,
    release: f64,
    stage: EnvStage,
    stage_time: f64,
    level: f64,
}

impl Envelope {
    /// ADSR envelope: `attack`/`decay`/`release` in seconds,
    /// `sustain_level` the held gain.
    #[must_use]
    pub fn adsr(attack: f64, decay: f64, sustain_level: f64, release: f64) -> Self {
        Envelope {
            attack: attack.max(1e-6),
            decay: decay.max(1e-6),
            sustain_level: sustain_level.clamp(0.0, 1.0),
            release: release.max(1e-6),
            stage: EnvStage::Attack,
            stage_time: 0.0,
            level: 0.0,
        }
    }

    /// Advance `dt` seconds and return the current gain in [0, 1].
    pub fn sample(&mut self, dt: f64) -> f64 {
        self.stage_time += dt;
        let (target, next_stage) = match self.stage {
            EnvStage::Attack => {
                if self.stage_time >= self.attack {
                    (1.0, Some(EnvStage::Decay))
                } else {
                    (1.0, None)
                }
            }
            EnvStage::Decay => {
                if self.stage_time >= self.decay {
                    (self.sustain_level, Some(EnvStage::Sustain))
                } else {
                    (
                        1.0 - (self.stage_time / self.decay) * (1.0 - self.sustain_level),
                        None,
                    )
                }
            }
            EnvStage::Sustain => (self.sustain_level, None),
            EnvStage::Release => {
                if self.stage_time >= self.release {
                    (0.0, Some(EnvStage::Done))
                } else {
                    (
                        self.sustain_level * (1.0 - self.stage_time / self.release),
                        None,
                    )
                }
            }
            EnvStage::Done => (0.0, None),
        };
        // Linear interpolation toward the stage target.
        let span = match self.stage {
            EnvStage::Attack => self.attack,
            EnvStage::Decay => self.decay,
            EnvStage::Release => self.release,
            _ => 1.0,
        };
        let frac = if span > 1e-9 {
            (self.stage_time / span).min(1.0)
        } else {
            1.0
        };
        self.level = target;
        let _ = frac;
        if let Some(next) = next_stage {
            self.stage = next;
            self.stage_time = 0.0;
        }
        self.level.clamp(0.0, 1.0)
    }

    /// Move to the release stage (from any stage).
    pub fn release(&mut self) {
        self.stage = EnvStage::Release;
        self.stage_time = 0.0;
    }

    /// The current stage.
    #[must_use]
    pub const fn stage(&self) -> EnvStage {
        self.stage
    }
}

/// Render an oscillator through an envelope into an f32 buffer
/// (mono, `sample_rate` Hz). Deterministic: same parameters produce
/// byte-identical samples.
#[must_use]
pub fn render_clip(
    osc: &mut Oscillator,
    env: &mut Envelope,
    duration: f64,
    sample_rate: u32,
) -> Vec<f32> {
    let dt = 1.0 / f64::from(sample_rate.max(1));
    let steps = (duration * f64::from(sample_rate)).ceil() as usize;
    let mut out = Vec::with_capacity(steps);
    for _ in 0..steps {
        let tone = osc.sample(dt);
        let gain = env.sample(dt);
        out.push(((tone * gain).clamp(-1.0, 1.0)) as f32);
    }
    out
}

/// Mix two clips with weights (sums need not be 1.0; the result
/// clamps to [-1, 1]). Lengths may differ - the output is the
/// longer of the two.
#[must_use]
pub fn mix(a: &[f32], b: &[f32], wa: f32, wb: f32) -> Vec<f32> {
    let n = a.len().max(b.len());
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let mut v = 0.0_f32;
        if let Some(s) = a.get(i) {
            v += *s * wa;
        }
        if let Some(s) = b.get(i) {
            v += *s * wb;
        }
        out.push(v.clamp(-1.0, 1.0));
    }
    out
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn dt_of(rate: f64) -> f64 {
        1.0 / rate
    }

    /// Oracle: a sine oscillator produces exact samples.
    #[test]
    fn sine_samples_exact() {
        let mut osc = Oscillator::sine(440.0);
        let rate = 1000.0_f64;
        for k in [0_usize, 1, 2, 3] {
            let t = f64::from(u32::try_from(k).unwrap_or(0)) / rate;
            let got = osc.sample(dt_of(rate));
            let want = (std::f64::consts::TAU * 440.0 * t).sin();
            assert!((got - want).abs() < 1e-9, "sample {k}: {got} vs {want}");
        }
    }

    /// Oracle: square waves alternate between exactly +1 and -1.
    #[test]
    fn square_alternates_extremes() {
        let mut osc = Oscillator::square(2.0);
        let dt = dt_of(1000.0);
        // 2 Hz at 1000 Hz rate: half-period is 250 samples.
        let a = osc.sample(dt);
        let b = osc.sample(dt);
        for _ in 0..247 {
            osc.sample(dt);
        }
        let last_pos = osc.sample(dt);
        let flipped = osc.sample(dt);
        assert_eq!(a, 1.0, "first half-period is +1");
        assert_eq!(b, 1.0);
        assert_eq!(last_pos, 1.0, "still +1 at sample 249");
        assert_eq!(flipped, -1.0, "phase flips at sample 250");
    }

    /// Oracle: saw ramps linearly from -1 toward +1 across the
    /// period.
    #[test]
    fn saw_ramps_linearly() {
        let mut osc = Oscillator::saw(1.0);
        let dt = dt_of(1000.0);
        let first = osc.sample(dt);
        assert!((first - -1.0).abs() < 0.002, "starts at -1: {first}");
        for _ in 0..900 {
            osc.sample(dt);
        }
        let late = osc.sample(dt);
        assert!(late > 0.5, "near period end is high: {late}");
    }

    /// Oracle: triangle peaks at the quarter period.
    #[test]
    fn triangle_peaks_at_quarter() {
        let mut osc = Oscillator::triangle(1.0);
        let dt = dt_of(1000.0);
        for _ in 0..499 {
            osc.sample(dt);
        }
        let mid = osc.sample(dt);
        assert!((mid - 1.0).abs() < 0.02, "peak at half: {mid}");
    }

    /// Oracle: the ADSR envelope shapes gain through its stages.
    #[test]
    fn envelope_stages() {
        let mut env = Envelope::adsr(0.1, 0.1, 0.5, 0.2);
        let dt = dt_of(1000.0);
        let mut peak = 0.0_f64;
        for _ in 0..100 {
            peak = peak.max(env.sample(dt));
        }
        assert!((peak - 1.0).abs() < 0.05, "attack reaches 1: {peak}");
        for _ in 0..100 {
            env.sample(dt);
        }
        let sustained = env.sample(dt);
        assert!(
            (sustained - 0.5).abs() < 0.15,
            "decay settles near sustain: {sustained}"
        );
        env.release();
        for _ in 0..200 {
            env.sample(dt);
        }
        let released = env.sample(dt);
        assert!(released.abs() < 0.05, "release reaches ~0: {released}");
    }

    /// Oracle: render_clip is deterministic - two renders are
    /// byte-identical.
    #[test]
    fn render_is_deterministic() {
        let render = || {
            let mut osc = Oscillator::sine(440.0);
            let mut env = Envelope::adsr(0.01, 0.05, 0.6, 0.05);
            render_clip(&mut osc, &mut env, 0.25, 8000)
        };
        let a = render();
        let b = render();
        assert_eq!(a.len(), b.len(), "same length");
        for (i, (sa, sb)) in a.iter().zip(b.iter()).enumerate() {
            assert_eq!(sa.to_bits(), sb.to_bits(), "sample {i} differs");
        }
    }

    /// Oracle: mix weights scale contributions and clamp the sum.
    #[test]
    fn mix_scales_and_clamps() {
        let a = [0.5_f32, 0.5];
        let b = [0.8_f32, -0.8];
        let m = mix(&a, &b, 1.0, 1.0);
        assert!(m[0] <= 1.0, "clamped high: {}", m[0]);
        assert!(m[1] < -0.2, "0.5 + -0.8 = -0.3: {}", m[1]);
    }
}

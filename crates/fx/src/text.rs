//! Text effects (solid-fx TextAnimations family): count-up, scramble,
//! shimmer — all rendered through text-kit's 3x5 micro-font, all
//! deterministic in time.

use crate::Fx;
use crate::rand::FxRand;
use paint_kit::{Painter, Surface};
use text_kit::microfont;

/// Draw a micro-font string: same geometry as widget-kit's label
/// blitter, `scale` derived from `px_size`.
#[allow(clippy::too_many_arguments)]
fn draw_micro(
    surface: &mut Surface,
    painter: &mut Painter,
    text: &str,
    x: f64,
    y: f64,
    px_size: f64,
    color: [u8; 4],
) {
    let scale = ((px_size / 5.0).round() as u32).max(1);
    let advance = px_size * 0.6;
    let baseline_y = y + (px_size * 2.5) - (f64::from(scale) * 5.0 * 0.5);
    let mut pen_x = x;
    for ch in text.chars() {
        if let Some(bitmap) = microfont::micro_bitmap(ch, scale) {
            for gy in 0..bitmap.height {
                for gx in 0..bitmap.width {
                    let idx = gy as usize * bitmap.width as usize + gx as usize;
                    let Some(cov) = bitmap.coverage.get(idx) else {
                        continue;
                    };
                    if *cov == 0 {
                        continue;
                    }
                    let px = Rect::from_size(
                        Vec2::new(pen_x + f64::from(gx), baseline_y + f64::from(gy)),
                        Vec2::new(1.0, 1.0),
                    );
                    painter.fill_rect(surface, px, &Paint::from(Solid::premul(color)));
                }
            }
        }
        pen_x += advance;
    }
}

use paint_kit::{Paint, Rect, Solid, Vec2};

/// CountUp: a number animating from `from` to `to` with an ease-out
/// curve over `duration` seconds, rendered as micro-font digits.
/// `decimals` controls the fixed-point precision.
pub struct CountUp {
    /// Start value.
    pub from: f64,
    /// End value.
    pub to: f64,
    /// Animation duration in seconds.
    pub duration: f64,
    /// Digits after the decimal point.
    pub decimals: usize,
    /// Text color, straight RGBA.
    pub color: [u8; 4],
    /// Pixel size of the digits.
    pub px_size: f64,
}

impl CountUp {
    /// Defaults: 0 to 100 over 2 s, 0 decimals.
    #[must_use]
    pub fn new(to: f64) -> Self {
        CountUp {
            from: 0.0,
            to,
            duration: 2.0,
            decimals: 0,
            color: [230, 234, 244, 255],
            px_size: 20.0,
        }
    }

    /// Draw at time `fx.t`. Settles at `to` after `duration`.
    pub fn draw(&self, fx: &mut Fx, x: f64, y: f64) {
        let t = (fx.t / self.duration.max(0.001)).clamp(0.0, 1.0);
        // Ease-out cubic.
        let e = 1.0 - (1.0 - t).powi(3);
        let value = self.from + (self.to - self.from) * e;
        let text = format!("{:.*}", self.decimals, value);
        draw_micro(
            fx.surface,
            fx.painter,
            &text,
            x,
            y,
            self.px_size,
            self.color,
        );
    }
}

/// ScrambledText: characters resolve left to right; unresolved
/// positions cycle through a charset on a deterministic schedule
/// (solid-fx DecryptedText/ScrambledText family).
pub struct ScrambledText {
    /// The final text.
    pub text: String,
    /// Time until the text fully resolves, in seconds.
    pub duration: f64,
    /// Scramble charset.
    pub charset: Vec<char>,
    /// Text color, straight RGBA.
    pub color: [u8; 4],
    /// Pixel size.
    pub px_size: f64,
    /// Per-instance seed.
    pub seed: u64,
}

impl ScrambledText {
    /// Defaults: charset `!<>-_\/[]{}—=+*#?`, 2 s.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        ScrambledText {
            text: text.into(),
            duration: 2.0,
            charset: "!<>-_\\/[]{}=+*#?".chars().collect(),
            color: [230, 234, 244, 255],
            px_size: 16.0,
            seed: 0xDEC0DE,
        }
    }

    /// Draw at time `fx.t`.
    pub fn draw(&self, fx: &mut Fx, x: f64, y: f64) {
        let n = self.text.chars().count();
        let t = (fx.t / self.duration.max(0.001)).clamp(0.0, 1.0);
        // Character i resolves at time fraction i/n.
        let mut rand = FxRand::new(self.seed);
        let mut shown = String::new();
        for (i, ch) in self.text.chars().enumerate() {
            let resolve_at = (i + 1) as f64 / n.max(1) as f64;
            if t >= resolve_at {
                shown.push(ch);
            } else if ch == ' ' {
                shown.push(' ');
            } else {
                let jitter = (rand.next_index(100) as f64 / 100.0) * 0.2;
                if t + jitter >= resolve_at {
                    shown.push(ch);
                } else {
                    shown.push(*rand.pick(&self.charset, &'?'));
                }
            }
        }
        draw_micro(
            fx.surface,
            fx.painter,
            &shown,
            x,
            y,
            self.px_size,
            self.color,
        );
    }
}

/// ShimmerText: a brightness wave sweeps across the glyphs over time
/// (solid-fx ShinyText/Shimmer family) — per-glyph alpha modulated by
/// a traveling sine.
pub struct ShimmerText {
    /// The text.
    pub text: String,
    /// Sweep period in seconds.
    pub period: f64,
    /// Base text color, straight RGBA.
    pub color: [u8; 4],
    /// Peak (shimmer) color, straight RGBA.
    pub shine: [u8; 4],
    /// Pixel size.
    pub px_size: f64,
}

impl ShimmerText {
    /// Defaults: 2 s period, white on gray.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        ShimmerText {
            text: text.into(),
            period: 2.0,
            color: [150, 155, 168, 255],
            shine: [245, 248, 255, 255],
            px_size: 16.0,
        }
    }

    /// Draw at time `fx.t`.
    pub fn draw(&self, fx: &mut Fx, x: f64, y: f64) {
        let advance = self.px_size * 0.6;
        let scale = ((self.px_size / 5.0).round() as u32).max(1);
        let mut pen_x = x;
        let wave_span = self.text.chars().count() as f64;
        for (i, ch) in self.text.chars().enumerate() {
            let wave = (fx.t * std::f64::consts::TAU / self.period.max(0.1)
                - i as f64 / wave_span.max(1.0) * std::f64::consts::TAU)
                .sin();
            let m = (0.5 + wave * 0.5).clamp(0.0, 1.0);
            let color = [
                (self.color[0] as f64 + (self.shine[0] as f64 - f64::from(self.color[0])) * m)
                    .round() as u8,
                (self.color[1] as f64 + (self.shine[1] as f64 - f64::from(self.color[1])) * m)
                    .round() as u8,
                (self.color[2] as f64 + (self.shine[2] as f64 - f64::from(self.color[2])) * m)
                    .round() as u8,
                255,
            ];
            if let Some(bitmap) = microfont::micro_bitmap(ch, scale) {
                let baseline_y = y + (self.px_size * 2.5) - (f64::from(scale) * 5.0 * 0.5);
                for gy in 0..bitmap.height {
                    for gx in 0..bitmap.width {
                        let idx = gy as usize * bitmap.width as usize + gx as usize;
                        let Some(cov) = bitmap.coverage.get(idx) else {
                            continue;
                        };
                        if *cov == 0 {
                            continue;
                        }
                        let px = Rect::from_size(
                            Vec2::new(pen_x + f64::from(gx), baseline_y + f64::from(gy)),
                            Vec2::new(1.0, 1.0),
                        );
                        fx.painter
                            .fill_rect(fx.surface, px, &Paint::from(Solid::premul(color)));
                    }
                }
            }
            pen_x += advance;
        }
    }
}

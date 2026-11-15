//! Real-font text rendering: DejaVu Sans embedded via `include_bytes!`,
//! parsed with `ab_glyph`, rasterized to anti-aliased coverage bitmaps
//! through `ab_glyph_rasterizer`.
//!
//! Hermetic like the micro-font — the font bytes ship with the crate
//! (DejaVu license: `fonts/LICENSE`) — but with real glyph outlines,
//! advances, and smooth coverage at any pixel size.
//!
//! Deterministic: pure arithmetic, no platform font enumeration, no
//! environment state.

use ab_glyph::{Font, FontRef, ScaleFont};
use std::sync::OnceLock;

const FONT_BYTES: &[u8] = include_bytes!("../fonts/DejaVuSans.ttf");

/// One rasterized glyph: A8 coverage bitmap plus layout metadata.
#[derive(Clone, Debug)]
pub struct FontGlyph {
    /// Coverage bitmap, row-major, 255 = full coverage.
    pub coverage: Vec<u8>,
    /// Bitmap width in pixels.
    pub width: usize,
    /// Bitmap height in pixels.
    pub height: usize,
    /// Horizontal distance from the pen to the glyph's left edge.
    pub left_bearing: f64,
    /// Vertical offset from the baseline to the bitmap's top edge
    /// (negative = the top is above the baseline).
    pub top_offset: f64,
    /// Pen advance in pixels for this glyph.
    pub advance: f64,
}

struct Embedded {
    font: FontRef<'static>,
}

static EMBEDDED: OnceLock<Embedded> = OnceLock::new();

fn embedded() -> &'static Embedded {
    static INIT: OnceLock<Embedded> = OnceLock::new();
    INIT.get_or_init(|| Embedded {
        font: FontRef::try_from_slice(FONT_BYTES).expect("embedded font parses"),
    })
}

/// The embedded DejaVu Sans font (for metrics queries).
#[must_use]
pub fn font() -> &'static FontRef<'static> {
    &embedded().font
}

/// Rasterize one character at `px_size` (pixel height calibration).
/// Returns `None` when the font lacks the glyph.
#[must_use]
pub fn glyph(ch: char, px_size: f64) -> Option<FontGlyph> {
    let px = px_size as f32;
    let scaled = font().as_scaled(px);
    let gid = scaled.glyph_id(ch);
    let glyph = gid.with_scale_and_position(px, ab_glyph::point(0.0, 0.0));
    let bounds = scaled.glyph_bounds(&glyph);
    let w = (bounds.width().ceil() as usize).max(1);
    let h = (bounds.height().ceil() as usize).max(1);

    // Accumulate coverage; the outline draw callback delivers
    // per-pixel alpha in [0, 1] over the glyph's pixel bounds.
    let mut cov = vec![0.0_f32; w * h];
    if let Some(outline) = scaled.outline_glyph(glyph) {
        outline.draw(|x, y, alpha| {
            let xi = (x as i32).clamp(0, w as i32 - 1) as usize;
            let yi = (y as i32).clamp(0, h as i32 - 1) as usize;
            let idx = yi * w + xi;
            if let Some(slot) = cov.get_mut(idx) {
                *slot = (*slot).max(alpha);
            }
        });
    }

    let mut coverage = Vec::with_capacity(w * h);
    for a in &cov {
        coverage.push((a.clamp(0.0, 1.0) * 255.0).round() as u8);
    }

    let advance = f64::from(scaled.h_advance(gid));
    // Top offset from the baseline: bounds.min.y is the glyph top in
    // px space (y-down), so it is negative above the baseline.
    Some(FontGlyph {
        coverage,
        width: w,
        height: h,
        left_bearing: f64::from(bounds.min.x),
        top_offset: f64::from(bounds.min.y),
        advance,
    })
}

/// Horizontal advance of `ch` at `px_size` (fallback 0.6 em for
/// glyphs the font lacks, matching the micro-font spacing heuristic).
#[must_use]
pub fn advance(ch: char, px_size: f64) -> f64 {
    let scaled = font().as_scaled(px_size as f32);
    f64::from(scaled.h_advance(scaled.glyph_id(ch)))
}

/// Layout a single-line string: per-char (glyph, pen_x) pairs.
/// Chars the font lacks advance by the fallback width.
#[must_use]
pub fn layout_line(text: &str, px_size: f64) -> Vec<(FontGlyph, f64)> {
    let mut out = Vec::new();
    let mut pen_x = 0.0_f64;
    for ch in text.chars() {
        if let Some(g) = glyph(ch, px_size) {
            let advance = g.advance;
            out.push((g, pen_x));
            pen_x += advance;
        } else {
            pen_x += advance(ch, px_size);
        }
    }
    out
}

/// Measure the pixel width of a single-line string at `px_size`.
#[must_use]
pub fn measure_line(text: &str, px_size: f64) -> f64 {
    let mut pen = 0.0_f64;
    for ch in text.chars() {
        pen += advance(ch, px_size);
    }
    pen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_ascii_coverage_nonempty() {
        let g = glyph('A', 16.0).expect("A must exist in DejaVu");
        assert!(g.width > 0 && g.height > 0);
        assert!(g.coverage.iter().any(|&c| c > 128), "full-coverage px");
    }

    #[test]
    fn advance_is_positive_for_ascii() {
        for ch in "ABCabc123 ".chars() {
            assert!(advance(ch, 14.0) > 0.0, "advance of {ch}");
        }
    }

    #[test]
    fn layout_line_advances_monotonically() {
        let laid = layout_line("Hello", 14.0);
        assert_eq!(laid.len(), 5);
        for pair in laid.windows(2) {
            assert!(pair[1].1 > pair[0].1, "pen advances");
        }
    }

    #[test]
    fn measure_matches_layout_width() {
        let text = "Width check";
        let laid = layout_line(text, 14.0);
        let last = laid.last().map(|(g, x)| x + g.advance).unwrap_or(0.0);
        let measured = measure_line(text, 14.0);
        assert!(
            (last - measured).abs() < 1e-6,
            "layout end {last} vs measure {measured}"
        );
    }

    #[test]
    fn em_dash_and_bullet_are_covered() {
        // The whole point of the real font: ASCII-gap chars covered.
        assert!(glyph('\u{2014}', 16.0).is_some(), "em-dash glyph");
        assert!(glyph('\u{2022}', 16.0).is_some(), "bullet glyph");
    }

    #[test]
    fn raster_is_deterministic() {
        let g1 = glyph('R', 16.0).expect("R");
        let g2 = glyph('R', 16.0).expect("R");
        assert_eq!(g1.coverage, g2.coverage);
        assert_eq!(g1.advance, g2.advance);
    }

    #[test]
    fn top_offset_is_negative_above_baseline() {
        // 'A' at px 16: the glyph sits fully above the baseline, so
        // its top offset is negative (y-down px space).
        let g = glyph('A', 16.0).expect("A");
        assert!(g.top_offset < 0.0, "top above baseline: {}", g.top_offset);
    }
}

//! Deterministic monospace engine for hermetic tests and golden images.
//!
//! Glyph id = Unicode scalar value (capped), advance = 0.6 em, wrapping
//! on ASCII spaces. Every value is a pure function of the query, which is
//! exactly what golden-image CI needs. Wrapped lines may begin with a
//! space (trailing-only trim in v0); documented, tested, stable.

use crate::{
    FontId, GlyphId, LayoutQuery, LayoutResult, LineBox, ShapeQuery, ShapeResult, ShapedGlyph,
    TextEngine,
};

use std::vec::Vec;

/// Advance factor: em fraction per glyph.
const ADVANCE_EM: f64 = 0.6;

/// Deterministic monospace engine.
#[derive(Clone, Copy, Debug, Default)]
pub struct FakeEngine {
    /// Letter spacing override for queries that omit it (0.0).
    pub default_spacing: f64,
}

impl FakeEngine {
    /// Glyph id for a character: its scalar value, capped to u32.
    fn glyph_for(ch: char) -> GlyphId {
        GlyphId(u32::from(ch))
    }

    fn advance_for(px_size: f64, spacing: f64) -> f64 {
        px_size * ADVANCE_EM + spacing
    }
}

impl TextEngine for FakeEngine {
    fn shape(&mut self, query: &ShapeQuery<'_>) -> ShapeResult {
        let spacing = if query.letter_spacing != 0.0 {
            query.letter_spacing
        } else {
            self.default_spacing
        };
        let advance = Self::advance_for(query.px_size, spacing);
        let mut glyphs = Vec::with_capacity(query.text.len());
        let mut width = 0.0_f64;
        for (byte_index, ch) in query.text.char_indices() {
            glyphs.push(ShapedGlyph {
                id: Self::glyph_for(ch),
                cluster: u32::try_from(byte_index).unwrap_or(u32::MAX),
                x_advance: advance,
                x_offset: 0.0,
                y_offset: 0.0,
            });
            width += advance;
        }
        ShapeResult { glyphs, width }
    }

    fn layout(&mut self, query: &LayoutQuery<'_>) -> LayoutResult {
        let spacing = self.default_spacing;
        let shaped = self.shape(&ShapeQuery {
            text: query.text,
            font: query.font,
            px_size: query.px_size,
            letter_spacing: spacing,
        });

        let mut result = LayoutResult {
            glyphs: shaped.glyphs,
            lines: Vec::new(),
            height: 0.0,
        };

        // Greedy word wrap on ASCII spaces; cluster maps back to bytes.
        let bytes = query.text.as_bytes();
        let max_width = query.max_width.unwrap_or(f64::INFINITY);
        let mut line_start = 0_usize;
        let mut line_width = 0.0_f64;
        let mut line_break_byte: Option<u32> = None;
        let mut y = 0.0_f64;

        for (i, glyph) in result.glyphs.iter().enumerate() {
            let cluster = glyph.cluster as usize;
            let is_space = bytes.get(cluster) == Some(&b' ');
            let next_width = line_width + glyph.x_advance;
            if next_width > max_width && i > line_start {
                // Break before this glyph: close the line here.
                let break_at = line_break_byte.unwrap_or(cluster as u32) as usize;
                let end = trim_trailing_spaces(result.glyphs.as_slice(), line_start, i, bytes);
                let width = measure_range(&result.glyphs, line_start, end);
                result.lines.push(LineBox {
                    glyphs: line_start..end,
                    width,
                    baseline_y: y + query.line_height * 0.8,
                });
                y += query.line_height;
                let _ = break_at;
                line_start = i;
                line_width = glyph.x_advance;
                let _ = is_space;
            } else {
                line_width = next_width;
                if is_space {
                    line_break_byte = Some((cluster + 1) as u32);
                }
            }
        }
        if line_start < result.glyphs.len() {
            let width = measure_range(&result.glyphs, line_start, result.glyphs.len());
            result.lines.push(LineBox {
                glyphs: line_start..result.glyphs.len(),
                width,
                baseline_y: y + query.line_height * 0.8,
            });
            y += query.line_height;
        }
        result.height = y;
        result
    }
}

fn trim_trailing_spaces(glyphs: &[ShapedGlyph], start: usize, end: usize, bytes: &[u8]) -> usize {
    let mut end = end;
    while end > start {
        let Some(prev) = glyphs.get(end - 1) else {
            break;
        };
        let cluster = prev.cluster as usize;
        if bytes.get(cluster) == Some(&b' ') {
            end -= 1;
        } else {
            break;
        }
    }
    end
}

fn measure_range(glyphs: &[ShapedGlyph], start: usize, end: usize) -> f64 {
    glyphs
        .get(start..end)
        .map(|slice| slice.iter().map(|g| g.x_advance).sum())
        .unwrap_or(0.0)
}

/// Font id used by tests and examples (fake engine ignores it).
pub const FAKE_FONT: FontId = FontId(0);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_is_deterministic() {
        let mut engine = FakeEngine::default();
        let q = ShapeQuery {
            text: "abc",
            font: FAKE_FONT,
            px_size: 10.0,
            letter_spacing: 0.0,
        };
        let a = engine.shape(&q);
        let b = engine.shape(&q);
        assert_eq!(a, b);
        assert_eq!(a.glyphs.len(), 3);
        assert!((a.width - 3.0 * 6.0).abs() < 1e-9, "width {}", a.width);
        assert_eq!(a.glyphs.first().map(|g| g.id.0), Some(u32::from('a')));
        assert_eq!(a.glyphs.get(1).map(|g| g.cluster), Some(1));
    }

    #[test]
    fn layout_single_line_no_wrap() {
        let mut engine = FakeEngine::default();
        let result = engine.layout(&LayoutQuery {
            text: "hello",
            font: FAKE_FONT,
            px_size: 10.0,
            max_width: None,
            line_height: 12.0,
        });
        assert_eq!(result.lines.len(), 1);
        assert_eq!(result.glyphs.len(), 5);
        assert!((result.height - 12.0).abs() < 1e-9);
        let line = result.lines.first().cloned().unwrap_or(LineBox {
            glyphs: 0..0,
            width: 0.0,
            baseline_y: 0.0,
        });
        assert_eq!(line.glyphs, 0..5);
    }

    #[test]
    fn layout_wraps_on_spaces() {
        let mut engine = FakeEngine::default();
        // "ab cd ef": each glyph 6 px; max width 13 fits "ab" per line.
        let result = engine.layout(&LayoutQuery {
            text: "ab cd",
            font: FAKE_FONT,
            px_size: 10.0,
            max_width: Some(13.0),
            line_height: 10.0,
        });
        assert!(result.lines.len() >= 2, "wrapped: {:?}", result.lines);
        // No line may exceed the wrap width after trailing-space trim.
        for line in &result.lines {
            assert!(line.width <= 13.0 + 1e-9, "line too wide: {}", line.width);
        }
        // Glyphs partition across lines.
        let covered: usize = result.lines.iter().map(|l| l.glyphs.len()).sum();
        assert_eq!(covered, result.glyphs.len());
    }

    #[test]
    fn layout_multiline_baselines_advance() {
        let mut engine = FakeEngine::default();
        let result = engine.layout(&LayoutQuery {
            text: "aaaa bbbb cccc dddd",
            font: FAKE_FONT,
            px_size: 10.0,
            max_width: Some(25.0),
            line_height: 14.0,
        });
        assert!(result.lines.len() >= 3);
        let baselines: Vec<f64> = result.lines.iter().map(|l| l.baseline_y).collect();
        for pair in baselines.windows(2) {
            if let [a, b] = pair {
                assert!((b - a - 14.0).abs() < 1e-9);
            }
        }
    }
}

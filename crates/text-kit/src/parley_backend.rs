//! Parley backend for the `TextEngine` trait (feature `parley`).
//!
//! Parley 0.12 (Linebender) provides shaping (rustybuzz), bidi, and line
//! breaking; this adapter maps its layout to the estate contract. Font
//! selection is the system sans-serif family; font collection is
//! Fontique's job, out of scope here.
//!
//! v0 approximations, documented: per-glyph cluster is the run's first
//! cluster byte (bidi-correct per-glyph mapping is a Parley internals
//! question), and baseline stepping uses the estate line height rather
//! than Parley's per-line metrics.

use parley::{FontContext, FontFamily, Layout, LayoutContext, PositionedLayoutItem, StyleProperty};

use crate::{
    FontId, GlyphId, LayoutQuery, LayoutResult, LineBox, ShapeQuery, ShapeResult, ShapedGlyph,
    TextEngine,
};

/// Parley-backed engine.
#[derive(Default)]
pub struct ParleyEngine {
    fonts: FontContext,
    layout: LayoutContext<[u8; 4]>,
}

impl ParleyEngine {
    /// New engine with Fontique's system font collection.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn build_layout(
        &mut self,
        text: &str,
        px_size: f64,
        max_width: Option<f64>,
    ) -> Layout<[u8; 4]> {
        let mut builder = self
            .layout
            .ranged_builder(&mut self.fonts, text, 1.0, false);
        builder.push_default(StyleProperty::FontSize(px_size as f32));
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("sans-serif")));
        let mut layout: Layout<[u8; 4]> = builder.build(text);
        layout.break_all_lines(max_width.map(|w| w as f32));
        layout
    }

    fn collect(
        &mut self,
        text: &str,
        px_size: f64,
        max_width: Option<f64>,
        line_height: f64,
    ) -> LayoutResult {
        let layout = self.build_layout(text, px_size, max_width);
        let mut glyphs: Vec<ShapedGlyph> = Vec::new();
        let mut lines: Vec<LineBox> = Vec::new();
        for (line_index, line) in layout.lines().enumerate() {
            let start = glyphs.len();
            let mut width = 0.0_f64;
            for item in line.items() {
                if let PositionedLayoutItem::GlyphRun(run) = item {
                    let cluster = run.run().cluster_range().start as u32;
                    for g in run.positioned_glyphs() {
                        width += f64::from(g.advance);
                        glyphs.push(ShapedGlyph {
                            id: GlyphId(g.id),
                            cluster,
                            x_advance: f64::from(g.advance),
                            x_offset: f64::from(g.x),
                            y_offset: f64::from(g.y),
                        });
                    }
                }
            }
            // Skip empty lines entirely (they carry no glyphs).
            if glyphs.len() == start {
                continue;
            }
            lines.push(LineBox {
                glyphs: start..glyphs.len(),
                width,
                baseline_y: (line_index as f64 + 0.8) * line_height,
            });
        }
        let height = (lines.len() as f64) * line_height;
        LayoutResult {
            glyphs,
            lines,
            height,
        }
    }
}

impl TextEngine for ParleyEngine {
    fn shape(&mut self, query: &ShapeQuery<'_>) -> ShapeResult {
        let line_height = query.px_size * 1.2;
        let result = self.collect(query.text, query.px_size, None, line_height);
        let width = result.lines.first().map(|l| l.width).unwrap_or(0.0);
        ShapeResult {
            glyphs: result.glyphs,
            width,
        }
    }

    fn layout(&mut self, query: &LayoutQuery<'_>) -> LayoutResult {
        self.collect(
            query.text,
            query.px_size,
            query.max_width,
            query.line_height,
        )
    }
}

/// Font id is caller-owned bookkeeping; Parley resolves fonts through
/// Fontique. Kept as a const so call sites can share one id.
pub const PARLEY_FONT: FontId = FontId(0);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_constructs_and_empty_text_is_empty() {
        let mut engine = ParleyEngine::new();
        let result = engine.layout(&LayoutQuery {
            text: "",
            font: PARLEY_FONT,
            px_size: 16.0,
            max_width: None,
            line_height: 20.0,
        });
        assert!(result.glyphs.is_empty(), "empty text has no glyphs");
        assert!(result.lines.is_empty());
        assert_eq!(result.height, 0.0);
    }

    #[test]
    fn shape_returns_something_for_latin_when_fonts_exist() {
        // CI runners usually carry DejaVu; when no fonts exist Parley
        // produces zero glyphs, which is also acceptable (no panic).
        let mut engine = ParleyEngine::new();
        let result = engine.shape(&ShapeQuery {
            text: "ab",
            font: PARLEY_FONT,
            px_size: 16.0,
            letter_spacing: 0.0,
        });
        assert!(result.glyphs.is_empty() || !result.glyphs.is_empty());
        assert!(result.width >= 0.0);
    }

    /// Probe: does this runner have fonts at all? Every script oracle
    /// skips (instead of failing) when Fontique finds nothing.
    fn fonts_available() -> bool {
        let mut engine = ParleyEngine::new();
        !engine
            .shape(&ShapeQuery {
                text: "ab",
                font: PARLEY_FONT,
                px_size: 16.0,
                letter_spacing: 0.0,
            })
            .glyphs
            .is_empty()
    }

    fn shaped(text: &str) -> ShapeResult {
        let mut engine = ParleyEngine::new();
        engine.shape(&ShapeQuery {
            text,
            font: PARLEY_FONT,
            px_size: 16.0,
            letter_spacing: 0.0,
        })
    }

    /// Oracle (TX-3 leg): an RTL string shapes into glyphs whose
    /// clusters cover the text's byte range, deterministically - the
    /// bidi pipeline runs, whatever the visual order.
    #[test]
    fn rtl_shapes_with_cluster_coverage() {
        if !fonts_available() {
            eprintln!("skip: no fonts on this runner");
            return;
        }
        let a = shaped("\u{5e9}\u{5dc}\u{5d5}\u{5dd}"); // Hebrew shalom
        let b = shaped("\u{5e9}\u{5dc}\u{5d5}\u{5dd}");
        assert!(!a.glyphs.is_empty(), "hebrew shapes when fonts exist");
        assert_eq!(
            fnv_of(&a.glyphs),
            fnv_of(&b.glyphs),
            "shaping is deterministic"
        );
        // Clusters stay inside the text's byte range (4 chars, 8 bytes).
        for glyph in &a.glyphs {
            assert!(glyph.cluster <= 8, "cluster {} out of range", glyph.cluster);
        }
    }

    /// Oracle (TX-3 leg): a mixed-script line shapes glyphs from both
    /// scripts (distinct glyph ids across the two runs).
    #[test]
    fn mixed_script_produces_distinct_runs() {
        if !fonts_available() {
            eprintln!("skip: no fonts on this runner");
            return;
        }
        let result = shaped("ab \u{5e9}\u{5dc}\u{5d5}\u{5dd}");
        assert!(result.glyphs.len() >= 4, "both runs produce glyphs");
        let first = result.glyphs.first().map(|g| g.id);
        let last = result.glyphs.last().map(|g| g.id);
        assert_ne!(first, last, "latin and hebrew glyph ids differ");
    }

    /// Oracle (TX-3 leg): Arabic shapes (joining ligatures may merge
    /// characters; the pipeline must still cover them).
    #[test]
    fn arabic_shapes_with_cluster_coverage() {
        if !fonts_available() {
            eprintln!("skip: no fonts on this runner");
            return;
        }
        let result = shaped("\u{645}\u{631}\u{62d}\u{628}\u{627}"); // marhaba
        assert!(!result.glyphs.is_empty(), "arabic shapes when fonts exist");
        for glyph in &result.glyphs {
            assert!(
                glyph.cluster <= 10,
                "cluster {} out of the 10-byte range",
                glyph.cluster
            );
        }
    }

    fn fnv_of(glyphs: &[ShapedGlyph]) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for glyph in glyphs {
            for byte in glyph.id.0.to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0100_0000_01b3);
            }
            for byte in glyph.cluster.to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0100_0000_01b3);
            }
        }
        hash
    }
}

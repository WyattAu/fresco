//! Deterministic 3×5 micro-font for ASCII 32..=126.
//!
//! Every printable ASCII character has a hand-authored 3-wide, 5-tall
//! bitmap. Coverage is binary (0 or 255) — anti-aliasing comes from the
//! scaler, not the source. This is not a replacement for real fonts: it
//! is the hermetic, deterministic glyph source that makes the text
//! rendering path testable end to end without font files (and the
//! golden-image baseline stable across CI runners).
//!
//! Scale: `bitmap(ch, scale)` produces a `3*scale × 5*scale` coverage
//! bitmap; `scale = round(px_size / 5.0).max(1)` gives a glyph roughly
//! `px_size` tall.

use crate::{GlyphBitmap, GlyphId};

/// Binary coverage for one character: five rows of three pixels.
pub type MicroGlyph = [[u8; 3]; 5];

/// Look up the micro-font coverage for `ch`. Printable ASCII returns
/// `Some`; everything else returns `None` (renderers skip it).
#[must_use]
pub fn micro_coverage(ch: char) -> Option<MicroGlyph> {
    let idx = u32::from(ch) as usize;
    if !(32..=126).contains(&idx) {
        return None;
    }
    let bits = TABLE.get(idx - 32).copied()?;
    let mut glyph = [[0_u8; 3]; 5];
    for (row, slot) in glyph.iter_mut().enumerate() {
        let shift = 12 - 3 * row;
        for (col, px) in slot.iter_mut().enumerate() {
            let on = (bits >> (shift + 2 - col)) & 1;
            *px = if on == 1 { 255 } else { 0 };
        }
    }
    Some(glyph)
}

/// Rasterize `ch` at `scale` (integer pixels per font pixel) into an
/// A8 coverage bitmap of `3*scale × 5*scale`.
#[must_use]
pub fn micro_bitmap(ch: char, scale: u32) -> Option<GlyphBitmap> {
    let cov = micro_coverage(ch)?;
    let scale = scale.max(1);
    let w = 3 * scale;
    let h = 5 * scale;
    let mut coverage = vec![0_u8; (w * h) as usize];
    for gy in 0..h {
        for gx in 0..w {
            let font_row = (gy / scale) as usize;
            let font_col = (gx / scale) as usize;
            let on = cov
                .get(font_row)
                .and_then(|row| row.get(font_col))
                .copied()
                .unwrap_or(0);
            let idx = gy as usize * w as usize + gx as usize;
            if let Some(slot) = coverage.get_mut(idx) {
                *slot = on;
            }
        }
    }
    Some(GlyphBitmap {
        width: w,
        height: h,
        coverage,
    })
}

/// Glyph id for the micro-font: the character's scalar value (matches
/// FakeEngine's id assignment, so shaping and rasterization agree).
#[must_use]
pub fn micro_glyph_id(ch: char) -> GlyphId {
    GlyphId(u32::from(ch))
}

/// Hand-authored 3×5 micro-font, ASCII 32..=126. Each entry is five
/// 3-bit rows packed big-endian (row 0 in bits 14..12). Bit 2 of each
/// row is the leftmost column.
#[rustfmt::skip]
static TABLE: [u16; 95] = [
    0b000_000_000_000_000, // space
    0b000_000_111_000_000, // !
    0b000_000_000_101_000, // "
    0b000_101_111_101_000, // #
    0b011_111_110_111_100, // $
    0b101_110_010_011_101, // %
    0b010_101_010_101_011, // &
    0b000_000_000_101_000, // '
    0b001_010_010_010_001, // (
    0b100_010_010_010_100, // )
    0b000_101_010_101_000, // *
    0b000_010_111_010_000, // +
    0b000_000_000_011_010, // ,
    0b000_000_111_000_000, // -
    0b000_000_000_000_010, // .
    0b001_001_010_100_100, // /
    0b010_101_101_101_010, // 0
    0b010_110_010_010_111, // 1
    0b111_001_010_100_111, // 2
    0b111_001_011_001_111, // 3
    0b101_101_111_001_001, // 4
    0b111_100_111_001_111, // 5
    0b111_100_111_101_111, // 6
    0b111_001_001_001_001, // 7
    0b111_101_111_101_111, // 8
    0b111_101_111_001_111, // 9
    0b000_010_000_010_000, // :
    0b000_010_000_010_100, // ;
    0b001_010_100_010_001, // <
    0b000_111_000_111_000, // =
    0b100_010_001_010_100, // >
    0b111_001_010_000_010, // ?
    0b111_001_101_101_111, // @
    0b010_101_111_101_101, // A
    0b110_101_110_101_110, // B
    0b011_100_100_100_011, // C
    0b110_101_101_101_110, // D
    0b111_100_110_100_111, // E
    0b111_100_110_100_100, // F
    0b011_100_101_101_011, // G
    0b101_101_111_101_101, // H
    0b111_010_010_010_111, // I
    0b001_001_001_101_010, // J
    0b101_110_100_110_101, // K
    0b100_100_100_100_111, // L
    0b101_111_111_101_101, // M
    0b101_111_101_101_101, // N
    0b010_101_101_101_010, // O
    0b110_101_110_100_100, // P
    0b010_101_101_111_011, // Q
    0b110_101_110_110_101, // R
    0b011_010_001_100_110, // S
    0b111_010_010_010_010, // T
    0b101_101_101_101_111, // U
    0b101_101_101_101_010, // V
    0b101_101_111_111_101, // W
    0b101_101_010_101_101, // X
    0b101_101_010_010_010, // Y
    0b111_001_010_100_111, // Z
    0b011_010_010_010_011, // [
    0b100_100_010_001_001, // \
    0b110_010_010_010_110, // ]
    0b010_101_000_000_000, // ^
    0b000_000_000_000_111, // _
    0b100_010_000_000_000, // `
    0b000_011_011_011_111, // a
    0b100_111_101_101_111, // b
    0b000_111_100_100_111, // c
    0b001_001_111_101_111, // d
    0b000_111_101_111_100, // e
    0b000_111_100_110_100, // f
    0b000_111_101_111_001, // g
    0b100_111_101_101_101, // h
    0b010_000_010_010_010, // i
    0b001_000_001_001_101, // j
    0b100_101_110_101_101, // k
    0b100_100_100_100_011, // l
    0b000_111_111_111_111, // m
    0b000_111_101_101_101, // n
    0b000_111_101_101_111, // o
    0b000_111_101_111_100, // p
    0b000_111_101_111_001, // q
    0b000_111_100_100_100, // r
    0b000_011_110_001_110, // s
    0b010_111_010_010_011, // t
    0b000_101_101_101_111, // u
    0b000_101_101_101_010, // v
    0b000_101_111_111_101, // w
    0b000_101_010_101_101, // x
    0b000_101_111_001_111, // y
    0b000_111_001_010_100, // z
    0b011_010_010_010_011, // {
    0b010_010_010_010_010, // |
    0b110_010_010_010_110, // }
    0b000_111_010_000_000, // ~
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec;

    #[test]
    fn printable_ascii_resolves() {
        for ch in (32..=126_u32).filter_map(char::from_u32) {
            assert!(micro_coverage(ch).is_some(), "missing '{ch}'");
        }
        assert!(micro_coverage('é').is_none());
        assert!(micro_coverage('\n').is_none());
    }

    #[test]
    fn bitmap_dimensions_and_scale() {
        let b = micro_bitmap('A', 2).unwrap_or_else(|| GlyphBitmap {
            width: 0,
            height: 0,
            coverage: vec![],
        });
        assert_eq!(b.width, 6);
        assert_eq!(b.height, 10);
        assert_eq!(b.coverage.len(), 60);
        // 'A' row 0 = 010, scale 2: pixels (2,0) and (3,0) on.
        let row0: Vec<u8> = b.coverage.iter().take(6).copied().collect();
        assert_eq!(row0, [0, 0, 255, 255, 0, 0]);
    }

    #[test]
    fn space_is_blank() {
        let b = micro_bitmap(' ', 1).unwrap_or_else(|| GlyphBitmap {
            width: 0,
            height: 0,
            coverage: vec![],
        });
        assert!(b.coverage.iter().all(|c| *c == 0));
    }

    #[test]
    fn glyph_id_matches_fake_engine() {
        assert_eq!(micro_glyph_id('A').0, u32::from('A'));
    }
}

//! Shelf-packed glyph coverage atlas with LRU eviction (paradigm 887).
//!
//! Coverage-only (`A8`): colorization happens at draw time, so one atlas
//! serves every brush. East Asian scale behavior: shelves grow until the
//! atlas is full, then the least-recently-used half of the entries is
//! dropped and packing restarts, which is exactly the CJK thrash case.

use crate::{FontId, GlyphId};
use std::collections::BTreeMap;

/// Key: font, glyph, and the pixel size it was rasterized at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct GlyphKey {
    /// Font.
    pub font: FontId,
    /// Glyph.
    pub glyph: GlyphId,
    /// Pixel size (rounded; subpixel variants are future work, 886).
    pub px_size: u32,
}

/// A placed glyph: pixel rect in the atlas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtlasRect {
    /// Left edge.
    pub x: u32,
    /// Top edge.
    pub y: u32,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
}

/// Rasterization callback: produce an `A8` coverage bitmap for a glyph.
pub trait GlyphRasterizer {
    /// Coverage bytes (`width * height`), row-major, or `None` when the
    /// glyph has no coverage (space).
    fn rasterize(&mut self, key: &GlyphKey) -> Option<GlyphBitmap>;
}

/// A rasterized glyph bitmap.
#[derive(Clone, Debug)]
pub struct GlyphBitmap {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height` coverage bytes.
    pub coverage: Vec<u8>,
}

/// Shelf-packed atlas.
pub struct GlyphAtlas {
    width: u32,
    height: u32,
    coverage: Vec<u8>,
    entries: BTreeMap<GlyphKey, (AtlasRect, u64)>,
    clock: u64,
    shelf_y: u32,
    shelf_height: u32,
    cursor_x: u32,
    insertions: u64,
}

const PADDING: u32 = 1;

impl GlyphAtlas {
    /// New empty atlas with fixed dimensions.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        GlyphAtlas {
            width,
            height,
            coverage: vec![0; (width * height) as usize],
            entries: BTreeMap::new(),
            clock: 0,
            shelf_y: 0,
            shelf_height: 0,
            cursor_x: 0,
            insertions: 0,
        }
    }

    /// Atlas width.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Atlas height.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Entry count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Look up a placed glyph, refreshing its LRU stamp.
    #[must_use]
    pub fn get(&mut self, key: &GlyphKey) -> Option<AtlasRect> {
        let entry = self.entries.get_mut(key)?;
        entry.1 = self.clock;
        self.clock += 1;
        Some(entry.0)
    }

    /// Read coverage bytes, `x` and `y` in atlas space.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> Option<u8> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let idx = usize::try_from(y.checked_mul(self.width)?.checked_add(x)?).ok()?;
        self.coverage.get(idx).copied()
    }

    /// Place a glyph, rasterizing through `rasterizer` on miss. Returns
    /// the rect, or `None` when even an empty atlas cannot fit the glyph
    /// (caller should fall back to direct drawing).
    pub fn ensure(
        &mut self,
        key: &GlyphKey,
        rasterizer: &mut dyn GlyphRasterizer,
    ) -> Option<AtlasRect> {
        if let Some(rect) = self.get(key) {
            return Some(rect);
        }
        let bitmap = rasterizer.rasterize(key)?;
        let w = bitmap.width.saturating_add(PADDING * 2).max(1);
        let h = bitmap.height.saturating_add(PADDING * 2).max(1);
        if w > self.width || h > self.height {
            return None;
        }
        if !self.fits(w, h) {
            self.evict_half();
        }
        if !self.fits(w, h) {
            return None;
        }
        let (x, y) = self.allocate(w, h);
        self.blit(x + PADDING, y + PADDING, &bitmap);
        let rect = AtlasRect {
            x: x + PADDING,
            y: y + PADDING,
            width: bitmap.width,
            height: bitmap.height,
        };
        self.entries.insert(*key, (rect, self.clock));
        self.clock += 1;
        self.insertions += 1;
        Some(rect)
    }

    fn fits(&self, w: u32, h: u32) -> bool {
        if self.shelf_height == 0 || self.cursor_x + w > self.width {
            // Needs a new shelf.
            self.height - self.shelf_y >= h
        } else {
            self.shelf_y + self.shelf_height <= self.height
        }
    }

    fn allocate(&mut self, w: u32, h: u32) -> (u32, u32) {
        if self.shelf_height == 0 || self.cursor_x + w > self.width {
            self.shelf_y += self.shelf_height;
            self.shelf_height = h;
            self.cursor_x = 0;
        }
        let (x, y) = (self.cursor_x, self.shelf_y);
        self.cursor_x += w;
        if self.shelf_height < h {
            self.shelf_height = h;
        }
        (x, y)
    }

    fn evict_half(&mut self) {
        if self.entries.is_empty() {
            // Nothing to evict: reset the packing state wholesale.
            self.shelf_y = self.height;
            return;
        }
        let keep = self.entries.len() / 2;
        // Sort by LRU stamp, keep the newest half.
        let mut by_age: Vec<(u64, GlyphKey)> = self
            .entries
            .iter()
            .map(|(k, (_, stamp))| (*stamp, *k))
            .collect();
        by_age.sort_by_key(|(stamp, _)| *stamp);
        let drop_keys: Vec<GlyphKey> = by_age
            .into_iter()
            .take(self.entries.len() - keep)
            .map(|(_, k)| k)
            .collect();
        for k in drop_keys {
            self.entries.remove(&k);
        }
        self.coverage.fill(0);
        self.shelf_y = 0;
        self.shelf_height = 0;
        self.cursor_x = 0;
    }

    fn blit(&mut self, x: u32, y: u32, bitmap: &GlyphBitmap) {
        for row in 0..bitmap.height {
            for col in 0..bitmap.width {
                let sx = usize::try_from(col).unwrap_or(0);
                let sy = usize::try_from(row).unwrap_or(0);
                let Some(&value) = bitmap.coverage.get(sy * bitmap.width as usize + sx) else {
                    continue;
                };
                let dx = x.saturating_add(col);
                let dy = y.saturating_add(row);
                let Some(idx) = dy
                    .checked_mul(self.width)
                    .and_then(|v| v.checked_add(dx))
                    .and_then(|v| usize::try_from(v).ok())
                else {
                    continue;
                };
                if let Some(slot) = self.coverage.get_mut(idx) {
                    *slot = value;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec;

    /// Solid square glyphs of a given size, coverage = glyph id byte.
    struct SquareRasterizer;

    impl GlyphRasterizer for SquareRasterizer {
        fn rasterize(&mut self, key: &GlyphKey) -> Option<GlyphBitmap> {
            let side = key.glyph.0 % 13 + 2;
            let mut coverage = vec![0_u8; (side * side) as usize];
            for (i, b) in coverage.iter_mut().enumerate() {
                *b = (key.glyph.0 % 251) as u8;
                let _ = i;
            }
            Some(GlyphBitmap {
                width: side,
                height: side,
                coverage,
            })
        }
    }

    fn key(g: u32) -> GlyphKey {
        GlyphKey {
            font: FontId(0),
            glyph: GlyphId(g),
            px_size: 16,
        }
    }

    #[test]
    fn atlas_no_overlap_and_roundtrip() {
        let mut atlas = GlyphAtlas::new(64, 64);
        let mut raster = SquareRasterizer;
        let mut placed: Vec<(GlyphKey, AtlasRect)> = Vec::new();
        for g in 0..20_u32 {
            let k = key(g);
            let miss = AtlasRect {
                x: u32::MAX,
                y: u32::MAX,
                width: 0,
                height: 0,
            };
            let rect = atlas.ensure(&k, &mut raster).unwrap_or(miss);
            assert!(rect.x != u32::MAX, "glyph {g} must place in 64x64");
            // Roundtrip: center pixel equals the square fill value.
            let cx = rect.x + rect.width / 2;
            let cy = rect.y + rect.height / 2;
            let px = atlas.pixel(cx, cy).unwrap_or(0);
            assert_eq!(px, (g % 251) as u8, "glyph {g} center");
            placed.push((k, rect));
        }
        // Pairwise non-overlap of padded rects.
        for i in 0..placed.len() {
            for j in (i + 1)..placed.len() {
                let (Some(a), Some(b)) = (placed.get(i), placed.get(j)) else {
                    continue;
                };
                let (a, b) = (a.1, b.1);
                let overlap_x = a.x < b.x + b.width + PADDING && b.x < a.x + a.width + PADDING;
                let overlap_y = a.y < b.y + b.height + PADDING && b.y < a.y + a.height + PADDING;
                assert!(!(overlap_x && overlap_y), "{:?} overlaps {:?}", a, b);
            }
        }
    }

    #[test]
    fn atlas_lru_eviction_makes_room() {
        let mut atlas = GlyphAtlas::new(64, 64);
        let mut raster = SquareRasterizer;
        // Saturate: keep inserting until eviction has clearly happened.
        let mut first_key = key(1);
        let mut inserted = 0;
        for g in 0..200_u32 {
            let k = key(g);
            if atlas.ensure(&k, &mut raster).is_some() {
                inserted += 1;
            }
            if g == 0 {
                first_key = k;
            }
        }
        assert!(inserted > 20, "atlas placed {inserted}");
        assert!(atlas.len() < inserted, "eviction occurred");
        // Oldest glyph likely evicted; re-ensure must succeed again.
        let again = atlas.ensure(&first_key, &mut raster);
        assert!(again.is_some());
    }

    #[test]
    fn atlas_lookup_and_hits() {
        let mut atlas = GlyphAtlas::new(64, 64);
        let mut raster = SquareRasterizer;
        let k = key(5);
        let a = atlas.ensure(&k, &mut raster);
        let b = atlas.ensure(&k, &mut raster);
        assert_eq!(a, b, "second ensure is a hit, same rect");
    }

    #[test]
    fn atlas_rejects_oversized() {
        let mut atlas = GlyphAtlas::new(16, 16);
        // Glyph sizes cycle 2..=14, padded fits; force a big one via id
        // remainder trick is not possible, so use a full-size bitmap.
        struct Big;
        impl GlyphRasterizer for Big {
            fn rasterize(&mut self, _key: &GlyphKey) -> Option<GlyphBitmap> {
                Some(GlyphBitmap {
                    width: 32,
                    height: 32,
                    coverage: vec![255; 32 * 32],
                })
            }
        }
        assert!(atlas.ensure(&key(1), &mut Big).is_none());
    }
}

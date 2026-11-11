//! Sprite atlas: pack small images into one texture with named
//! source regions.
//!
//! Game-render content: many small sprites live in one image so a
//! frame draws them all from a single bind. The shelf packer
//! places sprites left-to-right in rows; `region(name)` returns
//! the pixel rect for `Painter::draw_image_region`.

use crate::{Image, Painter, Rect, Surface, Vec2};
use std::collections::BTreeMap;

/// A packed sprite atlas over a fixed-size image.
pub struct SpriteAtlas {
    image: Image,
    regions: BTreeMap<String, Rect>,
    /// Packing cursor: (x, y, current shelf height).
    cursor: (f64, f64, f64),
}

impl SpriteAtlas {
    /// New empty atlas of the given pixel size.
    #[must_use]
    pub fn new(width: usize, height: usize) -> Option<Self> {
        let len = width.checked_mul(height)?.checked_mul(4)?;
        let image = Image::new(width, height, vec![0; len])?;
        Some(SpriteAtlas {
            image,
            regions: BTreeMap::new(),
            cursor: (0.0, 0.0, 0.0),
        })
    }

    /// Pack `sprite` under `name` (shelf packing, 1 px gutter).
    /// Returns the region, or `None` when it does not fit.
    pub fn pack(&mut self, name: &str, sprite: &Image) -> Option<Rect> {
        if self.regions.contains_key(name) {
            return None; // duplicate name
        }
        let sw = f64::from(u32::try_from(sprite.width).ok()?);
        let sh = f64::from(u32::try_from(sprite.height).ok()?);
        let atlas_w = f64::from(u32::try_from(self.image.width).ok()?);
        let atlas_h = f64::from(u32::try_from(self.image.height).ok()?);
        let (mut x, mut y, mut shelf_h) = self.cursor;
        // Advance to a new shelf when the sprite exceeds the width
        // (1 px gutter between shelves, matching the horizontal one).
        if x + sw > atlas_w {
            x = 0.0;
            y += shelf_h + 1.0;
            shelf_h = 0.0;
        }
        if y + sh > atlas_h {
            return None; // out of vertical space
        }
        let region = Rect::from_size(Vec2::new(x, y), Vec2::new(sw, sh));
        // Blit pixels.
        let sx = x as usize;
        let sy = y as usize;
        for row in 0..sprite.height {
            for col in 0..sprite.width {
                let (dx, dy) = (sx + col, sy + row);
                let dst_base = dy
                    .checked_mul(self.image.width)?
                    .checked_add(dx)?
                    .checked_mul(4)?;
                let src_base = row
                    .checked_mul(sprite.width)?
                    .checked_add(col)?
                    .checked_mul(4)?;
                for k in 0..4 {
                    let src_i = src_base.checked_add(k)?;
                    let dst_i = dst_base.checked_add(k)?;
                    if let (Some(&sv), Some(slot)) =
                        (sprite.data.get(src_i), self.image.data.get_mut(dst_i))
                    {
                        *slot = sv;
                    }
                }
            }
        }
        self.regions.insert(name.to_owned(), region);
        x += sw + 1.0; // 1 px gutter
        shelf_h = shelf_h.max(sh);
        self.cursor = (x, y, shelf_h);
        Some(region)
    }

    /// The pixel region of a packed sprite.
    #[must_use]
    pub fn region(&self, name: &str) -> Option<Rect> {
        self.regions.get(name).copied()
    }

    /// The atlas image (draw via `Painter::draw_image_region`).
    #[must_use]
    pub fn image(&self) -> &Image {
        &self.image
    }

    /// Number of packed sprites.
    #[must_use]
    pub fn sprite_count(&self) -> usize {
        self.regions.len()
    }

    /// Convenience: draw sprite `name` into `dst`.
    pub fn draw(&self, painter: &mut Painter, surface: &mut Surface, name: &str, dst: Rect) {
        if let Some(src) = self.region(name) {
            painter.draw_image_region(surface, self.image(), src, dst);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_image(w: usize, h: usize, rgba: [u8; 4]) -> Image {
        let flat: Vec<u8> = rgba.iter().cycle().take(w * h * 4).copied().collect();
        assert!(Image::new(w, h, flat.clone()).is_some());
        Image::new(w, h, flat).unwrap_or_else(|| Image {
            width: 1,
            height: 1,
            data: vec![rgba[0], rgba[1], rgba[2], rgba[3]],
        })
    }

    #[test]
    fn pack_places_sprites_without_overlap() {
        let Some(mut atlas) = SpriteAtlas::new(64, 32) else {
            return;
        };
        let a = solid_image(16, 16, [255, 0, 0, 255]);
        let b = solid_image(16, 16, [0, 255, 0, 255]);
        let ra = atlas.pack("red", &a);
        let rb = atlas.pack("green", &b);
        assert!(ra.is_some() && rb.is_some(), "sprites must pack");
        let empty = Rect::from_size(Vec2::ZERO, Vec2::ZERO);
        let ra = ra.unwrap_or(empty);
        let rb = rb.unwrap_or(empty);
        // Shelf layout: side by side.
        assert_eq!(ra.x0, 0.0);
        assert_eq!(rb.x0, 17.0, "gutter separates sprites");
        assert_eq!(ra.y0, 0.0);
        assert_eq!(rb.y0, 0.0);
    }

    #[test]
    fn shelf_wraps_at_width() {
        let Some(mut atlas) = SpriteAtlas::new(64, 64) else {
            return;
        };
        let row = solid_image(24, 10, [0, 0, 255, 255]);
        let _ = atlas.pack("a", &row);
        let _ = atlas.pack("b", &row);
        let rc = atlas.pack("c", &row);
        assert!(rc.is_some(), "third sprite must wrap to shelf 2");
        let rc = rc.unwrap_or(Rect::from_size(Vec2::ZERO, Vec2::ZERO));
        assert_eq!(rc.y0, 11.0, "wrapped onto shelf 2 (10 + 1 gutter)");
        assert_eq!(rc.x0, 0.0);
    }

    #[test]
    fn out_of_space_is_none() {
        let Some(mut atlas) = SpriteAtlas::new(16, 16) else {
            return;
        };
        let big = solid_image(32, 32, [255, 255, 255, 255]);
        assert!(atlas.pack("too-big", &big).is_none());
        assert_eq!(atlas.sprite_count(), 0, "failed pack leaves nothing");
    }

    #[test]
    fn duplicate_name_rejected() {
        let Some(mut atlas) = SpriteAtlas::new(64, 64) else {
            return;
        };
        let s = solid_image(8, 8, [255, 0, 0, 255]);
        assert!(atlas.pack("dup", &s).is_some());
        assert!(atlas.pack("dup", &s).is_none(), "duplicate name");
    }

    #[test]
    fn draw_region_blits_correct_pixels() {
        let Some(mut atlas) = SpriteAtlas::new(32, 32) else {
            return;
        };
        let red = solid_image(8, 8, [200, 0, 0, 255]);
        let _ = atlas.pack("red", &red);
        let Some(mut surface) = Surface::new(32, 32) else {
            return;
        };
        let mut painter = Painter::default();
        atlas.draw(
            &mut painter,
            &mut surface,
            "red",
            Rect::from_size(Vec2::new(0.0, 0.0), Vec2::new(8.0, 8.0)),
        );
        // Sample the center of the drawn region: should be red.
        let px = surface.data().get((4 * 32 + 4) * 4);
        assert_eq!(px, Some(&200), "red channel present at center");
    }

    #[test]
    fn draw_image_region_uses_source_rect() {
        // Half red, half blue image; draw only the blue half.
        let mut data = vec![0_u8; 8 * 4 * 4];
        for row in 0..4_usize {
            for col in 0..8_usize {
                let base = (row * 8 + col) * 4;
                let bytes = if col < 4 {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 255, 255]
                };
                if let Some(slot) = data.get_mut(base..base + 4) {
                    slot.copy_from_slice(&bytes);
                }
            }
        }
        assert!(Image::new(8, 4, data.clone()).is_some());
        let img = Image::new(8, 4, data).unwrap_or_else(|| Image {
            width: 1,
            height: 1,
            data: vec![0, 0, 0, 0],
        });
        let Some(mut surface) = Surface::new(8, 4) else {
            return;
        };
        let mut painter = Painter::default();
        let blue_half = Rect::from_size(Vec2::new(4.0, 0.0), Vec2::new(4.0, 4.0));
        painter.draw_image_region(
            &mut surface,
            &img,
            blue_half,
            Rect::from_size(Vec2::new(0.0, 0.0), Vec2::new(8.0, 4.0)),
        );
        // All drawn pixels should be blue-ish (b > r).
        let ok = surface.data().as_chunks::<4>().0.iter().all(|px| {
            let [r, _, b, _] = *px;
            b > r || (r == 0 && b == 0)
        });
        assert!(ok, "only the blue source half was drawn");
    }

    #[test]
    fn image_sample_edge_uv_returns_last_texel() {
        // REGRESSION: u=1.0 clamped to index `width` (out of bounds)
        // returned transparent black instead of the last texel.
        let img = solid_image(4, 4, [30, 60, 90, 255]);
        let edge = img.sample(1.0, 1.0);
        assert_eq!(edge, [30, 60, 90, 255], "corner sample is the last texel");
        let mid = img.sample(0.5, 0.5);
        assert_eq!(mid, [30, 60, 90, 255], "uniform image: any texel matches");
    }
}

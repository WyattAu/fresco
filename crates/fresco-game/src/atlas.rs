//! The game's sprite atlas: procedurally generated, deterministic.
//!
//! Six shaded ball sprites (one per palette color, radial shading
//! with an off-center highlight) plus a white player ring, packed
//! into one image via paint-kit's shelf packer. Pure arithmetic - no
//! environment state, byte-identical on every call - so the sprite
//! path is replay-safe like the rest of the sim.

use paint_kit::atlas::SpriteAtlas;
use paint_kit::{Image, Solid};

/// Sprite side in px (ball sprites are drawn 2x the logical ball
/// radius for a little headroom).
const S: usize = 32;

/// Ball radius inside the sprite.
const R: f64 = 14.0;

/// Highlight center offset (top-left lit).
const HX: f64 = 11.0;
const HY: f64 = 11.0;

/// Palette, same order as `game::PALETTE`.
const COLORS: [[u8; 4]; 6] = [
    [120, 170, 255, 255],
    [140, 220, 160, 255],
    [255, 200, 120, 255],
    [220, 140, 200, 255],
    [160, 220, 220, 255],
    [230, 230, 160, 255],
];

/// One shaded ball sprite: opaque disc, radial shade, top-left
/// highlight. Premultiplied bytes (alpha 255 inside = premul identity).
fn ball_sprite(base: [u8; 4]) -> Option<Image> {
    let mut data = Vec::with_capacity(S * S * 4);
    for y in 0..S {
        for x in 0..S {
            let xf = f64::from(u32::try_from(x).unwrap_or(0)) + 0.5;
            let yf = f64::from(u32::try_from(y).unwrap_or(0)) + 0.5;
            let d = ((xf - 16.0).powi(2) + (yf - 16.0).powi(2)).sqrt();
            if d > R {
                data.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            let shade = 1.0 - 0.45 * (d / R) * (d / R);
            let hd = ((xf - HX).powi(2) + (yf - HY).powi(2)).sqrt();
            let hl = ((1.0 - hd / 7.0).clamp(0.0, 1.0)) * 0.55;
            let [br, bg, bb, _ba] = base; // alpha forced opaque
            let lit = |v: u8| (f64::from(v) * shade + 255.0 * hl).round().min(255.0) as u8;
            let px = [lit(br), lit(bg), lit(bb), 255];
            data.extend_from_slice(&px);
        }
    }
    Image::new(S, S, data)
}

/// The player marker: white ring, hollow center so the ball color
/// shows through logic-wise distinct from fountain balls.
fn player_sprite() -> Option<Image> {
    let mut data = Vec::with_capacity(S * S * 4);
    for y in 0..S {
        for x in 0..S {
            let xf = f64::from(u32::try_from(x).unwrap_or(0)) + 0.5;
            let yf = f64::from(u32::try_from(y).unwrap_or(0)) + 0.5;
            let d = ((xf - 16.0).powi(2) + (yf - 16.0).powi(2)).sqrt();
            if !(R - 5.0..=R).contains(&d) {
                data.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            let shade = 1.0 - 0.3 * ((d - (R - 5.0)) / 5.0);
            data.extend_from_slice(&[
                (255.0 * shade).round() as u8,
                (255.0 * shade).round() as u8,
                (255.0 * shade).round() as u8,
                255,
            ]);
        }
    }
    Image::new(S, S, data)
}

/// The target marker: amber ring, hollow center.
fn target_sprite() -> Option<Image> {
    let mut data = Vec::with_capacity(S * S * 4);
    for y in 0..S {
        for x in 0..S {
            let xf = f64::from(u32::try_from(x).unwrap_or(0)) + 0.5;
            let yf = f64::from(u32::try_from(y).unwrap_or(0)) + 0.5;
            let d = ((xf - 16.0).powi(2) + (yf - 16.0).powi(2)).sqrt();
            if !(R - 6.0..=R).contains(&d) {
                data.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            data.extend_from_slice(&[255, 170, 60, 255]);
        }
    }
    Image::new(S, S, data)
}

/// Build the game atlas: "ball0".."ball5" per palette color plus
/// "player" and "target". Returns `None` only on packer overflow
/// (fixed sprite count in a fixed 128x128 atlas cannot overflow).
#[must_use]
pub fn game_atlas() -> Option<SpriteAtlas> {
    let mut atlas = SpriteAtlas::new(128, 128)?;
    for (i, color) in COLORS.iter().enumerate() {
        let sprite = ball_sprite(*color)?;
        let name = format!("ball{i}");
        atlas.pack(&name, &sprite)?;
    }
    atlas.pack("player", &player_sprite()?)?;
    let target = target_sprite()?;
    atlas.pack("target", &target)?;
    Some(atlas)
}

/// The atlas color for tile `i` (the ECS `Tint` component mirrors the
/// sprite palette for HUD/fallback paths).
#[must_use]
pub fn tile_color(i: u8) -> [u8; 4] {
    let idx = usize::from(i) % COLORS.len();
    COLORS.get(idx).copied().unwrap_or([120, 170, 255, 255])
}

/// The solid paint color helper mirrors `tile_color` for callers that
/// fill instead of sprite-draw.
#[must_use]
pub fn tile_solid(i: u8) -> Solid {
    Solid::premul(tile_color(i))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the atlas is deterministic - two builds are
    /// byte-identical (replay safety of the render path).
    #[test]
    fn atlas_is_deterministic() {
        let a = game_atlas();
        let b = game_atlas();
        assert!(a.is_some() && b.is_some(), "atlas build must succeed");
        if let (Some(a), Some(b)) = (a, b) {
            assert_eq!(a.image().data, b.image().data, "atlas bytes differ");
            assert_eq!(a.sprite_count(), 8, "6 balls + player + target");
        }
    }

    /// Oracle: every named region packs and lies inside the atlas.
    #[test]
    fn regions_are_packed() {
        assert!(game_atlas().is_some(), "atlas build must succeed");
        let Some(atlas) = game_atlas() else {
            return;
        };
        for i in 0..6u8 {
            let name = format!("ball{i}");
            assert!(atlas.region(&name).is_some(), "{name} missing");
        }
        assert!(atlas.region("player").is_some(), "player missing");
    }
}

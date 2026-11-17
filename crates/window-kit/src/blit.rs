//! Softbuffer blit: paint-kit logical surface -> physical framebuffer
//! upscale, shared by both windowed shells (fresco-android, fresco-game).
//!
//! Fast paths:
//! - integer scale: every logical pixel maps to a `k x m` physical
//!   block - one row of u32 expands, then `copy_from_slice` repeats it
//!   (memcpy speed); ~2.6 M px at 3x lands in ~2 ms instead of ~25 ms
//!   of per-pixel float math on a mid-range phone.
//! - otherwise: the general nearest-neighbor formula per pixel.

use paint_kit::Surface;

/// Blit the logical surface into the physical framebuffer with
/// nearest-neighbor upscaling (`0xAABBGGRR` u32 targets).
pub fn blit_upscaled(scene: &Surface, buffer: &mut [u32], phys_w: usize, phys_h: usize) {
    let lw = scene.width();
    let lh = scene.height();
    if lw == 0 || lh == 0 || phys_w == 0 || phys_h == 0 || buffer.is_empty() {
        return;
    }
    let data = scene.data();
    let k = phys_w / lw; // integer horizontal scale (0 when fractional)
    let m = phys_h / lh; // integer vertical scale
    if k >= 1 && m >= 1 && lw * k == phys_w && lh * m == phys_h {
        // Integer scale: expand + row repeat.
        for ly in 0..lh {
            let phys_row0 = ly * m;
            // One physical row of the block: each logical pixel
            // repeated k times.
            let row_start = phys_row0 * phys_w;
            let row_end = row_start + phys_w;
            let Some(dst_row) = buffer.get_mut(row_start..row_end) else {
                continue;
            };
            for lx in 0..lw {
                let base = (ly * lw + lx) * 4;
                let Some(q) = data.get(base..base + 4) else {
                    break;
                };
                let Ok([r, g, b, a]) = <[u8; 4]>::try_from(q) else {
                    break;
                };
                let px = u32::from(a) << 24 | u32::from(b) << 16 | u32::from(g) << 8 | u32::from(r);
                let start = lx * k;
                let end = start + k;
                for slot in dst_row.get_mut(start..end).into_iter().flatten() {
                    *slot = px;
                }
            }
            // Repeat the row for the remaining m-1 physical rows
            // (disjoint ranges: copy_within is memcpy-fast).
            let src_start = phys_row0 * phys_w;
            for rep in 1..m {
                let dst0 = (phys_row0 + rep) * phys_w;
                buffer.copy_within(src_start..src_start + phys_w, dst0);
            }
        }
        return;
    }
    // General nearest-neighbor (fractional scale).
    let bwidth = phys_w as f64;
    let bheight = phys_h as f64;
    let lwf = lw as f64;
    let lhf = lh as f64;
    let total = phys_w.saturating_mul(phys_h);
    for p in 0..total {
        let col = p % phys_w;
        let row = p / phys_w;
        let sx = ((col as f64) * lwf / bwidth).floor().max(0.0) as usize;
        let sy = ((row as f64) * lhf / bheight).floor().max(0.0) as usize;
        let base = (sy.min(lh - 1) * lw + sx.min(lw - 1)) * 4;
        let Some(q) = data.get(base..base + 4) else {
            break;
        };
        let Ok([r, g, b, a]) = <[u8; 4]>::try_from(q) else {
            break;
        };
        if let Some(px) = buffer.get_mut(p) {
            *px = u32::from(a) << 24 | u32::from(b) << 16 | u32::from(g) << 8 | u32::from(r);
        }
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn surface_of(pixels: &[(u8, u8, u8)]) -> Surface {
        let mut s = Surface::new(3, 2).unwrap_or_default();
        for (i, (r, g, b)) in pixels.iter().enumerate() {
            s.blend_pixel(i % 3, i / 3, [*r, *g, *b, 255]);
        }
        s
    }

    /// Oracle: integer upscale reproduces the naive nearest-neighbor
    /// mapping exactly (every physical pixel samples its logical cell).
    #[test]
    fn integer_upscale_matches_naive_mapping() {
        let s = surface_of(&[
            (255, 0, 0),
            (0, 255, 0),
            (0, 0, 255),
            (9, 9, 9),
            (1, 1, 1),
            (2, 2, 2),
        ]);
        let mut buf = vec![0_u32; 9 * 6];
        blit_upscaled(&s, &mut buf, 9, 6);
        // Naive reference.
        let mut want = vec![0_u32; 9 * 6];
        let data = s.data();
        for row in 0..6usize {
            for col in 0..9usize {
                let sx = ((col as f64) * 3.0 / 9.0).floor() as usize;
                let sy = ((row as f64) * 2.0 / 6.0).floor() as usize;
                let base = (sy * 3 + sx) * 4;
                let q = &data[base..base + 4];
                want[row * 9 + col] = u32::from(q[3]) << 24
                    | u32::from(q[2]) << 16
                    | u32::from(q[1]) << 8
                    | u32::from(q[0]);
            }
        }
        assert_eq!(buf, want, "integer upscale must match naive mapping");
    }

    /// Oracle: fractional scale falls back to the general path and
    /// still samples inside the surface.
    #[test]
    fn fractional_scale_stays_in_bounds() {
        let s = surface_of(&[(10, 20, 30); 6]);
        let mut buf = vec![0_u32; 7 * 5];
        blit_upscaled(&s, &mut buf, 7, 5);
        assert!(buf.iter().all(|&px| px != 0), "every pixel blended");
    }
}

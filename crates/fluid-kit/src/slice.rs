//! Cut-plane slice mapping for field visualization.
//!
//! Maps the pixels of a 2D viewport onto grid coordinates of a
//! slice plane (axis-aligned, at a fixed cell index). Pure math —
//! render layers map the returned grid coordinates through
//! `Field::sample` / `VelocityField::sample` themselves.

/// The axis the slice plane faces (the fixed axis).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SliceAxis {
    /// Plane at fixed X; pixels map to (Z, Y).
    X,
    /// Plane at fixed Y (top-down); pixels map to (X, Z).
    Y,
    /// Plane at fixed Z (front view); pixels map to (X, Y).
    Z,
}

/// Grid coordinates `(gx, gy, gz)` for pixel `(px, py)` of a
/// `width x height` viewport onto the slice `axis` at cell
/// `index` of a `grid_dims` field. Pixel rows run top-to-bottom;
/// the two varying axes map to `(left->right, bottom->top)` so
/// the image is not upside down relative to the grid. All
/// returned coordinates stay within `[0, dim-1]`.
#[must_use]
pub fn slice_grid_coords(
    axis: SliceAxis,
    index: usize,
    grid_dims: (usize, usize, usize),
    width: usize,
    height: usize,
    px: usize,
    py: usize,
) -> (f64, f64, f64) {
    let (dx, dy, dz) = grid_dims;
    let w = width.max(1) as f64;
    let h = height.max(1) as f64;
    let u = (px as f64 + 0.5) / w; // left -> right
    let v = 1.0 - (py as f64 + 0.5) / h; // bottom -> top
    let f = |i: usize| i as f64;
    let span = |d: usize| f(d.saturating_sub(1));
    match axis {
        SliceAxis::X => (
            f(index.min(dx.saturating_sub(1))),
            u * span(dy),
            v * span(dz),
        ),
        SliceAxis::Y => (
            u * span(dx),
            f(index.min(dy.saturating_sub(1))),
            v * span(dz),
        ),
        SliceAxis::Z => (
            u * span(dx),
            v * span(dy),
            f(index.min(dz.saturating_sub(1))),
        ),
    }
}

/// Color ramp: map `t` in `[0, 1]` through a blue->cyan->yellow->
/// red heat scale (an approximation of scientific colormaps).
#[must_use]
pub fn heat_ramp(t: f64) -> [u8; 4] {
    let t = t.clamp(0.0, 1.0);
    // Piecewise: blue (0) -> cyan (0.33) -> yellow (0.66) -> red (1).
    let seg = t * 3.0;
    let (r, g, b) = match seg as usize {
        0 => {
            let f = seg;
            (f * 0.0, 30.0 + f * 195.0, 255.0 - f * 155.0)
        }
        1 => {
            let f = seg - 1.0;
            (f * 225.0, 225.0, 100.0 - f * 90.0)
        }
        _ => {
            let f = seg - 2.0;
            (225.0 + f * 30.0, 225.0 - f * 205.0, 10.0)
        }
    };
    [
        r.round().clamp(0.0, 255.0) as u8,
        g.round().clamp(0.0, 255.0) as u8,
        b.round().clamp(0.0, 255.0) as u8,
        255,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice_axis_z_maps_u_to_x_v_to_y() {
        // 5x5 viewport on the Z=2 plane: corners map to grid corners.
        let (gx0, gy0, _) = slice_grid_coords(SliceAxis::Z, 2, (5, 5, 5), 5, 5, 0, 4);
        assert!(
            gx0.abs() < 0.6 && gy0.abs() < 0.6,
            "bottom-left: ({gx0},{gy0})"
        );
        let (gx1, gy1, gz) = slice_grid_coords(SliceAxis::Z, 2, (5, 5, 5), 5, 5, 4, 0);
        assert!(
            (gx1 - 4.0).abs() < 0.6 && (gy1 - 4.0).abs() < 0.6,
            "top-right: ({gx1},{gy1})"
        );
        assert_eq!(gz, 2.0, "fixed axis preserved");
    }

    #[test]
    fn slice_axis_y_is_top_down() {
        // Y slice: u -> x, v -> z.
        let (gx, _, gz) = slice_grid_coords(SliceAxis::Y, 3, (5, 5, 5), 5, 5, 4, 4);
        assert!((gx - 4.0).abs() < 0.6 && gz.abs() < 0.6, "({gx},{gz})");
        assert_eq!(
            slice_grid_coords(SliceAxis::Y, 3, (5, 5, 5), 5, 5, 0, 0).1,
            3.0,
            "fixed y"
        );
    }

    #[test]
    fn heat_ramp_endpoints() {
        let lo = heat_ramp(0.0);
        let hi = heat_ramp(1.0);
        assert!(lo[2] > lo[0], "cold end is blue-ish: {lo:?}");
        assert!(hi[0] > hi[2], "hot end is red-ish: {hi:?}");
        assert_eq!(heat_ramp(-5.0), heat_ramp(0.0), "clamped below");
        assert_eq!(heat_ramp(5.0), heat_ramp(1.0), "clamped above");
    }

    #[test]
    fn slice_grid_coords_always_in_bounds() {
        // Any pixel of any axis maps inside the 16-cube.
        for axis in [SliceAxis::X, SliceAxis::Y, SliceAxis::Z] {
            for px in 0..24 {
                for py in (0..24).step_by(6) {
                    let (gx, gy, gz) = slice_grid_coords(axis, 8, (16, 16, 16), 40, 40, px, py);
                    assert!((0.0..=15.0).contains(&gx));
                    assert!((0.0..=15.0).contains(&gy));
                    assert!((0.0..=15.0).contains(&gz));
                }
            }
        }
    }
}

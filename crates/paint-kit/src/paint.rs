//! Fill paint sources: solids and gradients.
//!
//! Gradient colors are evaluated per pixel in device space (the gradient
//! geometry is device-space in v0; scene-space gradients arrive with the
//! scene transform pipeline). Interpolation is premultiplied lerp, which
//! is the correct over-compositing algebra for premultiplied inputs.

use crate::Solid;
use path_kit::Vec2;

/// One gradient stop: offset in `[0, 1]` plus a premultiplied color.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorStop {
    /// Position along the gradient axis.
    pub offset: f32,
    /// Premultiplied sRGBA color.
    pub color: [u8; 4],
}

/// Fill paint.
#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    /// Uniform color.
    Solid(Solid),
    /// Linear gradient from `start` to `end` in device pixels.
    Linear(LinearGradient),
    /// Radial gradient centered at `center` with `radius` device pixels.
    Radial(RadialGradient),
}

impl From<Solid> for Paint {
    fn from(solid: Solid) -> Paint {
        Paint::Solid(solid)
    }
}

impl From<LinearGradient> for Paint {
    fn from(g: LinearGradient) -> Paint {
        Paint::Linear(g)
    }
}

impl From<RadialGradient> for Paint {
    fn from(g: RadialGradient) -> Paint {
        Paint::Radial(g)
    }
}

/// Linear gradient.
#[derive(Clone, Debug, PartialEq)]
pub struct LinearGradient {
    /// Start point.
    pub start: Vec2,
    /// End point.
    pub end: Vec2,
    /// Stops, any order; sorted on construction through
    /// [`LinearGradient::new`].
    pub stops: Vec<ColorStop>,
}

/// Radial gradient.
#[derive(Clone, Debug, PartialEq)]
pub struct RadialGradient {
    /// Center.
    pub center: Vec2,
    /// Radius in pixels; zero radius clamps to the first stop.
    pub radius: f64,
    /// Stops, any order; sorted on construction through
    /// [`RadialGradient::new`].
    pub stops: Vec<ColorStop>,
}

impl LinearGradient {
    /// New gradient with stops sorted by offset.
    #[must_use]
    pub fn new(start: Vec2, end: Vec2, mut stops: Vec<ColorStop>) -> Self {
        stops.sort_by(|a, b| {
            a.offset
                .partial_cmp(&b.offset)
                .unwrap_or(core::cmp::Ordering::Equal)
        });
        LinearGradient { start, end, stops }
    }
}

impl RadialGradient {
    /// New gradient with stops sorted by offset.
    #[must_use]
    pub fn new(center: Vec2, radius: f64, mut stops: Vec<ColorStop>) -> Self {
        stops.sort_by(|a, b| {
            a.offset
                .partial_cmp(&b.offset)
                .unwrap_or(core::cmp::Ordering::Equal)
        });
        RadialGradient {
            center,
            radius: radius.max(0.0),
            stops,
        }
    }
}

/// Evaluate stop interpolation at `t` in `[0, 1]`; premultiplied lerp.
#[must_use]
pub fn eval_stops(stops: &[ColorStop], t: f64) -> [u8; 4] {
    if stops.is_empty() {
        return [0, 0, 0, 0];
    }
    let t = t.clamp(0.0, 1.0);
    let mut lo = stops.first().copied().unwrap_or(ColorStop {
        offset: 0.0,
        color: [0; 4],
    });
    let mut hi = lo;
    for stop in stops {
        if stop.offset as f64 <= t {
            lo = *stop;
        }
    }
    for stop in stops.iter().rev() {
        if (stop.offset as f64) >= t {
            hi = *stop;
        }
    }
    if hi.offset <= lo.offset || (hi.offset as f64 - lo.offset as f64) == 0.0 {
        // Order matters: prefer the later stop at exact offsets.
        if t >= hi.offset as f64 {
            return hi.color;
        }
        return lo.color;
    }
    let span = hi.offset as f64 - lo.offset as f64;
    let frac = ((t - lo.offset as f64) / span).clamp(0.0, 1.0);
    let mut out = [0_u8; 4];
    for (k, pair) in out.iter_mut().zip(lo.color).enumerate() {
        let (slot, a) = pair;
        let b = f64::from(*hi.color.get(k).unwrap_or(&0));
        let a = f64::from(a);
        *slot = (a + (b - a) * frac).round() as u8;
    }
    out
}

impl Paint {
    /// Sample the paint at a device pixel center.
    #[must_use]
    pub fn eval(&self, x: f64, y: f64) -> [u8; 4] {
        match self {
            Paint::Solid(s) => s.color,
            Paint::Linear(g) => {
                let dx = g.end.x - g.start.x;
                let dy = g.end.y - g.start.y;
                let len2 = dx * dx + dy * dy;
                let t = if len2 <= 1e-12 {
                    0.0
                } else {
                    ((x - g.start.x) * dx + (y - g.start.y) * dy) / len2
                };
                eval_stops(&g.stops, t)
            }
            Paint::Radial(g) => {
                if g.radius <= 0.0 {
                    return g.stops.first().map(|s| s.color).unwrap_or([0, 0, 0, 0]);
                }
                let dx = x - g.center.x;
                let dy = y - g.center.y;
                let t = (dx * dx + dy * dy).sqrt() / g.radius;
                eval_stops(&g.stops, t)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stops_sorted_and_interpolated() {
        let g = LinearGradient::new(
            Vec2::ZERO,
            Vec2::new(100.0, 0.0),
            vec![
                ColorStop {
                    offset: 1.0,
                    color: [0, 0, 200, 255],
                },
                ColorStop {
                    offset: 0.0,
                    color: [200, 0, 0, 255],
                },
            ],
        );
        assert_eq!(g.stops.first().map(|s| s.offset), Some(0.0));
        let mid = eval_stops(&g.stops, 0.5);
        assert_eq!(mid[0], 100, "red halfway down");
        assert_eq!(mid[2], 100, "blue halfway up");
        let at_start = eval_stops(&g.stops, -1.0);
        assert_eq!(at_start[0], 200);
        let at_end = eval_stops(&g.stops, 2.0);
        assert_eq!(at_end[2], 200);
    }

    #[test]
    fn linear_projection() {
        let paint = Paint::Linear(LinearGradient::new(
            Vec2::ZERO,
            Vec2::new(10.0, 0.0),
            vec![
                ColorStop {
                    offset: 0.0,
                    color: [0, 0, 0, 255],
                },
                ColorStop {
                    offset: 1.0,
                    color: [100, 100, 100, 255],
                },
            ],
        ));
        assert_eq!(paint.eval(0.0, 7.0)[0], 0, "projected t ignores y");
        assert_eq!(paint.eval(5.0, 99.0)[0], 50);
        assert_eq!(paint.eval(10.0, 0.0)[0], 100);
    }

    #[test]
    fn radial_distance() {
        let paint = Paint::Radial(RadialGradient::new(
            Vec2::new(10.0, 10.0),
            10.0,
            vec![
                ColorStop {
                    offset: 0.0,
                    color: [0, 0, 0, 255],
                },
                ColorStop {
                    offset: 1.0,
                    color: [100, 100, 100, 255],
                },
            ],
        ));
        assert_eq!(paint.eval(10.0, 10.0)[0], 0, "center");
        assert_eq!(paint.eval(20.0, 10.0)[0], 100, "rim");
        let paint_zero = Paint::Radial(RadialGradient::new(
            Vec2::ZERO,
            0.0,
            vec![ColorStop {
                offset: 0.0,
                color: [9, 9, 9, 255],
            }],
        ));
        assert_eq!(paint_zero.eval(5.0, 5.0), [9, 9, 9, 255], "zero radius");
    }

    #[test]
    fn solid_paint_and_empty_stops() {
        let paint = Paint::from(Solid::premul([1, 2, 3, 4]));
        assert_eq!(paint.eval(0.0, 0.0), [1, 2, 3, 4]);
        assert_eq!(eval_stops(&[], 0.5), [0, 0, 0, 0]);
    }
}

//! Path construction and flattening to polylines.

use crate::bez::{CubicBez, QuadBez};
use crate::flatten::{flatten_cubic, flatten_quad};
use crate::geom::{Affine, Rect, Vec2};

/// A flattened subpath: polyline points plus a closed flag.
#[derive(Clone, Debug, PartialEq)]
pub struct Poly {
    /// Polyline vertices; first and last are distinct for closed paths.
    pub points: std::vec::Vec<Vec2>,
    /// Whether the subpath was explicitly closed.
    pub closed: bool,
}

impl Poly {
    /// Signed area (positive for clockwise turns in y-down coordinates).
    #[must_use]
    pub fn signed_area(&self) -> f64 {
        let Some(&last) = self.points.last() else {
            return 0.0;
        };
        let mut sum = 0.0;
        let mut prev = last;
        for p in &self.points {
            sum += prev.cross(*p);
            prev = *p;
        }
        sum * 0.5
    }
}

/// A path segment command.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathSeg {
    /// Start a new subpath.
    MoveTo(Vec2),
    /// Line to a point.
    LineTo(Vec2),
    /// Quadratic curve to `end` with control `ctrl`.
    QuadTo {
        /// Control point.
        ctrl: Vec2,
        /// End point.
        end: Vec2,
    },
    /// Cubic curve to `end` with controls `c1`, `c2`.
    CubicTo {
        /// First control point.
        c1: Vec2,
        /// Second control point.
        c2: Vec2,
        /// End point.
        end: Vec2,
    },
    /// Close the current subpath with a straight line to its start.
    Close,
}

/// An owned, ordered list of path segments.
///
/// A [`PathSeg::MoveTo`] before any other command opens a subpath. If the
/// first segment is not a move, an implicit move to the origin is assumed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    segments: std::vec::Vec<PathSeg>,
}

impl Path {
    /// New empty path.
    #[must_use]
    pub const fn new() -> Self {
        Path {
            segments: std::vec::Vec::new(),
        }
    }

    /// Append a segment.
    pub fn push(&mut self, seg: PathSeg) {
        self.segments.push(seg);
    }

    /// Start a new subpath at `p`.
    pub fn move_to(&mut self, p: Vec2) {
        self.push(PathSeg::MoveTo(p));
    }

    /// Line to `p`.
    pub fn line_to(&mut self, p: Vec2) {
        self.push(PathSeg::LineTo(p));
    }

    /// Quadratic curve.
    pub fn quad_to(&mut self, ctrl: Vec2, end: Vec2) {
        self.push(PathSeg::QuadTo { ctrl, end });
    }

    /// Cubic curve.
    pub fn cubic_to(&mut self, c1: Vec2, c2: Vec2, end: Vec2) {
        self.push(PathSeg::CubicTo { c1, c2, end });
    }

    /// Close the current subpath.
    pub fn close(&mut self) {
        self.push(PathSeg::Close);
    }

    /// True when the path has no segments.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Segment count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.segments.len()
    }

    /// Borrowed segment slice.
    #[must_use]
    pub fn segments(&self) -> &[PathSeg] {
        &self.segments
    }

    /// Drop all segments, retaining capacity.
    pub fn clear(&mut self) {
        self.segments.clear();
    }

    /// Flatten into subpath polylines under tolerance `tol` (REQ-P1).
    #[must_use]
    pub fn flatten(&self, tol: f64) -> std::vec::Vec<Poly> {
        let mut polys: std::vec::Vec<Poly> = std::vec::Vec::new();
        let mut current: std::vec::Vec<Vec2> = std::vec::Vec::new();
        let mut start = Vec2::ZERO;
        let mut cur = Vec2::ZERO;
        // A leading LineTo implies a move from the origin.
        let mut open = true;
        current.push(Vec2::ZERO);

        for seg in &self.segments {
            match *seg {
                PathSeg::MoveTo(p) => {
                    if open && current.len() > 1 {
                        polys.push(Poly {
                            points: core::mem::take(&mut current),
                            closed: false,
                        });
                    }
                    current.clear();
                    current.push(p);
                    start = p;
                    cur = p;
                    open = true;
                }
                PathSeg::LineTo(p) => {
                    current.push(p);
                    cur = p;
                }
                PathSeg::QuadTo { ctrl, end } => {
                    let q = QuadBez {
                        p0: cur,
                        p1: ctrl,
                        p2: end,
                    };
                    flatten_quad(&q, tol, 0, &mut current);
                    cur = end;
                }
                PathSeg::CubicTo { c1, c2, end } => {
                    let c = CubicBez {
                        p0: cur,
                        p1: c1,
                        p2: c2,
                        p3: end,
                    };
                    flatten_cubic(&c, tol, 0, &mut current);
                    cur = end;
                }
                PathSeg::Close => {
                    if open {
                        if cur.distance(start) > 0.0 {
                            current.push(start);
                        }
                        if current.len() > 1 {
                            polys.push(Poly {
                                points: core::mem::take(&mut current),
                                closed: true,
                            });
                        }
                        current.clear();
                        current.push(start);
                        cur = start;
                    }
                }
            }
        }
        if open && current.len() > 1 {
            polys.push(Poly {
                points: current,
                closed: false,
            });
        }
        polys
    }

    /// Conservative bounds: control-point hull, transformed corners exact
    /// for axis-aligned transforms. `None` when empty.
    #[must_use]
    pub fn bounds(&self) -> Option<Rect> {
        fn acc(r: &mut Option<Rect>, p: Vec2) {
            *r = Some(match *r {
                Some(old) => old.union(Rect::from_points(p, p)),
                None => Rect::from_points(p, p),
            });
        }
        let mut r: Option<Rect> = None;
        for seg in &self.segments {
            match *seg {
                PathSeg::MoveTo(p) | PathSeg::LineTo(p) => acc(&mut r, p),
                PathSeg::QuadTo { ctrl, end } => {
                    acc(&mut r, ctrl);
                    acc(&mut r, end);
                }
                PathSeg::CubicTo { c1, c2, end } => {
                    acc(&mut r, c1);
                    acc(&mut r, c2);
                    acc(&mut r, end);
                }
                PathSeg::Close => {}
            }
        }
        r
    }

    /// Return the path with `t` applied to every point.
    #[must_use]
    pub fn transformed(&self, t: Affine) -> Path {
        let map = |p: Vec2| t.apply(p);
        Path {
            segments: self
                .segments
                .iter()
                .map(|seg| match *seg {
                    PathSeg::MoveTo(p) => PathSeg::MoveTo(map(p)),
                    PathSeg::LineTo(p) => PathSeg::LineTo(map(p)),
                    PathSeg::QuadTo { ctrl, end } => PathSeg::QuadTo {
                        ctrl: map(ctrl),
                        end: map(end),
                    },
                    PathSeg::CubicTo { c1, c2, end } => PathSeg::CubicTo {
                        c1: map(c1),
                        c2: map(c2),
                        end: map(end),
                    },
                    PathSeg::Close => PathSeg::Close,
                })
                .collect(),
        }
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn flatten_closed_triangle() {
        let mut p = Path::new();
        p.move_to(Vec2::new(0.0, 0.0));
        p.line_to(Vec2::new(10.0, 0.0));
        p.line_to(Vec2::new(5.0, 10.0));
        p.close();
        let polys = p.flatten(0.1);
        assert_eq!(polys.len(), 1);
        let empty = Poly {
            points: std::vec::Vec::new(),
            closed: false,
        };
        let poly = polys.first().unwrap_or(&empty);
        assert!(poly.closed);
        assert_eq!(poly.points.len(), 4); // 3 vertices + repeat of start
        assert!((poly.signed_area().abs() - 50.0).abs() < 1e-9);
    }

    #[test]
    fn flatten_multiple_subpaths() {
        let mut p = Path::new();
        p.move_to(Vec2::ZERO);
        p.line_to(Vec2::new(1.0, 0.0));
        p.move_to(Vec2::new(10.0, 0.0));
        p.line_to(Vec2::new(11.0, 0.0));
        p.close();
        let polys = p.flatten(0.1);
        assert_eq!(polys.len(), 2);
        let empty = Poly {
            points: std::vec::Vec::new(),
            closed: false,
        };
        let first = polys.first().unwrap_or(&empty);
        assert!(!first.closed);
    }

    #[test]
    fn bounds_and_transform() {
        let mut p = Path::new();
        p.move_to(Vec2::ZERO);
        p.cubic_to(
            Vec2::new(-5.0, 5.0),
            Vec2::new(5.0, 5.0),
            Vec2::new(10.0, 0.0),
        );
        let b = p.bounds().unwrap_or(Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 0.0,
            y1: 0.0,
        });
        assert!(b.x0 <= -5.0 && b.x1 >= 10.0);
        let shifted = p.transformed(Affine::translate(Vec2::new(100.0, 0.0)));
        let b2 = shifted.bounds().unwrap_or(b);
        assert!(b2.x0 >= 95.0 - 1e-9);
    }

    #[test]
    fn implicit_move_on_leading_line() {
        let mut p = Path::new();
        p.line_to(Vec2::new(1.0, 1.0)); // no MoveTo first
        let polys = p.flatten(0.1);
        assert_eq!(polys.len(), 1);
    }

    #[test]
    fn empty_path_flattens_to_nothing() {
        assert!(Path::new().flatten(0.1).is_empty());
        assert!(Path::new().bounds().is_none());
    }
}

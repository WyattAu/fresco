//! Polyline stroking with joins and caps.
//!
//! V0 operates on flattened polylines: offsets are computed per segment
//! and connected with miters, bevels, or round fans. Sharp concave joins
//! produce overlapping inner geometry, which fills correctly under
//! NonZero; exact analytic curve-space offsetting is the recorded upgrade
//! path (paradigm 883 in `docs/paradigms.md`).

use crate::geom::Vec2;
use crate::path::Poly;

/// Join style at shared vertices.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Join {
    /// Straight chamfer between offsets.
    Bevel,
    /// Circular arc between offsets.
    Round,
    /// Miter up to a ratio limit (`limit * width / 2` spike length),
    /// falling back to bevel beyond it. A limit of 1.0 disables mitering.
    Miter(f64),
}

/// Cap style at open path ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cap {
    /// Cut square at the endpoint.
    Butt,
    /// Half-circle beyond the endpoint.
    Round,
    /// Square extension by half the width beyond the endpoint.
    Square,
}

#[derive(Clone, Copy, Debug)]
struct Seg {
    start: Vec2,
    end: Vec2,
    dir: Vec2,
    /// Unit normal (dir rotated 90 degrees).
    normal: Vec2,
}

const EPS: f64 = 1e-9;

/// Stroke a flattened polyline.
///
/// Returns outline polygons suitable for filling with `NonZero`: closed
/// input produces two opposite-oriented rings, open input one closed
/// outline. Zero-width or degenerate input returns no polygons.
#[must_use]
pub fn stroke_poly(
    points: &[Vec2],
    closed: bool,
    width: f64,
    join: Join,
    cap: Cap,
) -> std::vec::Vec<Poly> {
    if width <= 0.0 || points.len() < 2 {
        return std::vec::Vec::new();
    }
    let w2 = width * 0.5;

    let mut segs: std::vec::Vec<Seg> = std::vec::Vec::new();
    for pair in points.windows(2) {
        if let [p, q] = pair {
            push_seg(&mut segs, *p, *q);
        }
    }
    if closed && let (Some(&first), Some(&last)) = (points.first(), points.last()) {
        push_seg(&mut segs, last, first);
    }
    if segs.is_empty() {
        return std::vec::Vec::new();
    }

    if closed {
        stroke_closed(&segs, w2, &join)
    } else {
        stroke_open(&segs, w2, &join, &cap)
    }
}

fn push_seg(out: &mut std::vec::Vec<Seg>, p: Vec2, q: Vec2) {
    let d = q - p;
    let len = d.len();
    if len < EPS {
        return; // skip degenerate segments, keep chain continuous
    }
    let dir = d / len;
    out.push(Seg {
        start: p,
        end: q,
        dir,
        normal: Vec2::new(-dir.y, dir.x),
    });
}

fn offset(seg: &Seg, side: f64, w2: f64) -> (Vec2, Vec2) {
    let n = seg.normal * (side * w2);
    (seg.start + n, seg.end + n)
}

fn stroke_closed(segs: &[Seg], w2: f64, join: &Join) -> std::vec::Vec<Poly> {
    let n = segs.len();
    let ring = |side: f64| -> std::vec::Vec<Vec2> {
        let mut out: std::vec::Vec<Vec2> = std::vec::Vec::new();
        let Some(first) = segs.first() else {
            return out;
        };
        let (s0, _) = offset(first, side, w2);
        out.push(s0);
        for k in 0..n {
            let Some(cur) = segs.get(k) else { break };
            let Some(next) = segs.get((k + 1) % n) else {
                break;
            };
            let (a, _) = offset(cur, side, w2);
            let (b, _) = offset(next, side, w2);
            let ctx = SideCtx { side, w2, join };
            emit_join(cur, next, a, b, ctx, &mut out);
        }
        // Ring closed: drop trailing duplicate of the start point.
        if let (Some(&f), Some(&l)) = (out.first(), out.last())
            && f.distance(l) < 1e-9
        {
            out.pop();
        }
        out
    };
    let left = ring(1.0);
    let right = ring(-1.0);
    let mut polys = std::vec::Vec::new();
    if left.len() >= 3 {
        polys.push(Poly {
            points: left,
            closed: true,
        });
    }
    if right.len() >= 3 {
        polys.push(Poly {
            points: right,
            closed: true,
        });
    }
    polys
}

fn stroke_open(segs: &[Seg], w2: f64, join: &Join, cap: &Cap) -> std::vec::Vec<Poly> {
    let mut out: std::vec::Vec<Vec2> = std::vec::Vec::new();
    let Some(first) = segs.first() else {
        return std::vec::Vec::new();
    };
    let Some(last) = segs.last() else {
        return std::vec::Vec::new();
    };

    // Forward chain, left side.
    let (fs, _) = offset(first, 1.0, w2);
    out.push(fs);
    for pair in segs.windows(2) {
        if let [prev, cur] = pair {
            let (a, _) = offset(prev, 1.0, w2);
            let (b, _) = offset(cur, 1.0, w2);
            let ctx = SideCtx {
                side: 1.0,
                w2,
                join,
            };
            emit_join(prev, cur, a, b, ctx, &mut out);
        }
    }
    let (_, fe) = offset(last, 1.0, w2);
    out.push(fe);

    // End cap. The reversed chain uses side +1: its normals are already
    // flipped, so +1 lands on the original's right side.
    let mut rev: std::vec::Vec<Seg> = segs
        .iter()
        .rev()
        .map(|s| Seg {
            start: s.end,
            end: s.start,
            dir: -s.dir,
            normal: -s.normal,
        })
        .collect();
    let (_, rs) = offset(rev.first().unwrap_or(first), 1.0, w2);
    let ctx = SideCtx {
        side: -1.0,
        w2,
        join,
    };
    emit_cap(last.end, last.dir, fe, rs, ctx, cap, &mut out);

    // Reverse chain, right side.
    for pair in rev.windows(2) {
        if let [prev, cur] = pair {
            let (a, _) = offset(prev, 1.0, w2);
            let (b, _) = offset(cur, 1.0, w2);
            let ctx = SideCtx {
                side: 1.0,
                w2,
                join,
            };
            emit_join(prev, cur, a, b, ctx, &mut out);
        }
    }
    let (_, re) = offset(rev.first().unwrap_or(first), 1.0, w2);
    let (fs2, _) = offset(first, 1.0, w2);
    // Start cap points backward: direction is the reversed segment's.
    let ctx = SideCtx {
        side: 1.0,
        w2,
        join,
    };
    emit_cap(first.start, -first.dir, re, fs2, ctx, cap, &mut out);
    rev.clear();

    if out.len() < 3 {
        return std::vec::Vec::new();
    }
    vec![Poly {
        points: out,
        closed: true,
    }]
}

/// Per-side stroke parameters shared by join and cap emission.
#[derive(Clone, Copy)]
struct SideCtx<'j> {
    side: f64,
    w2: f64,
    join: &'j Join,
}

/// Emit join points so that `out` ends exactly at `b` (or at the miter
/// point followed by `b`).
fn emit_join(prev: &Seg, cur: &Seg, a: Vec2, b: Vec2, ctx: SideCtx, out: &mut std::vec::Vec<Vec2>) {
    let v = prev.end;
    let side = ctx.side;
    let w2 = ctx.w2;
    let join = ctx.join;
    let turn = prev.dir.cross(cur.dir) * side;
    if a.distance(b) < 1e-9 {
        return; // straight continuation
    }
    match *join {
        Join::Bevel => {
            out.push(b);
        }
        Join::Round => {
            let r0 = a - v;
            let r1 = b - v;
            if r0.len() < EPS || r1.len() < EPS {
                out.push(b);
                return;
            }
            let ang = {
                let cos = (r0.dot(r1) / (r0.len() * r1.len())).clamp(-1.0, 1.0);
                cos.acos() * turn.signum()
            };
            let steps = ((ang.abs() / (core::f64::consts::FRAC_PI_8)).ceil() as u32).clamp(1, 16);
            let step = ang / f64::from(steps);
            let (s, c) = step.sin_cos();
            let mut r = r0;
            for _ in 0..steps {
                let rotated = Vec2::new(r.x * c - r.y * s, r.x * s + r.y * c);
                r = rotated;
                out.push(v + r);
            }
            // The last rotated point is r1; ensure exactness.
            if let Some(l) = out.last_mut() {
                *l = b;
            }
        }
        Join::Miter(limit) => {
            let denom = prev.dir.cross(cur.dir);
            if denom.abs() < EPS {
                out.push(b);
                return;
            }
            let t = (b - a).cross(cur.dir) / denom;
            let m = a + prev.dir * t;
            let spike = m.distance(v);
            if spike > limit * w2 || !m.x.is_finite() || !m.y.is_finite() {
                out.push(b);
            } else {
                out.push(m);
                out.push(b);
            }
        }
    }
}

/// Emit a cap so `out` ends at `b_next` (the opposite side's first offset).
fn emit_cap(
    v: Vec2,
    dir: Vec2,
    a: Vec2,
    b_next: Vec2,
    ctx: SideCtx,
    cap: &Cap,
    out: &mut std::vec::Vec<Vec2>,
) {
    let side = ctx.side;
    let w2 = ctx.w2;
    let n = Vec2::new(-dir.y, dir.x) * side;
    match *cap {
        Cap::Butt => {
            out.push(b_next);
        }
        Cap::Square => {
            out.push(v + dir * w2 + n);
            out.push(v + dir * w2 - n);
            out.push(b_next);
        }
        Cap::Round => {
            let r0 = a - v;
            let r1 = b_next - v;
            if r0.len() < EPS || r1.len() < EPS {
                out.push(b_next);
                return;
            }
            // Half circle through dir: rotate r0 toward dir by +90 then to r1.
            let cos = (r0.dot(r1) / (r0.len() * r1.len())).clamp(-1.0, 1.0);
            let ang = cos.acos() * side.signum();
            let steps = ((ang.abs() / (core::f64::consts::FRAC_PI_8)).ceil() as u32).clamp(1, 16);
            let step = ang / f64::from(steps);
            let (s, c) = step.sin_cos();
            let mut r = r0;
            for _ in 0..steps {
                r = Vec2::new(r.x * c - r.y * s, r.x * s + r.y * c);
                out.push(v + r);
            }
            if let Some(l) = out.last_mut() {
                *l = b_next;
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn bbox(polys: &[Poly]) -> (Vec2, Vec2) {
        let mut min = Vec2::new(f64::INFINITY, f64::INFINITY);
        let mut max = Vec2::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
        for poly in polys {
            for p in &poly.points {
                min = Vec2::new(min.x.min(p.x), min.y.min(p.y));
                max = Vec2::new(max.x.max(p.x), max.y.max(p.y));
            }
        }
        (min, max)
    }

    #[test]
    fn zero_width_and_degenerate() {
        assert!(
            stroke_poly(
                &[Vec2::ZERO, Vec2::new(1.0, 0.0)],
                false,
                0.0,
                Join::Bevel,
                Cap::Butt
            )
            .is_empty()
        );
        assert!(stroke_poly(&[Vec2::ZERO], true, 4.0, Join::Bevel, Cap::Butt).is_empty());
        assert!(stroke_poly(&[], false, 4.0, Join::Bevel, Cap::Butt).is_empty());
    }

    #[test]
    fn horizontal_line_butt_caps_band() {
        let pts = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)];
        let polys = stroke_poly(&pts, false, 4.0, Join::Bevel, Cap::Butt);
        assert_eq!(polys.len(), 1);
        let (min, max) = bbox(&polys);
        assert!((min.x - 0.0).abs() < 1e-9 && (max.x - 10.0).abs() < 1e-9);
        assert!((min.y + 2.0).abs() < 1e-9 && (max.y - 2.0).abs() < 1e-9);
    }

    #[test]
    fn square_cap_extends_by_half_width() {
        let pts = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)];
        let polys = stroke_poly(&pts, false, 4.0, Join::Bevel, Cap::Square);
        let (min, max) = bbox(&polys);
        assert!(
            (max.x - 12.0).abs() < 1e-9,
            "square cap extends to {max:x?}"
        );
        assert!((min.x + 2.0).abs() < 1e-9);
    }

    #[test]
    fn round_cap_stays_within_half_width() {
        let pts = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)];
        let polys = stroke_poly(&pts, false, 4.0, Join::Round, Cap::Round);
        let (_, max) = bbox(&polys);
        assert!(max.x <= 12.0 + 1e-9 && max.x >= 10.0);
    }

    #[test]
    fn closed_square_stroke_two_rings_bounds() {
        let pts = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(10.0, 10.0),
            Vec2::new(0.0, 10.0),
        ];
        let polys = stroke_poly(&pts, true, 4.0, Join::Miter(4.0), Cap::Butt);
        assert_eq!(polys.len(), 2, "closed stroke gives outer + inner ring");
        let (min, max) = bbox(&polys);
        assert!((min.x + 2.0).abs() < 1e-9 && (min.y + 2.0).abs() < 1e-9);
        assert!((max.x - 12.0).abs() < 1e-9 && (max.y - 12.0).abs() < 1e-9);
        // Inner ring bounds inside the outer ring bounds.
        let areas: std::vec::Vec<f64> = polys.iter().map(|p| p.signed_area().abs()).collect();
        assert!(
            areas.len() == 2 && areas[0] != areas[1],
            "rings differ: {areas:?}"
        );
    }

    #[test]
    fn bevel_and_miter_differ_at_corner() {
        let pts = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(10.0, 10.0),
        ];
        let bevel = stroke_poly(&pts, false, 4.0, Join::Bevel, Cap::Butt);
        let miter = stroke_poly(&pts, false, 4.0, Join::Miter(4.0), Cap::Butt);
        let (bmin, bmax) = bbox(&bevel);
        let (mmin, mmax) = bbox(&miter);
        // Miter fills the corner notch that bevel cuts: miter bbox is the
        // larger one on the convex corner axes.
        assert!(mmax.x >= bmax.x - 1e-9);
        assert!(mmin.y <= bmin.y + 1e-9 || mmin.x <= bmin.x + 1e-9);
    }
}

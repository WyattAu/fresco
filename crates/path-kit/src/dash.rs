//! Dashing by arc length along flattened polylines.

use crate::geom::Vec2;
use crate::path::Poly;

const EPS: f64 = 1e-9;

/// Dash a flattened polyline.
///
/// `pattern` entries alternate dash, gap, dash, gap, ... and must all be
/// positive and finite; `phase` shifts the cycle start (mod total). An
/// invalid pattern returns the input un-dashed. `closed` joins the last
/// dash back to the first point.
#[must_use]
pub fn dash_flattened(
    points: &[Vec2],
    closed: bool,
    pattern: &[f64],
    phase: f64,
) -> std::vec::Vec<Poly> {
    let valid = !pattern.is_empty()
        && pattern.iter().all(|&v| v.is_finite() && v > EPS)
        && points.len() >= 2;
    if !valid {
        return undashed(points, closed);
    }

    let total: f64 = pattern.iter().sum();
    // Locate the pattern entry containing `phase`.
    let mut idx = 0_usize;
    let mut acc = phase.rem_euclid(total);
    loop {
        match pattern.get(idx) {
            Some(&entry) if acc >= entry => {
                acc -= entry;
                idx = (idx + 1) % pattern.len();
            }
            _ => break,
        }
    }
    let mut rem = pattern.get(idx).copied().unwrap_or(total).max(0.0) - acc;
    let mut on = idx.is_multiple_of(2);

    let mut polys: std::vec::Vec<Poly> = std::vec::Vec::new();
    let mut current: std::vec::Vec<Vec2> = std::vec::Vec::new();

    // Segment iterator: consecutive pairs, plus wrap when closed.
    let start = points.first().copied().unwrap_or(Vec2::ZERO);
    let mut prev = start;
    let emit_segment = |prev_in: Vec2,
                        next: Vec2,
                        polys: &mut std::vec::Vec<Poly>,
                        current: &mut std::vec::Vec<Vec2>,
                        rem: &mut f64,
                        idx: &mut usize,
                        on: &mut bool| {
        let seg = next - prev_in;
        let len = seg.len();
        let mut t = 0.0;
        while t < len - EPS {
            let run = (*rem).min(len - t);
            let from = prev_in + seg * (t / len);
            let to = prev_in + seg * ((t + run) / len);
            if *on {
                if current.is_empty() {
                    current.push(from);
                }
                current.push(to);
            } else if !current.is_empty() {
                polys.push(Poly {
                    points: core::mem::take(current),
                    closed: false,
                });
            }
            t += run;
            *rem -= run;
            if *rem <= EPS {
                *idx = (*idx + 1) % pattern.len();
                *rem = pattern.get(*idx).copied().unwrap_or(total).max(EPS);
                *on = (*idx).is_multiple_of(2);
            }
        }
    };

    for pair in points.windows(2) {
        if let [_p, q] = pair {
            emit_segment(
                prev,
                *q,
                &mut polys,
                &mut current,
                &mut rem,
                &mut idx,
                &mut on,
            );
            prev = *q;
        }
    }
    if closed
        && let (Some(&first), Some(&last)) = (points.first(), points.last())
        && last.distance(first) > EPS
    {
        emit_segment(
            prev,
            first,
            &mut polys,
            &mut current,
            &mut rem,
            &mut idx,
            &mut on,
        );
    }
    if !current.is_empty() {
        if closed {
            current.push(start);
        }
        polys.push(Poly {
            points: current,
            closed,
        });
    }
    polys
}

fn undashed(points: &[Vec2], closed: bool) -> std::vec::Vec<Poly> {
    if points.len() < 2 {
        return std::vec::Vec::new();
    }
    vec![Poly {
        points: points.to_vec(),
        closed,
    }]
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn poly_len(p: &Poly) -> f64 {
        let mut sum = 0.0;
        let mut prev = match p.points.first() {
            Some(&f) => f,
            None => return 0.0,
        };
        for pt in p.points.iter().skip(1) {
            sum += prev.distance(*pt);
            prev = *pt;
        }
        if p.closed
            && let Some(&first) = p.points.first()
        {
            sum += prev.distance(first);
        }
        sum
    }

    #[test]
    fn invalid_pattern_returns_undashed() {
        let pts = vec![Vec2::ZERO, Vec2::new(10.0, 0.0)];
        assert_eq!(dash_flattened(&pts, false, &[], 0.0).len(), 1);
        assert_eq!(dash_flattened(&pts, false, &[0.0, -1.0], 0.0).len(), 1);
        assert!(!dash_flattened(&pts, false, &[2.0, 1.0], 0.0).is_empty());
    }

    #[test]
    fn dash_gap_alternation_on_line() {
        let pts = vec![Vec2::ZERO, Vec2::new(10.0, 0.0)];
        let polys = dash_flattened(&pts, false, &[2.0, 1.0], 0.0);
        // 10 units, [2 on, 1 off]: on-runs at 0-2, 3-5, 6-8, 9-10.
        assert_eq!(polys.len(), 4, "got {} dashes", polys.len());
        let lens: std::vec::Vec<f64> = polys.iter().map(poly_len).collect();
        for l in &lens {
            assert!(*l <= 2.0 + EPS, "dash too long: {l}");
        }
        let total: f64 = lens.iter().sum();
        assert!((total - 7.0).abs() < 1e-6, "total on-length {total}");
    }

    #[test]
    fn phase_shifts_cycle() {
        let pts = vec![Vec2::ZERO, Vec2::new(10.0, 0.0)];
        let a = dash_flattened(&pts, false, &[2.0, 1.0], 0.0);
        let b = dash_flattened(&pts, false, &[2.0, 1.0], 0.5);
        let empty = Poly {
            points: Vec::new(),
            closed: false,
        };
        let first_a = poly_len(a.first().unwrap_or(&empty));
        let first_b = poly_len(b.first().unwrap_or(&empty));
        assert!((first_a - 2.0).abs() < 1e-6, "phase 0 first dash {first_a}");
        assert!(
            (first_b - 1.5).abs() < 1e-6,
            "phase 0.5 first dash {first_b}"
        );
    }

    #[test]
    fn closed_dash_wraps() {
        let pts = vec![
            Vec2::ZERO,
            Vec2::new(10.0, 0.0),
            Vec2::new(10.0, 10.0),
            Vec2::new(0.0, 10.0),
        ];
        let polys = dash_flattened(&pts, true, &[100.0, 1.0], 0.0);
        assert_eq!(polys.len(), 1, "single long dash wraps closed");
        let empty = Poly {
            points: Vec::new(),
            closed: false,
        };
        let p = polys.first().unwrap_or(&empty);
        assert!(p.closed);
        let l = poly_len(p);
        assert!((l - 40.0).abs() < 1e-6, "wrapped dash length {l}");
    }
}

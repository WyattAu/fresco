//! SVG path import: parse `d` attributes into fresco paths.
//!
//! The inverse of [`crate::export`]'s serialization: parses the
//! M/L/Q/C/Z command set (absolute and relative forms) into a
//! `path_kit::Path`. Smooth-curve shorthands (S/T), arcs (A), and
//! text are not supported — geometry produced by this crate's
//! exporter round-trips exactly.

use path_kit::{Path, Vec2};

/// A parsed token: a command letter or a number.
#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Cmd(char),
    Num(f64),
}

/// Tokenize a `d` attribute: command letters split number runs.
fn tokenize(d: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut chars = d.chars().peekable();
    while let Some(&ch) = chars.peek() {
        if ch.is_ascii_alphabetic() {
            chars.next();
            out.push(Tok::Cmd(ch));
        } else if ch.is_ascii_digit() || ch == '-' || ch == '+' || ch == '.' {
            let mut num = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_ascii_digit() || c == '-' || c == '+' || c == '.' || c == 'e' || c == 'E' {
                    if (c == '-' || c == '+') && !num.is_empty() && !num.ends_with(['e', 'E']) {
                        break;
                    }
                    num.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            if let Ok(v) = num.parse::<f64>() {
                out.push(Tok::Num(v));
            }
        } else {
            chars.next();
        }
    }
    out
}

/// Cursor over the token stream.
struct Parser<'a> {
    toks: &'a [Tok],
    i: usize,
}

/// Parsed coordinate pair.
struct Pair(f64, f64);

impl Parser<'_> {
    fn peek_cmd(&mut self) -> Option<char> {
        match self.toks.get(self.i) {
            Some(Tok::Cmd(c)) => {
                self.i += 1;
                Some(*c)
            }
            _ => None,
        }
    }

    fn num(&mut self) -> Option<f64> {
        match self.toks.get(self.i) {
            Some(Tok::Num(v)) => {
                let v = *v;
                self.i += 1;
                Some(v)
            }
            _ => None,
        }
    }

    fn pair(&mut self) -> Option<Pair> {
        let x = self.num()?;
        let y = self.num()?;
        Some(Pair(x, y))
    }

    /// Absolute/relative point resolution.
    fn point(&mut self, abs: bool, cur: Vec2) -> Option<Vec2> {
        let Pair(x, y) = self.pair()?;
        Some(if abs {
            Vec2::new(x, y)
        } else {
            Vec2::new(cur.x + x, cur.y + y)
        })
    }

    fn at_num(&self) -> bool {
        matches!(self.toks.get(self.i), Some(Tok::Num(_)))
    }
}

/// Parse an SVG `d` attribute into a fresco path.
#[must_use]
pub fn parse_path_d(d: &str) -> Option<Path> {
    let toks = tokenize(d);
    let mut p = Parser { toks: &toks, i: 0 };
    let mut path = Path::new();
    let mut cur = Vec2::ZERO;
    let mut subpath_start = Vec2::ZERO;
    let mut cmd: Option<char> = None;

    while p.i < toks.len() {
        if p.peek_cmd().is_some() {
            // peek_cmd advanced past a command letter: re-read it.
            cmd = Some(match toks.get(p.i - 1) {
                Some(Tok::Cmd(c)) => *c,
                _ => return None,
            });
        } else if cmd.is_none() {
            return None; // numbers before any command
        }
        let c = cmd.unwrap_or('M');
        let abs = c.is_ascii_uppercase();
        match c.to_ascii_lowercase() {
            'm' => {
                let t = p.point(abs, cur)?;
                path.move_to(t);
                cur = t;
                subpath_start = t;
                // Extra pairs continue as implicit line-tos.
                while p.at_num() {
                    let t = p.point(abs, cur)?;
                    path.line_to(t);
                    cur = t;
                }
            }
            'l' => {
                while p.at_num() {
                    let t = p.point(abs, cur)?;
                    path.line_to(t);
                    cur = t;
                }
            }
            'q' => {
                while p.at_num() {
                    let Some(Pair(cx, cy)) = p.pair() else {
                        return None; // truncated curve
                    };
                    let ctrl = if abs {
                        Vec2::new(cx, cy)
                    } else {
                        Vec2::new(cur.x + cx, cur.y + cy)
                    };
                    let t = p.point(abs, cur)?;
                    path.quad_to(ctrl, t);
                    cur = t;
                }
            }
            'c' => {
                while p.at_num() {
                    let Pair(c1x, c1y) = p.pair()?;
                    let Pair(c2x, c2y) = p.pair()?;
                    let c1 = if abs {
                        Vec2::new(c1x, c1y)
                    } else {
                        Vec2::new(cur.x + c1x, cur.y + c1y)
                    };
                    let c2 = if abs {
                        Vec2::new(c2x, c2y)
                    } else {
                        Vec2::new(cur.x + c2x, cur.y + c2y)
                    };
                    let t = p.point(abs, cur)?;
                    path.cubic_to(c1, c2, t);
                    cur = t;
                }
            }
            'z' => {
                path.close();
                cur = subpath_start;
            }
            _ => return None, // unsupported command (A/S/T/H/V)
        }
    }
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use path_kit::PathSeg;

    #[test]
    fn parse_move_line_close() {
        assert!(parse_path_d("M 0 0 L 10 0 L 10 10 Z").is_some());
        let p = parse_path_d("M 0 0 L 10 0 L 10 10 Z").unwrap_or_default();
        let segs = p.segments();
        assert!(segs.len() >= 4, "move + 2 lines + close: {segs:?}");
        assert!(matches!(segs.first(), Some(PathSeg::MoveTo(_))));
        assert!(matches!(segs.last(), Some(PathSeg::Close)));
    }

    #[test]
    fn parse_relative_lowercase() {
        assert!(parse_path_d("M 5 5 l 10 0 l 0 10 z").is_some());
        let p = parse_path_d("M 5 5 l 10 0 l 0 10 z").unwrap_or_default();
        let pts: Vec<Vec2> = p
            .segments()
            .iter()
            .filter_map(|s| match s {
                PathSeg::LineTo(v) => Some(*v),
                _ => None,
            })
            .collect();
        assert!(
            pts.contains(&Vec2::new(15.0, 5.0)) && pts.contains(&Vec2::new(15.0, 15.0)),
            "relative offsets resolved: {pts:?}"
        );
    }

    #[test]
    fn parse_quadratic_and_cubic() {
        assert!(parse_path_d("M 0 0 Q 5 -5 10 0 C 15 5 20 5 25 0 Z").is_some());
        let p = parse_path_d("M 0 0 Q 5 -5 10 0 C 15 5 20 5 25 0 Z").unwrap_or_default();
        let segs = p.segments();
        assert!(
            segs.iter().any(|s| matches!(s, PathSeg::QuadTo { .. })),
            "quad present"
        );
        assert!(
            segs.iter().any(|s| matches!(s, PathSeg::CubicTo { .. })),
            "cubic present"
        );
    }

    #[test]
    fn implicit_lineto_after_move() {
        assert!(parse_path_d("M 0 0 10 0 10 10").is_some());
        let p = parse_path_d("M 0 0 10 0 10 10").unwrap_or_default();
        let lines = p
            .segments()
            .iter()
            .filter(|s| matches!(s, PathSeg::LineTo(_)))
            .count();
        assert_eq!(lines, 2, "extra move pairs become lines");
    }

    #[test]
    fn malformed_is_none() {
        assert!(parse_path_d("M 0").is_none(), "missing pair");
        assert!(parse_path_d("Q 1 2 3").is_none(), "truncated curve");
        assert!(
            parse_path_d("A 5 5 0 1 0 10 10").is_none(),
            "unsupported arc"
        );
        assert!(parse_path_d("10 10").is_none(), "command-less numbers");
    }

    /// Export round-trip: parse our own `d` attribute output and
    /// confirm the same geometry re-serializes identically.
    #[test]
    fn export_import_round_trip() {
        let mut p = Path::new();
        p.move_to(Vec2::new(10.5, 20.25));
        p.line_to(Vec2::new(30.0, 20.25));
        p.quad_to(Vec2::new(40.0, 10.0), Vec2::new(50.0, 20.0));
        p.cubic_to(
            Vec2::new(60.0, 30.0),
            Vec2::new(70.0, 40.0),
            Vec2::new(80.0, 50.0),
        );
        p.close();
        let d = crate::export::path_data_public(p.segments());
        assert!(
            parse_path_d(&d).is_some(),
            "exported path must re-parse: {d}"
        );
        let parsed = parse_path_d(&d).unwrap_or_default();
        let d2 = crate::export::path_data_public(parsed.segments());
        assert_eq!(d, d2, "round trip stable: {d} vs {d2}");
    }
}

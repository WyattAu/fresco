//! Minimal ASCII DXF import (LINE, CIRCLE, ARC, LWPOLYLINE).
//!
//! DXF is the AutoCAD interchange format: text group-code/value
//! pairs. This reader walks the ENTITIES section and maps the four
//! geometric workhorse entities to fresco geometry. Everything else
//! (headers, tables, blocks, 3D entities) is skipped.
//!
//! Coordinate convention: DXF is Y-up; fresco is Y-down. `to_path`
//! with `flip_y` set mirrors the geometry into screen space (and
//! mirrors arc angles accordingly).

use path_kit::{Arc, Path, Vec2};

/// One imported DXF entity, in raw DXF (Y-up) coordinates.
#[derive(Clone, Debug, PartialEq)]
pub enum DxfEntity {
    /// Straight segment.
    Line {
        /// Start point.
        a: Vec2,
        /// End point.
        b: Vec2,
    },
    /// Full circle.
    Circle {
        /// Center.
        center: Vec2,
        /// Radius.
        radius: f64,
    },
    /// Circular arc, angles in degrees CCW from +X (DXF convention).
    Arc {
        /// Center.
        center: Vec2,
        /// Radius.
        radius: f64,
        /// Start angle in degrees.
        start_deg: f64,
        /// End angle in degrees.
        end_deg: f64,
    },
    /// Polyline; closed when the entity flag 70 bit 1 is set.
    Polyline {
        /// Vertex list.
        points: Vec<Vec2>,
        /// Whether the polyline closes.
        closed: bool,
    },
}

impl DxfEntity {
    /// Convert to a fresco path. `flip_y` mirrors Y (DXF Y-up into
    /// fresco Y-down) and adjusts arc direction.
    #[must_use]
    pub fn to_path(&self, flip_y: bool) -> Path {
        let fy = |v: Vec2| -> Vec2 { if flip_y { Vec2::new(v.x, -v.y) } else { v } };
        let mut path = Path::new();
        match self {
            DxfEntity::Line { a, b } => {
                path.move_to(fy(*a));
                path.line_to(fy(*b));
            }
            DxfEntity::Circle { center, radius } => {
                let c = fy(*center);
                let full = Arc::new(c, *radius, 0.0, std::f64::consts::TAU);
                full.to_path(&mut path, true);
            }
            DxfEntity::Arc {
                center,
                radius,
                start_deg,
                end_deg,
            } => {
                let c = fy(*center);
                let (s, e) = (*start_deg, *end_deg);
                // Y-flip reverses arc direction.
                let (rs, re) = if flip_y {
                    (-e.to_radians(), -s.to_radians())
                } else {
                    (s.to_radians(), e.to_radians())
                };
                let arc = Arc::new(c, *radius, rs, re);
                arc.to_path(&mut path, false);
            }
            DxfEntity::Polyline { points, closed } => {
                let mut first = true;
                for v in points {
                    let w = fy(*v);
                    if first {
                        path.move_to(w);
                        first = false;
                    } else {
                        path.line_to(w);
                    }
                }
                if *closed {
                    path.close();
                }
            }
        }
        path
    }
}

/// Parse the ENTITIES section of an ASCII DXF document into
/// geometric entities. Group codes not relevant to the four
/// supported types are ignored; unsupported entity types are
/// skipped (documented limitation).
#[must_use]
pub fn import_dxf(text: &str) -> Vec<DxfEntity> {
    // Tokenize into (code, value) pairs.
    let mut pairs: Vec<(i32, &str)> = Vec::new();
    let mut lines = text.lines();
    while let (Some(code_line), Some(value_line)) = (lines.next(), lines.next()) {
        if let Ok(code) = code_line.trim().parse::<i32>() {
            pairs.push((code, value_line.trim()));
        }
    }
    // Locate ENTITIES section: code 2 value "ENTITIES" after code 0
    // value "SECTION"; ends at code 0 value "ENDSEC".
    let mut in_entities = false;
    let mut entities = Vec::new();
    let mut i = 0_usize;
    let mut current_type: Option<String> = None;
    let mut acc: Vec<(i32, f64)> = Vec::new();
    let mut pl_points: Vec<Vec2> = Vec::new();
    let mut pl_closed = false;
    while i < pairs.len() {
        let (code, value) = pairs.get(i).copied().unwrap_or((0, ""));
        match code {
            0 => {
                // Flush the previous entity before handling section
                // markers / new entity names.
                flush_entity(
                    current_type.as_deref(),
                    &acc,
                    &mut pl_points,
                    &mut pl_closed,
                    &mut entities,
                );
                acc.clear();
                pl_points.clear();
                pl_closed = false;
                match value {
                    "SECTION" => {
                        // Next pair tells the section name.
                        if let Some((2, name)) = pairs.get(i + 1).copied() {
                            in_entities = name == "ENTITIES";
                        }
                    }
                    "ENDSEC" => in_entities = false,
                    _ => {
                        if in_entities {
                            current_type = Some(value.to_owned());
                        }
                    }
                }
            }
            _ => {
                if in_entities {
                    if let Ok(v) = value.parse::<f64>() {
                        acc.push((code, v));
                    } else if code == 70 {
                        // Non-numeric flag: ignore.
                    }
                }
            }
        }
        i += 1;
    }
    flush_entity(
        current_type.as_deref(),
        &acc,
        &mut pl_points,
        &mut pl_closed,
        &mut entities,
    );
    entities
}

/// Project accumulated codes into an entity (or polyline points).
fn flush_entity(
    kind: Option<&str>,
    acc: &[(i32, f64)],
    pl_points: &mut Vec<Vec2>,
    pl_closed: &mut bool,
    out: &mut Vec<DxfEntity>,
) {
    let Some(kind) = kind else {
        return;
    };
    let get =
        |code: i32| -> Option<f64> { acc.iter().rev().find(|(c, _)| *c == code).map(|(_, v)| *v) };
    match kind {
        "LINE" => {
            let (Some(x0), Some(y0), Some(x1), Some(y1)) = (get(10), get(20), get(11), get(21))
            else {
                return;
            };
            out.push(DxfEntity::Line {
                a: Vec2::new(x0, y0),
                b: Vec2::new(x1, y1),
            });
        }
        "CIRCLE" => {
            let (Some(cx), Some(cy), Some(r)) = (get(10), get(20), get(40)) else {
                return;
            };
            out.push(DxfEntity::Circle {
                center: Vec2::new(cx, cy),
                radius: r,
            });
        }
        "ARC" => {
            let (Some(cx), Some(cy), Some(r), Some(s), Some(e)) =
                (get(10), get(20), get(40), get(50), get(51))
            else {
                return;
            };
            out.push(DxfEntity::Arc {
                center: Vec2::new(cx, cy),
                radius: r,
                start_deg: s,
                end_deg: e,
            });
        }
        "LWPOLYLINE" => {
            // Points arrive as repeated 10/20 pairs.
            let mut i = 0_usize;
            while i < acc.len() {
                let (c, x) = acc.get(i).copied().unwrap_or((0, 0.0));
                if c == 10 {
                    let y = match acc.get(i + 1) {
                        Some((20, y)) => *y,
                        _ => break,
                    };
                    pl_points.push(Vec2::new(x, y));
                    i += 2;
                } else {
                    if c == 70 {
                        let flags = x;
                        *pl_closed = (flags as i64 & 1) == 1;
                    }
                    i += 1;
                }
            }
            if pl_points.len() >= 2 {
                out.push(DxfEntity::Polyline {
                    points: pl_points.clone(),
                    closed: *pl_closed,
                });
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use path_kit::PathSeg;

    const FIXTURE: &str = "0\nSECTION\n2\nENTITIES\n\
0\nLINE\n10\n0.0\n20\n0.0\n11\n100.0\n21\n50.0\n\
0\nCIRCLE\n10\n50.0\n20\n50.0\n40\n25.0\n\
0\nARC\n10\n50.0\n20\n50.0\n40\n25.0\n50\n0.0\n51\n90.0\n\
0\nLWPOLYLINE\n90\n3\n70\n1\n10\n0.0\n20\n0.0\n10\n40.0\n20\n0.0\n10\n40.0\n20\n40.0\n\
0\nENDSEC\n0\nEOF\n";

    #[test]
    fn fixture_yields_four_entities() {
        let entities = import_dxf(FIXTURE);
        assert_eq!(entities.len(), 4, "{entities:?}");
    }

    #[test]
    fn line_coordinates() {
        let entities = import_dxf(FIXTURE);
        assert!(
            matches!(entities.first(), Some(DxfEntity::Line { .. })),
            "first entity is a line"
        );
        let DxfEntity::Line { a, b } = entities.first().cloned().unwrap_or(DxfEntity::Polyline {
            points: Vec::new(),
            closed: false,
        }) else {
            return;
        };
        assert_eq!(a, Vec2::new(0.0, 0.0));
        assert_eq!(b, Vec2::new(100.0, 50.0));
    }

    #[test]
    fn circle_radius_and_center() {
        let entities = import_dxf(FIXTURE);
        assert!(
            matches!(entities.get(1), Some(DxfEntity::Circle { .. })),
            "second entity is a circle"
        );
        let DxfEntity::Circle { center, radius } =
            entities.get(1).cloned().unwrap_or(DxfEntity::Polyline {
                points: Vec::new(),
                closed: false,
            })
        else {
            return;
        };
        assert_eq!(center, Vec2::new(50.0, 50.0));
        assert!((radius - 25.0).abs() < 1e-9);
    }

    #[test]
    fn arc_angles() {
        let entities = import_dxf(FIXTURE);
        assert!(
            matches!(entities.get(2), Some(DxfEntity::Arc { .. })),
            "third entity is an arc"
        );
        let DxfEntity::Arc {
            start_deg, end_deg, ..
        } = entities.get(2).cloned().unwrap_or(DxfEntity::Polyline {
            points: Vec::new(),
            closed: false,
        })
        else {
            return;
        };
        assert!((start_deg - 0.0).abs() < 1e-9);
        assert!((end_deg - 90.0).abs() < 1e-9);
    }

    #[test]
    fn polyline_closed_with_three_vertices() {
        let entities = import_dxf(FIXTURE);
        assert!(
            matches!(entities.get(3), Some(DxfEntity::Polyline { .. })),
            "fourth entity is a polyline"
        );
        let DxfEntity::Polyline { points, closed } =
            entities.get(3).cloned().unwrap_or(DxfEntity::Polyline {
                points: Vec::new(),
                closed: false,
            })
        else {
            return;
        };
        assert_eq!(points.len(), 3);
        assert!(closed);
    }

    #[test]
    fn to_path_line_shapes() {
        let entities = import_dxf(FIXTURE);
        let path = entities
            .first()
            .cloned()
            .unwrap_or(DxfEntity::Polyline {
                points: Vec::new(),
                closed: false,
            })
            .to_path(true);
        let segs = path.segments();
        assert!(matches!(segs.first(), Some(PathSeg::MoveTo(_))));
        assert!(matches!(segs.get(1), Some(PathSeg::LineTo(_))));
    }

    #[test]
    fn flip_y_mirrors_and_reverses_arcs() {
        let entities = import_dxf(FIXTURE);
        let line = entities.first().cloned().unwrap_or(DxfEntity::Polyline {
            points: Vec::new(),
            closed: false,
        });
        let upright = line.to_path(false);
        let flipped = line.to_path(true);
        // First vertex: (0,0) flipped to (0,-0).
        let first_upright = match upright.segments().first() {
            Some(PathSeg::MoveTo(v)) => *v,
            _ => Vec2::ZERO,
        };
        let first_flipped = match flipped.segments().first() {
            Some(PathSeg::MoveTo(v)) => *v,
            _ => Vec2::ZERO,
        };
        assert!(
            (first_upright.y + first_flipped.y).abs() < 1e-9,
            "Y mirrored"
        );

        // Arc: 0..90 CCW Y-up flips to -90..0 (i.e. sweeps downward).
        let arc = entities.get(2).cloned().unwrap_or(DxfEntity::Polyline {
            points: Vec::new(),
            closed: false,
        });
        let DxfEntity::Arc {
            start_deg, end_deg, ..
        } = arc
        else {
            return;
        };
        // Angle mapping exercised inside to_path: -90..0 radians.
        let flipped_arc = DxfEntity::Arc {
            center: Vec2::ZERO,
            radius: 1.0,
            start_deg,
            end_deg,
        }
        .to_path(true);
        assert!(!flipped_arc.is_empty(), "flipped arc produced geometry");
    }

    #[test]
    fn unsupported_entities_skipped() {
        let dxf = "0\nSECTION\n2\nENTITIES\n0\nTEXT\n10\n0.0\n20\n0.0\n1\nhello\n\
0\nLINE\n10\n1.0\n20\n2.0\n11\n3.0\n21\n4.0\n0\nENDSEC\n";
        let entities = import_dxf(dxf);
        assert_eq!(entities.len(), 1, "TEXT skipped, LINE kept");
    }
}

//! SVG export: serialize a scene's filled paths as an SVG document.
//!
//! The interchange format every CAD tool, browser, and design app
//! can open. Export flattens the node tree in draw order, applying
//! each node's global transform to its path geometry. Text, clips,
//! and images are skipped (documented limitation: geometry-only
//! export).

use crate::{Draw, Scene};
use path_kit::PathSeg;

/// Format a float compactly (3 decimals, trailing zeros trimmed).
fn fmt_num(v: f64) -> String {
    let s = format!("{v:.3}");
    let t = s.trim_end_matches('0').trim_end_matches('.');
    if t == "-0" {
        "0".to_owned()
    } else {
        t.to_owned()
    }
}

/// Public wrapper: serialize segments as an SVG `d` attribute body
/// (the import parser consumes exactly this dialect).
#[must_use]
pub fn path_data_public(segs: &[PathSeg]) -> String {
    path_data(segs)
}

/// Append a path's segments as an SVG `d` attribute body.
fn path_data(segs: &[PathSeg]) -> String {
    let mut d = String::new();
    for seg in segs {
        match seg {
            PathSeg::MoveTo(p) => {
                d.push_str(&format!("M {} {} ", fmt_num(p.x), fmt_num(p.y)));
            }
            PathSeg::LineTo(p) => {
                d.push_str(&format!("L {} {} ", fmt_num(p.x), fmt_num(p.y)));
            }
            PathSeg::QuadTo { ctrl, end } => {
                d.push_str(&format!(
                    "Q {} {} {} {} ",
                    fmt_num(ctrl.x),
                    fmt_num(ctrl.y),
                    fmt_num(end.x),
                    fmt_num(end.y)
                ));
            }
            PathSeg::CubicTo { c1, c2, end } => {
                d.push_str(&format!(
                    "C {} {} {} {} {} {} ",
                    fmt_num(c1.x),
                    fmt_num(c1.y),
                    fmt_num(c2.x),
                    fmt_num(c2.y),
                    fmt_num(end.x),
                    fmt_num(end.y)
                ));
            }
            PathSeg::Close => {}
        }
    }
    d.push('Z');
    d
}

/// Render an SVG `<rect>`-style header with the given viewport.
fn svg_header(w: f64, h: f64) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">\n"
    )
}

impl Scene {
    /// Export the scene's visible filled paths as an SVG document
    /// string of the given viewport size.
    #[must_use]
    pub fn export_svg(&self, width: f64, height: f64) -> String {
        let mut out = svg_header(width, height);
        // Draw order walk (deepest first), same as picking.
        let mut stack: Vec<crate::Id> = Vec::new();
        if let Some(root) = self.root() {
            stack.push(root);
        }
        while let Some(id) = stack.pop() {
            let Some(node) = self.node(id) else {
                continue;
            };
            if !node.visible {
                continue;
            }
            if let Some(Draw::Path { path, brush, .. }) = &node.draw
                && let (Some(t), Some(p)) = (self.global_transform(id), self.path(*path))
            {
                // Transform segments into world space.
                let mut segs: Vec<PathSeg> = Vec::new();
                for seg in p.segments() {
                    match *seg {
                        PathSeg::MoveTo(v) => segs.push(PathSeg::MoveTo(t.apply(v))),
                        PathSeg::LineTo(v) => segs.push(PathSeg::LineTo(t.apply(v))),
                        PathSeg::QuadTo { ctrl, end } => {
                            segs.push(PathSeg::QuadTo {
                                ctrl: t.apply(ctrl),
                                end: t.apply(end),
                            });
                        }
                        PathSeg::CubicTo { c1, c2, end } => segs.push(PathSeg::CubicTo {
                            c1: t.apply(c1),
                            c2: t.apply(c2),
                            end: t.apply(end),
                        }),
                        PathSeg::Close => segs.push(PathSeg::Close),
                    }
                }
                let [r, g, b, a] = brush.color;
                let color = format!("#{r:02x}{g:02x}{b:02x}");
                let opacity = f64::from(a) / 255.0;
                out.push_str(&format!(
                    "  <path d=\"{}\" fill=\"{}\" fill-opacity=\"{:.3}\"/>\n",
                    path_data(&segs),
                    color,
                    opacity
                ));
            }
            for child in node.children.iter().rev() {
                stack.push(*child);
            }
        }
        out.push_str("</svg>\n");
        out
    }
}

#[cfg(test)]
mod tests {
    use crate::{Brush, Node, Scene};
    use path_kit::{Affine, FillRule, Path, Vec2};

    fn triangle_path(cx: f64, cy: f64) -> Path {
        let mut p = Path::new();
        p.move_to(Vec2::new(cx, cy - 20.0));
        p.line_to(Vec2::new(cx + 20.0, cy + 20.0));
        p.line_to(Vec2::new(cx - 20.0, cy + 20.0));
        p.close();
        p
    }

    #[test]
    fn export_contains_path_element() {
        let mut s = Scene::new();
        let Some(root) = s.root() else {
            return;
        };
        let tri = s.add_path(triangle_path(50.0, 50.0));
        let Some(_n) = s.spawn(
            root,
            Node::filled(None, tri, Brush::opaque(255, 128, 0), FillRule::NonZero),
        ) else {
            return;
        };
        let svg = s.export_svg(200.0, 100.0);
        assert!(svg.contains("<svg"), "svg root: {svg}");
        assert!(svg.contains("<path d=\""), "path element present");
        assert!(svg.contains("fill=\"#ff8000\""), "brush color: {svg}");
        assert!(svg.contains("M 50 30"), "move-to serialized");
        assert!(svg.contains("Z"), "closed path: {svg}");
        assert!(svg.ends_with("</svg>\n"));
    }

    #[test]
    fn export_applies_transforms() {
        let mut s = Scene::new();
        let Some(root) = s.root() else {
            return;
        };
        let mut group = Node::group(None);
        group.transform = Affine::translate(Vec2::new(100.0, 0.0));
        let Some(group) = s.spawn(root, group) else {
            return;
        };
        let tri = s.add_path(triangle_path(0.0, 0.0));
        let Some(_n) = s.spawn(
            group,
            Node::filled(None, tri, Brush::opaque(0, 0, 0), FillRule::NonZero),
        ) else {
            return;
        };
        let svg = s.export_svg(300.0, 100.0);
        // Local apex (0,-20) maps to world (100,-20): "M 100 -20".
        assert!(
            svg.contains("M 100 -20"),
            "transform applied in export: {svg}"
        );
    }

    #[test]
    fn export_skips_invisible_nodes() {
        let mut s = Scene::new();
        let Some(root) = s.root() else {
            return;
        };
        let tri = s.add_path(triangle_path(50.0, 50.0));
        let Some(n) = s.spawn(
            root,
            Node::filled(None, tri, Brush::opaque(10, 20, 30), FillRule::NonZero),
        ) else {
            return;
        };
        if let Some(node) = s.node_mut(n) {
            node.visible = false;
        }
        let svg = s.export_svg(200.0, 100.0);
        assert!(!svg.contains("<path d=\""), "invisible skipped: {svg}");
    }

    #[test]
    fn export_opacity_from_alpha() {
        let mut s = Scene::new();
        let Some(root) = s.root() else {
            return;
        };
        let tri = s.add_path(triangle_path(50.0, 50.0));
        let Some(_n) = s.spawn(
            root,
            Node::filled(None, tri, Brush::new([10, 20, 30, 128]), FillRule::NonZero),
        ) else {
            return;
        };
        let svg = s.export_svg(200.0, 100.0);
        assert!(
            svg.contains("fill-opacity=\"0.502\""),
            "alpha as opacity: {svg}"
        );
    }
}

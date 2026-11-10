//! Minimal OBJ mesh import (v, vt, f).
//!
//! Parses the OBJ subset that matters for textured geometry:
//! vertex positions, texture coordinates, and polygonal faces with
//! `v[/vt[/vn]]` index forms. Polygons with more than three
//! vertices triangulate as fans from the first vertex. Indices are
//! 1-based in OBJ (negative = relative from the end); normals are
//! parsed and discarded (flat shading derives normals per face).

use crate::math::Vec3;
use crate::mesh::Mesh;

/// An imported textured mesh: geometry plus per-triangle UV
/// triplets aligned with `mesh.faces`.
#[derive(Clone, Debug, Default)]
pub struct ObjMesh {
    /// Positions and triangle indices.
    pub mesh: Mesh,
    /// UV triplet per face, aligned with `mesh.faces` (each in
    /// normalized texture space).
    pub uvs: Vec<[(f64, f64); 3]>,
}

/// Resolve an OBJ face index (1-based; negative counts from the
/// end) against a list length.
fn resolve(index: i64, len: usize) -> Option<usize> {
    let len_i = i64::try_from(len).ok()?;
    if index > 0 {
        let idx = usize::try_from(index - 1).ok()?;
        if idx < len {
            return Some(idx);
        }
        return None;
    }
    if index < 0 {
        let idx = usize::try_from(len_i + index).ok()?;
        if idx < len {
            return Some(idx);
        }
    }
    None
}

/// Parse `f` corner token `v[/vt[/vn]]` into (vertex, uv) indices.
fn parse_corner(token: &str) -> Option<(i64, Option<i64>)> {
    let mut parts = token.split('/');
    let v = parts.next()?.parse::<i64>().ok()?;
    let vt = match parts.next() {
        Some("") | None => None,
        Some(s) => s.parse::<i64>().ok(),
    };
    Some((v, vt))
}

/// Parse an OBJ document. Returns `None` when no geometry parsed.
#[must_use]
pub fn import_obj(text: &str) -> Option<ObjMesh> {
    let mut verts: Vec<Vec3> = Vec::new();
    let mut uvs: Vec<(f64, f64)> = Vec::new();
    let mut faces: Vec<[u32; 3]> = Vec::new();
    let mut face_uvs: Vec<[(f64, f64); 3]> = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        let mut tokens = line.split_whitespace();
        match tokens.next() {
            Some("v") => {
                let x = tokens.next()?.parse::<f64>().ok()?;
                let y = tokens.next()?.parse::<f64>().ok()?;
                let z = tokens.next()?.parse::<f64>().ok()?;
                verts.push(Vec3::new(x, y, z));
            }
            Some("vt") => {
                let u = tokens.next()?.parse::<f64>().ok()?;
                let v = tokens.next()?.parse::<f64>().ok()?;
                uvs.push((u, v));
            }
            Some("f") => {
                let corners: Vec<(i64, Option<i64>)> = tokens.filter_map(parse_corner).collect();
                if corners.len() < 3 {
                    continue;
                }
                // Resolve position + uv indices per corner.
                let mut pos_idx: Vec<usize> = Vec::with_capacity(corners.len());
                let mut uv_idx: Vec<Option<usize>> = Vec::with_capacity(corners.len());
                for (v_i, vt_i) in &corners {
                    let p = resolve(*v_i, verts.len())?;
                    pos_idx.push(p);
                    uv_idx.push(vt_i.and_then(|t| resolve(t, uvs.len())));
                }
                // Fan triangulation from corner 0.
                for k in 1..pos_idx.len().saturating_sub(1) {
                    let (Some(i0), Some(i1), Some(i2)) = (
                        pos_idx.first().copied(),
                        pos_idx.get(k).copied(),
                        pos_idx.get(k + 1).copied(),
                    ) else {
                        continue;
                    };
                    faces.push([
                        u32::try_from(i0).unwrap_or(u32::MAX),
                        u32::try_from(i1).unwrap_or(u32::MAX),
                        u32::try_from(i2).unwrap_or(u32::MAX),
                    ]);
                    let uv_at = |slot: Option<Option<usize>>| -> (f64, f64) {
                        slot.and_then(|i| i.and_then(|u| uvs.get(u).copied()))
                            .unwrap_or((0.0, 0.0))
                    };
                    face_uvs.push([
                        uv_at(uv_idx.first().copied()),
                        uv_at(uv_idx.get(k).copied()),
                        uv_at(uv_idx.get(k + 1).copied()),
                    ]);
                }
            }
            _ => {}
        }
    }
    if faces.is_empty() {
        return None;
    }
    Some(ObjMesh {
        mesh: Mesh { verts, faces },
        uvs: face_uvs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = "v -1.0 -1.0 0.0\n\
v 1.0 -1.0 0.0\n\
v 1.0 1.0 0.0\n\
v -1.0 1.0 0.0\n\
vt 0.0 0.0\nvt 1.0 0.0\nvt 1.0 1.0\nvt 0.0 1.0\n\
f 1/1 2/2 3/3\n\
f 1/1 3/3 4/4\n";

    #[test]
    fn fixture_parses_two_triangles() {
        let m = import_obj(FIXTURE).unwrap_or_default();
        assert_eq!(m.mesh.verts.len(), 4);
        assert_eq!(m.mesh.faces.len(), 2);
        assert_eq!(m.uvs.len(), 2);
        assert_eq!(m.mesh.faces.first(), Some(&[0, 1, 2]));
        assert_eq!(m.mesh.faces.get(1), Some(&[0, 2, 3]));
    }

    #[test]
    fn uv_triplets_align_with_faces() {
        let m = import_obj(FIXTURE).unwrap_or_default();
        let first = m.uvs.first().copied().unwrap_or([(0.0, 0.0); 3]);
        assert_eq!(first[0], (0.0, 0.0));
        assert_eq!(first[1], (1.0, 0.0));
        assert_eq!(first[2], (1.0, 1.0));
    }

    #[test]
    fn quad_face_fans_into_two_triangles() {
        // A single quad face fans from corner 0.
        let obj = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3 4\n";
        let m = import_obj(obj).unwrap_or_default();
        assert_eq!(m.mesh.faces.len(), 2, "quad fans to 2 triangles");
        assert_eq!(m.mesh.faces.first(), Some(&[0, 1, 2]));
        assert_eq!(m.mesh.faces.get(1), Some(&[0, 2, 3]));
    }

    #[test]
    fn slash_and_negative_forms() {
        // v//vn (empty vt), and negative index from the end.
        let obj = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1/1/1 2//1 3/1/1\nf -3 -2 -1\n";
        let m = import_obj(obj).unwrap_or_default();
        assert_eq!(m.mesh.faces.len(), 2);
        // Second face uses the same three vertices via negative
        // indices (-3..-1 of 3 verts = 0,1,2).
        assert_eq!(m.mesh.faces.get(1), Some(&[0, 1, 2]));
        // First face's corners carry v//vn (no vt) -> default uvs.
        let first_uv = m.uvs.first().copied().unwrap_or([(9.0, 9.0); 3]);
        assert_eq!(first_uv[0], (0.0, 0.0), "missing vt defaults to origin");
    }

    #[test]
    fn no_faces_is_none() {
        assert!(import_obj("v 0 0 0\nv 1 0 0\n").is_none());
        assert!(import_obj("").is_none());
    }

    #[test]
    fn out_of_range_face_index_rejects() {
        let obj = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 9\n";
        assert!(import_obj(obj).is_none(), "index 9 out of range");
    }
}

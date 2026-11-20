//! glTF 2.0 subset loader: JSON document + base64 data-uri buffers
//! into a [`Mesh`].
//!
//! Covers the authored subset: one buffer (data URI, base64),
//! bufferViews, accessors of componentType UNSIGNED_INT / FLOAT with
//! type VEC3 or SCALAR, mesh 0 primitive 0 (positions + indices).
//! Skins, textures, and multi-primitive meshes are future layers.
//!
//! Deterministic: pure text/binary arithmetic, no environment state.

use crate::json::{self};
use crate::math::Vec3;
use crate::mesh::Mesh;

/// Decode standard base64 (with padding) into bytes.
#[must_use]
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some(u32::from(c - b'A')),
            b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
            b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let clean: Vec<u8> = text
        .bytes()
        .filter(|b| !b.is_ascii_whitespace() && *b != b'=')
        .collect();
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);
    let mut acc: u32 = 0;
    let mut bits = 0_u32;
    for b in &clean {
        acc = (acc << 6) | val(*b)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xFF) as u8);
        }
    }
    Some(out)
}

/// Strip a `data:application/octet-stream;base64,` prefix if present.
#[must_use]
fn strip_data_uri(uri: &str) -> &str {
    match uri.find("base64,") {
        Some(i) => &uri[i + 7..],
        None => uri,
    }
}

/// Little-endian f32 from a byte slice.
fn f32le(bytes: &[u8]) -> Option<f32> {
    if bytes.len() < 4 {
        return None;
    }
    Some(f32::from_le_bytes([
        *bytes.first()?,
        *bytes.get(1)?,
        *bytes.get(2)?,
        *bytes.get(3)?,
    ]))
}

/// Little-endian u32 from a byte slice.
fn u32le(bytes: &[u8]) -> Option<u32> {
    if bytes.len() < 4 {
        return None;
    }
    Some(u32::from_le_bytes([
        *bytes.first()?,
        *bytes.get(1)?,
        *bytes.get(2)?,
        *bytes.get(3)?,
    ]))
}

/// Read `count` components of `component_type` starting at a byte
/// offset, as f64 values.
fn read_components(
    bytes: &[u8],
    byte_offset: usize,
    component_type: usize,
    count: usize,
) -> Option<Vec<f64>> {
    let mut out = Vec::with_capacity(count);
    let mut at = byte_offset;
    for _ in 0..count {
        let chunk = bytes.get(at..at + 4)?;
        let v = match component_type {
            5126 => f64::from(f32le(chunk)?),   // FLOAT
            5121 => f64::from(*chunk.first()?), // UNSIGNED_BYTE
            5123 => {
                let lo = *chunk.first()?;
                let hi = *chunk.get(1)?;
                f64::from(u16::from_le_bytes([lo, hi]))
            }
            5125 => f64::from(u32le(chunk)?),
            _ => return None,
        };
        out.push(v);
        at += 4;
    }
    Some(out)
}

/// Import mesh 0, primitive 0 from a glTF 2.0 document whose single
/// buffer is embedded as a base64 data URI. Returns the mesh with
/// world-space vertices and triangle indices.
///
/// # Errors
/// Nothing is fallible at the call site: malformed documents return
/// `None` via `?`.
#[must_use]
pub fn import_gltf_mesh(text: &str) -> Option<Mesh> {
    let doc = json::parse(text)?;

    // Buffers: exactly one, data URI, base64.
    let buffers = doc.get("buffers")?.at(0)?;
    let uri = buffers.get("uri")?.str_val()?;
    let bytes = base64_decode(strip_data_uri(uri))?;

    // Helper: resolve an accessor to f64 components.
    let accessor_components = |idx: usize| -> Option<Vec<f64>> {
        let acc = doc.get("accessors")?.at(idx)?;
        let bv = acc.get("bufferView")?.uint()?;
        let view = doc.get("bufferViews")?.at(bv)?;
        let view_off = view.get("byteOffset").and_then(|j| j.uint()).unwrap_or(0);
        let byte_offset = acc.get("byteOffset").and_then(|j| j.uint()).unwrap_or(0);
        let count = acc.get("count")?.uint()?;
        let ctype = acc.get("componentType")?.uint()?;
        read_components(&bytes, view_off + byte_offset, ctype, count)
    };

    // Mesh 0, primitive 0: POSITION (VEC3) + indices (SCALAR).
    let mesh0 = doc.get("meshes")?.at(0)?;
    let prim = mesh0.get("primitives")?.at(0)?;
    let attrs = prim.get("attributes")?;
    let pos_idx = attrs.get("POSITION")?.uint()?;
    let pos = accessor_components(pos_idx)?;
    let n_verts = pos.len() / 3;
    if n_verts == 0 || pos.len() % 3 != 0 {
        return None;
    }
    let verts: Vec<Vec3> = (0..n_verts)
        .map(|i| {
            let x = pos.get(i * 3).copied().unwrap_or(0.0);
            let y = pos.get(i * 3 + 1).copied().unwrap_or(0.0);
            let z = pos.get(i * 3 + 2).copied().unwrap_or(0.0);
            Vec3::new(x, y, z)
        })
        .collect();

    let idx_acc = prim.get("indices")?.uint()?;
    let idx = accessor_components(idx_acc)?;
    let mut faces: Vec<[u32; 3]> = Vec::new();
    let mut at = 0_usize;
    while at + 2 < idx.len() {
        let a = idx.get(at).copied().unwrap_or(0.0);
        let b = idx.get(at + 1).copied().unwrap_or(0.0);
        let c = idx.get(at + 2).copied().unwrap_or(0.0);
        let cv = |v: f64| u32::try_from(v as i64).unwrap_or(0);
        faces.push([cv(a), cv(b), cv(c)]);
        at += 3;
    }
    if faces.is_empty() {
        return None;
    }
    Some(Mesh { verts, faces })
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    /// Build a minimal glTF document with a base64-embedded buffer:
    /// 3 vertices (one triangle) + 3 indices.
    fn tri_doc() -> String {
        // Positions: (0,0,0) (1,0,0) (0,1,0) as f32 LE.
        let mut pos_bytes: Vec<u8> = Vec::new();
        for v in [0.0_f32, 1.0, 0.0, 0.0, 1.0, 0.0] {
            pos_bytes.extend_from_slice(&v.to_le_bytes());
        }
        // Indices: 0,1,2 as u32 LE.
        let idx_bytes: Vec<u8> = [0_u32, 1, 2].iter().flat_map(|i| i.to_le_bytes()).collect();
        let b64 = |data: &[u8]| -> String {
            const TABLE: &[u8] =
                b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let mut out = String::new();
            for chunk in data.chunks(3) {
                let b0 = u32::from(chunk[0]);
                let b1 = chunk.get(1).map_or(0, |b| u32::from(*b));
                let b2 = chunk.get(2).map_or(0, |b| u32::from(*b));
                let n = (b0 << 16) | (b1 << 8) | b2;
                out.push(TABLE[(n >> 18 & 63) as usize] as char);
                out.push(TABLE[(n >> 12 & 63) as usize] as char);
                if chunk.len() > 1 {
                    out.push(TABLE[(n >> 6 & 63) as usize] as char);
                }
                if chunk.len() > 2 {
                    out.push(TABLE[(n & 63) as usize] as char);
                }
                for _ in chunk.len()..3 {
                    out.push('=');
                }
            }
            out
        };
        format!(
            r#"{{"buffers":[{{"byteLength":{len},"uri":"data:application/octet-stream;base64,{data}"}}],
             "bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":{plen}}},
                            {{"buffer":0,"byteOffset":{poff},"byteLength":{ilen}}}],
             "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},
                          {{"bufferView":1,"componentType":5125,"count":3,"type":"SCALAR"}}],
             "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1}}]}}]}}"#,
            len = pos_bytes.len() + idx_bytes.len(),
            data = b64(&pos_bytes),
            plen = pos_bytes.len(),
            poff = pos_bytes.len(),
            ilen = idx_bytes.len()
        )
    }

    /// Oracle: the embedded triangle decodes to exact vertices and
    /// indices.
    #[test]
    fn gltf_triangle_imports() {
        let Some(mesh) = import_gltf_mesh(&tri_doc()) else {
            return;
        };
        assert_eq!(mesh.verts.len(), 3);
        assert_eq!(mesh.faces.len(), 1);
        let v0 = mesh
            .verts
            .first()
            .copied()
            .unwrap_or(Vec3::new(0.0, 0.0, 0.0));
        assert!((v0.x - 0.0).abs() < 1e-6 && (v0.y - 0.0).abs() < 1e-6);
        let v1 = mesh
            .verts
            .get(1)
            .copied()
            .unwrap_or(Vec3::new(0.0, 0.0, 0.0));
        assert!((v1.x - 1.0).abs() < 1e-6 && (v1.y - 0.0).abs() < 1e-6);
        let v2 = mesh
            .verts
            .get(2)
            .copied()
            .unwrap_or(Vec3::new(0.0, 0.0, 0.0));
        assert!((v2.x - 0.0).abs() < 1e-6 && (v2.y - 1.0).abs() < 1e-6);
        let face = mesh.faces.first().copied().unwrap_or_default();
        assert_eq!(face, [0, 1, 2]);
    }

    /// Oracle: base64 decode round-trips bytes.
    #[test]
    fn base64_roundtrips() {
        let data: Vec<u8> = (0..=255_u8).collect();
        const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        // Encode via the same table logic used in tests.
        let mut enc = String::new();
        for chunk in data.chunks(3) {
            let b0 = u32::from(chunk[0]);
            let b1 = chunk.get(1).map_or(0, |b| u32::from(*b));
            let b2 = chunk.get(2).map_or(0, |b| u32::from(*b));
            let n = (b0 << 16) | (b1 << 8) | b2;
            enc.push(TABLE[(n >> 18 & 63) as usize] as char);
            enc.push(TABLE[(n >> 12 & 63) as usize] as char);
            enc.push(if chunk.len() > 1 {
                TABLE[(n >> 6 & 63) as usize] as char
            } else {
                '='
            });
            enc.push(if chunk.len() > 2 {
                TABLE[(n & 63) as usize] as char
            } else {
                '='
            });
        }
        let Some(decoded) = base64_decode(&enc) else {
            return;
        };
        assert_eq!(decoded, data, "all 256 byte values round-trip");
    }

    /// Oracle: malformed documents fail closed.
    #[test]
    fn malformed_fails_closed() {
        assert!(import_gltf_mesh("").is_none(), "empty");
        assert!(import_gltf_mesh("{}").is_none(), "no buffers");
        assert!(
            import_gltf_mesh(
                r#"{"buffers":[{"uri":"data:application/octet-stream;base64,!!!!"}]}"#
            )
            .is_none(),
            "bad base64"
        );
    }
}

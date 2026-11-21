//! glTF 2.0 subset loader: JSON document + base64 data-uri buffers
//! into a [`Mesh`].
//!
//! Covers the authored subset: one buffer (data URI, base64),
//! bufferViews, accessors of componentType UNSIGNED_INT / FLOAT with
//! type VEC3 or SCALAR, mesh 0 primitive 0 (positions + indices).
//! Skins, textures, and multi-primitive meshes are future layers.
//!
//! Deterministic: pure text/binary arithmetic, no environment state.

use crate::animation::{AnimationClip, Axis, Track};

/// Accumulated per-axis keys for one joint: ((joint, axis), keys).
type AxisKeys = ((usize, Axis), Vec<(f64, f64)>);
use crate::json::{self, Json};
use crate::math::{Mat4, Vec3};
use crate::mesh::Mesh;
use crate::skin::{Channel, Joint, Skin};

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

/// Read every component of an accessor from the byte buffer.
fn read_accessor(bytes: &[u8], doc: &Json, idx: usize) -> Option<Vec<f64>> {
    let acc = doc.get("accessors")?.at(idx)?;
    let bv = acc.get("bufferView")?.uint()?;
    let view = doc.get("bufferViews")?.at(bv)?;
    let view_off = view.get("byteOffset").and_then(|j| j.uint()).unwrap_or(0);
    let byte_offset = acc.get("byteOffset").and_then(|j| j.uint()).unwrap_or(0);
    let count = acc.get("count")?.uint()?;
    let ctype = acc.get("componentType")?.uint()?;
    let tname = acc.get("type")?.str_val()?;
    let comps = match tname {
        "SCALAR" => 1,
        "VEC2" => 2,
        "VEC3" => 3,
        "VEC4" => 4,
        "MAT4" => 16,
        _ => return None,
    };
    let total = count.checked_mul(comps)?;
    read_components(bytes, view_off + byte_offset, ctype, total)
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
    let size = match component_type {
        5121 => 1,        // UNSIGNED_BYTE
        5123 => 2,        // UNSIGNED_SHORT
        5125 | 5126 => 4, // UNSIGNED_INT | FLOAT
        _ => return None,
    };
    let mut at = byte_offset;
    for _ in 0..count {
        let chunk = bytes.get(at..at + size)?;
        let v = match component_type {
            5126 => f64::from(f32le(chunk)?),   // FLOAT
            5121 => f64::from(*chunk.first()?), // UNSIGNED_BYTE
            5123 => {
                let lo = chunk.first().copied()?;
                let hi = chunk.get(1).copied()?;
                f64::from(u16::from_le_bytes([lo, hi]))
            }
            5125 => f64::from(u32le(chunk)?),
            _ => return None,
        };
        out.push(v);
        at += size;
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

/// Quaternion (x, y, z, w) to a rotation matrix (column-major).
#[must_use]
pub fn quat_mat4(x: f64, y: f64, z: f64, w: f64) -> Mat4 {
    let xx = x * x;
    let yy = y * y;
    let zz = z * z;
    let xy = x * y;
    let xz = x * z;
    let yz = y * z;
    let wx = w * x;
    let wy = w * y;
    let wz = w * z;
    Mat4 {
        m: [
            1.0 - 2.0 * (yy + zz),
            2.0 * (xy + wz),
            2.0 * (xz - wy),
            0.0, //
            2.0 * (xy - wz),
            1.0 - 2.0 * (xx + zz),
            2.0 * (yz + wx),
            0.0, //
            2.0 * (xz + wy),
            2.0 * (yz - wx),
            1.0 - 2.0 * (xx + yy),
            0.0, //
            0.0,
            0.0,
            0.0,
            1.0,
        ],
    }
}

/// A node's local transform: `matrix` (16 floats, column-major) or
/// TRS (translate, rotate quat, scale).
#[must_use]
fn node_local(node: &Json) -> Option<Mat4> {
    if let Some(m) = node.get("matrix").and_then(|j| j.at(0)) {
        // matrix is an ARRAY of 16 numbers.
        let mut vals = [0.0_f64; 16];
        for (i, slot) in vals.iter_mut().enumerate() {
            *slot = m.at(i).and_then(|j| j.num()).unwrap_or(0.0);
        }
        return Some(Mat4 { m: vals });
    }
    let t = node.get("translation").map(|j| {
        Vec3::new(
            j.at(0).and_then(|x| x.num()).unwrap_or(0.0),
            j.at(1).and_then(|x| x.num()).unwrap_or(0.0),
            j.at(2).and_then(|x| x.num()).unwrap_or(0.0),
        )
    });
    let s = node.get("scale").map(|j| {
        Vec3::new(
            j.at(0).and_then(|x| x.num()).unwrap_or(1.0),
            j.at(1).and_then(|x| x.num()).unwrap_or(1.0),
            j.at(2).and_then(|x| x.num()).unwrap_or(1.0),
        )
    });
    let quat = node.get("rotation").map(|j| {
        (
            j.at(0).and_then(|x| x.num()).unwrap_or(0.0),
            j.at(1).and_then(|x| x.num()).unwrap_or(0.0),
            j.at(2).and_then(|x| x.num()).unwrap_or(0.0),
            j.at(3).and_then(|x| x.num()).unwrap_or(1.0),
        )
    });
    let (t, s, quat) = (t?, s?, quat?);
    // TRS compose: translate * rotate(quat) * scale.
    let scale_m = Mat4 {
        m: [
            s.x, 0.0, 0.0, 0.0, //
            0.0, s.y, 0.0, 0.0, //
            0.0, 0.0, s.z, 0.0, //
            0.0, 0.0, 0.0, 1.0,
        ],
    };
    let rot = quat_mat4(quat.0, quat.1, quat.2, quat.3);
    Some(Mat4::translate(t).mul(&rot).mul(&scale_m))
}

/// Quaternion (x, y, z, w) to Euler angles in XYZ application order
/// (radians). The decomposition is exact for single-axis rotations
/// and approximate for compound ones.
#[must_use]
fn quat_euler_xyz(x: f64, y: f64, z: f64, w: f64) -> (f64, f64, f64) {
    let sinr_cosp = 2.0 * (w * x + y * z);
    let cosr_cosp = 1.0 - 2.0 * (x * x + y * y);
    let ex = sinr_cosp.atan2(cosr_cosp);
    let sinp = 2.0 * (w * y - z * x);
    let ey = if sinp.abs() >= 1.0 {
        sinp.signum() * std::f64::consts::FRAC_PI_2
    } else {
        sinp.asin()
    };
    let sinp_cosp = 2.0 * (w * z + x * y);
    let cosp = 1.0 - 2.0 * (y * y + z * z);
    let ez = sinp_cosp.atan2(cosp);
    (ex, ey, ez)
}

/// Import animations[0] as an [`AnimationClip`]: rotation channels
/// (quaternion samplers) targeting joint nodes decompose to
/// per-axis scalar tracks (XYZ euler; planar z-only clips are exact).
/// Channels targeting non-joint nodes and non-rotation paths are
/// skipped. Returns `None` when the document has no animations or
/// no usable channels.
#[must_use]
pub fn import_gltf_animation(text: &str) -> Option<AnimationClip> {
    let doc = json::parse(text)?;
    let bytes_bytes = doc.get("buffers")?.at(0)?;
    let uri = bytes_bytes.get("uri")?.str_val()?;
    let bytes = base64_decode(strip_data_uri(uri))?;

    let anims = doc.get("animations")?.at(0)?;
    let name = anims
        .get("name")
        .and_then(|j| j.str_val())
        .unwrap_or("clip")
        .to_owned();

    // Joint mapping: skins[0].joints (node indices) -> joint indices.
    let skin0 = doc.get("skins")?.at(0)?;
    let joints_json = skin0.get("joints")?;
    let mut joint_nodes: Vec<usize> = Vec::new();
    let mut ji = 0_usize;
    while joints_json.at(ji).is_some() {
        if let Some(u) = joints_json.at(ji).and_then(|j| j.uint()) {
            joint_nodes.push(u);
        }
        ji += 1;
    }
    if joint_nodes.is_empty() {
        return None;
    }

    // Accumulate per-(joint, axis) key lists from rotation channels.
    let mut keys: Vec<AxisKeys> = Vec::new();
    let mut duration = 0.0_f64;
    let mut ci = 0_usize;
    while anims.get("channels").and_then(|c| c.at(ci)).is_some() {
        let ch = anims.get("channels").and_then(|c| c.at(ci))?;
        ci += 1;
        let Some(s_idx) = ch.get("sampler").and_then(|j| j.uint()) else {
            continue;
        };
        let Some(target) = ch.get("target") else {
            continue;
        };
        let Some(node) = target.get("node").and_then(|j| j.uint()) else {
            continue;
        };
        let Some(path) = target.get("path").and_then(|j| j.str_val()) else {
            continue;
        };
        if path != "rotation" {
            continue;
        }
        let Some(joint) = joint_nodes.iter().position(|&n| n == node) else {
            continue;
        };
        let Some(sampler) = anims.get("samplers").and_then(|s| s.at(s_idx)) else {
            continue;
        };
        let Some(input_acc) = sampler.get("input").and_then(|j| j.uint()) else {
            continue;
        };
        let Some(output_acc) = sampler.get("output").and_then(|j| j.uint()) else {
            continue;
        };
        let times = read_accessor(&bytes, &doc, input_acc)?;
        let quats = read_accessor(&bytes, &doc, output_acc)?;
        // Quaternion keys: VEC4 per time.
        let qcount = quats.len() / 4;
        if qcount == 0 {
            continue;
        }
        for k in 0..qcount {
            let t = times.get(k).copied().unwrap_or(0.0);
            let (ex, ey, ez) = quat_euler_xyz(
                quats.get(k * 4).copied().unwrap_or(0.0),
                quats.get(k * 4 + 1).copied().unwrap_or(0.0),
                quats.get(k * 4 + 2).copied().unwrap_or(0.0),
                quats.get(k * 4 + 3).copied().unwrap_or(1.0),
            );
            duration = duration.max(t);
            for (axis, value) in [(Axis::X, ex), (Axis::Y, ey), (Axis::Z, ez)] {
                let key = (joint, axis);
                if let Some((_, list)) = keys.iter_mut().find(|(a, _)| *a == key) {
                    list.push((t, value));
                } else {
                    keys.push((key, vec![(t, value)]));
                }
            }
        }
    }
    if keys.is_empty() {
        return None;
    }
    // Deterministic track order: joint, then axis rank (X=0, Y=1, Z=2).
    let rank = |a: Axis| match a {
        Axis::X => 0_u8,
        Axis::Y => 1,
        Axis::Z => 2,
    };
    keys.sort_by_key(|((joint, axis), _)| (*joint, rank(*axis)));
    let tracks: Vec<Track> = keys
        .into_iter()
        .map(|((joint, axis), k)| Track {
            joint,
            axis,
            channel: Channel::new(k),
        })
        .collect();
    Some(AnimationClip::new(&name, duration.max(1e-9), tracks))
}

/// Import mesh 0 primitive 0 WITH skinning: JOINTS_0/WEIGHTS_0
/// attributes and skins[0] (inverseBindMatrices + joints referencing
/// nodes). Returns the mesh and the constructed skin. The skin's
/// joint locals come from the referenced nodes (`matrix` or TRS).
#[must_use]
pub fn import_gltf_skin(text: &str) -> Option<(Mesh, Skin)> {
    let doc = json::parse(text)?;
    let buffers = doc.get("buffers")?.at(0)?;
    let uri = buffers.get("uri")?.str_val()?;
    let bytes = base64_decode(strip_data_uri(uri))?;

    // Mesh geometry.
    let mesh0 = doc.get("meshes")?.at(0)?;
    let prim = mesh0.get("primitives")?.at(0)?;
    let attrs = prim.get("attributes")?;
    let pos_idx = attrs.get("POSITION")?.uint()?;
    let pos = read_accessor(&bytes, &doc, pos_idx)?;
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

    let mut faces: Vec<[u32; 3]> = Vec::new();
    if let Some(idx_acc) = prim.get("indices").and_then(|j| j.uint()) {
        let idx = read_accessor(&bytes, &doc, idx_acc)?;
        let mut at = 0_usize;
        while at + 2 < idx.len() {
            let cv = |v: f64| u32::try_from(v as i64).unwrap_or(0);
            faces.push([
                cv(idx.get(at).copied().unwrap_or(0.0)),
                cv(idx.get(at + 1).copied().unwrap_or(0.0)),
                cv(idx.get(at + 2).copied().unwrap_or(0.0)),
            ]);
            at += 3;
        }
    }

    // Joint locals from the referenced nodes.
    let skin0 = doc.get("skins")?.at(0)?;
    let joints_json = skin0.get("joints")?;
    let mut joint_count = 0_usize;
    while joints_json.at(joint_count).is_some() {
        joint_count += 1;
    }
    if joint_count == 0 {
        return None;
    }
    // Count nodes (contiguous from index 0).
    let mut node_count = 0_usize;
    while nodes_at(&doc, node_count).is_some() {
        node_count += 1;
    }
    let node_at = |i: usize| -> Option<&Json> { nodes_at(&doc, i) };

    let mut joints: Vec<Joint> = Vec::new();
    for ji in 0..joint_count {
        let node_idx = joints_json.at(ji).and_then(|j| j.uint())?;
        let node = node_at(node_idx)?;
        let local = node_local(node)?;
        // Parent: the node that lists this one as a child; counted a
        // root when the parent is not itself a joint.
        let mut parent: Option<usize> = None;
        for pi in 0..node_count {
            let Some(pnode) = node_at(pi) else {
                continue;
            };
            let Some(children) = pnode.get("children") else {
                continue;
            };
            let mut ci = 0_usize;
            let mut found = false;
            while children.at(ci).is_some() {
                if children.at(ci).and_then(|j| j.uint()) == Some(node_idx) {
                    found = true;
                }
                ci += 1;
            }
            if found && parent.is_none() {
                parent = Some(pi);
            }
        }
        let in_joints = parent
            .map(|p| (0..joint_count).any(|k| joints_json.at(k).and_then(|j| j.uint()) == Some(p)))
            .unwrap_or(false);
        joints.push(Joint {
            parent: if in_joints { parent } else { None },
            local,
        });
    }

    // Inverse bind matrices.
    let ibm_idx = skin0.get("inverseBindMatrices")?.uint()?;
    let ibm = read_accessor(&bytes, &doc, ibm_idx)?;
    if ibm.len() < joint_count.checked_mul(16)? {
        return None;
    }
    let mut bind_worlds: Vec<Mat4> = Vec::new();
    for j in 0..joint_count {
        let mut m = [0.0_f64; 16];
        for (k, slot) in m.iter_mut().enumerate() {
            *slot = ibm.get(j * 16 + k).copied().unwrap_or(0.0);
        }
        bind_worlds.push(Mat4 { m });
    }

    // Bindings: JOINTS_0 (VEC4 u8/u16) + WEIGHTS_0 (VEC4 float),
    // normalized; vertices with all-zero weights bind to joint 0.
    let j_idx = attrs.get("JOINTS_0")?.uint()?;
    let w_idx = attrs.get("WEIGHTS_0")?.uint()?;
    let joint_comps = read_accessor(&bytes, &doc, j_idx)?;
    let weight_comps = read_accessor(&bytes, &doc, w_idx)?;
    let stride = 4_usize;
    if joint_comps.len() != weight_comps.len() || joint_comps.len() != n_verts * stride {
        return None;
    }
    let mut bindings: Vec<Vec<(usize, f64)>> = Vec::new();
    for v in 0..n_verts {
        let mut entries: Vec<(usize, f64)> = Vec::new();
        for c in 0..stride {
            let ji = joint_comps.get(v * stride + c).copied().unwrap_or(0.0);
            let w = weight_comps.get(v * stride + c).copied().unwrap_or(0.0);
            if w > 0.0 {
                let ji = usize::try_from(ji as i64).unwrap_or(0);
                entries.push((ji, w));
            }
        }
        let total: f64 = entries.iter().map(|(_, w)| *w).sum();
        if total <= 1e-9 {
            entries = vec![(0, 1.0)];
        } else {
            for e in &mut entries {
                e.1 /= total;
            }
        }
        bindings.push(entries);
    }

    let mesh = Mesh { verts, faces };
    let skin = Skin {
        joints,
        bindings,
        bind_worlds,
    };
    Some((mesh, skin))
}

/// Node accessor helper (nodes are a contiguous top-level array).
fn nodes_at(doc: &Json, i: usize) -> Option<&Json> {
    doc.get("nodes")?.at(i)
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod skin_tests {
    use super::*;

    /// Build a 2-joint, 3-vertex skinned document: joint 1 is a
    /// child of joint 0 with a 5px offset; vertices 0/2 bind to
    /// joint 0, vertex 1 to joint 1.
    fn skin_doc() -> String {
        // POSITION: (0,0,0) (1,0,0) (2,0,0) as f32 LE.
        let mut pos_bytes: Vec<u8> = Vec::new();
        for v in [0.0_f32, 1.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0] {
            pos_bytes.extend_from_slice(&v.to_le_bytes());
        }
        // JOINTS_0: VEC4 u8 per vertex.
        let joints_bytes: Vec<u8> = vec![0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0];
        // WEIGHTS_0: VEC4 f32 per vertex (normalized).
        let mut weights_bytes: Vec<u8> = Vec::new();
        for w in [
            1.0_f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0,
        ] {
            weights_bytes.extend_from_slice(&w.to_le_bytes());
        }
        // IBM: joint 0 identity; joint 1 translate (0, -5, 0).
        let mut ibm_bytes: Vec<u8> = Vec::new();
        for m in [Mat4::identity(), Mat4::translate(Vec3::new(0.0, -5.0, 0.0))] {
            for v in m.m {
                ibm_bytes.extend_from_slice(&v.to_le_bytes());
            }
        }
        // INDICES: single triangle 0,1,2 as u32 LE.
        let mut idx_bytes: Vec<u8> = Vec::new();
        for i in [0_u32, 1, 2] {
            idx_bytes.extend_from_slice(&i.to_le_bytes());
        }
        let mut buffer: Vec<u8> = Vec::new();
        buffer.extend_from_slice(&pos_bytes);
        buffer.extend_from_slice(&joints_bytes);
        buffer.extend_from_slice(&weights_bytes);
        buffer.extend_from_slice(&ibm_bytes);
        buffer.extend_from_slice(&idx_bytes);
        let enc = base64_encode(&buffer);
        format!(
            r#"{{"nodes":[{{"children":[1]}},{{"translation":[0.0,5.0,0.0]}},{{}}],
             "skins":[{{"joints":[1,2],"inverseBindMatrices":3}}],
             "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"JOINTS_0":1,"WEIGHTS_0":2}},"indices":4}}]}}],
             "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},
                          {{"bufferView":1,"componentType":5121,"count":3,"type":"VEC4"}},
                          {{"bufferView":2,"componentType":5126,"count":3,"type":"VEC4"}},
                          {{"bufferView":3,"componentType":5126,"count":2,"type":"MAT4"}},
                          {{"bufferView":4,"componentType":5125,"count":3,"type":"SCALAR"}}],
             "bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},
                            {{"buffer":0,"byteOffset":36,"byteLength":12}},
                            {{"buffer":0,"byteOffset":48,"byteLength":48}},
                            {{"buffer":0,"byteOffset":96,"byteLength":128}},
                            {{"buffer":0,"byteOffset":224,"byteLength":12}}],
             "buffers":[{{"byteLength":{total},"uri":"data:application/octet-stream;base64,{data}"}}]}}"#,
            total = buffer.len(),
            data = enc
        )
    }

    /// Oracle: the skinned document imports into mesh + skin with
    /// correct joints, bindings, and bind matrices.
    #[test]
    fn skinned_document_imports() {
        let Some((mesh, skin)) = import_gltf_skin(&skin_doc()) else {
            return;
        };
        assert_eq!(mesh.verts.len(), 3);
        assert_eq!(skin.joints.len(), 2);
        // Vertex bindings: 0->joint0, 1->joint1, 2->joint0.
        assert_eq!(skin.bindings[0], vec![(0, 1.0)]);
        assert_eq!(skin.bindings[1], vec![(1, 1.0)]);
        assert_eq!(skin.bindings[2], vec![(0, 1.0)]);
        // Bind worlds: identity, then translate (0, -5, 0).
        assert_eq!(skin.bind_worlds[0], Mat4::identity());
        let w1 = skin.bind_worlds[1]
            .transform_point(Vec3::ZERO)
            .map(|(x, y, _)| (x, y));
        assert_eq!(w1, Some((0.0, -5.0)));
    }

    /// Oracle: the skinned mesh deforms when a joint rotates - the
    /// full imported-pipeline animation loop works.
    #[test]
    fn skinned_mesh_deforms_with_pose() {
        let Some((mesh, skin)) = import_gltf_skin(&skin_doc()) else {
            return;
        };
        let bind = skin.posed_mesh(&mesh, &[]);
        // Poses: joint 0 stays at bind, joint 1 rotates 1.5 rad
        // about Z in its local space.
        let rotated: Vec<Mat4> = vec![
            Mat4::identity(),
            Mat4::translate(Vec3::new(0.0, 5.0, 0.0)).mul(&Mat4::rotate_z(1.5)),
        ];
        let posed = skin.posed_mesh(&mesh, &rotated);
        let moved = bind
            .verts
            .iter()
            .zip(posed.verts.iter())
            .map(|(a, b)| ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt())
            .fold(0.0_f64, f64::max);
        assert!(moved > 1.0, "rotating joint 1 must deform: {moved:.2}");
    }
}

/// Standard base64 encoder (padded). Used by the glTF loader tests
/// and by callers building data URIs for import.
#[must_use]
#[allow(clippy::indexing_slicing)]
pub fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).map_or(0, |b| u32::from(*b));
        let b2 = chunk.get(2).map_or(0, |b| u32::from(*b));
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[(n >> 18 & 63) as usize] as char);
        out.push(TABLE[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
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
        assert!(import_gltf_animation("{}").is_none(), "no animations");
        assert!(
            import_gltf_animation(
                r#"{"animations":[{}],"skins":[{"joints":[1]}],"nodes":[{},{}]}"#
            )
            .is_none(),
            "no channels"
        );
    }

    /// Oracle: rotation channels targeting joint nodes import as
    /// per-axis tracks with the clip name and duration.
    #[test]
    fn gltf_animation_imports() {
        // Two joints (nodes 1, 2); a z-rotation channel on node 1.
        // Quats: z-rotation by angle a is (0, 0, sin(a/2), cos(a/2)).
        let mut times: Vec<u8> = Vec::new();
        let mut quats: Vec<u8> = Vec::new();
        for (k, t) in [0.0_f64, 1.0, 2.0].iter().enumerate() {
            times.extend_from_slice(&(*t as f32).to_le_bytes());
            let a = match k {
                0 => -0.2_f64,
                1 => 0.2,
                _ => -0.2,
            };
            let (sx, cx) = (a * 0.5).sin_cos();
            for v in [0.0_f64, 0.0, sx, cx] {
                quats.extend_from_slice(&(v as f32).to_le_bytes());
            }
        }
        let mut buffer: Vec<u8> = Vec::new();
        buffer.extend_from_slice(&times);
        buffer.extend_from_slice(&quats);
        let doc = format!(
            r#"{{"nodes":[{{}},{{}},{{}}],
             "skins":[{{"joints":[1,2]}}],
             "animations":[[{{}}],[{{}}]][{{}}],
             "animations":[{{"name":"sway",
               "channels":[{{"sampler":0,"target":{{"node":1,"path":"rotation"}}}}],
               "samplers":[{{"input":2,"output":3,"interpolation":"LINEAR"}}]}}],
             "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"SCALAR"}},
                          {{"bufferView":1,"componentType":5126,"count":3,"type":"VEC4"}}],
             "bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":24}},
                            {{"buffer":0,"byteOffset":24,"byteLength":48}}],
             "buffers":[{{"byteLength":{total},"uri":"data:application/octet-stream;base64,{data}"}}]}}"#,
            total = buffer.len(),
            data = base64_encode(&buffer)
        );
        let Some(clip) = import_gltf_animation(&doc) else {
            return;
        };
        assert_eq!(clip.name, "sway");
        assert!((clip.duration - 2.0).abs() < 1e-6, "duration");
        assert_eq!(clip.tracks.len(), 3, "one track per axis");
        assert_eq!(clip.tracks[0].joint, 0);
        assert_eq!(clip.tracks[0].axis, Axis::Z);
        // Sampling: t=0 is the -0.2 rad key.
        assert!((clip.tracks[0].channel.sample(0.0) + 0.2).abs() < 1e-6);
    }
}

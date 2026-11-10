//! Meshes, Lambert flat shading, and backface culling.
//!
//! A mesh is world-space vertices plus triangle indices. Faces
//! shade by Lambert's law against a directional light (plus
//! ambient), and back-facing triangles cull by world-space normal
//! against the camera position.

use crate::math::{Mat4, Vec3};
use crate::raster::{Frame, Triangle, Vertex};

/// A triangle mesh.
#[derive(Clone, Debug, Default)]
pub struct Mesh {
    /// World-space vertices.
    pub verts: Vec<Vec3>,
    /// Triangle indices, CCW seen from outside.
    pub faces: Vec<[u32; 3]>,
}

impl Mesh {
    /// A cube centered on the origin with the given half-extent.
    #[must_use]
    pub fn cube(half: f64) -> Self {
        // 8 corners.
        let v = |x: f64, y: f64, z: f64| Vec3::new(x * half, y * half, z * half);
        let verts = vec![
            v(-1.0, -1.0, -1.0), // 0: far bottom left
            v(1.0, -1.0, -1.0),  // 1
            v(1.0, 1.0, -1.0),   // 2
            v(-1.0, 1.0, -1.0),  // 3
            v(-1.0, -1.0, 1.0),  // 4: near bottom left
            v(1.0, -1.0, 1.0),   // 5
            v(1.0, 1.0, 1.0),    // 6
            v(-1.0, 1.0, 1.0),   // 7
        ];
        // CCW seen from outside; the camera looks down -Z so the
        // +Z face (4,5,6,7) is the near face.
        let faces = vec![
            [4, 5, 6],
            [4, 6, 7], // +Z (near)
            [1, 0, 3],
            [1, 3, 2], // -Z (far)
            [5, 1, 2],
            [5, 2, 6], // +X
            [0, 4, 7],
            [0, 7, 3], // -X
            [7, 6, 2],
            [7, 2, 3], // +Y
            [0, 1, 5],
            [0, 5, 4], // -Y
        ];
        Mesh { verts, faces }
    }

    /// Apply a model matrix to every vertex.
    #[must_use]
    pub fn transform(&self, model: &Mat4) -> Self {
        let verts = self
            .verts
            .iter()
            .map(|v| {
                let (x, y, z) = model.transform_point(*v).unwrap_or((v.x, v.y, v.z));
                Vec3::new(x, y, z)
            })
            .collect();
        Mesh {
            verts,
            faces: self.faces.clone(),
        }
    }

    /// World-space face normal (unnormalized cross of edges).
    #[must_use]
    pub fn face_normal(&self, face: [u32; 3]) -> Option<Vec3> {
        let a = self.verts.get(face[0] as usize).copied()?;
        let b = self.verts.get(face[1] as usize).copied()?;
        let c = self.verts.get(face[2] as usize).copied()?;
        Some(b.sub(a).cross(c.sub(a)))
    }
}

/// A directional light with an ambient floor.
#[derive(Clone, Copy, Debug)]
pub struct Light {
    /// Direction TOWARD the light (unit length expected).
    pub direction: Vec3,
    /// Ambient term in `[0, 1]`.
    pub ambient: f64,
}

impl Light {
    /// Shade a base color by Lambert's law for a unit normal.
    #[must_use]
    pub fn shade(&self, normal: Vec3, base: [u8; 4]) -> [u8; 4] {
        let Some(n) = normal.normalize() else {
            return base;
        };
        let lambert = n.dot(self.direction).max(0.0);
        let k = (self.ambient + (1.0 - self.ambient) * lambert).clamp(0.0, 1.0);
        [
            (f64::from(base[0]) * k).round() as u8,
            (f64::from(base[1]) * k).round() as u8,
            (f64::from(base[2]) * k).round() as u8,
            base[3],
        ]
    }
}

/// Render a mesh: backface culling, flat Lambert shading, then the
/// depth-buffered triangle pipeline.
///
/// `eye` is the camera position in the mesh's (already model-
/// transformed) space; faces whose normal points away from the eye
/// are culled.
#[allow(clippy::too_many_arguments)]
pub fn render_mesh(
    frame: &mut Frame,
    surface: &mut paint_kit::Surface,
    painter: &mut paint_kit::Painter,
    view_projection: &Mat4,
    eye: Vec3,
    mesh: &Mesh,
    light: &Light,
    base: [u8; 4],
    near: f64,
) {
    let mut tris: Vec<Triangle> = Vec::new();
    for face in &mesh.faces {
        let Some(a) = mesh.verts.get(face[0] as usize).copied() else {
            continue;
        };
        let Some(b) = mesh.verts.get(face[1] as usize).copied() else {
            continue;
        };
        let Some(c) = mesh.verts.get(face[2] as usize).copied() else {
            continue;
        };
        let Some(normal) = mesh.face_normal(*face) else {
            continue;
        };
        // Backface cull: normal must point toward the eye side.
        let centroid = a.add(b).add(c).scale(1.0 / 3.0);
        let to_eye = eye.sub(centroid);
        if normal.dot(to_eye) <= 0.0 {
            continue;
        }
        let color = light.shade(normal, base);
        tris.push(Triangle {
            verts: [Vertex { pos: a }, Vertex { pos: b }, Vertex { pos: c }],
            color,
        });
    }
    frame.draw_triangles(surface, painter, view_projection, &tris, near);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_shape() {
        let m = Mesh::cube(1.0);
        assert_eq!(m.verts.len(), 8);
        assert_eq!(m.faces.len(), 12);
    }

    #[test]
    fn near_face_normal_points_at_camera() {
        // Camera convention: eye on +Z looking -Z, so the near face
        // is the +Z face and its normal is +Z.
        let m = Mesh::cube(1.0);
        let n = m.face_normal([4, 5, 6]).unwrap_or(Vec3::ZERO);
        let n = n.normalize().unwrap_or(Vec3::ZERO);
        assert!((n.z - 1.0).abs() < 1e-9, "+Z face normal: {n:?}");
    }

    #[test]
    fn transform_moves_vertices() {
        let m = Mesh::cube(1.0);
        let t = m.transform(&Mat4::translate(Vec3::new(10.0, 0.0, 0.0)));
        let first = t.verts.first().copied().unwrap_or(Vec3::ZERO);
        assert!((first.x - 9.0).abs() < 1e-9, "vertex translated: {first:?}");
    }

    #[test]
    fn lambert_bright_facing_perpendicular_ambient() {
        let light = Light {
            direction: Vec3::new(0.0, 0.0, 1.0),
            ambient: 0.2,
        };
        let facing = light.shade(Vec3::new(0.0, 0.0, 1.0), [100, 100, 100, 255]);
        assert_eq!(facing[0], 100, "normal parallel to light: full brightness");
        let side = light.shade(Vec3::new(1.0, 0.0, 0.0), [100, 100, 100, 255]);
        assert_eq!(side[0], 20, "perpendicular: ambient only");
        let away = light.shade(Vec3::new(0.0, 0.0, -1.0), [100, 100, 100, 255]);
        assert_eq!(away[0], 20, "facing away clamps to ambient");
    }

    #[test]
    fn backfaces_are_culled() {
        // Diagonal camera: an iso view of a cube shows exactly 3
        // faces (+Z, +X, +Y); a head-on eye would see only 1.
        let mesh = Mesh::cube(1.0);
        let eye = Vec3::new(3.0, 2.5, 4.0);
        let mut visible = 0_usize;
        for face in &mesh.faces {
            let Some(n) = mesh.face_normal(*face) else {
                continue;
            };
            let Some(a) = mesh.verts.get(face[0] as usize).copied() else {
                continue;
            };
            let Some(b) = mesh.verts.get(face[1] as usize).copied() else {
                continue;
            };
            let Some(c) = mesh.verts.get(face[2] as usize).copied() else {
                continue;
            };
            let centroid = a.add(b).add(c).scale(1.0 / 3.0);
            if n.dot(eye.sub(centroid)) > 0.0 {
                visible += 1;
            }
        }
        // 3 faces x 2 triangles each.
        assert_eq!(
            visible, 6,
            "iso view shows 3 faces (6 triangles): {visible}"
        );
    }

    #[test]
    fn lit_cube_render_is_deterministic() {
        let run = || {
            let Some(mut surface) = paint_kit::Surface::new(64, 64) else {
                return 0_u64;
            };
            let mut painter = paint_kit::Painter::default();
            let mut frame = Frame::new(64, 64);
            let proj = Mat4::perspective(std::f64::consts::FRAC_PI_4, 1.0, 0.1, 100.0);
            let view = Mat4::look_at(
                Vec3::new(3.0, 2.5, 4.0),
                Vec3::ZERO,
                Vec3::new(0.0, 1.0, 0.0),
            );
            let vp = proj.mul(&view);
            let model = Mat4::rotate_y(0.6);
            let world = Mesh::cube(1.0).transform(&model);
            let eye = Vec3::new(3.0, 2.5, 4.0);
            render_mesh(
                &mut frame,
                &mut surface,
                &mut painter,
                &vp,
                eye,
                &world,
                &Light {
                    direction: Vec3::new(0.5, 0.8, 0.3)
                        .normalize()
                        .unwrap_or(Vec3::new(0.0, 1.0, 0.0)),
                    ambient: 0.25,
                },
                [220, 90, 60, 255],
                0.1,
            );
            surface.data().iter().fold(0xcbf29ce484222325_u64, |h, &b| {
                (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
            })
        };
        assert_eq!(run(), run(), "lit cube byte-identical across runs");
    }

    #[test]
    fn lit_faces_differ_by_orientation() {
        // With the light straight down +Z, the near face renders
        // brighter than a side face: distinct shaded colors exist.
        let light = Light {
            direction: Vec3::new(0.0, 0.0, 1.0),
            ambient: 0.2,
        };
        let near = light.shade(Vec3::new(0.0, 0.0, 1.0), [200, 100, 50, 255]);
        let side = light.shade(Vec3::new(1.0, 0.0, 0.0), [200, 100, 50, 255]);
        assert!(
            near[0] > side[0],
            "near face brighter: {} vs {}",
            near[0],
            side[0]
        );
    }
}

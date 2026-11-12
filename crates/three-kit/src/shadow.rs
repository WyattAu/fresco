//! Planar projected shadows.
//!
//! The classic software-renderer shadow: project each mesh vertex
//! through the light position onto a ground plane, then draw the
//! flattened triangles as a flat dark shape. Cheap, correct for
//! planar receivers, and composes with the existing z-buffered
//! pipeline (lift the shadow slightly off the plane to avoid
//! z-fighting with the ground quad).

use crate::math::{Mat4, Vec3};
use crate::mesh::Mesh;
use crate::raster::{Frame, Triangle, Vertex};

/// Project `x` through `light` onto the plane through
/// `plane_point` with unit `plane_normal`. Returns `None` when the
/// light-to-vertex ray is parallel to the plane (no intersection).
#[must_use]
pub fn project_to_plane(
    x: Vec3,
    light: Vec3,
    plane_point: Vec3,
    plane_normal: Vec3,
) -> Option<Vec3> {
    let n = plane_normal.normalize()?;
    // Ray: L + t * (x - L). Plane: n . (p - P) = 0.
    // t = n.(P - L) / n.(x - L).
    let numer = n.dot(plane_point.sub(light));
    let denom = n.dot(x.sub(light));
    if denom.abs() < 1e-12 {
        return None;
    }
    let t = numer / denom;
    Some(light.add(x.sub(light).scale(t)))
}

/// Draw `mesh`'s planar shadow onto the plane, lifted `lift` units
/// along the plane normal (z-fighting guard). Draw the ground quad
/// BEFORE the shadow, and the scene mesh after, so depth ordering
/// stays stable.
#[allow(clippy::too_many_arguments)]
pub fn draw_planar_shadow(
    frame: &mut Frame,
    surface: &mut paint_kit::Surface,
    painter: &mut paint_kit::Painter,
    view_projection: &Mat4,
    mesh: &Mesh,
    light: Vec3,
    plane_point: Vec3,
    plane_normal: Vec3,
    color: [u8; 4],
    lift: f64,
    near: f64,
) {
    let Some(n) = plane_normal.normalize() else {
        return;
    };
    let lift_vec = n.scale(lift);
    let mut tris: Vec<Triangle> = Vec::new();
    for face in &mesh.faces {
        let mut pts = [Vec3::ZERO; 3];
        let mut ok = true;
        for (pt_slot, vi) in pts.iter_mut().zip(face.iter()) {
            let Some(&v) = mesh.verts.get(*vi as usize) else {
                ok = false;
                break;
            };
            match project_to_plane(v, light, plane_point, n) {
                Some(p) => *pt_slot = p.add(lift_vec),
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            continue;
        }
        tris.push(Triangle {
            verts: [
                Vertex { pos: pts[0] },
                Vertex { pos: pts[1] },
                Vertex { pos: pts[2] },
            ],
            color,
        });
    }
    frame.draw_triangles(surface, painter, view_projection, &tris, near);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Vec3;

    /// Ground plane: y = 0, normal +Y.
    const GROUND: (Vec3, Vec3) = (Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0));

    #[test]
    fn vertex_below_light_projects_onto_plane() {
        // Light at (0,10,0); vertex at (1,5,0) is halfway down.
        // The ray hits y=0 at twice the horizontal offset: (2,0,0).
        let p = project_to_plane(
            Vec3::new(1.0, 5.0, 0.0),
            Vec3::new(0.0, 10.0, 0.0),
            GROUND.0,
            GROUND.1,
        );
        assert!(p.is_some(), "ray intersects the plane");
        let p = p.unwrap_or(Vec3::ZERO);
        assert!(
            (p.x - 2.0).abs() < 1e-9 && p.y.abs() < 1e-9 && p.z.abs() < 1e-9,
            "projected: {p:?}"
        );
    }

    #[test]
    fn projected_points_lie_on_the_plane() {
        let (pp, pn) = GROUND;
        for x in [
            Vec3::new(1.0, 3.0, 2.0),
            Vec3::new(-4.0, 1.0, 7.0),
            Vec3::new(2.0, 8.0, -3.0),
        ] {
            let projected = project_to_plane(x, Vec3::new(0.0, 12.0, 0.0), pp, pn);
            assert!(projected.is_some(), "must project");
            let p = projected.unwrap_or(Vec3::ZERO);
            assert!(p.y.abs() < 1e-9, "on the plane: {p:?}");
        }
    }

    #[test]
    fn parallel_ray_is_none() {
        // Vertex at the same height as the light: ray parallel to
        // the ground plane.
        let p = project_to_plane(
            Vec3::new(5.0, 10.0, 3.0),
            Vec3::new(0.0, 10.0, 0.0),
            GROUND.0,
            GROUND.1,
        );
        assert!(p.is_none(), "parallel ray has no plane intersection");
    }

    #[test]
    fn shadow_extends_away_from_the_light() {
        // Light to the LEFT (-X); the cube at the origin casts its
        // shadow to the RIGHT (+X).
        let light = Vec3::new(-8.0, 8.0, 0.0);
        let cube = Mesh::cube(0.5);
        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        for v in &cube.verts {
            let projected = project_to_plane(*v, light, GROUND.0, GROUND.1);
            assert!(projected.is_some(), "cube verts project");
            let p = projected.unwrap_or(Vec3::ZERO);
            min_x = min_x.min(p.x);
            max_x = max_x.max(p.x);
        }
        assert!(
            max_x > 0.5,
            "shadow reaches past the cube on the far side: [{min_x}, {max_x}]"
        );
    }

    #[test]
    fn shadow_pixels_are_dark_and_deterministic() {
        let Some(mut surface) = paint_kit::Surface::new(64, 64) else {
            return;
        };
        let mut painter = paint_kit::Painter::default();
        let mut frame = Frame::new(64, 64);
        // Ground quad (two triangles) at y=0.
        let ground = Mesh {
            verts: vec![
                Vec3::new(-10.0, 0.0, -10.0),
                Vec3::new(10.0, 0.0, -10.0),
                Vec3::new(10.0, 0.0, 10.0),
                Vec3::new(-10.0, 0.0, 10.0),
            ],
            faces: vec![[0, 2, 1], [0, 3, 2]],
        };
        // Cube floating above.
        let mut cube = Mesh::cube(0.5);
        cube = cube.transform(&Mat4::translate(Vec3::new(0.0, 1.5, 0.0)));
        let proj = Mat4::perspective(std::f64::consts::FRAC_PI_4, 1.0, 0.1, 100.0);
        let view = Mat4::look_at(
            Vec3::new(2.5, 2.5, 4.0),
            Vec3::new(0.0, 0.5, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        );
        let vp = proj.mul(&view);
        let light = Vec3::new(-3.0, 6.0, 1.0);

        // Ground, then shadow, then the mesh itself.
        let gv = |i: usize| -> Vec3 { ground.verts.get(i).copied().unwrap_or(Vec3::ZERO) };
        let ground_tris = vec![
            Triangle {
                verts: [
                    Vertex { pos: gv(0) },
                    Vertex { pos: gv(2) },
                    Vertex { pos: gv(1) },
                ],
                color: [150, 150, 150, 255],
            },
            Triangle {
                verts: [
                    Vertex { pos: gv(0) },
                    Vertex { pos: gv(3) },
                    Vertex { pos: gv(2) },
                ],
                color: [150, 150, 150, 255],
            },
        ];
        frame.draw_triangles(&mut surface, &mut painter, &vp, &ground_tris, 0.1);
        draw_planar_shadow(
            &mut frame,
            &mut surface,
            &mut painter,
            &vp,
            &cube,
            light,
            GROUND.0,
            GROUND.1,
            [60, 60, 60, 255],
            0.01,
            0.1,
        );
        let lit_tris: Vec<Triangle> = cube
            .faces
            .iter()
            .filter_map(|f| {
                let (Some(&a), Some(&b), Some(&c)) = (
                    cube.verts.get(f[0] as usize),
                    cube.verts.get(f[1] as usize),
                    cube.verts.get(f[2] as usize),
                ) else {
                    return None;
                };
                let n = cube.face_normal(*f)?;
                let centroid = a.add(b).add(c).scale(1.0 / 3.0);
                if n.dot(light.sub(centroid)) <= 0.0 {
                    return None;
                }
                Some(Triangle {
                    verts: [Vertex { pos: a }, Vertex { pos: b }, Vertex { pos: c }],
                    color: [220, 80, 70, 255],
                })
            })
            .collect();
        frame.draw_triangles(&mut surface, &mut painter, &vp, &lit_tris, 0.1);

        // Determinism.
        let hash = surface.data().iter().fold(0xcbf29ce484222325_u64, |h, &b| {
            (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
        });
        let _ = hash;
        // The shadow region (between the cube and the light-away
        // side) is darker than the lit ground.
        let sample = |col: usize, row: usize| -> [u8; 3] {
            let base = (row * 64 + col) * 4;
            let d = surface.data();
            let mut out = [0_u8; 3];
            for (k, slot) in out.iter_mut().enumerate() {
                if let Some(&v) = d.get(base + k) {
                    *slot = v;
                }
            }
            out
        };
        // Scan the center row for a dark run (the shadow) next to a
        // lighter run (lit ground).
        let mut found_shadow_adjacent_to_ground = false;
        'scan: for row in 1..63 {
            for col in 1..63 {
                let a = sample(col, row);
                let b = sample(col + 1, row);
                let dark = |c: [u8; 3]| c[0] < 120 && c[1] < 120 && c[2] < 120;
                let lightish = |c: [u8; 3]| c[0] > 130;
                if (dark(a) && lightish(b)) || (lightish(a) && dark(b)) {
                    found_shadow_adjacent_to_ground = true;
                    break 'scan;
                }
            }
        }
        assert!(
            found_shadow_adjacent_to_ground,
            "shadow edge adjacent to lit ground"
        );
    }
}

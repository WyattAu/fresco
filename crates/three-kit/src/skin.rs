//! Skeletal skinning: joint hierarchies, rigid vertex binding, and
//! keyframed poses.
//!
//! The animation foundation: joints form a tree (each carries a
//! local transform relative to its parent); vertices bind rigidly
//! to one joint (weight 1); a pose is a set of joint-local
//! transforms evaluated by walking parent-to-child. Smooth (multi-
//! weight) skinning builds on this — documented simplification.

use crate::math::{Mat4, Vec3};
use crate::mesh::Mesh;

/// A joint: parent index (root has none) and a bind-pose local
/// transform relative to the parent.
#[derive(Clone, Copy, Debug)]
pub struct Joint {
    /// Parent joint index; `None` for the root.
    pub parent: Option<usize>,
    /// Local transform relative to the parent at bind pose.
    pub local: Mat4,
}

/// Rigid skin: a mesh whose vertices are each bound to exactly one
/// joint (weight 1). Smooth multi-weight skinning is a documented
/// future layer.
///
/// Vertices live in model space, so posing composes each joint's
/// world matrix with its inverse bind matrix: bind pose reproduces
/// the input exactly, and any pose applies only the delta.
#[derive(Clone, Debug)]
pub struct Skin {
    /// Joint hierarchy (index order is parent-before-child is NOT
    /// required; evaluation recurses through parents).
    pub joints: Vec<Joint>,
    /// One binding per mesh vertex: (joint index, weight) with
    /// weight normalized to 1.
    pub bindings: Vec<(usize, f64)>,
    /// Bind-pose world matrices (captured at construction).
    pub bind_worlds: Vec<Mat4>,
}

impl Skin {
    /// Bind every vertex of `mesh` to the joint nearest its
    /// position (Euclidean, on joint origins). The trivial but
    /// serviceable auto-rig for rigid props.
    #[must_use]
    pub fn bind_nearest(mesh: &Mesh, joints: &[Joint]) -> Self {
        let skin_stub = Skin {
            joints: joints.to_vec(),
            bindings: Vec::new(),
            bind_worlds: Vec::new(),
        };
        let bind_worlds = skin_stub.joint_worlds(&[]);
        let joint_origins: Vec<Vec3> = joints
            .iter()
            .map(|j| {
                let (x, y, z) = j
                    .local
                    .transform_point(Vec3::ZERO)
                    .unwrap_or((0.0, 0.0, 0.0));
                Vec3::new(x, y, z)
            })
            .collect();
        let bindings = mesh
            .verts
            .iter()
            .map(|v| {
                let mut best = 0_usize;
                let mut best_d = f64::INFINITY;
                for (i, o) in joint_origins.iter().enumerate() {
                    let d = v.distance(*o);
                    if d < best_d {
                        best_d = d;
                        best = i;
                    }
                }
                (best, 1.0)
            })
            .collect();
        Skin {
            joints: joints.to_vec(),
            bindings,
            bind_worlds,
        }
    }

    /// World transform per joint: walks parents, caching results.
    /// `poses` overrides each joint's local transform (same order
    /// as `joints`).
    #[must_use]
    pub fn joint_worlds(&self, poses: &[Mat4]) -> Vec<Mat4> {
        let mut worlds: Vec<Option<Mat4>> = vec![None; self.joints.len()];
        for i in 0..self.joints.len() {
            let _ = world_transform(self.joints.to_vec(), poses, i, &mut worlds);
        }
        worlds
            .into_iter()
            .map(|w| w.unwrap_or(Mat4::identity()))
            .collect()
    }

    /// Pose the bound mesh: every vertex transformed by
    /// `pose_world * inverse(bind_world)` for its joint. Bind pose
    /// (poses = joint locals) reproduces the input exactly.
    #[must_use]
    pub fn posed_mesh(&self, base: &Mesh, poses: &[Mat4]) -> Mesh {
        let worlds = self.joint_worlds(poses);
        let verts = base
            .verts
            .iter()
            .zip(self.bindings.iter())
            .map(|(v, (joint, weight))| {
                let pose_w = world_of(&worlds, *joint);
                let bind_inv = self
                    .bind_worlds
                    .get(*joint)
                    .and_then(Mat4::inverse)
                    .unwrap_or_else(Mat4::identity);
                let final_w = pose_w.mul(&bind_inv);
                let (x, y, z) = final_w.transform_point(*v).unwrap_or((v.x, v.y, v.z));
                Vec3::new(
                    v.x * (1.0 - weight) + x * weight,
                    v.y * (1.0 - weight) + y * weight,
                    v.z * (1.0 - weight) + z * weight,
                )
            })
            .collect();
        Mesh {
            verts,
            faces: base.faces.clone(),
        }
    }
}

fn world_of(worlds: &[Mat4], joint: usize) -> Mat4 {
    worlds.get(joint).copied().unwrap_or_else(Mat4::identity)
}

/// Recursive world-transform evaluation with memoization.
fn world_transform(
    joints: Vec<Joint>,
    poses: &[Mat4],
    i: usize,
    memo: &mut Vec<Option<Mat4>>,
) -> Option<Mat4> {
    if let Some(w) = memo.get(i).and_then(|m| *m) {
        return Some(w);
    }
    let joint = joints.get(i).copied()?;
    let local = poses.get(i).copied().unwrap_or(joint.local);
    let world = match joint.parent {
        None => local,
        Some(p) => {
            let parent_world = world_transform(joints, poses, p, memo).unwrap_or(Mat4::identity());
            parent_world.mul(&local)
        }
    };
    if let Some(slot) = memo.get_mut(i) {
        *slot = Some(world);
    }
    Some(world)
}

/// A keyframed scalar channel: piecewise-linear sample over time.
#[derive(Clone, Debug, Default)]
pub struct Channel {
    /// Sorted `(time, value)` keys.
    pub keys: Vec<(f64, f64)>,
}

impl Channel {
    /// New channel from keys (sorted automatically).
    #[must_use]
    pub fn new(mut keys: Vec<(f64, f64)>) -> Self {
        keys.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        Channel { keys }
    }

    /// Linear sample at `t`; clamps at the ends.
    #[must_use]
    pub fn sample(&self, t: f64) -> f64 {
        let keys = &self.keys;
        if keys.is_empty() {
            return 0.0;
        }
        let Some(&(t0, v0)) = keys.first() else {
            return 0.0;
        };
        if t <= t0 {
            return v0;
        }
        let Some(&(tl, vl)) = keys.last() else {
            return 0.0;
        };
        if t >= tl {
            return vl;
        }
        for k in 1..keys.len() {
            let Some((t1, v1)) = keys.get(k).copied() else {
                break;
            };
            let Some(&(t_prev, v_prev)) = keys.get(k - 1) else {
                break;
            };
            if t <= t1 {
                let span = t1 - t_prev;
                let f = if span > 1e-12 {
                    (t - t_prev) / span
                } else {
                    0.0
                };
                return v_prev + (v1 - v_prev) * f;
            }
        }
        vl
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skin_joints() -> Vec<Joint> {
        vec![
            Joint {
                parent: None,
                local: Mat4::identity(),
            },
            Joint {
                parent: Some(0),
                local: Mat4::translate(Vec3::new(2.0, 0.0, 0.0)),
            },
        ]
    }

    /// Two-joint arm: root at origin, child translated +2 on X.
    fn arm() -> (Skin, Mesh) {
        let joints = vec![
            Joint {
                parent: None,
                local: Mat4::identity(),
            },
            Joint {
                parent: Some(0),
                local: Mat4::translate(Vec3::new(2.0, 0.0, 0.0)),
            },
        ];
        let mesh = Mesh {
            verts: vec![
                Vec3::new(0.0, 0.0, 0.0), // bound to root
                Vec3::new(2.0, 0.0, 0.0), // on the child joint
                Vec3::new(3.0, 0.0, 0.0), // past the child joint
            ],
            faces: vec![[0, 1, 2]],
        };
        let bindings = vec![(0_usize, 1.0), (1_usize, 1.0), (1_usize, 1.0)];
        let stub = Skin {
            joints: skin_joints(),
            bindings: Vec::new(),
            bind_worlds: Vec::new(),
        };
        let bind_worlds = stub.joint_worlds(&[]);
        let skin = Skin {
            joints,
            bindings,
            bind_worlds,
        };
        (skin, mesh)
    }

    #[test]
    fn bind_pose_is_identity() {
        let (skin, mesh) = arm();
        let posed = skin.posed_mesh(&mesh, &[]);
        assert_eq!(posed.verts, mesh.verts, "empty poses = bind pose");
    }

    #[test]
    fn child_rotation_moves_bound_vertices() {
        let (skin, mesh) = arm();
        // Rotate the child joint 90 degrees about Z at its own
        // origin: children-of-child verts swing up to +Y.
        let poses = vec![
            Mat4::identity(),
            Mat4::translate(Vec3::new(2.0, 0.0, 0.0))
                .mul(&Mat4::rotate_z(std::f64::consts::FRAC_PI_2)),
        ];
        let posed = skin.posed_mesh(&mesh, &poses);
        // Root-bound vertex stays.
        assert_eq!(posed.verts.first().copied(), Some(Vec3::new(0.0, 0.0, 0.0)));
        // Child joint origin stays at its pivot (2,0,0).
        let v1 = posed.verts.get(1).copied().unwrap_or(Vec3::ZERO);
        assert!(
            (v1.x - 2.0).abs() < 1e-9 && v1.y.abs() < 1e-9,
            "joint origin is the pivot: {v1:?}"
        );
        // Vertex at (3,0,0) is +1 past the pivot: swings to (2,1,0).
        let v2 = posed.verts.get(2).copied().unwrap_or(Vec3::ZERO);
        assert!(
            (v2.x - 2.0).abs() < 1e-9 && (v2.y - 1.0).abs() < 1e-9,
            "rotated about the child pivot: {v2:?}"
        );
    }

    #[test]
    fn parent_rotation_propagates_to_child() {
        let (skin, mesh) = arm();
        // Rotate only the ROOT: both joints swing together.
        let poses = vec![
            Mat4::rotate_z(std::f64::consts::FRAC_PI_2),
            Mat4::translate(Vec3::new(2.0, 0.0, 0.0)),
        ];
        let posed = skin.posed_mesh(&mesh, &poses);
        // Root-bound vertex at origin stays at origin (rotation
        // about the root pivot there).
        assert_eq!(posed.verts.first().copied(), Some(Vec3::new(0.0, 0.0, 0.0)));
        // Child joint origin (2,0,0) rotates to (0,2,0).
        let v1 = posed.verts.get(1).copied().unwrap_or(Vec3::ZERO);
        assert!(
            v1.x.abs() < 1e-9 && (v1.y - 2.0).abs() < 1e-9,
            "parent rotation propagates: {v1:?}"
        );
    }

    #[test]
    fn channel_interpolates_linearly() {
        let ch = Channel::new(vec![(0.0, 0.0), (1.0, 10.0), (2.0, 5.0)]);
        assert!((ch.sample(-1.0) - 0.0).abs() < 1e-9, "clamped before");
        assert!((ch.sample(0.5) - 5.0).abs() < 1e-9, "midpoint");
        assert!((ch.sample(1.5) - 7.5).abs() < 1e-9, "second span");
        assert!((ch.sample(9.0) - 5.0).abs() < 1e-9, "clamped after");
    }

    #[test]
    fn animated_arm_is_deterministic() {
        let run = || {
            let (skin, mesh) = arm();
            let ch = Channel::new(vec![(0.0, 0.0), (1.0, std::f64::consts::PI)]);
            let mut out = Vec::new();
            for k in 0..10_u32 {
                let t = f64::from(k) / 9.0;
                let angle = ch.sample(t);
                let poses = vec![
                    Mat4::identity(),
                    Mat4::translate(Vec3::new(2.0, 0.0, 0.0)).mul(&Mat4::rotate_z(angle)),
                ];
                let posed = skin.posed_mesh(&mesh, &poses);
                out.extend(posed.verts.iter().map(|v| (v.x, v.y, v.z)));
            }
            out
        };
        assert_eq!(run(), run(), "animated pose byte-identical across runs");
    }

    #[test]
    fn bind_nearest_partitions_vertices() {
        let joints = vec![
            Joint {
                parent: None,
                local: Mat4::identity(),
            },
            Joint {
                parent: Some(0),
                local: Mat4::translate(Vec3::new(10.0, 0.0, 0.0)),
            },
        ];
        let mesh = Mesh {
            verts: vec![
                Vec3::new(1.0, 0.0, 0.0), // nearest root
                Vec3::new(9.0, 0.0, 0.0), // nearest child
            ],
            faces: vec![[0, 1, 0]],
        };
        let skin = Skin::bind_nearest(&mesh, &joints);
        assert_eq!(skin.bindings.first().copied(), Some((0_usize, 1.0)));
        assert_eq!(skin.bindings.get(1).copied(), Some((1_usize, 1.0)));
    }
}

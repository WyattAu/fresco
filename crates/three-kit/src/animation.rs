//! Animation clips: named tracks bound to joints, with playback.
//!
//! A clip binds rotation channels (piecewise-linear, from
//! [`Channel`]) to joints along X/Y/Z axes; sampling at time `t`
//! produces joint-local poses with untracked joints keeping their
//! bind locals. [`Animator`] owns playback state: time, looping,
//! play/pause.
//!
//! Deterministic: fixed iteration order, pure arithmetic.

use crate::math::Mat4;
use crate::skin::{Channel, Joint};

/// Which local-rotation axis a track drives.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Axis {
    /// Rotation about local X.
    X,
    /// Rotation about local Y.
    Y,
    /// Rotation about local Z.
    Z,
}

/// One animated joint: a rotation channel on one axis.
#[derive(Clone, Debug)]
pub struct Track {
    /// Joint index (into the skeleton's joint list).
    pub joint: usize,
    /// Animated axis.
    pub axis: Axis,
    /// Value channel (radians).
    pub channel: Channel,
}

/// A named, timed set of tracks.
#[derive(Clone, Debug)]
pub struct AnimationClip {
    /// Clip name.
    pub name: String,
    /// Loop length in seconds.
    pub duration: f64,
    /// Animated joints.
    pub tracks: Vec<Track>,
}

impl AnimationClip {
    /// New clip; duration clamps to positive.
    #[must_use]
    pub fn new(name: &str, duration: f64, tracks: Vec<Track>) -> Self {
        AnimationClip {
            name: name.to_owned(),
            duration: if duration > 1e-9 { duration } else { 1.0 },
            tracks,
        }
    }

    /// Joint-local poses at normalized time `t` in `[0, duration]`:
    /// tracked joints get their axis rotation, untracked joints
    /// keep their bind local.
    #[must_use]
    pub fn pose(&self, joints: &[Joint], t: f64) -> Vec<Mat4> {
        let t = t.clamp(0.0, self.duration);
        let axis_rot = |axis: Axis, angle: f64| -> Mat4 {
            match axis {
                Axis::X => Mat4::rotate_x(angle),
                Axis::Y => Mat4::rotate_y(angle),
                Axis::Z => Mat4::rotate_z(angle),
            }
        };
        joints
            .iter()
            .enumerate()
            .map(|(i, joint)| {
                let mut local = joint.local;
                for track in &self.tracks {
                    if track.joint == i {
                        let r = axis_rot(track.axis, track.channel.sample(t));
                        local = local.mul(&r);
                    }
                }
                local
            })
            .collect()
    }
}

/// Playback state for one clip.
#[derive(Clone, Debug)]
pub struct Animator {
    clip: AnimationClip,
    time: f64,
    playing: bool,
    looping: bool,
}

impl Animator {
    /// New animator over `clip`, paused at t=0, looping.
    #[must_use]
    pub fn new(clip: AnimationClip) -> Self {
        Animator {
            clip,
            time: 0.0,
            playing: false,
            looping: true,
        }
    }

    /// Start playback from the current time.
    pub fn play(&mut self) {
        self.playing = true;
    }

    /// Pause playback.
    pub fn pause(&mut self) {
        self.playing = false;
    }

    /// Whether looping wraps at the clip duration.
    pub fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
    }

    /// Current playhead time.
    #[must_use]
    pub const fn time(&self) -> f64 {
        self.time
    }

    /// Whether playback is running.
    #[must_use]
    pub const fn is_playing(&self) -> bool {
        self.playing
    }

    /// Advance the playhead by `dt` (no-op while paused). Non-
    /// looping playback stops at the clip end; looping wraps.
    pub fn advance(&mut self, dt: f64) {
        if !self.playing {
            return;
        }
        self.time += if dt > 0.0 { dt } else { 0.0 };
        if self.time >= self.clip.duration {
            if self.looping {
                self.time %= self.clip.duration;
            } else {
                self.time = self.clip.duration;
                self.playing = false;
            }
        }
    }

    /// Joint-local poses for the current playhead.
    #[must_use]
    pub fn pose(&self, joints: &[Joint]) -> Vec<Mat4> {
        self.clip.pose(joints, self.time)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Vec3;

    /// Skeleton: root + child translated 2 on X.
    fn joints() -> Vec<Joint> {
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

    /// A clip waving the child joint 0 -> pi -> 0 over one second.
    fn wave_clip() -> AnimationClip {
        AnimationClip::new(
            "wave",
            1.0,
            vec![Track {
                joint: 1,
                axis: Axis::Z,
                channel: Channel::new(vec![(0.0, 0.0), (0.5, std::f64::consts::PI), (1.0, 0.0)]),
            }],
        )
    }

    #[test]
    fn tracked_joint_rotates_by_channel() {
        let clip = wave_clip();
        let pose = clip.pose(&joints(), 0.25);
        // At t=0.25 the channel reads pi/2: the child local carries
        // a 90-degree Z rotation after its bind translate.
        let child = pose.get(1).copied().unwrap_or_else(Mat4::identity);
        // A point 1 unit along +X of the child pivot (2,0,0) swings
        // to (2,1,0) under a 90-degree Z rotation about the pivot.
        let (x, y, _) = child
            .transform_point(Vec3::new(1.0, 0.0, 0.0))
            .unwrap_or((1.0, 1.0, 1.0));
        assert!(
            (x - 2.0).abs() < 1e-9 && (y - 1.0).abs() < 1e-9,
            "channel rotation applied about the pivot: ({x},{y})"
        );
    }

    #[test]
    fn untracked_joints_keep_bind_local() {
        let clip = wave_clip();
        let pose = clip.pose(&joints(), 0.5);
        let root = pose.first().copied().unwrap_or_else(Mat4::identity);
        assert_eq!(root, Mat4::identity(), "root has no track");
    }

    #[test]
    fn animator_loops() {
        let mut a = Animator::new(wave_clip());
        a.play();
        a.advance(0.4);
        assert!((a.time() - 0.4).abs() < 1e-9);
        a.advance(0.4);
        assert!((a.time() - 0.8).abs() < 1e-9);
        a.advance(0.4); // wraps: 0.8 + 0.4 = 1.2 -> 0.2
        assert!((a.time() - 0.2).abs() < 1e-9, "looped past duration");
    }

    #[test]
    fn animator_stops_at_end_without_looping() {
        let mut a = Animator::new(wave_clip());
        a.set_looping(false);
        a.play();
        a.advance(2.0);
        assert!((a.time() - 1.0).abs() < 1e-9, "clamped to duration");
        assert!(!a.is_playing(), "playback stopped at the end");
    }

    #[test]
    fn paused_animator_does_not_advance() {
        let mut a = Animator::new(wave_clip());
        a.advance(0.5);
        assert!((a.time() - 0.0).abs() < 1e-9, "paused: no advance");
        a.play();
        a.advance(0.25);
        assert!((a.time() - 0.25).abs() < 1e-9);
        a.pause();
        a.advance(0.25);
        assert!((a.time() - 0.25).abs() < 1e-9, "paused mid-playback");
    }

    #[test]
    fn playback_is_deterministic() {
        let run = || {
            let mut a = Animator::new(wave_clip());
            a.play();
            let js = joints();
            let mut out = Vec::new();
            for _ in 0..12 {
                a.advance(1.0 / 60.0);
                for m in a.pose(&js) {
                    out.extend(m.m);
                }
            }
            out
        };
        assert_eq!(run(), run(), "playback byte-identical across runs");
    }
}

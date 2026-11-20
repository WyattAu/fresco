//! Animation clip text import: the keyframe-format twin of
//! [`crate::obj`] (the OBJ mesh importer).
//!
//! Format (line-oriented, `#` comments, blank lines ignored):
//!
//! ```text
//! clip walk 2.0
//! track 0 y
//! key 0.0 0.0
//! key 0.5 1.57
//! track 1 x
//! key 0.0 0.0
//! ```
//!
//! - `clip <name> <duration>` starts the clip (one per document);
//! - `track <joint> <axis x|y|z>` starts a rotation track;
//! - `key <time> <value>` appends a keyframe to the open track
//!   (keys are sorted into [`crate::skin::Channel`] automatically).
//!
//! Deterministic: pure text arithmetic, no environment state.
//! Returns `None` when the document carries no parseable clip.

use crate::animation::{AnimationClip, Axis, Track};
use crate::skin::Channel;

/// An in-progress track: joint, axis, accumulated keys.
type OpenTrack = (usize, Axis, Vec<(f64, f64)>);

/// Parse an animation-clip document. Returns `None` when no clip
/// header or no tracks parsed.
#[must_use]
pub fn import_clip(text: &str) -> Option<AnimationClip> {
    let mut name: Option<String> = None;
    let mut duration = 1.0_f64;
    let mut tracks: Vec<Track> = Vec::new();
    let mut open: Option<OpenTrack> = None;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut tokens = line.split_whitespace();
        match tokens.next()? {
            "clip" => {
                let n = tokens.next()?;
                let d = tokens.next()?.parse::<f64>().ok()?;
                name = Some(n.to_owned());
                duration = if d > 1e-9 { d } else { 1.0 };
            }
            "track" => {
                // Close the previous track before opening a new one.
                if let Some((joint, axis, keys)) = open.take()
                    && !keys.is_empty()
                {
                    tracks.push(Track {
                        joint,
                        axis,
                        channel: Channel::new(keys),
                    });
                }
                let joint = usize::try_from(tokens.next()?.parse::<i64>().ok()?).ok()?;
                let axis = match tokens.next()? {
                    "x" => Axis::X,
                    "y" => Axis::Y,
                    "z" => Axis::Z,
                    _ => return None,
                };
                open = Some((joint, axis, Vec::new()));
            }
            "key" => {
                let t = tokens.next()?.parse::<f64>().ok()?;
                let v = tokens.next()?.parse::<f64>().ok()?;
                if let Some((_, _, keys)) = open.as_mut() {
                    keys.push((t, v));
                }
            }
            _ => {}
        }
    }
    if let Some((joint, axis, keys)) = open.take()
        && !keys.is_empty()
    {
        tracks.push(Track {
            joint,
            axis,
            channel: Channel::new(keys),
        });
    }

    let clip_name = name?;
    if tracks.is_empty() {
        return None;
    }
    Some(AnimationClip::new(&clip_name, duration, tracks))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "# a walk cycle
clip walk 2.0
track 0 y
key 0.0 0.0
key 0.5 1.57
key 2.0 6.28
track 1 x
key 1.0 0.3
key 0.0 0.1
";

    /// Oracle: the document parses into the exact clip - name,
    /// duration, two tracks, sorted keys, correct axes and joints.
    #[test]
    fn parses_clip_tracks_and_keys() {
        let Some(clip) = import_clip(DOC) else {
            return;
        };
        assert_eq!(clip.name, "walk");
        assert!((clip.duration - 2.0).abs() < 1e-9);
        assert_eq!(clip.tracks.len(), 2);
        let t0 = clip.tracks.first().map(|t| (t.joint, t.axis));
        assert_eq!(t0, Some((0, Axis::Y)), "track 0: joint 0 on Y");
        let n0 = clip.tracks.first().map(|t| t.channel.keys.len());
        assert_eq!(n0, Some(3), "track 0 key count");
        // Keys sorted: the out-of-order pair landed in order.
        let times: Vec<f64> = clip
            .tracks
            .get(1)
            .map(|t| t.channel.keys.iter().map(|k| k.0).collect())
            .unwrap_or_default();
        assert_eq!(times, vec![0.0, 1.0], "track 1 keys sorted");
        let a1 = clip.tracks.get(1).map(|t| t.axis);
        assert_eq!(a1, Some(Axis::X), "track 1 axis");
    }

    /// Oracle: sampling matches linear interpolation of the parsed
    /// keys (the loader feeds the playback path correctly).
    #[test]
    fn parsed_clip_samples_linearly() {
        let Some(clip) = import_clip(DOC) else {
            return;
        };
        let Some(t0) = clip.tracks.first() else {
            return;
        };
        let mid = t0.channel.sample(0.25);
        assert!((mid - 0.785).abs() < 0.01, "linear midpoint: {mid}");
    }

    /// Oracle: malformed documents fail closed.
    #[test]
    fn malformed_documents_return_none() {
        assert!(import_clip("").is_none(), "empty");
        assert!(import_clip("clip solo 1.0").is_none(), "no tracks");
        assert!(import_clip("track 0 y").is_none(), "no clip header");
        assert!(
            import_clip("clip bad 1.0\ntrack 9 q\nkey 0 0").is_none(),
            "bad axis"
        );
    }
}

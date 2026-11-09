//! Input abstraction: platform events translated to semantic
//! actions, plus a camera controller wiring drag-pan, wheel zoom,
//! and keyboard nudges.
//!
//! The action enum and controller are plain data — testable
//! headless. A thin winit adapter maps window events onto actions
//! when a display session is available.

use path_kit::Vec2;
use scene_kit::Camera;

/// A semantic input action (platform-independent).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InputAction {
    /// Pointer button pressed at `pos` (screen pixels).
    PointerDown {
        /// Screen position.
        pos: Vec2,
        /// Button index (0 = primary).
        button: u8,
    },
    /// Pointer moved to `pos`.
    PointerMove {
        /// Screen position.
        pos: Vec2,
    },
    /// Pointer button released at `pos`.
    PointerUp {
        /// Screen position.
        pos: Vec2,
        /// Button index (0 = primary).
        button: u8,
    },
    /// Scroll wheel tick at `pos`; positive `delta` zooms in.
    Scroll {
        /// Wheel delta.
        delta: f64,
        /// Cursor position (zoom anchor).
        pos: Vec2,
    },
    /// A named key changed state. `key` is a semantic name
    /// ("pan-left", "pan-right", "pan-up", "pan-down",
    /// "zoom-in", "zoom-out", "reset").
    Key {
        /// Semantic key name.
        key: &'static str,
        /// True on press, false on release.
        pressed: bool,
    },
}

/// Keyboard-nudge distance as a fraction of the viewport size.
const KEY_PAN_FRACTION: f64 = 0.05;
/// Keyboard zoom factor per press.
const KEY_ZOOM_FACTOR: f64 = 1.25;
/// Wheel zoom factor per tick of delta.
const WHEEL_ZOOM_FACTOR: f64 = 1.15;

/// Drives a [`Camera`] from input actions: primary-button drag
/// pans, wheel zooms around the cursor, semantic keys nudge.
#[derive(Clone, Copy, Debug)]
pub struct CameraController {
    /// The driven camera.
    pub camera: Camera,
    /// Button index that pans (default 0 = primary).
    pub pan_button: u8,
    drag_start: Option<Vec2>,
    camera_start: Option<Vec2>,
}

impl CameraController {
    /// Controller with an identity camera; primary button pans.
    #[must_use]
    pub fn new() -> Self {
        CameraController {
            camera: Camera::identity(),
            pan_button: 0,
            drag_start: None,
            camera_start: None,
        }
    }

    /// Feed one action; returns `true` when the camera changed.
    pub fn handle(&mut self, action: &InputAction, view: Vec2) -> bool {
        match *action {
            InputAction::PointerDown { pos, button } if button == self.pan_button => {
                self.drag_start = Some(pos);
                self.camera_start = Some(self.camera.center);
                false
            }
            InputAction::PointerMove { pos } => {
                match (self.drag_start, self.camera_start) {
                    (Some(start), Some(cam_start)) => {
                        // Drag delta in screen space; camera center
                        // moves opposite (grab-the-world feel).
                        let center = Vec2::new(
                            cam_start.x - (pos.x - start.x) / self.camera.zoom,
                            cam_start.y - (pos.y - start.y) / self.camera.zoom,
                        );
                        if center != self.camera.center {
                            self.camera.center = center;
                            true
                        } else {
                            false
                        }
                    }
                    _ => false,
                }
            }
            InputAction::PointerUp { .. } => {
                let was = self.drag_start.is_some();
                self.drag_start = None;
                self.camera_start = None;
                was
            }
            InputAction::Scroll { delta, pos } if delta != 0.0 => {
                let steps = delta.abs();
                let factor = WHEEL_ZOOM_FACTOR.powf(steps);
                let factor = if delta > 0.0 { factor } else { 1.0 / factor };
                self.camera.zoom_at(view, pos, factor);
                true
            }
            InputAction::Key { key, pressed: true } => {
                let pan = view * KEY_PAN_FRACTION / self.camera.zoom;
                match key {
                    "pan-left" => {
                        self.camera.center =
                            Vec2::new(self.camera.center.x - pan.x, self.camera.center.y);
                        true
                    }
                    "pan-right" => {
                        self.camera.center =
                            Vec2::new(self.camera.center.x + pan.x, self.camera.center.y);
                        true
                    }
                    "pan-up" => {
                        self.camera.center =
                            Vec2::new(self.camera.center.x, self.camera.center.y - pan.y);
                        true
                    }
                    "pan-down" => {
                        self.camera.center =
                            Vec2::new(self.camera.center.x, self.camera.center.y + pan.y);
                        true
                    }
                    "zoom-in" => {
                        let c = Vec2::new(view.x * 0.5, view.y * 0.5);
                        self.camera.zoom_at(view, c, KEY_ZOOM_FACTOR);
                        true
                    }
                    "zoom-out" => {
                        let c = Vec2::new(view.x * 0.5, view.y * 0.5);
                        self.camera.zoom_at(view, c, 1.0 / KEY_ZOOM_FACTOR);
                        true
                    }
                    "reset" => {
                        self.camera = Camera::identity();
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }
}

impl Default for CameraController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use path_kit::Vec2;

    const VIEW: Vec2 = Vec2::new(800.0, 600.0);

    #[test]
    fn drag_pans_by_zoomed_delta() {
        let mut c = CameraController::new();
        c.camera = Camera::new(Vec2::ZERO, 2.0);
        assert!(!c.handle(
            &InputAction::PointerDown {
                pos: Vec2::new(400.0, 300.0),
                button: 0
            },
            VIEW
        ));
        assert!(c.handle(
            &InputAction::PointerMove {
                pos: Vec2::new(300.0, 300.0)
            },
            VIEW
        ));
        // Dragged 100 px left at zoom 2 -> world center +50.
        assert!(
            (c.camera.center.x - 50.0).abs() < 1e-9,
            "drag left pans world right: {}",
            c.camera.center.x
        );
        assert!(c.handle(
            &InputAction::PointerUp {
                pos: Vec2::new(300.0, 300.0),
                button: 0
            },
            VIEW
        ));
        // After release, moves no longer pan.
        assert!(!c.handle(
            &InputAction::PointerMove {
                pos: Vec2::new(100.0, 100.0)
            },
            VIEW
        ));
        assert!((c.camera.center.x - 50.0).abs() < 1e-9);
    }

    #[test]
    fn wheel_zoom_keeps_cursor_anchor() {
        let mut c = CameraController::new();
        let cursor = Vec2::new(650.0, 200.0);
        let before = c.camera.screen_point(VIEW, cursor);
        assert!(c.handle(
            &InputAction::Scroll {
                delta: 1.0,
                pos: cursor
            },
            VIEW
        ));
        let after = c.camera.screen_point(VIEW, cursor);
        assert!(
            (before.x - after.x).abs() < 1e-6 && (before.y - after.y).abs() < 1e-6,
            "world under cursor stationary: {before:?} vs {after:?}"
        );
        assert!(c.camera.zoom > 1.0, "scroll-in zoomed: {}", c.camera.zoom);
    }

    #[test]
    fn keys_nudge_and_reset() {
        let mut c = CameraController::new();
        let cx0 = c.camera.center.x;
        assert!(c.handle(
            &InputAction::Key {
                key: "pan-right",
                pressed: true
            },
            VIEW
        ));
        assert!(c.camera.center.x > cx0, "pan-right moved center");
        assert!(c.handle(
            &InputAction::Key {
                key: "zoom-in",
                pressed: true
            },
            VIEW
        ));
        assert!(c.camera.zoom > 1.0);
        assert!(c.handle(
            &InputAction::Key {
                key: "reset",
                pressed: true
            },
            VIEW
        ));
        assert_eq!(c.camera, Camera::identity(), "reset restores identity");
    }

    #[test]
    fn secondary_button_does_not_pan() {
        let mut c = CameraController::new();
        assert!(!c.handle(
            &InputAction::PointerDown {
                pos: Vec2::new(400.0, 300.0),
                button: 2
            },
            VIEW
        ));
        assert!(!c.handle(
            &InputAction::PointerMove {
                pos: Vec2::new(100.0, 100.0)
            },
            VIEW
        ));
        assert_eq!(c.camera.center, Vec2::ZERO, "secondary drag ignored");
    }
}

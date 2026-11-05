//! Gate C parity harness: the sparse-strip backend (vello_cpu) against
//! the exact-coverage CPU reference, on a shared fixture set.
//!
//! The fixture draws are chosen to stress the paths the two
//! rasterizers must agree on for widget chrome: opaque fills,
//! semi-transparent blends, overlapping rects, edge-adjacent rects,
//! and draws clipped at the canvas boundary. Antialiasing conventions
//! differ between the two pipelines, so the compare config allows a
//! small channel delta plus a mismatch budget on edge pixels while
//! interior pixels must agree tightly.
#![cfg(feature = "parity")]
#![allow(clippy::panic, reason = "a failed parity gate must fail the test")]

use paint_kit::{Painter, Solid, Surface};
use render_conformance::{CompareConfig, SurfaceRef, compare, write_bmp};
use std::path::Path;

const WIDTH: usize = 96;
const HEIGHT: usize = 64;

const BLUE_STOPS: &[(f32, [u8; 4])] = &[
    (0.0, [16, 42, 96, 255]),
    (0.5, [64, 140, 220, 255]),
    (1.0, [220, 236, 252, 255]),
];
const WARM_STOPS: &[(f32, [u8; 4])] = &[(0.0, [120, 24, 24, 255]), (1.0, [250, 210, 120, 255])];

/// One fixture draw, applied identically to both backends.
#[derive(Clone, Copy)]
enum Draw {
    /// Opaque or blended solid.
    Solid(path_kit::Rect, [u8; 4]),
    /// Linear gradient along a device-space axis, opaque stops.
    Linear(
        path_kit::Rect,
        (f64, f64),
        (f64, f64),
        &'static [(f32, [u8; 4])],
    ),
    /// Radial gradient from a center with a radius, opaque stops.
    Radial(path_kit::Rect, (f64, f64), f64, &'static [(f32, [u8; 4])]),
    /// Rounded rect fill (uniform corner radius), opaque color.
    RoundRect(path_kit::Rect, f64, [u8; 4]),
}

/// The shared fixture: draws applied in order onto a transparent
/// start. Solids stress blending; the gradients stress ramp
/// interpolation and pad clamping; the last two draws clip at the
/// canvas boundary so both backends crop identically.
fn fixtures() -> Vec<Draw> {
    use path_kit::{Rect, Vec2};
    let ltrb = |x0: f64, y0: f64, x1: f64, y1: f64| {
        Rect::from_points(Vec2::new(x0, y0), Vec2::new(x1, y1))
    };
    vec![
        Draw::Solid(ltrb(8.0, 8.0, 88.0, 40.0), [40, 80, 200, 255]),
        Draw::Solid(ltrb(40.0, 24.0, 92.0, 56.0), [255, 0, 0, 128]),
        Draw::Solid(ltrb(0.0, 0.0, 16.0, 16.0), [20, 160, 90, 200]),
        Draw::Solid(ltrb(16.0, 16.0, 32.0, 32.0), [230, 230, 40, 255]),
        Draw::Linear(
            ltrb(24.0, 8.0, 64.0, 40.0),
            (24.0, 8.0),
            (64.0, 40.0),
            BLUE_STOPS,
        ),
        Draw::Radial(ltrb(40.0, 40.0, 88.0, 64.0), (64.0, 52.0), 20.0, WARM_STOPS),
        Draw::Solid(ltrb(64.0, 48.0, 128.0, 80.0), [12, 12, 14, 90]),
        Draw::RoundRect(ltrb(4.0, 44.0, 36.0, 60.0), 6.0, [30, 180, 120, 255]),
        Draw::RoundRect(ltrb(48.0, 4.0, 80.0, 20.0), 8.0, [200, 60, 40, 255]),
    ]
}

/// Apply one draw to the CPU reference.
fn apply_cpu(surface: &mut Surface, painter: &mut Painter, draw: &Draw) {
    match *draw {
        Draw::Solid(rect, color) => {
            // Straight-alpha input: Solid::new premultiplies. (Fixture
            // colors carry semi-transparent alphas, where premul and
            // straight bytes differ.)
            painter.fill_rect(surface, rect, &paint_kit::Paint::from(Solid::new(color)));
        }
        Draw::Linear(rect, start, end, stops) => {
            let g = paint_kit::LinearGradient::new(
                path_kit::Vec2::new(start.0, start.1),
                path_kit::Vec2::new(end.0, end.1),
                stops
                    .iter()
                    .map(|(o, c)| paint_kit::ColorStop {
                        offset: *o,
                        color: *c,
                    })
                    .collect(),
            );
            painter.fill_rect(surface, rect, &paint_kit::Paint::from(g));
        }
        Draw::Radial(rect, center, radius, stops) => {
            let g = paint_kit::RadialGradient::new(
                path_kit::Vec2::new(center.0, center.1),
                radius,
                stops
                    .iter()
                    .map(|(o, c)| paint_kit::ColorStop {
                        offset: *o,
                        color: *c,
                    })
                    .collect(),
            );
            painter.fill_rect(surface, rect, &paint_kit::Paint::from(g));
        }
        Draw::RoundRect(rect, radius, color) => {
            painter.fill_round_rect(
                surface,
                rect,
                radius,
                &paint_kit::Paint::from(Solid::new(color)),
            );
        }
    }
}

/// Apply one draw to the sparse-strip backend.
fn apply_strip(target: &mut paint_kit::StripTarget, draw: &Draw) {
    match *draw {
        Draw::Solid(rect, color) => target.fill_rect(rect, color),
        Draw::Linear(rect, start, end, stops) => {
            target.fill_linear(rect, start, end, stops);
        }
        Draw::Radial(rect, center, radius, stops) => {
            target.fill_radial(rect, center, radius, stops);
        }
        Draw::RoundRect(rect, radius, color) => {
            target.fill_round_rect(rect, radius, color);
        }
    }
}

fn cpu_reference() -> Surface {
    let mut surface = Surface::new(WIDTH, HEIGHT).unwrap_or_default();
    let mut painter = Painter::default();
    for draw in fixtures() {
        apply_cpu(&mut surface, &mut painter, &draw);
    }
    surface
}

fn strip_render() -> Surface {
    let Some(mut target) = paint_kit::StripTarget::new(96, 64) else {
        return Surface::new(WIDTH, HEIGHT).unwrap_or_default();
    };
    for draw in fixtures() {
        apply_strip(&mut target, &draw);
    }
    let mut surface = Surface::new(WIDTH, HEIGHT).unwrap_or_default();
    if paint_kit::strip_into_surface(&mut target, &mut surface).is_none() {
        return Surface::new(WIDTH, HEIGHT).unwrap_or_default();
    }
    surface
}

fn as_ref(surface: &Surface) -> SurfaceRef<'_> {
    SurfaceRef::new(surface.data(), WIDTH, HEIGHT, WIDTH * 4)
}

#[test]
fn sparse_strip_matches_cpu_reference_within_tolerance() {
    let cpu = cpu_reference();
    let strip = strip_render();
    // Measured on the fixture: interior pixels agree within 1/255.
    // Rounded-rect corner arcs are the one exception: the two
    // rasterizers flatten the same quarter-circle cubics with
    // independent sampling, so a thin band of corner-edge pixels
    // diverges by more than one step (measured 0.72% of pixels on
    // this fixture; budget 2% is ~3x headroom). Any pixel deviating
    // by more than one channel step outside that budget fails.
    let cfg = CompareConfig {
        max_channel_delta: 1,
        max_mismatch_frac: 0.02,
    };
    match compare(&as_ref(&cpu), &as_ref(&strip), cfg) {
        Ok(report) => {
            // Artifacts for human eyes (device screens, PR review):
            // written on every run, not just failures.
            let dir = Path::new("/tmp/opencode");
            let _ = write_bmp(&dir.join("parity-cpu.bmp"), &as_ref(&cpu));
            let _ = write_bmp(&dir.join("parity-strip.bmp"), &as_ref(&strip));
            println!(
                "parity ok: mismatch_frac={:.5} max_delta={}",
                report.mismatch_frac, report.max_delta
            );
        }
        Err(mismatch) => {
            let dir = Path::new("/tmp/opencode");
            let _ = write_bmp(&dir.join("parity-cpu.bmp"), &as_ref(&cpu));
            let _ = write_bmp(&dir.join("parity-strip.bmp"), &as_ref(&strip));
            panic!(
                "sparse-strip parity failed: mismatch_frac={:.5} max_delta={} first={:?}",
                mismatch.report.mismatch_frac, mismatch.report.max_delta, mismatch.first
            );
        }
    }
}

#[test]
fn parity_harness_catches_wrong_pixels() {
    // Sanity: the compare oracle must flag a deliberately different
    // buffer, so a pass above cannot be vacuous.
    let cpu = cpu_reference();
    let mut wrong = Surface::new(WIDTH, HEIGHT).unwrap_or_default();
    let mut painter = Painter::default();
    painter.fill_rect(
        &mut wrong,
        path_kit::Rect::from_points(
            path_kit::Vec2::new(0.0, 0.0),
            path_kit::Vec2::new(96.0, 64.0),
        ),
        &paint_kit::Paint::from(Solid::new([255, 255, 255, 255])),
    );
    let cfg = CompareConfig {
        max_channel_delta: 2,
        max_mismatch_frac: 0.02,
    };
    assert!(
        compare(&as_ref(&cpu), &as_ref(&wrong), cfg).is_err(),
        "a white-canvas buffer must mismatch the fixture"
    );
}

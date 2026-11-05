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

/// The shared fixture: (rect, straight rgba8 color) pairs, drawn in
/// order onto a transparent start. The last rect extends past the
/// canvas so both backends clip identically.
fn fixtures() -> Vec<(path_kit::Rect, [u8; 4])> {
    use path_kit::{Rect, Vec2};
    let ltrb = |x0: f64, y0: f64, x1: f64, y1: f64| {
        Rect::from_points(Vec2::new(x0, y0), Vec2::new(x1, y1))
    };
    vec![
        (ltrb(8.0, 8.0, 88.0, 40.0), [40, 80, 200, 255]),
        (ltrb(40.0, 24.0, 92.0, 56.0), [255, 0, 0, 128]),
        (ltrb(0.0, 0.0, 16.0, 16.0), [20, 160, 90, 200]),
        (ltrb(16.0, 16.0, 32.0, 32.0), [230, 230, 40, 255]),
        (ltrb(64.0, 48.0, 128.0, 80.0), [12, 12, 14, 90]),
    ]
}

fn cpu_reference() -> Surface {
    let mut surface = Surface::new(WIDTH, HEIGHT).unwrap_or_default();
    let mut painter = Painter::default();
    for (rect, color) in fixtures() {
        // Straight-alpha input: Solid::new premultiplies. (The fixture
        // colors carry semi-transparent alphas, where premul and
        // straight bytes differ.)
        painter.fill_rect(
            &mut surface,
            rect,
            &paint_kit::Paint::from(Solid::new(color)),
        );
    }
    surface
}

fn strip_render() -> Surface {
    let Some(mut target) = paint_kit::StripTarget::new(96, 64) else {
        return Surface::new(WIDTH, HEIGHT).unwrap_or_default();
    };
    for (rect, color) in fixtures() {
        target.fill_rect(rect, color);
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
    // Measured on the fixture: every pixel agrees within 1/255. Hold
    // that line: any pixel deviating by more than one channel step, or
    // any budget at all, fails the gate.
    let cfg = CompareConfig {
        max_channel_delta: 1,
        max_mismatch_frac: 0.0,
    };
    match compare(&as_ref(&cpu), &as_ref(&strip), cfg) {
        Ok(report) => {
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

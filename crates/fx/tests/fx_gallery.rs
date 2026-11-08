//! fx golden gallery: all shipped components rendered into one grid,
//! determinism-asserted, FNV-golden-hashed.
//!
//! The gallery is the capability proof: if a component renders here,
//! its family works in fresco's paint model.

use fx::{CountUp, DotPattern, FxRand, GridPattern, ScrambledText, ShimmerText, StarField};
use paint_kit::{Painter, Surface};

/// Render every component into one 2x3 catalog grid at `t`.
fn render_catalog(t: f64) -> paint_kit::Surface {
    let mut surface = Surface::new(960, 420).unwrap_or_default();
    let mut painter = Painter::default();
    let mut fx = fx::Fx::new(&mut surface, &mut painter, t);

    // Dark backdrop.
    fx.painter.fill_rect(
        fx.surface,
        paint_kit::Rect::from_size(
            paint_kit::Vec2::new(0.0, 0.0),
            paint_kit::Vec2::new(960.0, 420.0),
        ),
        &paint_kit::Paint::from(paint_kit::Solid::new([16, 17, 22, 255])),
    );

    let cells: [(f64, f64); 9] = [
        (0.0, 0.0),
        (320.0, 0.0),
        (640.0, 0.0),
        (0.0, 140.0),
        (320.0, 140.0),
        (640.0, 140.0),
        (0.0, 280.0),
        (320.0, 280.0),
        (640.0, 280.0),
    ];
    let cell_w = 300.0_f64;
    let cell_h = 130.0_f64;

    // Row 1: backgrounds.
    GridPattern::new().draw(&mut fx, 960, 420);
    DotPattern::new().draw(&mut fx, 960, 420);
    StarField::new().draw(&mut fx, 960, 420);

    // Panel fills so row 2 and 3 components sit on cards.
    for (x, y) in [(0.0, 140.0), (320.0, 140.0), (640.0, 140.0)] {
        fx.painter.fill_rect(
            fx.surface,
            paint_kit::Rect::from_size(
                paint_kit::Vec2::new(x + 4.0, y + 4.0),
                paint_kit::Vec2::new(cell_w - 8.0, cell_h - 8.0),
            ),
            &paint_kit::Paint::from(paint_kit::Solid::new([24, 26, 34, 255])),
        );
    }

    // Row 3: text effects.
    CountUp::new(42.0).draw(&mut fx, cells[6].0 + 12.0, cells[6].1 + 20.0);
    ScrambledText::new("fresco fx").draw(&mut fx, cells[7].0 + 12.0, cells[7].1 + 20.0);
    ShimmerText::new("solid-fx capability").draw(&mut fx, cells[8].0 + 12.0, cells[8].1 + 20.0);

    surface
}

fn fnv1a(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in data {
        h ^= u64::from(*byte);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

#[test]
fn fx_gallery_catalog_artifact_written() {
    // Device-screen artifact: the full catalog at t = 1.0 s.
    let surface = render_catalog(1.0);
    let dir = std::path::Path::new("/tmp/opencode");
    std::fs::create_dir_all(dir).ok();
    std::fs::write(dir.join("fx-catalog.rgba"), surface.data()).ok();
}

#[test]
fn fx_gallery_is_deterministic() {
    let a = render_catalog(1.0);
    let b = render_catalog(1.0);
    assert_eq!(
        a.data(),
        b.data(),
        "fx catalog must render deterministically"
    );
}

#[test]
fn fx_gallery_matches_golden_hashes() {
    // Regenerate with: GATE_FX_REBASELINE=1 --nocapture (recorded by
    // hand into GOLDEN_* below).
    let a = render_catalog(1.0);
    let mut alpha = Vec::new();
    let mut rgb = Vec::new();
    for (i, b) in a.data().iter().enumerate() {
        if i % 4 == 3 {
            alpha.push(*b);
        } else {
            rgb.push(*b);
        }
    }
    let ha = fnv1a(&alpha);
    let hr = fnv1a(&rgb);
    const GOLDEN_T1_ALPHA: u64 = 0x9258_5da8_ff74_4e25;
    const GOLDEN_T1_RGB: u64 = 0xd8f9_8447_7f34_5bbc;
    if GOLDEN_T1_ALPHA == 0 {
        println!("REBASELINE alpha {ha:#018x} rgb {hr:#018x}");
        return;
    }
    assert_eq!(ha, GOLDEN_T1_ALPHA, "alpha hash drifted");
    assert_eq!(hr, GOLDEN_T1_RGB, "rgb hash drifted");
}

#[test]
fn fx_gallery_time_changes_output() {
    let early = render_catalog(0.1);
    let late = render_catalog(5.5);
    let differing = early
        .data()
        .iter()
        .zip(late.data().iter())
        .filter(|(a, b)| a != b)
        .count();
    assert!(
        differing > 0,
        "animated components must change output over time"
    );
}

#[test]
fn fx_rand_is_deterministic_across_instances() {
    let mut a = FxRand::new(0xFEED);
    let mut b = FxRand::new(0xFEED);
    for _ in 0..32 {
        assert_eq!(a.next_u64(), b.next_u64());
    }
}

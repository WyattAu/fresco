//! fx golden gallery: all shipped components rendered into one grid,
//! determinism-asserted, FNV-golden-hashed.
//!
//! The gallery is the capability proof: if a component renders here,
//! its family works in fresco's paint model.

use fx::{
    BarChart, CountUp, DotPattern, Fx, FxRand, GridPattern, LineChart, ScrambledText, ShimmerText,
    Sparkline, StarField,
};
use paint_kit::{Painter, Surface};

/// Render every component into one 2x3 catalog grid at `t`.
fn render_catalog(t: f64) -> paint_kit::Surface {
    let mut surface = Surface::new(960, 420).unwrap_or_default();
    let mut painter = Painter::default();
    let mut fx = Fx::new(&mut surface, &mut painter, t);

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

    // Row 2: charts.
    Sparkline::new(vec![3.0, 5.0, 4.0, 8.0, 6.0, 9.0, 7.0, 12.0, 10.0, 14.0]).draw(
        &mut fx,
        paint_kit::Rect::from_size(
            paint_kit::Vec2::new(cells[3].0 + 8.0, cells[3].1 + 8.0),
            paint_kit::Vec2::new(cell_w - 16.0, cell_h - 16.0),
        ),
        t,
    );
    BarChart::new(vec![4.0, 9.0, 6.0, 12.0, 7.0, 10.0, 5.0]).draw(
        &mut fx,
        paint_kit::Rect::from_size(
            paint_kit::Vec2::new(cells[4].0 + 8.0, cells[4].1 + 8.0),
            paint_kit::Vec2::new(cell_w - 16.0, cell_h - 16.0),
        ),
        t,
    );
    LineChart::new(vec![5.0, 7.0, 4.0, 9.0, 6.0, 11.0, 8.0, 6.0]).draw(
        &mut fx,
        paint_kit::Rect::from_size(
            paint_kit::Vec2::new(cells[5].0 + 8.0, cells[5].1 + 8.0),
            paint_kit::Vec2::new(cell_w - 16.0, cell_h - 16.0),
        ),
        t,
    );

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

fn render_charts_catalog(t: f64) -> paint_kit::Surface {
    let mut surface = paint_kit::Surface::new(1280, 900).unwrap_or_default();
    let mut painter = paint_kit::Painter::default();
    let mut fx = Fx::new(&mut surface, &mut painter, t);
    fx.painter.fill_rect(
        fx.surface,
        paint_kit::Rect::from_size(
            paint_kit::Vec2::new(0.0, 0.0),
            paint_kit::Vec2::new(1280.0, 900.0),
        ),
        &paint_kit::Paint::from(paint_kit::Solid::new([16, 17, 22, 255])),
    );
    let cell = |col: usize, row: usize| -> paint_kit::Rect {
        paint_kit::Rect::from_size(
            paint_kit::Vec2::new(10.0 + col as f64 * 315.0, 10.0 + row as f64 * 290.0),
            paint_kit::Vec2::new(300.0, 280.0),
        )
    };
    let vals = [4.0, 9.0, 6.0, 12.0, 7.0, 10.0, 5.0, 13.0];
    fx::Sparkline::new(vals.to_vec()).draw(&mut fx, cell(0, 0), t);
    fx::BarChart::new(vals.to_vec()).draw(&mut fx, cell(1, 0), t);
    fx::LineChart::new(vals.to_vec()).draw(&mut fx, cell(2, 0), t);
    fx::AreaChart::new(vals.to_vec()).draw(&mut fx, cell(3, 0), t);
    fx::Multiline::new(vec![
        vals.to_vec(),
        vals.iter().map(|v| 14.0 - v).collect(),
        vals.iter().map(|v| v * 0.5 + 3.0).collect(),
    ])
    .draw(&mut fx, cell(0, 1), t);
    fx::Histogram::new(vals.to_vec()).draw(&mut fx, cell(1, 1), t);
    fx::StackedBar::new(vec![vals.to_vec(), vals.iter().map(|v| 14.0 - v).collect()]).draw(
        &mut fx,
        cell(2, 1),
        t,
    );
    fx::Candlestick::new(vec![
        (10.0, 14.0, 8.0, 12.0),
        (12.0, 16.0, 11.0, 15.0),
        (15.0, 15.0, 9.0, 10.0),
        (10.0, 18.0, 9.0, 17.0),
        (17.0, 20.0, 14.0, 15.0),
        (15.0, 19.0, 13.0, 18.0),
    ])
    .draw(&mut fx, cell(3, 1), t);
    fx::Heatmap::new(vec![
        vec![1.0, 3.0, 5.0, 7.0],
        vec![7.0, 5.0, 3.0, 1.0],
        vec![2.0, 6.0, 4.0, 8.0],
    ])
    .draw(&mut fx, cell(0, 2), t);
    fx::Correlation::new(vec![
        vec![1.0, -0.8, 0.3, 0.9],
        vec![-0.8, 1.0, -0.4, 0.2],
        vec![0.3, -0.4, 1.0, 0.6],
        vec![0.9, 0.2, 0.6, 1.0],
    ])
    .draw(&mut fx, cell(1, 2), t);
    fx::Treemap::new(vec![30.0, 22.0, 18.0, 14.0, 10.0]).draw(&mut fx, cell(2, 2), t);
    fx::Waterfall::new(vec![12.0, -5.0, 8.0, -3.0, 6.0]).draw(&mut fx, cell(3, 2), t);
    surface
}

fn chart_catalog_hashes(t: f64) -> (u64, u64, u64) {
    let surface = render_charts_catalog(t);
    let mut alpha = Vec::new();
    let mut rgb = Vec::new();
    for (i, b) in surface.data().iter().enumerate() {
        if i % 4 == 3 {
            alpha.push(*b);
        } else {
            rgb.push(*b);
        }
    }
    (fnv1a(&alpha), fnv1a(&rgb), fnv1a(surface.data()))
}

#[test]
fn chart_catalog_is_deterministic_and_golden() {
    let (a1, r1, full1) = chart_catalog_hashes(1.0);
    let (a2, r2, full2) = chart_catalog_hashes(1.0);
    assert_eq!(a1, a2, "determinism alpha");
    assert_eq!(r1, r2, "determinism rgb");
    assert_eq!(full1, full2, "determinism full");
    if std::env::var("GATE_FX_REBASELINE").as_deref() == Ok("1") {
        println!("CHART REBASELINE alpha {a1:#018x} rgb {r1:#018x} full {full1:#018x}");
        return;
    }
    // Constants recorded from the first deterministic run; update via
    // GATE_FX_REBASELINE=1 when the catalog intentionally changes.
    const A: u64 = 0x56cf_e4a0_55f4_e725;
    const R: u64 = 0xf037_1cf6_0008_38bb;
    const F: u64 = 0xa213_4ba3_3a48_56d1;
    assert_eq!(a1, A, "alpha drifted");
    assert_eq!(r1, R, "rgb drifted");
    assert_eq!(full1, F, "full drifted");
}

#[test]
fn chart_catalog_artifact_written() {
    let surface = render_charts_catalog(1.0);
    let dir = std::path::Path::new("/tmp/opencode");
    std::fs::create_dir_all(dir).ok();
    std::fs::write(dir.join("chart-catalog.rgba"), surface.data()).ok();
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

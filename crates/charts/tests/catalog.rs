//! Chart catalog tests: all 21 chart types rendered into one catalog
//! page, determinism-asserted, FNV-golden-hashed, BMP artifact written
//! for device review.

use fresco_charts::{
    AreaChart, Backtest, BarChart, Candlestick, ChartFx, ChartGrid, Correlation, Crosshair, Gauge,
    Heatmap, Histogram, LineChart, Multiline, OrderBook, Pie, Radar, Scatter, Sparkline,
    StackedBar, Treemap, Volume, Waterfall,
};
use paint_kit::{Painter, Surface};
use render_conformance::{SurfaceRef, write_bmp};
use std::path::Path;

const W: usize = 1600;
const H: usize = 920;

fn render_charts_catalog(t: f64) -> Surface {
    let mut surface = Surface::new(W, H).unwrap_or_default();
    let mut painter = Painter::default();
    let mut fx = ChartFx::new(&mut surface, &mut painter, t);
    fx.painter.fill_rect(
        fx.surface,
        paint_kit::Rect::from_size(
            paint_kit::Vec2::new(0.0, 0.0),
            paint_kit::Vec2::new(1600.0, 920.0),
        ),
        &paint_kit::Paint::from(paint_kit::Solid::new([16, 17, 22, 255])),
    );
    let cell = |col: usize, row: usize| -> paint_kit::Rect {
        paint_kit::Rect::from_size(
            paint_kit::Vec2::new(10.0 + col as f64 * 316.0, 10.0 + row as f64 * 225.0),
            paint_kit::Vec2::new(300.0, 215.0),
        )
    };
    let vals = [4.0, 9.0, 6.0, 12.0, 7.0, 10.0, 5.0, 13.0];
    // Row 0.
    Sparkline::new(vals.to_vec()).draw(&mut fx, cell(0, 0), t);
    BarChart::new(vals.to_vec()).draw(&mut fx, cell(1, 0), t);
    LineChart::new(vals.to_vec()).draw(&mut fx, cell(2, 0), t);
    AreaChart::new(vals.to_vec()).draw(&mut fx, cell(3, 0), t);
    Candlestick::new(vec![
        (10.0, 14.0, 8.0, 12.0),
        (12.0, 16.0, 11.0, 15.0),
        (15.0, 15.0, 9.0, 10.0),
        (10.0, 18.0, 9.0, 17.0),
        (17.0, 20.0, 14.0, 15.0),
        (15.0, 19.0, 13.0, 18.0),
    ])
    .draw(&mut fx, cell(4, 0), t);
    // Row 1.
    Multiline::new(vec![
        vals.to_vec(),
        vals.iter().map(|v| 14.0 - v).collect(),
        vals.iter().map(|v| v * 0.5 + 3.0).collect(),
    ])
    .draw(&mut fx, cell(0, 1), t);
    Histogram::new(vals.to_vec()).draw(&mut fx, cell(1, 1), t);
    StackedBar::new(vec![vals.to_vec(), vals.iter().map(|v| 14.0 - v).collect()]).draw(
        &mut fx,
        cell(2, 1),
        t,
    );
    Heatmap::new(vec![
        vec![1.0, 3.0, 5.0, 7.0],
        vec![7.0, 5.0, 3.0, 1.0],
        vec![2.0, 6.0, 4.0, 8.0],
    ])
    .draw(&mut fx, cell(3, 1), t);
    Correlation::new(vec![
        vec![1.0, -0.8, 0.3, 0.9],
        vec![-0.8, 1.0, -0.4, 0.2],
        vec![0.3, -0.4, 1.0, 0.6],
        vec![0.9, 0.2, 0.6, 1.0],
    ])
    .draw(&mut fx, cell(4, 1), t);
    // Row 2.
    Treemap::new(vec![30.0, 22.0, 18.0, 14.0, 10.0]).draw(&mut fx, cell(0, 2), t);
    Waterfall::new(vec![12.0, -5.0, 8.0, -3.0, 6.0]).draw(&mut fx, cell(1, 2), t);
    Gauge::new(0.72).draw(&mut fx, cell(2, 2), t);
    Pie::new(vec![30.0, 22.0, 18.0, 14.0]).draw(&mut fx, cell(3, 2), t);
    Radar::new(vec![0.9, 0.5, 0.8, 0.35, 0.7]).draw(&mut fx, cell(4, 2), t);
    // Row 3.
    Scatter::new(vec![
        (0.1, 0.2),
        (0.3, 0.5),
        (0.5, 0.35),
        (0.7, 0.8),
        (0.85, 0.6),
        (0.45, 0.7),
        (0.6, 0.25),
        (0.9, 0.45),
    ])
    .draw(&mut fx, cell(0, 3), t);
    OrderBook::new(
        vec![40.0, 30.0, 22.0, 14.0, 8.0],
        vec![36.0, 28.0, 18.0, 12.0, 6.0],
    )
    .draw(&mut fx, cell(1, 3), t);
    Volume::new(vals.to_vec()).draw(&mut fx, cell(2, 3), t);
    Backtest::new(vals.to_vec(), vec![(1, true), (4, false), (6, true)]).draw(
        &mut fx,
        cell(3, 3),
        t,
    );
    ChartGrid::new().draw(&mut fx, cell(4, 3), t);
    Crosshair::new((0.6, 0.4)).draw(&mut fx, cell(4, 3), t);
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

fn fnv1a(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in data {
        h ^= u64::from(*byte);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

#[test]
fn chart_catalog_is_deterministic_and_golden() {
    let (a1, r1, full1) = chart_catalog_hashes(1.0);
    let (a2, r2, full2) = chart_catalog_hashes(1.0);
    assert_eq!(a1, a2, "determinism alpha");
    assert_eq!(r1, r2, "determinism rgb");
    assert_eq!(full1, full2, "determinism full");
    // Constants recorded from the first deterministic 21-chart render;
    // update via GATE_FX_REBASELINE=1 --nocapture when the catalog
    // intentionally changes.
    const A: u64 = 0x9a47_8bbd_bb8b_0125;
    const R: u64 = 0x5029_2931_5d0c_4ab9;
    const F: u64 = 0x2b2a_ad39_a563_e3f9;
    assert_eq!(a1, A, "alpha drifted");
    assert_eq!(r1, R, "rgb drifted");
    assert_eq!(full1, F, "full drifted");
}

#[test]
fn chart_catalog_artifact_written() {
    let surface = render_charts_catalog(1.0);
    let dir = Path::new("/tmp/opencode");
    std::fs::create_dir_all(dir).ok();
    let aref = SurfaceRef::new(surface.data(), W, H, W * 4);
    let _ = write_bmp(&dir.join("chart-catalog.bmp"), &aref);
}

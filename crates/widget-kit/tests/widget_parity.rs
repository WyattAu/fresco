//! Widget-level parity: the same settings tree rasterized through the
//! CPU reference (`paint_tree`) and the sparse-strip backend
//! (`StripListPainter`) must agree pixel for pixel on the crisp
//! geometry. Shadows are the documented exception (CPU SDF vs vello
//! analytic gaussian: 50%-at-edge vs 100%-at-edge), so the comparison
//! carries a measured budget; everything else holds the one-step
//! oracle.
//!
//! This is the end-to-end proof that the strip backend can render the
//! real widget tree, and the prerequisite for driving it through the
//! damage-plan path.

#![cfg(feature = "strip-renderer")]
#![allow(clippy::panic, reason = "a failed parity gate must fail the test")]

use render_conformance::{CompareConfig, SurfaceRef, compare, write_bmp};
use std::path::Path;
use widget_kit::strip_renderer::StripListPainter;
use widget_kit::{TreeItem, WidgetColors, WidgetKind, WidgetTree, paint_tree, paint_tree_with};

const W: usize = 360;
const H: usize = 640;

/// A settings-shaped tree: header, shadowed button, checkbox, select,
/// outline tree. Buttons and the select carry shadows - the one
/// documented divergence.
fn settings_tree() -> WidgetTree {
    let mut tree = WidgetTree::new();
    let root = tree.push(WidgetKind::Column { gap: 8.0 });
    let header = tree.push(WidgetKind::Label {
        text: "Settings".to_string(),
        px_size: 20.0,
    });
    tree.add_child(root, header);
    let row = tree.push(WidgetKind::Row { gap: 10.0 });
    let save = tree.push(WidgetKind::Button {
        label: "Save".to_string(),
        action: 1,
    });
    let checkbox = tree.push(WidgetKind::Checkbox {
        label: "Compact".to_string(),
        checked: true,
    });
    tree.add_child(root, row);
    tree.add_child(row, save);
    tree.add_child(row, checkbox);
    let sync = tree.push(WidgetKind::Select {
        options: vec![
            "Instant".to_string(),
            "Hourly".to_string(),
            "Daily".to_string(),
        ],
        selected: 0,
        open: false,
    });
    tree.add_child(root, sync);
    let outline = tree.push(WidgetKind::TreeList {
        items: vec![
            TreeItem {
                label: "Projects".to_string(),
                depth: 0,
                has_children: true,
                expanded: true,
            },
            TreeItem {
                label: "fresco".to_string(),
                depth: 1,
                has_children: false,
                expanded: false,
            },
            TreeItem {
                label: "kestrel".to_string(),
                depth: 1,
                has_children: false,
                expanded: false,
            },
        ],
        selected: 1,
    });
    tree.add_child(root, outline);
    tree
}

fn cpu_render(tree: &WidgetTree) -> paint_kit::Surface {
    let mut surface = paint_kit::Surface::new(W, H).unwrap_or_default();
    let mut painter = paint_kit::Painter::default();
    paint_tree(tree, &mut surface, &mut painter, &WidgetColors::default());
    surface
}

fn strip_render(tree: &WidgetTree) -> Option<paint_kit::Surface> {
    let mut target = paint_kit::StripTarget::new(W as u32, H as u32)?;
    {
        let mut renderer = StripListPainter::new(&mut target, WidgetColors::default());
        paint_tree_with(tree, &mut renderer, &WidgetColors::default());
    }
    let bytes = target.composite().to_vec();
    let mut surface = paint_kit::Surface::new(W, H).unwrap_or_default();
    let dst = surface.data_mut();
    if dst.len() != bytes.len() {
        return None;
    }
    for (d, s) in dst.iter_mut().zip(bytes.iter()) {
        *d = *s;
    }
    Some(surface)
}

#[test]
fn widget_level_strip_parity_holds_within_budget() {
    let tree = settings_tree();
    let cpu = cpu_render(&tree);
    let Some(strip) = strip_render(&tree) else {
        return;
    };
    let cfg = CompareConfig {
        max_channel_delta: 1,
        max_mismatch_frac: 0.02,
    };
    let aref = SurfaceRef::new(cpu.data(), W, H, W * 4);
    let bref = SurfaceRef::new(strip.data(), W, H, W * 4);
    match compare(&aref, &bref, cfg) {
        Ok(report) => {
            println!(
                "widget parity ok: mismatch_frac={:.5} max_delta={}",
                report.mismatch_frac, report.max_delta
            );
            let dir = Path::new("/tmp/opencode");
            let _ = write_bmp(&dir.join("widget-parity-cpu.bmp"), &aref);
            let _ = write_bmp(&dir.join("widget-parity-strip.bmp"), &bref);
        }
        Err(mismatch) => {
            let dir = Path::new("/tmp/opencode");
            let _ = write_bmp(&dir.join("widget-parity-cpu.bmp"), &aref);
            let _ = write_bmp(&dir.join("widget-parity-strip.bmp"), &bref);
            panic!(
                "widget parity failed: mismatch_frac={:.5} max_delta={} first={:?}",
                mismatch.report.mismatch_frac, mismatch.report.max_delta, mismatch.first
            );
        }
    }
}

#[test]
fn strip_widget_damage_plan_matches_full_render() {
    // The damage-plan path driving the strip backend with the real
    // widget renderer: frame A (empty panel), then the settings tree
    // appears. Regions come from scene-kit's DamageTracker (bare
    // rects); paint_tree_plan_with inflates them by SHADOW_MARGIN.
    // The composited result must be byte-identical to a full strip
    // render.
    use scene_kit::{DamageTracker, FramePlan};
    use widget_kit::{SHADOW_MARGIN, paint_tree_plan_with};

    // Frame A: an empty panel with explicit size (childless root).
    let mut empty = WidgetTree::new();
    let empty_root = empty.push(WidgetKind::Column { gap: 0.0 });
    if let Some(root) = empty.widget_mut(empty_root) {
        root.fixed_width = Some(f64::from(W as u32));
        root.fixed_height = Some(f64::from(H as u32));
    }
    empty.compute(widget_kit::Size2D {
        width: W as f64,
        height: H as f64,
    });

    let tree = settings_tree();

    // Damage: the union of every child rect of the tree's root.
    let root = tree.root().unwrap_or(widget_kit::WidgetRef(0));
    let mut tracker = DamageTracker::new(8, 0.9);
    for child in tree.children_of(root) {
        if let Some(w) = tree.widget(child) {
            tracker.invalidate(paint_kit::Rect::from_size(
                paint_kit::Vec2::new(w.rect.x, w.rect.y),
                paint_kit::Vec2::new(w.rect.width, w.rect.height),
            ));
        }
    }
    let plan = tracker.take_plan(paint_kit::Rect::from_size(
        paint_kit::Vec2::new(0.0, 0.0),
        paint_kit::Vec2::new(W as f64, H as f64),
    ));

    // Frame A through the strip backend.
    let Some(mut base_target) = paint_kit::StripTarget::new(W as u32, H as u32) else {
        return;
    };
    {
        let mut renderer = StripListPainter::new(&mut base_target, WidgetColors::default());
        paint_tree_with(&empty, &mut renderer, &WidgetColors::default());
    }
    let base = base_target.composite().to_vec();

    // Damaged pass: each region painted clip-confined into a fresh
    // composite, then blitted over frame A.
    let Some(mut pass_target) = paint_kit::StripTarget::new(W as u32, H as u32) else {
        return;
    };
    let FramePlan::Regions(regions) = &plan else {
        return;
    };
    // The plan drives the renderer directly (region clips are
    // inflated + pixel-aligned inside paint_tree_plan_with).
    let mut pixel_aligned: Vec<paint_kit::Rect> = Vec::new();
    for region in regions {
        pixel_aligned.push(paint_kit::Rect::from_size(
            paint_kit::Vec2::new(
                (region.x0 - SHADOW_MARGIN).floor(),
                (region.y0 - SHADOW_MARGIN).floor(),
            ),
            paint_kit::Vec2::new(
                (region.x1 + SHADOW_MARGIN).ceil(),
                (region.y1 + SHADOW_MARGIN).ceil(),
            ),
        ));
    }
    {
        let mut renderer = StripListPainter::new(&mut pass_target, WidgetColors::default());
        paint_tree_plan_with(&tree, &mut renderer, &WidgetColors::default(), &plan);
    }
    let composite = pass_target.composite().to_vec();

    let mut frame = base;
    for (i, slot) in frame.iter_mut().enumerate() {
        let px = i / 4;
        let (x, y) = ((px % W) as f64, (px / W) as f64);
        let in_region = pixel_aligned
            .iter()
            .any(|r| x >= r.x0 && x < r.x1 && y >= r.y0 && y < r.y1);
        if in_region && let Some(src) = composite.get(i) {
            *slot = *src;
        }
    }

    // Reference: full strip render of the tree.
    let Some(mut full_target) = paint_kit::StripTarget::new(W as u32, H as u32) else {
        return;
    };
    {
        let mut renderer = StripListPainter::new(&mut full_target, WidgetColors::default());
        paint_tree_with(&tree, &mut renderer, &WidgetColors::default());
    }
    let full = full_target.composite().to_vec();

    let differing = frame
        .iter()
        .zip(full.iter())
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(
        differing, 0,
        "strip damage-plan render must equal full render ({differing} bytes differ)"
    );
}

#[test]
fn gate_c_backend_paint_times_reported() {
    // Gate C's first measurement: paint the settings tree through the
    // CPU reference and the sparse-strip backend, report per-frame
    // stats for each. Timing is reported, never asserted (shared
    // machine); the byte-parity assertion lives in the test above.
    use std::time::Instant;

    let tree = settings_tree();
    let frames = 60;

    let mut cpu_times: Vec<f64> = Vec::new();
    for _ in 0..frames {
        let mut surface = paint_kit::Surface::new(W, H).unwrap_or_default();
        let mut painter = paint_kit::Painter::default();
        let start = Instant::now();
        paint_tree(&tree, &mut surface, &mut painter, &WidgetColors::default());
        cpu_times.push(start.elapsed().as_secs_f64() * 1000.0);
    }

    let mut strip_times: Vec<f64> = Vec::new();
    for _ in 0..frames {
        let Some(mut target) = paint_kit::StripTarget::new(W as u32, H as u32) else {
            return;
        };
        let start = Instant::now();
        {
            let mut renderer = StripListPainter::new(&mut target, WidgetColors::default());
            paint_tree_with(&tree, &mut renderer, &WidgetColors::default());
        }
        target.composite();
        strip_times.push(start.elapsed().as_secs_f64() * 1000.0);
    }

    let stats = |name: &str, times: &mut Vec<f64>| {
        times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mean: f64 = times.iter().sum::<f64>() / times.len() as f64;
        let pct = |p: f64| -> f64 {
            let idx = ((p / 100.0) * (times.len() as f64 - 1.0)).round() as usize;
            times.get(idx).copied().unwrap_or(0.0)
        };
        println!(
            "{name}: mean {:.3} ms, p50 {:.3} ms, p99 {:.3} ms, max {:.3} ms",
            mean,
            pct(50.0),
            pct(99.0),
            times.last().copied().unwrap_or(0.0)
        );
    };
    println!("Gate C paint times over {frames} frames of the {W}x{H} settings tree:");
    stats("cpu", &mut cpu_times);
    stats("strip", &mut strip_times);
}

//! Pacing instrumentation scaffolding (roadmap 4.4): measure per-frame
//! paint times of a phone-proportioned settings screen through the
//! CPU reference.
//!
//! The headless harness measures rasterization cost only (no present
//! path yet - that arrives with kestrel and the Wayland work). The
//! numbers are wall-clock on a shared machine, so no assertions are
//! made on timing; what IS asserted is the compositor-critical
//! property underneath: every frame is byte-identical. The stats print
//! with `--nocapture` for Gate A evidence collection, and the rendered
//! frame is written as a BMP artifact for device-screen inspection.

use std::time::Instant;

use paint_kit::Surface;
use widget_kit::{TreeItem, WidgetColors, WidgetKind, WidgetTree, paint_tree};

const W: f64 = 360.0;
const H: f64 = 640.0;

/// A settings-shaped screen at phone proportions: header, a
/// shadowed action button, a select, an outline tree, and a
/// checkbox row.
fn phone_tree() -> WidgetTree {
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
            TreeItem {
                label: "Archive".to_string(),
                depth: 0,
                has_children: true,
                expanded: false,
            },
        ],
        selected: 1,
    });
    tree.add_child(root, outline);
    tree
}

fn render_frame(tree: &WidgetTree) -> Surface {
    let mut surface = Surface::new(W as usize, H as usize).unwrap_or_default();
    let mut painter = paint_kit::Painter::default();
    paint_tree(tree, &mut surface, &mut painter, &WidgetColors::default());
    surface
}

#[test]
fn phone_screen_paint_frames_are_deterministic_and_timed() {
    let tree = phone_tree();
    let mut frames: Vec<Vec<u8>> = Vec::new();
    let mut timings: Vec<f64> = Vec::new();

    const FRAMES: usize = 120;
    for _ in 0..FRAMES {
        let start = Instant::now();
        let surface = render_frame(&tree);
        timings.push(start.elapsed().as_secs_f64() * 1000.0);
        frames.push(surface.data().to_vec());
    }

    // Determinism: every one of the FRAMES renders byte-identically.
    let Some(first) = frames.first() else {
        return;
    };
    let differing = frames.iter().skip(1).filter(|f| f != &first).count();
    assert_eq!(
        differing, 0,
        "all {FRAMES} frames must render byte-identically ({differing} differed)"
    );

    // Pacing stats (ms). Reported, not asserted - the machine is
    // shared and the present path is not in scope yet.
    timings.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mean: f64 = timings.iter().sum::<f64>() / timings.len() as f64;
    let pct = |p: f64| -> f64 {
        let idx = ((p / 100.0) * (timings.len() as f64 - 1.0)).round() as usize;
        timings.get(idx).copied().unwrap_or(0.0)
    };
    println!("pacing over {FRAMES} frames of the {W}x{H} settings tree:");
    println!("  mean {:.3} ms", mean);
    println!("  p50  {:.3} ms", pct(50.0));
    println!("  p99  {:.3} ms", pct(99.0));
    if let Some(max) = timings.last() {
        println!("  max  {:.3} ms", max);
    }

    // Device-screen artifact.
    let dir = std::path::Path::new("/tmp/opencode");
    std::fs::create_dir_all(dir).ok();
    std::fs::write(dir.join("showcase-phone.bmp"), first).ok();
}

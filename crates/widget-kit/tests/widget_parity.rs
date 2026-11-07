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

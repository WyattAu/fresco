//! Widget gallery: every widget kind x every palette, rendered through
//! paint-kit and gated by render-conformance hashes (the estate's
//! golden-image claim, applied to our own controls).
//!
//! Determinism contract: two renders of the same case are byte-identical;
//! the FNV-1a hashes below are checked-in baselines. A rendering change
//! that moves pixels must update the table in the same commit (and say
//! why in the changelog).

use paint_kit::{Painter, Surface};
use widget_kit::{LogicalAt, Size2D, TreeItem, WidgetColors, WidgetKind, WidgetTree, paint_tree};

/// Palettes: dark default, light, high contrast.
fn palette(name: &str) -> WidgetColors {
    match name {
        "light" => WidgetColors {
            surface: [236, 238, 242, 255],
            text: [30, 34, 44, 255],
            button: [220, 224, 232, 255],
            focus_ring: [30, 100, 220, 255],
            pressed_overlay: [255, 255, 255, 90],
            checkbox_box: [210, 214, 224, 255],
            check: [20, 120, 60, 255],
        },
        "high-contrast" => WidgetColors {
            surface: [0, 0, 0, 255],
            text: [255, 255, 255, 255],
            button: [255, 255, 255, 255],
            focus_ring: [255, 220, 0, 255],
            pressed_overlay: [80, 80, 80, 120],
            checkbox_box: [255, 255, 255, 255],
            check: [0, 0, 0, 255],
        },
        _ => WidgetColors::default(),
    }
}

const PALETTES: [&str; 3] = ["dark", "light", "high-contrast"];

/// The gallery screen: a kestrel-shaped column.
fn gallery_tree() -> WidgetTree {
    let mut tree = WidgetTree::new();
    let root = tree.push(WidgetKind::Column { gap: 5.0 });
    let label = tree.push(WidgetKind::Label {
        text: "Inbox".to_string(),
        px_size: 14.0,
    });
    tree.add_child(root, label);
    let row = tree.push(WidgetKind::Row { gap: 8.0 });
    let button = tree.push(WidgetKind::Button {
        label: "Compose".to_string(),
        action: 1,
    });
    let checkbox = tree.push(WidgetKind::Checkbox {
        label: "Compact".to_string(),
        checked: true,
    });
    let field = tree.push(WidgetKind::TextField {
        placeholder: "Search".to_string(),
        field: ui_text_edit::TextField::new(ui_text_edit::StringBuffer::new()),
    });
    let spacer = tree.push(WidgetKind::Spacer);
    tree.add_child(root, row);
    tree.add_child(row, button);
    tree.add_child(row, checkbox);
    tree.add_child(row, field);
    tree.add_child(root, spacer);
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
                label: "Archive".to_string(),
                depth: 0,
                has_children: true,
                expanded: false,
            },
        ],
        selected: 1,
    });
    tree.add_child(root, outline);
    // Interaction state: focused button, pressed checkbox, typed text.
    tree.set_focus(Some(button));
    tree.compute(Size2D {
        width: 220.0,
        height: 130.0,
    });
    let cb_rect = tree.widget(checkbox).map(|w| w.rect).unwrap_or_default();
    tree.pointer_down(LogicalAt {
        x: cb_rect.x + 1.0,
        y: cb_rect.y + 1.0,
    });
    tree.pointer_up(LogicalAt {
        x: cb_rect.x + 1.0,
        y: cb_rect.y + 1.0,
    });
    tree.set_focus(Some(field));
    for ch in "kiwi".chars() {
        tree.key_char(ch);
    }
    tree
}

fn render_case(palette_name: &str) -> Surface {
    let tree = gallery_tree();
    let mut surface = Surface::new(220, 130).unwrap_or_else(unreachable_surface);
    let mut painter = Painter::default();
    paint_tree(&tree, &mut surface, &mut painter, &palette(palette_name));
    surface
}

fn unreachable_surface() -> Surface {
    match Surface::new(1, 1) {
        Some(s) => s,
        None => Surface::with_stride(1, 1, 4).unwrap_or_else(unreachable_surface),
    }
}

fn fnv1a(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// Checked-in baselines: (palette, alpha-channel hash, rgb hash).
fn baselines() -> Vec<(&'static str, u64, u64)> {
    vec![
        (PALETTES[0], GOLDEN_DARK_ALPHA, GOLDEN_DARK_RGB),
        (PALETTES[1], GOLDEN_LIGHT_ALPHA, GOLDEN_LIGHT_RGB),
        (PALETTES[2], GOLDEN_HC_ALPHA, GOLDEN_HC_RGB),
    ]
}

// Baseline hashes are filled in by the bootstrap test below; the table
// goes green after the first local run records them.
const GOLDEN_DARK_ALPHA: u64 = 0xdd50_b1ad_2cb8_384d;
const GOLDEN_DARK_RGB: u64 = 0xea3a_80db_5b09_ae80;
const GOLDEN_LIGHT_ALPHA: u64 = 0xdd50_b1ad_2cb8_384d;
const GOLDEN_LIGHT_RGB: u64 = 0x47ff_725b_8175_dd4c;
const GOLDEN_HC_ALPHA: u64 = 0xdd50_b1ad_2cb8_384d;
const GOLDEN_HC_RGB: u64 = 0x4827_c00d_cc84_1cd3;

#[test]
fn gallery_is_deterministic() {
    for name in PALETTES {
        let a = render_case(name);
        let b = render_case(name);
        let report = render_conformance::compare(
            &render_conformance::SurfaceRef::new(a.data(), a.width(), a.height(), a.stride()),
            &render_conformance::SurfaceRef::new(b.data(), b.width(), b.height(), b.stride()),
            render_conformance::CompareConfig::strict(),
        );
        assert!(
            report.is_ok(),
            "palette {name} must render deterministically"
        );
    }
}

#[test]
fn gallery_matches_golden_hashes() {
    for (name, want_alpha, want_rgb) in baselines() {
        let s = render_case(name);
        // Alpha channel hash (shape) and RGB hash (color) checked apart.
        let mut alpha_bytes = Vec::new();
        let mut rgb_bytes = Vec::new();
        for (i, b) in s.data().iter().enumerate() {
            match i % 4 {
                3 => alpha_bytes.push(*b),
                _ => rgb_bytes.push(*b),
            }
        }
        let got_alpha = fnv1a(&alpha_bytes);
        let got_rgb = fnv1a(&rgb_bytes);
        assert_eq!(got_alpha, want_alpha, "palette {name} alpha hash");
        assert_eq!(got_rgb, want_rgb, "palette {name} rgb hash");
    }
}

#[test]
fn gallery_bootstraps_baseline_hashes() {
    // Prints the table to paste into the constants above; keep as a test
    // so the numbers are one `cargo test -- --nocapture` away.
    for name in PALETTES {
        let s = render_case(name);
        let mut alpha_bytes = Vec::new();
        let mut rgb_bytes = Vec::new();
        for (i, b) in s.data().iter().enumerate() {
            match i % 4 {
                3 => alpha_bytes.push(*b),
                _ => rgb_bytes.push(*b),
            }
        }
        println!(
            "{name}: alpha {:#x}, rgb {:#x}",
            fnv1a(&alpha_bytes),
            fnv1a(&rgb_bytes)
        );
    }
}

#[test]
fn label_text_pixels_differ_per_character() {
    use widget_kit::paint_tree;
    let make = |text: &str| {
        let mut tree = WidgetTree::new();
        let root = tree.push(WidgetKind::Column { gap: 0.0 });
        let label = tree.push(WidgetKind::Label {
            text: text.to_string(),
            px_size: 20.0,
        });
        tree.add_child(root, label);
        tree.compute(widget_kit::Size2D {
            width: 80.0,
            height: 24.0,
        });
        let mut s = paint_kit::Surface::new(80, 24).unwrap_or_default();
        let mut painter = paint_kit::Painter::default();
        paint_tree(&tree, &mut s, &mut painter, &WidgetColors::default());
        s
    };
    // Compare via FNV-1a so a failure reports one hash per surface
    // instead of dumping the full pixel buffer.
    let hash = |surface: &paint_kit::Surface| -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in surface.data() {
            h ^= u64::from(*byte);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        h
    };
    let a = make("A");
    let b = make("B");
    let c = make("C");
    assert_ne!(hash(&a), hash(&b), "A and B pixels must differ");
    assert_ne!(hash(&a), hash(&c), "A and C pixels must differ");
    assert_ne!(hash(&b), hash(&c), "B and C pixels must differ");
    // Blank differs from any character.
    let blank = make(" ");
    assert_ne!(hash(&a), hash(&blank), "text must differ from blank");
}

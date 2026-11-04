//! Theme-driven gallery: WCAG-audited token variants rendered per widget.
//!
//! The flagship gate (REQ-K9): the seed graph audits clean; every derived
//! variant (Light, HighContrast, Monochrome, CVD simulations) audits
//! clean; every variant renders deterministically through the stock
//! renderer; per-variant pixel hashes are recorded so palette drift is
//! visible in the same commit.

use std::vec::Vec;
use theme_kit::{Variant, audit};
use widget_kit::theme::{colors_from_graph, seed_graph, variant_graph};
use widget_kit::{WidgetColors, WidgetKind, WidgetTree, paint_tree};

use paint_kit::{Painter, Surface};

fn themed_tree() -> WidgetTree {
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
    tree.add_child(root, row);
    tree.add_child(row, button);
    tree.add_child(row, checkbox);
    tree.add_child(row, field);
    tree.set_focus(Some(field));
    for ch in "kiwi".chars() {
        tree.key_char(ch);
    }
    tree
}

fn render(colors: &WidgetColors) -> Surface {
    let tree = themed_tree();
    let mut surface = Surface::new(220, 110).unwrap_or_else(|| {
        Surface::with_stride(1, 1, 4).unwrap_or_else(|| match Surface::new(1, 1) {
            Some(s) => s,
            None => unreachable!("1x1 surface cannot overflow"),
        })
    });
    let mut painter = Painter::default();
    paint_tree(&tree, &mut surface, &mut painter, colors);
    surface
}

fn hashes(s: &Surface) -> (u64, u64) {
    let mut alpha_bytes = Vec::new();
    let mut rgb_bytes = Vec::new();
    for (i, b) in s.data().iter().enumerate() {
        match i % 4 {
            3 => alpha_bytes.push(*b),
            _ => rgb_bytes.push(*b),
        }
    }
    let (mut ah, mut rh) = (0_u64, 0_u64);
    for b in alpha_bytes {
        ah ^= u64::from(b);
        ah = ah.wrapping_mul(0x0100_0000_01b3);
    }
    for b in rgb_bytes {
        rh ^= u64::from(b);
        rh = rh.wrapping_mul(0x0100_0000_01b3);
    }
    (ah, rh)
}

const CASES: [(&str, Variant); 5] = [
    ("seed-dark", Variant::Dark),
    ("light", Variant::Light),
    ("high-contrast", Variant::HighContrast),
    ("monochrome", Variant::Monochrome),
    (
        "cvd-deuteranopia",
        Variant::Cvd(color_kit::CvdKind::Deuteranopia),
    ),
];

#[test]
fn seed_and_every_variant_audit_clean() {
    let seed = seed_graph();
    let gate = audit(&seed, None);
    if let Err(e) = &gate {
        println!("seed gate failure: {e}");
    }
    assert!(gate.is_ok(), "seed must pass the WCAG gate");
    for (name, variant) in CASES {
        let derived_result = variant_graph(&seed, variant);
        if let Err(e) = &derived_result {
            println!("variant {name} derivation failed: {e:?}");
        }
        assert!(
            derived_result.is_ok(),
            "variant {name} derivation must succeed: {:?}",
            derived_result.as_ref().err()
        );
        let derived = match derived_result {
            Ok(d) => d,
            // Assert above already failed the test; the seed keeps the
            // compiler happy without panicking.
            Err(_) => seed.clone(),
        };
        let cvd = match variant {
            Variant::Cvd(kind) => Some(kind),
            _ => None,
        };
        let gate = audit(&derived, cvd);
        if let Err(e) = &gate {
            println!("variant {name} gate failure: {e}");
        }
        assert!(gate.is_ok(), "variant {name} must audit clean");
    }
}

#[test]
fn themed_render_is_deterministic_per_variant() {
    let seed = seed_graph();
    for (name, variant) in CASES {
        let derived_result = variant_graph(&seed, variant);
        if let Err(e) = &derived_result {
            println!("variant {name} derivation failed: {e:?}");
        }
        assert!(
            derived_result.is_ok(),
            "variant {name} derivation must succeed: {:?}",
            derived_result.as_ref().err()
        );
        let derived = match derived_result {
            Ok(d) => d,
            // Assert above already failed the test; the seed keeps the
            // compiler happy without panicking.
            Err(_) => seed.clone(),
        };
        let colors = colors_from_graph(&derived);
        let a = render(&colors);
        let b = render(&colors);
        let report = render_conformance::compare(
            &render_conformance::SurfaceRef::new(a.data(), a.width(), a.height(), a.stride()),
            &render_conformance::SurfaceRef::new(b.data(), b.width(), b.height(), b.stride()),
            render_conformance::CompareConfig::strict(),
        );
        assert!(
            report.is_ok(),
            "variant {name} must render deterministically"
        );
    }
}

#[test]
fn themed_variants_differ_and_cover_pixels() {
    let seed = seed_graph();
    let mut seen: Vec<(u64, u64)> = Vec::new();
    for (name, variant) in CASES {
        let derived_result = variant_graph(&seed, variant);
        if let Err(e) = &derived_result {
            println!("variant {name} derivation failed: {e:?}");
        }
        assert!(
            derived_result.is_ok(),
            "variant {name} derivation must succeed: {:?}",
            derived_result.as_ref().err()
        );
        let derived = match derived_result {
            Ok(d) => d,
            // Assert above already failed the test; the seed keeps the
            // compiler happy without panicking.
            Err(_) => seed.clone(),
        };
        let colors = colors_from_graph(&derived);
        let s = render(&colors);
        let (ah, rh) = hashes(&s);
        // Every variant paints (alpha coverage) with distinct pixels.
        assert!(ah != 0, "variant {name} painted nothing");
        for (ph, prh) in &seen {
            let _ = (ph, prh);
        }
        seen.push((ah, rh));
        let unique = seen.len();
        let _ = unique;
        println!("{name}: alpha-hash {ah:#x} rgb-hash {rh:#x}");
        assert!(s.pixel(1, 1).map(|p| p[3]).unwrap_or(0) > 0);
    }
    // Distinct RGB hashes: the variants must actually differ visually.
    let mut uniq: Vec<(u64, u64)> = Vec::new();
    for (name, variant) in CASES {
        let derived_result = variant_graph(&seed, variant);
        if let Err(e) = &derived_result {
            println!("variant {name} derivation failed: {e:?}");
        }
        assert!(
            derived_result.is_ok(),
            "variant {name} derivation must succeed: {:?}",
            derived_result.as_ref().err()
        );
        let derived = match derived_result {
            Ok(d) => d,
            // Assert above already failed the test; the seed keeps the
            // compiler happy without panicking.
            Err(_) => seed.clone(),
        };
        let colors = colors_from_graph(&derived);
        let s = render(&colors);
        let pair = hashes(&s);
        assert!(
            !uniq.contains(&pair),
            "variant {name} pixel-identical to a sibling"
        );
        uniq.push(pair);
        let _ = name;
    }
}

#[test]
fn widget_states_stay_distinct_under_cvd_variants() {
    use widget_kit::theme::variant_colors;
    use widget_kit::{Size2D, TreeItem};

    let seed = seed_graph();
    let cases = [
        ("protanopia", color_kit::CvdKind::Protanopia),
        ("deuteranopia", color_kit::CvdKind::Deuteranopia),
        ("tritanopia", color_kit::CvdKind::Tritanopia),
    ];
    let rgb_hash = |s: &Surface| -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for (i, b) in s.data().iter().enumerate() {
            if i % 4 != 3 {
                h ^= u64::from(*b);
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
        }
        h
    };
    let paint = |colors: &WidgetColors, tree: &WidgetTree| {
        let mut surface = Surface::new(220, 110).unwrap_or_else(|| {
            Surface::with_stride(1, 1, 4).unwrap_or_else(|| {
                Surface::new(1, 1).unwrap_or_else(|| unreachable!("1x1 surface cannot overflow"))
            })
        });
        let mut painter = Painter::default();
        paint_tree(tree, &mut surface, &mut painter, colors);
        rgb_hash(&surface)
    };

    for (name, kind) in cases {
        let Ok(colors) = variant_colors(&seed, Variant::Cvd(kind)) else {
            panic_missing_variant(name);
        };

        // Checkbox: checked vs unchecked must read differently.
        let checkbox = |checked: bool| {
            let mut tree = WidgetTree::new();
            let root = tree.push(WidgetKind::Column { gap: 0.0 });
            let cb = tree.push(WidgetKind::Checkbox {
                label: "Compact".to_string(),
                checked,
            });
            tree.add_child(root, cb);
            tree.compute(Size2D {
                width: 220.0,
                height: 110.0,
            });
            tree
        };
        assert_ne!(
            paint(&colors, &checkbox(true)),
            paint(&colors, &checkbox(false)),
            "checked vs unchecked must differ under cvd {name}"
        );

        // Button: the focus ring must survive the simulation.
        let focused_button = |focused: bool| {
            let mut tree = WidgetTree::new();
            let root = tree.push(WidgetKind::Column { gap: 0.0 });
            let btn = tree.push(WidgetKind::Button {
                label: "Compose".to_string(),
                action: 1,
            });
            tree.add_child(root, btn);
            tree.compute(Size2D {
                width: 220.0,
                height: 110.0,
            });
            if focused {
                tree.set_focus(Some(btn));
            }
            tree
        };
        assert_ne!(
            paint(&colors, &focused_button(true)),
            paint(&colors, &focused_button(false)),
            "focus ring must survive cvd {name}"
        );

        // TreeList: selection highlight must survive the simulation.
        let tree_sel = |selected: usize| {
            let mut tree = WidgetTree::new();
            let root = tree.push(WidgetKind::Column { gap: 0.0 });
            let list = tree.push(WidgetKind::TreeList {
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
                ],
                selected,
            });
            tree.add_child(root, list);
            tree.compute(Size2D {
                width: 220.0,
                height: 110.0,
            });
            tree
        };
        assert_ne!(
            paint(&colors, &tree_sel(0)),
            paint(&colors, &tree_sel(1)),
            "tree selection must survive cvd {name}"
        );

        // TabList: the active tab marker must survive the simulation.
        let tabs_active = |active: usize| {
            let mut tree = WidgetTree::new();
            let root = tree.push(WidgetKind::Column { gap: 0.0 });
            let tabs = tree.push(WidgetKind::TabList {
                tabs: vec!["Read".to_string(), "Sent".to_string()],
                active,
            });
            tree.add_child(root, tabs);
            tree.compute(Size2D {
                width: 220.0,
                height: 110.0,
            });
            tree
        };
        assert_ne!(
            paint(&colors, &tabs_active(0)),
            paint(&colors, &tabs_active(1)),
            "active tab must survive cvd {name}"
        );
    }
}

/// `panic!` lives in a helper so the deny(panic) lint stays satisfied
/// while the failure message keeps the variant name.
#[allow(clippy::panic)]
fn panic_missing_variant(name: &str) -> ! {
    panic!("cvd {name} derive failed")
}

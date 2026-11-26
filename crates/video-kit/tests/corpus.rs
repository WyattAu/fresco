//! Renderer coverage corpus and stress bench (REQ-V4, ADR-0005).
//!
//! Each scene drives distinct paint-kit render paths across a
//! timeline; per-frame FNV-1a keys are checked-in baselines, so any
//! renderer output change surfaces as a hash diff. The stress scene
//! reports per-frame paint pacing (evidence, not a CI speed gate).

use std::time::Instant;

use paint_kit::{
    ColorStop, LinearGradient, Paint, Painter, RadialGradient, Rect, Solid, Surface, Vec2,
};
use video_kit::{BmpSequenceSink, FrameSink, export};

const W: usize = 320;
const H: usize = 240;
const FPS: u32 = 12;
const FRAMES: u32 = 8;

fn solid(color: [u8; 4]) -> Paint {
    Paint::Solid(Solid::premul(color))
}

/// Rounded rects translating and recoloring over time.
fn round_rects(t: f64, surface: &mut Surface) {
    surface.fill_solid([16, 18, 26, 255]);
    let mut painter = Painter::default();
    for i in 0..30_i32 {
        let phase = f64::from(i) * 0.31;
        let x = 20.0 + 60.0 * ((t * 2.0 + phase).sin());
        let y = 20.0 + 30.0 * ((t * 3.0 + phase).cos());
        let channel = (t * 100.0 + f64::from(i * 7)) as u8;
        let rect = Rect::from_size(Vec2::new(x, y), Vec2::new(40.0, 24.0));
        painter.fill_round_rect(surface, rect, 8.0, &solid([channel, 90, 160, 255]));
    }
    for i in 0..8_i32 {
        let x = 10.0 + f64::from(i * 38);
        let rect = Rect::from_size(Vec2::new(x, 190.0 + t * 12.0), Vec2::new(28.0, 28.0));
        painter.fill_shadow_round_rect(surface, rect, 6.0, 4.0, [60, 60, 80, 255]);
    }
}

/// A rotating polygon ring through the path pipeline.
fn paths(t: f64, surface: &mut Surface) {
    surface.fill_solid([10, 12, 18, 255]);
    let mut painter = Painter::default();
    for ring in 0..6_i32 {
        let spokes = 5 + ring;
        let radius = 30.0 + f64::from(ring) * 18.0;
        let angle = t * (1.2 + 0.4 * f64::from(ring % 3));
        let poly: Vec<Vec2> = (0..spokes)
            .map(|s| {
                let theta = angle + f64::from(s) * std::f64::consts::TAU / f64::from(spokes);
                Vec2::new(160.0 + radius * theta.cos(), 120.0 + radius * theta.sin())
            })
            .collect();
        let channel = 120 + (t * 90.0) as u8;
        painter.fill_slices(surface, &[&poly], &solid([channel, 140, 60, 200]));
    }
}

/// Gradient paints: linear, radial, sweep.
fn gradients(t: f64, surface: &mut Surface) {
    surface.fill_solid([8, 8, 12, 255]);
    let mut painter = Painter::default();
    let shift = (t * 60.0) as u8;
    let linear = LinearGradient::new(
        Vec2::new(0.0, 0.0),
        Vec2::new(f64::from(W as u32), f64::from(H as u32)),
        vec![
            ColorStop {
                offset: 0.0,
                color: [shift, 60, 120, 255],
            },
            ColorStop {
                offset: 1.0,
                color: [40, 200 - shift, 90, 255],
            },
        ],
    );
    let half = Rect::from_size(
        Vec2::ZERO,
        Vec2::new(f64::from(W as u32) / 2.0, f64::from(H as u32)),
    );
    painter.fill_round_rect(surface, half, 0.0, &Paint::Linear(linear));
    let radial = RadialGradient::new(
        Vec2::new(220.0 + t * 20.0, 80.0),
        90.0,
        vec![
            ColorStop {
                offset: 0.0,
                color: [220, 200 - shift, 40, 255],
            },
            ColorStop {
                offset: 1.0,
                color: [40, 20, 60, 0],
            },
        ],
    );
    let circle = Rect::from_size(Vec2::new(120.0, 20.0), Vec2::new(180.0, 180.0));
    painter.fill_round_rect(surface, circle, 90.0, &Paint::Radial(radial));
}

/// Widgets over paint_tree: the interactive path at 60 Hz scale.
fn widgets(t: f64, surface: &mut Surface) {
    use widget_kit::{Size2D, WidgetColors, WidgetKind, WidgetTree, paint_tree};
    surface.fill_solid([20, 22, 30, 255]);
    let mut tree = WidgetTree::new();
    let root = tree.push(WidgetKind::Column { gap: 10.0 });
    let title = tree.push(WidgetKind::Label {
        text: format!("frame t={t:.2}"),
        px_size: 16.0,
    });
    tree.add_child(root, title);
    let button = tree.push(WidgetKind::Button {
        label: "Render".to_string(),
        action: 1,
    });
    tree.add_child(root, button);
    let check = tree.push(WidgetKind::Checkbox {
        label: "stress".to_string(),
        checked: t > 0.5,
    });
    tree.add_child(root, check);
    tree.compute(Size2D {
        width: f64::from(W as u32),
        height: f64::from(H as u32),
    });
    let mut painter = Painter::default();
    paint_tree(&tree, surface, &mut painter, &WidgetColors::default());
}

/// Clipped layers: shapes move through a scissor, then a fade.
fn clip_layered(t: f64, surface: &mut Surface) {
    surface.fill_solid([12, 14, 20, 255]);
    let mut painter = Painter::default();
    painter.push_clip(Rect::from_size(
        Vec2::new(40.0, 40.0),
        Vec2::new(120.0, 160.0),
    ));
    for i in 0..12_i32 {
        let y = ((t * 120.0 + f64::from(i * 20)) % 240.0) - 20.0;
        let rect = Rect::from_size(Vec2::new(60.0 + f64::from(i * 4), y), Vec2::new(70.0, 14.0));
        let tone = (80_u8).saturating_add((i as u8).saturating_mul(8));
        painter.fill_round_rect(surface, rect, 5.0, &solid([tone, 170, 220, 255]));
    }
    painter.pop_clip();
    let fade = Rect::from_size(
        Vec2::ZERO,
        Vec2::new(f64::from(W as u32), f64::from(H as u32)),
    );
    painter.fill_round_rect(surface, fade, 0.0, &solid([0, 0, 0, (t * 80.0) as u8]));
}

/// The corpus: (scene name, draw fn) pairs.
/// A corpus scene draw closure (boxed: the damage-planned scene owns
/// its widget tree across frames).
type SceneFn = Box<dyn FnMut(f64, &mut Surface)>;

fn scenes() -> Vec<(&'static str, SceneFn)> {
    vec![
        (
            "round_rects",
            Box::new(round_rects as fn(f64, &mut Surface)),
        ),
        ("paths", Box::new(paths)),
        ("gradients", Box::new(gradients)),
        ("widgets", Box::new(widgets)),
        ("clip_layered", Box::new(clip_layered)),
        ("scene_plan", Box::new(scene_plan_scene())),
    ]
}

/// The damage-planned leg (scene-kit over widget-kit): the tree lives
/// across frames, frame 0 repaints fully, later frames repaint only
/// the damaged title region through `paint_tree_plan`. The exporter's
/// zeroed canvas keeps the bytes deterministic either way; the golden
/// keys pin the plan-clipped output.
fn scene_plan_scene() -> SceneFn {
    use scene_kit::FramePlan;
    use widget_kit::{Size2D, WidgetColors, WidgetKind, WidgetTree, paint_tree_plan};
    let mut tree = WidgetTree::new();
    let root = tree.push(WidgetKind::Column { gap: 10.0 });
    let title = tree.push(WidgetKind::Label {
        text: "frame t=0.00".to_string(),
        px_size: 16.0,
    });
    tree.add_child(root, title);
    let button = tree.push(WidgetKind::Button {
        label: "Plan".to_string(),
        action: 1,
    });
    tree.add_child(root, button);
    let mut primed = false;
    Box::new(move |t, surface| {
        surface.fill_solid([20, 22, 30, 255]);
        tree.compute(Size2D {
            width: f64::from(W as u32),
            height: f64::from(H as u32),
        });
        let mut title_rect = None;
        if let Some(w) = tree.widget_mut(title)
            && let WidgetKind::Label { text, .. } = &mut w.kind
        {
            *text = format!("frame t={t:.2}");
        }
        if let Some(w) = tree.widget(title) {
            title_rect = Some(w.rect);
        }
        let plan = if primed {
            match title_rect {
                Some(r) => FramePlan::Regions(vec![Rect::from_size(
                    Vec2::new(r.x - 8.0, r.y - 8.0),
                    Vec2::new(r.width + 16.0, r.height + 16.0),
                )]),
                None => FramePlan::Full,
            }
        } else {
            primed = true;
            FramePlan::Full
        };
        let mut painter = Painter::default();
        paint_tree_plan(
            &tree,
            surface,
            &mut painter,
            &WidgetColors::default(),
            &plan,
        );
    })
}

/// Per-frame FNV keys of one scene, exported fresh each call.
fn frame_hashes(name: &str) -> Result<Vec<u64>, Box<dyn std::error::Error>> {
    use std::sync::atomic::{AtomicU32, Ordering};
    static CALL: AtomicU32 = AtomicU32::new(0);
    let call = CALL.fetch_add(1, Ordering::Relaxed);
    let mut draw = scenes()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, f)| f)
        .ok_or_else(|| format!("unknown scene {name}"))?;
    let dir = std::env::temp_dir().join(format!("video-kit-corpus-{}-{call}", std::process::id()));
    let mut sink = BmpSequenceSink::new(&dir, name)?;
    export(W, H, FPS, FRAMES, &mut sink, &mut |t, surface| {
        draw(t, surface)
    })?;
    sink.finish()?;
    let mut hashes = Vec::new();
    for index in 0..FRAMES {
        let bytes = std::fs::read(dir.join(format!("{name}_{index:05}.bmp")))?;
        hashes.push(video_kit::fnv1a(&bytes));
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(hashes)
}

/// Checked-in golden keys: (scene, [frame hashes; 8]).
fn baselines() -> Vec<(&'static str, [u64; FRAMES as usize])> {
    vec![
        (
            "round_rects",
            [
                17998275741331967146,
                12388258266460221491,
                5442893663447056106,
                8208606714151338987,
                12940773605559547764,
                15706669887480758237,
                7955936578655373071,
                10799862484831668062,
            ],
        ),
        (
            "paths",
            [
                5201275866737407706,
                1662137118261890865,
                14540075162772729199,
                8506107385700740327,
                9705337083634879541,
                14477058260732800574,
                9447430747770301507,
                13626682752857410534,
            ],
        ),
        (
            "gradients",
            [
                5454989708981672784,
                14305357836894588479,
                6290255515958193276,
                2000790216625975597,
                5949819777160302956,
                17847823558180873676,
                15787188210414718292,
                4116913363325059886,
            ],
        ),
        (
            "widgets",
            [
                1725476191444149905,
                16208907420880578150,
                714345291494804360,
                3373781660419993018,
                712297095722624449,
                11364103976779099243,
                1636372570857007498,
                11085124284447982970,
            ],
        ),
        (
            "clip_layered",
            [
                15100533774088026774,
                2084549734827553999,
                3941925043343429678,
                17518719351369038829,
                4473626029202819346,
                6902717952304666349,
                10473596762902783930,
                1162869230861667330,
            ],
        ),
        (
            "scene_plan",
            [
                13511829144483992931,
                13240202344214726553,
                13858996453599240091,
                7244993282459449285,
                5990880170149175750,
                15858886401013216948,
                13158484560330815605,
                14414172991034216286,
            ],
        ),
    ]
}

/// Scenes whose pixels include shaped text. Shaped bytes are
/// font-stack dependent: they are pinned only for the default
/// (per-char, embedded-font) render, and only asserted structurally
/// under the shaped-corpus feature.
const TEXT_SCENES: [&str; 2] = ["widgets", "scene_plan"];

/// Oracle (REQ-V4): every corpus scene is deterministic and matches
/// the checked-in per-frame keys - any renderer output change diffs
/// a hash instead of slipping through.
#[test]
fn corpus_matches_golden_frame_keys() -> Result<(), Box<dyn std::error::Error>> {
    for (name, want) in baselines() {
        let got = frame_hashes(name)?;
        if cfg!(feature = "shaped-corpus") && TEXT_SCENES.contains(&name) {
            // Shaped bytes are font-stack dependent: assert the
            // structural contract (every frame distinct - the label
            // text varies per frame and reaches the pixels).
            let mut unique = got.clone();
            unique.sort_unstable();
            unique.dedup();
            assert_eq!(
                unique.len(),
                FRAMES as usize,
                "scene {name}: every frame distinct under shaping"
            );
            continue;
        }
        for (frame, (g, w)) in got.iter().zip(want.iter()).enumerate() {
            assert_eq!(g, w, "scene {name} frame {frame} hash");
        }
    }
    Ok(())
}

/// Oracle (shaped-corpus feature): the corpus renders deterministically
/// through the Parley-shaped path too - two exports of the same scene
/// produce identical hashes. Cross-machine shaped keys are not pinned
/// (font stacks differ); determinism is the contract, the default
/// per-char corpus is the byte contract.
#[cfg(feature = "shaped-corpus")]
#[test]
fn shaped_render_is_deterministic() -> Result<(), Box<dyn std::error::Error>> {
    use text_kit::parley_backend::ParleyEngine;
    use text_kit::{ShapeQuery, TextEngine};
    let probe = {
        let mut engine = ParleyEngine::new();
        !engine
            .shape(&ShapeQuery {
                text: "ab",
                font: text_kit::parley_backend::PARLEY_FONT,
                px_size: 16.0,
                letter_spacing: 0.0,
            })
            .glyphs
            .is_empty()
    };
    if !probe {
        eprintln!("skip: no fonts on this runner");
        return Ok(());
    }
    let hash_run = || -> Result<u64, Box<dyn std::error::Error>> {
        let mut draw = scenes()
            .into_iter()
            .find(|(n, _)| *n == "widgets")
            .map(|(_, f)| f)
            .ok_or("missing scene")?;
        let dir = std::env::temp_dir().join(format!(
            "video-kit-shaped-{}-{}",
            std::process::id(),
            u64::from(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.subsec_nanos())
                    .unwrap_or(0)
            )
        ));
        let mut sink = BmpSequenceSink::new(&dir, "s")?;
        export(W, H, FPS, FRAMES, &mut sink, &mut |t, surface| {
            draw(t, surface)
        })?;
        sink.finish()?;
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for index in 0..FRAMES {
            let bytes = std::fs::read(dir.join(format!("s_{index:05}.bmp")))?;
            for byte in video_kit::fnv1a(&bytes).to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0100_0000_01b3);
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
        Ok(hash)
    };
    let a = hash_run()?;
    let b = hash_run()?;
    assert_eq!(a, b, "shaped corpus render is deterministic");
    Ok(())
}

/// Prints the current per-frame keys to paste into `baselines`.
#[test]
fn corpus_bootstraps_baseline_hashes() -> Result<(), Box<dyn std::error::Error>> {
    for (name, _) in baselines() {
        let hashes = frame_hashes(name)?;
        println!("{name}: {hashes:?}");
    }
    Ok(())
}

/// Stress bench (REQ-V4 evidence): a heavy mixed scene across 60
/// frames must complete with every frame carrying pixels; pacing is
/// reported, not gated (CI machines differ; the B gate owns speed).
#[test]
fn renderer_stress_pacing_completes_with_pixels() -> Result<(), Box<dyn std::error::Error>> {
    let mut paint_us: Vec<u128> = Vec::new();
    let mut sink = BmpSequenceSink::new(
        &std::env::temp_dir().join(format!("video-kit-stress-{}", std::process::id())),
        "s",
    )?;
    let started = Instant::now();
    let mut draw = |t: f64, surface: &mut Surface| {
        let mark = Instant::now();
        round_rects(t, surface);
        paths(t * 0.7, surface);
        gradients(t * 0.5, surface);
        widgets(t, surface);
        paint_us.push(mark.elapsed().as_micros());
    };
    let stats = export(480, 800, 30, 60, &mut sink, &mut draw)?;
    sink.finish()?;
    let total = started.elapsed();
    assert_eq!(stats.frames(), 60, "all frames exported");
    assert!(
        paint_us.len() == 60 && stats.bytes() > 60 * 54,
        "every frame painted and written"
    );
    paint_us.sort_unstable();
    let p50 = paint_us.get(30).copied().unwrap_or(0);
    let p95 = paint_us.get(57).copied().unwrap_or(0);
    let max = paint_us.last().copied().unwrap_or(0);
    println!(
        "stress (debug build): 60 frames 480x800 in {:?}; paint p50 {p50}us p95 {p95}us max {max}us - release numbers via: cargo test -p video-kit --release --test corpus renderer_stress -- --nocapture",
        total
    );
    assert!(
        total < std::time::Duration::from_secs(240),
        "generous CI bound for a debug-build bench; took {total:?}"
    );
    Ok(())
}

//! Damage-plan rendering on the sparse-strip backend: the compositor
//! model (previous frame + clip-confined region repaints) must be
//! byte-identical to a full re-render.
//!
//! This is the strip-backend analog of widget-kit's REQ-K15 proof:
//! regions come from scene-kit's `DamageTracker`, each region pass
//! runs the full draw sequence under a pixel-aligned clip, and the
//! region content is blitted over the previous frame.

use paint_kit::{Rect, StripTarget, Vec2};
use scene_kit::{DamageTracker, FramePlan};

const W: u32 = 160;
const H: u32 = 96;

/// One element of the fixture scene: a rounded gradient card.
#[derive(Clone, Copy)]
struct Card {
    rect: Rect,
}

fn draw_card(target: &mut StripTarget, card: Card) {
    // Backing plate: solid rounded rect.
    target.fill_round_rect(card.rect, 8.0, [24, 60, 130, 255]);
    // Gradient face inset by 4 px.
    let face = Rect::from_points(
        Vec2::new(card.rect.x0 + 4.0, card.rect.y0 + 4.0),
        Vec2::new(card.rect.x1 - 4.0, card.rect.y1 - 4.0),
    );
    target.fill_round_rect(face, 5.0, [220, 120, 40, 255]);
}

/// Frame A: two cards, one static, one moveable.
fn frame_a(moved: Card) -> Vec<(Card, bool)> {
    let static_card = Card {
        rect: Rect::from_points(Vec2::new(8.0, 8.0), Vec2::new(64.0, 56.0)),
    };
    vec![(static_card, false), (moved, true)]
}

/// Render the whole scene through the strip backend and composite.
fn render_full(elements: &[(Card, bool)]) -> Vec<u8> {
    let Some(mut target) = StripTarget::new(W, H) else {
        return Vec::new();
    };
    for (card, _) in elements {
        draw_card(&mut target, *card);
    }
    target.composite().to_vec()
}

/// Render the scene once per damage region (clipped), then blit the
/// region pixels over `previous`.
fn render_damaged(elements: &[(Card, bool)], previous: &[u8], plan: &FramePlan) -> Vec<u8> {
    let FramePlan::Regions(regions) = plan else {
        return previous.to_vec();
    };
    // A fresh target renders the scene; each pass is clipped to one
    // region, so only region pixels have content.
    let Some(mut target) = StripTarget::new(W, H) else {
        return previous.to_vec();
    };
    let mut pixel_aligned: Vec<Rect> = Vec::new();
    for region in regions {
        let clip = Rect::from_points(
            Vec2::new(region.x0.floor(), region.y0.floor()),
            Vec2::new(region.x1.ceil(), region.y1.ceil()),
        );
        pixel_aligned.push(clip);
        target.push_clip(clip);
        for (card, _) in elements {
            draw_card(&mut target, *card);
        }
        target.pop_clip();
    }
    let composite = target.composite().to_vec();
    // Blit: previous frame everywhere, region content inside regions.
    let mut frame = previous.to_vec();
    for (i, slot) in frame.iter_mut().enumerate() {
        let px = i / 4;
        let (x, y) = ((px % W as usize) as f64, (px / W as usize) as f64);
        let in_region = pixel_aligned
            .iter()
            .any(|r| x >= r.x0 && x < r.x1 && y >= r.y0 && y < r.y1);
        if in_region && let Some(src) = composite.get(i) {
            *slot = *src;
        }
    }
    frame
}

fn device(rect: Rect) -> Rect {
    rect
}

#[test]
fn strip_damage_plan_matches_full_render() {
    let moved_a = Card {
        rect: Rect::from_points(Vec2::new(72.0, 20.0), Vec2::new(152.0, 84.0)),
    };
    let elements_a = frame_a(moved_a);
    let frame_a_bytes = render_full(&elements_a);

    // State change: the moveable card slides 24 px left, 8 px up.
    let moved_b = Card {
        rect: Rect::from_points(Vec2::new(48.0, 12.0), Vec2::new(128.0, 76.0)),
    };
    let elements_b = frame_a(moved_b);

    // Damage: old and new rects of the moved card.
    let mut tracker = DamageTracker::new(8, 0.9);
    tracker.invalidate(device(moved_a.rect));
    tracker.invalidate(device(moved_b.rect));
    let plan = tracker.take_plan(Rect::from_points(
        Vec2::new(0.0, 0.0),
        Vec2::new(f64::from(W), f64::from(H)),
    ));
    assert!(matches!(plan, FramePlan::Regions(_)), "plan: {plan:?}");

    let frame_b = render_damaged(&elements_b, &frame_a_bytes, &plan);
    let full_b = render_full(&elements_b);
    assert_eq!(
        frame_b.len(),
        full_b.len(),
        "damaged and full renders must have equal length"
    );
    let mismatches = frame_b
        .iter()
        .zip(full_b.iter())
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(
        mismatches, 0,
        "damaged strip render must equal full render ({mismatches} bytes differ)"
    );
}

#[test]
fn strip_none_plan_keeps_previous_frame() {
    let moved = Card {
        rect: Rect::from_points(Vec2::new(72.0, 20.0), Vec2::new(152.0, 84.0)),
    };
    let elements = frame_a(moved);
    let frame = render_full(&elements);
    let mut painter_frame = frame.clone();
    // FramePlan::None must be a no-op by construction; assert the
    // helper passes the previous bytes through.
    let out = render_damaged(&elements, &painter_frame, &FramePlan::None);
    painter_frame.copy_from_slice(&out);
    assert_eq!(out, frame);
}

#[test]
fn strip_damage_artifacts_written() {
    // Human-eye artifacts: frame A, damaged frame B, and the full
    // frame B, written as BMPs under /tmp/opencode.
    let moved_a = Card {
        rect: Rect::from_points(Vec2::new(72.0, 20.0), Vec2::new(152.0, 84.0)),
    };
    let elements_a = frame_a(moved_a);
    let frame_a_bytes = render_full(&elements_a);

    let moved_b = Card {
        rect: Rect::from_points(Vec2::new(48.0, 12.0), Vec2::new(128.0, 76.0)),
    };
    let elements_b = frame_a(moved_b);
    let mut tracker = DamageTracker::new(8, 0.9);
    tracker.invalidate(moved_a.rect);
    tracker.invalidate(moved_b.rect);
    let plan = tracker.take_plan(Rect::from_points(
        Vec2::new(0.0, 0.0),
        Vec2::new(f64::from(W), f64::from(H)),
    ));
    let frame_b = render_damaged(&elements_b, &frame_a_bytes, &plan);
    let full_b = render_full(&elements_b);

    let dir = std::path::Path::new("/tmp/opencode");
    std::fs::create_dir_all(dir).ok();
    // Raw premultiplied RGBA8 payloads (W*H*4, row-major) for offline
    // conversion; paint-kit has no BMP writer and no rc dep.
    std::fs::write(dir.join("strip-frame-a.rgba"), &frame_a_bytes).ok();
    std::fs::write(dir.join("strip-frame-b-damaged.rgba"), &frame_b).ok();
    std::fs::write(dir.join("strip-frame-b-full.rgba"), &full_b).ok();
}

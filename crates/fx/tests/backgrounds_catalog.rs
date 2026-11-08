//! Backgrounds catalog: all background components rendered per-kind
//! with determinism asserts, golden FNV hashes, and BMP artifacts for
//! device review.

use fx::{
    Beams, DotPattern, FlickeringGrid, GradientMesh, GridPattern, NoiseField, Particles, StarField,
    Waves,
};
use paint_kit::{Painter, Surface};

/// Render background `kind` (0..9) at time `t` onto a 320x160 surface.
fn render_one(kind: usize, t: f64) -> Surface {
    let mut surface = Surface::new(320, 160).unwrap_or_default();
    let mut painter = Painter::default();
    let mut fx = fx::Fx::new(&mut surface, &mut painter, t);
    match kind {
        0 => GridPattern::new().draw(&mut fx, 320, 160),
        1 => DotPattern::new().draw(&mut fx, 320, 160),
        2 => StarField::new().draw(&mut fx, 320, 160),
        3 => FlickeringGrid::new().draw(&mut fx, 320, 160),
        4 => Waves::new().draw(&mut fx, 320, 160),
        5 => GradientMesh::new().draw(&mut fx, 320, 160),
        6 => Particles::new().draw(&mut fx, 320, 160),
        7 => NoiseField::new().draw(&mut fx, 320, 160),
        8 => Beams::new().draw(&mut fx, 320, 160),
        _ => {}
    }
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
fn backgrounds_are_deterministic() {
    for kind in 0..9_usize {
        let a = render_one(kind, 1.0);
        let b = render_one(kind, 1.0);
        assert_eq!(
            a.data(),
            b.data(),
            "background kind {kind} must render deterministically"
        );
    }
}

#[test]
fn backgrounds_match_golden_hashes() {
    // Constants recorded from the first deterministic run; update via
    // GATE_FX_REBASELINE=1 --nocapture when a background intentionally
    // changes.
    const GOLDENS: [u64; 9] = [
        0xf584_5953_5b8b_c3a5, // grid pattern
        0x832d_bc45_a0a3_2d2d, // dot pattern
        0x5c16_4427_ff6e_3071, // star field
        0x743f_4b26_bf89_4595, // flickering grid
        0xf91a_8174_3498_8170, // waves
        0x9976_390d_ca76_7d33, // gradient mesh
        0xb273_06cb_8a26_16ca, // particles
        0xc5f7_5b1e_4b38_078a, // noise field (softened defaults)
        0xf76a_02d5_6251_5791, // beams
    ];
    for (kind, golden) in GOLDENS.iter().enumerate() {
        let surface = render_one(kind, 1.0);
        let h = fnv1a(surface.data());
        assert_eq!(h, *golden, "background kind {kind} golden drifted");
    }
}

#[test]
fn backgrounds_artifacts_written() {
    for kind in 0..9_usize {
        let surface = render_one(kind, 1.0);
        let dir = std::path::Path::new("/tmp/opencode");
        std::fs::create_dir_all(dir).ok();
        std::fs::write(dir.join(format!("bg-{kind}.rgba")), surface.data()).ok();
    }
}

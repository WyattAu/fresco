//! theme-kit adapter: resolved token graphs become [`WidgetColors`].
//!
//! This is the flagship claim made concrete: widget palettes are
//! *derived* from an audited token graph, not hand-picked hex. The seed
//! graph passes theme-kit's WCAG gate; variants (Light, HighContrast,
//! Monochrome, CVD simulations) are derived through theme-kit's
//! Oklab-space retinting and re-audited. The gallery then renders every
//! variant and pins the pixels (REQ-K9).
//!
//! Role mapping:
//!
//! - `Surface` -> panel/entry surface
//! - `SurfaceRaised` -> button and checkbox chrome
//! - `OnSurface` -> text placeholder color
//! - `Selection` -> pressed overlay
//! - `Success` -> check mark
//! - `FocusRing` -> focus ring and text-field caret

use crate::WidgetColors;
use theme_kit::{Role, TokenGraph, Variant};

/// Seed roles with hex values; a malformed constant here is a
/// programmer error, so malformed entries are skipped (the role then
/// falls back to the stock palette at resolve time).
const SEED: [(Role, &str); 12] = [
    (Role::Surface, "#10131a"),
    (Role::SurfaceRaised, "#383c48"),
    (Role::OnSurface, "#f2f4f8"),
    (Role::OnSurfaceMuted, "#9aa2b1"),
    (Role::Accent, "#1e40c8"),
    (Role::OnAccent, "#f2f4f8"),
    (Role::Warn, "#ffd166"),
    (Role::Error, "#ff8a80"),
    (Role::Success, "#7cd992"),
    (Role::Selection, "#2a3140"),
    (Role::Border, "#6b7488"),
    (Role::FocusRing, "#7fb2ff"),
];

/// The estate seed palette: a dark theme that passes the WCAG gate with
/// headroom, so every derived variant audits clean too (REQ-K9).
#[must_use]
pub fn seed_graph() -> TokenGraph {
    let mut g = TokenGraph::new("fresco-seed");
    for (role, hex) in SEED {
        let _ = g.set_hex(role, hex);
    }
    g
}

/// Map a resolved token graph to widget colors. Missing roles fall back
/// to the stock dark palette values.
#[must_use]
pub fn colors_from_graph(graph: &TokenGraph) -> WidgetColors {
    let resolve = |role: Role| -> Option<[u8; 4]> {
        graph
            .resolve(role)
            .and_then(|c| c.to_srgb8().ok())
            .map(|v| [v.r, v.g, v.b, v.a])
    };
    let fallback = WidgetColors::default();
    WidgetColors {
        surface: resolve(Role::Surface).unwrap_or(fallback.surface),
        text: resolve(Role::OnSurface).unwrap_or(fallback.text),
        button: resolve(Role::SurfaceRaised).unwrap_or(fallback.button),
        focus_ring: resolve(Role::FocusRing).unwrap_or(fallback.focus_ring),
        pressed_overlay: resolve(Role::Selection).unwrap_or(fallback.pressed_overlay),
        checkbox_box: resolve(Role::SurfaceRaised).unwrap_or(fallback.checkbox_box),
        check: resolve(Role::Success).unwrap_or(fallback.check),
    }
}

/// Derive a variant graph (Light, HighContrast, Monochrome, CVD) from
/// the seed. Callers audit the result through
/// `theme_kit::audit(&graph, cvd)`.
///
/// # Errors
/// Propagates theme-kit's derivation errors.
pub fn variant_graph(
    seed: &TokenGraph,
    variant: Variant,
) -> Result<TokenGraph, theme_kit::ThemeError> {
    theme_kit::derive(seed, variant)
}

/// Convenience: derive then resolve to widget colors.
///
/// # Errors
/// Propagates theme-kit's derivation errors.
pub fn variant_colors(
    seed: &TokenGraph,
    variant: Variant,
) -> Result<WidgetColors, theme_kit::ThemeError> {
    Ok(colors_from_graph(&variant_graph(seed, variant)?))
}

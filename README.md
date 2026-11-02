# fresco

Modular retained-mode UI engine for Rust: owned scene graph, damage-tracked compositing, and a split CPU/GPU paint stack. The estate's "modular Qt": Qt's architecture, Rust's ownership rules, no C++ runtime.

[![CI](https://github.com/WyattAu/fresco/actions/workflows/ci.yml/badge.svg)](https://github.com/WyattAu/fresco/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

## Status: DRAFT (0.1.0)

Phase 1 of the estate UI-engine plan (see `SPEC.md` and `docs/adr/`). The
semantic layers it builds on (`color-kit`, `theme-kit`, `a11y-kit`,
`input-kit`, `ui-text-edit`, `font-model`, `font-shape`) already exist in the
estate; fresco owns the middle of the stack between semantics and pixels.

## Crates

| Crate | Layer | Status | What it owns |
|---|---|---|---|
| `path-kit` | L0 | implemented | 2D geometry: béziers, adaptive flattening, Legendre-Gauss arc length, stroking, dashing, robust orientation predicates, hit testing |
| `anim-kit` | L0 | implemented | Exact closed-form damped-spring animation (no per-frame integration), easing curves |
| `paint-kit` | L2 | implemented (CPU) | Exact-coverage scanline rasterizer, premultiplied RGBA8 surfaces, solid fills, winding-integral AA |
| `scene-kit` | L1 | implemented (core) | Generational-arena retained scene graph, damage region algebra, frame plans, sorted `DrawList` flattening |
| `layout-kit` | L1 | implemented (core) | Memoized incremental measure cache (dirty-bit DAG semantics), single-axis flex solver |
| `window-kit` | L2 platform | implemented (facade + fake; winit backend default feature) | Estate window/event/present/frame-clock traits, winit + softbuffer software present, headless fake runner |
| `text-kit` | L2 | implemented (core + Parley feature) | `TextEngine` trait (ADR-0002), deterministic fake engine, shelf-packed glyph atlas with LRU (887), Parley 0.12 backend |
| `widget-kit` | L3 controls | implemented (Gate D spike) | Retained widget model, hit testing, wrapping focus chain, keyboard activation, a11y-kit emission (audited), renderer contract + paint-kit stock renderer |
| `render-conformance` | L3 | implemented | Golden-image differential harness, BMP diff output, buffer hashing. `publish = false` |

Deferred by ADR: text shaping (`ADR-0002`), windowing (`ADR-0003`), the GPU
backend (`ADR-0004`), widgets and the reactive layer (`ADR-0001` roadmap).

## Quick start

```bash
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

```rust
use path_kit::{Affine, Path, Vec2};
use paint_kit::{FillRule, Painter, Solid, Surface};

let mut surface = Surface::new(64, 64);
let mut path = Path::new();
path.move_to(Vec2::new(8.0, 8.0));
path.line_to(Vec2::new(56.0, 8.0));
path.line_to(Vec2::new(32.0, 56.0));
path.close();
Painter::default().fill_path(
    &mut surface,
    &path,
    Affine::IDENTITY,
    FillRule::NonZero,
    0.1,
    &Solid::new([200, 40, 40, 255]),
);
```

## Design invariants

1. `#![forbid(unsafe_code)]` in every crate. The Windows platform layer will
   be the single reviewed exception, in a future `window-kit`, never here.
2. L0 crates have zero external dependencies (`path-kit`, `anim-kit`); they
   are std-only because float transcendentals are std methods. `layout-kit`
   is `no_std + alloc` (pure arithmetic). See SPEC section 3.
3. The frame path reuses caller-owned buffers in steady state: damage
   trackers, `DrawList`, painter scratch. Setup-time work (flattening a
   changed shape) allocates per call in v0.
4. Anti-aliasing is exact coverage (winding integral), not supersampling,
   on the CPU backend; GPU coverage parity is a render-conformance gate.
5. Every published claim has a consumer in this repo's tests or benches.
   `render-conformance` is unpublished dev tooling by declaration.

## Honest limitations (0.1.0)

- Winding-integral coverage is exact for regions with `|winding| <= 1`
  (all normal UI shapes); heavier overlaps clamp and under-report ink.
- Stroking operates on flattened polylines; exact analytic curve offsets
  are deferred (`docs/paradigms.md`, item 883).
- No text, no gradients, no images, no GPU. Text stack decision recorded in
  `docs/adr/ADR-0002-text-stack.md`.

## License

MIT OR Apache-2.0.

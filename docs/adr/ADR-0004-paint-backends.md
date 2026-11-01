# ADR-0004: Paint trait with CPU reference backend, Vello GPU backend second

Status: ACCEPTED, 2026-11-01

## Context

Rendering today is scattered: Vello 0.3 behind `ldir-vello`, tiny-skia in
`polyfont-render`, scanline fill in `font-shape`. No estate crate owns a
paint abstraction, so backends cannot be swapped and golden-image CI has no
reference implementation.

## Decision

`paint-kit` defines the paint contract and ships the CPU reference backend:
exact-coverage winding-integral scanline (928), premultiplied RGBA8
surfaces, no unsafe, headless by construction. The GPU backend (Vello,
lineage of `ldir-vello`; wgpu pins localized there) implements the same
trait and must pass `render-conformance` parity against the CPU output
(950). Pipeline caches are content-addressed over WGSL + descriptor hashes
(864). Platform compute paths (999, 995, 1044) are per-frontend concerns
behind the same trait, never visible to scene-kit.

## Consequences

- CPU backend is the CI oracle: tests run headless everywhere, including
  the estate's Forgejo runners without GPUs.
- Coverage parity tolerance lives in `render-conformance`, one number, one
  gate.
- `ldir` remains the document-level consumer; fresco does not absorb it.
- SIMD spans (921) are an optimization inside the CPU backend only; the
  scalar path stays the correctness reference.

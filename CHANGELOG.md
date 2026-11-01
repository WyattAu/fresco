# CHANGELOG

## 0.1.0 (2026-11-01)

Initial release. Phase 1 of the estate UI-engine plan.

- `path-kit`: 2D geometry L0, no_std + alloc. Beziers, adaptive flattening
  with tolerance, Legendre-Gauss arc length, polyline stroking with joins
  and caps, dashing, robust adaptive orientation predicate, winding-number
  hit testing.
- `anim-kit`: L0 animation. Closed-form damped spring (under, critical,
  over), settle-time bound, easing functions. No alloc, no deps.
- `paint-kit`: L2 CPU raster. Exact-coverage scanline fill via winding
  integral, premultiplied RGBA8 surfaces, solid fills, NonZero and EvenOdd
  (parity reduction), rect fast path.
- `scene-kit`: L1 retained scene. Generational arena, tree, damage
  rectangles, frame plans (full or regions), sorted DrawList flattening.
- `layout-kit`: L1 layout. Memoized incremental measure cache with
  generation invalidation, single-axis flex solver (grow/shrink/gap/clamp).
- `render-conformance`: L3 dev harness, publish = false. Golden-image
  compare with budget and channel delta, BMP diff output, FNV-1a hashing.

Decisions recorded: ADR-0001 (retained scene + damage), ADR-0002 (text via
trait, rustybuzz/parley first), ADR-0003 (winit-first windowing facade),
ADR-0004 (CPU reference paint, Vello GPU parity).

# REQUIREMENTS

Traceability for the fresco engine. REQ identifiers match `SPEC.md` section
4; acceptance oracles are test names, not prose claims.

## R1 Geometry (path-kit)

- REQ-P1 flatten tolerance honored against true curve, recursive subdivision
  capped, endpoints exact.
- REQ-P2 orientation predicate robust: correct sign where naive f64 gives
  zero (determinants to 1e17).
- REQ-P3 arc length exact on lines, 1e-3 relative on the kappa-circle.
- REQ-P4 dashing preserves total on+off length within one pattern cycle.
- REQ-P5 stroked rect outline bounds equal rect inflated by half width.

## R2 Animation (anim-kit)

- REQ-A1 closed-form springs match numeric integration to 1e-6; initial
  position and velocity satisfied.
- REQ-A2 settle time bound: |u(t_settle)| <= epsilon * scale.
- REQ-A3 overdamped springs with zero initial velocity never overshoot.

## R3 Raster (paint-kit)

- REQ-R1 coverage: rect fill exact, triangle within 1 percent of analytic.
- REQ-R2 fill rules: NonZero honors opposite-oriented holes; EvenOdd parity
  reduction makes nested same-orientation outlines solid with hollow cores.
- REQ-R3 compositing is premultiplied and associative on test cases.
- REQ-R4 no unsafe; scalar code is the reference; SIMD is optimization only.

## R4 Scene (scene-kit)

- REQ-S1 arena generation safety: stale handles miss, slots reused with new
  generation.
- REQ-S2 damage plans: plans cover all invalidations; regions preferred when
  coalescing threshold holds.
- REQ-S3 flatten order by (layer, depth, seq); transforms compose.

## R5 Layout (layout-kit)

- REQ-L1 measure cache: hits on identical inputs, misses on generation bump
  or input hash change.
- REQ-L2 flex solver: grow, shrink, gap, clamps per single-axis rules.

## R6 Conformance (render-conformance)

- REQ-C1 golden compare: pass/fail per channel delta and mismatch budget,
  BMP diff artifact on failure, stable FNV-1a hashing.

## R7 Windowing (window-kit)

- REQ-W1 facade event delivery in script order (fake backend).
- REQ-W2 DPI scale math round-trips.
- REQ-W3 present size validation and capture.
- REQ-W4 frame clock statistics defined at the edges.
- Platform backends (winit) compile under --all-features; runtime window
  behavior is exercised by the consumer app, not CI.

## R8 Text (text-kit)

- REQ-T1 deterministic shaping (fake engine).
- REQ-T2 greedy space wrapping within budget.
- REQ-T3 atlas packing without overlap; coverage round-trip.
- REQ-T4 LRU eviction keeps the atlas placeable.
- Parley backend (feature `parley`) compiles; glyph fidelity is gated by
  font-conformance at the estate level, not here.

## R9 Paint breadth (paint-kit)

- REQ-G1/G2 gradient sampling semantics (linear projection, radial
  distance, premultiplied lerp, clamped t).
- REQ-G3 clip stack as intersecting scissors.
- REQ-I1 nearest-neighbor image blits with premultiplied compositing.

## R10 Widgets (widget-kit)

- REQ-K1 retained model, nested container layout.
- REQ-K2 click pairing (down inside + up inside = one activation).
- REQ-K3 wrapping focus chain; Enter/Space activation.
- REQ-K4 a11y emission with roles, tri-state, focus; audit clean.
- REQ-K5 stock renderer paints every kind.
- REQ-K6 text field: caret-accurate insert/backspace/delete/motion over
  ui-text-edit.
- REQ-K7 accesskit bridge (feature `accesskit`): roles, toggled, value,
  focus on the update.
- REQ-K8 gallery golden hashes: alpha and RGB per palette, determinism
  strict-compare.
- REQ-K9 theme adapter: seed graph WCAG-audited; derived variants
  (incl. CVD) audited and rendered deterministically.

## R11 Process

- Tier B coverage (80 percent) per published crate.
- `render-conformance` publish = false.
- No crate publishes without a recorded consumer (estate ADR-0005).

# fresco SPEC

Status stamp: DRAFT

## 1. Purpose

fresco is the estate's modular retained-mode UI engine: the layers between
UI semantics (theme, a11y, input, text) and pixels (scene graph, compositing,
paint). It exists to give Rust a Qt-shaped stack that the estate owns, with
damage-tracked partial redraw as the core performance contract.

## 2. Truth table (where the truth lives)

| Subject | Truth |
|---|---|
| Crate inventory and layer assignments | `README.md` crates table |
| Normative requirements | This file, section 4 |
| Architecture rationale and own-vs-consume calls | `ARCHITECTURE.md` + `docs/adr/` |
| Paradigm cross-references (651 to 950) | `docs/paradigms.md` |
| Coverage tier and CI gates | `.github/workflows/ci.yml` (tier B, 80 percent) |
| Rendered-output correctness | `render-conformance` tests; golden images in `crates/render-conformance/tests/` |
| Publication status | `Cargo.toml` per crate; `render-conformance` is `publish = false` |
| Competitive position vs Qt/ImGui/egui/iced/Slint | `docs/competitive-analysis.md` (audited 2026-11-01) |

## 3. Layering (normative)

- L0 (`path-kit`, `anim-kit`): std, **zero external dependencies**, `f64`,
  `#![forbid(unsafe_code)]`. std-only by decision: float transcendentals
  (`sqrt`, `sin`, `exp`) are std methods, and a `libm` dependency would
  break the zero-dep claim; a `no_std` resurrection via an optional `libm`
  feature is recorded as future work.
- L0-embedded (`layout-kit`): the one `no_std + alloc` crate: pure
  arithmetic, no transcendentals, zero dependencies.
- L1 (`scene-kit`): std, depends on L0 only. No pixel types.
- L2 (`paint-kit`): std; depends on L0 only. Owns rasterization.
- L3 (`render-conformance`): dev-only, `publish = false`, may depend on
  anything in the workspace.

A lower layer must never name a higher layer's types.

## 4. Requirements

- REQ-P1 (path fidelity): flattening error of any curve segment must not
  exceed the caller tolerance against the true curve.
  Oracle: `cargo test -p path-kit flatten_tolerance`.
- REQ-P2 (predicate robustness): orientation predicates must return the
  correct sign where naive `f64` evaluation returns zero or a wrong sign,
  for determinants up to 1e17 in magnitude.
  Oracle: `cargo test -p path-kit robust_orientation`.
- REQ-P3 (arc length): Legendre-Gauss arc length is exact on lines and
  within 1e-3 relative on the bezier circle approximation.
  Oracle: `cargo test -p path-kit arclen`.
- REQ-A1 (spring exactness): closed-form spring samples match numeric
  integration to 1e-6 and satisfy `u(0) = u0`, `u'(0) = v0`.
  Oracle: `cargo test -p anim-kit`.
- REQ-A2 (settle bound): `settle_time(e)` returns t with `|u(t)| <= e * scale`.
  Oracle: `cargo test -p anim-kit settle_bound`.
- REQ-R1 (coverage correctness): rasterized area of an axis-aligned rect is
  exact; a triangle's total coverage matches analytic area within 1 percent.
  Oracle: `cargo test -p paint-kit coverage_area`.
- REQ-R2 (fill rules): opposite-oriented holes are empty under NonZero;
  same-orientation nested outlines are solid under EvenOdd after parity
  reduction.
  Oracle: `cargo test -p paint-kit fill_rules`.
- REQ-S1 (arena safety): stale generation handles return `None`; slots are
  reused with a bumped generation.
  Oracle: `cargo test -p scene-kit arena_generations`.
- REQ-S2 (damage math): `FramePlan` covers every invalidated rect; regions
  are chosen over full repaint only when the coalescing threshold holds.
  Oracle: `cargo test -p scene-kit damage_plans`.
- REQ-S3 (flatten order): the draw list is sorted by (layer, depth, seq);
  transforms compose parent-to-child.
  Oracle: `cargo test -p scene-kit flatten_order`.
- REQ-L1 (measure cache): repeated identical queries hit the cache; a
  generation bump or changed input hash forces recompute.
  Oracle: `cargo test -p layout-kit cache::tests`.
- REQ-L2 (flex solver): grow, shrink, gap, and min/max clamps follow the
  single-axis flex distribution rules.
  Oracle: `cargo test -p layout-kit flex::tests`.
- REQ-C1 (golden compare): byte-identical surfaces pass; surfaces differing
  beyond the channel delta or the mismatch budget fail with a report; a
  viewable BMP diff is emitted on failure.
  Oracle: `cargo test -p render-conformance`.

## 5. Coverage tier

Tier B: `cargo llvm-cov` line coverage at or above 80 percent per published
crate. CI reports it; the gate number lives in `.github/workflows/ci.yml`.

## 6. Publication

`path-kit`, `anim-kit`, `paint-kit`, `scene-kit`, `layout-kit` publish after
estate-audit + semver-checks + SBOM. `render-conformance` never publishes.
No crate publishes before it has a consumer outside its own tests (estate
ADR-0005 rule), except where this SPEC records one.

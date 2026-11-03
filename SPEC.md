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
| Forward plan, phases, decision gates | `ROADMAP.md` (CURRENT) |

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
- L2 platform (`window-kit`): std; estate traits first (ADR-0003), winit
  and softbuffer behind the default `winit` feature, never in core.
- L2 text (`text-kit`): std; zero-dep core, Parley behind the `parley`
  feature per ADR-0002.
- L3 controls (`widget-kit`): std; internal deps only (layout-kit,
  paint-kit, text-kit, a11y-kit via git). Emits audited a11y trees.
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
- REQ-W1 (facade): the fake runner delivers scripted events in script
  order through the sink, with a present path in scope.
  Oracle: `cargo test -p window-kit fake_runner`.
- REQ-W2 (DPI): physical = logical * scale, and to_logical inverts it.
  Oracle: `cargo test -p window-kit fake_window_dpi`.
- REQ-W3 (present contract): a correctly sized buffer is captured; a
  mismatched buffer is a SizeMismatch error.
  Oracle: `cargo test -p window-kit fake_present`.
- REQ-W4 (frame clock): end without begin is zero duration; statistics
  accumulate per frame.
  Oracle: `cargo test -p window-kit frame_clock`.
- REQ-T1 (deterministic shaping): the fake engine's shape output is a pure
  function of the query; glyph ids are scalar values, advances are
  0.6 em.
  Oracle: `cargo test -p text-kit shape_is_deterministic`.
- REQ-T2 (wrapping): greedy wrap on spaces keeps every line within the
  wrap width, trailing spaces trimmed.
  Oracle: `cargo test -p text-kit layout_wraps`.
- REQ-T3 (atlas placement): shelf packing places glyphs without overlap,
  round-trips coverage bytes, and serves repeat lookups from the same
  rect.
  Oracle: `cargo test -p text-kit atlas`.
- REQ-T4 (atlas eviction): a full atlas evicts least-recently-used glyphs
  and accepts new placements afterwards.
  Oracle: `cargo test -p text-kit atlas_lru_eviction`.
- REQ-G1 (linear gradient): stop interpolation is premultiplied lerp with
  clamped t; projection uses only the axis component.
  Oracle: `cargo test -p paint-kit gradient`.
- REQ-G2 (radial gradient): t is euclidean distance over radius; zero
  radius clamps to the first stop.
  Oracle: `cargo test -p paint-kit radial_gradient`.
- REQ-G3 (clip): pushed clips are intersecting scissors over fills and
  images; pop restores.
  Oracle: `cargo test -p paint-kit clip_scissor`.
- REQ-I1 (images): nearest-neighbor sampling; premultiplied over
  compositing per pixel.
  Oracle: `cargo test -p paint-kit image_blit`.
- REQ-L3 (engine contract): layout engines implement one trait; the estate
  flow engine is the dependency-free reference, Taffy (feature `taffy`)
  the flexbox/grid backend (Gate B).
  Oracle: `cargo test -p layout-kit`.
- REQ-K1 (widget model): retained widgets compose rows and columns; a
  kestrel-shaped screen computes nested rects.
  Oracle: `cargo test -p widget-kit compute_assigns_nested_rects`.
- REQ-K2 (input): paired press/release inside a widget activates once;
  release outside cancels.
  Oracle: `cargo test -p widget-kit click_button`.
- REQ-K3 (focus): wrapping Tab traversal over focusable widgets; Enter and
  Space activate the focused widget.
  Oracle: `cargo test -p widget-kit focus_chain`.
- REQ-K4 (a11y): every widget emits an audited a11y-kit node with correct
  roles, tri-state checkbox values, and single focus; the root is the
  a11y Window.
  Oracle: `cargo test -p widget-kit --test a11y_render a11y_tree_has_roles`.
- REQ-K5 (render): the stock renderer covers every widget kind with
  opaque pixels (paint-kit path).
  Oracle: `cargo test -p widget-kit --test a11y_render paint_tree`.
- REQ-K6 (text field): single-line entry over ui-text-edit; printable
  characters insert at the caret, Backspace/Delete/Home/End/Left/Right
  navigate per the caret model.
  Oracle: `cargo test -p widget-kit text_field_edits`.
- REQ-K7 (accesskit bridge, feature `accesskit`): the widget tree lowers
  to an AccessKit `TreeUpdate` with roles, toggled state, text-entry
  values, and the focused node id.
  Oracle: `cargo test -p widget-kit --features accesskit`.
- REQ-K8 (gallery): every widget kind x palette renders deterministically
  and matches checked-in FNV baselines (alpha and RGB checked apart).
  Oracle: `cargo test -p widget-kit --test gallery`.
- REQ-K9 (theme): the seed token graph audits clean through theme-kit's
  WCAG gate; every derived variant (Light, HighContrast, Monochrome,
  CVD-deuteranopia) audits clean, renders deterministically, and pixels
  differ per variant.
  Oracle: `cargo test -p widget-kit --test theme_gallery`.
- REQ-K10 (scroll): ScrollPane wheel offsets clamp to content bounds;
  scrolling changes rendered pixels; hit testing applies scroll offsets
  while descending.
  Oracle: `cargo test -p widget-kit --test scroll_tabs`.
- REQ-K11 (tabs): TabList click cycles the active tab; focused
  Left/Right steps with wrap; the active tab name surfaces in a11y.
  Oracle: `cargo test -p widget-kit --test scroll_tabs tab_list`.
- REQ-K12 (text): labels render per-character micro-font glyphs, not
  placeholder boxes; distinct characters produce distinct pixels.
  Oracle: `cargo test -p widget-kit --test gallery label_text_pixels`.
- REQ-T5 (micro-font): the 3×5 micro-font covers all printable ASCII,
  is deterministic, and scales by integer factors.
  Oracle: `cargo test -p text-kit microfont`.

## 5. Coverage tier

Tier B: `cargo llvm-cov` line coverage at or above 80 percent per published
crate. CI reports it; the gate number lives in `.github/workflows/ci.yml`.

## 6. Publication

`path-kit`, `anim-kit`, `paint-kit`, `scene-kit`, `layout-kit` publish after
estate-audit + semver-checks + SBOM. `render-conformance` never publishes.
No crate publishes before it has a consumer outside its own tests (estate
ADR-0005 rule), except where this SPEC records one.

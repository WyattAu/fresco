# CHANGELOG

## 0.4.5 (2026-11-06)

Phase 4.1: clip support on the sparse-strip backend.

- `StripTarget::push_clip`/`pop_clip`: an intersecting clip stack.
  Axis-aligned rect draws (solids and gradients) scissor exactly by
  rect intersection - the paint stays in device space and coverage is
  identical to a clip layer; rounded rects draw under a vello clip
  layer when the clip cuts corners, directly when the clip contains
  the shape, and not at all when disjoint. This is the machinery the
  damage-plan path needs to drive the alternative backend.
- Parity: clipped sequences (solid band, gradient window, nested
  intersection) hold the one-step oracle with zero budget; clip BMP
  artifacts written per run.
- Unit tests: clip confinement, disjoint-clip suppression (an empty
  clip entry is distinct from no clip - the first cut got this
  backwards and the test caught it).

## 0.4.4 (2026-11-06)

Phase 4.3: rounded corners reach the stock widget chrome.

- `button_frame` and the `select` header draw rounded (r = 6) with a
  ring-inside-the-rect focus style: the fill insets 1.5 px and the
  focus-ring color shows as a border within the widget rect. This
  preserves the damage invariant that a widget paints strictly inside
  its rect (nothing bleeds into neighbors).
- Checkbox, tabs, tree rows, and select option rows stay sharp-corner
  (they tile or are too small for corners to read); borders on sharp
  widgets unchanged.
- Gallery golden hashes re-bootstrapped (button/select chrome
  changed). CVD per-widget and damage proof tests pass unchanged -
  focus distinctness and damage correctness survive the restyle.
- Parity fixture + gallery BMP/PNG artifacts pushed to a phone
  screen: rounded corners, gradient ramps, and blends visually
  confirmed end to end.

## 0.4.3 (2026-11-05)

Phase 4.1: damage-plan rendering (headless form of damage-region
re-encode).

- `widget-kit`: `paint_tree_plan` - paint the tree constrained to a
  scene-kit `FramePlan`. `Full` delegates to `paint_tree`; `None` is
  a no-op; `Regions` paints once per non-overlapping region with the
  painter clipped to it (scene-kit `take_plan` guarantees
  non-overlap, so every pixel is drawn exactly once).
- Rendering-boundary correctness detail the proof test caught: damage
  clips are pixel-aligned (floor leading edges, ceil trailing edges) -
  a widget edge at a fractional pixel blends into that pixel on a full
  render, and a sub-pixel scissor would leave stale bytes behind.
- Proof tests: a checkbox toggle and a tree-selection move repaint
  byte-identical to full renders; a `None` plan provably leaves the
  frame untouched; region mode produces at most two regions for a
  one-widget change (no full-repaint fallback).
- `Rect2D` is now re-exported from widget-kit (widget rects are
  layout geometry).

## 0.4.2 (2026-11-05)

Phase 4.3 groundwork: rounded-rect fills on both backends.

- `paint-kit`: `Painter::fill_round_rect` (CPU reference: kappa
  quarter-circle cubics through the winding raster) and
  `StripTarget::fill_round_rect` (kurbo RoundedRect on the sparse-strip
  pipeline), radius clamped to half the shorter side on both.
- Parity: rounded-rect draws join the shared fixture. Interior pixels
  hold the one-step oracle; corner arcs carry an explicit 2% pixel
  budget (measured 0.72%): independent curve flattening parameterizes
  the same quarter-circle cubics differently, so a thin corner-edge
  band diverges. Budget and measurement are documented in the test.
- Strip unit test: rounded rect covers its center exactly and leaves
  the square corner empty.

## 0.4.1 (2026-11-05)

Phase 4.1: gradient parity.

- `paint-kit` `sparse-strip`: `StripTarget::fill_linear` and
  `fill_radial` (device-space axes, straight opaque stops, pad
  clamping); `Paint` gains `From<LinearGradient>`/`From<RadialGradient>`.
- `render-conformance` parity fixture extended with linear and radial
  gradient draws. The strict oracle holds unchanged: every pixel
  within one channel step, zero mismatch budget (sRGB premultiplied
  interpolation on both sides; stops must be opaque so premultiplied
  and straight bytes coincide).
- Parity BMP artifacts (cpu + strip) written per run to
  /tmp/opencode for visual inspection.

## 0.4.0 (2026-11-05)

Phase 4.1 groundwork: sparse-strip backend + Gate C parity harness.

- `paint-kit` gains the default-off `sparse-strip` feature:
  `StripTarget` delegates the same fill-rect draw calls to vello_cpu
  0.2's sparse-strip pipeline (single-threaded dispatch, quality-first
  f32 rasterization), with `strip_into_surface` for engine consumers.
- `render-conformance` gains the `parity` feature: a shared rect
  fixture (opaque, semi-transparent, overlapping, edge-adjacent, and
  boundary-clipped draws) rasterized through both backends and
  compared with a strict oracle - every pixel within one channel step,
  zero mismatch budget, plus a vacuity check that a deliberately wrong
  buffer fails. Measured: mismatch_frac=0.00000, max_delta=1.
- This is Gate C's second option (hybrid CPU sparse-strip) made
  swappable by construction: the CPU reference stays primary until a
  measured decision says otherwise (ADR-0004). CI runs the parity gate
  through --all-features.
- Latent convention quirk surfaced by the harness and left documented:
  widget-kit's stock renderer calls `Solid::premul` with straight
  palette bytes - exact for the opaque palettes it uses, wrong for
  future semi-transparent ones (goldens bake the current convention).

## 0.3.7 (2026-11-04)

Phase 3.2 close: select widget.

- `widget-kit`: `Select` dropdown - closed shows the selected option
  with a caret; click opens, click on an option commits + closes,
  header click closes, empty space is ignored. Up/Down (focused) open
  and step with immediate commit; Enter closes. Layout height grows
  with the open state (header + one row per option). A11y emits
  ComboBox with the selected name; AccessKit bridge emits
  Role::ComboBox.
- Gallery gains the select (every 3.2 widget now has a golden entry);
  hashes re-bootstrapped. CVD per-widget suite covers the open
  dropdown's visibility.

## 0.3.6 (2026-11-04)

Phase 3.4 close: per-widget CVD verification.

- `theme_gallery`: widget_states_stay_distinct_under_cvd_variants -
  checkbox checked/unchecked, button focus ring, tree selection, and
  the active-tab marker each stay pixel-distinct under protanopia,
  deuteranopia, and tritanopia simulations. This is the roadmap 3.4
  claim: CVD variants verified per widget, not just per palette.
- `DrawListPainter::tab_strip` implemented (strip fill, per-tab boxes,
  active-tab fill + text-color underline marker, focus border); TabList
  previously rendered nothing through the stock painter.

## 0.3.5 (2026-11-04)

Phase 3: tree-lite widget.

- `widget-kit`: `TreeList` — flat rows with depth and expand state;
  visibility derived (a row shows only when every ancestor is
  expanded), so collapsed subtrees vanish from rendering, hit testing,
  and keyboard selection alike. Click selects + toggles (empty space
  does nothing); Up/Down step visible rows, Left/Right collapse/expand
  (Left on a leaf climbs to the ancestor). `NavKey::Up`/`Down` added in
  `window-kit`. A11y emits `List` with the selected row name. Gallery
  includes the tree; golden hashes re-bootstrapped. 8 tree tests.

## 0.3.4 (2026-11-03)

Phase 3: text rendering path.

- `text-kit`: 3×5 micro-font covering all printable ASCII —
  deterministic per-character coverage bitmaps, integer scaling, glyph
  ids aligned with the FakeEngine shaper.
- `widget-kit`: `WidgetRenderer::label_text` blits micro-font glyphs
  per pixel (premultiplied coverage blend); labels now show
  distinguishable characters instead of solid placeholder boxes.
  Gallery gains a per-character distinctness test.

## 0.3.3 (2026-11-03)

Phase 3 continued: ScrollPane and TabList.

- `widget-kit`: ScrollPane (clipped viewport, wheel clamped to content
  bounds, children translated by scroll offset, flex-shrink-0 overflow
  semantics in layout-kit so scroll content keeps its natural height);
  TabList (click cycles, focused Left/Right steps, active tab rendered
  in the focus color). Both emit audited a11y roles (List, TabList) and
  render through the stock renderer with clip support.
- `layout-kit`: flex overflow now follows CSS flex-shrink-0 semantics
  (zero-shrink children keep their basis and overflow the container —
  the scroll-pane case) instead of the old even-reduction fallback.
- `paint-kit`: Surface implements Default (1x1 transparent) for
  panic-free test fallbacks.
- Facade: hit testing applies ScrollPane offsets while descending, so
  pointer coordinates are viewport-correct at any scroll position.

## 0.3.2 (2026-11-03)

Phase 3.4: theme-driven styling.

- `widget-kit` theme adapter: `seed_graph()` (WCAG-audited dark seed),
  `variant_graph()` / `variant_colors()` (Light, HighContrast,
  Monochrome, CVD-deuteranopia via theme-kit's Oklab retinting),
  `colors_from_graph()` (roles to widget palette). Theme gallery test:
  every variant audits clean through theme-kit's gate, renders
  deterministically, and has distinct pixels.
- Deps: theme-kit 0.1.1, color-kit 0.1.

## 0.3.1 (2026-11-03)

Phase 3 continuation.

- `widget-kit`: TextField over ui-text-edit (caret-accurate insert,
  backspace, delete, Home/End/Left/Right); AccessKit bridge behind the
  default-off `accesskit` feature (roles, toggled state, text-entry
  values, update-level focus); widget gallery golden-hash suite (dark,
  light, high-contrast palettes; alpha and RGB checked apart; strict
  determinism compare).
- Facade: `Event::KeyNav` with `NavKey` (Backspace, Delete, arrows,
  Home, End, Tab, BackTab); winit maps named keys. `NavKey` re-exported
  through widget-kit.
- window-kit is now a default-features-off dependency at the workspace
  level (winit becomes opt-in per consumer).

## 0.3.0 (2026-11-01)

Phase 3 scaffolding (controls).

- `layout-kit` v2: the `LayoutEngine` contract (`NodeStyle` subset, add
  leaf/container, compute, read rects). New estate flow engine: rows and
  columns with gaps, grow distribution through the flex solver, recursive
  main/cross content sizing. Taffy 0.9 backend behind the default-off
  `taffy` feature (Gate B: adoption measured on kestrel screens).
- `widget-kit` (new): retained widget model (Label, Button, Checkbox,
  Row, Column, Spacer), pointer hit testing with click pairing,
  wrapping Tab focus chain (876), Enter/Space activation (875),
  a11y-kit tree emission with clean audits and tri-state checkbox
  values, `WidgetRenderer` contract plus a paint-kit stock renderer
  (label text pixels are a placeholder box until the consumer wires the
  text-kit glyph path). Gate D spike: retained-props API shape.
- Workspace: version 0.3.0; layering extended (L3 controls).

## 0.2.0 (2026-11-01)
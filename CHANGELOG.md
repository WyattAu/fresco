# CHANGELOG

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
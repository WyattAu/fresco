# CHANGELOG

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
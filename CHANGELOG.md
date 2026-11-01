# CHANGELOG

## 0.2.0 (2026-11-01)

Phase 2 of the UI-engine plan (pixels in a window).

- `window-kit` (new): estate windowing facade per ADR-0003. `Window`,
  `Present`, `EventSink`, `FrameClock` traits; headless fake backend with
  scripted events and captured presents; winit 0.30 backend behind the
  default `winit` feature with softbuffer software present (premultiplied
  RGBA8 in, opaque XRGB out).
- `text-kit` (new): the ADR-0002 `TextEngine` trait (shape + layout),
  deterministic FakeEngine (0.6 em monospace model, greedy space wrap),
  shelf-packed A8 glyph atlas with LRU eviction (887), Parley 0.12
  backend behind the default-off `parley` feature.
- `paint-kit`: gradients (linear, radial; premultiplied lerp, device-space
  v0), clip stack (intersecting scissors), nearest-neighbor image blits,
  per-pixel-source row blending. Painter APIs now take `Paint`.
- Workspace: version 0.2.0; layering extended (L2 platform, L2 text).

## 0.1.0 (2026-11-01)
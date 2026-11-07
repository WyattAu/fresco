# ARCHITECTURE

## Why fresco exists

The estate owns UI semantics (L0/L1: colour, tokens, a11y, IME, text edit,
font model and raster) and consumes everything between semantics and pixels
from upstream toolkits (Slint, Tauri, egui, Leptos). That middle is where
performance is won or lost, and it is the part of Qt worth rebuilding on
Rust's terms. fresco owns that middle. The layer plan and the own-vs-consume
analysis come from the estate UI-stack survey; see `docs/adr/` for the four
decisions that bound this design.

## Data flow (steady-state frame)

```text
input (future window-kit)
   |
state mutation (app / future widget-kit)
   |
scene-kit  Scene (generational arena, retained)
   |  invalidate(rect) -> DamageTracker
   |  take_plan() -> FramePlan { Full | Regions([Rect]) }
   |  flatten(plan) -> DrawList (sorted by layer:depth:seq key)
   |
paint-kit  Painter::fill_path / fill_rect
   |  path-kit flatten(tol) -> polylines
   |  winding-integral exact coverage -> premultiplied RGBA8 Surface
   |
present (future: software blit today, swapchain in window-kit)
```

The scene graph is a retained tree in a generational arena (slot semantics
without `slotmap`'s dependency), damage is plain data (rects in, frame plans
out, pure and headless-testable), and flattening emits a `DrawList` whose
u64 sort key packs `layer:depth:seq` so batching is a sort, not a tree walk.

## Own vs consume (the four ADRs)

| Decision | ADR | Own | Consume |
| --- | --- | --- | --- |
| Scene model | 0001 | retained scene + damage (fresco) | reactive layer later (widget-kit) |
| Text | 0002 | `TextEngine` trait | `rustybuzz` + `parley` first behind it |
| Windowing | 0003 | `window-kit` facade trait | `winit` first backend |
| Paint backends | 0004 | `paint-kit` CPU; backend trait | Vello GPU backend (from `ldir-vello` lineage) |

Rationale lives in the ADRs; this file records the outcome, not the debate.

## Performance contract

- Zero per-frame allocation in steady state: caches and arenas are
  caller-owned; `DrawList` reuses its backing buffer across frames.
- Partial redraw: `FramePlan::Regions` repaints only coalesced damage; the
  full-repaint fallback is a threshold decision in `DamageTracker`, tested.
- Exact coverage beats supersampling: one winding-integral pass, no 4x4
  sample loops (CPU); the GPU backend must match it within
  render-conformance tolerance.
- Springs are closed-form (`anim-kit`): animation sampling is O(1) math,
  never per-frame integration, and is frame-rate independent by
  construction.

## Platform notes (Windows, from the paradigm index)

The design keeps hooks for the Windows paradigms recorded in
`docs/paradigms.md` (851 to 860, 911 to 920, 931 to 950): DirectComposition
virtual-surface tiles map onto `DamageTracker` regions (852); flip-model
waitable swapchains replace the present path (855); per-monitor-v2 DPI is an
affine transform at scene root (858); `WM_NCHITTEST` custom titlebars and
`DwmExtendFrameIntoClientArea` are window-kit concerns (911, 912); ETW
markers and PresentMon-style frame pacing are the measurement path for the
"damage redraw beats full repaint" claim (941, 947); headless virtual
desktops run golden images in CI (950).

## Non-goals (0.x)

No widgets, no text, no CSS, no browser engine. fresco is the engine;
estate kits supply semantics; a future `widget-kit` supplies controls.

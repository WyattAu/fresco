# ADR-0001: Retained scene graph with damage tracking, not ECS or immediate mode

Status: ACCEPTED, 2026-11-01

## Context

The estate needs a Qt-shaped stack it owns. The scene model is the highest
leverage decision: it determines the performance contract, the API shape for
every later layer, and which upstream paradigms apply (see
`docs/paradigms.md` items 871 to 874, 930).

## Options

1. **ECS-driven UI** (item 872): nodes as entities, styles as components,
   parallel iteration. Best cache behaviour for huge uniform trees; worst
   ergonomics for retained platform integration (a11y, IME, focus) which is
   inherently hierarchical and stateful.
2. **Immediate mode** (item 873): stateless per-frame rebuilds, id-hash
   state maps. Small API surface, but re-fights every platform integration
   the estate already models as retained semantics (a11y-kit node model,
   input-kit IME state, ui-text-edit buffers).
3. **Retained graph + damage** (chosen): tree in a generational arena,
   invalidation as rect data, frame plans as pure functions of damage.

## Decision

Option 3. Rationale: the estate's L1 crates are already retained-mode
semantics; a retained scene is their natural lowering target. Damage as
plain data is headless-testable, maps directly onto Windows
DirectComposition virtual surfaces (852) and Wayland damage protocols, and
gives the performance claim (partial redraw) a CI oracle
(`render-conformance`). Reactive signal graphs (874) and immediate-mode
escape hatches remain open for `widget-kit`; they are API layers, not scene
model replacements.

## Consequences

- `scene-kit` owns: arena, tree, transforms, damage algebra, frame plans,
  sorted `DrawList` (930).
- No per-frame allocation in steady state; `DrawList` buffer is reused.
- Widget layer (future) will mutate the scene; it does not replace it.

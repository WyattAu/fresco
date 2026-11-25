# ADR-0006: GPU paint backend - sibling crate placement and trigger conditions

Status: ACCEPTED, 2026-11-25

## Context

ADR-0004 ratified a Vello GPU backend behind the paint contract but left
two things open: where it lives, and when it gets built. The M10 video
export arc (ADR-0005) produced the measuring stick that was missing:
the corpus stress bench renders a ~300-shape 480x800 scene at a
release p50 of 67 ms/frame (p95 125 ms), while every shipped consumer
paints interactively at 0.8-2.4 ms per frame on device. The A53 runs
at ~34 fps for present-side reasons (7.5 ms present plus ~18 ms
BufferQueue backpressure) - its raster budget is not the constraint.

## Decision

**Placement: sibling crate, dependency-inverted.** The GPU backend is
`paint-gpu-kit`: it depends on paint-kit for the contract types;
paint-kit never names GPU, wgpu, or vello. The wgpu and vello pins
localize in `paint-gpu-kit` (the ADR-0004 "localized" rule, made
concrete). Consumers opt in behind default-off features: the wgpu
present path lands in window-kit behind a feature; kestrel and other
shells enable it per platform. paint-kit stays dependency-pure because
it is the CI oracle crate.

**Triggers (any one starts the build ladder):**

- T1 (per-pixel effects): a shipped screen needs blur, glass, or heavy
  shadow stacks at interactive rates - the first wall CPU raster hits.
- T2 (scene complexity): a real consumer scene sustains more than
  ~100 shapes per frame against a 60 fps target, measured through the
  video-kit stress corpus (current envelope: ~300 shapes = 67 ms).
- T3 (export throughput): agent-facing fresco video export where CPU
  raster dominates wall-clock at 1080p60 scale.

**Non-trigger (recorded to prevent wrong-tool work):** device fps gaps
attributed to present or BufferQueue backpressure are frame-pacing
work, never GPU-backend work.

**Doctrine that does not change:** the CPU backend remains the CI
oracle and the only lane required for a green build (ADR-0004,
ADR-0005); GPU parity is tolerance-based in render-conformance, one
number, one gate; video-kit corpus golden keys stay CPU-generated;
Android ships CPU-default with device-capability GPU opt-in (Mali-class
compute-shader risk documented, lavapipe is not a device fallback).

**Build ladder when a trigger fires:** extract the paint backend
contract (the corpus proves zero behavior change) -> vello backend for
the corpus subset behind a default-off feature -> parity tolerance
calibration in render-conformance -> wgpu present path in window-kit.
Estimated five to eight sessions to a first GPU-presented frame, gated
at every step.

## Consequences

- The early placement decision removes a later migration: a GPU
  backend inside paint-kit would make the oracle crate carry the very
  dependencies it exists to avoid.
- The start condition is measurable, not vibes: the corpus and stress
  bench are the tripwire, and T1/T2/T3 are checkable against shipped
  screens as they evolve.
- Until a trigger fires, nothing in the tree depends on wgpu - the
  stack keeps its headless, deterministic, zero-GPU CI property.

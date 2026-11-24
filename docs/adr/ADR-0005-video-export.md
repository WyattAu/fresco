# ADR-0005: video-kit exports the stack's own frames; CPU render is the corpus

Status: ACCEPTED, 2026-11-24

## Context

The estate produces animations two ways today: agents call external
engines (Manim, now ManimGX), and fresco renders interactively to
windows. Neither exercises fresco's own renderer over time - the
golden-image gates cover single frames, and no artifact captures how
the renderer behaves across a timeline. The renderer also lacks a
stress vehicle: pacing evidence exists per app (kestrel, fresco-game)
but not as a renderer-level corpus.

## Decision

`video-kit` (app-support layer above paint-kit, unpublished like
render-conformance) owns export: a timed frame loop handing a fresh
zeroed `Surface` plus the frame timestamp to a caller draw closure,
feeding a `FrameSink` trait. The reference sink is a numbered 24-bit
BMP sequence - the estate's zero-dependency image convention - so
exports are deterministic, byte-hashed per frame, and viewable. Real
output backends (an ffmpeg rawvideo pipe into libx264) sit behind
default-off features and are additive, never on the oracle path.
Frame content corpora (per-kind shape/widget scenes with per-frame
golden FNV keys) live in video-kit's tests; the corpus is the
specification for renderer behavior over time, as render-conformance
is for a single frame.

## Consequences

- Determinism contract: same draw closure, byte-identical export.
  Wall-clock is unreachable by construction (the timestamp is the
  only input), so renderer regressions surface as hash diffs.
- The corpus doubles as the renderer stress bench: paint cost per
  frame is measured against the same scenes that pin correctness.
- Backends beyond BMP sequence (mp4) must never be required for a
  green build; CI runners without ffmpeg still run the full oracle.
- ManimGX remains an external tool for math/3D agent video; video-kit
  exports fresco's own stack. No fork relationship (ADR doctrine:
  external tools are consumed, not absorbed).

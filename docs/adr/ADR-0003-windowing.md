# ADR-0003: Windowing facade over winit first, native backends behind traits

Status: ACCEPTED, 2026-11-01

## Context

Every estate desktop app rents windowing today (kestrel: winit via Slint;
ferro, Tachyon, clawdius, nightjar: Tauri/wry). Performance-critical
platform behaviour (frame pacing, DPI, zero-copy present, custom frames)
lives in this layer, but owning four native backends is the most expensive
possible starting point. Paradigm matrices: Windows 851 to 860, 911 to 920;
Linux 951 to 965; macOS 976 to 990; mobile 1001 to 1032.

## Decision

A `window-kit` facade crate defines estate-owned traits (`Window`,
`EventSink`, `FrameClock`, `Present`); the first backend is `winit`. The
traits are shaped by the platform matrices so native backends
(`wayland-client` + `libinput`, `windows-rs` + DirectComposition, ObjC
runtime for AppKit, NDK for Android) can replace individual concerns
without facade changes.

Non-negotiable facade properties, each citing a paradigm:

- Frame clock with presentation feedback (959, 992, 1017, 1032), not a
  busy loop.
- Fractional DPI as affine scale at scene root (858, 956, 990, 1028).
- Damage-driven present (852, 952, 1003): the compositor interface is
  `FramePlan`, never "repaint everything".
- Safe handle wrappers: `Send` not `Sync`, liveness-checked (935).
- Custom-frame checklist recorded per platform (911, 912, 980, 958).

## Consequences

- `window-kit` is the one crate allowed `unsafe` (FFI), reviewed per
  invariant; everything below stays `forbid(unsafe_code)`.
- Measurement hooks (ETW 941, PresentMon 947) are facade features so the
  partial-redraw claim is measurable from day one.

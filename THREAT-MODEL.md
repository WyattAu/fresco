# THREAT-MODEL

Status: 0.1.0, no external-input parsers ship yet. This file is the plan
for when they do.

## Assets

- Rendered output correctness (golden images): tampering would hide visual
  regressions or clone-fighting failures.
- No secrets, no network, no file parsing in any fresco crate today.

## Trust boundaries

| Boundary | Status |
|---|---|
| Font files (font-model already in estate) | out of crate; fresco consumes parsed models only |
| SVG / scene serialization | planned `svg-kit`, v0.x: STRIDE review before first parse |
| Text shaping buffers | behind `text-kit` trait (ADR-0002), upstream-owned risk |

## Planned parser threats (when svg-kit lands)

- Spoofing: scene files claiming provenance; mitigation: content-addressed
  hashes via `render-conformance` keys.
- Tampering: crafted path data causing quadratic blowups; mitigation:
  flattening depth caps (already enforced), vertex-count budgets, fuzz
  harnesses in CI before publish.
- Repudiation / denial of service: recursion depth in nested transforms;
  mitigation: explicit depth limit, error not panic (`panic = "deny"` lint).
- Information disclosure: none; no telemetry, no logging of path data.

## Hardening hooks (window-kit, future)

Windows mitigation policies, AppContainer sandboxing for document
consumers, DPAPI zeroizing, secure desktop credential prompts: recorded as
paradigm hooks 931 to 940 in `docs/paradigms.md`; each becomes a facade
feature behind a capability, default off.

## Fuzzing

`cargo-fuzz` targets are required for any crate that parses bytes, before
its first publish. None exist today because nothing parses bytes.

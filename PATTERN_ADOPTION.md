# PATTERN_ADOPTION: fresco

Relationship to the WyattAu pattern canon
(`github.com/WyattAu/pattern-standards`), validated by
`scripts/adoption-check.py` per `references/adoption-format.md`.

fresco is a retained-mode UI engine. The windowing crate never links a
display server in its core; platform backends and a headless fake sit
behind the Runner trait, so the engine core runs and tests without a
compositor. Layout is a memoized measure cache with generation-based
invalidation.

| Pattern | Status | Evidence |
|---|---|---|
| T13 | adopted | crates/window-kit/src/lib.rs:12 (engine never links a display server; logic fully tested headless via the fake backend) plus crates/window-kit/src/lib.rs:40 (the imperative shell is a backend behind pub trait Runner) |
| T17 | partial | crates/layout-kit/src/cache.rs:1 (memoized measurement cache with generation-based invalidation; a measure memo, not a full query graph) |
| T15 | planned | docs/adr/ADR-0001-scene-model.md and README.md (reactive layer deferred to the phase roadmap; tracking reference per adoption rule 4) |
| P83 | n/a | (buffer hashing in render-conformance and gallery golden hashes are conformance fixtures, not audit chains; a UI engine has no audit surface) |

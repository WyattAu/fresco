# AGENTS.md

Contract for agents and humans working in this repository. Single source;
other agent configs are pointers to this file.

## What this repo is

fresco: the estate's modular retained-mode UI engine (scene graph, damage
tracking, CPU/GPU paint split). Read `SPEC.md` first, then
`ARCHITECTURE.md`, then `docs/adr/`. Paradigm claims live in
`docs/paradigms.md` and must stay in sync with code.

## Commands (canonical; CI mirrors these)

```bash
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo check --workspace --no-default-features
```

Coverage gate (tier B): `cargo llvm-cov --workspace --all-features` at or
above 80 percent per published crate.

## Hard rules

1. `#![forbid(unsafe_code)]` in every crate. No exceptions until
   `window-kit`, and never in this repo today.
2. L0 (`path-kit`, `anim-kit`): std, zero external dependencies, f64.
   `layout-kit` is `no_std + alloc`. Float transcendentals are std
   methods; no_std L0 needs an optional `libm` feature, tracked as
   future work, not built speculatively.
3. Layering: SPEC section 3. Lower layers never name higher-layer types.
4. Lints: `missing_docs`, `panic`, `unwrap_used`, `expect_used` denied
   everywhere; `indexing_slicing` denied in `src/`, allowed in test
   modules only. Use `Option`/`Result`, not panics.
5. MSRV 1.92, edition 2024, lockfile committed.
6. Every requirement in `SPEC.md` section 4 has a named test (the oracle
   line). If you change behavior, change the SPEC and the test in the same
   commit.
7. Docs: no em-dashes, no placeholder text in `llms.txt`, front-load
   conclusions (estate DOC-STYLE).
8. `render-conformance` is `publish = false`. Never remove that.
9. Names: crates keep the `-kit` suffix; no new top-level names without an
   ADR. On crates.io the bare names collide with third parties
   (`path-kit` is owned by songhuaixu), so publish as `fresco-<name>`
   (package rename; keep the `-kit` lib names).
10. The estate manifest (`engineering-standards/estate.yml`) records
   `crates:` as packages **published on crates.io**; `estate-audit.py`
   errors while an entry 404s. Add a crate to that list in the same
   commit as its publish; while unpublished, keep it in this file's
   Layout section only.

## Layout

- `crates/` one crate per directory, workspace members. New in 0.2.0:
  `window-kit` (winit behind default feature), `text-kit` (parley behind
  default-off feature). New in 0.3.0: `widget-kit` (a11y-kit via git dep),
  layout-kit gains the `taffy` feature (default off).
- `docs/adr/` decisions; `docs/paradigms.md` paradigm index (651 to 1050).
- `SPEC.md` requirements and truth table. `REQUIREMENTS.md` traceability.

## Where the truth lives

| Question | Answer |
| --- | --- |
| Requirements and oracles | `SPEC.md` (REQ-P/A/S/L/R/C/K/T/W), oracle lines match test names |
| Traceability index | `REQUIREMENTS.md` |
| Decisions | `docs/adr/` |
| Paradigm index (651 to 1050) | `docs/paradigms.md` |
| Release history | `CHANGELOG.md` |
| Layer map | `README.md` table + `ARCHITECTURE.md` |
| Crate list (published) | `engineering-standards/estate.yml` `crates:` (this repo publishes as `fresco-*`) |
| Layout engine contract | `layout-kit` `LayoutEngine` trait; taffy behind default-off feature |

## When adding a crate

Workspace member, `[workspace.package]` inheritance, lints block copied
from a sibling, layer declared in README table + SPEC section 3, tests with
named oracles, entry in this file's layout note if it changes the map.

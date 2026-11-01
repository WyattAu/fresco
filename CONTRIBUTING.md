# CONTRIBUTING

## The one contract

`AGENTS.md` is the contract for humans and agents alike. This file adds
workflow only.

## Workflow

1. Branch from `main`.
2. `cargo test --workspace --all-features` and
   `cargo clippy --workspace --all-targets --all-features -- -D warnings`
   green before pushing.
3. `cargo fmt --all` applied; no hand-formatting.
4. Update `SPEC.md` REQs and `docs/paradigms.md` status columns in the same
   change as behavior. Docs and code drift in one commit is the failure
   mode this repo refuses.
5. One logical change per PR; the merge queue (estate invariant 11g)
   validates grouped state.

## Layering rule (enforced in review)

L0 crates never depend on L1/L2. `scene-kit` never names pixel types.
Violations fail review even when tests pass. See `SPEC.md` section 3.

## Paradigm claims

`docs/paradigms.md` marks items "applied" only while code implements them.
A PR that removes the implementation must remove the mark in the same PR.

## Commits

Conventional Commits, subject at most 50 characters.

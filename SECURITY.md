# SECURITY

## Reporting

Report vulnerabilities privately: open a GitHub security advisory on this
repository, or route through the estate contact in
`WyattAu/engineering-standards` SECURITY.md. Do not open a public issue.

## Scope for 0.1.0

fresco crates parse no untrusted input, hold no secrets, open no sockets.
The trusted-computing base is the workspace itself. See `THREAT-MODEL.md`
for the plan as parsers land.

## Standing invariants

- `#![forbid(unsafe_code)]` everywhere; the only future exception is
  `window-kit` FFI (ADR-0003), reviewed per change.
- `panic`, `unwrap_used`, `expect_used`, `indexing_slicing` denied by lint
  in every crate.
- Dependencies: zero runtime deps in L0; workspace-internal only in L1/L2;
  dev-only criterion. `cargo-deny` gate before first publish.

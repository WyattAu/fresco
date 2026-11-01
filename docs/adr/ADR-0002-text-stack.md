# ADR-0002: Text stack adopted behind an estate-owned trait

Status: ACCEPTED, 2026-11-01

## Context

fresco owns no text stack. Estate assets: `font-model` (parse, outlines,
cmap 4/12, metrics), `font-shape` (exact-coverage glyph raster,
measurement), `ui-text-edit` (buffer/caret semantics), `input-kit` (IME
preedit FSM). Missing: shaping (GSUB/GPOS), bidi (UAX #9, 889), paragraph
layout (Knuth-Plass, 890), glyph atlasing (887), color fonts (888,
COLRv1/CPAL), variable fonts.

## Options

1. Own a full shaping engine (rustybuzz-class): decade-scale tar pit, zero
   consumers inside the estate today.
2. Consume `rustybuzz` + `parley` (or `cosmic-text`) directly in every
   toolkit: no insulation, upstream churn leaks into all consumers, the
   ropey-2.0 problem repeats.
3. Consume behind an estate-owned `TextEngine` trait (the `ui-text-edit`
   `TextBuffer` insurance pattern): L2 trait owned here, first
   implementation is rustybuzz + parley, swap stays possible.

## Decision

Option 3. The trait lives in a future `text-kit` (L2), defined by the
estate's semantic crates, implemented first by rustybuzz/parley. Windows
gets a DirectWrite adapter (881) as a second implementation; the trait is
the portability contract. `font-shape` remains the L0 raster oracle and
`font-conformance` the differential gate, unchanged.

## Consequences

- fresco 0.x ships no text code; `paint-kit` accepts glyph-positioned
  raster output later via a `GlyphRun` command type.
- Deferred paradigm hooks (885 to 890) are recorded in
  `docs/paradigms.md` under the text phase, not built speculatively.
- Publish gate: no `text-kit` until kestrel consumes it.

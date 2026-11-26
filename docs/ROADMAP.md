# Parity roadmap

Closes the gaps recorded in [comparison.md](comparison.md). Each track
maps to one recorded gap, lists concrete increments with oracles and
effort, and states what we deliberately do not chase. Sequencing
follows the estate doctrine: every increment is oracle-tested, corpus
covered where it touches rendering, and estate-recorded in its
closing commit.

## Verdict first

Three parities are closable now with no new dependencies: **text**
(Parley default-on, IME, RTL corpus), **accessibility** (AccessKit
audit to egui-grade coverage), and **ecosystem** (publish the
layer crates to crates.io, ship an llms.txt). One parity is
ladder-designed and waiting on its recorded trigger: **GPU rendering**
(ADR-0006; the nearest trigger is per-pixel effects). Two are
verification tasks, not feature work: **web** and **iOS**. Two we
deliberately do not chase: full framework breadth (Qt's non-UI stack)
and a designer tool. Widget breadth stays consumer-driven by policy.

## Track TX: text parity (the largest real gap)

Gap: complex scripts, RTL, and IME are unproven; Parley is behind a
default-off feature. egui and Slint both ship system-grade text.

| Inc | Deliverable | Oracle | Effort |
| --- | --- | --- | --- |
| TX-1 | Parley becomes the default text engine (feature flips to default-on); micro-font stays the zero-dep fallback | all widget galleries re-baselined; shaping output stable across two runs | 1-2 sessions |
| TX-2 | IME composition: winit `Ime` events (preedit, commit) flow into text-kit's editing state; in-composition preview renders | scripted composition oracle: preedit string visible, commit lands, backspace during preedit cancels | 2-3 sessions |
| TX-3 | RTL and complex-script corpus scenes (Arabic, Hebrew, Devanagari sample strings through the realfont/Parley path) with per-frame golden keys | corpus scene `text_scripts` matches baselines; runs render right-to-left | 1 session |
| TX-4 | Font fallback chain: system font enumeration through Parley's fontique fallback; missing-glyph path verified | notdef oracle extended: mixed-script string renders with per-run fallback | 2 sessions |

Dependency: TX-1 first (TX-3's corpus keys depend on the shaping
engine). Gallery golden hashes regenerate once (TX-1) and again per
shaking-out change; the corpus pins the settled result.

## Track AK: accessibility parity

Gap: AccessKit bridge young; Qt/egui/Flutter are years ahead.

| Inc | Deliverable | Oracle | Effort |
| --- | --- | --- | --- |
| AK-1 | AccessKit coverage audit: every widget emits the roles egui emits (same audit list from egui's 2026 release notes: popups, menus, modals, panels, resize handles, invisible-widget hiding) | a11y_render oracle extended per role | 1-2 sessions |
| AK-2 | Full keyboard parity: focus ring visibility, tab chain over every focusable widget, Enter/Space activation everywhere | focus_chain oracle extended to the full widget set | 1 session |
| AK-3 | Screen-reader smoke on one real platform (Linux ORCA via AccessKit's at-spi bridge), evidence recorded | manual evidence + checklist in the estate | 1 session |
| AK-4 | Text field a11y: value announcements, placeholder role, selection state | a11y tree assertions | 1 session |

## Track GP: GPU rendering (ADR-0006 ladder, restated for parity)

Gap: blur and per-pixel effects have no fast path; every competitor
with a GPU renderer ships them.

The ladder is already designed and doctrine-gated; closing this
parity means deciding to start it. The concrete effect demand is
shadow quality today and backdrop blur tomorrow (the recorded T1).

| Inc | Deliverable | Oracle | Effort |
| --- | --- | --- | --- |
| GP-1 | Extract the paint backend contract (trait) from paint-kit; the CPU backend becomes the reference implementation | video-kit corpus keys byte-identical (zero behavior change) | 1-2 sessions |
| GP-2 | `paint-gpu-kit`: Vello backend for the corpus subset (solid, round rect, gradients, paths) behind a default-off feature | render-conformance parity within the calibrated tolerance | 2-3 sessions |
| GP-3 | Parity tolerance calibration: one number, one gate | conformance gate green on CPU-only CI; GPU lane green on the discrete-GPU machine | 1 session |
| GP-4 | wgpu present path in window-kit (kills the CPU present copy) | pacing evidence: present cost collapses vs softbuffer | 1-2 sessions |
| GP-5 | Effects: backdrop blur, glass; the parity-closing deliverable | corpus gains an effects scene with keys (CPU reference renders the same scene via a defined fallback) | 2 sessions |

Total: five to eight sessions, matching ADR-0006's estimate. Starting
GP-1 flips the ADR-0006 trigger status: record which trigger fired
(recommended: declare T1 by product decision, since the demand is
known) before writing code.

## Track EC: ecosystem parity

Gap: one vendor, no external consumers, no public registry presence.

| Inc | Deliverable | Oracle | Effort |
| --- | --- | --- | --- |
| EC-1 | Publish the layer crates to crates.io as `fresco-*` (path, anim, layout, scene, paint, text, widget, window, audio; publish=false crates stay unpublished: render-conformance, video-kit, fresco-game, kestrel-gui, fresco-android) | `cargo publish --dry-run` per crate; estate-audit green after the manifest rows land in the same commits | 2-3 sessions |
| EC-2 | `llms.txt` for the repo (the ManimGX pattern: agent primer linking SPEC, ADRs, examples as Markdown) | file exists, links resolve | 1 session |
| EC-3 | Examples gallery crate: one runnable example per layer (the corpus scenes as `cargo run` examples) | examples build in CI | 1 session |

EC-1 is a public-release decision for the estate owner: once
published, versions are permanent. The CHANGELOG 0.8.7 consolidation
and the comparison doc are the prerequisites, and both are done.

## Track WEB: web verification

Gap: not a verified target.

| Inc | Deliverable | Oracle | Effort |
| --- | --- | --- | --- |
| WEB-1 | Verify the existing stack on wasm (winit web + softbuffer's web backend, or record the blocker) | a corpus scene renders in a headless browser check, or the gap is estate-recorded as unsupported | 2 sessions |

Decision point after WEB-1: if softbuffer-web works, the zero-GPU
story extends to the browser for free; if not, web waits for the GPU
backend (wasm wgpu).

## Track IP: iOS (decision-gated)

Requires Apple developer infrastructure and a device; the Android
milestones (M5, x packaging, S7 verification) are the template.
Recommendation: defer until a product needs it. Not scheduled.

## Deliberately not chased

- Full framework breadth (Qt's networking/multimedia/WebEngine
  stack): the estate composes specialized crates instead.
- Native platform controls (wxWidgets/Toga model): fresco is a
  custom-rendered engine by identity.
- Designer/hot-reload tooling: deferred until a product's iteration
  speed demands it; the inspector and the corpus cover correctness
  today.
- Widget breadth for its own sake: stays consumer-driven (kestrel and
  fresco-game are the demand signal).

## Sequencing

1. **Now**: TX-1, AK-1, AK-2 (biggest real parity per session).
2. **Then**: TX-2..TX-4, AK-3..AK-4, EC-2, EC-3.
3. **Decision point**: EC-1 (public crates.io release) and GP-1
   (declare the T1 trigger) - both are estate-owner calls.
4. **After**: GP ladder to GP-5, WEB-1, IP decision.
5. Continuous: every increment above extends the corpus; the estate
   notes record each closing commit.

Progress against this file is reviewed the same way as everything
else: an increment ships with its oracle, the corpus pins any render
change, and the estate ledger carries the closing commit.

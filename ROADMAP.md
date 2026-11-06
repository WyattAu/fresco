# fresco ROADMAP

Status stamp: CURRENT (living document; revisit at every decision gate,
section 5). Audited against docs/competitive-analysis.md 2026-11-01.

## 1. Where we are

Phase 1 shipped: six crates, 5,536 own lines, 101 tests, tier B coverage,
CI green. The competitive analysis measures the honest position: core
algorithms (raster, geometry, scene graph, damage, springs) are
class-correct; windowing, text shaping, GPU, and widgets are not started;
the estate's L1 semantics stack is unique in-tree among all compared
projects. Closure to "a real estate app runs on fresco" is 8,000 to 13,000
own lines plus roughly 250k lines of consumed upstream.

## 2. The organizing thesis

fresco does not win by being Qt. It wins by being the only UI engine whose
semantic layers are first-class and CI-gated: colour-contrast-gated theming
with CVD-derived variants (color-kit + theme-kit), audited accessibility
trees (a11y-kit), IME-correct text semantics (input-kit + ui-text-edit),
exact-coverage rendering with a golden-image oracle (paint-kit +
render-conformance). Every phase must end with at least one of those claims
stronger. "Accessibility- and colour-verified rendering" is the flagship
feature, not a checkbox.

Consumers gate features (estate ADR-0005): every phase names its consumer
before code. kestrel-gui is the anchor; ferro-desktop follows only if
Phase 2 earns it.

## 2b. Progress log

- 2026-11-01: Phase 2.1 window-kit landed (facade + fake backend + winit
  feature, present via softbuffer). Phase 2.2 text-kit landed (trait,
  FakeEngine, glyph atlas 887, Parley 0.12 backend behind `parley`).
  Phase 2.3 raster breadth landed (linear/radial gradients, clip stack,
  nearest-neighbor image blits). 124 tests green. Phase 2.4 (kestrel
  alpha) and 2.5 (estate.yml registration) remain.
- 2026-11-03: Phase 3 landed in steps (0.3.0-0.3.3): layout-engine
  contract (FlowEngine + Taffy feature), widget-kit retained-props
  spike (hit testing, focus chain 876, activation 875, a11y
  emission), TextField over ui-text-edit, AccessKit bridge, theme-kit
  styling resolver with audited CVD gallery, ScrollPane, TabList.
- 2026-11-03: text rendering path (0.3.4): 3x5 micro-font in text-kit
  (all printable ASCII, deterministic), `label_text` blits
  per-character glyphs - labels show distinguishable characters
  instead of placeholder boxes.
- 2026-11-04: tree-lite widget (0.3.5): TreeList (derived visibility,
  click select + toggle, visible-row keyboard nav, Left/Right
  collapse/expand, a11y List + selected name). NavKey gained Up/Down.
  Gallery includes the tree (hashes re-bootstrapped). 158 tests green.
- 2026-11-04: Phase 3.4 closed (0.3.6): per-widget CVD verification -
  checkbox state, focus ring, tree selection, and active-tab marker
  each pixel-distinct under protanopia/deuteranopia/tritanopia; stock
  tab_strip implemented (TabList rendered nothing before).
- 2026-11-04: Select dropdown landed (0.3.7) - the last Phase 3.2
  widget; every 3.2 widget now has a golden gallery entry. Phase 3
  headless items complete; kestrel-gui alpha (Phase 2.4) remains.
- 2026-11-05: Phase 4.1 groundwork (0.4.0): paint-kit `sparse-strip`
  feature (vello_cpu 0.2 backend, Gate C option B) + render-conformance
  `parity` harness - both backends agree within one channel step on
  the shared fixture (mismatch_frac=0, max_delta=1), strict gate runs
  in CI via --all-features.
- 2026-11-05: gradient parity (0.4.1): linear + radial gradient draws
  in the shared fixture; the strict one-step oracle holds unchanged.
  StripTarget gains fill_linear/fill_radial.
- 2026-11-05: rounded-rect fills (0.4.2, 4.3 groundwork): CPU kappa
  cubics vs kurbo RoundedRect on the strip pipeline; interior pixels
  hold one-step parity, corner arcs under an explicit 2% budget
  (measured 0.72%). First 4.3 chrome primitive.
- 2026-11-05: damage-plan rendering (0.4.3, 4.1): paint_tree_plan
  driven by scene-kit FramePlan - damaged repaint byte-identical to
  full render with pixel-aligned region clips; None is a no-op. This
  is the headless form of damage-region re-encode; the strip backend
  consumes the same plan when a compositor drives it.
- 2026-11-06: rounded chrome (0.4.4): buttons and select headers draw
  rounded with ring-inside-rect focus (damage invariant preserved);
  gallery re-bootstrapped; parity fixture verified on a phone screen
  end to end.
- 2026-11-06: clip stacks on the strip backend (0.4.5): scissor-exact
  rect intersection for rect/gradient draws, clip layers for corner-
  cutting round rects; clipped-sequence parity holds the one-step
  oracle with zero budget.
- 2026-11-06: strip-backend damage proof (0.4.6): the compositor
  model (previous frame + clip-confined region repaints) is byte-
  identical to a full render on the strip pipeline - the damage story
  is backend-complete. Phase 4.1 headless work closed.

## 3. Phases

### Phase 2: Pixels in a window (0.2 to 0.4) -> kestrel alpha

Consumer: kestrel-gui (replaces its Slint shell).

- 2.1 `window-kit`: estate traits (`Window`, `EventSink`, `FrameClock`,
  `Present`) + winit backend + software present path (blit `Surface` via
  softbuffer). DPI as root affine (858, 956, 990). ~1,500 lines.
- 2.2 `text-kit`: `TextEngine` trait per ADR-0002 + Parley backend
  (shaping, runs, bidi) + glyph atlas shelf packing (887) + `GlyphRun`
  draw command. font-shape stays the raster oracle. ~2,000 lines
  (Parley 11.8k + rustybuzz 29.2k stay out-of-house).
- 2.3 paint-kit breadth: linear and radial gradients, rect clip stack,
  image blits (media-kit decode to premultiplied), raster-composited
  strokes. ~4,000 lines to tiny-skia practical core.
- 2.4 kestrel alpha: compose + reader screens on fresco. Exit criteria:
  kestrel opens and reads mail in a fresco window; p50 frame under 8 ms
  CPU on the estate dev machine; damage-redraw beats full repaint in a
  measured bench (the 947 measurement hook), or the claim is dropped.
- 2.5 estate.yml registration, 0.2/0.3/0.4 releases, paradigm ledger rows
  moved to "applied" in the same commits.

### Phase 3: Controls (0.5) -> widget-kit beta

Consumer: kestrel full chrome; estate-integration dogfood suite.

- 3.1 layout-kit v2: `LayoutEngine` trait, taffy backend for flexbox and
  grid (~1,000 glue lines), single-axis solver kept for the no_std path.
- 3.2 `widget-kit`: retained widgets over scene-kit: label, button, text
  field (ui-text-edit core), checkbox, select, scroll pane, tab, tree-lite.
  Affine focus chain (876), typed FSM navigation (875), shortcut routing
  (tui-actions lineage). 8,000 to 15,000 lines.
- 3.3 a11y bridge: a11y-kit tree lowered to AccessKit; Narrator and Orca
  smoke tests in CI (901, 970, 984).
- 3.4 styling: theme-kit tokens through a widget style resolver; CVD
  variants verified per widget, not per palette.
- Exit criteria: kestrel settings and compose screens fully fresco; a11y
  tree verified through AccessKit on one platform; widget kit passes a
  golden-image gallery (every widget, every theme variant, one image).

### Phase 4: Compositor-class rendering (0.6) -> GPU + effects

- 4.1 GPU `PaintTarget` backend: Vello per ADR-0004, content-addressed
  pipeline cache (864), damage-region re-encode, parity gate against the
  CPU reference through render-conformance. 1,000 to 2,000 glue lines
  (Vello's 35.7k stays out-of-house).
- 4.2 Present paths: flip-model waitable swapchain (855), Wayland frame
  callbacks + fractional scale (956, 959), CAMetalLayer (991).
- 4.3 Effects: SDF shadows and blurs (926), rounded-rect fast paths.
- 4.4 Instrumentation: ETW markers (941), PresentMon-style pacing stats
  (947) as a Windows CI bench; frame-time p99 added to kit-conformance.
- Exit criteria: GPU parity green; 120 Hz capable pacing on one machine per
  platform; no frame-time regression gate red.

### Phase 5: Platform hardening and ecosystem (0.7+, 1.0 gate)

- Custom frames and titlebars (911, 912, 980, 958), clipboard (919, 963),
  drag and drop (906), tray and taskbar (904, 905).
- Security checklist for document consumers: ACG, CFG, AppContainer notes
  (931 to 934), DPAPI zeroizing (936).
- `svg-kit`: typed SVG model + renderer (resvg-class parity documented as
  gaps, not hidden).
- 1.0 gate: kestrel ships a release on fresco; coverage tier B held across
  published crates; a11y verified on two platforms; the performance claims
  in this file all carry CI benches.

## 4. What we will not build

Each with the measured reason (docs/competitive-analysis.md):

1. No declarative-UI compiler. Slint pays 87,657 lines, Qt QML 121,137.
   Widgets stay Rust-native.
2. No shaping engine. HarfBuzz is decades of work; rustybuzz behind a trait
   is the insurance pattern (ADR-0002), the same move as ui-text-edit's
   TextBuffer.
3. No GPU abstraction beyond `PaintTarget`. wgpu and Vello stay
   out-of-house.
4. No browser or web view. The estate already has Tauri for web-content
   apps; fresco replaces Slint-class native shells, not WebView shells.
5. No audio, no networking UI stacks: estate kits own those domains
   separately.

## 5. Decision gates

- **Gate A (end Phase 2):** is winit's frame pacing good enough for
  kestrel, or does the Wayland-native path (wl_surface.frame +
  wp_presentation_feedback, item 959) need to come forward? Evidence: the
  Phase 2 bench numbers.
- **Gate B (mid Phase 3):** taffy vs own constraint solver (Cassowary,
  877), measured on kestrel screens. Default: taffy.
- **Gate C (Phase 4):** Vello full compute vs the hybrid CPU sparse-strip
  path as primary, depending on upstream release cadence. The parity gate
  makes this swappable by construction.
- **Gate D (before widget-kit):** widget API shape: retained property
  structs vs reactive signal graph (874). Prototype both against kestrel
  settings; the signal graph is the front-runner on paper, the spike
  decides.

## 6. Risks

| Risk | Mitigation |
|---|---|
| Vello API churn (0.3 to 0.4 was a rewrite) | localized in one L2 crate; parity gate keeps swaps honest |
| Parley/rustybuzz script-coverage gaps | font-conformance differential suite extended before text-kit publishes |
| Widget API churn | Gate D spike before any publish; kestrel is the only consumer until 0.5 |
| Performance claims rot | every perf sentence in this file must trace to a CI bench or it gets deleted |
| Scope creep | the no-list, section 4, is contractual; additions require an ADR |
| Upstream toolchain drift (stable clippy gates) | CI stable + MSRV jobs already enforce; local check with cargo +stable clippy before push |

## 7. Measurement

Every phase exit is a taggable release with: REQ oracles green, paradigm
ledger rows synced, coverage tier held, one consumer demonstrably running,
and any performance claim backed by a bench. The competitive analysis gets
re-audited at each gate so the "how far behind" answer stays measured.

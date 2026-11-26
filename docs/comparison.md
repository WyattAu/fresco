# fresco vs the GUI landscape

A positioning comparison against Rust GUI frameworks, Rust rendering
engines, and the established cross-language stacks. Written 2026-11-26;
version and capability claims are sourced and dated in
[Sources](#sources). Claims about fresco itself are either measured in
this repository or marked.

## Verdict first

fresco is not a general-purpose application framework, and the
comparison below does not pretend otherwise. It is a retained-mode,
deterministic, CPU-first UI engine with a byte-exact testing doctrine,
a zero-GPU CI story, first-class offscreen video export, and permissive
licensing with no embedded restrictions. Against the landscape:

- vs **Qt**: Qt is 30 years ahead in breadth (widgets, styles, i18n,
  multimedia, networking, embedded board support). fresco offers Rust,
  no unsafe today, MIT/Apache licensing, no deployment plugin maze,
  and a rendering pipeline that is byte-reproducible in CI on machines
  with no GPU.
- vs **iced** (closest architectural cousin): both ship a software
  renderer and both value testing. iced has the more mature widget set,
  integrated end-to-end testing and time-travel debugging; fresco has
  byte-exact golden images, a temporal render corpus, and mp4 export
  of its own frames as first-class crates.
- vs **egui**: egui is immediate mode with a far larger community and
  ecosystem; fresco is retained mode with damage tracking. egui's
  kittest can record test videos; fresco's renderer output is
  hash-pinned per frame over time.
- vs **Slint**: Slint is production-grade with a declarative language,
  live tooling, and embedded board support, under a tri-license where
  embedded use is paid. fresco is Rust-API-only and MIT/Apache with no
  embedded carve-outs, but has a fraction of the widget breadth.
- Unique to fresco in this set: the renderer is pinned by per-frame
  FNV keys across time (a temporal corpus), and video export is a
  workspace crate, not an example.

## What fresco is

A layered Rust workspace (MSRV 1.92, edition 2024, `forbid(unsafe)`
with zero unsafe in the tree today, MIT OR Apache-2.0): path and
animation at L0 (zero dependencies), a no_std layout core, a retained
scene with damage tracking, an exact-coverage CPU raster as the
reference renderer, widget and text layers behind default-off
integrations (Parley, AccessKit, taffy), winit/softbuffer present,
plus application-layer crates for audio synthesis, deterministic
video export, charts, physics, ECS, a game, and a consumer shell
(kestrel-gui). 510 workspace tests. The Android story is real: a
Galaxy S7 (API 26, 2016 SoC) renders the shipped game at 60 fps
(measured 2026-11-25, paint p50 2.9 ms).

Measured fresco numbers used below (all from this repository's gates,
2026-11-24/26): desktop paint p50 0.27 ms (release, 360x640, Gate A
soak, 99.3 percent of frames inside 16.7 ms); S7 paint p50 2.9 ms,
present 7.3 ms at 60 fps; Pixel paint 0.8-0.9 ms; CPU stress envelope
300 shapes at 480x800 = 67 ms/frame p50 release; fresco-game APK
1.08 MB, kestrel APK 1.05 MB.

## Rust GUI frameworks

| | fresco | egui 0.37 | iced 0.14/0.15-dev | Slint 1.19 | Xilem/Masonry 0.4 | gpui (Zed) | Dioxus Native/Blitz |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Architecture | retained scene + damage tracking | immediate mode | Elm (messages) | declarative `.slint` language | SwiftUI-style views diffed into retained widgets | own retained model, own platform layer | HTML/CSS in Rust, Linebender stack |
| Renderer | exact-coverage CPU raster (reference); Vello GPU planned (ADR-0004) | wgpu 30 (triangles) | wgpu 27/30, tiny-skia fallback, vello-cpu option | FemtoVG, Skia, Qt, software; experimental Vello | Vello + parley stack | own GPU renderer | Vello lineage, wgpu version skew (two at once) |
| GPU dependency for CI | none (headless, no GPU) | none for logic, wgpu for render | headless mode exists | none (software renderer) | GPU for rendering | yes | yes |
| Render determinism | byte-exact, hash-pinned per frame over time | snapshot tests via kittest | snapshot/e2e testing, time travel | screenshot testing (tooling add-on) | not established (pre-1.0) | not published | not established |
| Text | 3x5 micro-font + embedded DejaVu realfont; Parley behind a feature | system fonts via fontique, emoji rasterizers | cryoglyph | Fontique + Parley (unified 1.14) | Parley 0.6 + HarfRust shaping | cosmic-text | Parley |
| Accessibility | audited a11y tree emission; AccessKit bridge behind a feature | AccessKit, deep (2026 releases) | partial (ecosystem split) | yes (commercial tooling add-ons) | AccessKit | custom (Zed's own) | AccessKit |
| Platforms verified here | Linux, Windows, macOS (winit), Android (API 26 S7) | web, native, game engines | Windows, macOS, Linux, web | desktop, Android, iOS, web, embedded (MCU to Linux) | early | macOS-first, others via Zed | early (Blitz) |
| License | MIT OR Apache-2.0 | MIT OR Apache-2.0 | MIT | GPL-3 / royalty-free (desktop+mobile, attribution) / commercial for embedded | MIT/Apache | Apache 2.0 | MIT/Apache |
| Maturity | single-vendor, 0.8.7, 510 tests, pre-1.0 | community standard, 33M downloads, Rerun-sponsored | 30k stars, batteries included, pre-1.0 | production, company-backed (SixtyFPS), 1.19 | Linebender flagship, pre-1.0 | proven at scale inside Zed | pre-1.0 |
| APK/binary weight (this repo, measured) | 1.05-1.08 MB | typical wgpu apps larger (not measured here) | typical wgpu apps larger (not measured here) | small (embedded heritage) | not measured | large (Zed-scale) | not measured |

Ecosystem context (Rust GUI frameworks book, snapshot 2026-09-30):
wgpu spans six major versions across frameworks (0.19.4 to 30.0.1);
Dioxus Native links two at once. fresco sidesteps the skew by having no
wgpu anywhere; the planned `paint-gpu-kit` localizes that pin when the
ADR-0006 triggers fire.

## Rust rendering engines

| | fresco paint-kit (CPU) | vello (GPU) | vello_cpu / sparse-strip | tiny-skia | Skia | Qt RHI/Quick |
| --- | --- | --- | --- | --- | --- | --- |
| Execution | exact-coverage winding scanline, CPU | compute-shader GPU, wgpu | CPU strips (Vello lineage) | CPU skia-small subset | CPU+GPU (SkiaGraphite) | RHI over GPU APIs |
| Determinism | byte-exact by construction | driver-invariant math, blending may vary by driver | deterministic | deterministic | large surface, versioned | versioned |
| Role here | the CI oracle; every golden key pins it | planned second backend (ADR-0004/0006), parity-gated | optional feature (`sparse-strip`) | not used | used by Slint/Freya | used by Slint's Qt backend |
| Antialiasing | exact coverage (analytic) | GPU mesh A8 | strip coverage | analytic | MSAA/analytic | vendor |

## Cross-language stacks

| | fresco | Qt 6.12 LTS | GTK 4 | Flutter 3.44 | Tauri 2 | Avalonia |
| --- | --- | --- | --- | --- | --- | --- |
| Language | Rust | C++, QML, Python bindings | C, Rust (gtk4-rs), others | Dart | Rust + web frontend | C#, XAML |
| UI model | retained scene graph, Rust widgets | Qt Widgets + Qt Quick/QML scene graph | retained widget tree, CSS-adjacent theming | own widget system, everything renders itself | system webview + Rust core | own XAML-rendered |
| Rendering | CPU raster reference; Vello planned | software raster + GPU scene graph (hardware-accelerated 2D in 6.11) | GPU (GSK) | Impeller (own engine) | browser engine | Skia |
| Determinism/testing | golden images + temporal corpus + headless CI, byte-exact | QML hot reload, test framework (commercial add-ons for GUI tests), no byte-exact render contract | no byte-exact contract | widget tests, golden files possible, no byte contract | web testing | no byte-exact contract |
| Mobile | Android verified (API 26 S7); iOS unverified | Android supported (15/16 in 6.10); iOS supported | not a focus | its strongest tier | Tauri 2 mobile | iOS/Android support |
| Embedded | unrestricted by license (no board support shipped) | the industry default; commercial | some | some (embedded Flutter exists) | no | no |
| License | MIT OR Apache-2.0 | LGPL-3 / commercial (static linking and some modules paid) | LGPL | BSD-3 | MIT/Apache | MIT |
| Scale of stack | one repo, ~20 crates, zero GPU deps | full framework: network, multimedia, WebEngine, device creators | large, Linux-desktop-oriented | large, Google-backed | runtime = the platform webview | medium |
| Maturity | 0.8.7, single-vendor, pre-1.0 | 30+ years, LTS until 2031 (6.12), CRA compliance in 6.12 | decades, GNOME-backed | Google-backed, huge community | active, growing | mature community |

## fresco vs Qt, specifically

The estate calls fresco its modular Qt. The comparison that matters:

What Qt gives that fresco does not (2019-2026 release cadence:
6.10 through 6.12 LTS):

- Widget and control breadth with platform styles, plus QML with hot
  reload and a declarative language; fresco's widget set is
  deliberately consumer-driven and small.
- Stack breadth: networking, multimedia, WebEngine/WebView2, graphs,
  Bluetooth, NFC, sensors; fresco's non-UI crates are audio, video,
  charts, physics, ECS.
- Accessibility maturity (platform AT integration reviewed every
  release, high-contrast alignment in 6.10); fresco's AccessKit bridge
  is behind a feature and young.
- Text: full shaping, IME, RTL and complex scripts at production
  quality; fresco's Parley integration is optional and its complex
  script support is unproven.
- Embedded board support and a commercial support organization; two
  of Qt's 2026 highlights are EU CRA compliance and HarmonyOS support.
- Tooling: Qt Creator, designer tools, profiler, QML hot reload.

What fresco gives that Qt does not:

- Rust with `forbid(unsafe)` in every crate (zero unsafe in the tree
  today); Qt is C++ with PIMPL and raw pointers throughout.
- Licensing with no carve-outs: MIT OR Apache-2.0, including embedded;
  Qt requires LGPL dynamic-linking discipline or commercial terms, and
  static linking or some modules are paid.
- A byte-exact render contract: golden images plus a per-frame corpus
  mean a rendering change is a reviewed diff, and CI needs no GPU.
  Qt has no byte-exact rendering contract at all.
- Deterministic offscreen video export as a workspace crate
  (video-kit): frames plus an audio track into mp4, same bytes every
  run, headless. Qt's closest equivalents (QScreen grab, offscreen
  surfaces) are neither deterministic nor CI-pipeline-grade.
- Deployment shape: two APKs at ~1 MB each with no plugin directory,
  no OpenSSL bundling, no QML cache. Qt deployment (platform plugins,
  OpenSSL, QML caches, WebEngine binaries) is a documented failure
  surface.
- Dependency shape: no wgpu, no version-skew problem, workspace
  crates resolve to one version each by policy.

Honest bottom line: for a product that needs mature widgets, i18n,
accessibility compliance, and embedded support contracts, Qt wins
today and will for years. For an estate that needs deterministic,
licensed-simple, Rust-native UI with a renderer that CI can hash,
fresco is the only option in its own niche, and the gaps are known and
tracked (ADR-0004/0006 for GPU, Parley for text, AccessKit for a11y).

## Determinism and testing compared

This is fresco's core claim, so it gets its own table.

| | Byte-exact render contract | Temporal (per-frame) pinning | Headless CI without GPU | Offscreen video export in-tree | Deterministic audio in-tree |
| --- | --- | --- | --- | --- | --- |
| fresco | yes (render-conformance golden images) | yes (video-kit corpus, per-frame FNV) | yes (CPU backend by design) | yes (video-kit mp4 + WAV mux) | yes (audio-kit) |
| egui | snapshot tests (image compare in kittest) | no (test videos are recordings, not pinned) | logic yes, render via wgpu | kittest can record mp4 of tests (2026) | no |
| iced | snapshot/e2e testing | no | headless mode | no | no |
| Slint | screenshot testing (paid add-on tier) | no | software renderer yes | no | no |
| Qt | no | no | offscreen plugin exists | grab-based, not deterministic | Qt Multimedia |
| Flutter | golden file tests | no | flutter test runs headless | no (plugin ecosystem) | no |

## Performance

Measured fresco numbers (this repository, release builds, 2026-11):

- Desktop 360x640 full-screen repaint through the CPU raster: paint
  p50 0.27 ms, p99 1.7 ms (Gate A 3000-frame soak).
- Galaxy S7 (2016, API 26): shipped game at 60 fps, paint p50 2.9 ms,
  present 7.3 ms.
- Pixel 10 Pro: paint 0.8-0.9 ms at 60 fps.
- CPU stress envelope: 300 shapes at 480x800 = 67 ms/frame p50
  release. Interactive UI is far inside budget; dense 60 fps
  animation is the recorded trigger for the GPU backend (ADR-0006).

Comparative performance claims for other frameworks are not reproduced
here on this hardware; published benchmarks are vendor-run and not
comparable across machines. The honest statement: fresco's CPU raster
covers interactive UI with large margin on every device measured, and
its dense-animation ceiling is known and recorded.

## Honest gaps in fresco

- Community: one vendor, pre-1.0, no external contributors. egui has
  33M downloads and 1470 dependents; Slint has a company and an
  embedded customer base.
- Widget breadth: consumer-driven by policy. No data grid, no rich
  text editor, no dock/menus beyond the youngest states.
- Text: complex scripts, RTL, and IME are unproven; Parley is behind a
  default-off feature. egui and Slint both ship system-grade text
  today.
- Accessibility: emission and an AccessKit bridge exist but are young;
  Qt, egui, and Flutter are years ahead.
- GPU rendering: planned and doctrine-gated (ADR-0004/0006), not
  shipped; blur and per-pixel effects have no fast path today.
- Web and iOS: not verified targets. Slint, egui, iced, Tauri, and
  Flutter all ship them.
- Tooling: no designer, no hot reload, no inspector UI; the estate
  relies on oracles and evidence files.

## When to pick what

| Need | Pick |
| --- | --- |
| Deterministic UI whose renders CI can hash, headless, no GPU | fresco |
| Rust product needing the largest widget ecosystem today | egui |
| Rust Elm-style product with integrated e2e tooling | iced |
| Declarative language, embedded devices, company support | Slint (mind the embedded license) |
| Zed-style editor or macOS-first product | gpui |
| HTML/CSS model in Rust | Dioxus/Blitz |
| Long-lived C++ product, embedded contracts, full framework breadth | Qt |
| Google-aligned mobile-first product | Flutter |
| GNOME/Linux-first desktop product | GTK 4 |
| Web-frontend team shipping a desktop shell | Tauri |

## Sources

- egui 0.37.0 release notes and crates.io page, 2026-10-15 (kittest
  video recording, AccessKit depth, fontique system fonts).
- iced releases and crates.io (0.14.0, 2025-12-07; master wgpu 30
  upgrade PR 3420, merged 2026-10-03; headless mode, time travel,
  tiny-skia and vello-cpu renderers).
- Rust GUI frameworks book, ecosystem snapshot 2026-09-30 (wgpu/
  winit/taffy/accesskit/parley version-skew tables).
- Slint 1.14 release blog 2025-10-21 (Fontique/Parley unification),
  Slint changelog (1.18.0, 2026-09-16: WGPU 30 + experimental Vello
  renderer; 1.19.0, 2026-11-16: editions), Slint LICENSES and pricing
  pages (triple license, embedded commercial from $1/device).
- Linebender blog, October 2025 (Masonry/Xilem 0.4, HarfRust shaping,
  Parley 0.6, ICU4X migration).
- Qt: 6.10 release 2025-10-07 (flexbox, accessibility, Android 15/16,
  macOS 26), 6.11 March 2026 (hardware-accelerated 2D), 6.12 LTS
  September 2026 (CRA compliance, QML hot reload, HarmonyOS), 6.10.3
  April 2026 as the final 6.10 patch.
- Flutter 3.44.0, May 2026 (research snapshot; rottenwifi framework
  comparison, August 2026, for the architecture taxonomy).
- fresco measured numbers: this repository's gates and estate records
  dated 2026-11-24/26 (Gate A soak, S7/Pixel/A53 sweeps, video-kit
  stress bench).

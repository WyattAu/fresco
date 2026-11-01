# Competitive analysis: fresco and the estate vs production UI stacks

Status stamp: DRAFT (numbers drift as upstreams move; re-audit command in
section 8). Audited 2026-11-01.

## 1. Verdict

The fresco core is **algorithm-class correct but at roughly 5 percent of the
reference mass**. The deficit is breadth (windowing, GPU, text shaping,
widgets, layout), not core mathematics, and three pieces of fresco are ahead
of every compared library: exact orientation predicates, general closed-form
springs, and a golden-image conformance gate. The estate's L1 semantics stack
(colour with CVD variants, IME, a11y audit, token governance) has no
equivalent in any compared project.

Quantified (raw production lines, method in section 2):

| Scope | Lines |
|---|---|
| fresco (all 6 crates) | 5,536 |
| estate UI-relevant crates (excl. ldir) | 42,985 |
| fresco + estate excl. ldir | 48,521 |
| fresco + estate incl. ldir (88.4k is document IR, not UI engine) | 136,915 |
| Qt UI slice (gui + widgets + QPA + windows plugin + quick/items + QML) | 953,806 |
| Dear ImGui core | 63,424 |
| egui (17 crates) | 129,241 |
| iced (5 counted crates) | 60,857 |
| Slint (compiler + core + renderers + backends) | 224,093 |

fresco alone is 8.7 percent of ImGui core and 0.6 percent of the Qt slice.
fresco + estate (excl. ldir) is 5.1 percent of the Qt slice and 37.5 percent
of egui. The ldir caveat matters: counting document IR as UI engine mass
would flatter us, so both numbers are given.

## 2. Method and sources

Shallow clones of default branches, 2026-11-01, into /tmp/opencode/libcmp:
qt/qtbase and qt/qtdeclarative (sparse checkout, dev branch), ocornut/imgui,
emilk/egui, iced-rs/iced, slint-ui/slint, linebender/xilem (includes
Masonry), linebender/parley (includes Fontique), linebender/vello,
RazrFalcon/tiny-skia, nical/lyon, rust-windowing/winit, AccessKit/accesskit,
RazrFalcon/rustybuzz. Counting rule: raw `wc -l` over production files only
(tests, benches, examples, fuzz, vendored 3rd-party excluded; ImGui count
includes its demo file). Qt is C++/headers; everything else is Rust.

Caveats, each with a state marker:

- Qt dev branch is mid-refactor for the next major: QEasingCurve.cpp is gone
  from corelib/animation (animation trimmed into src/gui/animation), and the
  classic scene-graph files (qsgbatchrenderer.cpp, 6.x lineage ~3k lines)
  are not present under the fetched paths. Qt 6.x numbers are cited from
  lineage where the dev tree moved (asserted, not measured here).
- HarfBuzz is consumed as a system dependency by Qt and is not counted
  (~300k lines C, asserted from its upstream, not measured here).
- FreeType bundling in qtbase was not fetched by the sparse checkout
  (unmeasured here).
- kurbo and resvg were not cloned; their scale (kurbo ~15k, resvg ~30k) is
  asserted from knowledge, not measured here.
- taffy (iced's layout engine) is an external dependency of iced, not
  counted in iced's numbers (~25k, asserted).

## 3. Code mass, reference libraries

| Project | Measured scope | Lines |
|---|---|---|
| Qt qtbase | src/gui C++ (painting, text, rhi, accessible, image, kernel) | 335,015 |
| Qt qtbase | src/gui headers | 101,412 |
| Qt qtbase | src/widgets C++ | 260,929 |
| Qt qtbase | src/gui/painting C++ (63 files) | 88,012 |
| Qt qtbase | src/gui/text C++ | 71,170 |
| Qt qtbase | src/gui/rhi C++ (GPU abstraction) | 52,007 |
| Qt qtbase | src/gui/image C++ | 28,311 |
| Qt qtbase | src/gui/accessible C++ | 8,999 |
| Qt qtbase | QPA platform layer C++ | 8,102 |
| Qt qtbase | windows platform plugin C++ (54 files) | 27,138 |
| Qt qtbase | src/corelib/kernel event loop (qeventloop + qeventdispatcher_win) | 1,409 |
| Qt qtbase | animation (qvariantanimation + qpropertyanimation, dev branch) | 1,019 |
| Qt qtdeclarative | src/quick/items C++ (item classes incl. render nodes) | 100,473 |
| Qt qtdeclarative | src/qml C++ (QML/JS engine) | 121,137 |
| Dear ImGui | imgui.cpp + draw + widgets + tables + demo + headers | 63,424 |
| egui | 17 crates total | 129,241 |
| egui | egui core | 59,631 |
| egui | epaint (tessellation) | 18,935 |
| iced | core | 16,410 |
| iced | widget library | 30,798 |
| iced | wgpu renderer | 8,193 |
| iced | tiny_skia renderer + runtime | 5,456 |
| Slint | compiler | 87,657 |
| Slint | core | 72,082 |
| Slint | backends + renderers (skia renderer 6,888) | 64,354 |
| Xilem | runtime | 2,171 |
| Masonry | widget tree + widgets | 25,541 |
| Parley | text layout | 11,759 |
| Fontique | font collection/fallback | 6,111 |
| Vello | vello_common (CPU-side encoding/sparse strips) | 16,039 |
| Vello | vello_gpu (compute pipelines) | 19,698 |
| tiny-skia | CPU raster | 15,469 |
| lyon | geom + path + tessellation + algorithms | 57,652 |
| winit | 11 per-platform crates + core + common + dpi | 71,575 |
| winit | win32 crate alone | 12,870 |
| AccessKit | platform-agnostic a11y | 36,988 |
| rustybuzz | OpenType shaping | 29,232 |

Winit's restructure into per-platform crates (winit-win32, winit-wayland,
winit-appkit, winit-x11 as separate crates over winit-core) independently
validates ADR-0003's facade shape.

## 4. Estate code mass

| Crate | Lines | Role |
|---|---|---|
| fresco path-kit | 2,043 | geometry, flattening, stroking, dashing, exact predicates |
| fresco scene-kit | 1,144 | arena scene graph, damage, frame plans, draw lists |
| fresco paint-kit | 1,075 | exact-coverage scanline rasterizer |
| fresco render-conformance | 411 | golden-image differential gate |
| fresco layout-kit | 486 | measure cache, single-axis flex |
| fresco anim-kit | 377 | closed-form springs, easing |
| font-model | 8,796 | font parsing, outlines, cmap, metrics |
| font-shape | 6,728 | glyph rasterization, exact-coverage scanline |
| plychart | 14,268 | charting family |
| ui-text-edit | 1,964 | buffer/caret/selection semantics |
| media-kit | 2,881 | image pipeline |
| input-kit | 1,397 | IME preedit FSM, caret mapping |
| color-kit | 851 | linear sRGB, Oklab, gamut mapping, WCAG |
| theme-kit | 766 | DTCG tokens, WCAG gate |
| a11y-kit | 520 | platform-free a11y node model + audit |
| simd-tokenizer, slab-pool, shm-rings | 3,740 | perf infrastructure |

## 5. Subsystem gap matrix

Verdict scale: Not started / Prototype / Core-correct (right algorithm
class, thin breadth) / Production-track.

| Subsystem | fresco/estate today | Reference evidence (files, measured lines) | Verdict |
|---|---|---|---|
| Windowing | nothing (ADR-0003 facade only) | winit 71,575 across 11 platform crates (win32 12,870); Qt windows plugin 27,138, QPA 8,102 | Not started |
| Event loop / dispatch | nothing UI-grade (eventbus is service pub/sub, wrong shape) | Qt qeventloop 468 + per-platform dispatchers (~1k each); winit-common 3,178 | Not started |
| GPU renderer | Vello wrapper inside ldir only | Vello 35,737 (common 16,039 + gpu 19,698); Qt RHI 52,007 | Not started |
| CPU 2D raster | paint-kit 1,075 | tiny-skia 15,469; Qt painting 88,012 (qpaintengine_raster.cpp 4,899, qrasterizer.cpp 1,216, qgrayraster.c 1,973, qstroker.cpp 1,254) | Core-correct: same scanline-coverage algorithm class, missing gradients, clip stack, image blits, effects |
| Geometry / paths | path-kit 2,043 | lyon 57,652 (tessellation 33,934); kurbo ~15k (asserted) | Core-correct subset; exact predicates are unique (verified, section 6) |
| Text shaping | zero owned, ADR-0002 consumes | rustybuzz 29,232; Parley 11,759 + Fontique 6,111; Qt gui/text 71,170 + system HarfBuzz | Behind by choice; closure is glue, not engine work |
| Paragraph / editing | ui-text-edit 1,964 + input-kit 1,397 | Qt qtextengine.cpp 4,189; Parley layout | Semantics present, line layout absent |
| Layout | layout-kit 486 (single axis) | taffy ~25k (asserted, iced dep); Qt qgridlayout.cpp 1,640 + qboxlayout.cpp 1,367 + qlayoutengine.cpp 403 | Prototype |
| Widgets | zero (deliberate) | Qt widgets 260,929; egui core 59,631; iced widget 30,798; Masonry 25,541 | Not started |
| Animation | anim-kit 377 | Qt qvariantanimation.cpp 688 + qpropertyanimation.cpp 331 (6.x adds QEasingCurve ~2k, asserted); egui scroll_physics.rs has exact critically-damped spring step | Math at or ahead of peers; timeline/compositor integration absent |
| Accessibility | a11y-kit 520 (semantics + audit) | AccessKit 36,988; Qt qaccessible.cpp 3,453 + platform bridges | Semantics ahead, platform bridges absent (consume AccessKit per ADR-0003) |
| Golden-image CI | render-conformance 411 | Qt lancelot (test-suite only); none found in egui, iced, Slint, Xilem sources (grep verified) | Ahead of all Rust peers |
| Damage / partial redraw | scene-kit damage algebra (tested, headless) | iced damage.rs 78 + tiny_skia compositor.rs 238 (shipping) | Parity with iced in feature; fresco's is more formalized but unused by any app yet |
| SVG / icons | nothing | resvg ~30k (asserted) | Not started |
| Declarative UI compiler | nothing, not on roadmap | Slint compiler 87,657; Qt QML engine 121,137 | Far future; these numbers are the caution against hand-rolling |

## 6. Where the estate is ahead (verified)

Each claim was checked by grep against the cloned sources on 2026-11-01:

1. **Exact orientation predicates.** No occurrence of two_sum, Shewchuk, or
   orient2d-style robust predicates in tiny-skia, lyon, Masonry, epaint,
   iced core, Parley, or Slint core. path-kit's adaptive filter + FMA
   exact-product fallback (pred.rs) is unique in this set, including against
   kurbo (asserted; kurbo not cloned).
2. **General closed-form springs.** egui's scroll_physics.rs implements an
   exact critically-damped step (verified: critically_damped_spring_step,
   exponential decay). anim-kit covers all three damping regimes with
   settle-time bounds; neither Qt (numeric QVariantAnimation), iced (no
   spring type in core, grep verified), nor Slint owns an equivalent.
3. **Golden-image conformance as a first-class gate.** No golden-image
   harness found in egui, iced, Slint, or Xilem sources. Qt's equivalent
   (lancelot) is a test suite, not a reusable gate. render-conformance
   exists before the GPU backend it will police, which is the correct order.
4. **Estate L1 semantics with no analogue.** CVD-derived theme variants with
   build-failing WCAG gates (theme-kit + color-kit), IME preedit FSM
   (input-kit), and a headless a11y tree audit (a11y-kit) have no equivalent
   in-tree in any compared project. AccessKit is the nearest neighbour and
   it stops at platform transport.

## 7. Where the estate is behind, and what closure costs

Ordered by blocking power, with closure budgets (estimated, own-code lines;
consumed upstream lines in parentheses):

1. **Windowing + event loop** (blocks every real app). Closure: window-kit
   facade over winit, 1,000 to 2,000 own lines (winit's 71,575 stay
   out-of-house). Qt parity would be 27,138 lines of Windows plugin alone;
   the facade decision means we never pay it.
2. **Text stack** (blocks readable UI). Closure: TextEngine trait + Parley
   and rustybuzz glue, 1,000 to 2,000 own lines (47k upstream stays
   out-of-house). Estate glyph raster (font-shape 6,728) and font parsing
   (font-model 8,796) already exceed Parley's font layer (Fontique 6,111) in
   mass, though not yet in feature breadth.
3. **Raster breadth** (blocks non-toy visuals). Gradients, clip stack,
   image blitting, raster-composited strokes: 4,000 to 6,000 own lines to
   reach tiny-skia's practical core (its 15,469 includes PNG codec and
   path plumbing we already own elsewhere).
4. **Layout** (blocks real screens). Consume taffy behind a LayoutEngine
   trait (~1,000 own lines), keep single-axis flex for the no_std path.
5. **GPU backend** (blocks scale). PaintTarget trait + Vello glue, 1,000 to
   2,000 own lines (Vello's 35,737 stay out-of-house), gated by
   render-conformance parity per ADR-0004.
6. **Widgets** (the true mountain). A minimal production set is 8,000 to
   15,000 own lines by iced/Masonry comparison, and Qt-class breadth is
   260,929 lines, which is precisely why widget-kit stays out of Phase 1
   and why the estate semantics layer, not widgets, is the differentiator.

Total to "engine runs a real kestrel window": roughly 8,000 to 13,000 own
lines added to fresco, or 2.5x current size, with ~250k lines of upstream
consumed rather than owned. Total to widget-kit with a credible control set:
another 8,000 to 15,000. These are quarter-scale efforts, not week-scale.

## 8. Re-audit

Rerun the clone + wc pipeline (section 2 list) and diff section 3 and
section 4 tables. The paradigm registry (docs/paradigms.md) and the SPEC
REQ oracles are the correctness half of this ledger; this file is only the
mass half.

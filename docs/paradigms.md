# Paradigm index (651 to 1050)

Curated registry of the supplied paradigm sets, mapped to fresco crates and
phases. Each row cites the source item number. Items marked "applied" are in
0.1.0 code; everything else is a design hook, not a claim. The full source
lists live with the estate owner; this file records what fresco does with
them. Rule: no row may stay "applied" if the code stops doing it.

## 1. Applied in 0.1.0

| Item | Paradigm | Where it lives |
|---|---|---|
| 880 | Exact analytical damped-oscillator springs (no integration) | `anim-kit` `Spring::sample` |
| 883 | Analytic bezier arc length via Legendre-Gauss quadrature | `path-kit` `CubicBez::arclen`, quad nodes/weights |
| 928 | Sub-pixel analytic coverage anti-aliasing | `paint-kit` winding-integral scanline |
| 831 | Exact predicates with adaptive precision | `path-kit` `pred::orient2d` (filter + FMA exact-product fallback) |
| 871 | Incremental layout with memoized measure caches, dirty-bit DAG | `layout-kit` `MeasureCache` |
| 930 | Draw-call batching via intrusive linear key arrays | `scene-kit` `SortKey` (layer:depth:seq packed u64) |
| 924 | Allocation-free frame path (ephemeral buffers) | design invariant 3; caches caller-owned, `DrawList` buffer reuse |
| 921 | Software rendering fallback path | `paint-kit` CPU backend is the reference path; headless by construction |
| 879 | Virtualized scroll via O(1) interval overlap | reserved: `layout-kit` viewport query API slot |
| 852 | DirectComposition virtual-surface tiles = damage regions | `scene-kit` `FramePlan::Regions` is the tile source of truth on Windows |
| 858 | Per-monitor-v2 DPI as continuous affine transform | `scene-kit` root `Affine`; logical coords are scene-invariant |
| 876 | Focus as affine/linear-token state | `widget-kit` wrapping focus chain, single owner (applied 0.3.0) |
| 875 | Type-safe FSM navigation routers | `widget-kit` activation model (actions as typed ids); full FSM routers reserved for widget-kit 0.4 |
| 874 | Retained reactive signal graphs | Gate D: retained-props spike shipped 0.3.0; signals deferred pending kestrel ergonomics evidence |
| 887 | Glyph atlas shelf packing with LRU | `text-kit` `GlyphAtlas` (applied 0.2.0) |
| 956/958/990 | Fractional/per-monitor DPI as continuous scale | `window-kit` scale facade + root affine plan (applied at facade level) |
| 959/992/1017/1032 | Platform frame-clock pacing | `window-kit` `FrameClock`/`Ticker` measurement contract; native clocks are backend upgrades |

## 2. Next phases, by crate

### path-kit / paint-kit

| Item | Paradigm | Hook |
|---|---|---|
| 883 (full) | Exact stroke offsetting on curves | replace polyline stroking with curve-space offsets |
| 837 | Seidel trapezoid decomposition | if tessellation for GPU backend is needed |
| 833 | Plane-sweep curve intersection | path boolean ops phase |
| 925 | YUV to RGB in compute shaders | media integration, GPU backend phase |
| 926 | SDF blur kernels, single pass | effects layer (shadows, glows) |
| 928 (GPU) | Analytic polygon AA in pixel shaders | GPU backend parity gate |
| 929 | Zero-copy native texture sharing (NT handles) | platform compositing phase |
| 885 | MSDF text | text rendering phase, GPU |
| 886 | Sub-pixel fractional glyph positioning | text rendering phase |
| 887 | Glyph atlas shelf packing with LRU | text rendering phase |
| 888 | COLR/CPAL color glyph compositing | font-model extension |
| 890 | Knuth-Plass paragraph optimization | text layout phase |
| 889 | Bidi UAX #9 | text layout phase |

### scene-kit / layout-kit

| Item | Paradigm | Hook |
|---|---|---|
| 874 | Retained reactive signal graphs | widget-kit decision, not scene-kit |
| 872 | ECS-driven UI | rejected for scene-kit (ADR-0001), revisit for widget internals only |
| 873 | Immediate-mode scratch allocators | rejected as primary model (ADR-0001); escape hatch API if needed |
| 875 | Type-safe FSM navigation routers | widget-kit navigation |
| 877 | Cassowary constraint solver | layout-kit v0.2 if flex+grid proves insufficient |
| 878 | Zero-copy Fluent bundles | i18n-kit integration |
| 949 | Live UI tree introspection over IPC | devtools phase |
| 930 (GPU) | instanced batch submission | GPU backend |

### GPU backend (ADR-0004)

| Item | Paradigm | Hook |
|---|---|---|
| 884 | Vello two-pass compute vector rasterization | first GPU backend |
| 864 | Content-addressed wgpu pipeline cache | backend bootstrap |
| 999 | Compute-driven vector raster (Metal variant) | macOS backend path |
| 995/997 | Indirect command buffers, argument buffers | draw-loop design, Metal |
| 1044/1049 | TBDR framebuffer fetch, load/store actions | Apple/mobile backend |
| 1047 | Dynamic threadgroup sizing | Apple compute |
| 1050 | On-chip single-pass blur | effects on Apple/mobile |
| 1041/992/998 | ProMotion/CADisplayLink/CVDisplayLink pacing | frame clock per platform |
| 1024/1011/1015 | setFrameRate, Swappy, performance hints | Android backend |
| 1046 | ASTC texture streaming | texture phase |

### window-kit (ADR-0003): Windows matrix (851 to 860, 911 to 920, 931 to 950, 901 to 910)

| Item | Hook |
|---|---|
| 851 | IOCP message pump inversion: alternative pump model, note only (tokio-free UI core) |
| 853/912/911 | Composition islands, WM_NCHITTEST titlebars, DwmExtendFrame: custom-frame checklist |
| 855 | Flip-model waitable swapchain: default present mode |
| 857 | Mica/Acrylic via DWMWA_SYSTEMBACKDROP_TYPE: material API |
| 859/860 | Raw input, DirectManipulation: input pipeline |
| 854/935 | Type-state subclassing, HWND Send-not-Sync wrappers: safety model |
| 901/902 | UIA provider, WM_GETOBJECT: a11y-kit bridge to AccessKit then UIA |
| 906/904/905 | IDropTarget, tray, taskbar: shell integration |
| 908/909 | HDR metadata (DXGI_OUTPUT_DESC1), theme registry hooks: color-kit + theme-kit consumers |
| 913/915 | Modal-loop pumping, power resume device-loss recovery |
| 917/918/919 | ReadDirectoryChangesW rings, restart/recovery, clipboard retry |
| 931/932/934/920 | AppContainer, CFG, ACG, mitigation policies: hardening checklist |
| 936/937 | DPAPI zeroizing, secure desktop: credentials phase |
| 941/942/947 | ETW markers, WPA, PresentMon pacing: measurement for damage-redraw claims |
| 943/944/948 | PIX markers, minidumps, DbgHelp: diagnostics |
| 945/946 | Segment heap, PDH counters: profiling |
| 950 | Headless virtual desktop golden images: render-conformance CI mode |

### window-kit: Linux matrix (951 to 975)

| Item | Hook |
|---|---|
| 951/957 | Atomic modesetting, GBM: kiosk/embedded path |
| 952/953 | dma-buf, explicit sync (linux-drm-syncobj-v1): zero-copy present |
| 956/955/958 | fractional-scale-v1, viewporter, CSD/SSD: Wayland first-class |
| 959 | wl_surface.frame + wp_presentation_feedback: the frame clock |
| 961/962 | single-pixel buffer, tearing control: fast paths |
| 960/963 | wl_region input shapes, data-control clipboard |
| 966/967/973 | libinput, evdev slot protocol, mmap input rings: input pipeline |
| 970 | AT-SPI2: a11y-kit Linux bridge |
| 972 | xx-color-management-v4: color-kit consumer |
| 969/968 | SCHED_DEADLINE, PipeWire SPA: audio phase (estate media kits) |
| 975 | Secret Service: credentials phase |

### window-kit: macOS/iOS matrix (976 to 1000, 1026 to 1050)

| Item | Hook |
|---|---|
| 976/977/979 | objc_msgSend pinning, bare NSWindow, CFRunLoop sources: native backend |
| 980/990/988 | full-content titlebars, backingScaleFactor, distributed theme notifications |
| 984/989 | NSAccessibility (a11y-kit bridge), secure event input |
| 991/993/994 | CAMetalLayer, unified memory zero-copy, runtime MSL: Metal backend |
| 996/1048 | Display P3, EDR headroom: color-kit HDR consumer |
| 1001/1002/1003/1004 | android_main, ANativeWindow, AHardwareBuffer, ASurfaceControl: Android path |
| 1013/1031 | cutout insets, safe area insets: inset model in layout-kit |
| 1017/1032 | Choreographer, CADisplayLink: frame clocks |
| 1019/1029/1030 | touch prediction, coalesced touches: input pipeline |
| 1028 | Dynamic Type: theme-kit text scaling consumer |
| 1037/1020/986 | Secure Enclave, Keystore attestation, sandbox check: credentials phase |
| 1045 | os_proc_available_memory: mobile cache policy |

## 3. Out of scope (recorded, not adopted)

Clusters 651 to 750 (differentiable rendering, hardware silicon, CFD,
robotics, geometric deep learning, acoustics, geomechanics, optics,
biomechanics, molecular dynamics) and 751 to 850 (relativistic graphics,
fabrication, SLAM, spectral geometry, DEC, non-Newtonian fluids,
metamaterials, audio DSP physics, computational geometry beyond 831/837/833,
computational photography) are research directions outside the UI engine's
charter. Exceptions already adopted are item 831 and future 833/837. They
remain in the estate index for the kits that do own those domains
(dsp-spectral, media-kit, latent-oracle, CivitForge).

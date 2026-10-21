# Kiesel Desktop – plan

A native Rust retained-mode port of [Kiesel UI](kiesel/) (Soft UI toolkit, SolidJS/CSS).
Goal: far less memory than a WebView. No browser, no JS, no Tauri.

- **Targets:** Linux/Wayland first, then Windows and macOS.
- **License:** MIT OR Apache-2.0. No GPL, LGPL or MPL in the dependency tree ([deny.toml](deny.toml)).
- **Look:** Kiesel's own Soft UI (no native widget emulation).
- **Feel:** native text rendering, scrolling, shortcuts, IME, accessibility, system dialogs.

## Stack (decided 2026-10-21)

| Concern | Choice | Why |
|---|---|---|
| Widget tree | **Own retained tree** | Floem has no AccessKit, git-only deps (forked winit), MPL via `imbl`, one maintainer. Masonry stalled since 2026-06 and pins old vello/parley/wgpu. Both are studied, not depended on. Slint (GPL/commercial): study only, never link. |
| Windowing | winit 0.31 (beta) | Starting now is cheaper than migrating from 0.30 later; kept behind a thin shell layer |
| Rendering | **vello_cpu + softbuffer first**; GPU (vello_hybrid or own wgpu SDF pipeline) later | Lowest memory, deterministic pixels for screenshot tests; `fill_blurred_rounded_rect` (with `invert` for inset shadows) covers Soft UI's raised and pressed looks |
| Text | parley / fontique (HarfRust, skrifa) | Active, system font fallback, editing/IME support; API still churns (0.12 split the crate) – wrap it |
| Layout | taffy | flex/grid like the CSS original |
| Accessibility | AccessKit (AT-SPI over zbus on Linux) | pure Rust |
| Reactivity | reactive_graph (Leptos) | fine-grained signals, the SolidJS model |
| Dialogs / clipboard / menus | rfd (`xdg-portal`, no GTK) · arboard or smithay-clipboard · muda on macOS/Windows only | muda needs GTK on Linux; Linux menus are drawn by us |

## Measured baselines

| What | Memory |
|---|---|
| vello_cpu Soft UI scene, 1280×800 ([tools/memprobe](tools/memprobe)) | 15 MiB RSS |
| same, 2560×1600 | 27 MiB RSS |
| Kiesel kitchensink in WebKitGTK ([tools/webkit-baseline.py](tools/webkit-baseline.py), software GL) | 604 MiB PSS (3 processes) |
| Button demo, windowed, loading (spinner running), 640×400 @ 2× = 1280×800 px | 12.5 MiB PSS, 19.7 MiB RSS |
| same at 1× | 9.2 MiB PSS, 13.4 MiB RSS |

Estimates (not measured yet, needs real hardware): GPU path with wgpu on Mesa ~50–90 MB, on NVIDIA ~120–200 MB.
Full CPU redraws took ~55 ms in the unoptimised probe (120 blurred cards) → damage regions and cached shadows are required for dense screens.
The button demo redraws the full 1280×800 px window in ~1.4 ms (release, median).

## Testing headless

The dev container runs a private headless sway (pixman), Mesa lavapipe/llvmpipe, and a D-Bus + AT-SPI session:

```sh
kiesel-headless start --size 1280x800 --scale 2
kiesel-headless run cargo run --example button
kiesel-headless shot out.png          # grim
kiesel-headless move 640 400          # logical coordinates (tools/kiesel-input)
kiesel-headless click 640 400         # also: down, up, scroll <dy>
kiesel-headless key -k Return         # wtype (see open points)
kiesel-headless a11y                  # AT-SPI tree
kiesel-headless stop
```

`tests/button-demo.sh` runs the button demo through hover, press, loading and back – light and dark, scale 1 and 2.

Open points: `wtype` (like `wlrctl`) creates its keyboard per call, so keys will likely race the same way the
pointer did – extend `kiesel-input` with a virtual keyboard when keyboard handling lands. No IME and no portal
(file dialogs) in the headless session yet.

## Milestones

1. ✅ **Button demo** (`cargo run --example button`) – a window, one Kiesel button centred horizontally and
   vertically, sharp at any output scale, light/dark from the system (`KIESEL_THEME` overrides); hover and press
   transitions as in CSS (180 ms, ease-3); a click toggles `loading` (spinner, progress cursor).
2. Event routing, focus, keyboard activation, AccessKit node for the button.
3. Text and layout through parley + taffy; damage tracking.
4. Port components following kiesel's ROADMAP order.

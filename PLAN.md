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

Estimates (not measured yet, needs real hardware): GPU path with wgpu on Mesa ~50–90 MB, on NVIDIA ~120–200 MB.
Full CPU redraws took ~55 ms in the unoptimised probe → damage regions and cached shadows are required.

## Testing headless

The dev container runs a private headless sway (pixman), Mesa lavapipe/llvmpipe, and a D-Bus + AT-SPI session:

```sh
kiesel-headless start --size 1280x800 --scale 2
kiesel-headless run cargo run --example button
kiesel-headless shot out.png          # grim
kiesel-headless click 640 400         # logical coordinates (wlrctl)
kiesel-headless key -k Return         # wtype
kiesel-headless a11y                  # AT-SPI tree
kiesel-headless stop
```

## Milestones

1. **Button demo** – a window, one Kiesel button centred, scaling with the window and the output scale; press toggles `loading` (spinner).
2. Event routing, focus, keyboard activation, AccessKit node for the button.
3. Text and layout through parley + taffy; damage tracking.
4. Port components following kiesel's ROADMAP order.

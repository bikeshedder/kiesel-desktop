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
| Accessibility | AccessKit (AT-SPI over zbus on Linux) | pure Rust; zbus 5 until AccessKit moves to 6 (see Dependency watch) |
| Reactivity | reactive_graph (Leptos) | fine-grained signals, the SolidJS model |
| Dialogs / clipboard / menus | rfd (`xdg-portal`, no GTK) · arboard or smithay-clipboard · muda on macOS/Windows only | muda needs GTK on Linux; Linux menus are drawn by us |

## Dependency watch (2026-10-21)

`cargo deny` flags crates whose authors stopped maintaining them. Where a maintained alternative exists we
switch; otherwise the advisory is ignored in `deny.toml` with its reason, and listed here to revisit.

| Crate | Via | Status | What to do |
|---|---|---|---|
| ttf-parser | winit → sctk-adwaita → ab_glyph | **Replaced**: winit's `wayland-csd-adwaita-skrifa` draws the title with skrifa, which parley already uses (same version, no duplicate) | – |
| zvariant, zvariant_derive, zbus_names | accesskit_unix → atspi; our portal code | Merged into zbus 6.0 (2026-10-16), not abandoned. atspi 0.30/accesskit_unix 0.24 are on zbus 5; AccessKit waits because of zbus 6's MSRV (1.89), and ignores the same advisories (PR #834) | Move our `zbus` to 6 together with accesskit_unix – earlier would put two zbus versions in the tree |
| paste | reactive_graph (Leptos) | A compile-time macro in two places (`callback.rs`); nothing at runtime. Maintained fork: pastey. Leptos main and 0.3.0-rc0 still use paste | Offer Leptos a PR for pastey. If reactive_graph ever stalls: **roll our own reactive core** – we use a small part (`ArcSignal`, `ArcRwSignal`, `derive`, `ImmediateEffect`), behind `reactive.rs` and the `lib.rs` exports |

## Measured baselines

| What | Memory |
|---|---|
| vello_cpu Soft UI scene, 1280×800 ([tools/memprobe](tools/memprobe)) | 15 MiB RSS |
| same, 2560×1600 | 27 MiB RSS |
| Kiesel kitchensink in WebKitGTK ([tools/webkit-baseline.py](tools/webkit-baseline.py), software GL) | 604 MiB PSS (3 processes) |
| Button demo, windowed, loading (spinner running), 640×400 @ 2× = 1280×800 px | 12.5 MiB PSS, 19.7 MiB RSS |
| same at 1× | 9.2 MiB PSS, 13.4 MiB RSS |
| Two-button demo with AccessKit active (AT-SPI listening), loading, 1280×800 px | 16.0 MiB PSS, 23.4 MiB RSS, 4 threads |
| Dialog demo (card, wrapped text, two buttons), AccessKit active, loading, 1280×840 px | 19.3 MiB PSS, 25.2 MiB peak RSS |
| Form demo (text field in use, CJK text, clipboard thread), AccessKit active, 1040×720 px | 19.6 MiB PSS, 24.9 MiB peak RSS, 5 threads |

A change of scheme cross-fades the whole window for 300 ms (about 18 full frames), then nothing.

Full-window frames (release, measured headless; `KIESEL_STATS` breaks a frame into wait, paint, raster, copy,
present): 2560×1600 ~4 ms, 3840×2160 ~13 ms – scene 0.6 ms, rasterizing ~9 ms, conversion into the window's
pixel format ~3 ms. That fits 60 Hz, not 120 Hz: two passes over every pixel are the floor of a CPU renderer,
and vello_cpu's worker threads saved ~25 % for 8–16 extra threads.
Clips are non-isolated (`push_clip_rect`/`push_clip_path`): an isolated clip layer around a scroll viewport cost
half of a 4K scroll frame (12.5 → 7.9 ms with the change). 120 Hz is for the GPU renderer
([ROADMAP.md](ROADMAP.md) D5). Debug builds optimize dependencies (opt-level 3) and kiesel-ui (opt-level 1),
or a full frame would take 170–360 ms.

Estimates (not measured yet, needs real hardware): GPU path with wgpu on Mesa ~50–90 MB, on NVIDIA ~120–200 MB.
Full CPU redraws took ~55 ms in the unoptimised probe (120 blurred cards) → damage regions and cached shadows are required for dense screens.
The button demo redraws the full 1280×800 px window in ~1.4 ms (release, median). With damage tracking, the
dialog's spinner frames redraw ~5 % of the window in ~1.2 ms; full frames at 1280×840 px take 4–6 ms.

## Testing headless

The dev container runs a private headless sway (pixman), Mesa lavapipe/llvmpipe, and a D-Bus + AT-SPI session:

```sh
kiesel-headless start --size 1280x800 --scale 2
kiesel-headless run cargo run --example button
kiesel-headless shot out.png          # grim
kiesel-headless move 640 400          # logical coordinates (tools/kiesel-input)
kiesel-headless click 640 400         # also: down, up; wheel <notches>, swipe <dy> (touchpad)
kiesel-headless key shift+Tab         # also: keydown, keyup, type <text> (us layout)
kiesel-headless ime preedit ありが     # act as the input method: also commit, clear, delete <b> <a>
kiesel-headless a11y                  # AT-SPI tree
kiesel-headless a11y click "Save changes"   # also: focus, where (centre), box (x y w h) <name>
kiesel-headless a11y set --role entry Name = Text   # also: value, caret [= offset]
kiesel-headless portal dark           # the system's colour scheme, from a fake XDG portal; also
                                      # accent R G B, contrast high, motion reduce, text-scale 1.5
kiesel-headless stop
```

`tests/button-demo.sh` drives the demo by pointer, keyboard and assistive technology – light and dark, scale 1
and 2 – and finds the buttons through the accessibility tree, not by coordinates. `tests/dialog-demo.sh` checks
wrapping on resize, that animation frames redraw only a small region, and that partial redraws match full ones
(`KIESEL_FULL_REDRAW=1`) within ±2 per channel – the same pixel can round one step apart at another origin.

`tests/i18n-demo.sh` detects the language, formats plurals, lists and numbers, and switches at runtime;
`tests/popover-demo.sh` opens, places and dismisses popovers and follows focus in and out;
`tests/list-demo.sh` scrolls by wheel, touchpad (momentum), keys and scrollbar, and checks AT-SPI scrollTo;
`tests/todo-demo.sh` adds and removes widgets at runtime and watches memory.
`tests/theme-demo.sh` follows a (fake) system preference, chooses, saves and restores the theme.
`tests/settings-demo.sh` follows the portal's text scale, accent colour, contrast and reduced motion live.
`tests/form-demo.sh` covers text input: keyboard editing, pointer selection, the Wayland clipboard both ways,
the input method (on only while a field is focused), AT-SPI Text/EditableText, and the caret's frame cost.

`wlrctl`/`wtype` create a device per call, which clients never get to bind; `tools/kiesel-input` keeps one virtual
pointer and keyboard for the whole session, and registers as the seat's input method (input-method-v2). Open: no
portal (file dialogs) in the headless session yet.

## Milestones

Done and planned work is in [ROADMAP.md](ROADMAP.md) – Kiesel Desktop's own order, not Kiesel Web's.

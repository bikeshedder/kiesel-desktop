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
| Two-button demo with AccessKit active (AT-SPI listening), loading, 1280×800 px | 16.0 MiB PSS, 23.4 MiB RSS, 4 threads |
| Dialog demo (card, wrapped text, two buttons), AccessKit active, loading, 1280×840 px | 19.3 MiB PSS, 25.2 MiB peak RSS |
| Form demo (text field in use, CJK text, clipboard thread), AccessKit active, 1040×720 px | 19.6 MiB PSS, 24.9 MiB peak RSS, 5 threads |

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
kiesel-headless click 640 400         # also: down, up, scroll <dy>
kiesel-headless key shift+Tab         # also: keydown, keyup, type <text> (us layout)
kiesel-headless ime preedit ありが     # act as the input method: also commit, clear, delete <b> <a>
kiesel-headless a11y                  # AT-SPI tree
kiesel-headless a11y click "Save changes"   # also: focus, where (centre), box (x y w h) <name>
kiesel-headless a11y set --role entry Name = Text   # also: value, caret [= offset]
kiesel-headless stop
```

`tests/button-demo.sh` drives the demo by pointer, keyboard and assistive technology – light and dark, scale 1
and 2 – and finds the buttons through the accessibility tree, not by coordinates. `tests/dialog-demo.sh` checks
wrapping on resize, that animation frames redraw only a small region, and that partial redraws match full ones
(`KIESEL_FULL_REDRAW=1`) within ±2 per channel – the same pixel can round one step apart at another origin.

`tests/form-demo.sh` covers text input: keyboard editing, pointer selection, the Wayland clipboard both ways,
the input method (on only while a field is focused), AT-SPI Text/EditableText, and the caret's frame cost.

`wlrctl`/`wtype` create a device per call, which clients never get to bind; `tools/kiesel-input` keeps one virtual
pointer and keyboard for the whole session, and registers as the seat's input method (input-method-v2). Open: no
portal (file dialogs) in the headless session yet.

## Milestones

1. ✅ **Button demo** (`cargo run --example button`) – a window, one Kiesel button centred horizontally and
   vertically, sharp at any output scale, light/dark from the system (`KIESEL_THEME` overrides); hover and press
   transitions as in CSS (180 ms, ease-3); a click toggles `loading` (spinner, progress cursor).
2. ✅ **Input, focus, accessibility** – a retained widget tree (`Widget`, `Row`); pointer routing with capture;
   Tab / Shift+Tab focus chain with a `:focus-visible` ring (keyboard and AT focus show it, pointer focus doesn't
   until a key is pressed); Button activation as in browsers (Space on release, Enter on press); AccessKit through
   `accesskit_unix` directly (accesskit_winit pins winit 0.30) – Window and Button nodes with busy, focus, Click
   and Focus actions. The demo's two buttons share a `reactive_graph` signal.
3. ✅ **Layout, text, partial redraws** – taffy lays out a persistent mirror of the widget tree (containers give a
   `Style`, leaves measure; only dirty leaves are measured again); `Flex::stack` / `Flex::cluster` (Kiesel's Stack
   and Cluster), `Card`, and `Label` (parley paragraphs wrapping at min/max-content or the given width, type scale:
   body, title, caption). Widgets report `Change { layout, paint }` from `update()` and their `ink()`; the shell
   redraws only the damaged region (rendered into its own pixmap and copied, or straight into the buffer above
   half the window), merged with the damage since the buffer was last shown (softbuffer's buffer age), and presents
   it with damage. Example: `cargo run --example dialog`.
4. **Reactive state, text input**
   - ✅ a) Signals reach exactly the widgets that show them: widgets `watch` their `ArcSignal` props on mount
     (reactive_graph's immediate effects, no executor); a change on any thread queues the widget and wakes the
     event loop, and only queued widgets and those that got events are updated. `Label::bound` follows a
     signal. The dialog demo saves on a background thread.
   - ✅ b) `TextField` – parley's `PlainEditor` in Kiesel's field well: caret and selection by pointer (click,
     drag, double/triple click, Shift+click) and keyboard (arrows, Home/End, Shift, Ctrl by word, Backspace/Delete,
     Ctrl+A/C/X/V), horizontal scrolling, input method composition (underlined, candidate window at the caret;
     winit's text-input-v3), clipboard (smithay-clipboard on the window's own Wayland connection, arboard
     elsewhere), the value as a two-way signal. For assistive technology it exposes AccessKit text runs (≤ 255
     characters each, from parley's clusters, without uncommitted composition) and the text selection, so screen
     readers get AT-SPI Text and EditableText: read it, move the caret, set the text. The caret blinks GTK-style
     (10 s, then steady) on timers – 2 frames/s, none when idle.
5. Port components following kiesel's ROADMAP order.

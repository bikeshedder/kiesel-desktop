# Kiesel Desktop – Roadmap

Kiesel Desktop's own order of work. Kiesel Web is the **design reference** – how a component looks, which states
it has, how it behaves – not the schedule: its roadmap follows the web product and is still moving. Desktop
apps need other things first, and many widgets need foundations (popups, scrolling, changing children) that
the web gets from the browser and Kiesel Desktop has to build.

The stack, measured baselines and the headless test setup are in [PLAN.md](PLAN.md).

## Rules for every step

- **Done means:** keyboard, pointer and assistive technology (AccessKit, checked through AT-SPI), light and dark,
  scale 1 and 2, a headless test (`tests/*-demo.sh`), unit tests for the logic, `cargo clippy` and `cargo deny`
  clean, memory and frames checked against PLAN.md. One commit per step, or a few.
- **Porting:** look and states from Kiesel Web (tokens, CSS, ARIA). Where the web's behaviour is a browser
  convention, the desktop convention wins: focus, the context-menu key, Ctrl/Cmd, double-click timing, scrolling.
- **Idle costs nothing:** no frames without a change; animations damage only what moves.
- **Frame budget:** the software renderer targets **60 Hz** (16.7 ms per frame, full-window frames included, at
  2560×1600 and 4K). 120 Hz is the GPU renderer's goal (D5), not the software renderer's.
- **Sizes:** **S** an afternoon · **M** a day or two · **L** several days.

## Done

| | What |
|---|---|
| Rendering | vello_cpu into softbuffer, partial redraws with buffer age, Soft UI box shadows (outer, inset, spread) |
| Layout & text | taffy (Flex stack/cluster), parley paragraphs, type scale, CJK and emoji fallback |
| Input | pointer routing and capture, focus chain, `:focus-visible`, IME, clipboard, multi-click, bubbling keys and scrolling |
| State | signals to the widgets that show them (any thread), timers (`wake_at`), `Show` and keyed `For` |
| Accessibility | AccessKit on AT-SPI: roles, states, actions, text runs, caret and selection |
| Theme | light/dark/system (XDG portal), saved per app, cross-fade, Lucide icons |
| Messages | Kiesel's catalogs (English, German), MessageFormat 2 subset, language switch at runtime |
| Widgets | Button (default, loading), Label, Card, TextField, Switch, IconToggle, ThemeSwitch, Scroll, Popover |

## D1 – Foundations (before many more widgets)

Without these, every widget after Field and Checkbox would be built twice.

- [x] **M** **Changing children.** Widgets added and removed at runtime: mount and unmount (watches stop), layout
  nodes and accessibility nodes follow, focus moves on when the focused widget goes. `Show` (when) and `For`
  (keyed lists) as SolidJS has them. Layout-transparent like CSS `display: contents`. Example: `todo`;
  `tests/todo-demo.sh` also checks that filling and emptying a list doesn't grow memory.
- [x] **M** **Scrolling.** `Scroll`: touchpads 1:1 with momentum after the lift (τ 325 ms), wheel steps of
  `viewport^(2/3)` animated, PageUp/Down, arrows, Home/End bubbling from the focused widget, focus and
  AT-SPI scrollTo revealing widgets, Kiesel's overlay scrollbar (thin while scrolling, wide under the pointer,
  draggable, fading out). Scroll frames only re-place (no measuring) and redraw the viewport; at 4K a momentum
  frame takes ~8 ms. Example: `list`.
- [x] **M** **Popups.** An in-window layer above the content, as on Kiesel Web – decided over real popup
  surfaces because softbuffer's Wayland buffers have no alpha (no rounded corners, no shadows). Panels lay
  themselves out at their natural size and are placed next to their anchor: flipped, slid along the edge, below
  when neither side fits; Kiesel's float shadow, optional arrow, the "shift" animation. A click outside or Escape
  dismisses the topmost; a press on the owner only closes it; opened from the keyboard, focus moves in and Tab
  stays inside; focus returns on close. `Popover` is the first user. Real popup surfaces (which may leave the
  window) can follow with the GPU renderer, whose surfaces have alpha. Example: `popover`.
- [x] **S** **Messages.** Kiesel Web's catalogs, embedded as they are (`locales/<lang>/kiesel.json`, keys
  `kiesel.<key>`), plus a small `desktop.json`; apps add their own (`add_catalog`). A MessageFormat 2 subset
  covers every message they use: placeholders, `:number`, `:list`, `.input`, `.match` over several selectors
  with exact keys, plural categories and `*`. The language comes from the environment (`KIESEL_LANG`, `LC_*`,
  `LANG`) and is a signal: `t()`/`t_with()` texts follow it, so switching updates exactly the widgets showing
  text. Plural rules, numbers and lists cover English and German; other languages and dates (ICU4X) when an app
  needs them. Keep the catalogs in sync with Kiesel Web by copying them. Example: `i18n`.
- [x] **S** **System settings.** The portal's appearance beyond the colour scheme, followed live (`SystemPrefs`):
  accent colour (`accent-color` → Kiesel's accent), contrast (`contrast` high → `--ksl-contrast: 2`), reduced
  motion (`reduced-motion` or GNOME's `enable-animations`: no transitions, fades, glides or popup animations;
  the theme changes in a cut) and text scale (GNOME's `text-scaling-factor`: text is shaped again at once, and
  controls grow with it, as on GTK). Tested with `tests/settings-demo.sh`.
- [x] **S** **Shortcuts.** `Shortcut` with platform modifiers ("Primary" is Ctrl, ⌘ on macOS), parsed from
  "Primary+Shift+Z"; matched by the key without modifiers, Shift-made symbols ("Primary++") and, on layouts
  without Latin letters, the US key position (Ctrl+C on a Russian layout). A press goes to the focused widget
  and its ancestors first (a field keeps Ctrl+A), then to widgets that registered it (`Widget::shortcuts`,
  `Button::shortcut`) – the topmost popup's before the window's – then to the app's (`Options::shortcuts`).
  Labels the platform's way and localised ("Ctrl+Shift+S", "Strg+Umschalt+S", "⇧⌘S"); `Kbd` shows them.
  TextField's clipboard keys use the same keymap. Open: macOS word movement (⌥), user-configurable keymaps,
  and AT-SPI key bindings (AccessKit leaves them empty on Linux). Example: `shortcuts`.
- [x] **S** **Icons.** `tools/lucide-icons.py --out src/icons.rs save trash-2 …` takes the icons an app uses
  from lucide-static (pinned version, checksum verified, cached) into `IconNode` consts, with Lucide's ISC notice
  and Feather's MIT one when needed; the toolkit's own are in `kiesel_ui::lucide`. `Icon` (1.25 em, stroke
  1.75, a tone; decorative unless labelled, then an image for assistive technology); Button `.icon()`,
  `.icon_end()`, `.icon_only()` (square, named by its label; the spinner takes the icon's place). Example:
  `icons`.

## D2 – Forms

What most desktop apps need first, settings dialogs above all.

- [x] **M** `Field` – label (with a red `*` when required), the control, then the error (warning icon) or else
  the hint. The control learns it through `Widget::link_field`: labelled-by the label (AT-SPI names it from
  that), described by the message, required, invalid – and draws a red edge and ring. A press on the label
  focuses the control (`Widget::press_focuses`). Open: AT-SPI has no invalid state from AccessKit (Windows and
  macOS get it); the web's error fade-in. Example: `field`.
- [ ] **M** `Checkbox`, `CheckboxGroup`, `Radio`; `Switch` with a visible label and description (settings rows,
  label first).
- [ ] **S** `SegmentedControl` / `ToggleGroup` – e.g. System · Light · Dark next to the ThemeSwitch.
- [ ] **M** `Textarea` – TextField over several lines: line navigation, vertical scrolling (needs D1 scrolling).
- [ ] **M** `NumberField` (steppers, Up/Down) and `PasswordField` (reveal, no copy).
- [ ] **L** `Select` and the combobox (`TextField` with options) on a shared option list – search, groups,
  keyboard, multiple – on D1 popups.
- [ ] **M** `Slider`.
- [ ] **S** Button variants and sizes (primary, soft, outline, ghost; xs–xl; colour) – the theme tokens exist.

## D3 – Application structure

- [ ] **M** `Menu` and context menus (right click, Shift+F10, the menu key; submenus), on popups. A native menu
  bar on macOS later (muda), Kiesel's own elsewhere.
- [ ] **S** `Tooltip` (hover and focus delays, on popups).
- [ ] **M** `Dialog` – modal inside the window (scrim, focus trapped, Escape, focus returns) as on the web; file
  and print dialogs are the system's (XDG portal via rfd, without GTK).
- [ ] **S** `Toast`.
- [ ] **M** `Tabs`.
- [ ] **L** App frame: `AppShell`, `Sidebar`, `ActivityBar`, top bar, `StatusBar`, `SplitView` (resizable panes);
  F6 cycles the regions.
- [ ] **M** Window decorations in Kiesel's style where the compositor leaves them to the app (GNOME): title bar,
  window buttons, moving and resizing – today winit draws Adwaita's.
- [ ] **M** A desktop kitchensink: one example app with every widget, light and dark, and its own visual test
  (screenshots compared like Kiesel Web's sweep).

## D4 – Data and status

- [ ] **L** `List` – virtualized, selection (single, multiple, range), type-ahead.
- [ ] **L** `Table` – columns (sort, resize), virtualized rows, keyboard grid navigation.
- [ ] **M** `Tree`.
- [ ] **S** each: `Progress`, `Meter`, `Spinner` on its own, `Badge`, `Tag`/`Chip`, `Avatar`, `EmptyState`,
  `Skeleton`, `Alert`/`Callout`.

## D5 – Platforms and rendering

- [ ] **L** Windows: AccessKit's Windows adapter (UIA), IME (TSF through winit), clipboard, theme events, DPI;
  CI builds and runs the unit tests.
- [ ] **L** macOS: AccessKit's macOS adapter, menu bar, Cmd shortcuts, theme; CI.
- [ ] **L** GPU rendering (vello_hybrid or an own wgpu SDF pipeline) behind the same painting code – the path
  to **120 Hz** and above, including full-window animations at 4K. Measured on real hardware (an RTX 5090
  machine is available) against PLAN.md's CPU numbers, for frame time and memory; chosen per platform and
  display, with the CPU renderer staying the low-memory default.
- [ ] **M** Packaging an example app: Flatpak, MSI, `.app`.

## Not planned

The web site's own parts – `Hero`, `FeatureList`, `Showcase`, `LivePreview`, `TokenTable`, the docs pages – and
anything that only makes sense in a browser. Components that need a decision before porting (`RichText`,
`Composer`, `CodeBlock` with highlighting, `QuakeConsole`) wait until an app asks for them.

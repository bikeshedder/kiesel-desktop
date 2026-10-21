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
| Input | pointer routing and capture, focus chain, `:focus-visible`, IME, clipboard, multi-click |
| State | signals to the widgets that show them (any thread), timers (`wake_at`), `Show` and keyed `For` |
| Accessibility | AccessKit on AT-SPI: roles, states, actions, text runs, caret and selection |
| Theme | light/dark/system (XDG portal), saved per app, cross-fade, Lucide icons |
| Widgets | Button (default, loading), Label, Card, TextField, Switch, IconToggle, ThemeSwitch |

## D1 – Foundations (before many more widgets)

Without these, every widget after Field and Checkbox would be built twice.

- [x] **M** **Changing children.** Widgets added and removed at runtime: mount and unmount (watches stop), layout
  nodes and accessibility nodes follow, focus moves on when the focused widget goes. `Show` (when) and `For`
  (keyed lists) as SolidJS has them. Layout-transparent like CSS `display: contents`. Example: `todo`;
  `tests/todo-demo.sh` also checks that filling and emptying a list doesn't grow memory.
- [ ] **M** **Scrolling.** A scroll container: pixel deltas from touchpads, wheel steps, momentum (Wayland
  compositors don't provide it, GTK doesn't either – implement it once), Kiesel's overlay scrollbar, PageUp/Down,
  Home/End, focus scrolled into view. Redraw: the viewport is damaged as a whole; measure before optimising.
- [ ] **M** **Popups.** Second surfaces anchored to a widget – winit's popups (xdg_popup on Wayland: they may leave
  the window, the compositor flips and slides them). The shell gets more than one surface: input, focus and
  Escape per popup, dismissal on outside click. Menus, Select, Combobox, Tooltip and DateField sit on this.
- [ ] **S** **Messages.** English and German catalogs like Kiesel Web's (the widgets' own strings: "Light",
  "Dark", "Theme", "Loading"); the language from the system; number and date formatting through ICU4X (already
  in the tree through parley).
- [ ] **S** **System settings.** More of the portal's appearance: accent colour (`accent-color` → Kiesel's accent),
  contrast (→ `--ksl-contrast`), reduced motion (transitions to 1 ms, as on the web); font scale.
- [ ] **S** **Shortcuts.** One keymap with platform modifiers (Ctrl, Cmd on macOS), shortcuts registered by
  widgets and the app, display strings for menus and `Kbd`.
- [ ] **S** **Icons.** A script that takes the icons an app uses from the Lucide package into Rust data, with
  Lucide's licence notice; an `Icon` widget; icons in Button (leading, trailing, icon-only).

## D2 – Forms

What most desktop apps need first, settings dialogs above all.

- [ ] **M** `Field` – label, hint and error around a control; names it for assistive technology (labelled-by
  instead of `TextField::label`), red edge and message when invalid.
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

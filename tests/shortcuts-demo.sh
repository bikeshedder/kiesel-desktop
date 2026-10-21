#!/usr/bin/env bash
# Shortcuts, with examples/shortcuts in a headless session, scale 1 and 2:
#   anywhere  – Ctrl+S presses Save without focus, and while typing in the note
#   focus     – the focused field keeps the keys it uses (Ctrl+A selects its text)
#   popups    – while the "More" popover is open its Ctrl+E (Archive) wins over the
#               window's (Export); closed, Export has it again
#   app       – Ctrl+L, an app shortcut, switches the language; the Kbd labels
#               follow ("Ctrl+S" → "Strg+S")
#   idle      – no frames after it all
# Screenshots and logs land in shots/shortcuts-demo/.
set -euo pipefail
cd "$(dirname "$0")/.."

H=kiesel-headless
OUT=shots/shortcuts-demo
mkdir -p "$OUT"
cargo build -q --release --example shortcuts -p kiesel-ui
BIN=target/release/examples/shortcuts

fail() { echo "FAIL: $*" >&2; exit 1; }
settle() { sleep 0.4; }
has() { $H a11y 2>/dev/null | grep -qF "$1" || fail "$tag: no \"$1\" ($2)"; }
frames() { grep -c '^frame' "$OUT/$tag.log" || true; }

run_case() {
  local scale=$1
  tag="${scale}x"
  pkill -x shortcuts || true
  $H stop >/dev/null 2>&1 || true
  $H start --size 520x460 --scale "$scale" >/dev/null
  env -u KIESEL_LANG -u LC_ALL -u LC_MESSAGES -u LANGUAGE LANG=en_US.UTF-8 KIESEL_STATS=1 \
    $H run "$BIN" >"$OUT/$tag.log" 2>&1 &
  sleep 1.5
  has "label 'Nothing yet'" "at start"
  has "label 'Ctrl+S'" "the Kbd's label"

  # nothing focused
  $H key ctrl+s; settle
  has "label 'Saved (1)'" "Ctrl+S without focus"

  # typing in the field: it keeps Ctrl+A, and Ctrl+S still saves
  $H a11y focus Note
  $H type hello; $H key ctrl+a; $H type X
  [[ "$($H a11y value Note)" == "X" ]] || fail "$tag: Ctrl+A in the field didn't select its text"
  $H key ctrl+s; settle
  has "label 'Saved (2)'" "Ctrl+S while typing"

  # the window's Ctrl+E, then the popover's
  $H key ctrl+e; settle
  has "label 'Exported (3)'" "Ctrl+E in the window"
  $H click $($H a11y where More); settle
  has "dialog 'More'" "the popover opened"
  $H key ctrl+e; settle
  has "label 'Archived (4)'" "the open popover's Ctrl+E comes first"
  $H shot "$OUT/$tag-popover.png"
  $H key Escape; settle
  $H key ctrl+e; settle
  has "label 'Exported (5)'" "closed, the window's Ctrl+E again"

  # the app's own: the language, and the labels with it
  $H key ctrl+l; settle
  has "label 'Exportiert (5)'" "Ctrl+L switched to German"
  has "label 'Strg+S'" "the Kbd's label in German"
  has "button 'Speichern'" "the button in German"
  $H shot "$OUT/$tag-de.png"
  $H key ctrl+l; settle
  has "label 'Ctrl+S'" "and back"

  sleep 0.5; local a b; a=$(frames); sleep 1; b=$(frames)
  (( a == b )) || fail "$tag: $((b - a)) frames while idle"

  pkill -x shortcuts || true
  echo "ok  $tag  (anywhere, in a field, popover first, app, labels in two languages)"
}

trap '$H stop >/dev/null 2>&1 || true; pkill -x shortcuts || true' EXIT
for scale in 1 2; do run_case "$scale"; done

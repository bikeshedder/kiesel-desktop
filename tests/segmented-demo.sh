#!/usr/bin/env bash
# Segmented controls and toggle groups, with examples/segmented in a headless session, scale 1 and 2:
#   a11y      – radio groups named by their field (or their own name), radio buttons and
#               toggle buttons with their states
#   choose    – a click on Dark switches the theme; the knob slides (frames), then rests
#   keyboard  – only the chosen option is a Tab stop; the arrows choose, wrap, move focus;
#               a click on the field's label focuses the chosen option
#   optional  – Density starts with none chosen (a ghost knob on the default); a click
#               chooses, a click on the chosen one clears again
#   toggles   – each option of the toggle group on and off by itself; Space toggles
#   plain     – the view switcher chooses, and the other controls on the same value follow
# Screenshots and logs land in shots/segmented-demo/.
set -euo pipefail
cd "$(dirname "$0")/.."

H=kiesel-headless
OUT=shots/segmented-demo
mkdir -p "$OUT"
cargo build -q --release --example segmented -p kiesel-ui
BIN=target/release/examples/segmented
export XDG_CONFIG_HOME="$PWD/$OUT/config"

fail() { echo "FAIL: $*" >&2; exit 1; }
settle() { sleep 0.5; }  # the knob's spring: 320 ms
line() { $H a11y 2>/dev/null | grep -F "$1" | head -1 | sed 's/^ *//' || true; }
expect() {
  local got; got=$(line "'$1'")
  [[ "$2" == *focused* ]] || got=${got// focused/}
  [[ "$got" == "$2" ]] || fail "$tag: '$got', expected '$2' ($3)"
}
frames() { grep -c '^frame' "$OUT/$tag.log" || true; }
background() {
  $H shot "$OUT/$tag-bg.png"
  python3 -c "
import sys
from PIL import Image
r, g, b = Image.open(sys.argv[1]).convert('RGB').getpixel((3, 3))
print('dark' if r + g + b < 3 * 128 else 'light')" "$OUT/$tag-bg.png"
}

run_case() {
  local scale=$1
  tag="${scale}x"
  rm -rf "$XDG_CONFIG_HOME"
  pkill -x segmented || true
  $H stop >/dev/null 2>&1 || true
  $H start --size 640x660 --scale "$scale" >/dev/null
  $H portal light
  KIESEL_STATS=1 $H run "$BIN" >"$OUT/$tag.log" 2>&1 &
  sleep 1.5

  $H a11y 2>/dev/null | grep -q "panel 'Theme'" || fail "$tag: the segmented control isn't named by its field"
  $H a11y 2>/dev/null | grep -q "panel 'View'" || fail "$tag: the plain one has no name"
  expect System "radio button 'System' checked" "the mode at start"
  expect Bold "toggle button 'Bold' pressed" "the toggle group at start"
  $H a11y 2>/dev/null | grep -A3 "panel 'Density'" | grep -q checked && fail "$tag: Density has a choice at start"

  # choose with the pointer: the knob slides, then nothing moves
  local a b c
  a=$(frames)
  $H click $($H a11y where --role "radio button" Dark); sleep 0.25
  b=$(frames); settle; sleep 0.5; c=$(frames); sleep 0.5
  expect Dark "radio button 'Dark' checked" "a click"
  [[ "$(background)" == dark ]] || fail "$tag: choosing Dark didn't switch the theme"
  (( b - a >= 4 )) || fail "$tag: only $((b - a)) frames while the knob slid"
  (( $(frames) == c )) || fail "$tag: frames after the knob came to rest"
  $H shot "$OUT/$tag-dark.png"

  # keyboard
  $H click $($H a11y where --role label Theme); settle
  expect Dark "radio button 'Dark' checked focused" "a click on the field's label"
  $H key Left; settle
  expect Light "radio button 'Light' checked focused" "Left"
  $H key Right; $H key Right; settle
  expect System "radio button 'System' checked focused" "Right wraps round"
  $H key Tab; settle
  $H a11y 2>/dev/null | grep -q "toggle button 'Light theme'.*focused\|toggle button 'Dark theme'.*focused" ||
    fail "$tag: Tab didn't leave the segmented control"

  # optional: choose, then clear with the same option
  $H click $($H a11y where --role "radio button" Compact); settle
  expect Compact "radio button 'Compact' checked" "Density chosen"
  $H click $($H a11y where --role "radio button" Compact); settle
  expect Compact "radio button 'Compact'" "a click on the chosen one clears"
  $H a11y 2>/dev/null | grep -qF "Nothing chosen: the default holds" || fail "$tag: the hint didn't follow"

  # toggles
  $H click $($H a11y where Italic); settle
  expect Italic "toggle button 'Italic' pressed" "Italic on"
  expect Bold "toggle button 'Bold' pressed" "Bold stays"
  $H click $($H a11y where Bold); settle
  expect Bold "toggle button 'Bold'" "Bold off"
  $H key space; settle
  expect Bold "toggle button 'Bold' focused pressed" "Space on the focused option"

  # plain: navigation; the other controls on the same value follow
  $H click $($H a11y where --role "radio button" Activity); settle
  $H a11y 2>/dev/null | grep -qF "label 'Showing activity'" || fail "$tag: the view didn't change"
  expect List "radio button 'List' checked" "the small control follows"
  $H shot "$OUT/$tag-done.png"

  pkill -x segmented || true
  echo "ok  $tag  ($((b - a)) frames in 250 ms of sliding; keyboard, optional, toggles, plain)"
}

trap '$H stop >/dev/null 2>&1 || true; pkill -x segmented || true' EXIT
for scale in 1 2; do run_case "$scale"; done

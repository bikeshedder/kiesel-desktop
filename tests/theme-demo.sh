#!/usr/bin/env bash
# Theme switching test for the gallery's theme page in a headless session, scale 1 and 2:
#   system   – with a (fake) XDG portal preferring dark, the app starts dark; a
#              change of the preference cross-fades the window, then frames stop
#   choice   – the switch (pointer) and Space on it choose light or dark; a choice
#              ignores the system; the sun and the moon choose too
#   saved    – the choice is in $XDG_CONFIG_HOME/de.kiesel.Gallery/kiesel.conf and
#              survives a restart; "Follow system" goes back to the portal
#   one run  – KIESEL_THEME picks a mode without saving it
# Screenshots and logs land in shots/theme-demo/.
set -euo pipefail
cd "$(dirname "$0")/.."

H=kiesel-headless
OUT=shots/theme-demo
mkdir -p "$OUT"
cargo build -q --release --example gallery -p kiesel-ui
BIN=target/release/examples/gallery
# the gallery shows this one page, filling the window
export KIESEL_PAGE=theme
export XDG_CONFIG_HOME="$PWD/$OUT/config"
CONF="$XDG_CONFIG_HOME/de.kiesel.Gallery/kiesel.conf"

fail() { echo "FAIL: $*" >&2; exit 1; }
settle() { sleep 0.5; }  # longer than the 300 ms fade
launch() { # [env…]
  pkill -x gallery || true
  env "$@" KIESEL_STATS=1 kiesel-headless run "$BIN" >"$OUT/$tag.log" 2>&1 &
  sleep 1.5
}
caption() { $H a11y | sed -n "s/^ *label '\(Following\|Light\|Dark\)\(.*\)'.*/\1\2/p"; }
expect_caption() {
  local got
  got=$(caption)
  [[ "$got" == "$1" ]] || fail "$tag: caption is '$got', expected '$1' ($2)"
}
# the window's background, from a corner pixel: dark or light
background() {
  $H shot "$OUT/$tag-bg.png"
  python3 -c "
import sys
from PIL import Image
r, g, b = Image.open(sys.argv[1]).convert('RGB').getpixel((3, 3))
print('dark' if r + g + b < 3 * 128 else 'light')" "$OUT/$tag-bg.png"
}
expect_bg() {
  local got
  got=$(background)
  [[ "$got" == "$1" ]] || fail "$tag: the window is $got, expected $1 ($2)"
}
frames() { grep -c '^frame' "$OUT/$tag.log" || true; }

run_case() {
  local scale=$1
  tag="${scale}x"
  rm -rf "$XDG_CONFIG_HOME"
  pkill -x gallery || true
  $H stop >/dev/null 2>&1 || true
  $H start --size 520x400 --scale "$scale" >/dev/null

  # the system prefers dark before the app starts: no light first frame
  $H portal dark
  launch
  expect_caption "Following the system (dark)" "at start"
  expect_bg dark "following a dark system"
  $H shot "$OUT/$tag-1-system-dark.png"

  # the system changes: a cross-fade (and the sun spinning in, 600 ms), then quiet
  local a b c
  a=$(frames)
  $H portal light; sleep 1.2
  b=$(frames); sleep 1; c=$(frames)
  expect_caption "Following the system (light)" "after the portal changed"
  expect_bg light "following a light system"
  (( b - a >= 8 )) || fail "$tag: only $((b - a)) frames for the cross-fade"
  (( c == b )) || fail "$tag: $((c - b)) frames after the fade ended"
  [[ ! -e "$CONF" ]] || fail "$tag: following the system wrote $CONF"

  # the switch chooses dark; the system no longer counts
  $H click $($H a11y where --role "toggle button" "Dark theme"); settle
  expect_caption "Dark, chosen here" "after clicking the switch"
  expect_bg dark "chosen dark"
  grep -qx "theme = dark" "$CONF" || fail "$tag: the choice was not saved"
  $H portal light; $H portal dark; $H portal light; settle
  expect_bg dark "a choice ignores the system"
  $H shot "$OUT/$tag-2-chosen-dark.png"

  # Space on the switch chooses light (it is focused from the click)
  $H key space; settle
  expect_caption "Light, chosen here" "after Space on the switch"
  # the moon and the sun choose as well
  $H click $($H a11y where --role "toggle button" "Light theme"); settle
  expect_caption "Light, chosen here" "pressing the pressed sun changes nothing"
  local moon
  moon=$($H a11y box --role "toggle button" "Dark theme" >/dev/null; $H a11y | grep -c "toggle button 'Dark theme'")
  [[ "$moon" == 2 ]] || fail "$tag: expected the switch and the moon named Dark theme"
  $H key Tab; $H key space; settle  # focus moves from the switch to the moon
  expect_caption "Dark, chosen here" "Space on the moon"

  # saved across a restart, even though the system prefers light
  launch
  expect_caption "Dark, chosen here" "after a restart"
  expect_bg dark "the saved choice at start"

  # back to the system
  $H a11y click "Follow system"; settle
  expect_caption "Following the system (light)" "after Follow system"
  expect_bg light "following the system again"
  grep -qx "theme = system" "$CONF" || fail "$tag: following the system was not saved"

  # one run in dark, without saving
  launch KIESEL_THEME=dark
  expect_caption "Dark, chosen here" "with KIESEL_THEME=dark"
  grep -qx "theme = system" "$CONF" || fail "$tag: KIESEL_THEME was saved"

  pkill -x gallery || true
  echo "ok  $tag  ($((b - a)) frames to cross-fade, 0 after)"
}

trap '$H stop >/dev/null 2>&1 || true; pkill -x gallery || true' EXIT
for scale in 1 2; do run_case "$scale"; done

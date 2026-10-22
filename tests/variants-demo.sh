#!/usr/bin/env bash
# Button styles, with the gallery's variants page in a headless session, scale 1 and 2:
#   sizes     – Sm/Md/Lg are 32/40/48 px high (the control heights); icon-only is square
#   toggle    – a pressed button is a toggle button; a click flips it
#   disabled  – disabled buttons are announced so, do nothing, leave the Tab order
#   groups    – an attached group is named and its segments toggle like the others
#   look      – the primary button is filled (accent), danger buttons are red
# Screenshots and logs land in shots/variants-demo/.
set -euo pipefail
cd "$(dirname "$0")/.."

H=kiesel-headless
OUT=shots/variants-demo
mkdir -p "$OUT"
cargo build -q --release --example gallery -p kiesel-ui
BIN=target/release/examples/gallery
# the gallery shows this one page, filling the window
export KIESEL_PAGE=variants
# its own settings: a theme chosen here stays here
export XDG_CONFIG_HOME="$PWD/$OUT/config"
rm -rf "$XDG_CONFIG_HOME"

fail() { echo "FAIL: $*" >&2; exit 1; }
settle() { sleep 0.4; }
box() { $H a11y box "$@" 2>/dev/null | head -1; }
states() { $H a11y info "$@" 2>/dev/null | tail -n +2; }
line() { $H a11y 2>/dev/null | grep -F "'$1'" | head -1 | sed 's/^ *//' || true; }
# the most common strongly coloured pixel in a box: r g b
tint() {
  $H shot "$OUT/$tag-tint.png"
  python3 -c "
import sys
from collections import Counter
from PIL import Image
s, (x, y, w, h) = float(sys.argv[2]), map(int, sys.argv[3:7])
im = Image.open(sys.argv[1]).convert('RGB').crop((int(x * s), int(y * s), int((x + w) * s), int((y + h) * s)))
px = [p for p in im.getdata() if max(p) - min(p) > 40]
print(*Counter(px).most_common(1)[0][0] if px else (0, 0, 0))" "$OUT/$tag-tint.png" "$scale" $1
}

run_case() {
  local scale=$1
  tag="${scale}x"
  pkill -x gallery || true
  $H stop >/dev/null 2>&1 || true
  $H start --size 860x620 --scale "$scale" >/dev/null
  $H run "$BIN" >"$OUT/$tag.log" 2>&1 &
  sleep 1.5

  local h
  read -r _ _ _ h <<<"$(box Small)";  (( h == 32 )) || fail "$tag: Small is $h px high"
  read -r _ _ _ h <<<"$(box Medium)"; (( h == 40 )) || fail "$tag: Medium is $h px high"
  read -r _ _ _ h <<<"$(box Large)";  (( h == 48 )) || fail "$tag: Large is $h px high"
  local w; read -r _ _ w h <<<"$(box Settings)"
  (( w == h && h == 32 )) || fail "$tag: the small icon-only button is ${w}x$h"

  # toggle
  [[ "$(line Bold)" == "toggle button 'Bold' pressed" ]] || fail "$tag: '$(line Bold)' at start"
  $H click $($H a11y where Bold); settle
  [[ "$(line Bold)" == "toggle button 'Bold'"* && "$(line Bold)" != *pressed* ]] || fail "$tag: a click didn't release Bold"

  # disabled
  $H click $($H a11y where --role "toggle button" Disabled); settle
  states Publish | grep -qx enabled && fail "$tag: Publish is still enabled"
  $H a11y focus Upload; $H key shift+Tab; settle
  [[ "$(line Bold)" == *focused* ]] || fail "$tag: Shift+Tab from Upload didn't skip the disabled buttons"
  $H click $($H a11y where --role "toggle button" Disabled); settle
  states Publish | grep -qx enabled || fail "$tag: Publish didn't come back"

  # the attached group
  $H a11y 2>/dev/null | grep -q "panel 'Alignment'" || fail "$tag: the group has no name"
  $H click $($H a11y where 'Align right'); settle
  [[ "$(line 'Align right')" == *pressed* && "$(line 'Align left')" != *pressed* ]] ||
    fail "$tag: the group's toggles didn't follow"

  # colours
  local r g b
  read -r r g b <<<"$(tint "$(box Primary)")"
  (( b > r + 40 )) || fail "$tag: the primary button isn't the (blue) accent: $r $g $b"
  read -r r g b <<<"$(tint "$(box 'Primary, danger')")"
  (( r > g + 60 && r > b + 30 )) || fail "$tag: the danger primary isn't red: $r $g $b"
  $H shot "$OUT/$tag-styles.png"

  pkill -x gallery || true
  echo "ok  $tag  (sizes 32/40/48, toggles, disabled out of the Tab order, groups, colours)"
}

trap '$H stop >/dev/null 2>&1 || true; pkill -x gallery || true' EXIT
for scale in 1 2; do run_case "$scale"; done

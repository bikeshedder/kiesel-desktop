#!/usr/bin/env bash
# System settings, with the gallery's theme page in a headless session, scale 1 and 2, all
# served by the fake XDG portal and changed while the app runs:
#   text scale – GNOME's text-scaling-factor 1.5 makes the text, the buttons and the
#                field larger; back at 1 every box is as before
#   accent     – the desktop's accent colour shows in the focused field
#   contrast   – "high" makes the secondary text darker (light theme)
#   motion     – with reduced motion a theme change is a cut: a frame or two, no fade
# Screenshots and logs land in shots/settings-demo/.
set -euo pipefail
cd "$(dirname "$0")/.."

H=kiesel-headless
OUT=shots/settings-demo
mkdir -p "$OUT"
cargo build -q --release --example gallery -p kiesel-ui
BIN=target/release/examples/gallery
# the gallery shows this one page, filling the window
export KIESEL_PAGE=theme
export XDG_CONFIG_HOME="$PWD/$OUT/config"

fail() { echo "FAIL: $*" >&2; exit 1; }
settle() { sleep 0.6; }
box() { $H a11y box "$@" 2>/dev/null | head -1; }
frames() { grep -c '^frame' "$OUT/$tag.log" || true; }

# pixels in the screenshot that are clearly green (the test's accent)
green() {
  $H shot "$OUT/$tag-$1.png"
  python3 -c "
import sys
from PIL import Image
im = Image.open(sys.argv[1]).convert('RGB')
print(sum(1 for r, g, b in im.getdata() if g > r + 60 and g > b + 60))" "$OUT/$tag-$1.png"
}

# the darkest pixel's brightness in the box x y w h (logical), 0 … 765
darkest() {
  $H shot "$OUT/$tag-$1.png"
  python3 -c "
import sys
from PIL import Image
s, (x, y, w, h) = float(sys.argv[2]), map(int, sys.argv[3:7])
im = Image.open(sys.argv[1]).convert('RGB')
crop = im.crop((int(x * s), int(y * s), int((x + w) * s), int((y + h) * s)))
print(min(r + g + b for r, g, b in crop.getdata()))" "$OUT/$tag-$1.png" "$scale" $2
}

run_case() {
  local scale=$1
  tag="${scale}x"
  rm -rf "$XDG_CONFIG_HOME"
  pkill -x gallery || true
  $H stop >/dev/null 2>&1 || true
  $H start --size 520x420 --scale "$scale" >/dev/null
  $H portal light
  KIESEL_STATS=1 $H run "$BIN" >"$OUT/$tag.log" 2>&1 &
  sleep 1.5

  # text scale: larger, then exactly as before
  local bx by bw bh fx fy fw fh
  read -r bx by bw bh <<<"$(box 'Follow system')"
  read -r fx fy fw fh <<<"$(box --role entry Note)"
  $H portal text-scale 1.5; settle
  local bw2 bh2 fh2
  read -r _ _ bw2 bh2 <<<"$(box 'Follow system')"
  read -r _ _ _ fh2 <<<"$(box --role entry Note)"
  (( bw2 * 10 >= bw * 13 )) || fail "$tag: the button grew from $bw to only $bw2 px at 1.5"
  (( bh2 > bh && fh2 > fh )) || fail "$tag: heights at 1.5 – button $bh → $bh2, field $fh → $fh2"
  $H shot "$OUT/$tag-1-text-scale.png"
  $H portal text-scale 1; settle
  [[ "$(box 'Follow system')" == "$bx $by $bw $bh" ]] ||
    fail "$tag: the button is at $(box 'Follow system') after scale 1, was $bx $by $bw $bh"

  # accent: a green focus ring around the field
  $H a11y focus Note; $H key Tab; $H key shift+Tab; settle
  local before after
  before=$(green 2-default-accent)
  $H portal accent 0.1 0.7 0.2; settle
  after=$(green 3-green-accent)
  (( before < 20 && after > 100 )) || fail "$tag: green pixels $before → $after with a green accent"
  $H portal accent none; settle

  # contrast: darker secondary text (the caption)
  local caption normal high
  caption=$(box 'Following the system (light)')
  normal=$(darkest 4-contrast-normal "$caption")
  $H portal contrast high; settle
  high=$(darkest 5-contrast-high "$caption")
  (( high + 15 < normal )) || fail "$tag: the caption's darkest pixel $normal → $high with high contrast"
  $H portal contrast normal; settle

  # reduced motion: a theme change without a fade (and without the icon spinning in)
  $H portal motion reduce; settle
  local a b
  a=$(frames)
  $H portal dark; settle
  b=$(frames)
  (( b - a <= 4 )) || fail "$tag: $((b - a)) frames for a theme change with reduced motion"
  $H shot "$OUT/$tag-6-reduced-motion-dark.png"
  $H portal motion normal; settle
  a=$(frames)
  $H portal light; settle
  b=$(frames)
  (( b - a >= 8 )) || fail "$tag: only $((b - a)) frames for the fade with motion back on"

  pkill -x gallery || true
  echo "ok  $tag  (button ${bw}→${bw2} px at 1.5, green $before→$after, caption $normal→$high)"
}

trap '$H stop >/dev/null 2>&1 || true; pkill -x gallery || true' EXIT
for scale in 1 2; do run_case "$scale"; done

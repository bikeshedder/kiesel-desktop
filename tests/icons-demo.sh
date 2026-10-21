#!/usr/bin/env bash
# Icons, with examples/icons in a headless session, scale 1 and 2:
#   a11y      – decorative icons stay out of the tree; a labelled one is an image;
#               icon-only buttons are named by their label
#   layout    – icon-only buttons are square (--ksl-control); a leading icon makes a
#               button wider by the icon and a gap
#   drawing   – the icon-only buttons' icons are there (dark pixels in their middle)
#   loading   – the spinner takes the icon's place: no width changes; frames while
#               spinning, none once it stops
# Screenshots and logs land in shots/icons-demo/.
set -euo pipefail
cd "$(dirname "$0")/.."

H=kiesel-headless
OUT=shots/icons-demo
mkdir -p "$OUT"
cargo build -q --release --example icons -p kiesel-ui
BIN=target/release/examples/icons

fail() { echo "FAIL: $*" >&2; exit 1; }
settle() { sleep 0.4; }
box() { $H a11y box "$1" 2>/dev/null | head -1; }
frames() { grep -c '^frame' "$OUT/$tag.log" || true; }

# dark pixels in the middle half of the box x y w h (logical)
ink() {
  python3 -c "
import sys
from PIL import Image
s, (x, y, w, h) = float(sys.argv[2]), map(int, sys.argv[3:7])
im = Image.open(sys.argv[1]).convert('RGB')
crop = im.crop((int((x + w / 4) * s), int((y + h / 4) * s), int((x + 3 * w / 4) * s), int((y + 3 * h / 4) * s)))
print(sum(1 for r, g, b in crop.getdata() if r + g + b < 3 * 170))" "$1" "$scale" $2
}

run_case() {
  local scale=$1
  tag="${scale}x"
  pkill -x icons || true
  $H stop >/dev/null 2>&1 || true
  $H start --size 540x380 --scale "$scale" >/dev/null
  KIESEL_STATS=1 $H run "$BIN" >"$OUT/$tag.log" 2>&1 &
  sleep 1.5

  local tree images
  tree=$($H a11y)
  images=$(grep -c '^ *image ' <<<"$tree" || true)
  [[ "$images" == 1 ]] || fail "$tag: $images images in the tree, expected only 'All saved'"
  grep -q "image 'All saved'" <<<"$tree" || fail "$tag: the labelled icon isn't an image"
  for b in Settings Delete; do
    grep -q "button '$b'" <<<"$tree" || fail "$tag: no button '$b'"
    read -r _ _ w h <<<"$(box $b)"
    (( w == 40 && h == 40 )) || fail "$tag: '$b' is ${w}x$h, expected a 40 px square"
  done
  local save next
  read -r _ _ save _ <<<"$(box Save)"
  read -r _ _ next _ <<<"$(box Next)"
  # Save and Next: label + gap 8 + icon 20 + padding 40 (labels of similar width)
  (( save >= 100 && next >= 100 )) || fail "$tag: Save ${save} px, Next ${next} px – no room for the icons"

  $H shot "$OUT/$tag-icons.png"
  local settings delete
  settings=$(ink "$OUT/$tag-icons.png" "$(box Settings)")
  delete=$(ink "$OUT/$tag-icons.png" "$(box Delete)")
  (( settings > 10 && delete > 10 )) || fail "$tag: icon-only buttons without icons ($settings, $delete dark pixels)"

  # loading: the spinner in the icon's place
  local before after a b
  before="$(box Save) $(box Delete)"
  $H click $($H a11y where Save); settle
  after="$(box Save) $(box Delete)"
  [[ "$before" == "$after" ]] || fail "$tag: loading moved or resized the buttons ($before → $after)"
  $H shot "$OUT/$tag-loading.png"
  a=$(frames); sleep 0.5; b=$(frames)
  (( b - a >= 10 )) || fail "$tag: only $((b - a)) frames in 0.5 s of spinning"
  $H click $($H a11y where Save); settle
  sleep 0.3; a=$(frames); sleep 1; b=$(frames)
  (( a == b )) || fail "$tag: $((b - a)) frames after the spinner stopped"

  pkill -x icons || true
  echo "ok  $tag  (Save ${save} px, icon-only 40 px squares, $settings/$delete px of icon)"
}

trap '$H stop >/dev/null 2>&1 || true; pkill -x icons || true' EXIT
for scale in 1 2; do run_case "$scale"; done

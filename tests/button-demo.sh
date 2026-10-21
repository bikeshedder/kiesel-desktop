#!/usr/bin/env bash
# Smoke test for examples/button in a headless session: hover, press, loading
# (spinner turns, button widens), unloading, light and dark, scale 1 and 2.
# Screenshots land in shots/button-demo/.
set -euo pipefail
cd "$(dirname "$0")/.."

H=kiesel-headless
OUT=shots/button-demo
mkdir -p "$OUT"
cargo build -q --release --example button -p kiesel-ui
cargo build -q -p kiesel-input
BIN=target/release/examples/button

fail() { echo "FAIL: $*" >&2; exit 1; }
differ() { python3 -c "import sys; from PIL import Image; a, b = (Image.open(p).convert('RGB') for p in sys.argv[1:3]); sys.exit(0 if a.tobytes() != b.tobytes() else 1)" "$1" "$2"; }
# width of the button's face along the row through the centre: the knob is
# lighter than the page in both schemes
face_width() {
  python3 -c "
import sys
from PIL import Image
im = Image.open(sys.argv[1]).convert('L'); w, h = im.size; y = h // 2
bg = im.getpixel((2, y))
xs = [x for x in range(w) if im.getpixel((x, y)) > bg + 6]
print(xs[-1] - xs[0] if xs else 0)" "$1"
}

run_case() {
  local scheme=$1 scale=$2 tag="$1-${2}x"
  $H stop >/dev/null 2>&1 || true
  $H start --size 480x300 --scale "$scale" >/dev/null
  KIESEL_THEME=$scheme $H run "$BIN" >"$OUT/$tag.log" 2>&1 &
  sleep 1.5
  $H move 10 10; sleep 0.4; $H shot "$OUT/$tag-rest.png"
  $H move 240 150; sleep 0.4; $H shot "$OUT/$tag-hover.png"
  differ "$OUT/$tag-rest.png" "$OUT/$tag-hover.png" || fail "$tag: hover changes nothing"
  $H down; sleep 0.4; $H shot "$OUT/$tag-pressed.png"
  differ "$OUT/$tag-hover.png" "$OUT/$tag-pressed.png" || fail "$tag: press changes nothing"
  $H up; $H move 10 10; sleep 0.4; $H shot "$OUT/$tag-loading-a.png"; sleep 0.2; $H shot "$OUT/$tag-loading-b.png"
  differ "$OUT/$tag-loading-a.png" "$OUT/$tag-loading-b.png" || fail "$tag: spinner does not turn"
  local rest loading
  rest=$(face_width "$OUT/$tag-rest.png"); loading=$(face_width "$OUT/$tag-loading-a.png")
  (( loading > rest )) || fail "$tag: loading button not wider ($rest → $loading px)"
  $H click 240 150; $H move 10 10; sleep 0.5; $H shot "$OUT/$tag-done.png"
  differ "$OUT/$tag-rest.png" "$OUT/$tag-done.png" && fail "$tag: not back at rest after the second click"
  pkill -x button || true
  echo "ok  $tag  (face ${rest}px → ${loading}px while loading)"
}

trap '$H stop >/dev/null 2>&1 || true; pkill -x button || true' EXIT
for scheme in light dark; do
  for scale in 1 2; do run_case "$scheme" "$scale"; done
done

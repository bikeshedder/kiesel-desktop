#!/usr/bin/env bash
# Smoke test for examples/button in a headless session, light and dark, scale 1 and 2:
#   pointer   – hover and press change the look; a click on "Save changes" starts
#               loading (spinner turns, button widens), "Reset" stops it
#   keyboard  – Tab / Shift+Tab move a visible focus ring, Space (on release) and
#               Enter (on press) activate
#   a11y      – the AT-SPI tree has both buttons, reports focus and busy, and its
#               click / focus actions work; positions come from the tree too
# Screenshots land in shots/button-demo/.
set -euo pipefail
cd "$(dirname "$0")/.."

H=kiesel-headless
OUT=shots/button-demo
mkdir -p "$OUT"
cargo build -q --release --example button -p kiesel-ui
BIN=target/release/examples/button

fail() { echo "FAIL: $*" >&2; exit 1; }
differ() { python3 -c "import sys; from PIL import Image; a, b = (Image.open(p).convert('RGB') for p in sys.argv[1:3]); sys.exit(0 if a.tobytes() != b.tobytes() else 1)" "$1" "$2"; }
# width of the knobs' faces along the row through the centre: they are lighter
# than the page in both schemes
face_width() {
  python3 -c "
import sys
from PIL import Image
im = Image.open(sys.argv[1]).convert('L'); w, h = im.size; y = h // 2
bg = im.getpixel((2, y))
xs = [x for x in range(w) if im.getpixel((x, y)) > bg + 6]
print(xs[-1] - xs[0] if xs else 0)" "$1"
}
# the AT-SPI state line of a button, e.g. "busy focused"
states() { $H a11y | sed -n "s/^ *push button '$1' *//p; s/^ *button '$1' *//p" | head -1; }
expect_states() {
  local got
  got=$(states "$1")
  [[ "$got" == "$2" ]] || fail "$tag: '$1' is '${got}', expected '$2' ($3)"
}
settle() { sleep 0.4; }

run_case() {
  local scheme=$1 scale=$2
  tag="$1-${2}x"
  $H stop >/dev/null 2>&1 || true
  $H start --size 480x300 --scale "$scale" >/dev/null
  KIESEL_THEME=$scheme $H run "$BIN" >"$OUT/$tag.log" 2>&1 &
  sleep 1.5
  local reset save
  reset=$($H a11y where Reset) || fail "$tag: no 'Reset' in the accessibility tree"
  save=$($H a11y where "Save changes") || fail "$tag: no 'Save changes' in the accessibility tree"
  expect_states Reset "" "at start"
  expect_states "Save changes" "" "at start"

  # pointer
  $H move 10 10; settle; $H shot "$OUT/$tag-rest.png"
  $H move $save; settle; $H shot "$OUT/$tag-hover.png"
  differ "$OUT/$tag-rest.png" "$OUT/$tag-hover.png" || fail "$tag: hover changes nothing"
  $H down; settle; $H shot "$OUT/$tag-pressed.png"
  differ "$OUT/$tag-hover.png" "$OUT/$tag-pressed.png" || fail "$tag: press changes nothing"
  $H up; $H move 10 10; settle; $H shot "$OUT/$tag-loading-a.png"; sleep 0.2; $H shot "$OUT/$tag-loading-b.png"
  differ "$OUT/$tag-loading-a.png" "$OUT/$tag-loading-b.png" || fail "$tag: spinner does not turn"
  expect_states "Save changes" "busy focused" "after a click"
  local rest loading
  rest=$(face_width "$OUT/$tag-rest.png"); loading=$(face_width "$OUT/$tag-loading-a.png")
  (( loading > rest )) || fail "$tag: loading button not wider ($rest → $loading px)"
  differ "$OUT/$tag-loading-a.png" "$OUT/$tag-rest.png" || fail "$tag: loading looks like rest"
  reset=$($H a11y where Reset)  # the row re-centred
  $H click $reset; $H move 10 10; settle; $H shot "$OUT/$tag-reset.png"
  expect_states "Save changes" "" "after Reset"
  expect_states Reset "focused" "after clicking it"
  # pointer focus shows no ring: the same pixels as at rest
  differ "$OUT/$tag-rest.png" "$OUT/$tag-reset.png" && fail "$tag: pointer focus shows a ring"

  # keyboard
  $H key Tab; settle; $H shot "$OUT/$tag-tab.png"
  expect_states "Save changes" "focused" "after Tab"
  differ "$OUT/$tag-rest.png" "$OUT/$tag-tab.png" || fail "$tag: no focus ring after Tab"
  $H keydown space; settle; $H shot "$OUT/$tag-space-held.png"
  expect_states "Save changes" "focused" "while Space is held (activates on release)"
  differ "$OUT/$tag-tab.png" "$OUT/$tag-space-held.png" || fail "$tag: Space held shows no press"
  $H keyup space; settle
  expect_states "Save changes" "busy focused" "after Space"
  $H key Return; settle
  expect_states "Save changes" "focused" "after Enter (toggles back)"
  $H key shift+Tab; settle
  expect_states Reset "focused" "after Shift+Tab"
  $H key Tab; $H key Tab; settle
  expect_states Reset "focused" "after Tab wraps around"

  # assistive technology
  $H a11y click "Save changes"; settle
  expect_states "Save changes" "busy" "after the AT click"
  $H a11y focus "Save changes"; settle
  expect_states "Save changes" "busy focused" "after the AT focus"
  $H a11y click Reset; settle
  expect_states "Save changes" "focused" "after the AT click on Reset"

  pkill -x button || true
  echo "ok  $tag  (faces ${rest}px → ${loading}px while loading)"
}

trap '$H stop >/dev/null 2>&1 || true; pkill -x button || true' EXIT
for scheme in light dark; do
  for scale in 1 2; do run_case "$scheme" "$scale"; done
done

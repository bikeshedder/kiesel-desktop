#!/usr/bin/env bash
# Sliders, with the gallery's slider page in a headless session, scale 1 and 2:
#   a11y      – sliders named by their field, value and limits through AT-SPI's Value
#   keyboard  – arrows step, PageUp a tenth, Home/End the ends; steps of 5 snap
#   pointer   – a press on the track jumps there, a drag follows to the end
#   errors    – past 90 % the brightness field's error describes the slider
#   assistive – a value set through AT-SPI is taken (snapped to the step)
#   idle      – no frames once the knob has settled
# Screenshots and logs land in shots/slider-demo/.
set -euo pipefail
cd "$(dirname "$0")/.."

H=kiesel-headless
OUT=shots/slider-demo
mkdir -p "$OUT"
cargo build -q --release --example gallery -p kiesel-ui
BIN=target/release/examples/gallery
# the gallery shows this one page, filling the window
export KIESEL_PAGE=slider
# its own settings: a theme chosen here stays here
export XDG_CONFIG_HOME="$PWD/$OUT/config"
rm -rf "$XDG_CONFIG_HOME"

fail() { echo "FAIL: $*" >&2; exit 1; }
settle() { sleep 0.3; }
num() { $H a11y number --role slider "$1" | awk '{print $1}'; }
expect() { [[ "$1" == "$2" ]] || fail "$tag: '$1', expected '$2' ($3)"; }
frames() { grep -c '^frame' "$OUT/$tag.log" || true; }

run_case() {
  local scale=$1
  tag="${scale}x"
  pkill -x gallery || true
  $H stop >/dev/null 2>&1 || true
  $H start --size 560x560 --scale "$scale" >/dev/null
  KIESEL_STATS=1 $H run "$BIN" >"$OUT/$tag.log" 2>&1 &
  sleep 1.5

  expect "$($H a11y number --role slider Volume)" "40 0 100" "value and limits"
  expect "$($H a11y number --role slider Temperature)" "21.5 16 28" "a range of its own"

  # keyboard
  $H click $($H a11y where --role label Volume); settle
  $H key Right; $H key Right; settle;  expect "$(num Volume)" 42 "Right twice"
  $H key Page_Up; settle;              expect "$(num Volume)" 52 "PageUp: a tenth"
  $H key End; settle;                  expect "$(num Volume)" 100 "End"
  $H key Home; settle;                 expect "$(num Volume)" 0 "Home"
  $H a11y focus --role slider Brightness; $H key Left; settle
  expect "$(num Brightness)" 65 "steps of 5"

  # pointer: jump, then drag to the far end
  local x y w h
  read -r x y w h <<<"$($H a11y box --role slider Temperature)"
  $H click $((x + 2)) $((y + h / 2)); settle
  expect "$(num Temperature)" 16 "a press at the start"
  $H move $((x + 2)) $((y + h / 2)); $H down
  $H move $((x + w / 2)) $((y + h / 2)); $H move $((x + w + 40)) $((y + h / 2)); $H up; settle
  expect "$(num Temperature)" 28 "dragged past the end"

  # the field's error
  $H a11y number --role slider Brightness = 96; settle
  expect "$(num Brightness)" 95 "a value from assistive technology, snapped"
  [[ "$($H a11y info --role slider Brightness | sed -n 's/^description: //p')" == "Above 90 % wears the screen" ]] ||
    fail "$tag: no error past 90 %"
  $H shot "$OUT/$tag-sliders.png"

  sleep 0.5; local a b; a=$(frames); sleep 1; b=$(frames)
  (( a == b )) || fail "$tag: $((b - a)) frames while idle"

  pkill -x gallery || true
  echo "ok  $tag  (keys, steps of 5, press and drag, AT-SPI value, error)"
}

trap '$H stop >/dev/null 2>&1 || true; pkill -x gallery || true' EXIT
for scale in 1 2; do run_case "$scale"; done

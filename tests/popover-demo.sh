#!/usr/bin/env bash
# Popups, with the gallery's popover page in a headless session, scale 1 and 2:
#   open/close – a click opens a panel below its trigger; a click outside, a click on the
#                trigger (without reopening it), Escape and the panel's own button close it
#   placement  – near the window's bottom a panel opens above its trigger; a panel that fits on
#                neither side of its trigger goes below it, inside the window
#   focus      – opened from the keyboard, focus moves into the panel, Tab goes round inside,
#                and closing returns focus to the trigger
#   a11y       – an open panel is a dialog in the AT-SPI tree; a closed one is gone
#   idle       – after the animations, no frames
# Screenshots and logs land in shots/popover-demo/.
set -euo pipefail
cd "$(dirname "$0")/.."

H=kiesel-headless
OUT=shots/popover-demo
mkdir -p "$OUT"
cargo build -q --release --example gallery -p kiesel-ui
BIN=target/release/examples/gallery
# the gallery shows this one page, filling the window
export KIESEL_PAGE=popover
# its own settings: a theme chosen here stays here
export XDG_CONFIG_HOME="$PWD/$OUT/config"
rm -rf "$XDG_CONFIG_HOME"

fail() { echo "FAIL: $*" >&2; exit 1; }
settle() { sleep 0.4; }   # opening 180 ms, closing 130 ms
open_count() { $H a11y 2>/dev/null | grep -c "dialog '$1'" || true; }
expect_open() {
  local got
  got=$(open_count "$1")
  [[ "$got" == "$2" ]] || fail "$tag: '$1' open=$got, expected $2 ($3)"
}
focused() { $H a11y 2>/dev/null | sed -n "s/^ *\(push button\|button\|entry\) '\(.*\)' focused$/\2/p"; }
expect_focus() {
  local got
  got=$(focused)
  [[ "$got" == "$1" ]] || fail "$tag: focus on '$got', expected '$1' ($2)"
}
box() { $H a11y box $1 2>/dev/null | head -1; }
frames() { grep -c '^frame' "$OUT/$tag.log" || true; }

run_case() {
  local scale=$1
  tag="${scale}x"
  pkill -x gallery || true
  $H stop >/dev/null 2>&1 || true
  $H start --size 560x440 --scale "$scale" >/dev/null
  KIESEL_STATS=1 $H run "$BIN" >"$OUT/$tag.log" 2>&1 &
  sleep 1.5

  # a click opens it below the trigger, aligned to its start
  $H click $($H a11y where Account); settle
  expect_open Account 1 "after a click on its trigger"
  local tx ty tw th px py pw ph
  read -r tx ty tw th <<<"$(box Account)"
  read -r px py pw ph <<<"$(box '--role dialog Account')"
  (( px == tx && py >= ty + th + 6 && py <= ty + th + 10 )) ||
    fail "$tag: panel at $px,$py, trigger at $tx,$ty ${tw}x$th – expected below it, 8 px apart"
  $H shot "$OUT/$tag-open.png"

  # a click outside closes it
  $H click 520 300; settle
  expect_open Account 0 "after a click outside"

  # a click on its open trigger only closes it; the next one opens it again
  $H click $($H a11y where Account); settle
  $H click $($H a11y where Account); settle
  expect_open Account 0 "a click on the open trigger closes it"
  $H click $($H a11y where Account); settle
  expect_open Account 1 "and the next opens it"
  $H key Escape; settle
  expect_open Account 0 "after Escape"

  # near the bottom edge: above the trigger
  $H click $($H a11y where More); settle
  read -r tx ty tw th <<<"$(box More)"
  read -r px py pw ph <<<"$(box '--role dialog More')"
  (( py + ph <= ty - 6 )) || fail "$tag: the More panel isn't above its trigger ($py+$ph vs $ty)"
  $H shot "$OUT/$tag-flipped.png"
  $H key Escape; settle
  expect_focus More "Escape returns focus to the trigger"

  # no room on either side: below, inside the window
  $H click $($H a11y where Info); settle
  read -r tx ty tw th <<<"$(box Info)"
  read -r px py pw ph <<<"$(box '--role dialog Info')"
  (( py > ty + th && px >= 8 && px + pw <= 560 - 8 )) ||
    fail "$tag: the Info panel at $px,$py ${pw}x$ph isn't below its trigger inside the window"
  $H key Escape; settle

  # the keyboard: focus goes in, Tab goes round inside, Done brings it back
  $H a11y focus Account; $H key Return; settle
  expect_focus "Display name" "opened from the keyboard"
  $H key Tab; expect_focus Done "Tab inside the panel"
  $H key Tab; expect_focus "Display name" "Tab goes round inside"
  $H key shift+Tab; $H key space; settle
  expect_open Account 0 "after Done"
  expect_focus Account "focus back on the trigger"

  # idle once the animations are over
  sleep 0.5; local a b; a=$(frames); sleep 1; b=$(frames)
  (( a == b )) || fail "$tag: $((b - a)) frames while idle"

  pkill -x gallery || true
  echo "ok  $tag  (panel ${pw}x$ph px, open/close/flip/focus)"
}

trap '$H stop >/dev/null 2>&1 || true; pkill -x gallery || true' EXIT
for scale in 1 2; do run_case "$scale"; done

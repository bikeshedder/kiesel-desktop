#!/usr/bin/env bash
# Scrolling, with examples/list (200 rows in a scroll view) in a headless session, scale 1 and 2:
#   wheel      – three notches glide the content up; frames redraw the viewport only
#   touchpad   – a swipe moves it, and momentum carries it further after the fingers lift, then stops
#   keys       – Tab reveals the focused row; End, Home, PageDown from a focused button reach the
#                scroll view (keys the button doesn't use go up)
#   scrollbar  – dragging the thumb to the bottom shows the last row
#   a11y       – rows just outside the view are exposed, and scrolling one into view works
#   idle       – once the scrollbar has faded, no frames
# Screenshots and logs land in shots/list-demo/.
set -euo pipefail
cd "$(dirname "$0")/.."

H=kiesel-headless
OUT=shots/list-demo
mkdir -p "$OUT"
cargo build -q --release --example list -p kiesel-ui
BIN=target/release/examples/list

fail() { echo "FAIL: $*" >&2; exit 1; }
settle() { sleep 0.6; }   # glides take 150 ms; the tree follows when motion rests
# a node's top edge (logical), empty if it isn't in the tree
top() { $H a11y box "$1" 2>/dev/null | awk '{print $2}' || true; }
frames() { grep -c '^frame' "$OUT/$tag.log" || true; }
# the scroll offset, from the first visible row: rows are 48 px apart (40 px button, 8 px gap)
offset() {
  local n y
  read -r n y < <($H a11y 2>/dev/null | sed -n "s/^ *label 'Item \([0-9]*\)'$/\1/p" | head -1 |
    while read -r n; do echo "$n $(top "Item $n")"; done)
  echo $(( (n - 1) * 48 + start1 - y ))
}

run_case() {
  local scale=$1
  tag="${scale}x"
  pkill -x list || true
  $H stop >/dev/null 2>&1 || true
  $H start --size 520x560 --scale "$scale" >/dev/null
  KIESEL_STATS=1 $H run "$BIN" >"$OUT/$tag.log" 2>&1 &
  sleep 1.5

  # the scroll view's box: rows are visible inside it
  local vx vy vw vh view_top view_bottom
  read -r vx vy vw vh <<<"$($H a11y box --role panel "" 2>/dev/null | head -1)"
  view_top=$vy; view_bottom=$((vy + vh))
  local start5 start1
  start5=$(top "Item 5"); start1=$(top "Item 1")
  [[ -n "$start5" ]] || fail "$tag: Item 5 not in view at start"
  [[ -z "$(top 'Item 150')" ]] || fail "$tag: a row far below the view is exposed"

  # wheel: three notches, a glide, the viewport redrawn and nothing else
  $H move $((vx + vw / 2)) $((vy + vh / 2)); settle  # the hover's frame first
  local a b c
  a=$(frames)
  $H wheel 3; settle
  local now5
  now5=$(top "Item 5")
  [[ -n "$now5" ]] && (( now5 < start5 - 50 )) || fail "$tag: the wheel did not scroll (Item 5 at $start5 → $now5)"
  # (the first frame may catch the other buffer up with the start's full frames)
  tail -n +$((a + 2)) "$OUT/$tag.log" | grep '^frame' | python3 -c "
import sys, re
share = [float(re.search(r'\(([\d.]+) %', l).group(1)) for l in sys.stdin]
assert share, 'no frames'
big = [s for s in share if s > 60]
sys.exit(f'FAIL: $tag: {len(big)} of {len(share)} wheel frames redrew more than the viewport') if big else None"

  # touchpad: momentum after the lift
  $H key Home 2>/dev/null || true
  # touchpad: a swipe, then momentum carries it further, then it rests
  local before after lifted
  before=$(offset)
  lifted=$(frames)
  $H swipe 120 6 120
  sleep 3
  after=$(offset)
  (( after - before > 120 + 60 )) || fail "$tag: no momentum: a 120 px swipe moved the content $((after - before)) px"
  (( $(frames) - lifted > 20 )) || fail "$tag: no momentum frames after the lift"
  sleep 1
  [[ "$(offset)" == "$after" ]] || fail "$tag: momentum did not stop"

  # keys: Tab reveals; End/Home/PageDown bubble from the focused button
  $H key Tab; $H key Tab; $H key Tab; $H key Tab; settle
  $H a11y | grep -q "button 'Open item 1' focused" || fail "$tag: Tab did not reach the first row"
  local t1; t1=$(top "Open item 1")
  [[ -n "$t1" ]] && (( t1 >= view_top && t1 < view_bottom )) || fail "$tag: the focused row isn't in view ($t1)"
  $H key End; settle
  local t200; t200=$(top "Item 200")
  [[ -n "$t200" ]] && (( t200 < view_bottom )) || fail "$tag: End did not show the last row ($t200)"
  $H key Home; settle
  [[ -n "$(top 'Item 1')" ]] || fail "$tag: Home did not go back to the top"
  $H key Page_Down; settle
  [[ -z "$(top 'Item 1')" ]] || fail "$tag: PageDown did not scroll"
  $H shot "$OUT/$tag-scrolled.png"

  # scrollbar: drag the thumb to the bottom
  $H key Home; settle
  local bar_x=$((vx + vw - 5))
  $H move "$bar_x" $((vy + 10)); $H down
  $H move "$bar_x" $((vy + vh / 2)); $H move "$bar_x" $((vy + vh + 40)); $H up; settle
  t200=$(top "Item 200")
  [[ -n "$t200" ]] && (( t200 < view_bottom )) || fail "$tag: dragging the thumb down did not show the last row ($t200)"

  # assistive technology: the row just past the view is there, and can be scrolled into view
  # (after the drag nothing is focused – keys need a focused widget, so focus a row first)
  $H a11y focus "Open item 200"; $H key Home; settle
  local last
  last=$($H a11y | sed -n "s/^ *label '\(Item [0-9]*\)'$/\1/p" | tail -1)
  local before_last; before_last=$(top "$last")
  (( before_last >= view_bottom - 24 )) || fail "$tag: '$last' is the last exposed row but it is in view"
  $H a11y scrollto "$last"; settle
  local after_last; after_last=$(top "$last")
  [[ -n "$after_last" ]] && (( after_last < view_bottom - 20 )) || fail "$tag: scrollTo did not bring '$last' into view ($before_last → $after_last)"

  # idle: after the scrollbar fades, nothing more
  sleep 1.8; a=$(frames); sleep 1; b=$(frames)
  (( a == b )) || fail "$tag: $((b - a)) frames while idle"

  pkill -x list || true
  echo "ok  $tag  (wheel moved Item 5 from $start5 to $now5; momentum and keys work)"
}

trap '$H stop >/dev/null 2>&1 || true; pkill -x list || true' EXIT
for scale in 1 2; do run_case "$scale"; done

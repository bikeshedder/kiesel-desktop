#!/usr/bin/env bash
# Checkboxes, radios and switches, with the gallery's choices page in a headless session, scale 1 and 2:
#   a11y      – groups named by their field, required; rows are check boxes, radio
#               buttons and a switch; "Select all" is indeterminate while some are
#   pointer   – a press on a row's text toggles it; "Select all" checks all, then none
#   keyboard  – Space toggles; only the chosen radio (or the first) is a Tab stop; the
#               arrows choose and move focus, wrapping; a click on the field's label
#               focuses the group's stop
#   errors    – "Continue" without a plan or consent: descriptions, red label
#   idle      – no frames once the springs have settled
# Screenshots and logs land in shots/choices-demo/.
set -euo pipefail
cd "$(dirname "$0")/.."

H=kiesel-headless
OUT=shots/choices-demo
mkdir -p "$OUT"
cargo build -q --release --example gallery -p kiesel-ui
BIN=target/release/examples/gallery
# the gallery shows this one page, filling the window
export KIESEL_PAGE=choices
# its own settings: a theme chosen here stays here
export XDG_CONFIG_HOME="$PWD/$OUT/config"
rm -rf "$XDG_CONFIG_HOME"

fail() { echo "FAIL: $*" >&2; exit 1; }
settle() { sleep 0.5; }  # the springs take up to 380 ms
line() { $H a11y 2>/dev/null | grep -F "$1" | head -1 | sed 's/^ *//' || true; }
expect() {
  local got; got=$(line "'$1'")
  # focus only counts where the expectation names it
  [[ "$2" == *focused ]] || got=${got% focused}
  [[ "$got" == "$2" ]] || fail "$tag: '$got', expected '$2' ($3)"
}
states() { $H a11y info "$@" 2>/dev/null | tail -n +2; }
desc() { $H a11y info "$@" 2>/dev/null | sed -n 's/^description: //p'; }
frames() { grep -c '^frame' "$OUT/$tag.log" || true; }
# where a row's text is: its box's right part (not the indicator)
text_of() { local x y w h; read -r x y w h <<<"$($H a11y box "$@")"; echo "$((x + w - 20)) $((y + h / 2))"; }
red() {
  python3 -c "
import sys
from PIL import Image
s, (x, y, w, h) = float(sys.argv[2]), map(int, sys.argv[3:7])
im = Image.open(sys.argv[1]).convert('RGB')
crop = im.crop((int(x * s), int(y * s), int((x + w) * s), int((y + h) * s)))
print(sum(1 for r, g, b in crop.getdata() if r > g + 60 and r > b + 40))" "$1" "$scale" $2
}

run_case() {
  local scale=$1
  tag="${scale}x"
  pkill -x gallery || true
  $H stop >/dev/null 2>&1 || true
  $H start --size 560x820 --scale "$scale" >/dev/null
  KIESEL_STATS=1 $H run "$BIN" >"$OUT/$tag.log" 2>&1 &
  sleep 1.5

  # the tree
  expect Plan "label 'Plan'" "the field's label"
  $H a11y 2>/dev/null | grep -q "panel 'Plan'" || fail "$tag: the radio group isn't named by its field"
  states --role panel Plan | grep -qx required || fail "$tag: the radio group isn't required"
  expect Monthly "radio button 'Monthly'" "a radio"
  expect "Automatic updates" "toggle button 'Automatic updates' pressed" "the switch, on"
  states "Select all" | grep -qx indeterminate || fail "$tag: Select all isn't indeterminate with 1 of 3"
  [[ "$(desc --role "radio button" Yearly)" == "Two months free" ]] || fail "$tag: Yearly's description"

  # pointer: on the text, not the box
  $H click $(text_of --role "check box" Push); settle
  expect Push "check box 'Push' checked" "a press on the label"
  $H click $(text_of "Select all"); settle
  expect "Text message" "check box 'Text message' checked" "Select all checks all"
  ! states "Select all" | grep -qx indeterminate || fail "$tag: Select all still indeterminate"
  $H click $(text_of "Select all"); settle
  expect Email "check box 'Email'" "Select all again: none"
  $H click $(text_of --role "toggle button" "Automatic updates"); settle
  expect "Automatic updates" "toggle button 'Automatic updates'" "the switch off from its label"

  # errors
  $H click $($H a11y where Continue); settle
  [[ "$(desc --role panel Plan)" == "Choose a plan" ]] || fail "$tag: the group's error"
  [[ "$(desc "I accept the terms")" == "Please accept the terms to continue" ]] || fail "$tag: the consent's error"
  $H shot "$OUT/$tag-errors.png"
  local reds; reds=$(red "$OUT/$tag-errors.png" "$($H a11y box "I accept the terms")")
  (( reds > 40 )) || fail "$tag: the consent isn't red ($reds red pixels)"

  # keyboard: the first radio is the stop, arrows choose and move focus
  $H click $($H a11y where --role label Plan); settle
  expect Monthly "radio button 'Monthly' focused" "a click on the field's label"
  $H key Down; settle
  expect Yearly "radio button 'Yearly' checked focused" "Down from the first"
  $H key Up; $H key Up; settle
  expect Team "radio button 'Team' checked focused" "Up wraps round"
  $H key Tab; $H key shift+Tab; settle
  expect Team "radio button 'Team' checked focused" "Tab comes back to the chosen radio"
  $H key Tab; settle
  expect "Select all" "check box 'Select all' focused" "Tab leaves the group"
  $H key space; settle
  expect Email "check box 'Email' checked" "Space on Select all"
  [[ "$(desc --role panel Plan)" == "" ]] || fail "$tag: the plan's error stayed"

  # through
  $H click $(text_of "I accept the terms"); settle
  $H click $($H a11y where Continue); settle
  expect "Subscribed: team" "label 'Subscribed: team'" "the form went through"
  $H shot "$OUT/$tag-done.png"

  sleep 0.5; local a b; a=$(frames); sleep 1; b=$(frames)
  (( a == b )) || fail "$tag: $((b - a)) frames while idle"

  pkill -x gallery || true
  echo "ok  $tag  (tree, pointer, keyboard, errors – $reds red px; idle)"
}

trap '$H stop >/dev/null 2>&1 || true; pkill -x gallery || true' EXIT
for scale in 1 2; do run_case "$scale"; done

#!/usr/bin/env bash
# Number and password fields, with the gallery's inputs page in a headless session, scale 1 and 2:
#   numbers   – spin buttons with value and limits (AT-SPI Value); Up/Down step, Shift ten;
#               typing sets the value, leaving clamps it; − is disabled at the minimum;
#               a held + repeats; a value set by assistive technology shows; German
#               writes a decimal comma
#   password  – password text reading as bullets; Ctrl+C copies nothing while hidden;
#               the eye shows it (and is announced pressed); Caps Lock gets a note
#               while hidden; the confirmation's error when they differ
# Screenshots and logs land in shots/inputs-demo/.
set -euo pipefail
cd "$(dirname "$0")/.."

H=kiesel-headless
OUT=shots/inputs-demo
mkdir -p "$OUT"
cargo build -q --release --example gallery -p kiesel-ui
BIN=target/release/examples/gallery
# the gallery shows this one page, filling the window
export KIESEL_PAGE=inputs
# its own settings: a theme chosen here stays here
export XDG_CONFIG_HOME="$PWD/$OUT/config"
rm -rf "$XDG_CONFIG_HOME"

fail() { echo "FAIL: $*" >&2; exit 1; }
settle() { sleep 0.3; }
num() { $H a11y number --role "spin button" "$1" | awk '{print $1}'; }
text() { $H a11y value --role "$1" "$2"; }
has() { $H a11y 2>/dev/null | grep -qF "$1"; }
expect() { [[ "$1" == "$2" ]] || fail "$tag: '$1', expected '$2' ($3)"; }

run_case() {
  local scale=$1
  tag="${scale}x"
  pkill -x gallery || true
  $H stop >/dev/null 2>&1 || true
  $H start --size 560x600 --scale "$scale" >/dev/null
  env -u KIESEL_LANG -u LC_ALL -u LC_MESSAGES -u LANGUAGE LANG=en_US.UTF-8 \
    $H run "$BIN" >"$OUT/$tag.log" 2>&1 &
  sleep 1.5

  # numbers
  expect "$($H a11y number --role 'spin button' Quantity)" "2 1 10" "value, min, max"
  $H click $($H a11y where --role label Quantity); settle
  $H key Up; $H key Up; settle
  expect "$(num Quantity)" 4 "Up twice"
  $H key shift+Down; settle
  expect "$(num Quantity)" 1 "Shift+Down: ten steps, to the minimum"
  $H a11y info Decrease | grep -qx enabled && fail "$tag: − is enabled at the minimum"
  $H key ctrl+a; $H type 42; settle
  expect "$(text 'spin button' Quantity)" 42 "typed"
  $H key Tab; settle
  expect "$(num Quantity)" 10 "leaving clamps to the maximum"
  local plus; plus=$($H a11y where Increase)
  $H a11y number --role 'spin button' Quantity = 3; settle
  expect "$(text 'spin button' Quantity)" 3 "a value from assistive technology shows"
  $H move $plus; $H down; sleep 1.2; $H up; settle
  local held; held=$(num Quantity)
  (( held >= 7 )) || fail "$tag: holding + for 1.2 s went only to $held"
  expect "$(text 'spin button' Width)" 12.5 "English"
  $H key ctrl+l; settle
  expect "$(text 'spin button' Width)" "12,5" "German writes a decimal comma"
  $H key ctrl+l; settle

  # password
  $H run wl-copy -- before
  $H click $($H a11y where --role label Password); settle
  $H type Secret12; settle
  expect "$(text 'password text' Password)" "••••••••" "hidden: bullets"
  $H key ctrl+a; $H key ctrl+c; settle
  expect "$($H run wl-paste -n)" before "Ctrl+C on a hidden password"
  $H key End; $H key Caps_Lock; $H type a; settle
  has "label 'Caps Lock is on'" || fail "$tag: no Caps Lock note"
  $H key Caps_Lock; $H key BackSpace; settle
  $H shot "$OUT/$tag-hidden.png"
  $H click $($H a11y where --role "toggle button" "Show password"); settle
  has "toggle button 'Hide password'" || fail "$tag: the eye isn't 'Hide password' now"
  expect "$(text 'password text' Password)" Secret12 "shown: the text"
  ! has "label 'Caps Lock is on'" || fail "$tag: the Caps Lock note while shown"
  $H shot "$OUT/$tag-shown.png"
  $H click $($H a11y where --role label 'Repeat it'); $H type Secret1; settle
  [[ "$($H a11y info --role 'password text' 'Repeat it' | sed -n 's/^description: //p')" == "The passwords differ" ]] ||
    fail "$tag: no error while they differ"
  $H type 2; settle
  [[ -z "$($H a11y info --role 'password text' 'Repeat it' | sed -n 's/^description: //p')" ]] ||
    fail "$tag: the error stayed"

  pkill -x gallery || true
  echo "ok  $tag  (steps, limits, held + to $held, German comma; bullets, no copy, eye, Caps Lock)"
}

trap '$H stop >/dev/null 2>&1 || true; pkill -x gallery || true; pkill -x wl-copy || true' EXIT
for scale in 1 2; do run_case "$scale"; done

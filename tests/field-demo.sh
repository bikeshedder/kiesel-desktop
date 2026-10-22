#!/usr/bin/env bash
# Fields, with the gallery's field page in a headless session, scale 1 and 2:
#   names     – the label names the control (AT-SPI name from labelled-by); the
#               hint is its description; "required" only on the required field
#   label     – a click on the label focuses the control
#   errors    – submitting empty shows Name's error: red label, the message as the
#               description; Email checks as you type, and its hint comes back
#   submit    – with both fine, the form goes through
# Screenshots and logs land in shots/field-demo/.
set -euo pipefail
cd "$(dirname "$0")/.."

H=kiesel-headless
OUT=shots/field-demo
mkdir -p "$OUT"
cargo build -q --release --example gallery -p kiesel-ui
BIN=target/release/examples/gallery
# the gallery shows this one page, filling the window
export KIESEL_PAGE=field
# its own settings: a theme chosen here stays here
export XDG_CONFIG_HOME="$PWD/$OUT/config"
rm -rf "$XDG_CONFIG_HOME"

fail() { echo "FAIL: $*" >&2; exit 1; }
settle() { sleep 0.4; }
has() { $H a11y 2>/dev/null | grep -qF "$1" || fail "$tag: no \"$1\" ($2)"; }
hasnt() { ! $H a11y 2>/dev/null | grep -qF "$1" || fail "$tag: still \"$1\" ($2)"; }
desc() { $H a11y info --role entry "$1" 2>/dev/null | sed -n 's/^description: //p'; }
states() { $H a11y info --role entry "$1" 2>/dev/null | tail -n +2; }
expect_desc() {
  local got; got=$(desc "$1")
  [[ "$got" == "$2" ]] || fail "$tag: $1's description is '$got', expected '$2' ($3)"
}
# red pixels in the box x y w h (logical) of the screenshot
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
  $H start --size 520x480 --scale "$scale" >/dev/null
  $H run "$BIN" >"$OUT/$tag.log" 2>&1 &
  sleep 1.5

  has "entry 'Name'" "the label names the field"
  has "entry 'Email'" "the label names the field"
  expect_desc Email "We never share it" "the hint describes it"
  expect_desc Name "" "no hint, no error"
  states Name | grep -qx required || fail "$tag: Name isn't required"
  ! states Email | grep -qx required || fail "$tag: Email is required"

  # submitting empty: Name's error
  $H click $($H a11y where "Create account"); settle
  has "label 'Please enter your name'" "the error after submitting"
  expect_desc Name "Please enter your name" "the error describes it"
  $H shot "$OUT/$tag-errors.png"
  local reds; reds=$(red "$OUT/$tag-errors.png" "$($H a11y box --role label Name)")
  (( reds > 20 )) || fail "$tag: the invalid field's label isn't red ($reds red pixels)"

  # a click on the label focuses the control; Email checks as you type
  $H click $($H a11y where --role label Email); settle
  states Email | grep -qx focused || fail "$tag: a click on the label didn't focus the field"
  $H type ada; settle
  has "label \"That isn't an email address\"" "Email's error while typing"
  expect_desc Email "That isn't an email address" "the error replaces the hint"
  $H type @example.com; settle
  hasnt "That isn't an email address" "fixed"
  expect_desc Email "We never share it" "the hint is back"

  # both fine: through
  $H click $($H a11y where --role label Name); $H type Ada; settle
  hasnt "Please enter your name" "a name was entered"
  $H click $($H a11y where "Create account"); settle
  has "label 'Created an account for Ada'" "the form went through"
  $H shot "$OUT/$tag-done.png"

  pkill -x gallery || true
  echo "ok  $tag  (names, descriptions, required, label focus, errors; $reds red px in the label)"
}

trap '$H stop >/dev/null 2>&1 || true; pkill -x gallery || true' EXIT
for scale in 1 2; do run_case "$scale"; done

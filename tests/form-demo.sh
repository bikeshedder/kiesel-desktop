#!/usr/bin/env bash
# Text input test for examples/form in a headless session, scale 1 and 2:
#   keyboard   – Tab focuses (and selects), typing, Ctrl+Backspace, Home/Shift+End, Enter submits
#   pointer    – a click places the caret, a double click selects a word
#   clipboard  – Ctrl+C reaches the system clipboard, Ctrl+V takes from it (wl-copy/wl-paste)
#   IME        – the input method is on only while the field is focused; preedit is not the
#                value yet, commit is, delete-surrounding removes characters
#   a11y       – value, caret and setting the text through AT-SPI's Text/EditableText
#   frames     – the caret blinks at 2 frames/s, and stops (no frames) after 10 s
# Screenshots and logs land in shots/form-demo/.
set -euo pipefail
cd "$(dirname "$0")/.."

H=kiesel-headless
OUT=shots/form-demo
mkdir -p "$OUT"
cargo build -q --release --example form -p kiesel-ui
BIN=target/release/examples/form

fail() { echo "FAIL: $*" >&2; exit 1; }
settle() { sleep 0.3; }
value() { $H a11y value --role entry Name; }
expect_value() {
  local got
  got=$(value)
  [[ "$got" == "$1" ]] || fail "$tag: value is '$got', expected '$1' ($2)"
}
frames() { grep -c '^frame' "$OUT/$tag.log" || true; }

run_case() {
  local scale=$1
  tag="${scale}x"
  pkill -x form || true
  $H stop >/dev/null 2>&1 || true
  $H start --size 520x360 --scale "$scale" >/dev/null
  KIESEL_STATS=1 $H run "$BIN" >"$OUT/$tag.log" 2>&1 &
  sleep 1.5
  $H ime commit x >/dev/null 2>&1 && fail "$tag: the input method is on before anything is focused"

  # keyboard
  $H key Tab; settle
  $H a11y | grep -q "entry 'Name' focused" || fail "$tag: Tab did not focus the field"
  $H type "Website relaunch 2026"; settle
  expect_value "Website relaunch 2026" "typed"
  $H a11y | grep -q "Will be created as “Website relaunch 2026”" || fail "$tag: the derived caption did not follow"
  $H key ctrl+BackSpace; settle
  expect_value "Website relaunch " "Ctrl+Backspace removes a word"
  $H key Home; $H key shift+End; $H type "Intranet"; settle
  expect_value "Intranet" "Home, Shift+End selects, typing replaces"
  $H shot "$OUT/$tag-typed.png"

  # clipboard, both ways
  $H key ctrl+a; $H key ctrl+c; settle
  [[ "$($H run wl-paste -n)" == "Intranet" ]] || fail "$tag: Ctrl+C did not reach the clipboard"
  $H run wl-copy -- " portal"; settle
  $H key End; $H key ctrl+v; settle
  expect_value "Intranet portal" "Ctrl+V from the system clipboard"

  # pointer: a click puts the caret, a double click selects a word
  local x y w h cy
  read -r x y w h <<<"$($H a11y box --role entry Name)"
  cy=$((y + h / 2))
  $H click $((x + w - 4)) "$cy"; settle
  [[ "$($H a11y caret --role entry Name)" == 15 ]] || fail "$tag: a click past the text did not put the caret at the end"
  $H click $((x + 30)) "$cy"; $H click $((x + 30)) "$cy"; $H type "Extranet"; settle
  expect_value "Extranet portal" "a double click selects the word"

  # input method
  $H key End
  $H ime preedit "ありが"; settle
  $H shot "$OUT/$tag-preedit.png"
  expect_value "Extranet portal" "preedit is not the value yet"
  $H ime commit "ありがとう"; settle
  expect_value "Extranet portalありがとう" "commit inserts"
  $H ime delete 6 0; settle
  expect_value "Extranet portalありが" "delete-surrounding removes two characters (6 bytes)"

  # assistive technology
  $H a11y set --role entry Name = "Set by AT"; settle
  expect_value "Set by AT" "EditableText.setTextContents"
  $H a11y caret --role entry Name = 3; settle
  [[ "$($H a11y caret --role entry Name)" == 3 ]] || fail "$tag: the caret did not move to 3"
  $H type "X"; settle
  expect_value "SetX by AT" "typing at the caret moved by AT"

  # Enter submits; focus leaving turns the input method off
  $H key Return; settle
  $H a11y | grep -q "label 'Created “SetX by AT”'" || fail "$tag: Enter did not submit"
  $H key Tab; settle
  $H ime commit x >/dev/null 2>&1 && fail "$tag: the input method stayed on after focus left"

  # frames: blinking costs 2 frames/s, an idle caret none
  $H key shift+Tab; $H move 5 5; sleep 1
  local a b c d
  a=$(frames); sleep 3; b=$(frames)
  (( b - a >= 5 && b - a <= 8 )) || fail "$tag: $((b - a)) frames in 3 s of blinking, expected about 6"
  sleep 8; c=$(frames); sleep 3; d=$(frames)
  (( d == c )) || fail "$tag: $((d - c)) frames in 3 s after the caret stopped blinking"

  pkill -x form || true
  pkill -x wl-copy || true  # it forked to keep serving the clipboard
  echo "ok  $tag  ($((b - a)) frames in 3 s blinking, $((d - c)) when idle)"
}

trap '$H stop >/dev/null 2>&1 || true; pkill -x form || true; pkill -x wl-copy || true' EXIT
for scale in 1 2; do run_case "$scale"; done

#!/usr/bin/env bash
# Textareas, with the gallery's textarea page in a headless session, scale 1 and 2:
#   a11y      – a multi-line entry named by its field, the count as its description
#   editing   – Enter starts a line, Up/End/Home move within lines, typed lines are the value
#   scrolling – more lines than rows: the caret's line stays in view; the wheel scrolls
#               the text (not the page), and stops at the ends
#   submit    – Ctrl+Enter sends; over the limit the field turns invalid and won't send
#   idle      – no frames once the caret has stopped blinking
# Screenshots and logs land in shots/textarea-demo/.
set -euo pipefail
cd "$(dirname "$0")/.."

H=kiesel-headless
OUT=shots/textarea-demo
mkdir -p "$OUT"
cargo build -q --release --example gallery -p kiesel-ui
BIN=target/release/examples/gallery
# the gallery shows this one page, filling the window
export KIESEL_PAGE=textarea
# its own settings: a theme chosen here stays here
export XDG_CONFIG_HOME="$PWD/$OUT/config"
rm -rf "$XDG_CONFIG_HOME"

fail() { echo "FAIL: $*" >&2; exit 1; }
settle() { sleep 0.3; }
value() { $H a11y value --role entry Message; }
desc() { $H a11y info --role entry Message | sed -n 's/^description: //p'; }
has() { $H a11y 2>/dev/null | grep -qF "$1" || fail "$tag: no \"$1\" ($2)"; }
# a fingerprint of the field's pixels
look() {
  $H shot "$OUT/$tag-look.png"
  python3 -c "
import sys, hashlib
from PIL import Image
s, (x, y, w, h) = float(sys.argv[2]), map(int, sys.argv[3:7])
im = Image.open(sys.argv[1]).convert('RGB').crop((int(x * s), int(y * s), int((x + w) * s), int((y + h) * s)))
print(hashlib.md5(im.tobytes()).hexdigest())" "$OUT/$tag-look.png" "$scale" $($H a11y box --role entry Message)
}
frames() { grep -c '^frame' "$OUT/$tag.log" || true; }

run_case() {
  local scale=$1
  tag="${scale}x"
  pkill -x gallery || true
  $H stop >/dev/null 2>&1 || true
  $H start --size 640x620 --scale "$scale" >/dev/null
  KIESEL_STATS=1 $H run "$BIN" >"$OUT/$tag.log" 2>&1 &
  sleep 1.5

  $H a11y info --role entry Message | grep -qx 'multi line' || fail "$tag: not a multi-line entry"
  [[ "$(desc)" == "0 of 280" ]] || fail "$tag: the description is '$(desc)'"

  # editing across lines
  $H click $($H a11y where --role label Message); settle
  $H type one; $H key Return; $H type two; settle
  [[ "$(value)" == $'one\ntwo' ]] || fail "$tag: value '$(value)' after two lines"
  $H key Up; $H key End; $H type '!'; $H key Down; $H key Home; $H type '>'; settle
  [[ "$(value)" == $'one!\n>two' ]] || fail "$tag: Up/End/Down/Home gave '$(value)'"

  # more lines than it shows: the caret's line stays in view
  local i
  for i in 3 4 5 6 7 8; do $H key Return; $H type "line $i"; done; settle
  local fx fy fw fh
  read -r fx fy fw fh <<<"$($H a11y box --role entry Message)"
  $H shot "$OUT/$tag-scrolled.png"
  local bottom; bottom=$(look)

  # the wheel scrolls the text (so it had followed the caret down), up to its top and no further
  $H move $((fx + fw / 2)) $((fy + fh / 2))
  $H wheel -20; settle
  local top; top=$(look)
  [[ "$top" != "$bottom" ]] || fail "$tag: the wheel didn't scroll the text"
  $H wheel -5; settle
  [[ "$(look)" == "$top" ]] || fail "$tag: the text scrolled past its top"
  read -r _ fy2 _ _ <<<"$($H a11y box --role entry Message)"
  (( fy2 == fy )) || fail "$tag: the page moved instead ($fy → $fy2)"

  # Ctrl+Enter sends
  $H key ctrl+Return; settle
  has "label 'Sent 8 line(s)'" "Ctrl+Enter"

  # over the limit: invalid, and nothing is sent
  $H a11y set --role entry Message = "$(printf 'x%.0s' $(seq 300))"; settle
  [[ "$(desc)" == "20 characters too many" ]] || fail "$tag: the description is '$(desc)' over the limit"
  $H click $($H a11y where Send); settle
  has "label 'Sent 8 line(s)'" "nothing new sent while invalid"
  $H shot "$OUT/$tag-invalid.png"

  $H click $($H a11y where --role label 'Write a message'); sleep 0.5
  local a b; a=$(frames); sleep 1; b=$(frames)
  (( a == b )) || fail "$tag: $((b - a)) frames while idle"

  pkill -x gallery || true
  echo "ok  $tag  (lines, scrolling by caret and wheel, Ctrl+Enter, limit)"
}

trap '$H stop >/dev/null 2>&1 || true; pkill -x gallery || true' EXIT
for scale in 1 2; do run_case "$scale"; done

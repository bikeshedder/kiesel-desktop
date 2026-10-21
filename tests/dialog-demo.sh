#!/usr/bin/env bash
# Layout and redraw test for examples/dialog in a headless session, scale 1 and 2:
#   layout   – the paragraph wraps to more lines in a narrow window, the
#              buttons stay on the card's right; all of it found through AT-SPI
#   redraw   – while the spinner turns, frames redraw a small region only
#   pixels   – partial redraws end up identical (±2 per channel) to redrawing
#              everything (KIESEL_FULL_REDRAW=1) after the same interactions
# Screenshots and logs land in shots/dialog-demo/.
set -euo pipefail
cd "$(dirname "$0")/.."

H=kiesel-headless
OUT=shots/dialog-demo
mkdir -p "$OUT"
cargo build -q --release --example dialog -p kiesel-ui
BIN=target/release/examples/dialog
PARAGRAPH="You changed the invoice period and two time entries. Save them now, or reset the form to what was stored – your edits are gone then."

fail() { echo "FAIL: $*" >&2; exit 1; }
settle() { sleep 0.4; }

launch() { # <scale> <log> [env…]
  local scale=$1 log=$2
  shift 2
  pkill -x dialog || true
  $H stop >/dev/null 2>&1 || true
  $H start --size 640x420 --scale "$scale" >/dev/null
  env "$@" KIESEL_STATS=1 kiesel-headless run "$BIN" >"$log" 2>&1 &
  sleep 1.5
}

# the same interactions, ending in a settled state: hover, press, focus, unhover
interact() {
  local save reset
  save=$($H a11y where "Save changes"); reset=$($H a11y where Reset)
  $H move $save; settle; $H move $reset; settle
  $H down; settle; $H up; $H key Tab; settle
  $H move 5 5; $H key Tab; sleep 0.8
}

run_case() {
  local scale=$1 tag="${1}x"

  # layout: narrower window, more lines; buttons right-aligned in the card
  launch "$scale" "$OUT/$tag.log"
  local wide narrow card save
  wide=$($H a11y box "$PARAGRAPH" | awk '{print $4}')
  $H run swaymsg -q 'floating enable, resize set 320 420, move position 0 0'; sleep 0.8
  narrow=$($H a11y box "$PARAGRAPH" | awk '{print $4}')
  (( narrow > wide )) || fail "$tag: paragraph did not wrap further ($wide → $narrow px high)"
  $H shot "$OUT/$tag-narrow.png"
  # back to tiled: the window fills the output from its origin again (a resized
  # floating window is re-centred, and AT-SPI positions are window-relative here)
  $H run swaymsg -q 'floating disable'; sleep 0.8
  card=$($H a11y box "$PARAGRAPH" | awk '{print $1 + $3}')
  save=$($H a11y box "Save changes" | awk '{print $1 + $3}')
  (( save == card )) || fail "$tag: buttons end at $save, the text at $card – not aligned to the end"

  # redraw: the spinner only redraws around the button
  : >"$OUT/$tag.log.frames"
  local before
  before=$(grep -c '^frame' "$OUT/$tag.log")
  $H click $($H a11y where "Save changes"); $H move 5 5; sleep 1.5
  tail -n +"$((before + 1))" "$OUT/$tag.log" | grep '^frame' >"$OUT/$tag.log.frames"
  python3 - "$OUT/$tag.log.frames" "$tag" <<'EOF' || exit 1
import re, sys
frames = [l for l in open(sys.argv[1]) if l.startswith("frame")]
share = [float(re.search(r"\(([\d.]+) %", l).group(1)) for l in frames]
partial = sum("partial" in l for l in frames)
if len(frames) < 30:
    sys.exit(f"FAIL: {sys.argv[2]}: only {len(frames)} frames while the spinner turned")
if partial < 0.9 * len(frames) or sorted(share)[len(share) // 2] > 15:
    sys.exit(f"FAIL: {sys.argv[2]}: {partial}/{len(frames)} partial, median {sorted(share)[len(share) // 2]} % of the window")
print(f"     {sys.argv[2]}: {partial}/{len(frames)} frames partial, median {sorted(share)[len(share) // 2]:.1f} % of the window")
EOF
  $H click $($H a11y where "Save changes"); settle

  # pixels: partial redraws vs. full redraws after the same interactions
  launch "$scale" "$OUT/$tag-partial.log"; interact; $H shot "$OUT/$tag-partial.png"
  launch "$scale" "$OUT/$tag-full.log" KIESEL_FULL_REDRAW=1; interact; $H shot "$OUT/$tag-full.png"
  python3 - "$OUT/$tag-partial.png" "$OUT/$tag-full.png" "$tag" <<'EOF' || exit 1
import sys
from PIL import Image, ImageChops
a, b = (Image.open(p).convert("RGB") for p in sys.argv[1:3])
diff = max(hi for _, hi in ImageChops.difference(a, b).getextrema())
if diff > 2:
    sys.exit(f"FAIL: {sys.argv[3]}: partial and full redraws differ by up to {diff}")
EOF
  pkill -x dialog || true
  echo "ok  $tag  (paragraph ${wide} → ${narrow} px high when narrow)"
}

trap '$H stop >/dev/null 2>&1 || true; pkill -x dialog || true' EXIT
for scale in 1 2; do run_case "$scale"; done

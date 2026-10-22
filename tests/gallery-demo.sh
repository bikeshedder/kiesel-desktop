#!/usr/bin/env bash
# The gallery as a whole (the pages have their own tests, run with KIESEL_PAGE), scale 1 and 2:
#   pages     – every page in the list opens (its own content in the tree), the last
#               one's widgets go; the arrows walk the list
#   app-wide  – the theme chosen on one page holds on the others; Ctrl+L switches the
#               language from any page
#   memory    – visiting every page twice doesn't grow memory (pages are dropped)
#   idle      – no frames once a page has settled
# Screenshots and logs land in shots/gallery-demo/.
set -euo pipefail
cd "$(dirname "$0")/.."

H=kiesel-headless
OUT=shots/gallery-demo
mkdir -p "$OUT"
cargo build -q --release --example gallery -p kiesel-ui
BIN=target/release/examples/gallery
export XDG_CONFIG_HOME="$PWD/$OUT/config"

fail() { echo "FAIL: $*" >&2; exit 1; }
settle() { sleep 0.5; }
has() { $H a11y 2>/dev/null | grep -qF "$1"; }
frames() { grep -c '^frame' "$OUT/$tag.log" || true; }
pss() { awk '/^Pss:/{print $2}' "/proc/$(pgrep -x gallery)/smaps_rollup"; }
background() {
  $H shot "$OUT/$tag-bg.png"
  python3 -c "
import sys
from PIL import Image
r, g, b = Image.open(sys.argv[1]).convert('RGB').getpixel((3, 3))
print('dark' if r + g + b < 3 * 128 else 'light')" "$OUT/$tag-bg.png"
}

# page in the list → something only that page shows
PAGES=(
  "Buttons|button 'Save changes'"
  "Button styles|panel 'Alignment'"
  "Icons|image 'All saved'"
  "Text input|label 'New project'"
  "Numbers & passwords|label 'Numbers and passwords'"
  "Sliders|slider 'Volume'"
  "Fields|label 'Create an account'"
  "Textarea|label 'Write a message'"
  "Choices|label 'Subscription'"
  "Segmented|panel 'Format'"
  "Wrapping text|label 'Unsaved changes'"
  "Popovers|label 'Popovers'"
  "Long list|label 'Item 1'"
  "Tasks|button 'Add'"
  "Theme|button 'Follow system'"
  "Languages|button 'Deutsch'"
  "Shortcuts|label 'Ctrl+S'"
)

visit_all() {
  local prev=""
  for entry in "${PAGES[@]}"; do
    local name=${entry%%|*} mark=${entry#*|}
    $H click $($H a11y where --role "radio button" "$name"); settle
    has "$mark" || fail "$tag: '$name' didn't show $mark"
    [[ -z "$prev" ]] || ! has "$prev" || fail "$tag: '$name' still shows the last page's $prev"
    prev=$mark
  done
}

run_case() {
  local scale=$1
  tag="${scale}x"
  rm -rf "$XDG_CONFIG_HOME"
  pkill -x gallery || true
  $H stop >/dev/null 2>&1 || true
  $H start --size 960x720 --scale "$scale" >/dev/null
  $H portal light
  env -u KIESEL_PAGE -u KIESEL_LANG -u LC_ALL -u LC_MESSAGES -u LANGUAGE LANG=en_US.UTF-8 KIESEL_STATS=1 \
    $H run "$BIN" >"$OUT/$tag.log" 2>&1 &
  sleep 1.5
  has "radio button 'Buttons' checked" || fail "$tag: the gallery doesn't open on its first page"

  visit_all
  local before after
  before=$(pss)
  visit_all
  after=$(pss)
  (( after <= before + 2048 )) || fail "$tag: memory grew from $before to $after KiB over a second round"

  # the arrows walk the list
  $H a11y focus Shortcuts; $H key Up; settle
  has "radio button 'Languages' checked" || fail "$tag: Up didn't go to the page above"

  # one theme for all pages, one language switch everywhere
  $H click $($H a11y where --role "radio button" Theme); settle
  $H click $($H a11y where --role "toggle button" "Dark theme"); settle
  $H click $($H a11y where --role "radio button" Buttons); settle
  [[ "$(background)" == dark ]] || fail "$tag: the dark theme didn't hold on another page"
  $H key ctrl+l; settle
  $H click $($H a11y where --role "radio button" Languages); settle
  has "label 'Sprachen'" || fail "$tag: Ctrl+L on the Buttons page didn't switch to German"
  $H key ctrl+l; settle
  $H shot "$OUT/$tag-dark.png"

  sleep 0.5; local a b; a=$(frames); sleep 1; b=$(frames)
  (( a == b )) || fail "$tag: $((b - a)) frames while idle"

  pkill -x gallery || true
  echo "ok  $tag  (17 pages twice, PSS $((before / 1024)) → $((after / 1024)) MiB; theme and language app-wide)"
}

trap '$H stop >/dev/null 2>&1 || true; pkill -x gallery || true' EXIT
for scale in 1 2; do run_case "$scale"; done

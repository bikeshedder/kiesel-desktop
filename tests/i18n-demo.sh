#!/usr/bin/env bash
# Messages, with examples/i18n in a headless session, scale 1 and 2:
#   detection – LANG=de_AT.UTF-8 gives German; a language without a catalog gives English
#   catalogs  – Kiesel's messages (Cancel, the ThemeSwitch's names) and the app's own
#   formats   – plurals (1 Ergebnis / 2 Ergebnisse), lists (Ann und Bo), grouped numbers
#   switching – a click on English changes every text and name; buttons take their new width
# Screenshots and logs land in shots/i18n-demo/.
set -euo pipefail
cd "$(dirname "$0")/.."

H=kiesel-headless
OUT=shots/i18n-demo
mkdir -p "$OUT"
cargo build -q --release --example i18n -p kiesel-ui
BIN=target/release/examples/i18n

fail() { echo "FAIL: $*" >&2; exit 1; }
settle() { sleep 0.3; }
has() { $H a11y 2>/dev/null | grep -qF "$1" || fail "$tag: no \"$1\" ($2)"; }
width() { $H a11y box "$1" 2>/dev/null | awk '{print $3}'; }

launch() { # env…
  pkill -x i18n || true
  env -u KIESEL_LANG -u LC_ALL -u LC_MESSAGES -u LANGUAGE "$@" kiesel-headless run "$BIN" >"$OUT/$tag.log" 2>&1 &
  sleep 1.5
}

run_case() {
  local scale=$1
  tag="${scale}x"
  $H stop >/dev/null 2>&1 || true
  $H start --size 520x420 --scale "$scale" >/dev/null

  # a language without a catalog: English
  launch LANG=fr_FR.UTF-8
  has "label 'Languages'" "fr falls back to English"

  # German from LANG, with a region and an encoding
  launch LANG=de_AT.UTF-8
  has "label 'Sprachen'" "the app's own catalog"
  has "button 'Abbrechen'" "Kiesel's catalog"
  has "toggle button 'Dunkles Design'" "the ThemeSwitch's names"
  has "panel 'Design'" "the desktop catalog"
  has "label '1 Ergebnis'" "plural: one"
  has "label '1.000 von 1.234.567'" "German digit grouping"
  $H shot "$OUT/$tag-de.png"
  local de_width; de_width=$(width "Einer mehr")

  $H a11y click "Einer mehr"; settle
  has "label '2 Ergebnisse'" "plural: other"
  has "label 'Ann und Bo schreiben…'" "a list and an exact plural key"

  # switching at runtime
  $H a11y click English; settle
  has "label 'Languages'" "after switching"
  has "button 'Cancel'" "Kiesel's messages follow"
  has "toggle button 'Dark theme'" "names for screen readers follow"
  has "label '2 results'" "the plural follows"
  has "label 'Ann and Bo are typing…'" "the list follows"
  has "label '2,000 of 1,234,567'" "English digit grouping"
  local en_width; en_width=$(width "One more")
  (( en_width < de_width )) || fail "$tag: the button didn't take its new width ($de_width → $en_width)"
  $H shot "$OUT/$tag-en.png"

  pkill -x i18n || true
  echo "ok  $tag  (\"Einer mehr\" ${de_width} px → \"One more\" ${en_width} px)"
}

trap '$H stop >/dev/null 2>&1 || true; pkill -x i18n || true' EXIT
for scale in 1 2; do run_case "$scale"; done

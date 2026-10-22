#!/usr/bin/env bash
# Selects, with the gallery's select page in a headless session, scale 1 and 2:
#   open      – a click opens the list below, focus on the chosen option; Escape closes,
#               focus back on the trigger, nothing changed
#   keyboard  – Down skips the disabled option and wraps; a letter jumps; Enter chooses
#               and closes; Enter on the trigger opens it again
#   clear     – "Clear" at the list's foot empties an optional value
#   search    – typing filters (codes too), counts the matches, says when none;
#               Down goes into the list, Up back; Enter takes the first match
#   multiple  – Space and Enter tick and the list stays open; Backspace on the trigger
#               takes the last value off
#   combobox  – typing shows the matching suggestions, the field keeps the focus;
#               Down/Down/Enter takes the second – its label is the text; Escape closes;
#               Down on an empty field shows them all; free text stays
#   room      – the list stays inside the window
# Screenshots and logs land in shots/select-demo/.
set -euo pipefail
cd "$(dirname "$0")/.."

H=kiesel-headless
OUT=shots/select-demo
mkdir -p "$OUT"
cargo build -q --release --example gallery -p kiesel-ui
BIN=target/release/examples/gallery
# the gallery shows this one page, filling the window
export KIESEL_PAGE=select
# its own settings: a theme chosen here stays here
export XDG_CONFIG_HOME="$PWD/$OUT/config"
rm -rf "$XDG_CONFIG_HOME"

fail() { echo "FAIL: $*" >&2; exit 1; }
settle() { sleep 0.45; }   # the list opens in 180 ms, closes in 130
focused() { $H a11y 2>/dev/null | grep ' focused' | head -1 | sed "s/^ *//; s/ focused//" || true; }
summary() { $H a11y 2>/dev/null | sed -n "s/^ *label '\(.* · .*\)'$/\1/p"; }
has() { $H a11y 2>/dev/null | grep -qF "$1"; }
expect() { [[ "$1" == "$2" ]] || fail "$tag: '$1', expected '$2' ($3)"; }
open_count() { $H a11y 2>/dev/null | grep -c '^ *dialog ' || true; }

run_case() {
  local scale=$1
  tag="${scale}x"
  pkill -x gallery || true
  $H stop >/dev/null 2>&1 || true
  $H start --size 600x760 --scale "$scale" >/dev/null
  env -u KIESEL_LANG -u LC_ALL -u LC_MESSAGES -u LANGUAGE LANG=en_US.UTF-8 \
    $H run "$BIN" >"$OUT/$tag.log" 2>&1 &
  sleep 1.5
  expect "$(summary)" "active · no country · email" "at start"

  # open and close
  $H click $($H a11y where --role "combo box" Status); settle
  expect "$(open_count)" 1 "a click opens the list"
  expect "$(focused)" "list item 'Active'" "focus on the chosen option"
  $H key Escape; settle
  expect "$(open_count)" 0 "Escape closes"
  expect "$(focused)" "combo box 'Status'" "focus back on the trigger"

  # keyboard
  $H key Return; settle
  $H key Down; $H key Down; $H key Down; settle
  expect "$(focused)" "list item 'Active'" "Down skips Archived (disabled) and wraps"
  $H key d; settle
  expect "$(focused)" "list item 'Done'" "a letter jumps"
  $H key Return; settle
  expect "$(summary)" "done · no country · email" "Enter chooses"
  expect "$(open_count)" 0 "and closes"

  # clear
  $H key Return; settle
  $H click $($H a11y where --role button Clear); settle
  expect "$(summary)" "no status · no country · email" "Clear"
  expect "$(focused)" "combo box 'Status'" "focus back on the trigger (Clear went away)"

  # search
  $H click $($H a11y where --role "combo box" Country); settle
  expect "$(focused)" "entry 'Search options'" "focus in the search field"
  $H type jp; settle
  has "list item 'Japan'" || fail "$tag: 'jp' didn't find Japan by its code"
  has "label '1 of 18'" || fail "$tag: no count"
  $H key Down; settle
  expect "$(focused)" "list item 'Japan'" "Down into the list"
  $H key Up; settle
  expect "$(focused)" "entry 'Search options'" "Up back to the search"
  $H key ctrl+a; $H type zz; settle
  has "No results for “zz”" || fail "$tag: no 'No results'"
  $H key ctrl+a; $H type swe; $H key BackSpace; $H key BackSpace; $H key BackSpace; $H type swi; settle
  $H key Return; settle
  expect "$(summary)" "no status · ch · email" "Enter took the first match"
  local y h; read -r _ y _ h <<<"$($H a11y box --role "combo box" Country)"
  $H key Return; settle
  local py ph; read -r _ py _ ph <<<"$($H a11y box --role dialog 'Choose a country…')"
  (( py + ph <= 760 )) || fail "$tag: the list leaves the window ($py + $ph)"
  $H shot "$OUT/$tag-search.png"
  $H key Escape; settle

  # several
  $H click $($H a11y where --role "combo box" "Notify via"); settle
  $H key Down; $H key space; settle
  expect "$(open_count)" 1 "the list stays open"
  $H key Down; $H key Return; settle
  expect "$(summary)" "no status · ch · email+push+sms" "Space and Enter tick"
  $H shot "$OUT/$tag-multiple.png"
  $H key Escape; settle
  $H key BackSpace; settle
  expect "$(summary)" "no status · ch · email+push" "Backspace takes the last off"

  # the combobox
  $H click $($H a11y where --role label City); settle
  $H type ber; settle
  expect "$(open_count)" 1 "typing shows the suggestions"
  expect "$($H a11y 2>/dev/null | grep -c "list item 'Ber")" 3 "Berlin, Bern, Bergen"
  expect "$(focused)" "combo box 'City'" "the field keeps the focus"
  $H key Down; $H key Down; $H key Return; settle
  expect "$($H a11y value --role 'combo box' City)" Bern "the label is the text"
  has "label 'Suggestion taken: brn'" || fail "$tag: on_choose didn't hear 'brn'"
  expect "$(open_count)" 0 "taken, the list closes"
  $H key ctrl+a; $H type Ham; settle
  expect "$(open_count)" 1 "Hamburg suggested"
  $H key Escape; settle
  expect "$(open_count)" 0 "Escape closes"
  expect "$($H a11y value --role 'combo box' City)" Ham "free text stays"
  $H key ctrl+a; $H key BackSpace; $H key Down; settle
  expect "$($H a11y 2>/dev/null | grep -c "^ *list item")" 8 "Down on an empty field: all of them"
  $H shot "$OUT/$tag-combobox.png"
  $H key Escape; settle

  pkill -x gallery || true
  echo "ok  $tag  (open/close, keys, clear, search, several, combobox, inside the window)"
}

trap '$H stop >/dev/null 2>&1 || true; pkill -x gallery || true' EXIT
for scale in 1 2; do run_case "$scale"; done

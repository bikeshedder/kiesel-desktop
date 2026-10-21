#!/usr/bin/env bash
# Changing children, with examples/todo in a headless session, scale 1 and 2:
#   For        – rows appear for added tasks (Enter and the button), in order
#   keyed      – removing one task keeps the other rows' widgets: a focused row stays focused
#   focus      – removing the focused "Remove" moves focus to the next one, at the end to the
#                previous one, and out of the emptied list to the previous control
#   Show       – the empty state comes and goes; empty labels stay out of the AT-SPI tree
#   no leaks   – filling and emptying the list again and again doesn't keep growing memory
# Screenshots and logs land in shots/todo-demo/.
set -euo pipefail
cd "$(dirname "$0")/.."

H=kiesel-headless
OUT=shots/todo-demo
mkdir -p "$OUT"
cargo build -q --release --example todo -p kiesel-ui
BIN=target/release/examples/todo

fail() { echo "FAIL: $*" >&2; exit 1; }
settle() { sleep 0.3; }
tree() { $H a11y; }
focused() { tree | sed -n "s/^ *\(push \)\?button '\(.*\)' focused$/\2/p; s/^ *entry '\(.*\)' focused$/\1/p"; }
expect_focus() {
  local got
  got=$(focused)
  [[ "$got" == "$1" ]] || fail "$tag: focus is on '$got', expected '$1' ($2)"
}
rows() { tree | sed -n "s/^ *label '\(.*\)'$/\1/p" | grep -v -e '^Tasks$' -e 'tasks\?$' -e '^Nothing to do' || true; }
expect_rows() {
  local got
  got=$(rows | paste -sd '|')
  [[ "$got" == "$1" ]] || fail "$tag: rows are '$got', expected '$1' ($2)"
}
# types a task the quick way: focus the field, set its text, Return (which goes to the focused widget)
add() { $H a11y focus --role entry "New task"; $H a11y set --role entry "New task" = "$1"; $H key Return; }
pss() { awk '/^Pss:/{print $2}' "/proc/$(pgrep -x todo)/smaps_rollup"; }

run_case() {
  local scale=$1
  tag="${scale}x"
  pkill -x todo || true
  $H stop >/dev/null 2>&1 || true
  $H start --size 520x520 --scale "$scale" >/dev/null
  KIESEL_STATS=1 $H run "$BIN" >"$OUT/$tag.log" 2>&1 &
  sleep 1.5

  tree | grep -q "label 'Nothing to do – add a task above'" || fail "$tag: no empty state at start"
  tree | grep -q "label ''" && fail "$tag: an empty label in the accessibility tree"

  # adding: Enter in the field, and the button
  $H key Tab; $H type "Buy milk"; $H key Return
  $H type "Write report"; $H key Return
  $H type "Call Anna"; $H a11y click Add; settle
  expect_rows "Buy milk|Write report|Call Anna" "after adding three"
  tree | grep -q "Nothing to do" && fail "$tag: the empty state stayed"
  tree | grep -q "label '3 tasks'" || fail "$tag: the count did not follow"
  $H shot "$OUT/$tag-three.png"

  # keyed: removing another row keeps the focused one (same widget, still focused)
  $H a11y focus "Remove “Call Anna”"; settle
  $H a11y click "Remove “Buy milk”"; settle
  expect_rows "Write report|Call Anna" "after removing the first"
  expect_focus "Remove “Call Anna”" "the focused row survived another row's removal"

  # focus moves on: the focused one goes → the next; at the end → the previous
  add "Plan trip"; add "Pay rent"; settle
  $H a11y focus "Remove “Call Anna”"; settle
  $H key space; settle
  expect_rows "Write report|Plan trip|Pay rent" "after removing the focused one"
  expect_focus "Remove “Plan trip”" "focus moved to the next row"
  $H a11y focus "Remove “Pay rent”"; $H key space; settle
  expect_focus "Remove “Plan trip”" "at the end, focus moved to the previous row"

  # pointer: a click removes; emptying the list brings the empty state back
  $H click $($H a11y where "Remove “Write report”"); settle
  expect_rows "Plan trip" "after a click"
  $H a11y focus "Remove “Plan trip”"; $H key space; settle
  expect_rows "" "all removed"
  expect_focus "Add" "out of the emptied list to the previous control"
  tree | grep -q "label 'Nothing to do – add a task above'" || fail "$tag: no empty state after emptying"
  $H shot "$OUT/$tag-empty.png"

  # no leaks: fill and empty the list five times; memory settles after the first rounds
  local round before after
  for round in 1 2 3 4 5; do
    for i in $(seq 25); do add "Task $round.$i"; done
    settle
    [[ "$(rows | wc -l)" == 25 ]] || fail "$tag: round $round has $(rows | wc -l) rows"
    $H a11y focus "Remove “Task $round.1”"
    for i in $(seq 25); do $H key space; done
    settle
    [[ -z "$(rows)" ]] || fail "$tag: round $round left rows behind"
    [[ $round == 2 ]] && before=$(pss)
  done
  after=$(pss)
  (( after - before < 1024 )) || fail "$tag: memory grew by $(( (after - before) / 1024 )) MiB over three rounds"

  pkill -x todo || true
  echo "ok  $tag  (PSS $((before / 1024)) → $((after / 1024)) MiB over three rounds of 25 tasks)"
}

trap '$H stop >/dev/null 2>&1 || true; pkill -x todo || true' EXIT
for scale in 1 2; do run_case "$scale"; done

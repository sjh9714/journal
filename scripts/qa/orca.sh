# Helpers for driving the native app with Orca computer use (macOS).
# Source it, then call the functions:  . scripts/qa/orca.sh; qa_launch; qa_click 'button Settings'
# Orca, open, osascript and pbcopy are blocked inside the Claude Code sandbox.

QA_DIR="${QA_DIR:-${TMPDIR:-/tmp}/journal-qa}"
QA_BIN="${QA_BIN:-$(git rev-parse --show-toplevel)/target/release/bundle/macos/Journal.app/Contents/MacOS/journal-desktop}"

# Start the app on a throwaway journal, config and index; the real ones stay untouched.
# Optional seed: QA_SEED=1 creates a sample entry with the CLI first.
qa_launch() {
  pkill -f "$QA_BIN" 2>/dev/null; sleep 1
  mkdir -p "$QA_DIR"
  local cli; cli="$(git rev-parse --show-toplevel)/target/debug/journal"
  if [[ ! -d "$QA_DIR/Journal" ]]; then
    JOURNAL_INDEX_DIR="$QA_DIR/idx" "$cli" --root "$QA_DIR/Journal" init >/dev/null
    [[ -n "${QA_SEED:-}" ]] && JOURNAL_INDEX_DIR="$QA_DIR/idx" "$cli" --root "$QA_DIR/Journal" add "QA seed entry" >/dev/null
  fi
  [[ -f "$QA_DIR/config.json" ]] || echo '{}' > "$QA_DIR/config.json"
  (JOURNAL_ROOT="$QA_DIR/Journal" JOURNAL_CONFIG="$QA_DIR/config.json" JOURNAL_INDEX_DIR="$QA_DIR/idx" \
    nohup "$QA_BIN" > "$QA_DIR/app.log" 2>&1 &)
  sleep 4
  # By PID, never --restore-window: either bundle-ID path makes macOS launch an
  # installed copy of the app, which opens the real journal.
  QA_APP="pid:$(pgrep -nf "$QA_BIN")"
  echo "app: $QA_APP  log: $QA_DIR/app.log"
}

qa_reset() { pkill -f "$QA_BIN" 2>/dev/null; rm -rf "$QA_DIR"; }

# Accessibility tree as text: one line per element, "<index> <role> <label>".
qa_tree() {
  orca computer get-app-state --app "$QA_APP" --no-screenshot --json \
    | python3 -c "import json,sys; print(json.load(sys.stdin)['result']['snapshot']['treeText'])"
}

# Lines of the tree matching a regex; the cheap way to assert on UI state.
qa_find() { qa_tree | grep -E "$1"; }

# Element index of the first line whose text after the index matches the regex exactly.
qa_idx() {
  qa_tree | python3 -c "
import re,sys; m=re.search(r'^\s*(\d+) ' + sys.argv[1] + r'\s*$', sys.stdin.read(), re.M); print(m.group(1) if m else -1)" "$1"
}

qa_click() {
  local i; i=$(qa_idx "$1")
  [[ "$i" == -1 ]] && { echo "not found: $1" >&2; return 1; }
  orca computer click --app "$QA_APP" --element-index "$i" --no-screenshot --json >/dev/null
  sleep 0.5
}

# Text into the focused field. Use this for the editor and password inputs: Orca's
# type-text doubles characters there. Goes through the clipboard, so Korean works.
qa_paste() {
  printf '%s' "$1" | pbcopy
  osascript -e 'tell application "System Events" to keystroke "v" using command down'
  sleep 0.3
}

# A real key event, e.g. qa_key return, qa_key "n" command.
qa_key() {
  case "$1" in
    return) osascript -e 'tell application "System Events" to key code 36' ;;
    escape) osascript -e 'tell application "System Events" to key code 53' ;;
    *) osascript -e "tell application \"System Events\" to keystroke \"$1\"${2:+ using $2 down}" ;;
  esac
  sleep 0.3
}

# Screenshot of the app window, downscaled so reading it costs a fraction of the original.
qa_shot() {
  local out="${1:-$QA_DIR/shot.png}" src
  src=$(orca computer get-app-state --app "$QA_APP" --json \
    | python3 -c "import json,sys; print(json.load(sys.stdin)['result']['screenshot']['path'])")
  sips -Z "${QA_SHOT_MAX:-1200}" "$src" --out "$out" >/dev/null
  echo "$out"
}

# Drag a file from Finder onto the QA window. Orca's drag stays inside one app, so this
# posts real mouse events, and aborts unless Finder is topmost at the source point and
# the QA app at the drop point (another window there would receive the drop).
# Place the windows apart first; coordinates are screen points.
qa_drop() {  # qa_drop srcX srcY dstX dstY
  local bin="$QA_DIR/guarded_drag"
  [[ -x "$bin" ]] || swiftc -O "$(git rev-parse --show-toplevel)/scripts/qa/guarded_drag.swift" -o "$bin" || return 1
  osascript -e 'tell application "Finder" to activate'; sleep 0.6
  osascript -e "tell application \"System Events\" to set frontmost of (first process whose unix id is ${QA_APP#pid:}) to true"; sleep 0.5
  "$bin" "$1" "$2" "$3" "$4" "${QA_APP#pid:}"
}

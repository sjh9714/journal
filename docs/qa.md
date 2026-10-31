# Native QA

The mock UI (`npx vite`) covers layout and most flows. Use the native app for
anything that touches Tauri: file IO, the `dayfile://` scheme, dialogs, the
lock, notifications, and real keyboard input.

## Build and launch

```sh
cargo build -p dayfile-cli                 # the CLI seeds test journals
cd apps/desktop && npm run tauri build     # bundle: target/release/bundle/macos/Dayfile.app
. scripts/qa/orca.sh && qa_launch          # throwaway journal, config and index
```

`qa_launch` sets three variables so a QA run never touches the real journal or
its lock settings:

| variable | overrides |
|---|---|
| `DAYFILE_ROOT` | journal folder from config.json |
| `DAYFILE_CONFIG` | config.json path (lock hash, reminder, root) |
| `DAYFILE_INDEX_DIR` | where the SQLite index lives |

Webview logs (`log_js`) and Rust stderr land in `$QA_DIR/app.log`.

## Driving the UI

Address the QA app by PID and leave out `--restore-window`, as the helpers do.
Both `--app dev.dayfile.app` and `--restore-window` go through the bundle
ID, so macOS launches an installed copy, which opens the real journal.

Assert on the accessibility tree (`qa_find`, `qa_idx`, `qa_click`); it is text
and costs little. Take a screenshot (`qa_shot`, downscaled) only to judge
something visual.

Input, by target:

| target | works | fails |
|---|---|---|
| editor (ProseMirror), password fields | `qa_paste` (clipboard + ⌘V), `qa_key` | `orca computer type-text` inserts every character twice |
| plain text inputs (search, tags) | `orca computer type-text` | |
| React-controlled inputs | typing or pasting | `orca computer set-value` (onChange never fires) |
| Korean and other IME text | `qa_paste` | |
| `<input type=time>` | none in WKWebView; the app uses hour/minute selects | |

`qa_click` does not focus the editor. Click it by coordinates with
`orca computer click --x --y`, then `qa_paste` or `orca computer paste-text`,
and wait at least 1.5 s before asserting. With the system input source on
Korean, a simulated Backspace is unreliable.

## Feature notes

- **Lock**: a fresh `DAYFILE_CONFIG` starts unlocked. Set the idle timeout to
  1 minute in Settings to check the idle lock without waiting long.
- **Reminder**: notifications fire only from the bundled app, not `tauri dev`.
  Write `{"reminder": {"enabled": true, "time": "HH:MM"}}` to the QA config
  two minutes ahead, on a journal with no entry today. The app log prints
  `reminder HH:MM: permission=…` when it fires. macOS Focus (Do Not Disturb,
  Sleep) hides the banner even when the call succeeds.
- **Location**: WKWebView geolocation fails silently; the app falls back to
  ipapi.co, so the first entry needs network.
- **Capture and memory**: `scripts/qa/fixtures.sh "$QA_DIR"` before `qa_launch`
  seeds a Korean day dated today: commits, calendar, an EXIF photo and facts,
  with sources in the QA config, never the real one. The app captures once per
  launch after unlock.
- **Image drag and drop**: `qa_drop` from a Finder window that does not overlap
  the QA window. Drop twice in a row to check the first image survives.
- **Not yet verified natively**: Windows and Linux bundles.

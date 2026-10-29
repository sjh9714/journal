# Native QA

The mock UI (`npx vite`) covers layout and most flows. Use the native app for
anything that touches Tauri: file IO, the `journal://` scheme, dialogs, the
lock, notifications, and real keyboard input.

## Build and launch

```sh
cargo build -p journal-cli                 # the CLI seeds test journals
cd apps/desktop && npm run tauri build     # bundle: target/release/bundle/macos/Journal.app
. scripts/qa/orca.sh && qa_launch          # throwaway journal, config and index
```

`qa_launch` sets three variables so a QA run never touches the real journal or
its lock settings:

| variable | overrides |
|---|---|
| `JOURNAL_ROOT` | journal folder from config.json |
| `JOURNAL_CONFIG` | config.json path (lock hash, reminder, root) |
| `JOURNAL_INDEX_DIR` | where the SQLite index lives |

Webview logs (`log_js`) and Rust stderr land in `$QA_DIR/app.log`.

## Driving the UI

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

`orca computer paste-text` exists but has not been tried against the editor.

## Feature notes

- **Lock**: a fresh `JOURNAL_CONFIG` starts unlocked. Set the idle timeout to
  1 minute in Settings to check the idle lock without waiting long.
- **Reminder**: notifications fire only from the bundled app, not `tauri dev`.
  Write `{"reminder": {"enabled": true, "time": "HH:MM"}}` to the QA config
  two minutes ahead, on a journal with no entry today. The app log prints
  `reminder HH:MM: permission=…` when it fires. macOS Focus (Do Not Disturb,
  Sleep) hides the banner even when the call succeeds.
- **Location**: WKWebView geolocation fails silently; the app falls back to
  ipapi.co, so the first entry needs network.
- **Not yet verified natively**: image drag and drop, Windows and Linux bundles.

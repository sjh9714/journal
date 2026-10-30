# Design decisions

Five choices shape everything else. Each one was a trade, not a default.

## 1. Entries are files; the index is disposable

Every entry is a Markdown file with YAML frontmatter. The SQLite/FTS5 index
lives outside the journal folder and can be rebuilt from the files at any
time (`dayfile reindex --full`).

Why: ownership and sync. A folder of text files survives the app, syncs with
any tool, and a conflict damages one entry, not a database. The cost is that
every read goes through parsing, so listing and search go through the index
and only `get` touches the file.

Rejected: SQLite as the source of truth (fast, but one corrupted file loses
everything and sync tools cannot merge it) and a custom binary format.

## 2. One command registry, three surfaces

All behaviour lives in `crates/dayfile-core::Store`. The CLI, the MCP server
and the desktop app call the same functions. A feature is not done until it
exists on all three.

Why: agents are a first-class user. If the app can do something the MCP
server cannot, an agent is a second-class citizen; if the MCP server can do
something the app cannot, the user cannot see what the agent did. One
registry keeps them honest. It also made the desktop app thin: 16 Tauri
commands, no logic.

## 3. Compatible import, not a clone

The app reads Day One's JSON export completely and writes its own format. It
does not try to be Day One: no attempt at its file format, sync or UI
details. Import is idempotent (same export twice creates nothing) so a user
can re-export as often as they like while migrating.

Why: the value people want to keep is their years of entries, not Day One's
feature list. Matching the export is a bounded problem; matching the product
is not.

## 4. Location: system service first, network guess second

New entries try `navigator.geolocation`, then fall back to a city-level
guess from the network connection (ipapi.co), then fetch weather from
Open-Meteo. Both are keyless. One toggle turns the whole thing off.

Why: inside the macOS webview the system location service fails silently,
and a journal without weather and place feels dead. The fallback is
disclosed in Settings and the README. Precise location would need native
CoreLocation code, which is on the list.

## 5. Tauri 2 over Electron, Tiptap over a plain textarea

Tauri gives a 4 MB download and reuses the Rust core directly. Tiptap with
`@tiptap/markdown` gives a WYSIWYG editor whose output is still Markdown on
disk; every save is round-tripped through the serializer and normalised
(trailing newline, no `&nbsp;` paragraphs).

Cost paid: WKWebView quirks. `window.confirm` returned true on Cancel, the
system location service is unavailable, and Rust's serde omits empty fields
so the frontend normalises them. Each is documented next to its fix.

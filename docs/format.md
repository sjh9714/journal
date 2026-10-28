# On-disk format

A journal is a folder. Nothing in it needs this app to be read.

```
Journal/
  .journal/journal.toml        marker; safe to sync
  entries/
    2026/
      2026-10-28-6d65312f.md   one entry per file
  attachments/
    <sha256>.png               content-addressed; referenced from entries
```

The search index (SQLite, FTS5) lives **outside** the folder, under the OS
data directory (`~/Library/Application Support/journal/<hash>/index.db` on
macOS, `~/.local/share/journal/...` on Linux, `%LOCALAPPDATA%\journal\...` on
Windows). It is a cache: delete it and it is rebuilt from the files.
`JOURNAL_INDEX_DIR` overrides the base directory.

## Entry file

```markdown
---
uuid: 6d65312f-0ff3-4ad3-9fad-ba4d628be823
created: 2026-10-28T21:03:00+09:00
timezone: Asia/Seoul
journal: default
tags:
- life
- work
starred: true
location:
  latitude: 37.5
  longitude: 126.9
  place_name: Han River Park
weather:
  temperature_c: 18.5
  conditions: Clear
---

# Title is just the first line

Body is CommonMark. Images point at the attachments folder relative to the
entry file:

![](../../attachments/b1ff9c8e….png)
```

| field | required | notes |
|---|---|---|
| `uuid` | yes | canonical lower-case UUID. File name carries its first 8 hex chars. |
| `created` | yes | RFC 3339 **with the entry's own offset**. The calendar date of the entry is taken from this, never from the machine's zone. |
| `timezone` | yes | IANA name. Used to render the offset and for "today" arithmetic. |
| `journal` | no | notebook name, default `default`. |
| `tags` | no | sorted, de-duplicated, no leading `#`. |
| `starred` | no | omitted when false. |
| `location`, `weather` | no | omitted when absent. |
| `extra` | no | free-form map an importer may fill (Day One keeps its original fields under `extra.dayone`). |

Rules the app follows so other tools can too:

- Body always ends with exactly one newline.
- Files are written to `<name>.md.tmp` then renamed, so a sync tool never sees a half file.
- A file that fails to parse is skipped by the indexer and reported; it never breaks the journal.
- Entry path is `entries/<YYYY>/<YYYY-MM-DD>-<uuid8>.md` where the date is `created` in its own offset. Moving or renaming files is fine; the index keys on `uuid`.

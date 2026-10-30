# On-disk format

A journal is a folder. Nothing in it needs this app to be read.

```
Journal/
  .dayfile/dayfile.toml        marker; safe to sync
  .dayfile/trash/              deleted entries, `<unix-secs>__<file>.md`, purged after 30 days
  entries/
    2026/
      2026-10-28-6d65312f.md   one entry per file
  attachments/
    <sha256>.png               content-addressed; referenced from entries
  templates/
    gratitude.md               frontmatter `name`, optional `tags`/`journal`; body may use
                               {{date}} {{weekday}} {{time}} {{year}}
  memory/
    home.md                    facts about one topic, see "Memory" below
```

The search index (SQLite, FTS5) lives **outside** the folder, under the OS
data directory (`~/Library/Application Support/dayfile/<hash>/index.db` on
macOS, `~/.local/share/dayfile/...` on Linux, `%LOCALAPPDATA%\dayfile\...` on
Windows). It is a cache: delete it and it is rebuilt from the files.
`DAYFILE_INDEX_DIR` overrides the base directory.

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
| `status` | no | `draft` while a captured day waits for review. Drafts are left out of listings, search, On This Day and stats. Removing the line keeps the entry. |
| `traces` | no | keys of the captured items the entry holds (`git:<sha>`, `cal:<uid>:<start>`, `photo:<sha256>`). Capture skips any key some entry already has, so a day is never captured twice. |

Rules the app follows so other tools can too:

- Body always ends with exactly one newline.
- Files are written to `<name>.md.tmp` then renamed, so a sync tool never sees a half file.
- A file that fails to parse is skipped by the indexer and reported; it never breaks the journal.
- Entry path is `entries/<YYYY>/<YYYY-MM-DD>-<uuid8>.md` where the date is `created` in its own offset. Moving or renaming files is fine; the index keys on `uuid`.

## Memory

Facts agents (or `dayfile remember`) record about the user's life, one file per
topic. The file is YAML frontmatter; anything after the closing `---` is free
notes and is ignored.

```markdown
---
topic: Home
facts:
- id: f3a9c2e1
  text: Lives in Busan
  valid_from: 2019-01-01
  valid_to: 2024-03-01
  recorded_at: 2026-01-10T09:00:00+09:00
  closed_at: 2026-05-02T09:00:00+09:00
  source: 6d65312f-0ff3-4ad3-9fad-ba4d628be823
- id: f11aa22b
  text: Lives in Seoul
  valid_from: 2024-03-01
  recorded_at: 2026-05-02T09:00:00+09:00
  replaces: [f3a9c2e1]
---
```

Facts carry two timelines and are never deleted.

| field | timeline | meaning |
|---|---|---|
| `valid_from`, `valid_to` | world | when it was true; `valid_to` is exclusive, absent means still true |
| `recorded_at` | journal | when it was written down |
| `closed_at` | journal | when a newer fact ended it (set together with `valid_to`) |
| `retracted_at` | journal | when it was found never to be true |
| `source` | | uuid of the entry it came from |
| `replaces` | | ids of the facts it ended |

"What held on date D, as the journal knew it at moment T" keeps facts with
`recorded_at <= T`, not retracted by T, `valid_from <= D`, and either no
`valid_to`, `valid_to > D`, or a `closed_at` later than T. The file name is the
topic in lower case with non-alphanumerics turned into `-` (`memory/회사.md`
for `회사`).

# MCP server

`dayfile mcp serve` speaks MCP over stdio. Add `--read-only` to expose only
read tools, `--root <dir>` to point at a journal other than `~/Journal`.

```sh
claude mcp add dayfile -- dayfile mcp serve
claude mcp add journal -- journal --root ~/Dropbox/Journal mcp serve --read-only
```

Dates are `YYYY-MM-DD` in the user's local zone. IDs accept a full uuid or a
unique prefix.

| tool | args | returns |
|---|---|---|
| `list_entries` | `journal?`, `tag?`, `from?`, `to?`, `starred?`, `limit?` (≤200), `offset?` | summaries, newest first |
| `get_entry` | `id` | full entry incl. body, location, weather |
| `search_entries` | `query`, `limit?` | summaries ranked by FTS5 bm25. Words are ANDed; `river*` prefix-matches |
| `on_this_day` | `date?` | entries from the same month/day in other years |
| `entries_on_date` | `date?` | entries on that day, oldest first |
| `list_tags` | – | `[{tag, count}]` |
| `list_journals` | – | `[{journal, count}]` |
| `create_entry` | `body`, `journal?`, `tags?`, `starred?`, `created?` (RFC 3339), `location?`, `weather?` | full entry |
| `append_to_today` | `text`, `journal?` | appends a paragraph to today's entry in that journal, creating it if needed |
| `update_entry` | `id`, `body?`, `tags?`, `starred?`, `journal?` | full entry |
| `delete_entry` | `id` | moves the entry to the trash (30-day retention); returns its summary |

Write tools return an `invalid_request` error in read-only mode.

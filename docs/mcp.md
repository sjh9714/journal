# MCP server

`dayfile mcp serve` speaks MCP over stdio. Add `--read-only` to expose only
read tools, `--root <dir>` to point at a journal other than the app's.

```sh
claude mcp add dayfile -- dayfile mcp serve
claude mcp add dayfile -- dayfile --root ~/Dropbox/Journal mcp serve --read-only
```

Dates are `YYYY-MM-DD` in the user's local zone. IDs accept a full uuid or a
unique prefix. Capture sources come from the app's config (`dayfile sources`).

## Entries

| tool | args | returns |
|---|---|---|
| `list_entries` | `journal?`, `tag?`, `from?`, `to?`, `starred?`, `limit?` (≤200), `offset?` | summaries, newest first; drafts left out |
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

## Days

| tool | args | returns |
|---|---|---|
| `capture_day` | `date?` | `{added, commits, events, photos, draft, created, errors}`; adds only traces no entry holds yet |
| `list_drafts` | – | drafts waiting for review |
| `accept_draft` | `id` | the entry, now ordinary |

## Memory

| tool | args | returns |
|---|---|---|
| `remember` | `topic`, `text`, `valid_from?`, `source?`, `replaces?` | the fact; replaced facts end at `valid_from` |
| `recall` | `topic?`, `query?`, `as_of?`, `known_at?` (RFC 3339) | facts true on `as_of` (default today) as known at `known_at` (default now) |
| `fact_history` | `topic` | every fact of the topic, oldest first, incl. ended and retracted |
| `list_topics` | – | `[{topic, current}]` |
| `retract_fact` | `id` | the fact, marked never true |

## Prompts

| prompt | args | what it asks the agent to do |
|---|---|---|
| `review_day` | `date?` | capture the day, ask the user a few questions, rewrite the draft as their entry, then record durable facts with `remember` |
| `weekly_review` | `date?` (last day) | summarise seven days and compare `recall` now with a week ago |

Write tools return an `invalid_request` error in read-only mode. Draft text
comes partly from other people (calendar invites, commit messages); the
server's instructions and the prompts tell agents to treat it as data.
